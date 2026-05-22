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
    UsesCodexBackend,
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

    pub fn uses_codex_backend(&self) -> bool {
        matches!(self, Self::UsesCodexBackend)
    }
}

/// Stub default client module.
pub mod default_client {
    use reqwest::Client;

    /// Build a reqwest client. Returns a default client.
    pub fn build_reqwest_client() -> std::result::Result<Client, reqwest::Error> {
        Client::builder().build()
    }

    /// Stub Originator struct.
    #[derive(Debug, Clone)]
    pub struct Originator {
        pub value: String,
    }

    /// Stub: always returns empty originator.
    pub fn originator() -> Originator {
        Originator {
            value: String::new(),
        }
    }

    /// Stub: always returns false.
    pub fn is_first_party_chat_originator(_value: &str) -> bool {
        false
    }

    /// Stub user agent suffix.
    pub const USER_AGENT_SUFFIX: &str = "";

    /// Stub: returns default user agent string.
    pub fn get_codex_user_agent() -> String {
        String::from("loom/0.2.0")
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
