//! Bridge types: Platform, BridgeMessage, MessageContent, ChannelAdapter trait.

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::bot_info::BotInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Telegram,
    Feishu,
    #[serde(rename = "wechat")]
    Wechat,
    Wecom,
    Dingtalk,
    #[serde(rename = "qq")]
    QQ,
    Discord,
    Popo,
}

impl Platform {
    pub fn name(&self) -> &'static str {
        match self {
            Platform::Telegram => "telegram",
            Platform::Feishu => "feishu",
            Platform::Wechat => "wechat",
            Platform::Wecom => "wecom",
            Platform::Dingtalk => "dingtalk",
            Platform::QQ => "qq",
            Platform::Discord => "discord",
            Platform::Popo => "popo",
        }
    }

    /// Which layer implements this platform's adapter
    pub fn layer(&self) -> AdapterLayer {
        match self {
            Platform::Wechat | Platform::Popo => AdapterLayer::Electron,
            _ => AdapterLayer::Rust,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterLayer {
    Rust,
    Electron,
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageContent {
    Text(String),
    Image {
        url: String,
        caption: Option<String>,
    },
    File {
        url: String,
        name: String,
        size: u64,
    },
    Audio {
        url: String,
        duration_secs: u32,
    },
}

/// Maximum byte length of inbound text/caption/content accepted from an
/// external (untrusted) sender before it is truncated at ingest. Caps memory
/// use and limits prompt-injection surface from arbitrary remote users.
pub const MAX_INBOUND_CONTENT_LEN: usize = 16 * 1024;

/// Marker appended to inbound content that was truncated because it exceeded
/// [`MAX_INBOUND_CONTENT_LEN`]. Downstream consumers can detect truncation.
pub const TRUNCATION_MARKER: &str = "…[truncated]";

/// Truncate untrusted inbound text to [`MAX_INBOUND_CONTENT_LEN`] bytes,
/// appending [`TRUNCATION_MARKER`] when truncation occurs. Truncation happens
/// on a UTF-8 char boundary so the result is always valid UTF-8.
pub fn cap_inbound_text(text: &str) -> String {
    if text.len() <= MAX_INBOUND_CONTENT_LEN {
        return text.to_string();
    }
    // Find the largest char boundary <= the cap so we never split a code point.
    let mut end = MAX_INBOUND_CONTENT_LEN;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    let mut out = String::with_capacity(end + TRUNCATION_MARKER.len());
    out.push_str(&text[..end]);
    out.push_str(TRUNCATION_MARKER);
    out
}

impl MessageContent {
    pub fn media_type(&self) -> &'static str {
        match self {
            Self::Text(_) => "text",
            Self::Image { .. } => "image",
            Self::File { .. } => "file",
            Self::Audio { .. } => "audio",
        }
    }
    pub fn text_content(&self) -> Option<&str> {
        match self {
            Self::Text(t) => Some(t),
            _ => None,
        }
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
    /// Provenance flag: `true` when the content originates from an arbitrary
    /// external/remote sender and must be treated as untrusted (e.g. subject to
    /// prompt-injection). Inbound platform messages set this; defaults to
    /// `true` on deserialization so unknown-origin messages fail safe.
    #[serde(default = "default_untrusted")]
    pub untrusted: bool,
}

fn default_untrusted() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AdapterHealth {
    Connected,
    Connecting,
    Disconnected,
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageDirection {
    Inbound,
    Outbound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccessState {
    Active,
    Blocked,
    Pending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AccessMode {
    #[default]
    Open,
    Pairing,
    Allowlist,
}

#[async_trait]
pub trait ChannelAdapter: Send + Sync {
    fn platform(&self) -> Platform;
    fn instance_id(&self) -> &str;
    fn instance_name(&self) -> &str;

    async fn connect(&mut self) -> Result<()>;
    async fn disconnect(&mut self) -> Result<()>;
    async fn send(&self, chat_id: &str, content: MessageContent) -> Result<String>;
    fn receive_rx(&mut self) -> &mut mpsc::Receiver<BridgeMessage>;
    fn health(&self) -> AdapterHealth;

    /// Validate credentials without establishing long connection (for "test connectivity")
    async fn validate_credentials(&self) -> Result<()>;
    /// Get bot identity info (username etc.)
    async fn get_bot_info(&self) -> Option<BotInfo> {
        None
    }
}
