use crate::{Skill, SkillManifest, SkillPermissions};
use anyhow::Result;
use serde_json::{json, Value};

pub struct ScheduleReminder;

#[async_trait::async_trait]
impl Skill for ScheduleReminder {
    fn name(&self) -> &str { "schedule-reminder" }

    fn manifest(&self) -> &SkillManifest {
        static M: std::sync::OnceLock<SkillManifest> = std::sync::OnceLock::new();
        M.get_or_init(|| SkillManifest {
            name: "schedule-reminder".into(),
            description: "Schedule reminders: manage calendar, set reminders, view agenda".into(),
            triggers: vec![
                "提醒".into(), "日程".into(), "日历".into(), "会议".into(),
                "安排".into(), "定时".into(),
            ],
            permissions: SkillPermissions::default(),
            min_engine_version: "0.1.0".into(),
        })
    }

    async fn invoke(&self, params: Value) -> Result<Value> {
        let action = params.get("action").and_then(|v| v.as_str()).unwrap_or("list");
        match action {
            "add" => {
                let title = params.get("title").and_then(|v| v.as_str()).unwrap_or("untitled");
                let time = params.get("time").and_then(|v| v.as_str()).unwrap_or("unspecified");
                Ok(json!({"added": {"title": title, "time": time}}))
            }
            "list" => {
                Ok(json!({"reminders": [], "note": "Phase 2: persisted reminders"}))
            }
            _ => Ok(json!({"error": "unknown action", "available": ["add", "list"]})),
        }
    }

    fn context_md(&self) -> &str {
        "Schedule reminder skill: add/view/cancel reminders with time parsing."
    }
}
