use crate::tui::app::App;

#[derive(Debug, Clone)]
pub enum SlashCommand {
    Help,
    Model(String),
    Cost,
    Token(String),
    Clear,
    Theme(String),
    Session(String),
    Memory(String),
    Skills(String),
    Config(String),
    Health,
    Local(String),
}

pub fn parse_slash_command(input: &str) -> Option<SlashCommand> {
    let input = input.trim();
    if !input.starts_with('/') || input.starts_with("//") {
        return None;
    }

    let parts: Vec<&str> = input[1..].splitn(2, ' ').collect();
    let cmd = parts[0].to_lowercase();
    let args = parts.get(1).map(|s| s.to_string()).unwrap_or_default();

    match cmd.as_str() {
        "help" | "h" => Some(SlashCommand::Help),
        "model" | "m" => Some(SlashCommand::Model(args)),
        "cost" => Some(SlashCommand::Cost),
        "token" | "tokens" | "usage" => Some(SlashCommand::Token(args)),
        "clear" | "cls" => Some(SlashCommand::Clear),
        "theme" => Some(SlashCommand::Theme(args)),
        "session" => Some(SlashCommand::Session(args)),
        "memory" => Some(SlashCommand::Memory(args)),
        "skills" | "skill" => Some(SlashCommand::Skills(args)),
        "config" => Some(SlashCommand::Config(args)),
        "health" | "status" => Some(SlashCommand::Health),
        "local" => Some(SlashCommand::Local(args)),
        _ => None,
    }
}

/// Execute a slash command. Returns the response string.
pub async fn execute_command(app: &mut App, cmd: SlashCommand) -> String {
    match cmd {
        SlashCommand::Help => {
            app.overlay = Some(Box::new(crate::tui::overlays::help::HelpOverlay::new()));
            String::new() // overlay handles display
        }
        SlashCommand::Model(args) => {
            let parts: Vec<&str> = args.splitn(2, ' ').collect();
            let sub = parts.first().copied().unwrap_or("");
            let sub_args = parts.get(1).copied().unwrap_or("");
            match sub {
                "" => {
                    let info = app.engine.model_info().await;
                    let ctx_str = if info.context_size >= 1_000_000 {
                        format!("{}M", info.context_size / 1_000_000)
                    } else if info.context_size >= 1_000 {
                        format!("{}k", info.context_size / 1_000)
                    } else {
                        format!("{}", info.context_size)
                    };
                    let key_status = if info.api_key_env.is_empty() {
                        "not configured".into()
                    } else if info.api_key_set {
                        format!("{} (set)", info.api_key_env)
                    } else {
                        format!("{} (NOT SET)", info.api_key_env)
                    };
                    let url_line = if info.base_url.is_empty() {
                        String::new()
                    } else {
                        format!("\n  Base URL:  {}", info.base_url)
                    };
                    format!(
                        "Model:     {}\n  Backend:   {}\n  Model ID:  {}\n  Context:   {}{}\n  API Key:   {}\n  Session:   {} prompt / {} completion",
                        info.display_name,
                        info.backend,
                        info.model_id,
                        ctx_str,
                        url_line,
                        key_status,
                        format_tokens_short(app.total_prompt_tokens),
                        format_tokens_short(app.total_completion_tokens),
                    )
                }
                "set" => {
                    // /model set <backend> <model> [api_key_env]
                    let set_parts: Vec<&str> = sub_args.splitn(3, ' ').collect();
                    let backend = set_parts.first().copied().unwrap_or("");
                    let model_name = set_parts.get(1).copied().unwrap_or("");
                    let api_key_env = set_parts.get(2).copied().unwrap_or("");

                    if backend.is_empty() || model_name.is_empty() {
                        return "Usage: /model set <backend> <model> [api_key_env]\n\nBackends: anthropic, openai, deepseek\nExamples:\n  /model set anthropic claude-sonnet-4-20250514 ANTHROPIC_API_KEY\n  /model set openai gpt-4o OPENAI_API_KEY\n  /model set deepseek deepseek-chat DEEPSEEK_API_KEY".into();
                    }

                    let (backend_str, default_url) = match backend.to_lowercase().as_str() {
                        "anthropic" | "claude" => ("Anthropic", "https://api.anthropic.com"),
                        "openai" | "gpt" => ("OpenAI", "https://api.openai.com"),
                        "deepseek" => ("DeepSeek", "https://api.deepseek.com"),
                        _ => {
                            return format!(
                                "Unknown cloud backend: {}. Use 'anthropic', 'openai', or 'deepseek'.",
                                backend
                            );
                        }
                    };

                    let default_key_env = if api_key_env.is_empty() {
                        match backend_str {
                            "Anthropic" => "ANTHROPIC_API_KEY",
                            "OpenAI" => "OPENAI_API_KEY",
                            "DeepSeek" => "DEEPSEEK_API_KEY",
                            _ => "API_KEY",
                        }
                    } else {
                        api_key_env
                    };

                    let config_path = dirs::data_dir()
                        .unwrap_or_else(|| std::path::PathBuf::from("."))
                        .join("openLoom")
                        .join("config.toml");

                    let current = std::fs::read_to_string(&config_path).unwrap_or_default();

                    // Extract existing base_url and api_key_env from current cloud block
                    let mut existing_base_url: Option<String> = None;
                    let mut existing_key_env: Option<String> = None;
                    {
                        let lines: Vec<&str> = current.lines().collect();
                        let mut i = 0;
                        while i < lines.len() {
                            if lines[i].trim() == "[[models]]" {
                                let block_end = lines[i + 1..]
                                    .iter()
                                    .position(|l| {
                                        l.trim() == "[[models]]"
                                            || (l.starts_with('[') && !l.starts_with("[["))
                                    })
                                    .map(|p| i + 1 + p)
                                    .unwrap_or(lines.len());
                                let block: String = lines[i..block_end].join("\n");
                                if block.contains("Anthropic")
                                    || block.contains("OpenAI")
                                    || block.contains("DeepSeek")
                                {
                                    for line in &lines[i..block_end] {
                                        let trimmed = line.trim();
                                        if trimmed.starts_with("base_url")
                                            && let Some(val) = trimmed.split('=').nth(1)
                                        {
                                            let val = val.trim().trim_matches('"');
                                            if !val.is_empty() {
                                                existing_base_url = Some(val.to_string());
                                            }
                                        }
                                        if trimmed.starts_with("api_key_env")
                                            && let Some(val) = trimmed.split('=').nth(1)
                                        {
                                            let val = val.trim().trim_matches('"');
                                            if !val.is_empty() {
                                                existing_key_env = Some(val.to_string());
                                            }
                                        }
                                    }
                                    break;
                                }
                            }
                            i += 1;
                        }
                    }

                    // Use existing values if user didn't explicitly provide new ones
                    let final_url = existing_base_url.as_deref().unwrap_or(default_url);
                    let final_key_env = if api_key_env.is_empty() {
                        existing_key_env.as_deref().unwrap_or(default_key_env)
                    } else {
                        default_key_env
                    };

                    // Parse context size from [Xm]/[Xk] suffix or use existing/default
                    let context_size =
                        parse_context_hint_from_model(model_name).unwrap_or(match backend_str {
                            "Anthropic" => 200_000,
                            "OpenAI" => 128_000,
                            "DeepSeek" => 64_000,
                            _ => 128_000,
                        });

                    let config_content = format!(
                        "[[models]]\nname = \"cloud\"\nbackend = \"{}\"\nmodel = \"{}\"\napi_key_env = \"{}\"\nbase_url = \"{}\"\ncontext_size = {}\n",
                        backend_str, model_name, final_key_env, final_url, context_size
                    );

                    // Remove existing cloud model blocks
                    let new_content = {
                        let mut lines: Vec<&str> = current.lines().collect();
                        let mut i = 0;
                        while i < lines.len() {
                            if lines[i].trim() == "[[models]]" {
                                let block_end = lines[i + 1..]
                                    .iter()
                                    .position(|l| {
                                        l.trim() == "[[models]]"
                                            || (l.starts_with('[') && !l.starts_with("[["))
                                    })
                                    .map(|p| i + 1 + p)
                                    .unwrap_or(lines.len());
                                let block: String = lines[i..block_end].join("\n");
                                if block.contains("Anthropic")
                                    || block.contains("OpenAI")
                                    || block.contains("DeepSeek")
                                {
                                    lines.drain(i..block_end);
                                    continue;
                                }
                            }
                            i += 1;
                        }
                        format!("{}\n{}", lines.join("\n").trim(), config_content)
                    };

                    if let Some(parent) = config_path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    match std::fs::write(&config_path, new_content) {
                        Ok(()) => {
                            app.status.context_max = context_size;
                            app.status.model = format!(
                                "{} ({})",
                                model_name.split('[').next().unwrap_or(model_name).trim(),
                                backend_str
                            );
                            format!(
                                "Cloud model configured:\n  backend: {}\n  model: {}\n  api_key_env: {}\n  base_url: {}\n  context: {}\n\nMake sure ${} is set in your environment.\nRestart openloom to apply.",
                                backend_str,
                                model_name,
                                final_key_env,
                                final_url,
                                format_context_hint(context_size),
                                final_key_env
                            )
                        }
                        Err(e) => format!("Failed to write config: {}", e),
                    }
                }
                _ => "Usage: /model [set <backend> <model> [api_key_env]]".into(),
            }
        }
        SlashCommand::Cost => token_command(app, "").await,
        SlashCommand::Token(ref args) => token_command(app, args).await,
        SlashCommand::Clear => {
            app.messages.clear();
            app.viewport.jump_to_bottom();
            "Screen cleared.".into()
        }
        SlashCommand::Theme(args) => match args.as_str() {
            "light" => {
                app.theme = crate::tui::theme::Theme::light();
                "Theme switched to light.".into()
            }
            "dark" | "" => {
                app.theme = crate::tui::theme::Theme::dark();
                "Theme switched to dark.".into()
            }
            _ => format!("Unknown theme: {}. Use /theme dark or /theme light.", args),
        },
        SlashCommand::Session(args) => match args.as_str() {
            "new" => match app.engine.create_session().await {
                Ok(s) => {
                    app.session_id = s.id.clone();
                    app.messages.clear();
                    format!("Created session: {}", s.id)
                }
                Err(e) => format!("Failed to create session: {}", e),
            },
            "list" => match app.engine.list_sessions().await {
                Ok(sessions) => {
                    if sessions.is_empty() {
                        "No sessions.".into()
                    } else {
                        let mut lines = vec!["Sessions:".into()];
                        for s in &sessions {
                            let current = if s.id == app.session_id {
                                " (current)"
                            } else {
                                ""
                            };
                            lines.push(format!(
                                "  {} {} ({} msgs){}",
                                s.id, s.created_at, s.message_count, current
                            ));
                        }
                        lines.join("\n")
                    }
                }
                Err(e) => format!("Error: {}", e),
            },
            _ => {
                let args_trimmed = args.trim();
                if !args_trimmed.is_empty() && args_trimmed != "new" && args_trimmed != "list" {
                    // Treat as session ID to resume
                    let target_id = args_trimmed.to_string();
                    match app.engine.get_working_memory(&target_id) {
                        Ok(history) => {
                            app.session_id = target_id.clone();
                            app.messages.clear();
                            for msg in &history {
                                app.messages.push(crate::tui::app::Message {
                                    role: msg.role.clone(),
                                    content: msg.content.clone(),
                                    collapsed: msg.role == "thinking",
                                });
                            }
                            app.viewport.jump_to_bottom();
                            format!("Resumed session {} ({} messages)", target_id, history.len())
                        }
                        Err(e) => format!("Error loading session: {}", e),
                    }
                } else {
                    "Usage: /session [new|list|<session-id>]".into()
                }
            }
        },
        SlashCommand::Memory(args) => {
            let parts: Vec<&str> = args.splitn(2, ' ').collect();
            let sub = parts.first().copied().unwrap_or("");
            let sub_args = parts.get(1).copied().unwrap_or("");
            match sub {
                "persona" => app.engine.persona_summary().await,
                "events" => {
                    let limit: usize = sub_args.parse().unwrap_or(20);
                    match app.engine.list_events(limit).await {
                        Ok(events) => {
                            if events.is_empty() {
                                "No events recorded.".into()
                            } else {
                                let lines: Vec<String> = events
                                    .iter()
                                    .map(|e| {
                                        format!(
                                            "[{}] {}: {} (conf: {:.0}%)",
                                            e.timestamp,
                                            e.event_type,
                                            e.action,
                                            e.confidence * 100.0
                                        )
                                    })
                                    .collect();
                                format!("Events ({}):\n{}", events.len(), lines.join("\n"))
                            }
                        }
                        Err(e) => format!("Error: {}", e),
                    }
                }
                "cognitions" => {
                    let subject = if sub_args.is_empty() {
                        "USER"
                    } else {
                        sub_args
                    };
                    match app.engine.list_cognitions(subject, 20).await {
                        Ok(cognitions) => {
                            if cognitions.is_empty() {
                                format!("No cognitions for '{}'.", subject)
                            } else {
                                let lines: Vec<String> = cognitions
                                    .iter()
                                    .map(|c| {
                                        let scope_tag = if c.scope == "global" { "G" } else { "P" };
                                        format!(
                                            "[{}][{}] {} (confidence: {:.0}%, evidence: {}, v{})",
                                            scope_tag,
                                            c.trait_name,
                                            c.value,
                                            c.confidence * 100.0,
                                            c.evidence_count,
                                            c.version
                                        )
                                    })
                                    .collect();
                                lines.join("\n")
                            }
                        }
                        Err(e) => format!("Error: {}", e),
                    }
                }
                "search" => {
                    if sub_args.is_empty() {
                        "Usage: /memory search <query>".into()
                    } else {
                        match app.engine.search_events(sub_args, 20).await {
                            Ok(events) => {
                                if events.is_empty() {
                                    format!("No results for '{}'.", sub_args)
                                } else {
                                    let lines: Vec<String> = events
                                        .iter()
                                        .map(|e| {
                                            format!(
                                                "[{}] {}: {}",
                                                e.timestamp, e.event_type, e.action
                                            )
                                        })
                                        .collect();
                                    format!(
                                        "Search results ({}):\n{}",
                                        events.len(),
                                        lines.join("\n")
                                    )
                                }
                            }
                            Err(e) => format!("Error: {}", e),
                        }
                    }
                }
                _ => {
                    "Usage: /memory [persona|events [N]|cognitions [subject]|search <query>]".into()
                }
            }
        }
        SlashCommand::Skills(args) => {
            let parts: Vec<&str> = args.splitn(2, ' ').collect();
            let sub = parts.first().copied().unwrap_or("");
            match sub {
                "list" | "" => {
                    let skills = app.engine.list_skills();
                    if skills.is_empty() {
                        "No skills registered.".into()
                    } else {
                        let lines: Vec<String> = skills
                            .iter()
                            .map(|s| {
                                format!(
                                    "  {} — {} (triggers: {:?})",
                                    s.name, s.description, s.triggers
                                )
                            })
                            .collect();
                        format!("Skills:\n{}", lines.join("\n"))
                    }
                }
                "invoke" => {
                    let rest = parts.get(1).copied().unwrap_or("");
                    let invoke_parts: Vec<&str> = rest.splitn(2, ' ').collect();
                    let skill_name = invoke_parts.first().copied().unwrap_or("");
                    let params_str = invoke_parts.get(1).copied().unwrap_or("{}");
                    if skill_name.is_empty() {
                        "Usage: /skills invoke <name> [json_params]".into()
                    } else {
                        let params: serde_json::Value =
                            serde_json::from_str(params_str).unwrap_or(serde_json::json!({}));
                        match app.engine.invoke_skill(skill_name, params).await {
                            Ok(result) => format!("{}", result),
                            Err(e) => format!("Skill error: {}", e),
                        }
                    }
                }
                _ => "Usage: /skills [list|invoke <name> [params]]".into(),
            }
        }
        SlashCommand::Config(args) => {
            let parts: Vec<&str> = args.splitn(2, ' ').collect();
            match parts.first().copied() {
                Some("get") | Some("") => {
                    let key = parts.get(1).copied();
                    let v = app.engine.get_config(key).await;
                    format!("{}", v)
                }
                Some("set") => {
                    let rest = parts.get(1).unwrap_or(&"");
                    let kv: Vec<&str> = rest.splitn(2, ' ').collect();
                    if kv.len() < 2 {
                        "Usage: /config set <key> <value>".into()
                    } else {
                        match app.engine.set_config(kv[0], kv[1]).await {
                            Ok(()) => format!("{} = {}", kv[0], kv[1]),
                            Err(e) => format!("Error: {}", e),
                        }
                    }
                }
                _ => "Usage: /config [get [key]|set <key> <value>]".into(),
            }
        }
        SlashCommand::Health => {
            let health = app.engine.health_check().await;
            let cache = app.engine.cache_stats();
            format!(
                "Status: {} | Uptime: {}s | GPU: {} ({}MB)\nCache: {:.0}% hit rate | {} blocks | {:.1}MB",
                health.status,
                health.uptime,
                health.gpu_info.vendor,
                health.gpu_info.vram_mb,
                cache.hit_rate * 100.0,
                cache.block_count,
                cache.total_size_mb,
            )
        }
        SlashCommand::Local(args) => {
            let parts: Vec<&str> = args.splitn(2, ' ').collect();
            let sub = parts.first().copied().unwrap_or("status");
            let sub_args = parts.get(1).copied().unwrap_or("");
            match sub {
                "status" | "" => {
                    let model = app.status.model.clone();
                    let lmstudio_status =
                        ping_endpoint("http://localhost:1234/v1/models").await;
                    let ollama_status =
                        ping_endpoint("http://localhost:11434/v1/models").await;
                    format!(
                        "Active model: {}\nLM Studio (localhost:1234): {}\nOllama (localhost:11434): {}\n\nUse /local set <backend> <model> to configure.\nExample: /local set lmstudio qwen2.5-7b-instruct",
                        model, lmstudio_status, ollama_status
                    )
                }
                "set" => {
                    let set_parts: Vec<&str> = sub_args.splitn(2, ' ').collect();
                    let backend = set_parts.first().copied().unwrap_or("");
                    let model_name = set_parts.get(1).copied().unwrap_or("");

                    if backend.is_empty() || model_name.is_empty() {
                        return "Usage: /local set <backend> <model_name>\n\nBackends: lmstudio, ollama\nExamples:\n  /local set lmstudio qwen2.5-7b-instruct\n  /local set ollama qwen2.5:7b".into();
                    }

                    let (backend_str, default_url) = match backend.to_lowercase().as_str() {
                        "lmstudio" | "lm-studio" | "lm_studio" => {
                            ("LmStudio", "http://localhost:1234/v1")
                        }
                        "ollama" => ("Ollama", "http://localhost:11434/v1"),
                        _ => {
                            return format!(
                                "Unknown backend: {}. Use 'lmstudio' or 'ollama'.",
                                backend
                            );
                        }
                    };

                    let config_content = format!(
                        "[[models]]\nname = \"local\"\nbackend = \"{}\"\nmodel = \"{}\"\ncontext_size = 32000\nbase_url = \"{}\"\n",
                        backend_str, model_name, default_url
                    );

                    let config_path = dirs::data_dir()
                        .unwrap_or_else(|| std::path::PathBuf::from("."))
                        .join("openLoom")
                        .join("config.toml");

                    let current = std::fs::read_to_string(&config_path).unwrap_or_default();

                    // Check if there's already a local model entry and replace it
                    let new_content = if current.contains("backend = \"LmStudio\"")
                        || current.contains("backend = \"Ollama\"")
                    {
                        // Remove existing local model block and append new one
                        let mut lines: Vec<&str> = current.lines().collect();
                        let mut i = 0;
                        while i < lines.len() {
                            if lines[i].trim() == "[[models]]" {
                                // Check if this block is a local model
                                let block_end = lines[i + 1..]
                                    .iter()
                                    .position(|l| l.trim() == "[[models]]" || l.trim().starts_with('['))
                                    .map(|p| i + 1 + p)
                                    .unwrap_or(lines.len());
                                let block: String = lines[i..block_end].join("\n");
                                if block.contains("LmStudio") || block.contains("Ollama") {
                                    lines.drain(i..block_end);
                                    continue;
                                }
                            }
                            i += 1;
                        }
                        format!("{}\n{}", lines.join("\n").trim(), config_content)
                    } else {
                        format!("{}\n{}", current.trim(), config_content)
                    };

                    if let Some(parent) = config_path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    match std::fs::write(&config_path, new_content) {
                        Ok(()) => format!(
                            "Local model configured:\n  backend: {}\n  model: {}\n  url: {}\n\nRestart openloom to apply. Config: {}",
                            backend_str, model_name, default_url, config_path.display()
                        ),
                        Err(e) => format!("Failed to write config: {}", e),
                    }
                }
                "url" => {
                    if sub_args.is_empty() {
                        return "Usage: /local url <endpoint>\nExample: /local url http://localhost:1234/v1".into();
                    }
                    format!("Local endpoint URL: {}\nNote: edit config.toml base_url field and restart to apply.", sub_args)
                }
                "test" => {
                    use openloom_models::ChatMessage;
                    let msg = ChatMessage {
                        role: "user".into(),
                        content: "Hello, respond with just 'ok' to confirm you are working."
                            .into(),
                        timestamp: chrono::Utc::now(),
                    };
                    match app.engine.handle_message(msg, &app.session_id).await {
                        Ok(resp) => format!("Test response: {}", resp.response),
                        Err(e) => format!("Test failed: {}", e),
                    }
                }
                _ => {
                    "Usage: /local [status|set|test|url]\n\n  /local status          Check connectivity\n  /local set <be> <m>    Configure model\n  /local test            Send test prompt\n  /local url <endpoint>  Show/set endpoint".into()
                }
            }
        }
    }
}

async fn ping_endpoint(url: &str) -> &'static str {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build();
    match client {
        Ok(c) => match c.get(url).send().await {
            Ok(resp) if resp.status().is_success() => "online ●",
            Ok(_) => "error (bad status)",
            Err(_) => "offline ○",
        },
        Err(_) => "offline ○",
    }
}

fn parse_context_hint_from_model(model_name: &str) -> Option<usize> {
    let start = model_name.find('[')?;
    let end = model_name.find(']')?;
    if end <= start + 1 {
        return None;
    }
    let hint = &model_name[start + 1..end];
    let hint_lower = hint.to_lowercase();
    if let Some(num_str) = hint_lower.strip_suffix('m') {
        num_str
            .parse::<f64>()
            .ok()
            .map(|n| (n * 1_000_000.0) as usize)
    } else if let Some(num_str) = hint_lower.strip_suffix('k') {
        num_str.parse::<f64>().ok().map(|n| (n * 1_000.0) as usize)
    } else {
        hint.parse::<usize>().ok()
    }
}

fn format_context_hint(n: usize) -> String {
    if n >= 1_000_000 {
        format!("{}M", n / 1_000_000)
    } else if n >= 1_000 {
        format!("{}k", n / 1_000)
    } else {
        format!("{}", n)
    }
}

fn format_tokens_short(n: usize) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else if n > 0 {
        format!("{}", n)
    } else {
        "0".into()
    }
}

async fn token_command(app: &mut App, args: &str) -> String {
    let sub = args.split_whitespace().next().unwrap_or("");
    let sub_args = args.strip_prefix(sub).unwrap_or("").trim();

    match sub {
        "" => {
            let cache_info = if app.total_cached_tokens > 0 && app.total_prompt_tokens > 0 {
                let rate =
                    (app.total_cached_tokens as f64 / app.total_prompt_tokens as f64) * 100.0;
                format!(
                    "\n  Cache hit: {:.0}% ({} tokens)",
                    rate,
                    format_tokens_short(app.total_cached_tokens)
                )
            } else {
                String::new()
            };
            format!(
                "Session token usage:\n  Prompt:     {}\n  Completion: {}\n  Total:      {}\n  Cost:       ${:.4}{}",
                format_tokens_short(app.total_prompt_tokens),
                format_tokens_short(app.total_completion_tokens),
                format_tokens_short(app.total_prompt_tokens + app.total_completion_tokens),
                app.total_cost,
                cache_info,
            )
        }
        "summary" => match app.engine.token_summary_by_model() {
            Ok(summaries) => {
                if summaries.is_empty() {
                    "No token usage recorded yet.".into()
                } else {
                    let mut lines = vec!["Token usage by model:".into()];
                    let mut total_p = 0usize;
                    let mut total_c = 0usize;
                    for s in &summaries {
                        total_p += s.prompt_tokens;
                        total_c += s.completion_tokens;
                        lines.push(format!(
                            "  {} — {} prompt / {} completion ({} requests)",
                            s.model,
                            format_tokens_short(s.prompt_tokens),
                            format_tokens_short(s.completion_tokens),
                            s.request_count,
                        ));
                    }
                    lines.push(format!(
                        "\nTotal: {} prompt / {} completion",
                        format_tokens_short(total_p),
                        format_tokens_short(total_c),
                    ));
                    lines.join("\n")
                }
            }
            Err(e) => format!("Error: {}", e),
        },
        "today" => match app.engine.token_usage_today() {
            Ok(agg) => {
                if agg.request_count == 0 {
                    "No usage today.".into()
                } else {
                    let cache_info = if agg.cached_tokens > 0 {
                        format!(" (cache: {})", format_tokens_short(agg.cached_tokens))
                    } else {
                        String::new()
                    };
                    format!(
                        "Today's usage ({} requests):\n  Prompt:     {}\n  Completion: {}\n  Total:      {}{}",
                        agg.request_count,
                        format_tokens_short(agg.prompt_tokens),
                        format_tokens_short(agg.completion_tokens),
                        format_tokens_short(agg.prompt_tokens + agg.completion_tokens),
                        cache_info,
                    )
                }
            }
            Err(e) => format!("Error: {}", e),
        },
        "session" => {
            let sid = if sub_args.is_empty() {
                &app.session_id
            } else {
                sub_args
            };
            match app.engine.token_usage_session(sid, 20) {
                Ok(rows) => {
                    if rows.is_empty() {
                        format!("No token usage for session {}.", sid)
                    } else {
                        let total_p: usize = rows.iter().map(|r| r.prompt_tokens).sum();
                        let total_c: usize = rows.iter().map(|r| r.completion_tokens).sum();
                        let mut lines = vec![format!(
                            "Session {} ({} turns, {} prompt / {} completion):",
                            &sid[..sid.len().min(8)],
                            rows.len(),
                            format_tokens_short(total_p),
                            format_tokens_short(total_c),
                        )];
                        for r in rows.iter().take(10) {
                            let ts = &r.timestamp[..r.timestamp.len().min(16)];
                            lines.push(format!(
                                "  {} — {} + {} ({}ms)",
                                ts,
                                format_tokens_short(r.prompt_tokens),
                                format_tokens_short(r.completion_tokens),
                                r.latency_ms,
                            ));
                        }
                        if rows.len() > 10 {
                            lines.push(format!("  ... and {} more", rows.len() - 10));
                        }
                        lines.join("\n")
                    }
                }
                Err(e) => format!("Error: {}", e),
            }
        }
        "history" => {
            let limit: usize = sub_args.parse().unwrap_or(10);
            match app.engine.token_recent(limit) {
                Ok(rows) => {
                    if rows.is_empty() {
                        "No token usage history.".into()
                    } else {
                        let mut lines = vec![format!("Recent {} requests:", rows.len())];
                        for r in &rows {
                            let ts = &r.timestamp[..r.timestamp.len().min(16)];
                            lines.push(format!(
                                "  {} | {} | {} + {} | {}ms",
                                ts,
                                &r.model,
                                format_tokens_short(r.prompt_tokens),
                                format_tokens_short(r.completion_tokens),
                                r.latency_ms,
                            ));
                        }
                        lines.join("\n")
                    }
                }
                Err(e) => format!("Error: {}", e),
            }
        }
        _ => "Usage: /token [summary|today|session [id]|history [N]]".into(),
    }
}

#[allow(dead_code)]
fn help_text() -> String {
    r#"Slash Commands:
  /help           Show this help
  /model          Show current model info
  /token          Session token usage
  /token summary  Usage by model (all time)
  /token today    Today's usage
  /token history  Recent requests
  /cost           Alias for /token
  /clear          Clear screen
  /theme dark|light  Switch theme
  /session new|list  Session management
  /memory persona|events|search  Memory queries
  /skills list    List registered skills
  /config get|set Config management
  //message       Send literal /message (not a command)"#
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_help() {
        assert!(matches!(
            parse_slash_command("/help"),
            Some(SlashCommand::Help)
        ));
        assert!(matches!(
            parse_slash_command("/h"),
            Some(SlashCommand::Help)
        ));
    }

    #[test]
    fn test_parse_model() {
        assert!(matches!(
            parse_slash_command("/model"),
            Some(SlashCommand::Model(_))
        ));
        assert!(matches!(
            parse_slash_command("/m"),
            Some(SlashCommand::Model(_))
        ));
    }

    #[test]
    fn test_parse_cost() {
        assert!(matches!(
            parse_slash_command("/cost"),
            Some(SlashCommand::Cost)
        ));
    }

    #[test]
    fn test_parse_clear() {
        assert!(matches!(
            parse_slash_command("/clear"),
            Some(SlashCommand::Clear)
        ));
        assert!(matches!(
            parse_slash_command("/cls"),
            Some(SlashCommand::Clear)
        ));
    }

    #[test]
    fn test_parse_theme() {
        match parse_slash_command("/theme dark") {
            Some(SlashCommand::Theme(a)) => assert_eq!(a, "dark"),
            other => panic!("expected Theme(\"dark\"), got {:?}", other),
        }
        match parse_slash_command("/theme light") {
            Some(SlashCommand::Theme(a)) => assert_eq!(a, "light"),
            other => panic!("expected Theme(\"light\"), got {:?}", other),
        }
        match parse_slash_command("/theme") {
            Some(SlashCommand::Theme(a)) => assert!(a.is_empty()),
            other => panic!("expected Theme(\"\"), got {:?}", other),
        }
    }

    #[test]
    fn test_parse_session() {
        match parse_slash_command("/session new") {
            Some(SlashCommand::Session(a)) => assert_eq!(a, "new"),
            other => panic!("expected Session(\"new\"), got {:?}", other),
        }
        match parse_slash_command("/session list") {
            Some(SlashCommand::Session(a)) => assert_eq!(a, "list"),
            other => panic!("expected Session(\"list\"), got {:?}", other),
        }
    }

    #[test]
    fn test_parse_memory() {
        match parse_slash_command("/memory persona") {
            Some(SlashCommand::Memory(a)) => assert_eq!(a, "persona"),
            other => panic!("expected Memory(\"persona\"), got {:?}", other),
        }
        match parse_slash_command("/memory events 10") {
            Some(SlashCommand::Memory(a)) => assert_eq!(a, "events 10"),
            other => panic!("expected Memory(\"events 10\"), got {:?}", other),
        }
        match parse_slash_command("/memory search hello world") {
            Some(SlashCommand::Memory(a)) => assert_eq!(a, "search hello world"),
            other => panic!("expected Memory(\"search hello world\"), got {:?}", other),
        }
    }

    #[test]
    fn test_parse_skills() {
        assert!(matches!(
            parse_slash_command("/skills"),
            Some(SlashCommand::Skills(_))
        ));
        assert!(matches!(
            parse_slash_command("/skill list"),
            Some(SlashCommand::Skills(_))
        ));
    }

    #[test]
    fn test_parse_config() {
        match parse_slash_command("/config get foo") {
            Some(SlashCommand::Config(a)) => assert_eq!(a, "get foo"),
            other => panic!("expected Config(\"get foo\"), got {:?}", other),
        }
        match parse_slash_command("/config set foo bar") {
            Some(SlashCommand::Config(a)) => assert_eq!(a, "set foo bar"),
            other => panic!("expected Config(\"set foo bar\"), got {:?}", other),
        }
    }

    #[test]
    fn test_parse_unknown() {
        assert!(parse_slash_command("/foobar").is_none());
        assert!(parse_slash_command("/").is_none());
    }

    #[test]
    fn test_literal_slash() {
        assert!(parse_slash_command("//help").is_none());
        assert!(parse_slash_command("//anything").is_none());
    }

    #[test]
    fn test_no_slash() {
        assert!(parse_slash_command("hello").is_none());
        assert!(parse_slash_command("").is_none());
    }

    #[test]
    fn test_help_text_contains_all_commands() {
        let text = help_text();
        assert!(text.contains("/help"));
        assert!(text.contains("/model"));
        assert!(text.contains("/cost"));
        assert!(text.contains("/clear"));
        assert!(text.contains("/theme"));
        assert!(text.contains("/session"));
        assert!(text.contains("/memory"));
        assert!(text.contains("/skills"));
        assert!(text.contains("/config"));
        assert!(text.contains("//"));
    }
}
