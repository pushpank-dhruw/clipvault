use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Config {
    pub max_entries: usize,
    pub max_image_entries: usize,
    pub poll_interval_ms: u64,
    pub theme: String,
    pub shelf_width: f32,
    pub shelf_height: f32,
    pub shelf_thumb_size: f32,
    pub shelf_max_entries: usize,
    pub notch_hover: bool,
    pub notch_hover_width: f32,
    pub notch_hover_height: f32,
    pub notch_hover_dwell_ms: u64,
    pub notch_hover_close_delay_ms: u64,
    pub notch_hover_poll_ms: u64,
    pub ocr_enabled: bool,
    pub hide_sensitive: bool,
    pub image_store_dir: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_entries: 500,
            max_image_entries: 50,
            poll_interval_ms: 500,
            theme: "tokyo-night".into(),
            shelf_width: 820.0,
            shelf_height: 220.0,
            shelf_thumb_size: 56.0,
            shelf_max_entries: 50,
            notch_hover: true,
            notch_hover_width: 300.0,
            notch_hover_height: 8.0,
            notch_hover_dwell_ms: 120,
            notch_hover_close_delay_ms: 400,
            notch_hover_poll_ms: 90,
            ocr_enabled: false,
            hide_sensitive: false,
            image_store_dir: "images".into(),
        }
    }
}

impl Config {
    pub fn path() -> Result<PathBuf> {
        let dir = directories::ProjectDirs::from("", "", "clipvault")
            .context("failed to determine config directory")?;
        let config_dir = dir.config_dir().to_path_buf();
        Ok(config_dir.join("config.toml"))
    }

    pub fn data_dir() -> Result<PathBuf> {
        let dir = directories::ProjectDirs::from("", "", "clipvault")
            .context("failed to determine data directory")?;
        let data_dir = dir.data_dir().to_path_buf();
        Ok(data_dir)
    }

    pub fn images_dir() -> Result<PathBuf> {
        Ok(Self::data_dir()?.join("images"))
    }

    pub fn db_path() -> Result<PathBuf> {
        Ok(Self::data_dir()?.join("clipvault.db"))
    }

    pub fn socket_path() -> PathBuf {
        let uid = unsafe { libc::getuid() };
        PathBuf::from(format!("/run/user/{}/clipvault.sock", uid))
    }

    pub fn load() -> Result<Self> {
        let path = Self::path()?;
        if !path.exists() {
            let config = Config::default();
            config.save()?;
            return Ok(config);
        }
        let contents = std::fs::read_to_string(&path)
            .context(format!("failed to read config at {}", path.display()))?;
        toml::from_str(&contents).context("failed to parse config")
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).context("failed to create config directory")?;
        }
        let contents = toml::to_string_pretty(self).context("failed to serialize config")?;
        std::fs::write(&path, contents).context("failed to write config")
    }
}
