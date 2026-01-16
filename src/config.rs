use std::{collections::HashSet, fs, path::PathBuf, sync::LazyLock as Lazy};

use arc_swap::ArcSwap;
use config_file2::{LoadConfigFile, StoreConfigFile};
use serde::{Deserialize, Serialize};

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
    pub images_dir: PathBuf,
    pub thumbs_dir: PathBuf,
    pub temp_dir: PathBuf,
    pub max_size_mb: usize,
    pub tokens: HashSet<String>,
    pub blacklist: HashSet<String>,
    pub images: Vec<ImageMeta>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            images_dir: PathBuf::from("data/images"),
            thumbs_dir: PathBuf::from("data/thumbs"),
            temp_dir: PathBuf::from("data/temp"),
            max_size_mb: 20,
            tokens: HashSet::new(),
            blacklist: HashSet::new(),
            images: Vec::new(),
        }
    }
}

pub struct AppState {
    pub config: ArcSwap<AppConfig>,
    pub config_path: PathBuf,
}

// 加载配置
pub fn load_config(path: &PathBuf) -> anyhow::Result<AppConfig> {
    let config = AppConfig::load_or_default(path)?;
    // 确保存储目录存在
    fs::create_dir_all(&config.images_dir)?;
    fs::create_dir_all(&config.thumbs_dir)?;
    fs::create_dir_all(&config.temp_dir)?;
    Ok(config)
}

// 保存配置 (持久化)
pub fn save_config(path: &PathBuf, config: &AppConfig) -> anyhow::Result<()> {
    Ok(config.store(path)?)
}
