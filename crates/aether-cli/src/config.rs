use anyhow::{Result, Context};
use crate::errors::{CliError, CliErrorKind};
use tracing::debug;
use serde::{Deserialize};
use std::{fs, path::PathBuf};

#[derive(Debug, Deserialize, Default)]
pub struct FileConfig {
    pub default_namespace: Option<String>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct EffectiveConfig {
    pub default_namespace: Option<String>,
}

#[allow(dead_code)]
impl EffectiveConfig {
    pub fn load() -> Result<Self> {
        let cfg_path = config_file_path();
        debug!(path=?cfg_path, exists=?cfg_path.exists(), "config.load.attempt");
        let mut file_cfg: FileConfig = if cfg_path.exists() {
            let content = fs::read_to_string(&cfg_path).with_context(|| format!("read config {cfg_path:?}"))
                .map_err(|e| CliError::with_source(CliErrorKind::Config("failed to read config".into()), e))?;
            debug!(len=content.len(), preview=%content, "config.read");
            match toml::from_str(&content) {
                Ok(v)=>{ debug!("config.parse.success"); v }
                Err(e)=>{ debug!(error=?e, "config.parse.error"); return Err(CliError::with_source(CliErrorKind::Config("failed to parse config".into()), e).into()); }
            }
        } else { FileConfig::default() };
        // Env overrides (AETHER_DEFAULT_NAMESPACE)
        if let Ok(ns) = std::env::var("AETHER_DEFAULT_NAMESPACE") { if !ns.is_empty() { file_cfg.default_namespace = Some(ns); } }
        Ok(Self { default_namespace: file_cfg.default_namespace })
    }
}

pub fn config_dir() -> PathBuf { dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")) .join("aether") }
pub fn cache_dir() -> PathBuf { dirs::cache_dir().unwrap_or_else(|| PathBuf::from(".")) .join("aether") }
pub fn config_file_path() -> PathBuf { config_dir().join("config.toml") }
pub fn session_file_path() -> PathBuf { cache_dir().join("session.json") }
