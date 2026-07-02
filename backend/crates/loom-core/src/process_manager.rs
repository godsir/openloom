//! Background process manager — spawn, monitor, and control long-lived child processes.
//!
//! Processes survive WebSocket disconnects. Stdout/stderr lines are published
//! to the EventBus as AgentEvent::ProcessOutput. Exit is published as ProcessExited.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, RwLock, mpsc};
use tracing::info;
use uuid::Uuid;

use crate::event_bus::{AgentEvent, EventBus};

/// Maximum byte length of a single stdout/stderr line before truncation.
const MAX_LINE_BYTES: usize = 8192;

/// A managed background process.
struct ManagedProcess {
    id: String,
    name: String,
    child: Arc<Mutex<Option<Child>>>,
    stdin_tx: Option<mpsc::UnboundedSender<String>>,
    #[allow(dead_code)]
    started_at: Instant,
    started_at_ms: i64,
    exited_at: Option<Instant>,
    exit_code: Option<i32>,
    /// Buffered output lines for non-blocking process_read.
    output_buffer: Arc<Mutex<Vec<String>>>,
}

/// Manages background child processes that survive WebSocket disconnects.
pub struct ProcessManager {
    processes: Arc<RwLock<HashMap<String, ManagedProcess>>>,
    event_bus: EventBus,
}

impl ProcessManager {
    /// Create a new ProcessManager backed by the given EventBus.
    pub fn new(event_bus: EventBus) -> Self {
        Self {
            processes: Arc::new(RwLock::new(HashMap::new())),
            event_bus,
        }
    }

    /// Spawn a background process. Returns (pid, name) immediately.
    /// Stdout/stderr lines are published as AgentEvent::ProcessOutput.
    pub async fn spawn(
        &self,
        command: &str,
        cwd: Option<&str>,
        env: Option<&HashMap<String, String>>,
        name: Option<&str>,
    ) -> Result<(String, String)> {
        let pid = Uuid::new_v4().to_string();
        let proc_name = name.unwrap_or(command).to_string();

        // Build the command — use shell on all platforms
        let mut cmd = if cfg!(windows) {
            let mut c = Command::new("cmd");
            c.args(["/C", command]);
            c
        } else {
            let mut c = Command::new("sh");
            c.args(["-c", command]);
            c
        };

        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        cmd.stdin(std::process::Stdio::piped());
        cmd.kill_on_drop(true);

        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }
        if let Some(env_vars) = env {
            for (k, v) in env_vars {
                cmd.env(k, v);
            }
        }

        let mut child = cmd
            .spawn()
            .context("failed to spawn process")?;

        // ── stdout reader ──
        let stdout = child.stdout.take();
        let event_bus = self.event_bus.clone();
        let pid_clone = pid.clone();

        if let Some(stdout) = stdout {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    let data = if line.len() > MAX_LINE_BYTES {
                        format!("{}...", &line[..MAX_LINE_BYTES])
                    } else {
                        line
                    };
                    event_bus.publish(AgentEvent::ProcessOutput {
                        pid: pid_clone.clone(),
                        data,
                        stream: "stdout".into(),
                    });
                }
            });
        }

        // ── stderr reader ──
        let stderr = child.stderr.take();
        let event_bus = self.event_bus.clone();
        let pid_clone = pid.clone();

        if let Some(stderr) = stderr {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    let data = if line.len() > MAX_LINE_BYTES {
                        format!("{}...", &line[..MAX_LINE_BYTES])
                    } else {
                        line
                    };
                    event_bus.publish(AgentEvent::ProcessOutput {
                        pid: pid_clone.clone(),
                        data,
                        stream: "stderr".into(),
                    });
                }
            });
        }

        // ── stdin channel ──
        let stdin = child.stdin.take();
        let (stdin_tx, mut stdin_rx) = mpsc::unbounded_channel::<String>();

        if let Some(mut stdin_writer) = stdin {
            tokio::spawn(async move {
                use tokio::io::AsyncWriteExt;
                while let Some(line) = stdin_rx.recv().await {
                    let to_write = if line.ends_with('\n') { line } else { format!("{}\n", line) };
                    if stdin_writer.write_all(to_write.as_bytes()).await.is_err() {
                        break;
                    }
                }
            });
        }

        // Wrap the child in Arc<Mutex<Option<Child>>> so the exit waiter and
        // kill() can both access it.
        let child_arc: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(Some(child)));
        let output_buffer: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

        // ── exit waiter ──
        let processes = self.processes.clone();
        let event_bus = self.event_bus.clone();
        let pid_clone = pid.clone();
        let child_for_wait = child_arc.clone();

        tokio::spawn(async move {
            // Take the child out of the mutex so we can wait on it.
            let owned_child = {
                let mut guard = child_for_wait.lock().await;
                guard.take()
            };
            let code = if let Some(mut c) = owned_child {
                c.wait().await.map(|s| s.code().unwrap_or(-1)).unwrap_or(-1)
            } else {
                -1
            };
            info!(%pid_clone, code, "background process exited");

            event_bus.publish(AgentEvent::ProcessExited {
                pid: pid_clone.clone(),
                exit_code: code,
            });

            // Clean up from the registry (don't remove immediately — keep exit_code)
            let mut procs = processes.write().await;
            if let Some(entry) = procs.get_mut(&pid_clone) {
                entry.exit_code = Some(code);
                entry.exited_at = Some(Instant::now());
            }
        });

        let now = Instant::now();
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        let mut procs = self.processes.write().await;
        procs.insert(
            pid.clone(),
            ManagedProcess {
                id: pid.clone(),
                name: proc_name.clone(),
                child: child_arc,
                stdin_tx: Some(stdin_tx),
                started_at: now,
                started_at_ms: now_ms,
                exited_at: None,
                exit_code: None,
                output_buffer: Arc::new(Mutex::new(Vec::new())),
            },
        );

        info!(%pid, %proc_name, "background process spawned");
        Ok((pid, proc_name))
    }

    /// Kill a managed process by ID. Returns true if the process was found
    /// and killed (or was already exiting), false if the pid is unknown.
    pub async fn kill(&self, pid: &str) -> Result<bool> {
        let mut procs = self.processes.write().await;
        if let Some(entry) = procs.get_mut(pid) {
            // Already exited — nothing to kill.
            if entry.exit_code.is_some() {
                return Ok(true);
            }
            let mut guard = entry.child.lock().await;
            if let Some(ref mut child) = *guard {
                let _ = child.start_kill();
            }
            // Even if the exit waiter already took the child, mark it as killed.
            drop(guard);
            entry.exit_code = Some(-1);
            entry.exited_at = Some(Instant::now());
            info!(%pid, "process killed");
            return Ok(true);
        }
        Ok(false)
    }

    /// Write a line to a process's stdin.
    pub async fn stdin_write(&self, pid: &str, input: &str) -> Result<bool> {
        let procs = self.processes.read().await;
        if let Some(entry) = procs.get(pid) {
            if let Some(ref tx) = entry.stdin_tx {
                if entry.exit_code.is_none() {
                    let _ = tx.send(input.to_string());
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// List all managed processes.
    pub async fn list(&self) -> Vec<ProcessInfo> {
        let procs = self.processes.read().await;
        procs
            .values()
            .map(|e| ProcessInfo {
                pid: e.id.clone(),
                name: e.name.clone(),
                running: e.exit_code.is_none(),
                exit_code: e.exit_code,
                started_at_ms: e.started_at_ms,
            })
            .collect()
    }

    /// Remove exited processes whose exit time is older than `max_age`.
    /// Running processes are never removed.
    pub async fn gc(&self, max_age: Duration) {
        let mut procs = self.processes.write().await;
        let now = Instant::now();
        procs.retain(|_pid, entry| {
            if let Some(exited_at) = entry.exited_at {
                now.duration_since(exited_at) < max_age
            } else {
                true // keep running processes
            }
        });
    }

    /// Block until a managed process exits, collecting all stdout/stderr output.
    /// Returns (exit_code, output, truncated) — truncated=true if output exceeded max_bytes.
    /// `timeout_secs`: max wait time, 0 = no limit.
    pub async fn wait(
        &self,
        pid: &str,
        timeout_secs: u64,
        max_output_bytes: usize,
    ) -> Result<ProcessWaitResult> {
        let pid = pid.to_string();
        let mut output = String::new();
        let mut truncated = false;

        // Subscribe to ProcessOutput and ProcessExited events for this pid
        let mut rx = self.event_bus.subscribe();
        let pid_for_events = pid.clone();

        let start = Instant::now();
        let deadline = if timeout_secs > 0 {
            Some(start + Duration::from_secs(timeout_secs))
        } else {
            None
        };

        loop {
            // Check if already exited
            {
                let procs = self.processes.read().await;
                if let Some(entry) = procs.get(&pid) {
                    if let Some(code) = entry.exit_code {
                        return Ok(ProcessWaitResult {
                            exit_code: code,
                            output,
                            truncated,
                        });
                    }
                } else {
                    // PID not found (already GC'd or never existed)
                    return Ok(ProcessWaitResult {
                        exit_code: -2,
                        output,
                        truncated,
                    });
                }
            }

            // Compute remaining timeout
            let remaining = deadline.map(|d| {
                let r = d.saturating_duration_since(Instant::now());
                r
            });

            let event = if let Some(r) = remaining {
                if r.is_zero() {
                    return Ok(ProcessWaitResult {
                        exit_code: -1,
                        output: format!("{output}\n[process_wait timed out after {timeout_secs}s]"),
                        truncated: true,
                    });
                }
                match tokio::time::timeout(r, rx.recv()).await {
                    Ok(Ok(ev)) => ev,
                    Ok(Err(_)) => return Err(anyhow::anyhow!("event bus closed")),
                    Err(_) => {
                        // timeout
                        return Ok(ProcessWaitResult {
                            exit_code: -1,
                            output: format!("{output}\n[process_wait timed out after {timeout_secs}s]"),
                            truncated: true,
                        });
                    }
                }
            } else {
                match rx.recv().await {
                    Ok(ev) => ev,
                    Err(_) => return Err(anyhow::anyhow!("event bus closed")),
                }
            };

            match event {
                AgentEvent::ProcessOutput { pid: ev_pid, data, .. } if ev_pid == pid_for_events => {
                    if output.len() + data.len() + 1 > max_output_bytes {
                        if !truncated {
                            output.push_str("\n[output truncated — max size reached]\n");
                            truncated = true;
                        }
                    } else {
                        if !output.is_empty() {
                            output.push('\n');
                        }
                        output.push_str(&data);
                    }
                }
                AgentEvent::ProcessExited { pid: ev_pid, exit_code } if ev_pid == pid_for_events => {
                    return Ok(ProcessWaitResult {
                        exit_code,
                        output,
                        truncated,
                    });
                }
                _ => {} // ignore unrelated events
            }
        }
    }
}

/// Result of waiting for a background process to finish.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProcessWaitResult {
    pub exit_code: i32,
    pub output: String,
    pub truncated: bool,
}

/// Public-facing process metadata.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProcessInfo {
    pub pid: String,
    pub name: String,
    pub running: bool,
    pub exit_code: Option<i32>,
    pub started_at_ms: i64,
}

/// Result of a non-blocking process_read.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProcessReadResult {
    pub pid: String,
    pub name: String,
    pub running: bool,
    pub exit_code: Option<i32>,
    /// Lines of output accumulated since the last read for this pid, newest first.
    pub new_lines: Vec<String>,
}
