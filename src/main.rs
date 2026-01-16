pub mod config;
pub mod handler;
pub mod logging;

use std::{net::SocketAddr, path::PathBuf, sync::Arc};
use tokio::sync::RwLock;

use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, post},
};
use clap::{CommandFactory, Parser, Subcommand};
use log::info;
use tokio::fs::{self};

use crate::{
    config::{AppState, CONFIG_DIR, load_config, save_config},
    handler::{delete_image, download_image, list_images, upload_image},
};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Config file path
    #[arg(short, long)]
    config: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate a new admin token
    GenToken,
    /// Run the server
    Serve {
        #[arg(short, long, default_value = "0.0.0.0:3918")]
        addr: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // 确定配置文件路径
    let config_path = cli.config.unwrap_or_else(|| CONFIG_DIR.join("config.toml"));

    // 确保配置目录存在
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).await?;
    }

    match cli.command {
        Some(Commands::GenToken) => {
            let token: String = (0..32)
                .map(|_| {
                    let idx: usize = rand::random_range(0..62);
                    const CHARS: &[u8] =
                        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
                    CHARS[idx] as char
                })
                .collect();

            // 加载现有配置并添加 Token
            let mut config = load_config(&config_path)?;
            config.tokens.insert(token.clone());
            save_config(&config_path, &config)?;

            println!("Generated Admin Token: {}", token);
            println!("Token added to config at: {:?}", config_path);
        }
        Some(Commands::Serve { addr }) => {
            let config = load_config(&config_path)?;
            let _logger = logging::init_logger(config.logs_dir().to_path_buf()).unwrap();
            let max_size = config.max_size_mb * 1024 * 1024;

            info!("Server starting with config: {:?}", config_path);
            info!("Images dir: {:?}", config.images_dir());

            let state = Arc::new(AppState {
                config: RwLock::new(config),
                config_path,
            });

            use tower_http::cors::{Any, CorsLayer};
            let cors = CorsLayer::new()
                .allow_origin(Any) // 允许任何来源 (生产环境建议指定具体域名)
                .allow_methods(Any) // 允许 GET, POST, DELETE 等
                .allow_headers(Any); // 允许 x-admin-token 等 Header

            let app = Router::new()
                .route("/images", post(upload_image).get(list_images))
                .route("/images/{id}", get(download_image).delete(delete_image))
                .layer(DefaultBodyLimit::max(max_size)) // 限制上传大小
                .layer(cors)
                .with_state(state);

            let listener = tokio::net::TcpListener::bind(&addr).await?;
            info!("Listening on {}", addr);
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await?;
        }
        None => {
            Cli::command().print_help()?;
        }
    }

    Ok(())
}
