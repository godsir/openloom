use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use serde_json::{Value, json};
use tokio::sync::RwLock;

use openloom_models::AppConfig;

use crate::settings_registry::{SettingType, settings_registry};
use crate::{Skill, SkillManifest, SkillPermissions};

pub struct UpdateSettingsSkill {
    config: Arc<RwLock<AppConfig>>,
    data_dir: PathBuf,
}

impl UpdateSettingsSkill {
    pub fn new(config: Arc<RwLock<AppConfig>>, data_dir: PathBuf) -> Self {
        Self { config, data_dir }
    }
}

#[async_trait::async_trait]
impl Skill for UpdateSettingsSkill {
    fn name(&self) -> &str {
        "update_settings"
    }

    fn manifest(&self) -> &SkillManifest {
        static M: std::sync::OnceLock<SkillManifest> = std::sync::OnceLock::new();
        M.get_or_init(|| SkillManifest {
            name: "update_settings".into(),
            description: "Search and modify application settings. Use 'search' to find settings by keyword, then 'apply' to change a value.".into(),
            triggers: vec!["设置".into(), "配置".into(), "修改设置".into()],
            permissions: SkillPermissions::default(),
            min_engine_version: "0.1.0".into(),
        })
    }

    async fn invoke(&self, params: Value) -> Result<Value> {
        let action = params
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("search");

        match action {
            "search" => self.search(params).await,
            "apply" => self.apply(params).await,
            _ => Ok(json!({"error": "unknown action", "available": ["search", "apply"]})),
        }
    }

    fn context_md(&self) -> &str {
        "update_settings: search and modify application config. Use search to find settings, apply to change values."
    }
}

impl UpdateSettingsSkill {
    async fn search(&self, params: Value) -> Result<Value> {
        let query = params
            .get("query")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_lowercase();

        let config = self.config.read().await;
        let config_val = serde_json::to_value(&*config).unwrap_or_default();

        let results: Vec<Value> = settings_registry()
            .iter()
            .filter(|m| {
                query.is_empty()
                    || m.key.to_lowercase().contains(&query)
                    || m.label.to_lowercase().contains(&query)
                    || m.description.to_lowercase().contains(&query)
            })
            .map(|m| {
                let mut current = &config_val;
                for segment in m.key.split('.') {
                    current = current.get(segment).unwrap_or(&Value::Null);
                }
                let current = current.clone();

                let entry = json!({
                    "key": m.key,
                    "label": m.label,
                    "description": m.description,
                    "type": setting_type_str(m.setting_type),
                    "scope": match m.scope {
                        crate::settings_registry::SettingScope::Global => "global",
                        crate::settings_registry::SettingScope::Agent => "agent",
                    },
                    "current_value": current,
                });

                match m.setting_type {
                    SettingType::List(options) => {
                        let mut e = entry;
                        if let Some(obj) = e.as_object_mut() {
                            obj.insert("options".into(), json!(options));
                        }
                        e
                    }
                    _ => entry,
                }
            })
            .collect();

        Ok(json!({"results": results}))
    }

    async fn apply(&self, params: Value) -> Result<Value> {
        let key = params.get("key").and_then(|v| v.as_str()).unwrap_or("");

        let value = params.get("value").cloned().unwrap_or(Value::Null);

        if key.is_empty() {
            return Ok(json!({"error": "key is required"}));
        }

        // Validate key exists in registry
        let meta = settings_registry().iter().find(|m| m.key == key);
        if meta.is_none() {
            return Ok(json!({
                "error": format!("unknown setting: '{}'. Use search to find available settings.", key)
            }));
        }
        let meta = meta.unwrap();

        // Validate value against type
        if let Some(err) = validate_value(meta.setting_type, &value) {
            return Ok(json!({"error": err}));
        }

        // Update in-memory config
        {
            let mut config = self.config.write().await;
            config.set_nested(key, value.clone())?;

            // Persist to disk
            let toml_str = toml::to_string_pretty(&*config)
                .map_err(|e| anyhow::anyhow!("Failed to serialize config: {}", e))?;
            std::fs::write(self.data_dir.join("config.toml"), toml_str)
                .map_err(|e| anyhow::anyhow!("Failed to write config.toml: {}", e))?;
        }

        tracing::info!(key = key, "config updated by update_settings tool");
        Ok(json!({"ok": true, "key": key, "message": format!("Setting '{}' updated.", meta.label)}))
    }
}

fn setting_type_str(t: SettingType) -> &'static str {
    match t {
        SettingType::Toggle => "toggle",
        SettingType::List(_) => "list",
        SettingType::Text => "text",
    }
}

fn validate_value(st: SettingType, value: &Value) -> Option<String> {
    match st {
        SettingType::Toggle => match value {
            Value::Bool(_) => None,
            Value::String(s) if s == "true" || s == "false" => None,
            _ => Some(format!("Expected 'true' or 'false', got: {}", value)),
        },
        SettingType::List(options) => {
            if let Value::String(s) = value {
                if options.contains(&s.as_str()) {
                    None
                } else {
                    Some(format!(
                        "Invalid option '{}'. Available: {}",
                        s,
                        options.join(", ")
                    ))
                }
            } else {
                Some(format!("Expected a string value, got: {}", value))
            }
        }
        SettingType::Text => {
            // Text accepts anything string-like
            None
        }
    }
}
