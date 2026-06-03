// SPDX-License-Identifier: Apache-2.0
//! File-system sandbox — restricts read/write/exec operations to allowed paths.
//!
//! When enabled, every filesystem operation passes through a `SandboxGuard`
//! that canonicalizes the target path and checks it against built-in deny
//! patterns, user-configured deny/allow lists, and an optional workspace
//! boundary.
//!
//! ## Check order (per spec)
//!
//! 1. Resolve `~`, relative → workspace, canonicalize symlinks / `..`
//! 2. Built-in deny patterns (hard-coded, always first)
//! 3. If `workspace_only`: check `is_within_workspace`; allow if inside
//! 4. Check `allowed_paths` (if not already granted by workspace)
//! 5. Check `denied_paths` — user-configured veto overrides all previous grants
//! 6. Default-deny if no grant applies

use std::path::{Component, Path, PathBuf};

use loom_types::config::SandboxConfig;

/// ---------------------------------------------------------------------------
/// Built-in deny patterns — always enforced when the sandbox is enabled.
///
/// Each entry is a human-readable description of the protected resource.
/// The actual matching logic lives in [`check_builtin_deny`].
/// ---------------------------------------------------------------------------
const BUILTIN_DENY_PATTERNS: &[&str] = &[
    "~/.ssh (SSH key directory)",
    "~/.aws (AWS credential directory)",
    ".env files (environment variables)",
    "*.pem / *.key / *.p12 / *.pfx (credential files)",
    "/etc/passwd / /etc/shadow (Unix system auth)",
    "C:\\Windows\\System32\\config\\* (Windows SAM / registry)",
    ".loom/ config directory (sandbox config tamper guard)",
];

// ── Guard ──────────────────────────────────────────────────────────────────

/// Runtime file-system sandbox guard.
///
/// Created once per session / agent with a [`SandboxConfig`] and an optional
/// workspace root. Every proposed file operation must be checked through
/// [`check_read`](SandboxGuard::check_read),
/// [`check_write`](SandboxGuard::check_write), or
/// [`check_exec`](SandboxGuard::check_exec) before reaching the filesystem.
#[derive(Debug, Clone)]
pub struct SandboxGuard {
    config: SandboxConfig,
    workspace: Option<PathBuf>,
    home: PathBuf,
    builtin_deny_patterns: Vec<String>,
}

impl SandboxGuard {
    /// Create a new guard.
    ///
    /// * `config` — user-facing sandbox configuration (may be Default).
    /// * `workspace` — the agent/project workspace root; `None` if not
    ///   applicable (e.g. server-global guard).
    pub fn new(config: SandboxConfig, workspace: Option<PathBuf>) -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
        let builtin_deny_patterns: Vec<String> = BUILTIN_DENY_PATTERNS
            .iter()
            .map(|s| s.to_string())
            .collect();

        Self {
            config,
            workspace,
            home,
            builtin_deny_patterns,
        }
    }

    // ── Public check methods ────────────────────────────────────────────

    /// Check whether reading `path` is permitted.
    ///
    /// Returns `Ok(())` when allowed, `Err(reason)` when denied.
    /// When the sandbox master switch is off (`config.enabled == false`),
    /// this always returns `Ok`.
    pub fn check_read(&self, path: &Path) -> Result<(), String> {
        if !self.config.enabled {
            return Ok(());
        }
        self.check_access(path, "read")
    }

    /// Check whether writing `path` is permitted.
    pub fn check_write(&self, path: &Path) -> Result<(), String> {
        if !self.config.enabled {
            return Ok(());
        }
        self.check_access(path, "write")
    }

    /// Check whether shell/process execution is permitted in `cwd`.
    ///
    /// The sandbox treats the working directory itself as the target — if the
    /// directory is denied, execution is not allowed there.
    pub fn check_exec(&self, cwd: &Path) -> Result<(), String> {
        if !self.config.enabled {
            return Ok(());
        }
        self.check_access(cwd, "exec")
    }

    /// Return the built-in deny pattern descriptions (for display/diagnostics).
    pub fn builtin_deny_patterns(&self) -> &[String] {
        &self.builtin_deny_patterns
    }

    /// Returns `true` when `path` (after canonicalization) is inside the
    /// configured workspace root.
    pub fn is_within_workspace(&self, path: &Path) -> bool {
        let Some(ref ws) = self.workspace else {
            return false;
        };
        // Use canonicalize_safe on both sides so they receive the same
        // normalization (UNC prefix, case, trailing slashes, etc.).
        let ws_canon = self.canonicalize_safe(ws);
        let path_canon = self.canonicalize_safe(path);
        path_canon.starts_with(&ws_canon)
    }

    /// Canonicalize a path safely:
    ///
    /// 1. Expand `~` → home directory.
    /// 2. Resolve relative paths against the workspace root.
    /// 3. Normalize `..` and `.` components.
    /// 4. Canonicalize via `dunce::canonicalize` (which handles Windows UNC
    ///    and long-path quirks better than `std::fs::canonicalize`).
    /// 5. Walk up to the longest existing ancestor when the full path does
    ///    not exist yet (important for write-target checks).
    ///
    /// After canonicalization the caller should still run the deny/allow
    /// checks — this function only resolves the *true* path, it does not
    /// make security decisions.
    pub fn canonicalize_safe(&self, path: &Path) -> PathBuf {
        // --- Step 1: tilde expansion ---
        let expanded = expand_tilde(path, &self.home);

        // --- Step 2: relative → workspace ---
        let resolved = if expanded.is_relative() {
            if let Some(ref ws) = self.workspace {
                ws.join(&expanded)
            } else {
                expanded
            }
        } else {
            expanded
        };

        // --- Step 3: normalize `..` and `.` without touching the fs ---
        let normalized = normalize_path(&resolved);

        // --- Step 4: canonicalize via dunce (handles symlinks, case, UNC) ---
        if let Ok(canon) = dunce::canonicalize(&normalized) {
            return canon;
        }

        // --- Step 5: path (or part of it) does not exist ---
        // Walk up to the longest existing ancestor, canonicalize that
        // ancestor, then re-append the missing suffix.
        let mut ancestor = normalized.clone();
        let mut missing_components: Vec<std::ffi::OsString> = Vec::new();

        while !ancestor.exists() {
            let parent = match ancestor.parent() {
                Some(p) => p.to_path_buf(),
                None => return normalized,
            };
            // capture file_name as owned before reassigning ancestor
            let file_name = ancestor.file_name().map(|n| n.to_os_string());
            ancestor = parent;
            if let Some(comp) = file_name {
                missing_components.push(comp);
            }
        }

        let ancestor_canon = dunce::canonicalize(&ancestor).unwrap_or(ancestor);

        let mut result = ancestor_canon;
        for comp in missing_components.into_iter().rev() {
            result.push(comp);
        }
        result
    }

    // ── Internal helpers ────────────────────────────────────────────────

    /// Core access check, shared by `check_read`, `check_write`, `check_exec`.
    fn check_access(&self, path: &Path, operation: &str) -> Result<(), String> {
        // --- canonicalize (steps 1-3 of the spec) ---
        let canonical = self.canonicalize_safe(path);

        // --- Step 4: built-in deny patterns first ---
        if let Some(reason) = check_builtin_deny(&canonical) {
            return Err(format!(
                "{} denied for '{}': {}",
                operation,
                canonical.display(),
                reason
            ));
        }

        // --- Step 5: workspace check (if workspace_only) ---
        let granted_by_workspace = self.config.workspace_only
            && self.is_within_workspace(&canonical);

        // --- Step 6: allowed_paths (if not already granted by workspace) ---
        let granted_by_allowed = if granted_by_workspace {
            false // already granted; no need to evaluate
        } else {
            self.is_in_allowed_paths(&canonical)
        };

        let any_grant = granted_by_workspace || granted_by_allowed;

        // --- Step 7: user-configured denied_paths (veto) ---
        if self.is_in_denied_paths(&canonical) {
            return Err(format!(
                "{} denied for '{}': path matches sandbox denied_paths",
                operation,
                canonical.display()
            ));
        }

        // --- Final decision ---
        if any_grant {
            Ok(())
        } else if self.config.workspace_only {
            Err(format!(
                "{} denied for '{}': outside workspace and not in allowed_paths",
                operation,
                canonical.display()
            ))
        } else {
            Err(format!(
                "{} denied for '{}': not in allowed_paths",
                operation,
                canonical.display()
            ))
        }
    }

    /// Check whether `path` is covered by any user-configured allowed_path entry.
    fn is_in_allowed_paths(&self, path: &Path) -> bool {
        if self.config.allowed_paths.is_empty() {
            return false;
        }
        let path_str = path.to_string_lossy();
        self.config.allowed_paths.iter().any(|entry| {
            let expanded = expand_tilde(Path::new(entry), &self.home);
            let canon = dunce::canonicalize(&expanded).unwrap_or(expanded);
            let allow_str = canon.to_string_lossy();
            path_str.starts_with(allow_str.as_ref())
        })
    }

    /// Check whether `path` is covered by any user-configured denied_path entry.
    fn is_in_denied_paths(&self, path: &Path) -> bool {
        if self.config.denied_paths.is_empty() {
            return false;
        }
        let path_str = path.to_string_lossy();
        self.config.denied_paths.iter().any(|entry| {
            let expanded = expand_tilde(Path::new(entry), &self.home);
            let canon = dunce::canonicalize(&expanded).unwrap_or(expanded);
            let deny_str = canon.to_string_lossy();
            path_str.starts_with(deny_str.as_ref())
        })
    }
}

// ── Built-in deny logic ────────────────────────────────────────────────────

/// Check `path` against the hard-coded sensitive-resource list.
///
/// Returns `Some(reason)` if the path should be denied, `None` if it passes.
fn check_builtin_deny(path: &Path) -> Option<&'static str> {
    let path_str = path.to_string_lossy();
    let path_lower = path_str.to_lowercase();

    // --- SSH / AWS credential directories (checked as path components) ---
    if has_component_named(path, ".ssh") {
        return Some("SSH key directory (.ssh) is protected");
    }
    if has_component_named(path, ".aws") {
        return Some("AWS credential directory (.aws) is protected");
    }

    // --- .env files ---
    if path.file_name().and_then(|n| n.to_str()) == Some(".env") {
        return Some(".env files are protected");
    }

    // --- Credential file extensions ---
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        match ext.to_lowercase().as_str() {
            "pem" | "key" | "p12" | "pfx" | "crt" | "cert" | "cer" | "der" | "jks" | "keystore" => {
                return Some("credential files (*.pem / *.key / *.p12 / *.pfx / *.crt / *.jks) are protected");
            }
            _ => {}
        }
    }

    // --- Unix system authentication files ---
    if path_str == "/etc/passwd" || path_str == "/etc/shadow" {
        return Some("system authentication files are protected");
    }

    // --- Windows SAM / registry hive (any drive letter) ---
    if path_lower.contains("\\windows\\system32\\config") {
        return Some("Windows system configuration is protected");
    }

    // --- .loom config directory (prevent sandbox config tampering) ---
    if has_component_named(path, ".loom") {
        return Some(".loom configuration directory is protected");
    }

    None
}

/// Returns `true` if any [`Component::Normal`] in `path` matches `name`
/// (case-insensitive on Windows, case-sensitive on Unix).
fn has_component_named(path: &Path, name: &str) -> bool {
    path.components().any(|c| match c {
        Component::Normal(s) => s.eq_ignore_ascii_case(name),
        _ => false,
    })
}

// ── Path utilities ─────────────────────────────────────────────────────────

/// Expand a leading `~` to `home_dir`.
///
/// `~` alone or `~/` maps to `home_dir`; `~user` is NOT supported (Unix
/// feature that requires reading `/etc/passwd`, which we deliberately
/// forbid in the sandbox).
fn expand_tilde(path: &Path, home_dir: &Path) -> PathBuf {
    let path_str = path.to_string_lossy();
    if path_str == "~" {
        return home_dir.to_path_buf();
    }
    // Check for "~/" or "~\" prefix
    if path_str.starts_with("~/") || path_str.starts_with("~\\") {
        let remainder = &path_str[2..];
        return home_dir.join(remainder);
    }
    path.to_path_buf()
}

/// Normalize `.` and `..` components without touching the filesystem.
///
/// This is a pure string-level normalization — it does NOT resolve symlinks
/// or check file existence. Use [`dunce::canonicalize`] for the real deal.
fn normalize_path(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                result.pop();
            }
            Component::CurDir => {
                // skip — adds nothing
            }
            other => {
                result.push(other);
            }
        }
    }
    result
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // canonicalize_safe + tilde expansion + relative resolution
    // ------------------------------------------------------------------

    #[test]
    fn tilde_expands_to_home() {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/home/test"));
        let result = expand_tilde(Path::new("~/projects"), &home);
        assert_eq!(result, home.join("projects"));
    }

    #[test]
    fn normalize_resolves_dotdot() {
        let p = Path::new("/foo/bar/../baz/./qux");
        let n = normalize_path(p);
        assert_eq!(n, PathBuf::from("/foo/baz/qux"));
    }

    #[test]
    fn normalize_empty_path() {
        assert_eq!(normalize_path(Path::new("")), PathBuf::from(""));
    }

    // ------------------------------------------------------------------
    // is_within_workspace
    // ------------------------------------------------------------------

    #[test]
    fn path_inside_workspace() {
        let tmp = std::env::temp_dir();
        let guard = SandboxGuard::new(SandboxConfig::default(), Some(tmp.clone()));
        // The workspace root itself is always inside the workspace.
        assert!(guard.is_within_workspace(&tmp));
    }

    #[test]
    fn path_outside_workspace() {
        let tmp = std::env::temp_dir();
        let guard = SandboxGuard::new(SandboxConfig::default(), Some(tmp.clone()));
        let outside = Path::new("/etc");
        // /etc may not exist on Windows; the method should still work with
        // normalized paths.
        assert!(!guard.is_within_workspace(outside));
    }

    #[test]
    fn is_within_workspace_returns_false_when_no_workspace() {
        let guard = SandboxGuard::new(SandboxConfig::default(), None);
        assert!(!guard.is_within_workspace(Path::new("/tmp/anything")));
    }

    // ------------------------------------------------------------------
    // check_read / check_write — disabled sandbox (enabled: false)
    // ------------------------------------------------------------------

    #[test]
    fn disabled_sandbox_allows_everything() {
        let mut cfg = SandboxConfig::default();
        cfg.enabled = false;
        let guard = SandboxGuard::new(cfg, None);
        assert!(guard.check_read(Path::new("/etc/passwd")).is_ok());
        assert!(guard.check_write(Path::new("/etc/passwd")).is_ok());
        assert!(guard.check_exec(Path::new("/root")).is_ok());
    }

    // ------------------------------------------------------------------
    // Built-in deny patterns
    // ------------------------------------------------------------------

    #[test]
    fn denies_ssh_directory() {
        assert!(check_builtin_deny(Path::new("/home/user/.ssh/id_rsa")).is_some());
        assert!(check_builtin_deny(Path::new("/home/user/.ssh")).is_some());
    }

    #[test]
    fn denies_aws_directory() {
        assert!(check_builtin_deny(Path::new("/home/user/.aws/credentials")).is_some());
    }

    #[test]
    fn denies_dotenv_files() {
        assert!(check_builtin_deny(Path::new("/project/.env")).is_some());
        assert!(check_builtin_deny(Path::new("/project/subdir/.env")).is_some());
    }

    #[test]
    fn denies_credential_extensions() {
        assert!(check_builtin_deny(Path::new("/tmp/secret.pem")).is_some());
        assert!(check_builtin_deny(Path::new("/tmp/secret.key")).is_some());
        assert!(check_builtin_deny(Path::new("/tmp/cert.p12")).is_some());
        assert!(check_builtin_deny(Path::new("/tmp/cert.pfx")).is_some());
        assert!(check_builtin_deny(Path::new("/tmp/cert.crt")).is_some());
        assert!(check_builtin_deny(Path::new("/tmp/cert.cer")).is_some());
        assert!(check_builtin_deny(Path::new("/tmp/keystore.jks")).is_some());
    }

    #[test]
    fn denies_unix_system_auth_files() {
        assert!(check_builtin_deny(Path::new("/etc/passwd")).is_some());
        assert!(check_builtin_deny(Path::new("/etc/shadow")).is_some());
    }

    #[test]
    fn denies_windows_system_config() {
        assert!(check_builtin_deny(Path::new("C:\\Windows\\System32\\config\\SAM")).is_some());
        // Should match any drive letter
        assert!(check_builtin_deny(Path::new("D:\\Windows\\System32\\config\\SAM")).is_some());
    }

    #[test]
    fn denies_loom_config_directory() {
        assert!(check_builtin_deny(Path::new("/project/.loom/sandbox.toml")).is_some());
        assert!(check_builtin_deny(Path::new("/home/user/.loom/config.json")).is_some());
    }

    #[test]
    fn allows_normal_files() {
        assert!(check_builtin_deny(Path::new("/project/src/main.rs")).is_none());
        assert!(check_builtin_deny(Path::new("/tmp/data.csv")).is_none());
        assert!(check_builtin_deny(Path::new("/home/user/projects/readme.md")).is_none());
    }

    // ------------------------------------------------------------------
    // has_component_named
    // ------------------------------------------------------------------

    #[test]
    fn component_match() {
        assert!(has_component_named(Path::new("/a/b/.ssh/c"), ".ssh"));
        assert!(!has_component_named(Path::new("/a/b/ssh_config"), ".ssh"));
        assert!(has_component_named(Path::new(".ssh/id_rsa"), ".ssh"));
    }

    // ------------------------------------------------------------------
    // expand_tilde edge cases
    // ------------------------------------------------------------------

    #[test]
    fn tilde_alone() {
        let home = PathBuf::from("/home/test");
        assert_eq!(expand_tilde(Path::new("~"), &home), home);
    }

    #[test]
    fn no_tilde_passthrough() {
        let p = Path::new("/absolute/path");
        assert_eq!(expand_tilde(p, Path::new("/home/other")), p);
    }

    // ------------------------------------------------------------------
    // normalize_path edge cases
    // ------------------------------------------------------------------

    #[test]
    fn normalize_many_parentdirs() {
        let p = Path::new("/a/b/c/../../../../d");
        let n = normalize_path(p);
        assert_eq!(n, PathBuf::from("/d"));
    }
}
