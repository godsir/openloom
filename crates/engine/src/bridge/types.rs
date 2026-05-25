use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Supported messaging platforms
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
            Self::Telegram => "telegram",
            Self::Feishu => "feishu",
            Self::Wechat => "wechat",
            Self::QQ => "qq",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "telegram" => Some(Self::Telegram),
            "feishu" => Some(Self::Feishu),
            "wechat" => Some(Self::Wechat),
            "qq" => Some(Self::QQ),
            _ => None,
        }
    }
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Message content types supported across all platforms
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageContent {
    Text(String),
    Image { url: String, caption: Option<String> },
    File { url: String, name: String, size: u64 },
    Audio { url: String, duration_secs: u32 },
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
            Self::Text(s) => Some(s.as_str()),
            Self::Image { caption, .. } => caption.as_deref(),
            _ => None,
        }
    }
}

/// Standardized inbound/outbound message — all adapters convert to/from this
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeMessage {
    pub platform: Platform,
    pub chat_id: String,
    pub sender_id: String,
    pub sender_name: String,
    pub content: MessageContent,
    pub reply_to: Option<String>,
    pub external_message_id: String,
    pub timestamp: DateTime<Utc>,
}

/// Health status of a platform adapter
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AdapterHealth {
    Connected,
    Connecting,
    Disconnected,
    Error(String),
}

/// Direction of a bridge message (for DB storage)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageDirection {
    Inbound,
    Outbound,
}

impl MessageDirection {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Inbound => "inbound",
            Self::Outbound => "outbound",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "inbound" => Some(Self::Inbound),
            "outbound" => Some(Self::Outbound),
            _ => None,
        }
    }
}

/// Access state for a bridge session
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccessState {
    Active,
    Blocked,
    Pending,
}

impl AccessState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Blocked => "blocked",
            Self::Pending => "pending",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "active" => Some(Self::Active),
            "blocked" => Some(Self::Blocked),
            "pending" => Some(Self::Pending),
            _ => None,
        }
    }
}

/// Access mode for new users
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccessMode {
    Pairing,
    Allowlist,
    Open,
}

impl Default for AccessMode {
    fn default() -> Self {
        Self::Open
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_roundtrip() {
        let platforms = [Platform::Telegram, Platform::Feishu, Platform::Wechat, Platform::QQ];
        for p in &platforms {
            let name = p.name();
            let parsed = Platform::from_str(name);
            assert_eq!(parsed, Some(*p), "roundtrip failed for {name}");
        }
    }

    #[test]
    fn test_platform_display() {
        assert_eq!(Platform::Telegram.to_string(), "telegram");
        assert_eq!(Platform::Feishu.to_string(), "feishu");
    }

    #[test]
    fn test_platform_from_str_case_insensitive() {
        assert_eq!(Platform::from_str("TELEGRAM"), Some(Platform::Telegram));
        assert_eq!(Platform::from_str("Feishu"), Some(Platform::Feishu));
        assert_eq!(Platform::from_str("unknown"), None);
    }

    #[test]
    fn test_message_content_media_type() {
        assert_eq!(MessageContent::Text("hi".into()).media_type(), "text");
        let img = MessageContent::Image { url: "u".into(), caption: None };
        assert_eq!(img.media_type(), "image");
    }

    #[test]
    fn test_message_content_text_content() {
        assert_eq!(
            MessageContent::Text("hello".into()).text_content(),
            Some("hello")
        );
        let img = MessageContent::Image {
            url: "u".into(),
            caption: Some("pic".into()),
        };
        assert_eq!(img.text_content(), Some("pic"));
        let file = MessageContent::File {
            url: "u".into(),
            name: "f".into(),
            size: 0,
        };
        assert_eq!(file.text_content(), None);
    }

    #[test]
    fn test_bridge_message_serialize() {
        let msg = BridgeMessage {
            platform: Platform::Telegram,
            chat_id: "123".into(),
            sender_id: "user1".into(),
            sender_name: "Alice".into(),
            content: MessageContent::Text("hi".into()),
            reply_to: None,
            external_message_id: "msg_1".into(),
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&msg);
        assert!(json.is_ok());
        let deserialized: BridgeMessage = serde_json::from_str(&json.unwrap()).unwrap();
        assert_eq!(deserialized.platform, Platform::Telegram);
        assert_eq!(deserialized.chat_id, "123");
    }

    #[test]
    fn test_access_state_roundtrip() {
        let states = [AccessState::Active, AccessState::Blocked, AccessState::Pending];
        for s in &states {
            let name = s.as_str();
            let parsed = AccessState::from_str(name);
            assert_eq!(parsed, Some(*s));
        }
    }

    #[test]
    fn test_direction_roundtrip() {
        assert_eq!(MessageDirection::from_str("inbound"), Some(MessageDirection::Inbound));
        assert_eq!(MessageDirection::from_str("outbound"), Some(MessageDirection::Outbound));
        assert_eq!(MessageDirection::from_str("invalid"), None);
    }
}
