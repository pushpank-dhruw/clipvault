use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub max_entries: usize,
    pub poll_interval_ms: u64,
    pub theme: String,
    pub overlay_width: f32,
    pub overlay_height: f32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_entries: 500,
            poll_interval_ms: 500,
            theme: "tokyo-night".into(),
            overlay_width: 600.0,
            overlay_height: 450.0,
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
