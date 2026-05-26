//! JSON-RPC 2.0 request/response envelope types.
//!
//! Consumers: loom-server (dispatch, WS handler), loom-core (event bus), loom-cli

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// JSON-RPC 2.0 request.
///
/// Consumers: loom-server (dispatch), loom-core (internal RPC)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<Value>,
    pub id: u64,
}

/// JSON-RPC 2.0 response.
///
/// Consumers: loom-server (dispatch response), loom-core
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
    pub id: u64,
}

/// JSON-RPC 2.0 error object.
///
/// Consumers: loom-server (dispatch), loom-core (tool execution)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// JSON-RPC 2.0 standard and openLoom-specific error codes.
///
/// Consumers: loom-server (dispatch), loom-core (agent loop errors)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    #[serde(rename = "-32700")]
    ParseError = -32700,
    #[serde(rename = "-32600")]
    InvalidRequest = -32600,
    #[serde(rename = "-32601")]
    MethodNotFound = -32601,
    #[serde(rename = "-32603")]
    InternalError = -32603,
    #[serde(rename = "-32000")]
    ModelUnavailable = -32000,
    #[serde(rename = "-32001")]
    SkillFailed = -32001,
    #[serde(rename = "-32002")]
    PermissionDenied = -32002,
    #[serde(rename = "-32003")]
    Timeout = -32003,
    #[serde(rename = "-32004")]
    McpServerError = -32004,
    #[serde(rename = "-32005")]
    SessionNotFound = -32005,
    #[serde(rename = "-32006")]
    AgentNotFound = -32006,
    #[serde(rename = "-32007")]
    StreamAborted = -32007,
}
