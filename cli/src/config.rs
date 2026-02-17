use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    pub base_url: Option<String>,
    pub tenant_api_key: Option<String>,
    pub tenant_jwt: Option<String>,
    pub agent_key: Option<String>,
    // Key: "{base_url}|{cluster_id}|{node_id}" -> node token
    pub node_tokens: BTreeMap<String, String>,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let bytes = fs::read(path).with_context(|| format!("Failed to read config {:?}", path))?;
        let cfg = serde_json::from_slice(&bytes)
            .with_context(|| format!("Failed to parse {:?}", path))?;
        Ok(cfg)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create dir {:?}", parent))?;
        }
        let bytes = serde_json::to_vec_pretty(self).context("Failed to serialize config")?;
        fs::write(path, bytes).with_context(|| format!("Failed to write {:?}", path))?;
        Ok(())
    }
}

pub fn default_config_path() -> PathBuf {
    let mut dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    dir.push("quiltc");
    dir.push("config.json");
    dir
}

pub fn node_token_key(base_url: &str, cluster_id: &str, node_id: &str) -> String {
    format!("{}|{}|{}", base_url, cluster_id, node_id)
}
