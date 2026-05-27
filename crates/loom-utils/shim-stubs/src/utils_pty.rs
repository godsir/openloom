// Stub for codex-utils-pty types.

use std::collections::HashMap;
use std::io;
use std::path::Path;

/// Stub: always returns false on non-Windows (ConPTY is Windows-only).
pub fn conpty_supported() -> bool {
    false
}

/// Stub process group management.
pub mod process_group {
    /// Stub: no-op on non-Unix.
    pub fn kill_process_group(_pid: u32) {}
    /// Stub: no-op on non-Unix.
    pub fn terminate_process_group(_pid: u32) {}
}

/// Stub terminal size.
#[derive(Debug, Clone, Default)]
pub struct TerminalSize;

/// Stub spawned process result returned by spawn functions.
pub struct SpawnedProcess {
    pub session: ExecCommandSession,
    pub stdout_rx: tokio::sync::mpsc::Receiver<Vec<u8>>,
    pub stderr_rx: tokio::sync::mpsc::Receiver<Vec<u8>>,
    pub exit_rx: tokio::sync::oneshot::Receiver<i32>,
}

/// Stub command session.
pub struct ExecCommandSession {
    writer_tx: Option<tokio::sync::mpsc::Sender<Vec<u8>>>,
}

impl Default for ExecCommandSession {
    fn default() -> Self {
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        Self {
            writer_tx: Some(tx),
        }
    }
}

impl ExecCommandSession {
    pub fn writer_sender(&self) -> tokio::sync::mpsc::Sender<Vec<u8>> {
        self.writer_tx.as_ref().cloned().unwrap_or_else(|| {
            let (tx, _rx) = tokio::sync::mpsc::channel(1);
            tx
        })
    }

    pub fn terminate(&self) {}
}

/// Stub process driver.
#[derive(Debug)]
pub struct ProcessDriver {
    pub writer_tx: tokio::sync::mpsc::Sender<Vec<u8>>,
    pub stdout_rx: tokio::sync::broadcast::Receiver<Vec<u8>>,
    pub stderr_rx: Option<tokio::sync::broadcast::Receiver<Vec<u8>>>,
    pub exit_rx: tokio::sync::oneshot::Receiver<i32>,
}

/// Stub: creates an ExecCommandSession from a driver.
pub fn spawn_from_driver(_driver: ProcessDriver) -> ExecCommandSession {
    ExecCommandSession::default()
}

/// Stub: returns an error.
pub async fn spawn_pty_process(
    _program: &str,
    _args: &[String],
    _cwd: &Path,
    _env: &HashMap<String, String>,
    _arg0: &Option<String>,
    _size: TerminalSize,
) -> io::Result<SpawnedProcess> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "PTY not supported in stub",
    ))
}

/// Stub: returns an error.
pub async fn spawn_pipe_process(
    _program: &str,
    _args: &[String],
    _cwd: &Path,
    _env: &HashMap<String, String>,
    _arg0: &Option<String>,
) -> io::Result<SpawnedProcess> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "pipe process not supported in stub",
    ))
}

/// Stub: returns an error.
pub async fn spawn_pipe_process_no_stdin(
    _program: &str,
    _args: &[String],
    _cwd: &Path,
    _env: &HashMap<String, String>,
    _arg0: &Option<String>,
) -> io::Result<SpawnedProcess> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "pipe process not supported in stub",
    ))
}
