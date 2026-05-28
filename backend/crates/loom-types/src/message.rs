//! Message and content types for agent-model communication.
//!
//! Consumers: loom-core (agent loop), loom-inference, loom-server (dispatch), loom-context

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::role::Role;

// --- Legacy chat message (deprecated, kept for migration) ---

/// Legacy flat chat message. Use `Message` for new code.
///
/// Consumers: loom-server (dispatch, session messages), loom-core (agent loop input)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(default = "Utc::now")]
    pub timestamp: DateTime<Utc>,
    #[serde(default)]
    pub metadata: Option<String>,
    #[serde(default)]
    pub id: Option<i64>,
    #[serde(default)]
    pub seq: Option<i64>,
}

// --- Native tool-calling message types ---

/// A single content block within a message.
///
/// Consumers: loom-core (agent loop), loom-inference (request construction), loom-context (assembler)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentPart {
    Text {
        text: String,
    },
    Image {
        source_type: String,
        media_type: String,
        data: String,
    },
    ToolCall {
        id: String,
        name: String,
        arguments: serde_json::Value,
    },
    ToolResult {
        tool_call_id: String,
        name: String,
        result: String,
    },
    /// Anthropic extended thinking — must be passed back to the API verbatim.
    Thinking {
        text: String,
    },
}

/// Structured message with rich content parts.
///
/// Consumers: loom-core (agent loop), loom-inference (CompletionRequest), loom-context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Vec<ContentPart>,
    #[serde(default = "Utc::now")]
    pub timestamp: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub usage: Option<crate::inference::TokenUsage>,
}

impl Message {
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: vec![ContentPart::Text { text: text.into() }],
            timestamp: Utc::now(),
            usage: None,
        }
    }

    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: vec![ContentPart::Text { text: text.into() }],
            timestamp: Utc::now(),
            usage: None,
        }
    }

    pub fn tool(
        tool_call_id: impl Into<String>,
        name: impl Into<String>,
        result: impl Into<String>,
    ) -> Self {
        Self {
            role: Role::Tool,
            content: vec![ContentPart::ToolResult {
                tool_call_id: tool_call_id.into(),
                name: name.into(),
                result: result.into(),
            }],
            timestamp: Utc::now(),
            usage: None,
        }
    }

    pub fn text_content(&self) -> String {
        self.content
            .iter()
            .filter_map(|c| match c {
                ContentPart::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }

    pub fn tool_calls(&self) -> Vec<(String, String, serde_json::Value)> {
        self.content
            .iter()
            .filter_map(|c| match c {
                ContentPart::ToolCall {
                    id,
                    name,
                    arguments,
                } => Some((id.clone(), name.clone(), arguments.clone())),
                _ => None,
            })
            .collect()
    }
}

/// Lightweight image part for multimodal message construction.
///
/// Consumers: loom-core (agent loop), loom-inference (request construction)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImagePart {
    pub data: String,
    pub mime_type: String,
}

// --- Conversion from legacy ---

impl Message {
    /// Convert a legacy `ChatMessage` into a structured `Message`.
    pub fn from_legacy(msg: &ChatMessage) -> Self {
        let role = match msg.role.as_str() {
            "user" => Role::User,
            "assistant" => Role::Assistant,
            "system" => Role::System,
            _ => Role::User,
        };
        Self {
            role,
            content: vec![ContentPart::Text {
                text: msg.content.clone(),
            }],
            timestamp: msg.timestamp,
            usage: None,
        }
    }
}
