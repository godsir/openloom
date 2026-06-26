//! InstanceConfig, BridgeConfig, IMSettings — persistent IM config types.

use serde::{Deserialize, Serialize};

use crate::types::{AccessMode, Platform};

/// Single platform instance persistent config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceConfig {
    pub id: String,             // uuid
    pub platform: Platform,
    pub instance_id: String,   // "default" / "work" / ...
    pub instance_name: String, // "个人微信" / "工作飞书"
    pub enabled: bool,
    /// Platform-specific config JSON (feishu: {appId, appSecret, domain} etc)
    pub config_json: serde_json::Value,
    /// DM policy
    pub dm_policy: AccessMode,
    pub allow_from: Vec<String>,
    /// Group policy
    pub group_policy: AccessMode,
    pub group_allow_from: Vec<String>,
    /// Agent binding (None = use default)
    pub agent_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// All-platform config snapshot
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BridgeConfig {
    pub instances: Vec<InstanceConfig>,
    pub settings: IMSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IMSettings {
    pub default_dm_policy: AccessMode,
    pub skills_enabled: bool,
    pub default_agent_id: String,
}

impl Default for IMSettings {
    fn default() -> Self {
        Self {
            default_dm_policy: AccessMode::Pairing,
            skills_enabled: true,
            default_agent_id: "main".to_string(),
        }
    }
}
