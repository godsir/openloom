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
    /// Accumulated stdout/stderr lines — never lost between process_wait calls.
    /// This mirrors Claude Code's Monitor persistent buffer: events arriving
    /// while the agent is busy (LLM call, ccl do) are captured here and drained
    /// by the next process_wait, instead of being dropped.
    output_buffer: Arc<Mutex<Vec<String>>>,
    /// Read cursor — index of the next unread line in output_buffer.
    /// Advanced by process_wait; shared across calls so nothing is re-read.
    read_cursor: usize,
}

/// Cap on buffered output lines to bound memory for long-running processes.
const MAX_BUFFERED_LINES: usize = 10_000;

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
        let output_buffer = Arc::new(Mutex::new(Vec::<String>::new()));
        let stdout_buffer = output_buffer.clone();

        if let Some(stdout) = stdout {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    let data = if line.len() > MAX_LINE_BYTES {
                        format!("{}...", &line[..MAX_LINE_BYTES])
                    } else {
                        line
                    };
                    // Buffer the line so process_wait can drain it even if it
                    // arrives while no subscriber is listening (e.g. during an
                    // LLM call between process_wait calls).
                    {
                        let mut buf = stdout_buffer.lock().await;
                        if buf.len() >= MAX_BUFFERED_LINES {
                            buf.remove(0);
                        }
                        buf.push(data.clone());
                    }
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
        let stderr_buffer = output_buffer.clone();

        if let Some(stderr) = stderr {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    let data = if line.len() > MAX_LINE_BYTES {
                        format!("{}...", &line[..MAX_LINE_BYTES])
                    } else {
                        line
                    };
                    {
                        let mut buf = stderr_buffer.lock().await;
                        if buf.len() >= MAX_BUFFERED_LINES {
                            buf.remove(0);
                        }
                        buf.push(data.clone());
                    }
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
                output_buffer,
                read_cursor: 0,
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
                true
            }
        });
    }

    /// Non-blocking status check — returns immediately without waiting.
    pub async fn peek(&self, pid: &str) -> Option<ProcessPeekResult> {
        let procs = self.processes.read().await;
        procs.get(pid).map(|e| ProcessPeekResult {
            pid: e.id.clone(),
            name: e.name.clone(),
            running: e.exit_code.is_none(),
            exit_code: e.exit_code,
        })
    }

    /// Block until a managed process exits, collecting all stdout/stderr output.
    /// Returns (exit_code, output, truncated) — truncated=true if output exceeded max_bytes.
    /// `timeout_secs`: max wait time, 0 = no limit.
    pub async fn wait(
        &self,
        pid: &str,
        timeout_secs: u64,
        max_output_bytes: usize,
        cancel: Option<tokio_util::sync::CancellationToken>,
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

        // Idle-return: once we've received output, if no new output arrives
        // within this window, return the accumulated output immediately.
        // This lets the agent react to interactive processes (e.g. ccl emitting
        // `speech_your_turn` then waiting for input) without blocking the full
        // timeout — the game's speech timer would otherwise expire first.
        let idle_window = Duration::from_millis(300);
        let mut last_output_at: Option<Instant> = None;

        loop {
            // Check cancel token
            if let Some(ref ct) = cancel {
                if ct.is_cancelled() {
                    return Ok(ProcessWaitResult { exit_code: -1, output, truncated: false });
                }
            }
            // Drain buffered output + check exit status in one lock.
            // Draining the buffer here is the KEY fix: events that arrived
            // between process_wait calls (while the agent was doing an LLM
            // call or ccl do) are captured in output_buffer and never lost.
            // This mirrors Claude Code's Monitor persistent-listen behaviour.
            {
                let mut procs = self.processes.write().await;
                if let Some(entry) = procs.get_mut(&pid) {
                    // Drain any unread buffered lines.
                    let new_lines: Vec<String> = {
                        let buf = entry.output_buffer.lock().await;
                        let cursor = entry.read_cursor.min(buf.len());
                        buf[cursor..].to_vec()
                    };
                    if !new_lines.is_empty() {
                        entry.read_cursor += new_lines.len();
                        // Cap read_cursor so it doesn't grow unbounded if the
                        // buffer was trimmed (oldest lines dropped).
                        let buf_len = entry.output_buffer.lock().await.len();
                        entry.read_cursor = entry.read_cursor.min(buf_len);
                        for line in &new_lines {
                            if output.len() + line.len() + 1 > max_output_bytes {
                                if !truncated {
                                    output.push_str("\n[output truncated — max size reached]\n");
                                    truncated = true;
                                }
                                break;
                            }
                            if !output.is_empty() {
                                output.push('\n');
                            }
                            output.push_str(line);
                        }
                        last_output_at = Some(Instant::now());
                    }
                    // Check exit status after draining.
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

            // Idle-return check: if we have output and the process has been
            // quiet for idle_window, it's likely waiting for input — return
            // so the caller (agent) can act on the output.
            if let Some(lo) = last_output_at {
                if lo.elapsed() >= idle_window && !output.is_empty() {
                    return Ok(ProcessWaitResult {
                        exit_code: -1,
                        output,
                        truncated,
                    });
                }
            }

            // Compute the wait duration for this iteration:
            //  - If we have output (idle-return armed), cap at idle_window so we
            //    wake up to check the idle timer even with no events.
            //  - No output yet: cap at no_output_window (3s) so the caller gets
            //    a quick "no new output" result instead of blocking the full
            //    timeout. This lets an agent detect an idle/ended process (e.g.
            //    ClawClaw game over with no more events) and decide to stop,
            //    rather than looping for 30s × N iterations.
            let no_output_window = Duration::from_secs(3);
            let overall_remaining = deadline.map(|d| d.saturating_duration_since(Instant::now()));
            let wait_dur = if last_output_at.is_some() {
                let since = last_output_at.unwrap().elapsed();
                if since >= idle_window {
                    continue; // idle timer already expired — loop back to return above
                }
                let idle_left = idle_window - since;
                if let Some(r) = overall_remaining {
                    r.min(idle_left)
                } else {
                    idle_left
                }
            } else if let Some(r) = overall_remaining {
                if r.is_zero() {
                    return Ok(ProcessWaitResult {
                        exit_code: -1,
                        output: format!("{output}\n[process_wait timed out after {timeout_secs}s]"),
                        truncated: true,
                    });
                }
                r.min(no_output_window)
            } else {
                no_output_window
            };

            // Use select! so cancel is observed immediately even while
            // blocked waiting for the next event (the loop-top cancel check
            // alone isn't enough — we can be stuck in timeout(rx.recv()) for
            // up to wait_dur). This makes chat.stop responsive during games.
            let recv_future = rx.recv();
            tokio::pin!(recv_future);
            let event_result = if let Some(ref ct) = cancel {
                tokio::select! {
                    biased;
                    _ = ct.cancelled() => {
                        return Ok(ProcessWaitResult { exit_code: -1, output, truncated: false });
                    }
                    r = &mut recv_future => match r {
                        Ok(ev) => Some(ev),
                        Err(_) => return Err(anyhow::anyhow!("event bus closed")),
                    },
                }
            } else {
                match tokio::time::timeout(wait_dur, &mut recv_future).await {
                    Ok(Ok(ev)) => Some(ev),
                    Ok(Err(_)) => return Err(anyhow::anyhow!("event bus closed")),
                    Err(_) => None, // timed out — loop back to re-check idle/deadline
                }
            };

            let Some(event) = event_result else {
                // This iteration's wait timed out with no cancel.
                if last_output_at.is_some() {
                    // Had output — loop back, the idle-return check at the top
                    // (output + 300ms quiet) will fire and return the output.
                    continue;
                }
                // No output at all in this call. The no_output_window (3s)
                // elapsed — return now so the caller knows the process is idle
                // (no new output) instead of blocking the full timeout. This
                // lets an agent detect an ended/idle process and decide to stop.
                // If the overall deadline also expired, label it as a timeout.
                let is_overall_timeout = overall_remaining.map_or(false, |r| r.is_zero());
                return Ok(ProcessWaitResult {
                    exit_code: -1,
                    output: if is_overall_timeout {
                        format!("{output}\n[process_wait timed out after {timeout_secs}s]")
                    } else {
                        format!("{output}\n[no new output — process still running]")
                    },
                    truncated: is_overall_timeout,
                });
            };

            match event {
                AgentEvent::ProcessOutput { pid: ev_pid, .. } if ev_pid == pid_for_events => {
                    // The event is just a wake signal — the actual data lives
                    // in output_buffer. Drain it at the top of the next loop
                    // iteration to avoid duplicating this line (which is both
                    // in the event and the buffer). Just mark that output is
                    // fresh so the idle-return timer resets.
                    last_output_at = Some(Instant::now());
                    continue;
                }
                AgentEvent::ProcessExited { pid: ev_pid, exit_code } if ev_pid == pid_for_events => {
                    // Drain any final buffered output before returning.
                    let mut procs = self.processes.write().await;
                    if let Some(entry) = procs.get_mut(&pid) {
                        let new_lines: Vec<String> = {
                            let buf = entry.output_buffer.lock().await;
                            let cursor = entry.read_cursor.min(buf.len());
                            buf[cursor..].to_vec()
                        };
                        entry.read_cursor += new_lines.len();
                        for line in &new_lines {
                            if output.len() + line.len() + 1 > max_output_bytes {
                                if !truncated {
                                    output.push_str("\n[output truncated — max size reached]\n");
                                    truncated = true;
                                }
                                break;
                            }
                            if !output.is_empty() {
                                output.push('\n');
                            }
                            output.push_str(line);
                        }
                    }
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

/// Non-blocking peek — returns process status immediately without waiting.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProcessPeekResult {
    pub pid: String,
    pub name: String,
    pub running: bool,
    pub exit_code: Option<i32>,
}
