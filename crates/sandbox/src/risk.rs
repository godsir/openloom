use openloom_models::RiskLevel;
use serde_json::Value;

const FORBIDDEN_PATTERNS: &[&str] = &[
    "rm -rf /",
    "rm -rf ~",
    "rm -rf /*",
    "mkfs",
    "dd if=",
    ":(){:|:&};:",
    "chmod -R 777 /",
    "format c:",
    "del /f /s /q c:\\",
    "> /dev/sda",
    "shutdown",
    "reboot",
    "init 0",
    "halt",
];

const HIGH_RISK_PATTERNS: &[&str] = &[
    "rm -rf",
    "rm -r",
    "rmdir /s",
    "del /s",
    "git push --force",
    "git push -f",
    "git reset --hard",
    "git clean -fd",
    "drop table",
    "drop database",
    "truncate",
    "chmod",
    "chown",
    "sudo",
    "curl | sh",
    "curl | bash",
    "wget -O - | sh",
    "pip install",
    "npm install -g",
];

pub fn classify_risk(tool: &str, params: &Value) -> RiskLevel {
    match tool {
        "file_read" | "file_search" | "content_search" => RiskLevel::Low,
        "file_write" | "file_edit" => RiskLevel::Medium,
        "shell" => {
            let command = params.get("command").and_then(|v| v.as_str()).unwrap_or("");
            let cmd_lower = command.to_lowercase();

            for pattern in FORBIDDEN_PATTERNS {
                if cmd_lower.contains(pattern) {
                    return RiskLevel::Forbidden;
                }
            }

            for pattern in HIGH_RISK_PATTERNS {
                if cmd_lower.contains(pattern) {
                    return RiskLevel::High;
                }
            }

            RiskLevel::Medium
        }
        "web-browser" => RiskLevel::Medium,
        _ => RiskLevel::Medium,
    }
}

pub fn should_block(risk: &RiskLevel, skip_permissions: bool) -> bool {
    match risk {
        RiskLevel::Forbidden => true,
        RiskLevel::High => !skip_permissions,
        RiskLevel::Medium | RiskLevel::Low => false,
    }
}

pub fn risk_message(tool: &str, params: &Value, risk: &RiskLevel) -> String {
    let detail = match tool {
        "shell" => {
            let cmd = params
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            format!("Command: {}", cmd)
        }
        "file_write" | "file_edit" => {
            let path = params.get("path").and_then(|v| v.as_str()).unwrap_or("?");
            format!("File: {}", path)
        }
        _ => format!("Tool: {}", tool),
    };

    match risk {
        RiskLevel::Forbidden => {
            format!("[BLOCKED] Dangerous operation forbidden.\n{}", detail)
        }
        RiskLevel::High => {
            format!(
                "[BLOCKED] High-risk operation requires --dangerously-skip-permissions.\n{}",
                detail
            )
        }
        _ => String::new(),
    }
}
