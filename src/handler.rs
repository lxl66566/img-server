use std::{io::BufWriter, net::SocketAddr, path::PathBuf, sync::Arc};

use axum::{
    Json,
    body::Body,
    extract::{ConnectInfo, Multipart, Path, Query, State},
    http::{StatusCode, header},
    response::Response,
};
use futures::TryStreamExt;
use image::{GenericImageView as _, ImageReader};
use log::{error, info, warn};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::{
    fs::{self, File},
    io::AsyncWriteExt,
};
use tokio_util::io::ReaderStream;

use crate::config::{AppConfig, AppState, ImageMeta, save_config};

// 检查 IP 黑名单
fn check_ip(config: &AppConfig, addr: &SocketAddr) -> Result<(), (StatusCode, String)> {
    let ip = addr.ip().to_string();
    if config.blacklist.contains(&ip) {
        warn!("Blocked request from blacklisted IP: {}", ip);
        return Err((StatusCode::FORBIDDEN, "IP Blacklisted".to_string()));
    }
    Ok(())
}

// 检查 Admin Token
fn check_token(config: &AppConfig, token: Option<&str>) -> Result<(), (StatusCode, String)> {
    match token {
        Some(t) if config.tokens.contains(t) => Ok(()),
        _ => Err((
            StatusCode::UNAUTHORIZED,
            "Invalid or missing token".to_string(),
        )),
    }
}

// 一个简单的 RAII 守卫，用于自动删除临时文件
// 如果在 drop 时 persist 仍为 false，则删除 path 指向的文件
struct TempFileGuard {
    path: Option<PathBuf>,
}

impl TempFileGuard {
    fn new(path: PathBuf) -> Self {
        Self { path: Some(path) }
    }

    // 调用此方法表示文件已被移动或不再需要自动删除
    fn persist(&mut self) {
        self.path = None;
    }
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        if let Some(path) = &self.path {
            // 使用 std::fs 删除，因为 drop 不能调用 async
            // 在这一步通常文件要么是临时垃圾，要么很小，同步删除影响不大
            let _ = std::fs::remove_file(path);
        }
    }
}

pub async fn upload_image(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: header::HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<ImageMeta>, (StatusCode, String)> {
    let token = headers.get("x-admin-token").and_then(|v| v.to_str().ok());

    // 1. 初始读取配置：检查权限和获取配置参数
    let (temp_dir, images_dir, thumbs_dir, thumbnail_pixels) = {
        let config = state.config.read().await;
        check_ip(&config, &addr)?;
        check_token(&config, token)?;
        (
            config.temp_dir().clone(),
            config.images_dir().clone(),
            config.thumbs_dir().clone(),
            config.thumbnail_pixels,
        )
    };

    let mut name = None;
    let mut desc = String::new();
    let mut file_hash = String::new();

    // 生成临时文件路径 (使用 uuid 避免冲突)
    let temp_file_path = temp_dir.join(uuid::Uuid::new_v4().to_string());
    // **创建守卫**：如果本函数中途报错退出，这个守卫会自动删除临时文件
    let mut temp_guard = TempFileGuard::new(temp_file_path.clone());

    // 2. 处理 Multipart
    let mut file_received = false;

    while let Ok(Some(field)) = multipart.next_field().await {
        let field_name = field.name().unwrap_or("").to_string();

        if field_name == "name" {
            name = Some(
                field
                    .text()
                    .await
                    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?,
            );
        } else if field_name == "desc" {
            desc = field
                .text()
                .await
                .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
        } else if field_name == "file" {
            // 打开临时文件准备写入
            let mut file = File::create(&temp_file_path).await.map_err(|e| {
                error!("Failed to create temp file: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, "IO Error".to_string())
            })?;

            let mut hasher = Sha256::new();
            let mut stream = field;

            while let Ok(Some(chunk)) = stream.try_next().await {
                hasher.update(&chunk);
                file.write_all(&chunk)
                    .await
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            }

            // 刷入磁盘
            file.flush()
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            file_hash = hex::encode(hasher.finalize());
            file_received = true;
        }
    }

    let name = name.ok_or((StatusCode::BAD_REQUEST, "Missing 'name'".to_string()))?;
    if !file_received {
        return Err((StatusCode::BAD_REQUEST, "Missing 'file'".to_string()));
    }

    // 3. 文件移动处理 (I/O 阶段，不持有锁)
    // 逻辑：基于 Hash 去重。如果目标文件已存在，则直接复用，删除临时文件。
    let target_path = images_dir.join(&file_hash);
    let thumb_path = thumbs_dir.join(&file_hash);

    if target_path.exists() {
        // 文件已存在，不需要移动，不需要生成缩略图
        // 这里的 temp_guard 在函数结束或 drop 时会自动删除临时文件，符合预期
    } else {
        // 文件不存在，移动临时文件到目标位置
        fs::rename(&temp_file_path, &target_path)
            .await
            .map_err(|e| {
                error!("Failed to move file: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "File move failed".to_string(),
                )
            })?;

        // 生成缩略图 (Blocking)
        let t_p = target_path.clone();
        if let Some(thumbnail_pixels) = thumbnail_pixels {
            let th_p = thumb_path.clone();
            tokio::task::spawn_blocking(move || {
                let res = (|| -> image::ImageResult<()> {
                    // 1. 打开文件并猜测格式
                    let reader = ImageReader::open(&t_p)?.with_guessed_format()?;

                    // 2. 在解码前获取格式，用于后续保存
                    let format = reader.format().unwrap_or(image::ImageFormat::Png);

                    // 3. 解码图片
                    let img = reader.decode()?;

                    // 4. 计算缩放后的尺寸
                    let (width, height) = img.dimensions();
                    let current_pixels = (width * height) as f64;

                    // 计算缩放比例：sqrt(目标像素 / 当前像素)
                    let scale_factor = (thumbnail_pixels as f64 / current_pixels).sqrt();

                    // 如果当前像素已经小于目标值，可以选择不缩放，或者仍然强制缩放
                    // 这里假设：如果图片太大，就缩小；如果本来就小，保持原样 (scale_factor > 1.0)
                    let (new_w, new_h) = if scale_factor < 1.0 {
                        (
                            (width as f64 * scale_factor) as u32,
                            (height as f64 * scale_factor) as u32,
                        )
                    } else {
                        (width, height)
                    };

                    // 5. 生成缩略图 (thumbnail 会保持宽高比)
                    let thumb = img.thumbnail(new_w, new_h);

                    // 6. 使用与输入相同的格式保存
                    let mut output_file = BufWriter::new(std::fs::File::create(&th_p)?);
                    thumb.write_to(&mut output_file, format)?;

                    Ok(())
                })();

                if let Err(e) = res {
                    error!("Image processing failed: {}", e);
                }
            })
            .await
            .map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Thumb gen failed".to_string(),
                )
            })?;
        }
        temp_guard.persist();
    }

    let meta = ImageMeta {
        name: name.clone(),
        desc,
        hash: file_hash.clone(),
        created_at: chrono::Utc::now(),
    };

    let mut config = state.config.write().await;
    config.images.push(meta.clone());

    if let Err(e) = save_config(&state.config_path, &config) {
        error!("Failed to save config: {}", e);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "Save config failed".to_string(),
        ));
    }

    info!(
        "addr: {:?}, action: upload, name: {:?}, hash: {:?}",
        addr, meta.name, meta.hash
    );
    Ok(Json(meta))
}

// 下载图片
#[derive(Deserialize)]
pub struct DownloadParams {
    thumb: Option<bool>,
}

pub async fn download_image(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
    Query(params): Query<DownloadParams>,
) -> Result<Response, (StatusCode, String)> {
    let config = state.config.read().await;
    check_ip(&config, &addr)?;

    // 查找逻辑：先匹配 Name，如果没找到且 id 看起来像 hash，则匹配 Hash
    let hash = if let Some(img) = config.images.iter().find(|i| i.name == id) {
        img.hash.clone()
    } else if id.len() == 64 && id.chars().all(|c| c.is_ascii_hexdigit()) {
        id.clone()
    } else {
        return Err((StatusCode::NOT_FOUND, "Image not found".to_string()));
    };

    let is_thumb = params.thumb.unwrap_or(false);
    let dir = if is_thumb {
        &config.thumbs_dir()
    } else {
        &config.images_dir()
    };
    let path = dir.join(&hash);

    if !path.exists() {
        // 如果请求缩略图但不存在，回退到原图（可选策略，这里直接返回404）
        return Err((StatusCode::NOT_FOUND, "File not found".to_string()));
    }

    // 核心要求：Async Read -> Async Write
    let file = File::open(&path)
        .await
        .map_err(|_| (StatusCode::NOT_FOUND, "File open error".to_string()))?;
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    info!(
        "addr: {:?}, action: download, id: {:?}, thumb: {:?}",
        addr, id, is_thumb
    );

    Ok(Response::builder()
        .header(header::CONTENT_TYPE, "application/octet-stream") // 前端处理 Content-Type
        .header(
            header::CONTENT_DISPOSITION,
            format!("inline; filename=\"{}\"", hash),
        )
        .body(body)
        .unwrap())
}

// 列出图片
#[derive(Deserialize)]
pub struct ListParams {
    page: Option<usize>,
    page_size: Option<usize>,
}

pub async fn list_images(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Query(params): Query<ListParams>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let config = state.config.read().await;
    check_ip(&config, &addr)?;

    let page = params.page.unwrap_or(1).max(1);
    let page_size = params.page_size.unwrap_or(20).clamp(1, 100);

    let total = config.images.len();
    let skip = (page - 1) * page_size;

    let data: Vec<_> = config
        .images
        .iter()
        .rev()
        .skip(skip)
        .take(page_size)
        .collect();

    info!("addr: {:?}, action: list, page: {:?}", addr, page);

    Ok(Json(serde_json::json!({
        "total": total,
        "page": page,
        "page_size": page_size,
        "data": data
    })))
}

pub async fn delete_image(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: header::HeaderMap,
    Path(name): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let token = headers.get("x-admin-token").and_then(|v| v.to_str().ok());
    {
        let config = state.config.read().await;
        check_ip(&config, &addr)?;
        check_token(&config, token)?;
    }
    let mut config = state.config.write().await;

    let img = if let Some(index) = config.images.iter().position(|i| i.name == name) {
        config.images.remove(index)
    } else {
        return Err((StatusCode::NOT_FOUND, "Image not found".to_string()));
    };

    // 检查是否还有其他图片使用相同的 Hash (去重)
    let hash_in_use = config.images.iter().any(|i| i.hash == img.hash);

    if !hash_in_use {
        // 忽略文件不存在的错误
        let _ = fs::remove_file(config.images_dir().join(&img.hash)).await;
        let _ = fs::remove_file(config.thumbs_dir().join(&img.hash)).await;
    }

    // 保存到磁盘
    save_config(&state.config_path, &config).map_err(|e| {
        error!("Failed to save config: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, "Save failed".to_string())
    })?;

    info!("addr: {:?}, action: delete, name: {:?}", addr, name);
    Ok(StatusCode::NO_CONTENT)
}
