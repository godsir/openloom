use openloom_models::SkillPermissions;
use anyhow::Result;

pub fn check_permissions(permissions: &SkillPermissions, skill_name: &str, params: &serde_json::Value) -> Result<()> {
    if let Some(paths) = &permissions.fs_read
        && let Some(file_path) = params.get("path").and_then(|v| v.as_str())
        && !paths.iter().any(|allowed| file_path.starts_with(allowed))
    {
        anyhow::bail!("Permission denied: {} may not read '{}' (allowed: {:?})", skill_name, file_path, paths);
    }
    if let Some(paths) = &permissions.fs_write
        && let Some(file_path) = params.get("path").and_then(|v| v.as_str())
        && !paths.iter().any(|allowed| file_path.starts_with(allowed))
    {
        anyhow::bail!("Permission denied: {} may not write '{}' (allowed: {:?})", skill_name, file_path, paths);
    }
    if let Some(domains) = &permissions.network
        && let Some(url) = params.get("url").and_then(|v| v.as_str())
        && !domains.iter().any(|allowed| url.contains(allowed))
    {
        anyhow::bail!("Permission denied: {} may not access '{}' (allowed: {:?})", skill_name, url, domains);
    }
    if !permissions.subprocess && params.get("command").is_some() {
        anyhow::bail!("Permission denied: {} may not execute subprocesses", skill_name);
    }
    Ok(())
}
