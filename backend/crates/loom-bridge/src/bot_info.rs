//! BotInfo — identity info returned by platform adapters.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct BotInfo {
    pub username: Option<String>,
    pub display_name: Option<String>,
}
