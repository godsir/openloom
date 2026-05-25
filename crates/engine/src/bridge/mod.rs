// Bridge module — social platform integration for openLoom.
//
// Manages per-agent bridge configuration for Telegram, WeChat, Feishu, QQ.
// Stores credentials, enabled state, and owner assignment.
// Provides test-connectivity endpoints that validate credentials against
// the actual platform APIs.

pub mod adapter;
pub mod manager;
pub mod router;
pub mod security;
pub mod store;
pub mod telegram;
pub mod feishu;
pub mod wechat;
pub mod qq;
pub mod types;

pub use types::*;
pub use adapter::ChannelAdapter;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Data types ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgePlatformConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub credentials: serde_json::Value,
    #[serde(default)]
    pub owner: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlatformStatus {
    pub enabled: bool,
    pub configured: bool,
    pub status: String, // "connected", "disconnected", "error"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    // Masked credential fields (show only last 4 chars)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_secret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_id_qq: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_secret_qq: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BridgeStatus {
    pub telegram: PlatformStatus,
    pub feishu: PlatformStatus,
    pub qq: PlatformStatus,
    pub wechat: PlatformStatus,
    pub whatsapp: PlatformStatus,
    pub read_only: bool,
    pub receipt_enabled: bool,
    pub known_users: HashMap<String, Vec<KnownUser>>,
    pub owner: HashMap<String, Option<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct KnownUser {
    pub user_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
}

// ── Config helpers ──

fn mask_secret(value: &str) -> String {
    if value.len() <= 4 {
        return "****".to_string();
    }
    format!("****{}", &value[value.len() - 4..])
}

fn platform_status(
    config: Option<&BridgePlatformConfig>,
    agent_id: &str,
) -> PlatformStatus {
    match config {
        Some(cfg) => {
            let configured = has_credentials(&cfg.credentials);
            let status = if cfg.enabled && configured {
                "disconnected"
            } else {
                "disconnected"
            };
            let (token, app_id, app_secret, app_id_qq, app_secret_qq) =
                extract_credential_fields(&cfg.credentials);

            PlatformStatus {
                enabled: cfg.enabled,
                configured,
                status: status.to_string(),
                error: None,
                agent_id: Some(agent_id.to_string()),
                token: token.map(|s| mask_secret(&s)),
                app_id: app_id,
                app_secret: app_secret.map(|s| mask_secret(&s)),
                app_id_qq,
                app_secret_qq: app_secret_qq.map(|s| mask_secret(&s)),
            }
        }
        None => PlatformStatus {
            enabled: false,
            configured: false,
            status: "disconnected".to_string(),
            error: None,
            agent_id: Some(agent_id.to_string()),
            token: None,
            app_id: None,
            app_secret: None,
            app_id_qq: None,
            app_secret_qq: None,
        },
    }
}

fn has_credentials(credentials: &serde_json::Value) -> bool {
    match credentials {
        serde_json::Value::Object(obj) => obj.values().any(|v| {
            v.as_str().map_or(false, |s| !s.is_empty())
        }),
        _ => false,
    }
}

fn extract_credential_fields(
    credentials: &serde_json::Value,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    let token = credentials.get("token").and_then(|v| v.as_str()).map(String::from);
    let app_id = credentials
        .get("appId")
        .and_then(|v| v.as_str())
        .map(String::from);
    let app_secret = credentials
        .get("appSecret")
        .and_then(|v| v.as_str())
        .map(String::from);
    let app_id_qq = credentials
        .get("appID")
        .and_then(|v| v.as_str())
        .map(String::from);
    let app_secret_qq = credentials
        .get("appSecret")
        .and_then(|v| v.as_str())
        .map(String::from);
    (token, app_id, app_secret, app_id_qq, app_secret_qq)
}

// ── Public API ──

/// Build the full bridge status for an agent from config values.
pub fn build_status(
    bridge_config: &serde_json::Value,
    global_config: &serde_json::Value,
    agent_id: &str,
) -> serde_json::Value {
    let platform = |name: &str| -> Option<BridgePlatformConfig> {
        bridge_config
            .get(name)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    };

    let read_only = global_config.get("readOnly").and_then(|v| v.as_bool()).unwrap_or(false);
    let receipt_enabled = global_config
        .get("receiptEnabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    serde_json::json!({
        "telegram": platform_status(platform("telegram").as_ref(), agent_id),
        "feishu": platform_status(platform("feishu").as_ref(), agent_id),
        "qq": platform_status(platform("qq").as_ref(), agent_id),
        "wechat": platform_status(platform("wechat").as_ref(), agent_id),
        "whatsapp": PlatformStatus {
            enabled: false,
            configured: false,
            status: "disconnected".to_string(),
            error: Some("WhatsApp not yet supported".to_string()),
            agent_id: Some(agent_id.to_string()),
            token: None, app_id: None, app_secret: None,
            app_id_qq: None, app_secret_qq: None,
        },
        "readOnly": read_only,
        "receiptEnabled": receipt_enabled,
        "knownUsers": {},
        "owner": {
            "telegram": platform("telegram").and_then(|c| c.owner),
            "feishu": platform("feishu").and_then(|c| c.owner),
            "qq": platform("qq").and_then(|c| c.owner),
            "wechat": platform("wechat").and_then(|c| c.owner),
            "whatsapp": null,
        }
    })
}

/// Test platform credentials by calling the real API.
pub async fn test_platform(
    platform: &str,
    credentials: &serde_json::Value,
) -> Result<serde_json::Value> {
    match platform {
        "telegram" => test_telegram(credentials).await,
        "feishu" => test_feishu(credentials).await,
        "qq" => test_qq(credentials).await,
        "wechat" => test_wechat(credentials).await,
        "whatsapp" => anyhow::bail!("WhatsApp not yet supported"),
        _ => anyhow::bail!("unknown platform: {}", platform),
    }
}

// ── Platform test implementations ──

async fn test_telegram(credentials: &serde_json::Value) -> Result<serde_json::Value> {
    let token = credentials
        .get("token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("token is required"))?;

    let url = format!("https://api.telegram.org/bot{}/getMe", token);
    let resp = reqwest::get(&url).await?;
    let body: serde_json::Value = resp.json().await?;

    if body.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
        let user = body.get("result").cloned().unwrap_or_default();
        Ok(serde_json::json!({
            "ok": true,
            "info": {
                "username": user.get("username").and_then(|v| v.as_str()).unwrap_or(""),
                "firstName": user.get("first_name").and_then(|v| v.as_str()).unwrap_or(""),
            }
        }))
    } else {
        let desc = body
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("unauthorized");
        anyhow::bail!("{}", desc)
    }
}

async fn test_feishu(credentials: &serde_json::Value) -> Result<serde_json::Value> {
    let app_id = credentials
        .get("appId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("appId is required"))?;
    let app_secret = credentials
        .get("appSecret")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("appSecret is required"))?;

    let client = reqwest::Client::new();
    let resp = client
        .post("https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal")
        .json(&serde_json::json!({
            "app_id": app_id,
            "app_secret": app_secret,
        }))
        .send()
        .await?;
    let body: serde_json::Value = resp.json().await?;

    if body.get("code").and_then(|v| v.as_i64()).unwrap_or(-1) == 0 {
        Ok(serde_json::json!({
            "ok": true,
            "info": {
                "appId": app_id,
            }
        }))
    } else {
        let msg = body
            .get("msg")
            .and_then(|v| v.as_str())
            .unwrap_or("unauthorized");
        anyhow::bail!("{}", msg)
    }
}

async fn test_qq(credentials: &serde_json::Value) -> Result<serde_json::Value> {
    let app_id = credentials
        .get("appID")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("appID is required"))?;
    let app_secret = credentials
        .get("appSecret")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("appSecret is required"))?;

    let client = reqwest::Client::new();
    let resp = client
        .post("https://bots.qq.com/app/getAppAccessToken")
        .json(&serde_json::json!({
            "appId": app_id,
            "clientSecret": app_secret,
        }))
        .send()
        .await?;
    let body: serde_json::Value = resp.json().await?;

    if body.get("access_token").and_then(|v| v.as_str()).is_some() {
        Ok(serde_json::json!({
            "ok": true,
            "info": {
                "appId": app_id,
            }
        }))
    } else {
        let msg = body
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("unauthorized");
        anyhow::bail!("{}", msg)
    }
}

async fn test_wechat(credentials: &serde_json::Value) -> Result<serde_json::Value> {
    let token = credentials
        .get("token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("token is required"))?;

    let client = reqwest::Client::new();
    let resp = client
        .post("https://ilinkai.weixin.qq.com/ilink/bot/getconfig")
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;
    let body: serde_json::Value = resp.json().await?;

    if body.get("errcode").and_then(|v| v.as_i64()).unwrap_or(-1) == 0 {
        Ok(serde_json::json!({
            "ok": true,
            "info": {
                "name": body.get("name").and_then(|v| v.as_str()).unwrap_or(""),
            }
        }))
    } else {
        let msg = body
            .get("errmsg")
            .and_then(|v| v.as_str())
            .unwrap_or("unauthorized");
        anyhow::bail!("{}", msg)
    }
}
