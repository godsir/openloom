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
use std::collections::VecDeque;
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

// ── Output ring buffer ──────────────────────────────────────────────────────

/// 输出的环形缓冲（最多保留 `MAX_BUFFERED_LINES` 行，超出从队首丢弃最旧行）。
///
/// 关键设计：消费方用的游标不是"缓冲内下标"，而是**累计已消费行数**
/// （`total_consumed`，见 `MonitorInstance::read_cursor`）。早期实现用绝对
/// 下标做游标，缓冲饱和后每次 `remove(0)` 让所有行下标前移、游标却停在旧值，
/// 导致 `buf[cursor..]` 恒为空——**饱和后永久读不到新输出**。改用累计序号后，
/// 丢弃旧行不再影响游标有效性：`drain_from` 把 `total_consumed` 映射回当前
/// 缓冲窗口内的实际位置，落后的行按"已丢弃"处理。
struct OutputRing {
    lines: VecDeque<String>,
    /// 累计追加过的行数（单调递增）。
    total_appended: usize,
}

impl OutputRing {
    fn new() -> Self {
        Self {
            lines: VecDeque::new(),
            total_appended: 0,
        }
    }

    /// 追加一行；超过容量时丢弃最旧行。
    fn push(&mut self, line: String) {
        if self.lines.len() >= MAX_BUFFERED_LINES {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
        self.total_appended += 1;
    }

    /// 读取自 `consumed`（累计已消费数）之后的新行。
    ///
    /// 返回 `(新行, 是否发生丢弃, 推进后的 consumed)`：
    /// - 现存行的序号区间为 `[total_appended - lines.len(), total_appended)`；
    /// - 若 `consumed` 早于区间下界，说明落后的行已被环形缓冲丢弃，将其钳制到
    ///   下界并置 `dropped=true`；
    /// - 推进后的 `consumed` 恒等于 `total_appended`（读完当前所有可得行）。
    fn drain_from(&self, consumed: usize) -> (Vec<String>, bool, usize) {
        let oldest_available = self.total_appended - self.lines.len();
        let mut consumed = consumed;
        let mut dropped = false;
        if consumed < oldest_available {
            consumed = oldest_available;
            dropped = true;
        }
        let start = consumed - oldest_available; // 缓冲窗口内起始下标
        let new_lines = self.lines.iter().skip(start).cloned().collect();
        (new_lines, dropped, self.total_appended)
    }
}

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
    pub session_id: String,
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
    output_buffer: Arc<Mutex<OutputRing>>,
    /// Read cursor — **累计已消费行数**（total_consumed，非缓冲内下标）。
    /// 由 `OutputRing::drain_from` 映射回当前缓冲窗口位置，饱和丢弃旧行时不失效。
    read_cursor: usize,

    /// Session this monitor belongs to.
    session_id: String,

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
        session_id: &str,
        cancel: Option<CancellationToken>,
    ) -> Result<MonitorInfo> {
        let id = uuid::Uuid::now_v7().to_string();
        let now = Instant::now();
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let output_buffer = Arc::new(Mutex::new(OutputRing::new()));
        let cancel_token = cancel.unwrap_or_default();

        let (source, source_str) = match (command, ws) {
            (Some(cmd), None) => {
                let (pid, _name) = self
                    .process_manager
                    .spawn(cmd, cwd, env, Some(description), session_id)
                    .await
                    .context("failed to spawn shell monitor")?;
                (
                    MonitorSource::Shell { pid: pid.clone() },
                    "shell".to_string(),
                )
            }
            (None, Some(ws_cfg)) => (
                MonitorSource::WebSocket {
                    url: ws_cfg.url.clone(),
                    protocols: ws_cfg.protocols.clone(),
                },
                "websocket".to_string(),
            ),
            _ => {
                anyhow::bail!("one of 'command' or 'ws' is required");
            }
        };

        let instance = MonitorInstance {
            id: id.clone(),
            name: description.to_string(),
            source: source.clone(),
            persistent,
            started_at: now,
            started_at_ms: now_ms,
            exited_at: None,
            exit_code: None,
            output_buffer: output_buffer.clone(),
            read_cursor: 0,
            session_id: session_id.to_string(),
            handle: None,
            cancel_token: cancel_token.clone(),
        };

        // Publish started event (before inserting so observers see MonitorStarted first).
        self.event_bus.publish(AgentEvent::MonitorStarted {
            monitor_id: id.clone(),
            name: description.to_string(),
            source: source_str.clone(),
            persistent,
            started_at_ms: now_ms,
            session_id: session_id.to_string(),
        });

        // Fix 1: insert instance and drop the write lock immediately, so the
        // batcher functions (which need to read/update monitors) do not deadlock.
        {
            let mut monitors = self.monitors.write().await;
            monitors.insert(id.clone(), instance);
        }

        // Spawn background batching task — called OUTSIDE the lock.
        match source {
            MonitorSource::Shell { pid } => {
                self.spawn_shell_batcher(
                    &id,
                    &pid,
                    cancel_token,
                    timeout_ms,
                    output_buffer,
                    self.monitors.clone(),
                    session_id.to_string(),
                )
                .await;
            }
            MonitorSource::WebSocket { url, protocols } => {
                self.spawn_ws_batcher(
                    &id,
                    &url,
                    &protocols,
                    cancel_token,
                    timeout_ms,
                    output_buffer,
                    self.monitors.clone(),
                    session_id.to_string(),
                )
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
            session_id: session_id.to_string(),
        })
    }

    /// Spawn a background task that subscribes to ProcessOutput/ProcessExited
    /// events for the given pid, batches them, and republishes as Monitor events.
    ///
    /// Fix 1: now accepts `output_buffer` and `monitors` as parameters instead
    /// of reading them from `self.monitors` (which would deadlock).
    /// Fix 2: uses `mark_monitor_exited()` on every exit path.
    /// Fix 6: holds `process_manager` to kill the process on cancel/timeout.
    async fn spawn_shell_batcher(
        &self,
        monitor_id: &str,
        pid: &str,
        cancel: CancellationToken,
        timeout_ms: u64,
        output_buffer: Arc<Mutex<OutputRing>>,
        monitors: Arc<RwLock<HashMap<String, MonitorInstance>>>,
        session_id: String,
    ) {
        let event_bus = self.event_bus.clone();
        let process_manager = self.process_manager.clone();
        let mid = monitor_id.to_string();
        let pid_owned = pid.to_string();
        let sid = session_id.clone();

        let deadline = if timeout_ms > 0 {
            Some(Instant::now() + Duration::from_millis(timeout_ms))
        } else {
            None
        };

        tokio::spawn(async move {
            let mut rx = event_bus.subscribe();
            let mut batch: Vec<String> = Vec::new();
            let mut last_batch_at: Option<Instant> = None;
            // Fix 3: rate-limit now uses window-based counting instead of
            // per-event counting. `rate_consecutive` increments once per
            // rate-limited window (second), not per event.
            let mut rate_count: usize = 0;
            let mut rate_window_start = Instant::now();
            let mut rate_consecutive: u32 = 0;
            let mut window_was_rate_limited = false;

            loop {
                let wait_dur = if batch.is_empty() {
                    Duration::from_secs(3)
                } else {
                    Duration::from_millis(BATCH_WINDOW_MS)
                };

                tokio::select! {
                    biased;
                    _ = cancel.cancelled() => {
                        // Fix 6: kill the underlying process so non-persistent
                        // monitors don't leave orphan processes.
                        let _ = process_manager.kill(&pid_owned).await;
                        if !batch.is_empty() {
                            flush_batch(&event_bus, &mid, &mut batch, &mut last_batch_at, &output_buffer, "stdout", &sid).await;
                        }
                        // Fix 4: send MonitorExited so observers know the monitor stopped.
                        let _ = event_bus.sender().send(AgentEvent::MonitorExited {
                            monitor_id: mid.clone(),
                            exit_code: -1,
                            session_id: sid.clone(),
                        });
                        // Fix 2: update MonitorInstance so list() / gc() see correct state.
                        mark_monitor_exited(&monitors, &mid, -1).await;
                        return;
                    }
                    event = tokio::time::timeout(wait_dur, rx.recv()) => {
                        match event {
                            Ok(Ok(AgentEvent::ProcessOutput { pid: ev_pid, data, stream, .. }))
                                if ev_pid == pid_owned =>
                            {
                                // Fix 3: window-based rate-limit counting.
                                // Only increment rate_consecutive at window boundaries.
                                rate_count += 1;
                                if rate_window_start.elapsed() >= Duration::from_secs(1) {
                                    if window_was_rate_limited {
                                        rate_consecutive += 1;
                                    } else {
                                        rate_consecutive = 0;
                                    }
                                    rate_count = 1;
                                    rate_window_start = Instant::now();
                                    window_was_rate_limited = false;
                                }
                                if rate_count > RATE_LIMIT_PER_SEC {
                                    window_was_rate_limited = true;
                                    if rate_consecutive >= RATE_LIMIT_CONSECUTIVE_BEFORE_KILL {
                                        let _ = process_manager.kill(&pid_owned).await;
                                        let _ = event_bus.sender().send(AgentEvent::MonitorError {
                                            monitor_id: mid.clone(),
                                            error: "rate-limited: too many events, monitor stopped".into(),
                                            session_id: sid.clone(),
                                        });
                                        mark_monitor_exited(&monitors, &mid, -1).await;
                                        return;
                                    }
                                    if rate_count == RATE_LIMIT_PER_SEC + 1 {
                                        let _ = event_bus.sender().send(AgentEvent::MonitorOutput {
                                            monitor_id: mid.clone(),
                                            data: "[rate-limited: suppressing excess events]".into(),
                                            stream: stream.clone(),
                                            session_id: sid.clone(),
                                        });
                                        append_to_buffer(&output_buffer, "[rate-limited: suppressing excess events]").await;
                                    }
                                    continue;
                                }

                                batch.push(data);
                                if batch.len() >= 50 {
                                    flush_batch(&event_bus, &mid, &mut batch, &mut last_batch_at, &output_buffer, "stdout", &sid).await;
                                }
                            }
                            Ok(Ok(AgentEvent::ProcessExited { pid: ev_pid, exit_code, .. }))
                                if ev_pid == pid_owned =>
                            {
                                if !batch.is_empty() {
                                    flush_batch(&event_bus, &mid, &mut batch, &mut last_batch_at, &output_buffer, "stdout", &sid).await;
                                }
                                let _ = event_bus.sender().send(AgentEvent::MonitorExited {
                                    monitor_id: mid.clone(),
                                    exit_code,
                                    session_id: sid.clone(),
                                });
                                // Fix 2: update MonitorInstance on normal exit.
                                mark_monitor_exited(&monitors, &mid, exit_code).await;
                                return;
                            }
                            Ok(Ok(_)) => {} // ignore unrelated events
                            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(n))) => {
                                tracing::warn!(skipped = n, monitor_id = %mid, "monitor shell batcher event lag");
                                // Fix 7: before re-subscribing, peek the process
                                // to check if we missed ProcessExited.
                                if let Some(peek) = process_manager.peek(&pid_owned).await
                                    && !peek.running
                                {
                                    if !batch.is_empty() {
                                        flush_batch(&event_bus, &mid, &mut batch, &mut last_batch_at, &output_buffer, "stdout", &sid).await;
                                    }
                                    let ec = peek.exit_code.unwrap_or(-1);
                                    let _ = event_bus.sender().send(AgentEvent::MonitorExited {
                                        monitor_id: mid.clone(),
                                        exit_code: ec,
                                        session_id: sid.clone(),
                                    });
                                    mark_monitor_exited(&monitors, &mid, ec).await;
                                    return;
                                }
                                rx = event_bus.subscribe();
                            }
                            Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => {
                                mark_monitor_exited(&monitors, &mid, -1).await;
                                return;
                            }
                            Err(_) => {
                                // Timeout — flush batch and check idle/deadline.
                                if !batch.is_empty() {
                                    flush_batch(&event_bus, &mid, &mut batch, &mut last_batch_at, &output_buffer, "stdout", &sid).await;
                                }
                                // Fix 3: also reset rate-limit window on timeout
                                // so the counter doesn't stay stale during idle.
                                if rate_window_start.elapsed() >= Duration::from_secs(1) {
                                    if window_was_rate_limited {
                                        rate_consecutive += 1;
                                    } else {
                                        rate_consecutive = 0;
                                    }
                                    rate_count = 0;
                                    rate_window_start = Instant::now();
                                    window_was_rate_limited = false;
                                }
                                // Idle marker
                                if let Some(last) = last_batch_at
                                    && last.elapsed() >= Duration::from_millis(IDLE_WINDOW_MS)
                                {
                                    let _ = event_bus.sender().send(AgentEvent::MonitorOutput {
                                        monitor_id: mid.clone(),
                                        data: "[idle — waiting for input]".into(),
                                        stream: "stdout".into(),
                                        session_id: sid.clone(),
                                    });
                                    last_batch_at = None;
                                }
                                // Fix 5: on timeout, kill the process and send
                                // MonitorExited instead of just MonitorError.
                                if let Some(dl) = deadline
                                    && dl <= Instant::now()
                                {
                                    let _ = process_manager.kill(&pid_owned).await;
                                    let _ = event_bus.sender().send(AgentEvent::MonitorExited {
                                        monitor_id: mid.clone(),
                                        exit_code: -1,
                                        session_id: sid.clone(),
                                    });
                                    mark_monitor_exited(&monitors, &mid, -1).await;
                                    return;
                                }
                            }
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
    ///
    /// Fix 1: now accepts `output_buffer` and `monitors` as parameters.
    /// Fix 9: passes protocols via Sec-WebSocket-Protocol header.
    async fn spawn_ws_batcher(
        &self,
        monitor_id: &str,
        url: &str,
        protocols: &[String],
        cancel: CancellationToken,
        timeout_ms: u64,
        output_buffer: Arc<Mutex<OutputRing>>,
        monitors: Arc<RwLock<HashMap<String, MonitorInstance>>>,
        session_id: String,
    ) {
        let event_bus = self.event_bus.clone();
        let mid = monitor_id.to_string();
        let ws_url = url.to_string();
        let protocols_owned = protocols.to_vec();
        let sid = session_id.clone();

        tokio::spawn(async move {
            let deadline = if timeout_ms > 0 {
                Some(Instant::now() + Duration::from_millis(timeout_ms))
            } else {
                None
            };

            // ── Connect ──────────────────────────────────────────────────────
            // Fix 9: pass protocols via Sec-WebSocket-Protocol header when non-empty.
            let connect_result = if protocols_owned.is_empty() {
                tokio_tungstenite::connect_async(&ws_url).await
            } else {
                use tokio_tungstenite::tungstenite::http::Request;
                let request = match Request::builder()
                    .uri(&ws_url)
                    .header("Sec-WebSocket-Protocol", protocols_owned.join(", "))
                    .body(())
                {
                    Ok(req) => req,
                    Err(e) => {
                        let _ = event_bus.sender().send(AgentEvent::MonitorError {
                            monitor_id: mid.clone(),
                            error: format!("WebSocket request build failed: {}", e),
                            session_id: sid.clone(),
                        });
                        mark_monitor_exited(&monitors, &mid, -1).await;
                        return;
                    }
                };
                tokio_tungstenite::connect_async(request).await
            };

            let (ws_stream, _response) = match connect_result {
                Ok(s) => s,
                Err(e) => {
                    let _ = event_bus.sender().send(AgentEvent::MonitorError {
                        monitor_id: mid.clone(),
                        error: format!("WebSocket connect failed: {}", e),
                        session_id: sid.clone(),
                    });
                    mark_monitor_exited(&monitors, &mid, -1).await;
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
                                break;
                            }
                        }
                        Err(_) => {
                            let _ = ws_tx.send(tungstenite::Message::Close(None));
                            break;
                        }
                    }
                }
            });

            let mut batch: Vec<String> = Vec::new();
            let mut last_batch_at: Option<Instant> = None;
            // Fix 3: window-based rate-limit counting.
            let mut rate_count: usize = 0;
            let mut rate_window_start = Instant::now();
            let mut rate_consecutive: u32 = 0;
            let mut window_was_rate_limited = false;

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
                            // Fix 8: use "ws" stream for WS batcher.
                            flush_batch(&event_bus, &mid, &mut batch, &mut last_batch_at, &output_buffer, "ws", &sid).await;
                        }
                        let _ = ws_sink.send(tungstenite::Message::Close(None)).await;
                        // Fix 4: send MonitorExited on cancel.
                        let _ = event_bus.sender().send(AgentEvent::MonitorExited {
                            monitor_id: mid.clone(),
                            exit_code: -1,
                            session_id: sid.clone(),
                        });
                        // Fix 2: update MonitorInstance.
                        mark_monitor_exited(&monitors, &mid, -1).await;
                        return;
                    }
                    _ = tokio::time::sleep(batch_deadline) => {
                        if !batch.is_empty() {
                            flush_batch(&event_bus, &mid, &mut batch, &mut last_batch_at, &output_buffer, "ws", &sid).await;
                        }
                        // Fix 3: rate-limit window reset in timeout path.
                        if rate_window_start.elapsed() >= Duration::from_secs(1) {
                            if window_was_rate_limited {
                                rate_consecutive += 1;
                            } else {
                                rate_consecutive = 0;
                            }
                            rate_count = 0;
                            rate_window_start = Instant::now();
                            window_was_rate_limited = false;
                        }
                        // Idle marker
                        if let Some(last) = last_batch_at
                            && last.elapsed() >= Duration::from_millis(IDLE_WINDOW_MS)
                        {
                            let _ = event_bus.sender().send(AgentEvent::MonitorOutput {
                                monitor_id: mid.clone(),
                                data: "[idle — no new frames]".into(),
                                stream: "ws".into(),
                                session_id: sid.clone(),
                            });
                            last_batch_at = None;
                        }
                        // Fix 5: send MonitorExited on timeout instead of just MonitorError.
                        if let Some(dl) = deadline
                            && dl <= Instant::now()
                        {
                            let _ = ws_sink.send(tungstenite::Message::Close(None)).await;
                            let _ = event_bus.sender().send(AgentEvent::MonitorExited {
                                monitor_id: mid.clone(),
                                exit_code: -1,
                                session_id: sid.clone(),
                            });
                            mark_monitor_exited(&monitors, &mid, -1).await;
                            return;
                        }
                    }
                    msg = ws_rx.recv() => {
                        match msg {
                            Some(tungstenite::Message::Text(text)) => {
                                // Fix 3: window-based rate-limit counting.
                                rate_count += 1;
                                if rate_window_start.elapsed() >= Duration::from_secs(1) {
                                    if window_was_rate_limited {
                                        rate_consecutive += 1;
                                    } else {
                                        rate_consecutive = 0;
                                    }
                                    rate_count = 1;
                                    rate_window_start = Instant::now();
                                    window_was_rate_limited = false;
                                }
                                if rate_count > RATE_LIMIT_PER_SEC {
                                    window_was_rate_limited = true;
                                    if rate_consecutive >= RATE_LIMIT_CONSECUTIVE_BEFORE_KILL {
                                        let _ = event_bus.sender().send(AgentEvent::MonitorError {
                                            monitor_id: mid.clone(),
                                            error: "rate-limited: too many WebSocket frames, monitor stopped".into(),
                                            session_id: sid.clone(),
                                        });
                                        let _ = ws_sink.send(tungstenite::Message::Close(None)).await;
                                        mark_monitor_exited(&monitors, &mid, -1).await;
                                        return;
                                    }
                                    if rate_count == RATE_LIMIT_PER_SEC + 1 {
                                        let _ = event_bus.sender().send(AgentEvent::MonitorOutput {
                                            monitor_id: mid.clone(),
                                            data: "[rate-limited: suppressing excess events]".into(),
                                            stream: "ws".into(),
                                            session_id: sid.clone(),
                                        });
                                        append_to_buffer(&output_buffer, "[rate-limited: suppressing excess events]").await;
                                    }
                                    continue;
                                }

                                batch.push(text);
                                if batch.len() >= 50 {
                                    flush_batch(&event_bus, &mid, &mut batch, &mut last_batch_at, &output_buffer, "ws", &sid).await;
                                }
                            }
                            Some(tungstenite::Message::Binary(data)) => {
                                let line = format!("[binary frame, {} bytes]", data.len());
                                batch.push(line);
                                if batch.len() >= 50 {
                                    flush_batch(&event_bus, &mid, &mut batch, &mut last_batch_at, &output_buffer, "ws", &sid).await;
                                }
                            }
                            Some(tungstenite::Message::Close(_)) => {
                                if !batch.is_empty() {
                                    flush_batch(&event_bus, &mid, &mut batch, &mut last_batch_at, &output_buffer, "ws", &sid).await;
                                }
                                let _ = event_bus.sender().send(AgentEvent::MonitorExited {
                                    monitor_id: mid.clone(),
                                    exit_code: -1,
                                    session_id: sid.clone(),
                                });
                                // Fix 2: update MonitorInstance.
                                mark_monitor_exited(&monitors, &mid, -1).await;
                                return;
                            }
                            Some(tungstenite::Message::Ping(data)) => {
                                let _ = ws_sink.send(tungstenite::Message::Pong(data)).await;
                            }
                            None => {
                                // mpsc channel closed — WS reader task exited.
                                if !batch.is_empty() {
                                    flush_batch(&event_bus, &mid, &mut batch, &mut last_batch_at, &output_buffer, "ws", &sid).await;
                                }
                                let _ = event_bus.sender().send(AgentEvent::MonitorExited {
                                    monitor_id: mid.clone(),
                                    exit_code: -1,
                                    session_id: sid.clone(),
                                });
                                // Fix 2: update MonitorInstance.
                                mark_monitor_exited(&monitors, &mid, -1).await;
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
                session_id: m.session_id.clone(),
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

    /// Non-blocking status check — returns immediately without waiting.
    /// Includes a snapshot of the current buffered output so the agent can
    /// decide whether to call monitor_wait for more.
    pub async fn peek(&self, monitor_id: &str) -> Option<MonitorPeekResult> {
        let monitors = self.monitors.read().await;
        let m = monitors.get(monitor_id)?;
        // Snapshot metadata under the read lock, then drop it before acquiring
        // the output_buffer lock so we don't deadlock (output_buffer writers
        // don't hold the monitors lock).
        let output_buffer = m.output_buffer.clone();
        let read_cursor = m.read_cursor;
        let id = m.id.clone();
        let name = m.name.clone();
        let running = m.exit_code.is_none();
        let exit_code = m.exit_code;
        drop(monitors);

        let output = {
            let ring = output_buffer.lock().await;
            // peek 只展示未读行、不推进游标（drain_from 的推进值此处丢弃）。
            let (new_lines, _dropped, _consumed) = ring.drain_from(read_cursor);
            new_lines.join("\n")
        };

        Some(MonitorPeekResult {
            monitor_id: id,
            name,
            running,
            exit_code,
            output,
        })
    }

    /// Block until a monitor exits, collecting all buffered output.
    /// Returns (exit_code, output, truncated, running) — truncated=true if output exceeded max_bytes.
    /// `timeout_secs`: max wait time, 0 = no limit.
    /// `cancel`: optional cancellation token for user interruption.
    pub async fn wait(
        &self,
        monitor_id: &str,
        timeout_secs: u64,
        max_output_bytes: usize,
        cancel: Option<tokio_util::sync::CancellationToken>,
    ) -> Result<MonitorWaitResult> {
        let mid = monitor_id.to_string();
        let mut output = String::new();
        let mut truncated = false;

        // Subscribe to MonitorOutput and MonitorExited events for this monitor
        let mut rx = self.event_bus.subscribe();

        let start = Instant::now();
        let deadline = if timeout_secs > 0 {
            Some(start + Duration::from_secs(timeout_secs))
        } else {
            None
        };

        let idle_window = Duration::from_millis(300);
        let mut last_output_at: Option<Instant> = None;

        // Drain any existing buffered output first
        {
            let mut monitors = self.monitors.write().await;
            if let Some(entry) = monitors.get_mut(&mid) {
                let cur = entry.read_cursor;
                let (new_lines, dropped, new_consumed) = {
                    let ring = entry.output_buffer.lock().await;
                    ring.drain_from(cur)
                };
                entry.read_cursor = new_consumed;
                if append_drained(new_lines, dropped, &mut output, max_output_bytes, &mut truncated) {
                    last_output_at = Some(Instant::now());
                }
                // Check exit status after draining
                if let Some(code) = entry.exit_code {
                    return Ok(MonitorWaitResult {
                        exit_code: code,
                        output,
                        truncated,
                        running: false,
                    });
                }
            } else {
                // Monitor not found
                return Ok(MonitorWaitResult {
                    exit_code: -2,
                    output,
                    truncated,
                    running: false,
                });
            }
        }

        loop {
            // Check cancel token
            if let Some(ref ct) = cancel
                && ct.is_cancelled() {
                    return Ok(MonitorWaitResult {
                        exit_code: -1,
                        output,
                        truncated,
                        running: true,
                    });
                }

            // Drain buffered output + check exit status
            {
                let mut monitors = self.monitors.write().await;
                if let Some(entry) = monitors.get_mut(&mid) {
                    let cur = entry.read_cursor;
                    let (new_lines, dropped, new_consumed) = {
                        let ring = entry.output_buffer.lock().await;
                        ring.drain_from(cur)
                    };
                    entry.read_cursor = new_consumed;
                    if append_drained(new_lines, dropped, &mut output, max_output_bytes, &mut truncated) {
                        last_output_at = Some(Instant::now());
                    }
                    if let Some(code) = entry.exit_code {
                        return Ok(MonitorWaitResult {
                            exit_code: code,
                            output,
                            truncated,
                            running: false,
                        });
                    }
                } else {
                    return Ok(MonitorWaitResult {
                        exit_code: -2,
                        output,
                        truncated,
                        running: false,
                    });
                }
            }

            // Idle-return: have output and been quiet → likely waiting for input
            if let Some(lo) = last_output_at
                && lo.elapsed() >= idle_window && !output.is_empty() {
                    return Ok(MonitorWaitResult {
                        exit_code: -1,
                        output,
                        truncated,
                        running: true,
                    });
                }

            // Compute wait duration
            let no_output_window = Duration::from_secs(3);
            let overall_remaining = deadline.map(|d| d.saturating_duration_since(Instant::now()));
            let wait_dur = if last_output_at.is_some() {
                let since = last_output_at.unwrap().elapsed();
                if since >= idle_window {
                    continue;
                }
                let idle_left = idle_window - since;
                if let Some(r) = overall_remaining {
                    r.min(idle_left)
                } else {
                    idle_left
                }
            } else if let Some(r) = overall_remaining {
                if r.is_zero() {
                    return Ok(MonitorWaitResult {
                        exit_code: -1,
                        output: format!("{output}\n[monitor_wait timed out after {timeout_secs}s]"),
                        truncated: true,
                        running: true,
                    });
                }
                r.min(no_output_window)
            } else {
                no_output_window
            };

            // Wait for next event or timeout
            let recv_future = rx.recv();
            tokio::pin!(recv_future);
            let event_result = if let Some(ref ct) = cancel {
                tokio::select! {
                    biased;
                    _ = ct.cancelled() => {
                        return Ok(MonitorWaitResult { exit_code: -1, output, truncated, running: true });
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
                    Err(_) => None,
                }
            };

            let Some(event) = event_result else {
                if last_output_at.is_some() {
                    continue;
                }
                let is_overall_timeout = overall_remaining.is_some_and(|r| r.is_zero());
                return Ok(MonitorWaitResult {
                    exit_code: -1,
                    output: if is_overall_timeout {
                        format!("{output}\n[monitor_wait timed out after {timeout_secs}s]")
                    } else {
                        format!("{output}\n[no new output — monitor still running]")
                    },
                    truncated: is_overall_timeout,
                    running: true,
                });
            };

            match event {
                AgentEvent::MonitorOutput {
                    monitor_id: ev_mid, ..
                } if ev_mid == mid => {
                    last_output_at = Some(Instant::now());
                    continue;
                }
                AgentEvent::MonitorExited {
                    monitor_id: ev_mid,
                    exit_code,
                    ..
                } if ev_mid == mid => {
                    // Drain any final buffered output
                    let mut monitors = self.monitors.write().await;
                    if let Some(entry) = monitors.get_mut(&mid) {
                        let cur = entry.read_cursor;
                        let (new_lines, dropped, new_consumed) = {
                            let ring = entry.output_buffer.lock().await;
                            ring.drain_from(cur)
                        };
                        entry.read_cursor = new_consumed;
                        append_drained(new_lines, dropped, &mut output, max_output_bytes, &mut truncated);
                    }
                    return Ok(MonitorWaitResult {
                        exit_code,
                        output,
                        truncated,
                        running: false,
                    });
                }
                _ => {} // ignore unrelated events
            }
        }
    }
}

// ── Result types ─────────────────────────────────────────────────────────────

/// Result of waiting for a monitor to exit.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MonitorWaitResult {
    pub exit_code: i32,
    pub output: String,
    pub truncated: bool,
    /// True if the monitor is still running after this wait call.
    pub running: bool,
}

/// Non-blocking peek — returns monitor status immediately with current output.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MonitorPeekResult {
    pub monitor_id: String,
    pub name: String,
    pub running: bool,
    pub exit_code: Option<i32>,
    pub output: String,
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Flush accumulated lines to the EventBus and output buffer.
///
/// Fix 8: `stream` parameter replaces the hardcoded "stdout", so the WS
/// batcher can tag its output as "ws".
async fn flush_batch(
    event_bus: &EventBus,
    monitor_id: &str,
    batch: &mut Vec<String>,
    last_batch_at: &mut Option<Instant>,
    output_buffer: &Arc<Mutex<OutputRing>>,
    stream: &str,
    session_id: &str,
) {
    let data = batch.join("\n");
    batch.clear();
    *last_batch_at = Some(Instant::now());

    append_to_buffer(output_buffer, &data).await;

    let _ = event_bus.sender().send(AgentEvent::MonitorOutput {
        monitor_id: monitor_id.to_string(),
        data,
        stream: stream.to_string(),
        session_id: session_id.to_string(),
    });
}

/// Append a line to the ring buffer, trimming the oldest if over capacity.
async fn append_to_buffer(buf: &Arc<Mutex<OutputRing>>, line: &str) {
    buf.lock().await.push(line.to_string());
}

/// 把 `OutputRing::drain_from` 的结果写入 `output`（受 `max_output_bytes` 约束）。
/// 返回本次是否有新行。`dropped`（缓冲饱和丢弃过旧行）以一行提示体现并计入 truncated。
fn append_drained(
    new_lines: Vec<String>,
    dropped: bool,
    output: &mut String,
    max_output_bytes: usize,
    truncated: &mut bool,
) -> bool {
    if dropped && !*truncated {
        output.push_str("[earlier output dropped — monitor buffer overrun]\n");
        *truncated = true;
    }
    let had_new = !new_lines.is_empty();
    for line in &new_lines {
        if output.len() + line.len() + 1 > max_output_bytes {
            if !*truncated {
                output.push_str("[output truncated — max size reached]\n");
                *truncated = true;
            }
            break;
        }
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(line);
    }
    had_new
}

/// Update the MonitorInstance exit status so that `list()` and `gc()` see the
/// correct state after the batcher exits.
///
/// Fix 2 / Fix 5: called on every batcher exit path (normal exit, cancel,
/// timeout, rate-limit kill, WS close, WS stream end, broadcast lagged exit).
async fn mark_monitor_exited(
    monitors: &Arc<RwLock<HashMap<String, MonitorInstance>>>,
    monitor_id: &str,
    exit_code: i32,
) {
    let mut guard = monitors.write().await;
    if let Some(instance) = guard.get_mut(monitor_id) {
        instance.exit_code = Some(exit_code);
        instance.exited_at = Some(Instant::now());
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// 核心回归：缓冲饱和（触发从队首丢弃）后继续追加，消费者必须仍能读到新行。
    /// 旧的"绝对下标游标"实现里，`remove(0)` 让所有行下标前移而游标停在 len，
    /// 导致 `buf[cursor..]` 恒为空——饱和后永久读不到新输出（卡死）。
    #[test]
    fn output_ring_no_stuck_cursor_after_saturation() {
        let mut ring = OutputRing::new();
        // 填满并越过容量，触发队首丢弃
        for i in 0..(MAX_BUFFERED_LINES + 5) {
            ring.push(format!("line-{i}"));
        }
        // 模拟一个已读到 saturation 点的消费者
        let consumed = MAX_BUFFERED_LINES;
        // 饱和后继续追加新行
        for i in 0..3 {
            ring.push(format!("new-{i}"));
        }
        let (lines, _dropped, new_consumed) = ring.drain_from(consumed);
        assert!(
            lines.iter().any(|l| l == "new-2"),
            "饱和后追加的新行必须可读（修复前会永久丢失）: tail={:?}",
            &lines[lines.len().saturating_sub(3)..]
        );
        assert_eq!(new_consumed, ring.total_appended);
        // 读完后再 drain 应无新行
        let (lines2, _, _) = ring.drain_from(new_consumed);
        assert!(lines2.is_empty(), "已读完不应再返回行");
    }

    /// 消费者落后于缓冲窗口（行已被丢弃）时应报告 dropped，并钳制到最早可得行。
    #[test]
    fn output_ring_reports_dropped_when_consumer_lags() {
        let mut ring = OutputRing::new();
        for i in 0..(MAX_BUFFERED_LINES + 10) {
            ring.push(format!("l{i}"));
        }
        let (lines, dropped, consumed) = ring.drain_from(0);
        assert!(dropped, "消费者落后于缓冲窗口应报告丢弃");
        assert_eq!(lines.len(), MAX_BUFFERED_LINES);
        assert_eq!(lines[0], "l10", "应从最早仍可得行开始");
        assert_eq!(consumed, ring.total_appended);
    }

    /// 正常（未饱和）读取：顺序、完整、游标正确推进。
    #[test]
    fn output_ring_normal_sequential_read() {
        let mut ring = OutputRing::new();
        ring.push("a".into());
        ring.push("b".into());
        let (l1, d1, c1) = ring.drain_from(0);
        assert_eq!(l1, vec!["a", "b"]);
        assert!(!d1);
        assert_eq!(c1, 2);
        ring.push("c".into());
        let (l2, d2, c2) = ring.drain_from(c1);
        assert_eq!(l2, vec!["c"]);
        assert!(!d2);
        assert_eq!(c2, 3);
    }
}
