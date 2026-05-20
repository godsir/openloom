use super::Engine;
use anyhow::Result;
use openloom_models::AppConfig;
use std::path::PathBuf;

impl Engine {
    pub async fn get_config(&self, key: Option<&str>) -> serde_json::Value {
        let config = self.config.read().await;
        match key {
            Some(k) => config.get_nested(k).unwrap_or(serde_json::Value::Null),
            None => serde_json::to_value(&*config).unwrap_or_default(),
        }
    }

    pub async fn set_config(&self, key: &str, value: &str) -> Result<()> {
        let mut config = self.config.write().await;
        config.set_nested(key, value)?;
        let path = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("openLoom")
            .join("config.toml");
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let content = toml::to_string_pretty(&*config)?;
        std::fs::write(&path, content)?;
        tracing::info!(key, value, "config updated");
        Ok(())
    }

    pub async fn load_config_into_engine(&self, config: AppConfig) {
        *self.config.write().await = config;
    }
}
