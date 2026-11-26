//! Persistent app settings under `%LOCALAPPDATA%\Recleaner\config.toml`.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// User preferences loaded/saved as TOML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub language: String,
    #[serde(default)]
    pub exclusions: Vec<String>,
    #[serde(default)]
    pub last_selected_rules: Vec<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            language: "en".into(),
            exclusions: Vec::new(),
            last_selected_rules: Vec::new(),
        }
    }
}

pub fn app_data_dir() -> PathBuf {
    let dir = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Recleaner");
    let _ = fs::create_dir_all(&dir);
    let _ = fs::create_dir_all(dir.join("logs"));
    dir
}

pub fn config_path() -> PathBuf {
    app_data_dir().join("config.toml")
}

pub fn load_config() -> AppConfig {
    let path = config_path();
    match fs::read_to_string(&path) {
        Ok(s) => toml::from_str(&s).unwrap_or_default(),
        Err(_) => AppConfig::default(),
    }
}

pub fn save_config(cfg: &AppConfig) -> Result<()> {
    let s = toml::to_string_pretty(cfg).context("serialize config")?;
    fs::write(config_path(), s).context("write config")?;
    Ok(())
}
