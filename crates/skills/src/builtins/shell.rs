use crate::{Skill, SkillManifest, SkillPermissions};
use anyhow::Result;
use serde_json::{Value, json};
use std::process::Command;

pub struct Shell;

#[async_trait::async_trait]
impl Skill for Shell {
    fn name(&self) -> &str {
        "shell"
    }

    fn manifest(&self) -> &SkillManifest {
        static M: std::sync::OnceLock<SkillManifest> = std::sync::OnceLock::new();
        M.get_or_init(|| SkillManifest {
            name: "shell".into(),
            description: "Execute a shell command and return output".into(),
            triggers: vec![],
            permissions: SkillPermissions {
                shell: true,
                subprocess: true,
                ..Default::default()
            },
            min_engine_version: "0.1.0".into(),
        })
    }

    async fn invoke(&self, params: Value) -> Result<Value> {
        let command = params.get("command").and_then(|v| v.as_str()).unwrap_or("");
        let cwd = params.get("cwd").and_then(|v| v.as_str());
        let timeout_ms = params
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(120_000);

        if command.is_empty() {
            return Ok(json!({"error": "command is required"}));
        }

        // Build the OS-appropriate command.
        // On Windows we prefer PowerShell over cmd for better Unicode/quoting support.
        // If the model already wrote "powershell …" or "cmd /c …", pass through as-is.
        let cmd_lower = command.to_ascii_lowercase();
        let mut cmd = if cfg!(target_os = "windows") {
            if cmd_lower.starts_with("powershell") || cmd_lower.starts_with("cmd") {
                // Model provided an explicit shell invocation — run directly via cmd /C
                let mut c = Command::new("cmd");
                c.args(["/C", command]);
                c
            } else {
                // Wrap in powershell for better Unicode and quoting support
                let mut c = Command::new("powershell");
                c.args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-Command",
                    command,
                ]);
                c
            }
        } else {
            let mut c = Command::new("bash");
            c.arg("-c").arg(command);
            c
        };

        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }

        let handle = tokio::task::spawn_blocking(move || {
            let start = std::time::Instant::now();
            let output = cmd.output();
            let elapsed = start.elapsed().as_millis() as u64;
            (output, elapsed)
        });

        let result =
            tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), handle).await;

        match result {
            Ok(Ok((Ok(output), elapsed))) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                Ok(json!({
                    "stdout": truncate_output(&stdout, 10000),
                    "stderr": truncate_output(&stderr, 3000),
                    "exit_code": output.status.code().unwrap_or(-1),
                    "timed_out": false,
                    "elapsed_ms": elapsed
                }))
            }
            Ok(Ok((Err(e), _))) => Ok(json!({
                "error": format!("Command failed to start: {}", e),
                "timed_out": false
            })),
            Ok(Err(e)) => Ok(json!({
                "error": format!("Task error: {}", e),
                "timed_out": false
            })),
            Err(_) => Ok(json!({
                "error": "Command timed out",
                "timed_out": true,
                "timeout_ms": timeout_ms
            })),
        }
    }

    fn context_md(&self) -> &str {
        "Execute shell command. Params: {\"command\": \"...\", \"cwd\": \".\", \"timeout_ms\": 120000}. Returns stdout, stderr, exit_code."
    }
}

fn truncate_output(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let half = max / 2;
        format!(
            "{}...\n[{} chars truncated]\n...{}",
            &s[..half],
            s.len() - max,
            &s[s.len() - half..]
        )
    }
}
