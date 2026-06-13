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

pub use catalog::{MarketEntryKind, MarketPlugin, MarketplaceCatalog};

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
    fn from_catalog_entry(entry: &MarketPlugin, plugins_dir: &Path, skills_dir: &Path) -> Self {
        let target_dir = match entry.kind {
            MarketEntryKind::Plugin => plugins_dir,
            MarketEntryKind::Skill => skills_dir,
        };
        let (installed, installed_version, installed_path) = check_installed(&entry.id, target_dir);

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
    validate_entry_id(entry_id)?;
    validate_git_url(git_url)?;
    let target = target_dir.join(entry_id);

    if target.exists() {
        return Err(anyhow!(
            "'{}' is already installed at {}",
            entry_id,
            target.display()
        ));
    }

    std::fs::create_dir_all(target_dir)
        .map_err(|e| anyhow!("Failed to create directory: {}", e))?;

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
            // `--` terminates option parsing so `git_url`/`target` can never be
            // interpreted as flags even if validation is somehow bypassed.
            "--",
            git_url,
            target.to_string_lossy().as_ref(),
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .status()
        .await
        .map_err(|e| anyhow!("Failed to run git. Is git installed and in PATH? ({})", e))?;

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
    validate_entry_id(entry_id)?;
    let target = target_dir.join(entry_id);

    if !target.exists() {
        return Err(anyhow!("'{}' is not installed", entry_id));
    }

    tracing::info!(
        entry_id = %entry_id,
        path = %target.display(),
        "removing entry directory"
    );

    std::fs::remove_dir_all(&target).map_err(|e| anyhow!("Failed to remove directory: {}", e))?;

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
    validate_entry_id(entry_id)?;
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
        .args(["-C", target.to_string_lossy().as_ref(), "pull", "--ff-only"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .status()
        .await;

    let success = match &pull_status {
        Ok(s) if s.success() => true,
        _ => {
            // Fallback for shallow (`--depth 1`) repos, which cannot `git pull`.
            //
            // `git reset --hard origin/HEAD` is unreliable here: a shallow clone
            // usually leaves `refs/remotes/origin/HEAD` unset, so the reset
            // target does not exist. Instead we detect the remote's default
            // branch, fetch exactly that branch shallowly, and reset to the
            // freshly-written `FETCH_HEAD` (which any successful fetch sets).
            tracing::info!(entry_id = %entry_id, "pull failed, trying fetch+reset for shallow repo");

            let branch = detect_default_branch(&target).await;
            tracing::info!(entry_id = %entry_id, branch = %branch, "resolved default branch for fetch");

            let fetch = tokio::process::Command::new("git")
                .args([
                    "-C",
                    target.to_string_lossy().as_ref(),
                    "fetch",
                    "--depth",
                    "1",
                    "origin",
                    &branch,
                ])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::piped())
                .status()
                .await;

            match fetch {
                Ok(s) if s.success() => {
                    let reset = tokio::process::Command::new("git")
                        .args([
                            "-C",
                            target.to_string_lossy().as_ref(),
                            "reset",
                            "--hard",
                            "FETCH_HEAD",
                        ])
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
    validate_entry_id(entry_id)?;
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
    if validate_entry_id(entry_id).is_err() {
        return (false, None, None);
    }
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
    if let Ok(content) = std::fs::read_to_string(&toml_path)
        && let Ok(toml) = content.parse::<toml::Table>()
        && let Some(version) = toml.get("version").and_then(|v| v.as_str())
    {
        return Some(version.to_string());
    }

    // Try .claude-plugin/plugin.json (Claude Code format)
    let cc_path = entry_dir.join(".claude-plugin").join("plugin.json");
    if let Ok(content) = std::fs::read_to_string(&cc_path)
        && let Ok(json) = serde_json::from_str::<serde_json::Value>(&content)
        && let Some(version) = json.get("version").and_then(|v| v.as_str())
    {
        return Some(version.to_string());
    }

    // Try manifest.json (OpenClaw format)
    let mf_path = entry_dir.join("manifest.json");
    if let Ok(content) = std::fs::read_to_string(&mf_path)
        && let Ok(json) = serde_json::from_str::<serde_json::Value>(&content)
        && let Some(version) = json.get("version").and_then(|v| v.as_str())
    {
        return Some(version.to_string());
    }

    None
}

/// Validate that a marketplace entry id is a safe single path segment so it can
/// never escape `target_dir` — rejects traversal (`..`), path separators,
/// absolute/drive-prefixed paths, leading dots, and any non-`[A-Za-z0-9._-]` char.
fn validate_entry_id(entry_id: &str) -> Result<()> {
    let ok = !entry_id.is_empty()
        && entry_id.len() <= 128
        && !entry_id.starts_with('.')
        && !entry_id.contains("..")
        && entry_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if ok {
        Ok(())
    } else {
        Err(anyhow!("invalid entry id: '{}'", entry_id))
    }
}

/// Validate a git clone URL before passing it to `git clone`.
///
/// Defends against argument injection and unsafe transports:
/// - rejects URLs beginning with `-` (would be parsed by git as a flag),
/// - rejects embedded ASCII control characters / whitespace,
/// - allows only an explicit transport allowlist (`https://`, `http://`,
///   `ssh://`, or scp-like `git@host:path`); everything else — `file://`,
///   `ext::`, `git://`, bare paths, `-c…` — is refused.
///
/// Note: callers MUST still pass a `--` terminator before the URL so a value
/// that slips past this check can never be interpreted as an option.
fn validate_git_url(git_url: &str) -> Result<()> {
    let url = git_url.trim();

    if url.is_empty() {
        return Err(anyhow!("git URL must not be empty"));
    }
    if url.starts_with('-') {
        return Err(anyhow!("git URL must not start with '-': '{git_url}'"));
    }
    if url.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(anyhow!(
            "git URL must not contain whitespace or control characters: '{git_url}'"
        ));
    }

    // Allow only vetted transports. Default posture is https-only, but we also
    // permit http/ssh and the scp-like `user@host:path` form used by GitHub.
    let allowed_scheme = url.starts_with("https://")
        || url.starts_with("http://")
        || url.starts_with("ssh://")
        || is_scp_like(url);

    if !allowed_scheme {
        return Err(anyhow!(
            "unsupported git URL scheme (only https/http/ssh/git@host:path allowed): '{git_url}'"
        ));
    }

    Ok(())
}

/// Detect the scp-like git syntax `user@host:path` (e.g. `git@github.com:org/repo.git`)
/// while excluding URLs that carry an explicit `scheme://` (those are handled by
/// the scheme allowlist) and bare Windows drive paths like `C:\repo`.
fn is_scp_like(url: &str) -> bool {
    // Must contain a ':' that is not part of a "://" scheme separator.
    let Some(colon) = url.find(':') else {
        return false;
    };
    if url[colon..].starts_with("://") {
        return false;
    }
    let user_host = &url[..colon];
    // Require an '@' before the colon (user@host) and a non-empty host.
    match user_host.split_once('@') {
        Some((user, host)) => !user.is_empty() && !host.is_empty(),
        None => false,
    }
}

/// Detect the remote's default branch name for a cloned repo at `repo`.
///
/// Resolution order (each step is best-effort and bounded):
/// 1. Local `refs/remotes/origin/HEAD` symbolic ref (set by full clones).
/// 2. Remote `git ls-remote --symref origin HEAD` (works for shallow clones,
///    requires network — but the caller is about to fetch anyway).
/// 3. Fallback to `main`.
///
/// The returned name is the short branch (e.g. `main`, `master`, `develop`)
/// with any `refs/heads/` or `origin/` prefix stripped.
async fn detect_default_branch(repo: &Path) -> String {
    const FALLBACK: &str = "main";

    // 1. Local symbolic ref: `origin/main` → `main`.
    let local = tokio::process::Command::new("git")
        .args([
            "-C",
            repo.to_string_lossy().as_ref(),
            "symbolic-ref",
            "--short",
            "refs/remotes/origin/HEAD",
        ])
        .stderr(std::process::Stdio::null())
        .output()
        .await;
    if let Ok(out) = local
        && out.status.success()
        && let Ok(text) = String::from_utf8(out.stdout)
    {
        let branch = text.trim().trim_start_matches("origin/").trim();
        if !branch.is_empty() {
            return branch.to_string();
        }
    }

    // 2. Ask the remote. Output line looks like:
    //    `ref: refs/heads/main\tHEAD`
    let remote = tokio::process::Command::new("git")
        .args([
            "-C",
            repo.to_string_lossy().as_ref(),
            "ls-remote",
            "--symref",
            "origin",
            "HEAD",
        ])
        .stderr(std::process::Stdio::null())
        .output()
        .await;
    if let Ok(out) = remote
        && out.status.success()
        && let Ok(text) = String::from_utf8(out.stdout)
    {
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("ref:")
                && let Some(ref_name) = rest.split_whitespace().next()
                && let Some(branch) = ref_name.strip_prefix("refs/heads/")
                && !branch.is_empty()
            {
                return branch.to_string();
            }
        }
    }

    // 3. Give up gracefully.
    FALLBACK.to_string()
}

/// Simple semver comparison — returns true if `newer` > `older`.
///
/// Compares only the dotted numeric *core* of each version. Any pre-release or
/// build metadata (everything from the first `-` or `+`) is stripped before
/// comparison, so `1.0.0` correctly ranks NEWER than `1.0.0-rc2`. When the
/// numeric cores are equal, a version *with* a pre-release suffix is treated as
/// older than one without (per semver ordering: `1.0.0-rc2` < `1.0.0`).
fn version_newer(newer: &str, older: &str) -> bool {
    /// Split a version into its dotted numeric core and whether a pre-release
    /// (`-…`) or build (`+…`) suffix was present.
    fn parse(v: &str) -> (Vec<u32>, bool) {
        let core = v.trim();
        // Take everything before the first `-` (pre-release) or `+` (build).
        let suffix_start = core.find(['-', '+']);
        let (numeric, has_suffix) = match suffix_start {
            Some(idx) => (&core[..idx], true),
            None => (core, false),
        };
        let parts = numeric
            .split('.')
            .filter_map(|s| s.parse::<u32>().ok())
            .collect();
        (parts, has_suffix)
    }

    let (a, a_pre) = parse(newer);
    let (b, b_pre) = parse(older);
    for i in 0..a.len().max(b.len()) {
        let av = a.get(i).copied().unwrap_or(0);
        let bv = b.get(i).copied().unwrap_or(0);
        match av.cmp(&bv) {
            std::cmp::Ordering::Greater => return true,
            std::cmp::Ordering::Less => return false,
            std::cmp::Ordering::Equal => continue,
        }
    }
    // Numeric cores are equal: a release (no pre-release) outranks a pre-release.
    b_pre && !a_pre
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
        assert!(
            !catalog.plugins.is_empty(),
            "default catalog should have entries"
        );
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

    #[test]
    fn test_version_newer_basic() {
        assert!(version_newer("1.0.1", "1.0.0"));
        assert!(version_newer("1.1.0", "1.0.9"));
        assert!(version_newer("2.0.0", "1.9.9"));
        assert!(!version_newer("1.0.0", "1.0.0"));
        assert!(!version_newer("1.0.0", "1.0.1"));
        // Differing component counts: 1.0 == 1.0.0.
        assert!(!version_newer("1.0", "1.0.0"));
        assert!(version_newer("1.0.1", "1.0"));
    }

    #[test]
    fn test_version_newer_prerelease() {
        // A release must outrank its own pre-release / build metadata.
        assert!(
            version_newer("1.0.0", "1.0.0-rc2"),
            "1.0.0 must be newer than 1.0.0-rc2"
        );
        assert!(version_newer("1.0.0", "1.0.0-alpha"));
        assert!(version_newer("1.0.0", "1.0.0-rc.1"));
        assert!(version_newer("1.0.0", "1.0.0+build.5"));

        // The pre-release must NOT be considered newer than the release.
        assert!(
            !version_newer("1.0.0-rc2", "1.0.0"),
            "1.0.0-rc2 must NOT be newer than 1.0.0 (regression: used to parse as [1,0,0,2])"
        );
        assert!(!version_newer("1.0.0-alpha", "1.0.0"));

        // Two identical pre-releases are equal (neither newer).
        assert!(!version_newer("1.0.0-rc2", "1.0.0-rc2"));

        // Numeric core dominates regardless of suffix.
        assert!(version_newer("1.0.1-rc1", "1.0.0"));
        assert!(!version_newer("1.0.0-rc1", "1.0.1"));
    }

    #[test]
    fn test_validate_git_url_accepts_https() {
        assert!(validate_git_url("https://github.com/org/repo").is_ok());
        assert!(validate_git_url("https://github.com/org/repo.git").is_ok());
        assert!(validate_git_url("http://example.com/repo.git").is_ok());
        assert!(validate_git_url("ssh://git@github.com/org/repo.git").is_ok());
        // scp-like form used by GitHub.
        assert!(validate_git_url("git@github.com:org/repo.git").is_ok());
        // Surrounding whitespace is trimmed, not rejected.
        assert!(validate_git_url("  https://github.com/org/repo  ").is_ok());
    }

    #[test]
    fn test_validate_git_url_rejects_flags_and_bad_schemes() {
        // Argument-injection: leading dash would be parsed as a git flag.
        assert!(validate_git_url("-c").is_err());
        assert!(validate_git_url("--upload-pack=touch /tmp/pwned").is_err());
        assert!(validate_git_url("-oProxyCommand=evil").is_err());

        // Unsafe / unsupported transports.
        assert!(validate_git_url("file:///etc/passwd").is_err());
        assert!(validate_git_url("ext::sh -c touch%20/tmp/x").is_err());
        assert!(validate_git_url("git://example.com/repo.git").is_err());

        // Bare paths and drive letters are not valid remotes here.
        assert!(validate_git_url("/local/path/repo").is_err());
        assert!(validate_git_url("C:\\repo").is_err());

        // Empty / whitespace / control chars.
        assert!(validate_git_url("").is_err());
        assert!(validate_git_url("   ").is_err());
        assert!(validate_git_url("https://github.com/org/re\npo").is_err());
        assert!(validate_git_url("https://github.com/org repo").is_err());
    }
}
