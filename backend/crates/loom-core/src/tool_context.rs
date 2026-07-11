// SPDX-License-Identifier: Apache-2.0
//! Tool execution context — provides workspace path resolution and sandbox
//! guard for tools.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::event_bus::EventBus;
use loom_memory::TodoStore;
use loom_security::sandbox::SandboxGuard;

/// Context passed to tool execution, containing session-level information
/// such as the workspace path, optional sandbox guard, and read-before-edit
/// tracking.
#[derive(Clone)]
pub struct ToolContext {
    /// Workspace directory for the current session.
    /// Relative paths in file operations will be resolved against this path.
    pub workspace_path: Option<String>,
    /// Optional sandbox guard for path-based access control.
    /// When None, no sandbox checks are performed (backward compatible).
    pub sandbox: Option<Arc<SandboxGuard>>,
    /// Set of file paths that have been recently read, with their read timestamps.
    /// Used to enforce a read-before-edit guard: write/edit tools warn when a
    /// file has not been read within the last 5 minutes.
    /// Wrapped in Arc<Mutex<>> so all clones of the context share the same map.
    pub recently_read: Arc<Mutex<HashMap<PathBuf, Instant>>>,
    /// Session identifier for session-scoped operations (e.g. todo tracking).
    pub session_id: Option<String>,
    /// Shared todo store for session-scoped todo list management.
    pub todo_store: Option<Arc<TodoStore>>,
    /// Event bus for publishing events (e.g. TodosReplaced after todo_write).
    pub event_bus: Option<EventBus>,
    /// Cancel token for the current agent turn — tools should check this during
    /// long operations and return early when cancelled.
    pub cancel_token: Option<tokio_util::sync::CancellationToken>,
}

impl ToolContext {
    /// Create a new empty context with no workspace and no sandbox.
    pub fn new() -> Self {
        Self {
            workspace_path: None,
            sandbox: None,
            recently_read: Arc::new(Mutex::new(HashMap::new())),
            session_id: None,
            todo_store: None,
            event_bus: None,
            cancel_token: None,
        }
    }

    /// Create a context with a workspace path and no sandbox.
    pub fn with_workspace(workspace_path: Option<String>) -> Self {
        Self {
            workspace_path,
            sandbox: None,
            recently_read: Arc::new(Mutex::new(HashMap::new())),
            session_id: None,
            todo_store: None,
            event_bus: None,
            cancel_token: None,
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
            recently_read: Arc::new(Mutex::new(HashMap::new())),
            session_id: None,
            todo_store: None,
            event_bus: None,
            cancel_token: None,
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

    /// Maximum age for a read record to be considered "recent".
    const READ_GRACE_PERIOD: Duration = Duration::from_secs(5 * 60);

    /// Record that a file was successfully read at this moment.
    /// Also cleans up entries older than the grace period to prevent
    /// unbounded growth.
    pub fn record_read(&self, path: PathBuf) {
        if let Ok(mut map) = self.recently_read.lock() {
            let now = Instant::now();
            // Insert the new record
            map.insert(path, now);
            // Clean up stale entries
            map.retain(|_, t| now.duration_since(*t) <= Self::READ_GRACE_PERIOD);
        }
    }

    /// Check whether a path has been recently read (within the grace period).
    pub fn was_recently_read(&self, path: &Path) -> bool {
        if let Ok(map) = self.recently_read.lock() {
            if let Some(t) = map.get(path) {
                return t.elapsed() <= Self::READ_GRACE_PERIOD;
            }
        }
        false
    }
}

impl Default for ToolContext {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ToolContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolContext")
            .field("workspace_path", &self.workspace_path)
            .field("sandbox", &self.sandbox)
            .field(
                "recently_read_count",
                &self.recently_read.lock().map(|m| m.len()).unwrap_or(0),
            )
            .field("session_id", &self.session_id)
            .field("todo_store", &self.todo_store.as_ref().map(|_| "TodoStore"))
            .field("event_bus", &self.event_bus.as_ref().map(|_| "EventBus"))
            .finish()
    }
}
