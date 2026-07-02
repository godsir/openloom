//! Monitor manager — unified abstraction for long-running background watchers.
//!
//! Supports two event sources:
//! - Shell: delegates to ProcessManager, applies 200ms batching
//! - WebSocket: connects via tokio-tungstenite, each text frame is an event
//!
//! Publishes MonitorStarted / MonitorOutput / MonitorExited / MonitorError
//! via the EventBus. Monitors survive WebSocket disconnects. Output is
//! buffered in a 10,000-line ring buffer.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use tokio::sync::{Mutex, RwLock, mpsc};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::event_bus::{AgentEvent, EventBus};
use crate::process_manager::ProcessManager;

/// Maximum lines in the per-monitor output ring buffer.
const MAX_BUFFERED_LINES: usize = 10_000;

/// Batch window: consecutive lines within this window are merged.
const BATCH_WINDOW_MS: u64 = 200;

/// Idle-return window: if no new output for this long after a batch flush,
/// the agent is woken (via MonitorOutput with an idle marker).
const IDLE_WINDOW_MS: u64 = 300;

/// Rate-limit threshold: max events per second before throttling.
const RATE_LIMIT_PER_SEC: usize = 50;

/// Consecutive seconds of rate-limiting before auto-killing the monitor.
const RATE_LIMIT_CONSECUTIVE_BEFORE_KILL: u32 = 3;

// ── Public types ────────────────────────────────────────────────────────────

/// Configuration for a WebSocket-based monitor source.
#[derive(Debug, Clone)]
pub struct MonitorWsConfig {
    pub url: String,
    pub protocols: Vec<String>,
}

/// Public-facing monitor metadata.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MonitorInfo {
    pub id: String,
    pub name: String,
    pub source: String,
    pub running: bool,
    pub persistent: bool,
    pub exit_code: Option<i32>,
    pub started_at_ms: i64,
}

#[derive(Clone)]
enum MonitorSource {
    Shell {
        /// PID in the ProcessManager.
        pid: String,
    },
    WebSocket {
        url: String,
        protocols: Vec<String>,
    },
}

struct MonitorInstance {
    id: String,
    name: String,
    source: MonitorSource,
    persistent: bool,
    #[allow(dead_code)]
    started_at: Instant,
    started_at_ms: i64,
    exited_at: Option<Instant>,
    exit_code: Option<i32>,

    /// Ring buffer for output lines.
    output_buffer: Arc<Mutex<Vec<String>>>,

    /// Handle to the background batching task.
    #[allow(dead_code)]
    handle: Option<JoinHandle<()>>,

    /// Cancellation token for the batching task.
    cancel_token: CancellationToken,
}

/// Manages all active and recently-exited monitors.
pub struct MonitorManager {
    monitors: Arc<RwLock<HashMap<String, MonitorInstance>>>,
    event_bus: EventBus,
    process_manager: Arc<ProcessManager>,
}

impl MonitorManager {
    pub fn new(event_bus: EventBus, process_manager: Arc<ProcessManager>) -> Self {
        Self {
            monitors: Arc::new(RwLock::new(HashMap::new())),
            event_bus,
            process_manager,
        }
    }

    /// Start a monitor. Returns MonitorInfo immediately — does not block.
    ///
    /// One of `command` or `ws` must be provided.
    pub async fn spawn(
        &self,
        command: Option<&str>,
        ws: Option<MonitorWsConfig>,
        cwd: Option<&str>,
        env: Option<&HashMap<String, String>>,
        description: &str,
        timeout_ms: u64,
        persistent: bool,
        cancel: Option<CancellationToken>,
    ) -> Result<MonitorInfo> {
        let id = uuid::Uuid::now_v7().to_string();
        let now = Instant::now();
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let (source, source_str) = match (command, ws) {
            (Some(cmd), None) => {
                let (pid, _name) = self
                    .process_manager
                    .spawn(cmd, cwd, env, Some(description))
                    .await
                    .context("failed to spawn shell monitor")?;
                (MonitorSource::Shell { pid: pid.clone() }, "shell".to_string())
            }
            (None, Some(ws_cfg)) => {
                (
                    MonitorSource::WebSocket {
                        url: ws_cfg.url.clone(),
                        protocols: ws_cfg.protocols.clone(),
                    },
                    "websocket".to_string(),
                )
            }
            _ => {
                anyhow::bail!("one of 'command' or 'ws' is required");
            }
        };

        let cancel_token = cancel.unwrap_or_else(CancellationToken::new);

        let instance = MonitorInstance {
            id: id.clone(),
            name: description.to_string(),
            source,
            persistent,
            started_at: now,
            started_at_ms: now_ms,
            exited_at: None,
            exit_code: None,
            output_buffer: Arc::new(Mutex::new(Vec::new())),
            handle: None,
            cancel_token: cancel_token.clone(),
        };

        // Publish started event
        self.event_bus.publish(AgentEvent::MonitorStarted {
            monitor_id: id.clone(),
            name: description.to_string(),
            source: source_str.clone(),
            persistent,
            started_at_ms: now_ms,
        });

        let mut monitors = self.monitors.write().await;
        monitors.insert(id.clone(), instance);

        // Spawn background batching task based on source type
        match monitors.get(&id).unwrap().source.clone() {
            MonitorSource::Shell { pid } => {
                self.spawn_shell_batcher(&id, &pid, cancel_token, timeout_ms)
                    .await;
            }
            MonitorSource::WebSocket { url, protocols } => {
                self.spawn_ws_batcher(&id, &url, &protocols, cancel_token, timeout_ms)
                    .await;
            }
        }

        Ok(MonitorInfo {
            id,
            name: description.to_string(),
            source: source_str,
            running: true,
            persistent,
            exit_code: None,
            started_at_ms: now_ms,
        })
    }

    /// Spawn a background task that subscribes to ProcessOutput/ProcessExited
    /// events for the given pid, batches them, and republishes as Monitor events.
    async fn spawn_shell_batcher(
        &self,
        monitor_id: &str,
        pid: &str,
        cancel: CancellationToken,
        timeout_ms: u64,
    ) {
        let event_bus = self.event_bus.clone();
        let mut rx = event_bus.subscribe();
        let mid = monitor_id.to_string();
        let pid_owned = pid.to_string();
        let output_buffer = {
            let monitors = self.monitors.read().await;
            monitors
                .get(&mid)
                .map(|m| m.output_buffer.clone())
                .unwrap_or_default()
        };

        let deadline = if timeout_ms > 0 {
            Some(Instant::now() + Duration::from_millis(timeout_ms))
        } else {
            None
        };

        tokio::spawn(async move {
            let mut batch: Vec<String> = Vec::new();
            let mut last_batch_at: Option<Instant> = None;
            let mut rate_count: usize = 0;
            let mut rate_window_start = Instant::now();
            let mut rate_consecutive: u32 = 0;

            loop {
                // Compute the wait duration for this iteration
                let wait_dur = if batch.is_empty() {
                    // No batch accumulation yet — wait up to 3s for any event
                    // before checking deadline
                    Duration::from_secs(3)
                } else {
                    // Batching — wait BATCH_WINDOW_MS for more lines
                    Duration::from_millis(BATCH_WINDOW_MS)
                };

                tokio::select! {
                    biased;
                    _ = cancel.cancelled() => {
                        if !batch.is_empty() {
                            flush_batch(&event_bus, &mid, &mut batch, &mut last_batch_at, &output_buffer).await;
                        }
                        return;
                    }
                    event = tokio::time::timeout(wait_dur, rx.recv()) => {
                        match event {
                            Ok(Ok(AgentEvent::ProcessOutput { pid: ev_pid, data, stream }))
                                if ev_pid == pid_owned =>
                            {
                                // Rate-limit check
                                rate_count += 1;
                                if rate_window_start.elapsed() >= Duration::from_secs(1) {
                                    rate_count = 1;
                                    rate_window_start = Instant::now();
                                }
                                if rate_count > RATE_LIMIT_PER_SEC {
                                    rate_consecutive += 1;
                                    if rate_consecutive >= RATE_LIMIT_CONSECUTIVE_BEFORE_KILL {
                                        let _ = event_bus.sender().send(AgentEvent::MonitorError {
                                            monitor_id: mid.clone(),
                                            error: "rate-limited: too many events, monitor stopped".into(),
                                        });
                                        return;
                                    }
                                    // Skip this line, emit rate-limit warning once per second
                                    if rate_count == RATE_LIMIT_PER_SEC + 1 {
                                        let _ = event_bus.sender().send(AgentEvent::MonitorOutput {
                                            monitor_id: mid.clone(),
                                            data: "[rate-limited: suppressing excess events]".into(),
                                            stream: stream.clone(),
                                        });
                                        append_to_buffer(&output_buffer, "[rate-limited: suppressing excess events]").await;
                                    }
                                    continue;
                                }
                                rate_consecutive = 0;

                                // Rate-limit check passed — add to batch
                                batch.push(data);
                                if batch.len() >= 50 {
                                    // Hard cap: flush immediately
                                    flush_batch(&event_bus, &mid, &mut batch, &mut last_batch_at, &output_buffer).await;
                                }
                            }
                            Ok(Ok(AgentEvent::ProcessExited { pid: ev_pid, exit_code }))
                                if ev_pid == pid_owned =>
                            {
                                // Drain any remaining batch
                                if !batch.is_empty() {
                                    flush_batch(&event_bus, &mid, &mut batch, &mut last_batch_at, &output_buffer).await;
                                }
                                let _ = event_bus.sender().send(AgentEvent::MonitorExited {
                                    monitor_id: mid.clone(),
                                    exit_code,
                                });
                                return;
                            }
                            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(n))) => {
                                tracing::warn!(skipped = n, monitor_id = %mid, "monitor shell batcher event lag");
                                rx = event_bus.subscribe();
                            }
                            Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => {
                                return;
                            }
                            Err(_) => {
                                // Timeout — flush batch and check idle/deadline
                                if !batch.is_empty() {
                                    flush_batch(&event_bus, &mid, &mut batch, &mut last_batch_at, &output_buffer).await;
                                }
                                // Check idle-return
                                if let Some(last) = last_batch_at {
                                    if last.elapsed() >= Duration::from_millis(IDLE_WINDOW_MS) {
                                        // Idle detected, but monitor still running.
                                        // Publish an idle marker so the agent knows
                                        // the process is waiting for input.
                                        let _ = event_bus.sender().send(AgentEvent::MonitorOutput {
                                            monitor_id: mid.clone(),
                                            data: "[idle — waiting for input]".into(),
                                            stream: "stdout".into(),
                                        });
                                        last_batch_at = None; // only emit idle marker once per idle period
                                    }
                                }
                                // Check overall deadline
                                if let Some(dl) = deadline {
                                    if dl <= Instant::now() {
                                        let _ = event_bus.sender().send(AgentEvent::MonitorError {
                                            monitor_id: mid.clone(),
                                            error: format!("timeout after {}ms", timeout_ms),
                                        });
                                        return;
                                    }
                                }
                            }
                            _ => {} // ignore unrelated events
                        }
                    }
                }
            }
        });
    }

    /// Spawn a background task that connects to a WebSocket endpoint and
    /// publishes each text frame as a MonitorOutput event.
    ///
    /// Uses an mpsc channel bridge to avoid `tokio::select!` over a `Stream` —
    /// a reader task converts WS frames into channel messages consumed by the
    /// main select! loop.
    async fn spawn_ws_batcher(
        &self,
        monitor_id: &str,
        url: &str,
        _protocols: &[String],
        cancel: CancellationToken,
        timeout_ms: u64,
    ) {
        let event_bus = self.event_bus.clone();
        let mid = monitor_id.to_string();
        let ws_url = url.to_string();
        let output_buffer = {
            let monitors = self.monitors.read().await;
            monitors
                .get(&mid)
                .map(|m| m.output_buffer.clone())
                .unwrap_or_default()
        };

        tokio::spawn(async move {
            let deadline = if timeout_ms > 0 {
                Some(Instant::now() + Duration::from_millis(timeout_ms))
            } else {
                None
            };

            // ── Connect ──────────────────────────────────────────────────────
            let connect_result = tokio_tungstenite::connect_async(&ws_url).await;
            let (ws_stream, _response) = match connect_result {
                Ok(s) => s,
                Err(e) => {
                    let _ = event_bus.sender().send(AgentEvent::MonitorError {
                        monitor_id: mid.clone(),
                        error: format!("WebSocket connect failed: {}", e),
                    });
                    return;
                }
            };

            // ── Split WS into read/write halves ─────────────────────────────
            use futures::SinkExt;
            use futures::StreamExt;
            let (mut ws_sink, ws_read) = ws_stream.split();

            // ── mpsc channel bridge: reader task feeds WS frames to channel ──
            let (ws_tx, mut ws_rx) = mpsc::unbounded_channel();
            tokio::spawn(async move {
                futures::pin_mut!(ws_read);
                while let Some(msg_result) = ws_read.next().await {
                    match msg_result {
                        Ok(msg) => {
                            if ws_tx.send(msg).is_err() {
                                break; // receiver dropped — main task exited
                            }
                        }
                        Err(_) => {
                            // Stream error — signal close to main loop
                            let _ = ws_tx.send(tungstenite::Message::Close(None));
                            break;
                        }
                    }
                }
            });

            let mut batch: Vec<String> = Vec::new();
            let mut last_batch_at: Option<Instant> = None;
            #[allow(dead_code)]
            let mut rate_count: usize = 0;
            let mut rate_window_start = Instant::now();
            #[allow(dead_code)]
            let mut rate_consecutive: u32 = 0;

            loop {
                let batch_deadline = if batch.is_empty() {
                    Duration::from_secs(3)
                } else {
                    Duration::from_millis(BATCH_WINDOW_MS)
                };

                tokio::select! {
                    biased;
                    _ = cancel.cancelled() => {
                        if !batch.is_empty() {
                            flush_batch(&event_bus, &mid, &mut batch, &mut last_batch_at, &output_buffer).await;
                        }
                        let _ = ws_sink.send(tungstenite::Message::Close(None)).await;
                        return;
                    }
                    _ = tokio::time::sleep(batch_deadline) => {
                        if !batch.is_empty() {
                            flush_batch(&event_bus, &mid, &mut batch, &mut last_batch_at, &output_buffer).await;
                        }
                        // Check idle
                        if let Some(last) = last_batch_at {
                            if last.elapsed() >= Duration::from_millis(IDLE_WINDOW_MS) {
                                let _ = event_bus.sender().send(AgentEvent::MonitorOutput {
                                    monitor_id: mid.clone(),
                                    data: "[idle — no new frames]".into(),
                                    stream: "ws".into(),
                                });
                                last_batch_at = None;
                            }
                        }
                        // Check deadline
                        if let Some(dl) = deadline {
                            if dl <= Instant::now() {
                                let _ = event_bus.sender().send(AgentEvent::MonitorError {
                                    monitor_id: mid.clone(),
                                    error: format!("timeout after {}ms", timeout_ms),
                                });
                                let _ = ws_sink.send(tungstenite::Message::Close(None)).await;
                                return;
                            }
                        }
                    }
                    msg = ws_rx.recv() => {
                        match msg {
                            Some(tungstenite::Message::Text(text)) => {
                                // Rate-limit check
                                rate_count += 1;
                                if rate_window_start.elapsed() >= Duration::from_secs(1) {
                                    rate_count = 1;
                                    rate_window_start = Instant::now();
                                }
                                if rate_count > RATE_LIMIT_PER_SEC {
                                    rate_consecutive += 1;
                                    if rate_consecutive >= RATE_LIMIT_CONSECUTIVE_BEFORE_KILL {
                                        let _ = event_bus.sender().send(AgentEvent::MonitorError {
                                            monitor_id: mid.clone(),
                                            error: "rate-limited: too many WebSocket frames, monitor stopped".into(),
                                        });
                                        let _ = ws_sink.send(tungstenite::Message::Close(None)).await;
                                        return;
                                    }
                                    if rate_count == RATE_LIMIT_PER_SEC + 1 {
                                        let _ = event_bus.sender().send(AgentEvent::MonitorOutput {
                                            monitor_id: mid.clone(),
                                            data: "[rate-limited: suppressing excess events]".into(),
                                            stream: "ws".into(),
                                        });
                                        append_to_buffer(&output_buffer, "[rate-limited: suppressing excess events]").await;
                                    }
                                    continue;
                                }
                                rate_consecutive = 0;

                                batch.push(text);
                                if batch.len() >= 50 {
                                    flush_batch(&event_bus, &mid, &mut batch, &mut last_batch_at, &output_buffer).await;
                                }
                            }
                            Some(tungstenite::Message::Binary(data)) => {
                                let line = format!("[binary frame, {} bytes]", data.len());
                                batch.push(line);
                                if batch.len() >= 50 {
                                    flush_batch(&event_bus, &mid, &mut batch, &mut last_batch_at, &output_buffer).await;
                                }
                            }
                            Some(tungstenite::Message::Close(_)) => {
                                if !batch.is_empty() {
                                    flush_batch(&event_bus, &mid, &mut batch, &mut last_batch_at, &output_buffer).await;
                                }
                                let _ = event_bus.sender().send(AgentEvent::MonitorExited {
                                    monitor_id: mid.clone(),
                                    exit_code: -1,
                                });
                                return;
                            }
                            Some(tungstenite::Message::Ping(data)) => {
                                let _ = ws_sink.send(tungstenite::Message::Pong(data)).await;
                            }
                            None => {
                                // mpsc channel closed — WS reader task exited
                                if !batch.is_empty() {
                                    flush_batch(&event_bus, &mid, &mut batch, &mut last_batch_at, &output_buffer).await;
                                }
                                let _ = event_bus.sender().send(AgentEvent::MonitorExited {
                                    monitor_id: mid.clone(),
                                    exit_code: -1,
                                });
                                return;
                            }
                            _ => {} // Pong ignored
                        }
                    }
                }
            }
        });
    }

    /// Kill a monitor by ID. Returns true if found and killed.
    pub async fn kill(&self, monitor_id: &str) -> Result<bool> {
        let mut monitors = self.monitors.write().await;
        if let Some(instance) = monitors.get_mut(monitor_id) {
            if instance.exit_code.is_some() {
                return Ok(true); // already exited
            }
            // Cancel the batching task
            instance.cancel_token.cancel();

            // For shell source, also kill the underlying process
            if let MonitorSource::Shell { ref pid } = instance.source {
                let _ = self.process_manager.kill(pid).await;
            }

            instance.exited_at = Some(Instant::now());
            instance.exit_code = Some(-1);

            tracing::info!(%monitor_id, "monitor killed");
            return Ok(true);
        }
        Ok(false)
    }

    /// List all monitors (running and exited).
    pub async fn list(&self) -> Vec<MonitorInfo> {
        let monitors = self.monitors.read().await;
        monitors
            .values()
            .map(|m| MonitorInfo {
                id: m.id.clone(),
                name: m.name.clone(),
                source: match m.source {
                    MonitorSource::Shell { .. } => "shell".into(),
                    MonitorSource::WebSocket { .. } => "websocket".into(),
                },
                running: m.exit_code.is_none(),
                persistent: m.persistent,
                exit_code: m.exit_code,
                started_at_ms: m.started_at_ms,
            })
            .collect()
    }

    /// Remove exited monitors whose exit time is older than `max_age`.
    pub async fn gc(&self, max_age: Duration) {
        let mut monitors = self.monitors.write().await;
        let now = Instant::now();
        monitors.retain(|_id, m| {
            if let Some(exited_at) = m.exited_at {
                now.duration_since(exited_at) < max_age
            } else {
                true // still running
            }
        });
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Flush accumulated lines to the EventBus and output buffer.
async fn flush_batch(
    event_bus: &EventBus,
    monitor_id: &str,
    batch: &mut Vec<String>,
    last_batch_at: &mut Option<Instant>,
    output_buffer: &Arc<Mutex<Vec<String>>>,
) {
    let data = batch.join("\n");
    batch.clear();
    *last_batch_at = Some(Instant::now());

    append_to_buffer(output_buffer, &data).await;

    let _ = event_bus.sender().send(AgentEvent::MonitorOutput {
        monitor_id: monitor_id.to_string(),
        data,
        stream: "stdout".into(),
    });
}

/// Append a line to the ring buffer, trimming if over capacity.
async fn append_to_buffer(buf: &Arc<Mutex<Vec<String>>>, line: &str) {
    let mut b = buf.lock().await;
    if b.len() >= MAX_BUFFERED_LINES {
        b.remove(0);
    }
    b.push(line.to_string());
}
