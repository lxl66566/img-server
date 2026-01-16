use std::{collections::HashSet, fs, path::PathBuf, sync::LazyLock as Lazy, sync::OnceLock};

use config_file2::{LoadConfigFile, StoreConfigFile};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

pub static CONFIG_DIR: Lazy<PathBuf> = Lazy::new(|| {
    let dir = home::home_dir()
        .expect("cannot find home dir on your OS!")
        .join(".config")
        .join(env!("CARGO_PKG_NAME"));
    _ = fs::create_dir_all(&dir);
    dir
});

// --- 1. 配置与数据结构 ---

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ImageMeta {
    pub name: String,
    pub desc: String,
    pub hash: String,
    #[serde(default = "chrono::Utc::now")]
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct AppConfig {
    pub data_dir: PathBuf,
    pub max_size_mb: usize,
    pub tokens: HashSet<String>,
    pub blacklist: HashSet<String>,
    pub images: Vec<ImageMeta>,
    pub thumbnail_pixels: Option<u32>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("data"),
            max_size_mb: 20,
            tokens: HashSet::new(),
            blacklist: HashSet::new(),
            images: Vec::new(),
            thumbnail_pixels: Some(50000),
        }
    }
}

impl AppConfig {
    pub fn images_dir(&self) -> &PathBuf {
        static IMAGES_DIR: OnceLock<PathBuf> = OnceLock::new();
        IMAGES_DIR.get_or_init(|| self.data_dir.join("images"))
    }

    pub fn thumbs_dir(&self) -> &PathBuf {
        static THUMBS_DIR: OnceLock<PathBuf> = OnceLock::new();
        THUMBS_DIR.get_or_init(|| self.data_dir.join("thumbs"))
    }

    pub fn temp_dir(&self) -> &PathBuf {
        static TEMP_DIR: OnceLock<PathBuf> = OnceLock::new();
        TEMP_DIR.get_or_init(|| self.data_dir.join("temp"))
    }

    pub fn logs_dir(&self) -> &PathBuf {
        static LOG_DIR: OnceLock<PathBuf> = OnceLock::new();
        LOG_DIR.get_or_init(|| self.data_dir.join("logs"))
    }
}

pub struct AppState {
    pub config: RwLock<AppConfig>,
    pub config_path: PathBuf,
}

// 加载配置
pub fn load_config(path: &PathBuf) -> anyhow::Result<AppConfig> {
    let config = AppConfig::load_or_default(path)?;
    // 确保存储目录存在
    fs::create_dir_all(config.images_dir())?;
    fs::create_dir_all(config.thumbs_dir())?;
    fs::create_dir_all(config.temp_dir())?;
    fs::create_dir_all(config.logs_dir())?;
    Ok(config)
}

// 保存配置 (持久化)
pub fn save_config(path: &PathBuf, config: &AppConfig) -> anyhow::Result<()> {
    Ok(config.store(path)?)
}
