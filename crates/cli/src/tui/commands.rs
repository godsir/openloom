use crate::tui::app::App;

#[derive(Debug, Clone)]
pub enum SlashCommand {
    Help,
    Model,
    Cost,
    Clear,
    Theme(String),
    Session(String),
    Memory(String),
    Skills(String),
    Config(String),
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
        "model" | "m" => Some(SlashCommand::Model),
        "cost" => Some(SlashCommand::Cost),
        "clear" | "cls" => Some(SlashCommand::Clear),
        "theme" => Some(SlashCommand::Theme(args)),
        "session" => Some(SlashCommand::Session(args)),
        "memory" => Some(SlashCommand::Memory(args)),
        "skills" | "skill" => Some(SlashCommand::Skills(args)),
        "config" => Some(SlashCommand::Config(args)),
        _ => None, // unknown command
    }
}

/// Execute a slash command. Returns the response string.
pub async fn execute_command(app: &mut App, cmd: SlashCommand) -> String {
    match cmd {
        SlashCommand::Help => help_text(),
        SlashCommand::Model => {
            format!("Current model: {}", app.status.model)
        }
        SlashCommand::Cost => {
            format!(
                "Token usage — Prompt: {}k | Completion: {}k | Cost: ${:.4}",
                app.total_prompt_tokens as f64 / 1000.0,
                app.total_completion_tokens as f64 / 1000.0,
                app.total_cost,
            )
        }
        SlashCommand::Clear => {
            app.messages.clear();
            app.scroll = 0;
            app.auto_scroll = true;
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
            _ => "Usage: /session [new|list]".into(),
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
                                        format!(
                                            "[{}] {} (confidence: {:.0}%, evidence: {}, v{})",
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
        SlashCommand::Skills(args) => match args.as_str() {
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
            _ => "Usage: /skills [list]".into(),
        },
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
    }
}

fn help_text() -> String {
    r#"Slash Commands:
  /help           Show this help
  /model          Show current model
  /cost           Show token usage and cost
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
        assert!(matches!(parse_slash_command("/help"), Some(SlashCommand::Help)));
        assert!(matches!(parse_slash_command("/h"), Some(SlashCommand::Help)));
    }

    #[test]
    fn test_parse_model() {
        assert!(matches!(parse_slash_command("/model"), Some(SlashCommand::Model)));
        assert!(matches!(parse_slash_command("/m"), Some(SlashCommand::Model)));
    }

    #[test]
    fn test_parse_cost() {
        assert!(matches!(parse_slash_command("/cost"), Some(SlashCommand::Cost)));
    }

    #[test]
    fn test_parse_clear() {
        assert!(matches!(parse_slash_command("/clear"), Some(SlashCommand::Clear)));
        assert!(matches!(parse_slash_command("/cls"), Some(SlashCommand::Clear)));
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
        assert!(matches!(parse_slash_command("/skills"), Some(SlashCommand::Skills(_))));
        assert!(matches!(parse_slash_command("/skill list"), Some(SlashCommand::Skills(_))));
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
