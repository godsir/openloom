//! Credential persistence — saves API keys as permanent user environment variables.
//! On Windows: uses `setx` to set user-level env var (survives restarts).
//! On Unix: appends to ~/.profile for shell-level persistence.

/// Set the env var for the current process (always instant).
/// Spawns a background thread to persist via setx / ~/.profile.
/// Returns the env var name that was set.
pub fn save_credential(env_name: &str, api_key: &str) -> String {
    unsafe { std::env::set_var(env_name, api_key); }

    let name = env_name.to_string();
    let key = api_key.to_string();
    let ret = name.clone();

    #[cfg(target_os = "windows")]
    {
        std::thread::spawn(move || {
            match std::process::Command::new("cmd")
                .args(["/c", "setx", &name, &key])
                .output()
            {
                Ok(o) if o.status.success() => tracing::info!(%name, "credential persisted via setx"),
                Ok(o) => {
                    tracing::warn!(%name, stdout=%String::from_utf8_lossy(&o.stdout).trim(), stderr=%String::from_utf8_lossy(&o.stderr).trim(), "setx failed");
                }
                Err(e) => tracing::warn!(%name, error=%e, "failed to run setx"),
            }
        });
    }

    #[cfg(not(target_os = "windows"))]
    {
        std::thread::spawn(move || {
            if let Err(e) = (|| -> anyhow::Result<()> {
                let home = std::env::var("HOME").unwrap_or_default();
                let profile = std::path::PathBuf::from(&home).join(".profile");
                let line = format!("\n# openLoom credential\nexport {}=\"{}\"\n", name, key);
                let mut c = std::fs::read_to_string(&profile).unwrap_or_default();
                if let Some(s) = c.find(&format!("export {}=", name)) {
                    if let Some(e) = c[s..].find('\n') { c.replace_range(s..s+e+1, &line); }
                    else { c.replace_range(s.., &line); }
                } else { c.push_str(&line); }
                std::fs::write(&profile, &c)?;
                tracing::info!(%name, "credential persisted to ~/.profile");
                Ok(())
            })() {
                tracing::warn!(%name, error=%e, "failed to persist credential");
            }
        });
    }

    ret
}
