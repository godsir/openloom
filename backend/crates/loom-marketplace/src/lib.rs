//! Plugin & skill marketplace for openLoom.
//!
//! Provides:
//! - A curated default catalog of community plugins and skills.
//! - Installation via `git clone` into the correct target directory
//!   (plugins → ~/.loom/plugins/, skills → ~/.loom/skills/).
//! - Uninstallation.
//! - Install-status tracking.

mod catalog;

use anyhow::{Result, anyhow};
use serde::Serialize;
use std::path::{Path, PathBuf};

pub use catalog::{MarketPlugin, MarketplaceCatalog, MarketEntryKind};

// ============================================================================
// Types
// ============================================================================

/// A marketplace entry annotated with local install status.
#[derive(Debug, Clone, Serialize)]
pub struct MarketPluginWithStatus {
    /// Unique identifier.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Short description.
    pub description: String,
    /// Latest available version in the marketplace.
    pub version: String,
    /// Author / maintainer.
    pub author: String,
    /// Git clone URL.
    pub git_url: String,
    /// Category (security, productivity, etc.).
    pub category: String,
    /// Entry kind: plugin or skill.
    pub kind: String,
    /// Search tags.
    pub tags: Vec<String>,
    /// Optional project homepage.
    pub homepage: Option<String>,
    /// Whether the entry is currently installed.
    pub installed: bool,
    /// Whether a newer version is available in the catalog.
    pub has_update: bool,
    /// Version of the locally installed copy (if any).
    pub installed_version: Option<String>,
    /// Path to the installed entry on disk (if any).
    pub installed_path: Option<String>,
}

impl MarketPluginWithStatus {
    /// Build from a catalog entry + local filesystem state.
    fn from_catalog_entry(
        entry: &MarketPlugin,
        plugins_dir: &Path,
        skills_dir: &Path,
    ) -> Self {
        let target_dir = match entry.kind {
            MarketEntryKind::Plugin => plugins_dir,
            MarketEntryKind::Skill => skills_dir,
        };
        let (installed, installed_version, installed_path) =
            check_installed(&entry.id, target_dir);

        let has_update = installed
            && installed_version.is_some()
            && version_newer(&entry.version, installed_version.as_deref().unwrap_or("0"));

        Self {
            id: entry.id.clone(),
            name: entry.name.clone(),
            description: entry.description.clone(),
            version: entry.version.clone(),
            author: entry.author.clone(),
            git_url: entry.git_url.clone(),
            category: entry.category.clone(),
            kind: entry.kind.to_string(),
            tags: entry.tags.clone(),
            homepage: entry.homepage.clone(),
            installed,
            has_update,
            installed_version,
            installed_path,
        }
    }
}

// ============================================================================
// Public API
// ============================================================================

/// Load the default built-in catalog.
pub fn default_catalog() -> MarketplaceCatalog {
    catalog::default_catalog()
}

/// List all marketplace entries with their install status.
///
/// Plugins are looked up in `plugins_dir`, skills in `skills_dir`.
pub fn list_with_status(plugins_dir: &Path, skills_dir: &Path) -> Vec<MarketPluginWithStatus> {
    let catalog = default_catalog();
    catalog
        .plugins
        .iter()
        .map(|entry| MarketPluginWithStatus::from_catalog_entry(entry, plugins_dir, skills_dir))
        .collect()
}

/// Install a marketplace entry by cloning its git repository.
///
/// Clones `git_url` into `<target_dir>/<entry_id>` with `--depth 1`.
/// Returns the path where the entry was installed.
pub async fn install(entry_id: &str, git_url: &str, target_dir: &Path) -> Result<PathBuf> {
    let target = target_dir.join(entry_id);

    if target.exists() {
        return Err(anyhow!(
            "'{}' is already installed at {}",
            entry_id,
            target.display()
        ));
    }

    std::fs::create_dir_all(target_dir).map_err(|e| {
        anyhow!("Failed to create directory: {}", e)
    })?;

    tracing::info!(
        entry_id = %entry_id,
        git_url = %git_url,
        target = %target.display(),
        "cloning marketplace entry"
    );

    let status = tokio::process::Command::new("git")
        .args([
            "clone",
            "--depth",
            "1",
            git_url,
            &target.to_string_lossy().to_string(),
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .status()
        .await
        .map_err(|e| {
            anyhow!(
                "Failed to run git. Is git installed and in PATH? ({})",
                e
            )
        })?;

    if !status.success() {
        let _ = std::fs::remove_dir_all(&target);
        return Err(anyhow!(
            "git clone failed (exit code: {:?}). \
             Check that the repository URL is correct and accessible.",
            status.code()
        ));
    }

    tracing::info!(
        entry_id = %entry_id,
        target = %target.display(),
        "entry installed successfully"
    );

    Ok(target)
}

/// Uninstall a marketplace entry by removing its directory.
pub fn uninstall(entry_id: &str, target_dir: &Path) -> Result<()> {
    let target = target_dir.join(entry_id);

    if !target.exists() {
        return Err(anyhow!("'{}' is not installed", entry_id));
    }

    tracing::info!(
        entry_id = %entry_id,
        path = %target.display(),
        "removing entry directory"
    );

    std::fs::remove_dir_all(&target).map_err(|e| {
        anyhow!("Failed to remove directory: {}", e)
    })?;

    tracing::info!(entry_id = %entry_id, "entry uninstalled successfully");
    Ok(())
}

/// Install a marketplace entry from the catalog by its ID.
///
/// Routes to the correct directory: plugins → `plugins_dir`, skills → `skills_dir`.
pub async fn install_from_catalog(
    entry_id: &str,
    plugins_dir: &Path,
    skills_dir: &Path,
) -> Result<PathBuf> {
    let catalog = default_catalog();
    let entry = catalog
        .plugins
        .iter()
        .find(|p| p.id == entry_id)
        .ok_or_else(|| anyhow!("'{}' not found in catalog", entry_id))?;

    let target_dir = match entry.kind {
        MarketEntryKind::Plugin => plugins_dir,
        MarketEntryKind::Skill => skills_dir,
    };

    install(&entry.id, &entry.git_url, target_dir).await
}

/// Determine the kind of a catalog entry.
pub fn entry_kind(entry_id: &str) -> Option<String> {
    let catalog = default_catalog();
    catalog
        .plugins
        .iter()
        .find(|p| p.id == entry_id)
        .map(|e| e.kind.to_string())
}

/// Update an installed marketplace entry by running `git pull` in its directory.
///
/// Shallow-cloned repos (`--depth 1`) need `git fetch --depth 1` + `git reset`
/// because shallow repos cannot `git pull` directly.
pub async fn update(entry_id: &str, target_dir: &Path) -> Result<()> {
    let target = target_dir.join(entry_id);

    if !target.exists() {
        return Err(anyhow!("'{}' is not installed", entry_id));
    }

    let git_dir = target.join(".git");
    if !git_dir.exists() {
        return Err(anyhow!("'{}' is not a git repository", entry_id));
    }

    tracing::info!(entry_id = %entry_id, target = %target.display(), "updating marketplace entry");

    // Try `git pull` first; fall back to `git fetch` + `git reset` for shallow repos.
    let pull_status = tokio::process::Command::new("git")
        .args(["-C", &target.to_string_lossy().to_string(), "pull", "--ff-only"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .status()
        .await;

    let success = match &pull_status {
        Ok(s) if s.success() => true,
        _ => {
            // Fallback: fetch + reset for shallow repos
            tracing::info!(entry_id = %entry_id, "pull failed, trying fetch+reset for shallow repo");
            let fetch = tokio::process::Command::new("git")
                .args(["-C", &target.to_string_lossy().to_string(), "fetch", "--depth", "1", "origin"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::piped())
                .status()
                .await;

            match fetch {
                Ok(s) if s.success() => {
                    let reset = tokio::process::Command::new("git")
                        .args(["-C", &target.to_string_lossy().to_string(), "reset", "--hard", "origin/HEAD"])
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::piped())
                        .status()
                        .await;

                    match reset {
                        Ok(s) if s.success() => true,
                        Ok(s) => {
                            tracing::warn!(entry_id = %entry_id, exit = ?s.code(), "git reset failed");
                            false
                        }
                        Err(e) => {
                            tracing::warn!(entry_id = %entry_id, error = %e, "git reset error");
                            false
                        }
                    }
                }
                Ok(s) => {
                    tracing::warn!(entry_id = %entry_id, exit = ?s.code(), "git fetch failed");
                    false
                }
                Err(e) => {
                    tracing::warn!(entry_id = %entry_id, error = %e, "git fetch error");
                    false
                }
            }
        }
    };

    if !success {
        return Err(anyhow!(
            "Failed to update '{}'. Check your network connection and git configuration.",
            entry_id
        ));
    }

    tracing::info!(entry_id = %entry_id, "entry updated successfully");
    Ok(())
}

/// Update a marketplace entry by looking up its kind and installed location.
pub async fn update_from_catalog(
    entry_id: &str,
    plugins_dir: &Path,
    skills_dir: &Path,
) -> Result<()> {
    let plugin_path = plugins_dir.join(entry_id);
    let skill_path = skills_dir.join(entry_id);
    let target_dir = if plugin_path.exists() {
        plugins_dir
    } else if skill_path.exists() {
        skills_dir
    } else {
        return Err(anyhow!("'{}' is not installed", entry_id));
    };

    update(entry_id, target_dir).await
}

// ============================================================================
// Helpers
// ============================================================================

/// Check whether an entry is installed locally.
///
/// Returns `(installed, version, path)`.
fn check_installed(entry_id: &str, target_dir: &Path) -> (bool, Option<String>, Option<String>) {
    let dir = target_dir.join(entry_id);
    if !dir.exists() || !dir.is_dir() {
        return (false, None, None);
    }

    let version = try_read_version(&dir);
    let path = Some(dir.to_string_lossy().to_string());
    (true, version, path)
}

/// Try to read an entry's version from its manifest or SKILL.md file.
fn try_read_version(entry_dir: &Path) -> Option<String> {
    // Try plugin.toml first (native format)
    let toml_path = entry_dir.join("plugin.toml");
    if let Ok(content) = std::fs::read_to_string(&toml_path) {
        if let Ok(toml) = content.parse::<toml::Table>() {
            if let Some(version) = toml.get("version").and_then(|v| v.as_str()) {
                return Some(version.to_string());
            }
        }
    }

    // Try .claude-plugin/plugin.json (Claude Code format)
    let cc_path = entry_dir.join(".claude-plugin").join("plugin.json");
    if let Ok(content) = std::fs::read_to_string(&cc_path) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(version) = json.get("version").and_then(|v| v.as_str()) {
                return Some(version.to_string());
            }
        }
    }

    // Try manifest.json (OpenClaw format)
    let mf_path = entry_dir.join("manifest.json");
    if let Ok(content) = std::fs::read_to_string(&mf_path) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(version) = json.get("version").and_then(|v| v.as_str()) {
                return Some(version.to_string());
            }
        }
    }

    None
}

/// Simple semver comparison — returns true if `newer` > `older`.
fn version_newer(newer: &str, older: &str) -> bool {
    let parse = |v: &str| -> Vec<u32> {
        v.split(|c: char| !c.is_ascii_digit())
            .filter_map(|s| s.parse::<u32>().ok())
            .collect()
    };
    let a = parse(newer);
    let b = parse(older);
    for i in 0..a.len().max(b.len()) {
        let av = a.get(i).copied().unwrap_or(0);
        let bv = b.get(i).copied().unwrap_or(0);
        match av.cmp(&bv) {
            std::cmp::Ordering::Greater => return true,
            std::cmp::Ordering::Less => return false,
            std::cmp::Ordering::Equal => continue,
        }
    }
    false
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_catalog_has_entries() {
        let catalog = default_catalog();
        assert!(!catalog.plugins.is_empty(), "default catalog should have entries");
        for p in &catalog.plugins {
            assert!(!p.id.is_empty(), "entry must have an id");
            assert!(!p.name.is_empty(), "entry must have a name");
            assert!(!p.git_url.is_empty(), "entry must have a git_url");
        }
    }

    #[test]
    fn test_check_installed_not_found() {
        let tmp = std::env::temp_dir().join("loom-marketplace-test-nonexistent");
        let (installed, version, path) = check_installed("no-such-entry", &tmp);
        assert!(!installed);
        assert!(version.is_none());
        assert!(path.is_none());
    }

    #[test]
    fn test_list_with_status() {
        let tmp = std::env::temp_dir().join("loom-marketplace-test-list");
        let results = list_with_status(&tmp, &tmp);
        assert!(!results.is_empty());
        for r in &results {
            assert!(!r.installed, "entry should not be installed in temp dir");
            assert!(r.installed_version.is_none());
        }
    }
}
