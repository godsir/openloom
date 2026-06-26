//! Bridge system for openLoom v2 — connects Agent to external messaging platforms.
//!
//! Supported: Telegram, Feishu, WeChat, WeCom, DingTalk, QQ, Discord, Popo
//! (Telegram implemented first; others are placeholder / in-progress).

pub mod bot_info;
pub mod channel_config;
pub mod feishu;
pub mod manager;
pub mod store;
pub mod telegram;
pub mod types;
pub mod wechat;

pub use bot_info::BotInfo;
pub use channel_config::{BridgeConfig, IMSettings, InstanceConfig};
pub use feishu::FeishuAdapter;
pub use manager::BridgeManager;
pub use store::{BridgeStore, NullOffsetStore, OffsetStore};
pub use telegram::TelegramAdapter;
pub use types::*;
pub use wechat::WechatAdapter;
