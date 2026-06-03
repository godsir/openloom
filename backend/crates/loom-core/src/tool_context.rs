// SPDX-License-Identifier: Apache-2.0
//! Tool execution context — provides workspace path resolution and sandbox
//! guard for tools.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use loom_security::sandbox::SandboxGuard;

/// Context passed to tool execution, containing session-level information
/// such as the workspace path and optional sandbox guard.
#[derive(Debug, Clone)]
pub struct ToolContext {
    /// Workspace directory for the current session.
    /// Relative paths in file operations will be resolved against this path.
    pub workspace_path: Option<String>,
    /// Optional sandbox guard for path-based access control.
    /// When None, no sandbox checks are performed (backward compatible).
    pub sandbox: Option<Arc<SandboxGuard>>,
}

impl ToolContext {
    /// Create a new empty context with no workspace and no sandbox.
    pub fn new() -> Self {
        Self {
            workspace_path: None,
            sandbox: None,
        }
    }

    /// Create a context with a workspace path and no sandbox.
    pub fn with_workspace(workspace_path: Option<String>) -> Self {
        Self {
            workspace_path,
            sandbox: None,
        }
    }

    /// Create a context with both a workspace path and a sandbox guard.
    pub fn with_workspace_and_sandbox(
        workspace_path: Option<String>,
        sandbox: Option<Arc<SandboxGuard>>,
    ) -> Self {
        Self {
            workspace_path,
            sandbox,
        }
    }

    /// Resolve a path, using workspace as the base for relative paths.
    /// - Absolute paths are returned as-is
    /// - Relative paths are joined with workspace_path (if set)
    /// - If no workspace is set, relative paths are returned as-is (will resolve to process CWD)
    pub fn resolve_path(&self, path: &str) -> PathBuf {
        let p = Path::new(path);
        if p.is_absolute() {
            p.to_path_buf()
        } else if let Some(ref ws) = self.workspace_path {
            Path::new(ws).join(p)
        } else {
            p.to_path_buf()
        }
    }
}

impl Default for ToolContext {
    fn default() -> Self {
        Self::new()
    }
}
