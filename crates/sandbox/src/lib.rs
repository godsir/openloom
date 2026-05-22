pub mod risk;

use anyhow::Result;
use openloom_models::SkillPermissions;
use std::path::Path;

pub use risk::{classify_risk, risk_message, should_block};

/// Resolve an allowed path prefix to its canonical absolute form.
fn resolve_allowed(allowed: &str) -> String {
    match allowed {
        "~" => std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .unwrap_or_default(),
        "." => std::env::current_dir()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        other => {
            if Path::new(other).is_absolute() {
                other.to_string()
            } else {
                std::env::current_dir()
                    .unwrap_or_default()
                    .join(other)
                    .to_string_lossy()
                    .to_string()
            }
        }
    }
}

/// Resolve a file path to an absolute form for permission checking.
fn resolve_file_path(file_path: &str) -> String {
    let p = Path::new(file_path);
    if p.is_absolute() {
        p.to_string_lossy().to_string()
    } else {
        std::env::current_dir()
            .unwrap_or_default()
            .join(p)
            .to_string_lossy()
            .to_string()
    }
}

pub fn check_permissions(
    permissions: &SkillPermissions,
    skill_name: &str,
    params: &serde_json::Value,
) -> Result<()> {
    if let Some(paths) = &permissions.fs_read
        && let Some(file_path) = params.get("path").and_then(|v| v.as_str())
    {
        let resolved = resolve_file_path(file_path);
        let allowed = !paths.iter().any(|allowed| {
            let resolved_allowed = resolve_allowed(allowed);
            resolved.starts_with(&resolved_allowed)
        });
        if allowed {
            anyhow::bail!(
                "Permission denied: {} may not read '{}' (allowed: {:?})",
                skill_name,
                file_path,
                paths
            );
        }
    }
    if let Some(paths) = &permissions.fs_write
        && let Some(file_path) = params.get("path").and_then(|v| v.as_str())
    {
        let resolved = resolve_file_path(file_path);
        let allowed = !paths.iter().any(|allowed| {
            let resolved_allowed = resolve_allowed(allowed);
            resolved.starts_with(&resolved_allowed)
        });
        if allowed {
            anyhow::bail!(
                "Permission denied: {} may not write '{}' (allowed: {:?})",
                skill_name,
                file_path,
                paths
            );
        }
    }
    if let Some(domains) = &permissions.network
        && let Some(url) = params.get("url").and_then(|v| v.as_str())
        && !domains.iter().any(|allowed| url.contains(allowed))
    {
        anyhow::bail!(
            "Permission denied: {} may not access '{}' (allowed: {:?})",
            skill_name,
            url,
            domains
        );
    }
    if !permissions.subprocess && params.get("command").is_some() {
        anyhow::bail!(
            "Permission denied: {} may not execute subprocesses",
            skill_name
        );
    }
    Ok(())
}
