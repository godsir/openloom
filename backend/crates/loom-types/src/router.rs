//! Intent classification types used by the router.
//!
//! Consumers: loom-core (classify), loom-server (RPC), loom-cli

use serde::{Deserialize, Serialize};

/// Classified user intent from message analysis.
///
/// Consumers: loom-core (SmartRouter), loom-server
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Intent {
    Chat,
    FileOperation,
    WebSearch,
    CodeAssist,
    Schedule,
    Question,
    Other,
}

impl std::fmt::Display for Intent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Intent::Chat => write!(f, "chat"),
            Intent::FileOperation => write!(f, "file_operation"),
            Intent::WebSearch => write!(f, "web_search"),
            Intent::CodeAssist => write!(f, "code_assist"),
            Intent::Schedule => write!(f, "schedule"),
            Intent::Question => write!(f, "question"),
            Intent::Other => write!(f, "other"),
        }
    }
}

/// Target model for routing after intent classification.
///
/// Consumers: loom-core (router dispatch), loom-server
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TargetModel {
    Local,
    None,
    Cloud,
}

/// Result of intent classification with metadata.
///
/// Consumers: loom-core (router), loom-server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassifyOutput {
    pub intent: Intent,
    pub complexity: f32,
    pub skill_match: Option<String>,
    pub confidence: f32,
    pub cache_hit: bool,
    pub target_model: TargetModel,
    #[serde(default)]
    pub route_reason: String,
}
