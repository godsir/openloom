//! Bridge system for openLoom v2 — connects Agent to external messaging platforms.
//!
//! Supported: Telegram, Feishu, WeChat, QQ (Telegram implemented first).

pub mod manager;
pub mod store;
pub mod telegram;
pub mod types;
pub mod wechat;

pub use manager::BridgeManager;
pub use telegram::TelegramAdapter;
pub use types::*;
pub use wechat::WechatAdapter;
