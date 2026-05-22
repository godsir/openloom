// Stub for codex-login types.

use std::path::Path;

/// Stub auth manager. All methods return None / are no-ops.
#[derive(Clone, Debug)]
pub struct AuthManager;

impl AuthManager {
    pub fn new() -> Self {
        Self
    }

    pub async fn auth(&self) -> Option<CodexAuth> {
        None
    }
}

/// Stub auth type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CodexAuth {
    None,
}

impl CodexAuth {
    pub fn get_account_id(&self) -> Option<String> {
        None
    }

    pub fn get_chatgpt_user_id(&self) -> Option<String> {
        None
    }

    pub fn is_workspace_account(&self) -> bool {
        false
    }
}

/// Stub default client module.
pub mod default_client {
    use reqwest::Client;

    /// Build a reqwest client. Returns a default client.
    pub fn build_reqwest_client() -> std::result::Result<Client, reqwest::Error> {
        Client::builder().build()
    }
}

/// Stub function (no-op).
pub fn enforce_login_restrictions() {}

/// Stub function (no-op).
pub fn set_default_client_residency_requirement() {}

/// Stub: always returns None.
pub fn read_openai_api_key_from_env() -> Option<String> {
    None
}

/// Stub: always returns None.
pub fn load_auth_dot_json(_path: &Path) -> Option<serde_json::Value> {
    None
}
