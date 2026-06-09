//! Natural-language scheduled task detector.
//!
//! Two-phase pipeline:
//! 1. Regex pre-scan — cheap filter
//! 2. LLM extraction — parse time + AI instruction from the message
//!
//! The extracted task body is an **AI prompt** (not a shell command).
//! When the cron job fires, this prompt is sent to the AI for execution.
//!
//! Supports both Chinese and English input.

use anyhow::Result;
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// Result of task detection on a user message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedTask {
    pub should_create: bool,
    pub schedule_at: Option<String>,
    pub body: Option<String>,
    pub name: Option<String>,
    pub cron_expression: Option<String>,
    pub kind: Option<String>,
    pub confirmation: Option<String>,
}

/// Phase 1: regex pre-scan.
pub fn pre_scan(message: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(
            r"(?ix)
            提醒|定时|闹钟|通知|叫我|叫醒|稍后|之后|到点|分钟后|小时后|秒后|天后|明天|后天|今晚|
            明早|明晚|下周|每天|每天早上|每天晚上|每月|
            remind|reminder|alarm|timer|schedule|scheduled|tomorrow|tonight|
            later\s+at|at\s+\d{1,2}(:\d{2})?|in\s+\d+\s+(?:second|minute|hour|day|week)s?
            "
        ).expect("task detector regex should compile")
    });
    re.is_match(message)
}

/// Build the LLM prompt for extraction.
///
/// The LLM is asked to extract the user's intent and produce an **AI instruction**
/// that will be sent to the AI when the schedule fires. This is NOT a shell command —
/// it's a natural language task description for the AI to execute.
pub fn build_extraction_prompt(message: &str, now: &DateTime<Utc>) -> String {
    format!(
        r#"你是一个任务时间解析器。分析用户消息，判断是否应该创建一个定时任务。

当前时间（UTC）: {now}
用户消息: {message}

以 JSON 格式返回（不要包含其他内容）：
{{
  "shouldCreate": true或false,
  "scheduleAt": "ISO 8601 格式的触发时间，带时区偏移，如 2026-06-09T09:00:00+08:00",
  "aiInstruction": "给 AI 的自然语言任务指令（50字以内）。这是 AI 到时需要执行的任务描述，不是 shell 命令。例如：'检查服务器状态并总结报告'、'提醒用户提交代码'、'搜索今日科技新闻并摘要'",
  "taskName": "简短的任务名称（10字以内）",
  "kind": "at" 表示一次性 / "daily" 表示每天 / "interval" 表示间隔,
  "everyMinutes": 如果kind是interval这里是分钟数,
  "timeOfDay": 如果kind是daily这里是每天触发时间如 "09:00"
}}

规则：
- 只有用户明确表达"创建""设置""帮我""提醒"等意图时才创建
- scheduleAt 必须是未来时间
- aiInstruction 是自然语言，描述 AI 应该做什么，不是 shell 命令
- 如果无法确定具体时间但表达了意图，使用合理默认值
- 仅返回 JSON，不要包含任何解释"#,
        now = now.format("%Y-%m-%dT%H:%M:%SZ"),
        message = message,
    )
}

/// Parse the LLM response into a DetectedTask.
pub fn parse_extraction_response(json_text: &str) -> Result<DetectedTask> {
    let json = json_text
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    let v: serde_json::Value = serde_json::from_str(json)?;
    let should_create = v["shouldCreate"].as_bool().unwrap_or(false);

    if !should_create {
        return Ok(DetectedTask {
            should_create: false,
            schedule_at: None,
            body: None,
            name: None,
            cron_expression: None,
            kind: None,
            confirmation: None,
        });
    }

    let kind = v["kind"].as_str().unwrap_or("at").to_string();
    let schedule_at = v["scheduleAt"].as_str().map(|s| s.to_string());
    // Read aiInstruction (v2) with fallback to reminderBody (v1 compatibility)
    let body = v["aiInstruction"]
        .as_str()
        .or_else(|| v["reminderBody"].as_str())
        .map(|s| s.to_string());
    let name = v["taskName"].as_str().map(|s| s.to_string());

    let cron_expression = match kind.as_str() {
        "daily" => {
            let time = v["timeOfDay"].as_str().unwrap_or("09:00");
            let parts: Vec<&str> = time.split(':').collect();
            if parts.len() == 2 {
                let hour: u32 = parts[0].parse().unwrap_or(9);
                let min: u32 = parts[1].parse().unwrap_or(0);
                Some(format!("0 {} {} * * * *", min, hour))
            } else {
                Some("0 0 9 * * * *".to_string())
            }
        }
        "interval" => {
            let mins = v["everyMinutes"].as_u64().unwrap_or(60);
            Some(format!("0 */{} * * * *", mins))
        }
        _ => {
            if let Some(at) = schedule_at.as_deref() {
                if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(at) {
                    Some(format!(
                        "0 {} {} {} {} {} ? {}",
                        dt.format("%M"),
                        dt.format("%H"),
                        dt.format("%d"),
                        dt.format("%m"),
                        dt.format("%Y"),
                        dt.format("%Y")
                    ))
                } else {
                    None
                }
            } else {
                None
            }
        }
    };

    let task_label = name.as_deref().unwrap_or("定时任务");
    let confirmation = match kind.as_str() {
        "daily" => {
            let t = v["timeOfDay"].as_str().unwrap_or("09:00");
            format!("每天 {} AI 执行「{}」", t, task_label)
        }
        "interval" => {
            let m = v["everyMinutes"].as_u64().unwrap_or(60);
            format!("每 {} 分钟 AI 执行「{}」", m, task_label)
        }
        _ => {
            if let Some(at) = schedule_at.as_deref() {
                if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(at) {
                    format!(
                        "{} AI 执行「{}」",
                        dt.with_timezone(&chrono::Local)
                            .format("%Y-%m-%d %H:%M"),
                        task_label
                    )
                } else {
                    format!("「{}」", task_label)
                }
            } else {
                format!("「{}」", task_label)
            }
        }
    };

    Ok(DetectedTask {
        should_create: true,
        schedule_at,
        body,
        name,
        cron_expression,
        kind: Some(kind),
        confirmation: Some(confirmation),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pre_scan_chinese() {
        assert!(pre_scan("帮我创建一个明天早上九点的闹钟"));
        assert!(pre_scan("半小时后提醒我"));
        assert!(pre_scan("提醒我明天提交代码"));
    }

    #[test]
    fn test_pre_scan_english() {
        assert!(pre_scan("remind me tomorrow at 9am"));
        assert!(pre_scan("set an alarm for 30 minutes"));
    }

    #[test]
    fn test_pre_scan_skip() {
        assert!(!pre_scan("你好"));
        assert!(!pre_scan("帮我写一段代码"));
        assert!(!pre_scan("今天天气怎么样"));
    }

    #[test]
    fn test_parse_not_create() {
        let r = parse_extraction_response(r#"{"shouldCreate": false}"#).unwrap();
        assert!(!r.should_create);
    }

    #[test]
    fn test_parse_daily() {
        let json = r#"{"shouldCreate":true,"kind":"daily","timeOfDay":"08:30","scheduleAt":"2026-06-09T08:30:00+08:00","aiInstruction":"检查今日日程并提醒用户","taskName":"日程提醒"}"#;
        let r = parse_extraction_response(json).unwrap();
        assert!(r.should_create);
        assert_eq!(r.kind.as_deref(), Some("daily"));
        assert_eq!(r.body.as_deref(), Some("检查今日日程并提醒用户"));
        assert!(r.cron_expression.is_some());
    }

    #[test]
    fn test_parse_at() {
        let json = r#"{"shouldCreate":true,"kind":"at","scheduleAt":"2026-06-09T14:00:00+08:00","aiInstruction":"提醒用户提交代码","taskName":"提交提醒"}"#;
        let r = parse_extraction_response(json).unwrap();
        assert!(r.should_create);
        assert_eq!(r.body.as_deref(), Some("提醒用户提交代码"));
    }

    #[test]
    fn test_parse_v1_reminder_body_fallback() {
        // Old v1 format with "reminderBody" instead of "aiInstruction"
        let json = r#"{"shouldCreate":true,"kind":"at","scheduleAt":"2026-06-09T14:00:00+08:00","reminderBody":"提交代码","taskName":"提交提醒"}"#;
        let r = parse_extraction_response(json).unwrap();
        assert!(r.should_create);
        assert_eq!(r.body.as_deref(), Some("提交代码"));
    }
}
