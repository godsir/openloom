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

    pub async fn set_config(&self, key: &str, mut value: serde_json::Value) -> Result<()> {
        // Before storing, intercept api_key in provider data and save to secrets
        if key == "general" || key.starts_with("settings.") {
            self.extract_api_keys_to_secrets(&mut value);
        }

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
        tracing::info!(key, "config updated");

        // Re-apply tools.disabled to SkillRegistry when tools config changes
        if key == "settings.tools" || key == "settings" {
            let names: Vec<String> = config
                .settings
                .get("tools")
                .and_then(|t| t.get("disabled"))
                .and_then(|d| d.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            tracing::info!(?names, "Applying tool disabled list from config");
            self.skills.set_disabled(names);
        }

        Ok(())
    }

    pub async fn load_config_into_engine(&self, config: AppConfig) {
        *self.config.write().await = config;
    }

    /// Extract api_key values from provider data and save to secrets.json.
    /// Replaces the plaintext api_key with an api_key_env reference.
    fn extract_api_keys_to_secrets(&self, value: &mut serde_json::Value) {
        // Look for { providers: { name: { api_key: "sk-xxx" } } }
        let providers = match value.get_mut("providers").and_then(|v| v.as_object_mut()) {
            Some(p) => p,
            None => return,
        };

        for (prov_name, prov_val) in providers.iter_mut() {
            // Handle null (delete) — also delete from secrets
            if prov_val.is_null() {
                let _ = crate::secrets::delete(&self.data_dir, prov_name);
                continue;
            }

            // Extract api_key if present
            if let Some(api_key) = prov_val.get("api_key").and_then(|v| v.as_str()) {
                if !api_key.is_empty() {
                    match crate::secrets::set(&self.data_dir, prov_name, api_key) {
                        Ok(env_var) => {
                            // Replace api_key with api_key_env reference
                            if let Some(obj) = prov_val.as_object_mut() {
                                obj.remove("api_key");
                                obj.insert(
                                    "api_key_env".into(),
                                    serde_json::Value::String(env_var),
                                );
                                obj.insert("has_api_key".into(), serde_json::Value::Bool(true));
                            }
                        }
                        Err(e) => {
                            tracing::warn!(provider = prov_name, error = %e, "Failed to save API key to secrets");
                        }
                    }
                } else {
                    // Empty string — remove the api_key field
                    if let Some(obj) = prov_val.as_object_mut() {
                        obj.remove("api_key");
                        obj.insert("has_api_key".into(), serde_json::Value::Bool(false));
                    }
                }
            }
        }
    }
}
