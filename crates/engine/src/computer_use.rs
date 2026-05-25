// Computer Use provider for Windows (UIAutomation via PowerShell).
//
// Embeds a PowerShell script (uia_helper.ps1) that wraps
// System.Windows.Automation for screenshot capture, accessibility
// tree enumeration, and element actions (click, type, scroll).
//
// Communication: stdin/stdout JSON via powershell.exe subprocess.

use anyhow::{Context, Result};
use std::io::Write;

const UIA_HELPER_SCRIPT: &str = include_str!("uia_helper.ps1");

// ── Public API ──

pub fn check_available() -> bool {
    cfg!(target_os = "windows")
}

pub fn check_status() -> serde_json::Value {
    if !check_available() {
        return serde_json::json!({
            "available": false,
            "reason": "unsupported-platform",
            "permissions": []
        });
    }
    match run_uia(&serde_json::json!({"command": "status"})) {
        Ok(data) => serde_json::json!({
            "available": data.get("available").and_then(|v| v.as_bool()).unwrap_or(true),
            "permissions": data.get("permissions").cloned().unwrap_or(serde_json::json!([])),
        }),
        Err(e) => serde_json::json!({
            "available": false,
            "reason": "powershell-unavailable",
            "error": e.to_string(),
            "permissions": [
                {"name": "screen-capture", "granted": false},
                {"name": "input-control", "granted": false},
                {"name": "clipboard", "granted": true}
            ]
        }),
    }
}

/// List open app windows via UIA.
pub fn list_apps() -> Result<serde_json::Value> {
    let data = run_uia(&serde_json::json!({"command": "list_apps"}))?;
    Ok(data)
}

/// Get screenshot + accessibility tree for a target window.
/// target: { "processId": N, "windowId": N, "appName": "..." }
pub fn get_app_state(target: &serde_json::Value) -> Result<serde_json::Value> {
    run_uia(&serde_json::json!({
        "command": "get_app_state",
        "target": target,
    }))
}

/// Perform an action on an element.
/// action: { "type": "click_element"|"type_text"|"scroll"|"stop",
///           "elementId": "uia:N", "text": "...", "direction": "up"|"down", ... }
pub fn perform_action(
    target: &serde_json::Value,
    action: &serde_json::Value,
) -> Result<serde_json::Value> {
    run_uia(&serde_json::json!({
        "command": "perform_action",
        "target": target,
        "action": action,
    }))
}

// ── Internal ──

fn run_uia(payload: &serde_json::Value) -> Result<serde_json::Value> {
    let script_path = ensure_script()?;
    let payload_json = serde_json::to_string(payload)?;

    let mut child = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            &script_path.to_string_lossy(),
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    if let Some(ref mut stdin) = child.stdin {
        stdin.write_all(payload_json.as_bytes())?;
        stdin.write_all(b"\n")?;
    }

    let result = child.wait_with_output()?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        anyhow::bail!("UIA helper failed: {}", stderr.trim());
    }

    let stdout = String::from_utf8_lossy(&result.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).context("UIA helper returned invalid JSON")?;

    if parsed.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
        Ok(parsed.get("data").cloned().unwrap_or(serde_json::json!({})))
    } else {
        let msg = parsed
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown error");
        anyhow::bail!("{}", msg)
    }
}

fn ensure_script() -> Result<std::path::PathBuf> {
    let dir = std::env::temp_dir().join("loom-computer-use");
    std::fs::create_dir_all(&dir)?;

    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    UIA_HELPER_SCRIPT.hash(&mut h);
    let path = dir.join(format!("loom-uia-{:#016x}.ps1", h.finish()));

    let stale = std::fs::read_to_string(&path).map_or(true, |s| s != UIA_HELPER_SCRIPT);
    if stale {
        std::fs::write(&path, UIA_HELPER_SCRIPT)?;
    }
    Ok(path)
}
