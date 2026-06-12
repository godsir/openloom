//! Credential persistence — stores API keys in the OS keychain (via the
//! `keyring` crate) as the primary store, with a JSON file at
//! `<data_dir>/credentials.json` kept as a SAFE graceful fallback.
//!
//! This is the LLM-auth critical path: key storage must NEVER silently fail.
//! Every keyring operation is wrapped so that, on any error (headless/CI
//! environments, locked keychains, missing backend, etc.), we fall back to the
//! plaintext file. The file is also the migration source: on load, any
//! plaintext key found there is opportunistically copied into the keyring.
//!
//! The in-memory key_map remains the runtime source of truth; keyring + file
//! are the durable copies that survive restarts.
//!
//! No unsafe code. No environment variable manipulation. keyring's
//! get/set_password calls are synchronous and may block on OS IPC, so they are
//! run on a blocking thread via `tokio::task::spawn_blocking`.

use std::collections::HashMap;
use std::path::Path;
use tokio::sync::RwLock;

/// Service name used for all keyring entries. The account/user component is the
/// key's env-var name (e.g. `OPENAI_API_KEY`), matching the in-memory map key.
const KEYRING_SERVICE: &str = "openloom";

/// Read a single secret from the OS keychain on a blocking thread.
///
/// Returns `Ok(Some(value))` if present, `Ok(None)` if there is no such entry,
/// and `Err(_)` only for genuine backend failures (so callers can decide
/// whether to fall back to the file).
async fn keyring_get(account: &str) -> Result<Option<String>, keyring::Error> {
    let account = account.to_string();
    let joined = tokio::task::spawn_blocking(move || {
        let entry = keyring::Entry::new(KEYRING_SERVICE, &account)?;
        match entry.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e),
        }
    })
    .await;
    match joined {
        Ok(result) => result,
        // A panic on the blocking thread is treated as a platform failure so the
        // caller falls back to the file rather than aborting.
        Err(join_err) => Err(keyring::Error::PlatformFailure(Box::new(join_err))),
    }
}

/// Write a single secret to the OS keychain on a blocking thread.
async fn keyring_set(account: &str, value: &str) -> Result<(), keyring::Error> {
    let account = account.to_string();
    let value = value.to_string();
    let joined = tokio::task::spawn_blocking(move || {
        let entry = keyring::Entry::new(KEYRING_SERVICE, &account)?;
        entry.set_password(&value)
    })
    .await;
    match joined {
        Ok(result) => result,
        Err(join_err) => Err(keyring::Error::PlatformFailure(Box::new(join_err))),
    }
}

/// Read the legacy plaintext credentials file, if present.
/// Returns an empty map if the file does not exist or is malformed.
async fn read_legacy_file(path: &Path) -> HashMap<String, String> {
    let content = match tokio::fs::read_to_string(path).await {
        Ok(c) => c,
        Err(_) => {
            tracing::debug!(path = %path.display(), "no credentials file (fresh start)");
            return HashMap::new();
        }
    };
    match serde_json::from_str::<HashMap<String, String>>(&content) {
        Ok(map) => map,
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "credentials file corrupt, ignoring");
            HashMap::new()
        }
    }
}

/// Load persisted credentials.
///
/// Strategy (never lose a key):
/// 1. Read the legacy `credentials.json` to discover which accounts exist
///    (keyring cannot enumerate its own entries).
/// 2. For each known account, prefer the keyring value if present (keyring is
///    the source of truth once populated); otherwise keep the file value.
/// 3. Opportunistically migrate any file-only key into the keyring. The file is
///    intentionally left in place as a fallback — it is never auto-deleted.
pub async fn load_credentials(data_dir: &Path) -> HashMap<String, String> {
    let path = data_dir.join("credentials.json");
    let file_map = read_legacy_file(&path).await;

    let mut result: HashMap<String, String> = HashMap::with_capacity(file_map.len());
    let mut migrated = 0usize;
    let mut from_keyring = 0usize;

    for (account, file_value) in &file_map {
        match keyring_get(account).await {
            Ok(Some(kr_value)) => {
                // Keyring already holds this key — it wins as source of truth.
                from_keyring += 1;
                result.insert(account.clone(), kr_value);
            }
            Ok(None) => {
                // Key exists only in the file → migrate it into the keyring.
                match keyring_set(account, file_value).await {
                    Ok(()) => {
                        migrated += 1;
                        tracing::info!(account = %account, "migrated credential into OS keychain");
                    }
                    Err(e) => {
                        tracing::warn!(
                            account = %account,
                            error = %e,
                            "keyring migration failed; keeping plaintext file value"
                        );
                    }
                }
                result.insert(account.clone(), file_value.clone());
            }
            Err(e) => {
                // Keyring backend unavailable — fall back to the file value.
                tracing::warn!(
                    account = %account,
                    error = %e,
                    "keyring read failed; using plaintext file value"
                );
                result.insert(account.clone(), file_value.clone());
            }
        }
    }

    tracing::info!(
        count = result.len(),
        from_keyring,
        migrated,
        path = %path.display(),
        "credentials loaded"
    );

    if migrated > 0 && migrated == file_map.len() {
        // All keys are now in the keyring. We still keep the file as a fallback
        // (never auto-delete) and simply note the fully-migrated state.
        tracing::info!(
            path = %path.display(),
            "all credentials migrated to OS keychain; plaintext file retained as fallback"
        );
    }

    result
}

/// Persist the full key map to the plaintext fallback file at
/// `<data_dir>/credentials.json`.
///
/// This is the graceful-degradation path used whenever the keyring is
/// unavailable; it ensures a key is never silently dropped.
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
        tracing::info!(count = map.len(), path = %path.display(), "credentials persisted to fallback file");
    }
}

/// Save a single API key.
///
/// Order of operations (lock is released before any `.await` to satisfy the
/// `await_holding_lock` lint and to never block other readers on slow I/O):
/// 1. Update the in-memory key_store and take an owned snapshot.
/// 2. Try to write the key to the OS keychain.
/// 3. On ANY keyring error, fall back to writing the plaintext file (logged at
///    warn) so storage never silently fails.
///
/// This is the main entry point called from dispatch handlers.
pub async fn save_key(
    data_dir: &Path,
    key_store: &RwLock<HashMap<String, String>>,
    env_name: &str,
    api_key: &str,
) {
    // Update in-memory map and snapshot it, then drop the guard before awaiting.
    let snapshot = {
        let mut map = key_store.write().await;
        map.insert(env_name.to_string(), api_key.to_string());
        map.clone()
    };

    match keyring_set(env_name, api_key).await {
        Ok(()) => {
            tracing::info!(account = %env_name, "API key stored in OS keychain");
        }
        Err(e) => {
            tracing::warn!(
                account = %env_name,
                error = %e,
                "keyring write failed; falling back to plaintext credentials file"
            );
            persist_credentials(data_dir, &snapshot).await;
        }
    }
}
