use crossterm::ExecutableCommand;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseEventKind};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use openloom_engine::Engine;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};
use std::sync::Arc;
use tui_textarea::TextArea;

const BRAND: Color = Color::Rgb(102, 59, 249);

const COMMANDS: &[(&str, &str)] = &[
    // Chat
    ("/exit", "Quit openLoom"),
    ("/help", "Show help"),
    ("/clear", "Clear conversation"),
    // Session
    ("/session", "List sessions"),
    ("/session new", "Create new session"),
    ("/session switch", "Switch to a session"),
    // Memory
    ("/memory persona", "Show persona summary"),
    ("/memory events", "Show recent events"),
    ("/memory cognitions", "Show cognitions for USER"),
    // System
    ("/model", "Show model info"),
    ("/skills", "List available skills"),
    ("/agent", "Show agent state"),
    ("/doctor", "System diagnostic"),
    ("/cache", "KV cache statistics"),
    ("/config", "Show all config"),
    ("/config <key>", "Show a config value"),
    ("/version", "Version info"),
];

struct Message {
    role: String,
    content: String,
}

pub async fn run(engine: Arc<Engine>) -> anyhow::Result<()> {
    let session_id = engine.create_session().await?.id;

    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    stdout.execute(EnterAlternateScreen)?;
    stdout.execute(crossterm::event::EnableMouseCapture)?;
    let mut terminal = ratatui::Terminal::new(ratatui::backend::CrosstermBackend::new(stdout))?;

    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "?".into());
    let model_name = engine.model_display_name();

    let mut app = ChatApp {
        engine,
        session_id,
        cwd,
        model_name,
        messages: Vec::new(),
        input: build_textarea(),
        history: Vec::new(),
        history_idx: None,
        loading: false,
        scroll: 0,
        show_commands: false,
        selected_command: 0,
    };

    let res = app.run(&mut terminal).await;

    disable_raw_mode()?;
    terminal.backend_mut().execute(LeaveAlternateScreen)?;
    terminal
        .backend_mut()
        .execute(crossterm::event::DisableMouseCapture)?;
    res
}

fn build_textarea() -> TextArea<'static> {
    let mut ta = TextArea::default();
    ta.set_block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(" Message (Enter send, Shift+Enter newline, / for commands) "),
    );
    ta.set_style(Style::default().fg(Color::White));
    ta.set_cursor_style(Style::default().fg(BRAND).add_modifier(Modifier::REVERSED));
    ta.set_cursor_line_style(Style::default());
    ta.set_placeholder_text("Type your message...");
    ta.set_placeholder_style(Style::default().fg(Color::DarkGray));
    ta
}

struct ChatApp {
    engine: Arc<Engine>,
    session_id: String,
    cwd: String,
    model_name: String,
    messages: Vec<Message>,
    input: TextArea<'static>,
    history: Vec<String>,
    history_idx: Option<usize>,
    loading: bool,
    scroll: usize,
    show_commands: bool,
    selected_command: usize,
}

impl ChatApp {
    async fn run(&mut self, terminal: &mut DefaultTerminal) -> anyhow::Result<()> {
        loop {
            let cur = self.current_line();
            self.show_commands = cur.trim_start().starts_with('/');
            terminal.draw(|f| self.draw(f))?;

            match event::read()? {
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Release {
                        continue;
                    }
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        match key.code {
                            KeyCode::Char('c') => break,
                            KeyCode::Char('d') => break,
                            _ => {}
                        }
                        continue;
                    }
                    match key.code {
                        KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
                            self.input.insert_newline();
                        }
                        KeyCode::Tab => {
                            if self.show_commands {
                                let suggestions = self.filter_commands();
                                if let Some((cmd, _)) = suggestions.get(self.selected_command) {
                                    self.input = build_textarea();
                                    self.input.insert_str(cmd);
                                    self.input.insert_str(" ");
                                    self.selected_command = 0;
                                }
                            }
                        }
                        KeyCode::Up => {
                            if self.show_commands {
                                let suggestions = self.filter_commands();
                                if !suggestions.is_empty() {
                                    self.selected_command = self
                                        .selected_command
                                        .checked_sub(1)
                                        .unwrap_or(suggestions.len() - 1);
                                }
                            } else {
                                self.navigate_history(-1);
                            }
                        }
                        KeyCode::Down => {
                            if self.show_commands {
                                let suggestions = self.filter_commands();
                                let max = suggestions.len().saturating_sub(1);
                                self.selected_command = (self.selected_command + 1).min(max);
                            } else {
                                self.navigate_history(1);
                            }
                        }
                        KeyCode::Enter => {
                            self.history_idx = None;
                            let text = self.input.lines().join("\n").trim().to_string();
                            self.input = build_textarea();
                            if text.is_empty() {
                                continue;
                            }
                            if self.handle_command(&text).await {
                                continue;
                            }
                            // Not a command — send as chat message
                            if self.history.last() != Some(&text) {
                                self.history.push(text.clone());
                            }
                            self.history_idx = None;
                            self.messages.push(Message {
                                role: "user".into(),
                                content: text.clone(),
                            });
                            self.loading = true;
                            self.scroll = 0;
                            terminal.draw(|f| self.draw(f))?;

                            let msg = openloom_models::ChatMessage {
                                role: "user".into(),
                                content: text,
                                timestamp: chrono::Utc::now(),
                            };
                            match self.engine.handle_message(msg, &self.session_id).await {
                                Ok(resp) => {
                                    self.messages.push(Message {
                                        role: "assistant".into(),
                                        content: resp.response,
                                    });
                                    let usage = format!(
                                        "{} prompt + {} completion tokens · {}ms",
                                        resp.token_usage.prompt_tokens,
                                        resp.token_usage.completion_tokens,
                                        resp.token_usage.latency_ms,
                                    );
                                    self.messages.push(Message {
                                        role: "usage".into(),
                                        content: usage,
                                    });
                                }
                                Err(e) => {
                                    self.messages.push(Message {
                                        role: "error".into(),
                                        content: format!("Error: {}", e),
                                    });
                                }
                            }
                            self.loading = false;
                            self.scroll = 0;
                        }
                        KeyCode::Esc => break,
                        _ => {
                            self.history_idx = None;
                            self.selected_command = 0;
                            self.input.input(key);
                        }
                    }
                }
                Event::Mouse(mouse) => match mouse.kind {
                    MouseEventKind::ScrollUp => {
                        self.scroll = self.scroll.saturating_add(1);
                    }
                    MouseEventKind::ScrollDown => {
                        self.scroll = self.scroll.saturating_sub(1);
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        Ok(())
    }

    /// Returns true if `text` was a slash command and was handled.
    async fn handle_command(&mut self, text: &str) -> bool {
        let text = text.trim();
        if !text.starts_with('/') {
            return false;
        }

        match text {
            "/exit" => {
                // Signal caller to break the loop
                std::process::exit(0);
            }
            "/help" => {
                let mut out = String::from("Chat:\n");
                for (c, d) in COMMANDS.iter().take(3) {
                    out.push_str(&format!("  {} — {}\n", c, d));
                }
                out.push_str("\nSession:\n");
                for (c, d) in COMMANDS.iter().skip(3).take(3) {
                    out.push_str(&format!("  {} — {}\n", c, d));
                }
                out.push_str("\nMemory:\n");
                for (c, d) in COMMANDS.iter().skip(6).take(3) {
                    out.push_str(&format!("  {} — {}\n", c, d));
                }
                out.push_str("\nSystem:\n");
                for (c, d) in COMMANDS.iter().skip(9) {
                    out.push_str(&format!("  {} — {}\n", c, d));
                }
                self.push_assistant(&out);
                return true;
            }
            "/clear" => {
                self.messages.clear();
                return true;
            }
            "/model" => {
                let health = self.engine.health_check().await;
                let out = format!(
                    "Model: {}\nStatus: {}\nUptime: {}s\nGPU: {} ({}MB, supported: {})",
                    self.model_name,
                    health.status,
                    health.uptime,
                    health.gpu_info.vendor,
                    health.gpu_info.vram_mb,
                    health.gpu_info.supported,
                );
                self.push_assistant(&out);
                return true;
            }
            "/session" => {
                match self.engine.list_sessions().await {
                    Ok(sessions) => {
                        if sessions.is_empty() {
                            self.push_assistant("No sessions found.");
                        } else {
                            let mut out = String::new();
                            for s in &sessions {
                                let active = if s.id == self.session_id { " *" } else { "" };
                                out.push_str(&format!(
                                    "{}  {}  ({} msgs){}\n",
                                    s.id, s.created_at, s.message_count, active
                                ));
                            }
                            out.push_str("\n* = current session");
                            self.push_assistant(&out);
                        }
                    }
                    Err(e) => self.push_error(&format!("Failed to list sessions: {}", e)),
                }
                return true;
            }
            "/session new" => {
                match self.engine.create_session().await {
                    Ok(s) => {
                        self.session_id = s.id.clone();
                        self.messages.clear();
                        self.push_assistant(&format!("Created and switched to session: {}", s.id));
                    }
                    Err(e) => self.push_error(&format!("Failed to create session: {}", e)),
                }
                return true;
            }
            "/doctor" => {
                let health = self.engine.health_check().await;
                let gpu = &health.gpu_info;
                let out = format!(
                    "openLoom System Diagnostic\n\
                     Status:   {}\n\
                     Uptime:   {}s\n\
                     GPU:      {} ({}MB)\n\
                     GPU OK:   {}\n\
                     Model:    {}",
                    health.status,
                    health.uptime,
                    gpu.vendor,
                    gpu.vram_mb,
                    gpu.supported,
                    self.model_name,
                );
                self.push_assistant(&out);
                return true;
            }
            "/memory persona" => {
                let summary = self.engine.persona_summary().await;
                if summary.is_empty() {
                    self.push_assistant(
                        "No persona data yet. Interact more to build a cognition profile.",
                    );
                } else {
                    self.push_assistant(&summary);
                }
                return true;
            }
            "/memory cognitions" => {
                match self.engine.list_cognitions("USER", 20).await {
                    Ok(cognitions) => {
                        if cognitions.is_empty() {
                            self.push_assistant("No cognitions for USER yet.");
                        } else {
                            let mut out = String::new();
                            for c in &cognitions {
                                out.push_str(&format!(
                                    "[v{}] {} = {} (confidence: {:.0}%, evidence: {})\n",
                                    c.version,
                                    c.trait_name,
                                    c.value,
                                    c.confidence * 100.0,
                                    c.evidence_count,
                                ));
                            }
                            self.push_assistant(&out);
                        }
                    }
                    Err(e) => self.push_error(&format!("Failed: {}", e)),
                }
                return true;
            }
            "/skills" => {
                let skills = self.engine.list_skills();
                if skills.is_empty() {
                    self.push_assistant("No skills registered.");
                } else {
                    let mut out = String::new();
                    for s in &skills {
                        out.push_str(&format!(
                            "{} — {}\n  triggers: {:?}\n",
                            s.name, s.description, s.triggers,
                        ));
                    }
                    self.push_assistant(&out);
                }
                return true;
            }
            "/agent" => {
                let state = self.engine.agent_state().await;
                let out = format!("Agent state: {:?}", state);
                self.push_assistant(&out);
                return true;
            }
            "/cache" => {
                let stats = self.engine.cache_stats();
                let out = format!(
                    "Cache: hit_rate={:.1}%, blocks={}, size={:.1}MB",
                    stats.hit_rate * 100.0,
                    stats.block_count,
                    stats.total_size_mb,
                );
                self.push_assistant(&out);
                return true;
            }
            "/version" => {
                self.push_assistant(&format!("openLoom {}", env!("CARGO_PKG_VERSION")));
                return true;
            }
            _ => {}
        }

        // Sub-commands with arguments
        if let Some(rest) = text.strip_prefix("/session switch ") {
            let id = rest.trim();
            if id.is_empty() {
                self.push_assistant("Usage: /session switch <session_id>");
            } else {
                self.session_id = id.to_string();
                self.messages.clear();
                self.push_assistant(&format!("Switched to session: {}", id));
            }
            return true;
        }

        if let Some(rest) = text.strip_prefix("/memory events") {
            let limit: usize = rest.trim().parse().unwrap_or(10);
            match self.engine.list_events(limit).await {
                Ok(events) => {
                    if events.is_empty() {
                        self.push_assistant("No events recorded yet.");
                    } else {
                        let mut out = String::new();
                        for e in &events {
                            out.push_str(&format!(
                                "[{}] {}: {} (conf: {:.0}%)\n",
                                e.timestamp,
                                e.event_type,
                                e.action,
                                e.confidence * 100.0,
                            ));
                        }
                        self.push_assistant(&out);
                    }
                }
                Err(e) => self.push_error(&format!("Failed: {}", e)),
            }
            return true;
        }

        if let Some(key) = text.strip_prefix("/config ") {
            let key = key.trim();
            let val = self
                .engine
                .get_config(if key.is_empty() { None } else { Some(key) })
                .await;
            let out = if key.is_empty() {
                serde_json::to_string_pretty(&val).unwrap_or_else(|_| "error".into())
            } else if val.is_null() {
                format!("Key '{}' not found", key)
            } else {
                format!("{} = {}", key, val)
            };
            self.push_assistant(&out);
            return true;
        }

        if text == "/config" {
            let val = self.engine.get_config(None).await;
            let out = serde_json::to_string_pretty(&val).unwrap_or_else(|_| "error".into());
            self.push_assistant(&out);
            return true;
        }

        // Unknown command — treat as chat message
        false
    }

    fn push_assistant(&mut self, content: &str) {
        self.messages.push(Message {
            role: "assistant".into(),
            content: content.to_string(),
        });
    }

    fn push_error(&mut self, content: &str) {
        self.messages.push(Message {
            role: "error".into(),
            content: content.to_string(),
        });
    }

    fn current_line(&self) -> String {
        let cursor = self.input.cursor();
        let (row, _col) = cursor;
        self.input.lines().get(row).cloned().unwrap_or_default()
    }

    /// Navigate input history: -1 = older, +1 = newer.
    fn navigate_history(&mut self, delta: isize) {
        if self.history.is_empty() {
            return;
        }
        let len = self.history.len();
        let new = match self.history_idx {
            None if delta < 0 => len.saturating_sub(1), // first Up: go to last
            None => return,                               // first Down: stay
            Some(i) => {
                let next = i as isize + delta;
                if next < 0 || next as usize >= len {
                    return;
                }
                next as usize
            }
        };
        self.history_idx = Some(new);
        self.input = build_textarea();
        self.input.insert_str(&self.history[new]);
    }

    fn draw(&self, f: &mut Frame) {
        let area = f.area();

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),
                Constraint::Length(1),
                Constraint::Length(self.input_height() as u16),
            ])
            .split(area);

        self.draw_messages(f, chunks[0]);
        self.draw_status(f, chunks[1]);

        // Command suggestions popup
        if self.show_commands {
            let suggestions = self.filter_commands();
            if !suggestions.is_empty() {
                let visible_rows = 8usize;
                let total = suggestions.len();
                let selected = self.selected_command.min(total.saturating_sub(1));

                // Scroll window: keep selected item centered
                let window_start = if total <= visible_rows {
                    0
                } else {
                    let half = visible_rows / 2;
                    selected.saturating_sub(half).min(total - visible_rows)
                };
                let window_end = (window_start + visible_rows).min(total);
                let win_slice = &suggestions[window_start..window_end];

                let show_count = win_slice.len() + 1; // +1 for header
                let popup_height = (show_count + 2) as u16; // +2 for borders
                let popup_width = 42u16;
                let input_area = chunks[2];
                let popup_y = input_area.y.saturating_sub(popup_height);
                let popup_x = input_area.x + 2;

                let popup_rect = Rect::new(popup_x, popup_y, popup_width, popup_height);
                if popup_rect.y >= chunks[0].y + 2 {
                    let selected_style = Style::default()
                        .fg(Color::White)
                        .bg(BRAND)
                        .add_modifier(Modifier::BOLD);
                    let normal_name = Style::default().fg(BRAND);
                    let normal_desc = Style::default().fg(Color::DarkGray);
                    let selected_desc = Style::default().fg(Color::Rgb(200, 180, 255)).bg(BRAND);

                    let mut cmd_lines: Vec<Line> = win_slice
                        .iter()
                        .enumerate()
                        .map(|(i, (name, desc))| {
                            let global_i = window_start + i;
                            if global_i == selected {
                                Line::from(vec![
                                    Span::styled(format!(" {}", name), selected_style),
                                    Span::raw("  "),
                                    Span::styled(*desc, selected_desc),
                                ])
                            } else {
                                Line::from(vec![
                                    Span::styled(format!(" {}", name), normal_name),
                                    Span::raw("  "),
                                    Span::styled(*desc, normal_desc),
                                ])
                            }
                        })
                        .collect();
                    // Show scroll indicator when there are more items above/below
                    let header_text = if total > visible_rows {
                        format!(
                            " \u{2191}\u{2193} choose ({}/{}) Tab to complete",
                            selected + 1,
                            total
                        )
                    } else {
                        " \u{2191}\u{2193} choose, Tab to complete".to_string()
                    };
                    cmd_lines.insert(
                        0,
                        Line::from(Span::styled(
                            header_text,
                            Style::default().fg(Color::DarkGray),
                        )),
                    );
                    let cmd_text = Text::from(cmd_lines);
                    let popup = Paragraph::new(cmd_text)
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .border_style(Style::default().fg(BRAND)),
                        )
                        .style(Style::default().bg(Color::Rgb(20, 18, 30)));
                    f.render_widget(Clear, popup_rect);
                    f.render_widget(popup, popup_rect);
                }
            }
        }

        f.render_widget(&self.input, chunks[2]);
    }

    fn filter_commands(&self) -> Vec<(&'static str, &'static str)> {
        let line = self.current_line();
        let prefix = line.trim_start();
        if !prefix.starts_with('/') {
            return vec![];
        }
        COMMANDS
            .iter()
            .filter(|(c, _)| c.starts_with(prefix))
            .copied()
            .collect()
    }

    fn draw_messages(&self, f: &mut Frame, area: Rect) {
        let mut lines: Vec<Line> = Vec::new();

        if self.messages.is_empty() {
            let dim = Style::default().fg(Color::DarkGray);
            let brand_bold = Style::default().fg(BRAND).add_modifier(Modifier::BOLD);

            lines.push(Line::from(""));
            // LOOM logo (from loom_logo.py)
            for row in loom_rows() {
                lines.push(row);
            }
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("  openLoom", brand_bold),
                Span::styled(" v0.1.0", dim),
            ]));
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("  Model   ", dim),
                Span::styled(&self.model_name, Style::default().fg(Color::White)),
            ]));
            lines.push(Line::from(vec![
                Span::styled("  Project ", dim),
                Span::styled(&self.cwd, Style::default().fg(Color::White)),
            ]));
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("  /", Style::default().fg(BRAND)),
                Span::styled(" for commands  ", dim),
                Span::styled("Shift+Enter", Style::default().fg(BRAND)),
                Span::styled(" newline  ", dim),
                Span::styled("Ctrl+C", Style::default().fg(BRAND)),
                Span::styled(" quit", dim),
            ]));
            lines.push(Line::from(""));
        }

        for msg in &self.messages {
            match msg.role.as_str() {
                "user" => {
                    lines.push(Line::from(""));
                    lines.push(Line::from(vec![Span::styled(
                        "  ▸ You",
                        Style::default().fg(BRAND).add_modifier(Modifier::BOLD),
                    )]));
                    for line in wrap_text(&msg.content, area.width.saturating_sub(4)) {
                        lines.push(Line::from(Span::styled(
                            format!("  {}", line),
                            Style::default().fg(Color::White),
                        )));
                    }
                }
                "assistant" => {
                    lines.push(Line::from(""));
                    lines.push(Line::from(vec![Span::styled(
                        "  ▸ Assistant",
                        Style::default().fg(BRAND).add_modifier(Modifier::BOLD),
                    )]));
                    for line in wrap_text(&msg.content, area.width.saturating_sub(4)) {
                        lines.push(Line::from(Span::styled(
                            format!("  {}", line),
                            Style::default().fg(Color::White),
                        )));
                    }
                }
                "usage" => {
                    lines.push(Line::from(vec![Span::styled(
                        format!("  ── {}", msg.content),
                        Style::default().fg(Color::DarkGray),
                    )]));
                }
                _ => {
                    for line in wrap_text(&msg.content, area.width.saturating_sub(4)) {
                        lines.push(Line::from(Span::styled(
                            format!("  {}", line),
                            Style::default().fg(Color::Red),
                        )));
                    }
                }
            }
        }

        if self.loading {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled(
                    "  ✌ Assistant",
                    Style::default().fg(BRAND).add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled("● Thinking...", Style::default().fg(Color::Yellow)),
            ]));
        }

        let msg_height = lines.len() as u16;
        let max_scroll = msg_height.saturating_sub(area.height);
        let scroll = self.scroll.min(max_scroll as usize);

        let content = Text::from(lines);
        let p = Paragraph::new(content)
            .block(Block::default().borders(Borders::NONE))
            .scroll((scroll as u16, 0))
            .wrap(Wrap { trim: false });
        f.render_widget(p, area);

        // Scrollbar overlay
        if max_scroll > 0 {
            let track = Style::default().fg(Color::Rgb(50, 50, 50));
            let thumb = Style::default().fg(Color::Rgb(120, 120, 130));
            let total = msg_height.max(1) as f64;
            let vis = area.height as f64;
            let pos = scroll as f64;
            let max = max_scroll as f64;

            let thumb_h = ((vis / total) * vis).ceil() as u16;
            let thumb_h = thumb_h.max(1).min(area.height);
            let thumb_y = if max > 0.0 {
                ((pos / max) * (vis - thumb_h as f64)) as u16
            } else {
                0
            };
            let x = area.right().saturating_sub(1);

            for row in 0..area.height {
                let y = area.y + row;
                let ch = if row >= thumb_y && row < thumb_y + thumb_h {
                    Span::styled("\u{2593}", thumb) // ▓ medium shade
                } else {
                    Span::styled("\u{2502}", track) // │ light vertical
                };
                f.render_widget(Paragraph::new(Line::from(ch)), Rect::new(x, y, 1, 1));
            }
        }
    }

    fn draw_status(&self, f: &mut Frame, area: Rect) {
        let status = if self.loading {
            Span::styled(" ● Thinking... ", Style::default().fg(Color::Yellow))
        } else {
            Span::styled(" ● Ready ", Style::default().fg(Color::DarkGray))
        };
        let session = Span::styled(
            format!(" {} ", &self.session_id[..self.session_id.len().min(8)]),
            Style::default().fg(Color::DarkGray),
        );
        let msg_count = Span::styled(
            format!(" {} messages ", self.messages.len() / 2),
            Style::default().fg(Color::DarkGray),
        );
        let line = Line::from(vec![status, session, msg_count]);
        f.render_widget(
            Paragraph::new(line).style(Style::default().bg(Color::Rgb(30, 30, 30))),
            area,
        );
    }

    fn input_height(&self) -> usize {
        self.input.lines().len().clamp(3, 10)
    }
}

/// LOOM logo from loom_logo.py — white bold on #663BF9 background.
fn loom_rows() -> [Line<'static>; 9] {
    let s = Style::default()
        .fg(Color::White)
        .bg(BRAND)
        .add_modifier(Modifier::BOLD);
    // Each row: 8-space pad + L(7) + "  " + O(7) + "  " + O(7) + "  " + M(7)
    [
        Line::from(Span::styled(
            "        \u{2588}       \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}     \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}    \u{2588}     \u{2588}",
            s,
        )),
        Line::from(Span::styled(
            "        \u{2588}      \u{2588}     \u{2588}  \u{2588}     \u{2588}  \u{2588}\u{2588}   \u{2588}\u{2588}",
            s,
        )),
        Line::from(Span::styled(
            "        \u{2588}      \u{2588}     \u{2588}  \u{2588}     \u{2588}  \u{2588} \u{2588} \u{2588} \u{2588}",
            s,
        )),
        Line::from(Span::styled(
            "        \u{2588}      \u{2588}     \u{2588}  \u{2588}     \u{2588}  \u{2588}  \u{2588}  \u{2588}",
            s,
        )),
        Line::from(Span::styled(
            "        \u{2588}      \u{2588}     \u{2588}  \u{2588}     \u{2588}  \u{2588}     \u{2588}",
            s,
        )),
        Line::from(Span::styled(
            "        \u{2588}      \u{2588}     \u{2588}  \u{2588}     \u{2588}  \u{2588}     \u{2588}",
            s,
        )),
        Line::from(Span::styled(
            "        \u{2588}      \u{2588}     \u{2588}  \u{2588}     \u{2588}  \u{2588}     \u{2588}",
            s,
        )),
        Line::from(Span::styled(
            "        \u{2588}      \u{2588}     \u{2588}  \u{2588}     \u{2588}  \u{2588}     \u{2588}",
            s,
        )),
        Line::from(Span::styled(
            "        \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}  \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}     \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}    \u{2588}     \u{2588}",
            s,
        )),
    ]
}

fn wrap_text(text: &str, width: u16) -> Vec<String> {
    let width = width.max(20) as usize;
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut current = String::new();
        for word in paragraph.split(' ') {
            if current.len() + word.len() + 1 > width && !current.is_empty() {
                lines.push(current);
                current = word.to_string();
            } else if current.is_empty() {
                current = word.to_string();
            } else {
                current.push(' ');
                current.push_str(word);
            }
        }
        if !current.is_empty() {
            lines.push(current);
        }
    }
    lines
}
