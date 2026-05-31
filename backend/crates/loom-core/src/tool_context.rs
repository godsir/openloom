// SPDX-License-Identifier: Apache-2.0
//! Tool execution context — provides workspace path resolution for tools.

use std::path::{Path, PathBuf};

/// Context passed to tool execution, containing session-level information
/// such as the workspace path.
#[derive(Debug, Clone)]
pub struct ToolContext {
    /// Workspace directory for the current session.
    /// Relative paths in file operations will be resolved against this path.
    pub workspace_path: Option<String>,
}

impl ToolContext {
    /// Create a new empty context with no workspace.
    pub fn new() -> Self {
        Self {
            workspace_path: None,
        }
    }

    /// Create a context with a workspace path.
    pub fn with_workspace(workspace_path: Option<String>) -> Self {
        Self { workspace_path }
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
