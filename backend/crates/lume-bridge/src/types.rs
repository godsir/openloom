//! Bridge types: Platform, BridgeMessage, MessageContent, ChannelAdapter trait.

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Telegram,
    Feishu,
    Wechat,
    QQ,
}

impl Platform {
    pub fn name(&self) -> &'static str {
        match self {
            Platform::Telegram => "telegram",
            Platform::Feishu => "feishu",
            Platform::Wechat => "wechat",
            Platform::QQ => "qq",
        }
    }
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageContent {
    Text(String),
    Image { url: String, caption: Option<String> },
    File { url: String, name: String, size: u64 },
    Audio { url: String, duration_secs: u32 },
}

impl MessageContent {
    pub fn media_type(&self) -> &'static str {
        match self { Self::Text(_) => "text", Self::Image { .. } => "image", Self::File { .. } => "file", Self::Audio { .. } => "audio" }
    }
    pub fn text_content(&self) -> Option<&str> {
        match self { Self::Text(t) => Some(t), _ => None }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeMessage {
    pub platform: Platform,
    pub chat_id: String,
    pub sender_id: String,
    pub sender_name: String,
    pub content: MessageContent,
    pub reply_to: Option<String>,
    pub external_message_id: String,
    #[serde(default = "Utc::now")]
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AdapterHealth { Connected, Connecting, Disconnected, Error(String) }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageDirection { Inbound, Outbound }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccessState { Active, Blocked, Pending }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AccessMode { #[default] Open, Pairing, Allowlist }

#[async_trait]
pub trait ChannelAdapter: Send + Sync {
    fn platform(&self) -> Platform;
    async fn connect(&mut self) -> Result<()>;
    async fn disconnect(&mut self) -> Result<()>;
    async fn send(&self, chat_id: &str, content: MessageContent) -> Result<String>;
    fn receive_rx(&mut self) -> &mut mpsc::Receiver<BridgeMessage>;
    fn health(&self) -> AdapterHealth;
}
