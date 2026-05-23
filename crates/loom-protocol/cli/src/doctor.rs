//! Implements the `loom doctor` diagnostic report.
//!
//! A simplified diagnostic tool that checks basic openLoom installation health:
//! - Loom home directory
//! - Config file presence
//! - Database file presence
//! - Terminal capability


use clap::Parser;
use loom_arg0::Arg0DispatchPaths;
use loom_cli_utils::CliConfigOverrides;
use loom_tui::Cli as TuiCli;
use serde::Serialize;
use supports_color::Stream;

#[derive(Debug, Parser)]
pub struct DoctorCommand {
    /// Emit a redacted machine-readable report.
    #[arg(long, default_value_t = false)]
    json: bool,

    /// Only show grouped check rows and the final count summary.
    #[arg(long, default_value_t = false)]
    summary: bool,

    /// Disable ANSI color in human output.
    #[arg(long, default_value_t = false)]
    no_color: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
enum CheckStatus {
    Ok,
    Warning,
    Fail,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DoctorReport {
    schema_version: u32,
    generated_at: String,
    overall_status: CheckStatus,
    loom_version: String,
    checks: Vec<DoctorCheck>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct DoctorCheck {
    id: String,
    category: String,
    status: CheckStatus,
    summary: String,
    details: Vec<String>,
    remediation: Option<String>,
    #[serde(skip_serializing)]
    duration_ms: u64,
}

fn status_icon(status: CheckStatus, ascii: bool) -> &'static str {
    if ascii {
        match status {
            CheckStatus::Ok => "[OK]",
            CheckStatus::Warning => "[WARN]",
            CheckStatus::Fail => "[FAIL]",
        }
    } else {
        match status {
            CheckStatus::Ok => "\u{2714}",     // heavy check mark
            CheckStatus::Warning => "\u{26a0}", // warning sign
            CheckStatus::Fail => "\u{2718}",     // heavy ballot x
        }
    }
}

/// Render a human-readable report.
fn render_human_report(report: &DoctorReport, summary_only: bool, ascii: bool) {
    let _color_enabled = supports_color::on(Stream::Stdout).is_some();

    println!("Loom Doctor — version {}", report.loom_version);
    println!();

    let mut ok_count = 0;
    let mut warn_count = 0;
    let mut fail_count = 0;

    let categories: Vec<&str> = report
        .checks
        .iter()
        .map(|c| c.category.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();

    for category in &categories {
        let checks: Vec<_> = report
            .checks
            .iter()
            .filter(|c| c.category == *category)
            .collect();

        if checks.is_empty() {
            continue;
        }

        println!("  {category}:");

        for check in &checks {
            match check.status {
                CheckStatus::Ok => ok_count += 1,
                CheckStatus::Warning => warn_count += 1,
                CheckStatus::Fail => fail_count += 1,
            }

            let icon = status_icon(check.status, ascii);
            println!("    {icon} {}", check.summary);

            if !summary_only && !check.details.is_empty() {
                for detail in &check.details {
                    println!("       {detail}");
                }
            }

            if let Some(remediation) = &check.remediation {
                if !summary_only {
                    println!("       Fix: {remediation}");
                }
            }
        }
        println!();
    }

    println!(
        "Results: {ok_count} passed, {warn_count} warnings, {fail_count} failures"
    );
}

pub async fn run_doctor(
    doctor_cli: DoctorCommand,
    _root_config_overrides: CliConfigOverrides,
    _interactive: &TuiCli,
    _arg0_paths: &Arg0DispatchPaths,
) -> anyhow::Result<()> {
    let mut checks = Vec::new();
    let _start = std::time::Instant::now();

    // Check 1: Loom home directory
    let home_check = check_loom_home().await;
    checks.push(home_check);

    // Check 2: Config file
    let config_check = check_config().await;
    checks.push(config_check);

    // Check 3: Database files
    let db_check = check_database().await;
    checks.push(db_check);

    // Check 4: Terminal
    let term_check = check_terminal();
    checks.push(term_check);

    let overall_status = checks
        .iter()
        .map(|c| c.status)
        .max()
        .unwrap_or(CheckStatus::Ok);

    let report = DoctorReport {
        schema_version: 1,
        generated_at: chrono::Utc::now().to_rfc3339(),
        overall_status,
        loom_version: env!("CARGO_PKG_VERSION").to_string(),
        checks,
    };

    if doctor_cli.json {
        let json = serde_json::to_string_pretty(&report)?;
        println!("{json}");
    } else {
        render_human_report(&report, doctor_cli.summary, false);
    }

    if overall_status == CheckStatus::Fail {
        std::process::exit(1);
    }

    Ok(())
}

async fn check_loom_home() -> DoctorCheck {
    let start = std::time::Instant::now();
    match loom_tui_stubs::config::find_codex_home() {
        Ok(home) => {
            let exists = home.exists();
            DoctorCheck {
                id: "loom-home".to_string(),
                category: "Installation".to_string(),
                status: if exists {
                    CheckStatus::Ok
                } else {
                    CheckStatus::Warning
                },
                summary: format!("Loom home: {}", home.display()),
                details: if exists {
                    vec![format!("Directory exists at {}", home.display())]
                } else {
                    vec![format!("Directory does not exist yet at {}", home.display())]
                },
                remediation: if !exists {
                    Some("Run `loom` to initialize the home directory".to_string())
                } else {
                    None
                },
                duration_ms: start.elapsed().as_millis() as u64,
            }
        }
        Err(e) => DoctorCheck {
            id: "loom-home".to_string(),
            category: "Installation".to_string(),
            status: CheckStatus::Fail,
            summary: "Cannot resolve Loom home directory".to_string(),
            details: vec![format!("Error: {e}")],
            remediation: Some("Set CODEX_HOME environment variable to a writable directory".to_string()),
            duration_ms: start.elapsed().as_millis() as u64,
        },
    }
}

async fn check_config() -> DoctorCheck {
    let start = std::time::Instant::now();
    let home = match loom_tui_stubs::config::find_codex_home() {
        Ok(h) => h,
        Err(e) => {
            return DoctorCheck {
                id: "config".to_string(),
                category: "Configuration".to_string(),
                status: CheckStatus::Fail,
                summary: "Cannot check config: home directory not found".to_string(),
                details: vec![format!("Error: {e}")],
                remediation: Some("Set CODEX_HOME first".to_string()),
                duration_ms: start.elapsed().as_millis() as u64,
            };
        }
    };

    let config_path = home.join("config.toml");
    let exists = tokio::fs::try_exists(&config_path).await.unwrap_or(false);

    DoctorCheck {
        id: "config".to_string(),
        category: "Configuration".to_string(),
        status: if exists {
            CheckStatus::Ok
        } else {
            CheckStatus::Warning
        },
        summary: if exists {
            format!("Config file exists: {}", config_path.display())
        } else {
            format!("No config file at {}", config_path.display())
        },
        details: if exists {
            vec!["Configuration file found".to_string()]
        } else {
            vec!["Using default configuration".to_string()]
        },
        remediation: if !exists {
            Some("Create a config.toml in your Loom data directory, or use defaults".to_string())
        } else {
            None
        },
        duration_ms: start.elapsed().as_millis() as u64,
    }
}

async fn check_database() -> DoctorCheck {
    let start = std::time::Instant::now();
    let home = match loom_tui_stubs::config::find_codex_home() {
        Ok(h) => h,
        Err(e) => {
            return DoctorCheck {
                id: "database".to_string(),
                category: "Storage".to_string(),
                status: CheckStatus::Fail,
                summary: "Cannot check database: home directory not found".to_string(),
                details: vec![format!("Error: {e}")],
                remediation: Some("Set CODEX_HOME first".to_string()),
                duration_ms: start.elapsed().as_millis() as u64,
            };
        }
    };

    let db_path = home.join("state.db");
    let exists = tokio::fs::try_exists(&db_path).await.unwrap_or(false);

    DoctorCheck {
        id: "database".to_string(),
        category: "Storage".to_string(),
        status: if exists {
            CheckStatus::Ok
        } else {
            CheckStatus::Ok // Not having a DB yet is fine
        },
        summary: if exists {
            format!("State database exists: {}", db_path.display())
        } else {
            "No state database yet (will be created on first run)".to_string()
        },
        details: Vec::new(),
        remediation: None,
        duration_ms: start.elapsed().as_millis() as u64,
    }
}

fn check_terminal() -> DoctorCheck {
    let start = std::time::Instant::now();
    let info = loom_terminal_detection::terminal_info();

    let status = if info.name == loom_terminal_detection::TerminalName::Dumb {
        CheckStatus::Warning
    } else {
        CheckStatus::Ok
    };

    let summary = format!(
        "Terminal: {:?}",
        info.name,
    );

    let mut details = Vec::new();
    if info.name == loom_terminal_detection::TerminalName::Dumb {
        details.push("TERM=dumb may cause display issues".to_string());
    }
    if let Some(tmux) = &info.multiplexer {
        details.push(format!("Multiplexer: {tmux:?}"));
    }

    DoctorCheck {
        id: "terminal".to_string(),
        category: "Environment".to_string(),
        status,
        summary,
        details,
        remediation: if info.name == loom_terminal_detection::TerminalName::Dumb {
            Some("Set TERM to a supported value (e.g. xterm-256color)".to_string())
        } else {
            None
        },
        duration_ms: start.elapsed().as_millis() as u64,
    }
}
