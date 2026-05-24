use std::collections::HashMap;
use std::path::{Path, PathBuf};

// Manages API keys in a separate secrets.json file, keeping them out of config.toml.
// On load, keys are injected into the process environment so existing `api_key_env`
// lookups via `std::env::var` continue to work unchanged.

const SECRETS_FILE: &str = "secrets.json";

fn secrets_path(data_dir: &Path) -> PathBuf {
    data_dir.join(SECRETS_FILE)
}

/// Generate a conventional env var name for a provider: `LOOM_{NAME}_API_KEY`.
pub fn env_var_name(provider: &str) -> String {
    format!("LOOM_{}_API_KEY", provider.to_uppercase().replace(|c: char| !c.is_alphanumeric(), "_"))
}

/// Load secrets from disk and inject into process environment.
pub fn load(data_dir: &Path) {
    let path = secrets_path(data_dir);
    if !path.exists() {
        return;
    }
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            match serde_json::from_str::<HashMap<String, String>>(&content) {
                Ok(map) => {
                    for (provider, key) in &map {
                        let var = env_var_name(provider);
                        // SAFETY: Setting env vars at startup is safe; we own these LOOM_* keys.
                        unsafe { std::env::set_var(&var, key); }
                    }
                    tracing::info!(count = map.len(), "Loaded secrets into environment");
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to parse secrets.json");
                }
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "Failed to read secrets.json");
        }
    }
}

/// Read the full secrets map from disk.
fn read_map(data_dir: &Path) -> HashMap<String, String> {
    let path = secrets_path(data_dir);
    if !path.exists() {
        return HashMap::new();
    }
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_default()
}

/// Write the full secrets map to disk.
fn write_map(data_dir: &Path, map: &HashMap<String, String>) -> anyhow::Result<()> {
    let path = secrets_path(data_dir);
    let content = serde_json::to_string_pretty(map)?;
    std::fs::write(&path, content)?;
    Ok(())
}

/// Store an API key for a provider. Writes to secrets.json and sets the env var.
pub fn set(data_dir: &Path, provider: &str, api_key: &str) -> anyhow::Result<String> {
    let mut map = read_map(data_dir);
    map.insert(provider.to_string(), api_key.to_string());
    write_map(data_dir, &map)?;

    let var = env_var_name(provider);
    // SAFETY: We own these LOOM_* env vars; setting them is safe.
    unsafe { std::env::set_var(&var, api_key); }
    tracing::info!(provider, var = %var, "Saved API key to secrets");
    Ok(var)
}

/// Get the actual API key for a provider from the environment.
pub fn get(provider: &str) -> Option<String> {
    let var = env_var_name(provider);
    std::env::var(&var).ok()
}

/// Get a masked version of the API key for display: show only last 4 chars.
pub fn get_masked(provider: &str) -> Option<String> {
    let key = get(provider)?;
    if key.len() <= 4 {
        Some("*".repeat(key.len()))
    } else {
        Some(format!("{}{}", "*".repeat(key.len() - 4), &key[key.len() - 4..]))
    }
}

/// Delete the API key for a provider. Removes from secrets.json and unsets the env var.
pub fn delete(data_dir: &Path, provider: &str) -> anyhow::Result<()> {
    let mut map = read_map(data_dir);
    if map.remove(provider).is_some() {
        write_map(data_dir, &map)?;
        let var = env_var_name(provider);
        // SAFETY: We own these LOOM_* env vars; removing them is safe.
        unsafe { std::env::remove_var(&var); }
        tracing::info!(provider, "Deleted API key from secrets");
    }
    Ok(())
}
