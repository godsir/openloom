//! Credential persistence — stores API keys in a JSON file at
//! <data_dir>/credentials.json. The in-memory key_map is the primary
//! source of truth; the file is a secondary copy that survives restarts.
//!
//! No unsafe code. No environment variable manipulation.

use std::collections::HashMap;
use std::path::Path;
use tokio::sync::RwLock;

/// Load persisted credentials from <data_dir>/credentials.json.
/// Returns an empty map if the file does not exist or is malformed.
pub async fn load_credentials(data_dir: &Path) -> HashMap<String, String> {
    let path = data_dir.join("credentials.json");
    let content = match tokio::fs::read_to_string(&path).await {
        Ok(c) => c,
        Err(_) => {
            tracing::debug!(path = %path.display(), "no credentials file (fresh start)");
            return HashMap::new();
        }
    };
    match serde_json::from_str::<HashMap<String, String>>(&content) {
        Ok(map) => {
            tracing::info!(count = map.len(), path = %path.display(), "credentials loaded");
            map
        }
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "credentials file corrupt, starting empty");
            HashMap::new()
        }
    }
}

/// Persist the full key map to <data_dir>/credentials.json.
pub async fn persist_credentials(data_dir: &Path, map: &HashMap<String, String>) {
    let _ = tokio::fs::create_dir_all(data_dir).await;
    let path = data_dir.join("credentials.json");
    let content = match serde_json::to_string_pretty(map) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = %e, "failed to serialize credentials");
            return;
        }
    };
    if let Err(e) = tokio::fs::write(&path, &content).await {
        tracing::error!(path = %path.display(), error = %e, "failed to write credentials file");
    } else {
        tracing::info!(count = map.len(), path = %path.display(), "credentials persisted");
    }
}

/// Save a single API key into the in-memory key_store and persist to disk.
/// This is the main entry point called from dispatch handlers.
pub async fn save_key(
    data_dir: &Path,
    key_store: &RwLock<HashMap<String, String>>,
    env_name: &str,
    api_key: &str,
) {
    {
        let mut map = key_store.write().await;
        map.insert(env_name.to_string(), api_key.to_string());
        persist_credentials(data_dir, &map).await;
    }
}
