use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use serde_json::{json, Value};

use crate::cron_store::{CronJob, CronStore, ScheduleType};
use crate::{Skill, SkillManifest, SkillPermissions};

pub struct CronSkill {
    store: Arc<CronStore>,
}

impl CronSkill {
    pub fn new(store: Arc<CronStore>) -> Self {
        Self { store }
    }
}

#[async_trait::async_trait]
impl Skill for CronSkill {
    fn name(&self) -> &str {
        "cron"
    }

    fn manifest(&self) -> &SkillManifest {
        static M: std::sync::OnceLock<SkillManifest> = std::sync::OnceLock::new();
        M.get_or_init(|| SkillManifest {
            name: "cron".into(),
            description: "Create and manage scheduled tasks. Use 'at' for one-shot timers, 'every' for recurring intervals (in minutes), or 'cron' for 5-field cron expressions.".into(),
            triggers: vec![
                "定时".into(),
                "计划".into(),
                "提醒".into(),
                "安排".into(),
                "时间".into(),
            ],
            permissions: SkillPermissions::default(),
            min_engine_version: "0.1.0".into(),
        })
    }

    async fn invoke(&self, params: Value) -> Result<Value> {
        let action = params
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("list");

        match action {
            "list" => self.list(),
            "add" => self.add(&params),
            "remove" => self.remove(&params),
            "toggle" => self.toggle(&params),
            _ => Ok(json!({
                "error": "unknown action",
                "available": ["list", "add", "remove", "toggle"]
            })),
        }
    }

    fn context_md(&self) -> &str {
        "cron: manage scheduled tasks with at/every/cron schedules."
    }
}

impl CronSkill {
    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    fn list(&self) -> Result<Value> {
        let jobs = self.store.list_all();
        let items: Vec<Value> = jobs
            .iter()
            .map(|j| {
                json!({
                    "id": j.id,
                    "label": j.label,
                    "schedule_type": j.schedule_type,
                    "schedule": j.schedule,
                    "prompt": j.prompt,
                    "enabled": j.enabled,
                    "last_run_at": j.last_run_at,
                    "next_run_at": j.next_run_at,
                    "consecutive_errors": j.consecutive_errors,
                })
            })
            .collect();
        Ok(json!({"jobs": items}))
    }

    fn add(&self, params: &Value) -> Result<Value> {
        let schedule_type_str = params
            .get("schedule_type")
            .and_then(|v| v.as_str())
            .unwrap_or("every");

        let schedule_type = match schedule_type_str {
            "at" => ScheduleType::At,
            "every" => ScheduleType::Every,
            "cron" => ScheduleType::Cron,
            other => {
                return Ok(json!({
                    "error": format!("Unknown schedule_type '{}'. Use: at, every, cron", other)
                }))
            }
        };

        let schedule = params
            .get("schedule")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if schedule.is_empty() {
            return Ok(json!({"error": "'schedule' is required. Examples: '2026-06-01T09:00:00' for at, '60' for every (minutes), '0 9 * * *' for cron"}));
        }

        let prompt = params
            .get("prompt")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if prompt.is_empty() {
            return Ok(json!({"error": "'prompt' is required — what should the agent do when the job fires?"}));
        }

        let label = params
            .get("label")
            .and_then(|v| v.as_str())
            .unwrap_or(&prompt[..prompt.len().min(40)])
            .to_string();

        let model = params
            .get("model")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let now = Self::now_ms();
        let job = CronJob {
            id: String::new(), // assigned by store
            label,
            schedule_type,
            schedule,
            prompt,
            enabled: true,
            created_at: now,
            last_run_at: None,
            next_run_at: None,
            consecutive_errors: 0,
            model,
            executor: None,
        };

        // Validate schedule by computing next run
        let next = job.calc_next_run(now);
        if next.is_none() && job.schedule_type == ScheduleType::At {
            return Ok(json!({
                "error": "The scheduled time is in the past. Use a future ISO datetime, e.g. '2026-06-01T09:00:00'"
            }));
        }

        match self.store.add(job) {
            Ok(id) => Ok(json!({
                "ok": true,
                "id": id,
                "next_run_at": next,
                "message": format!("Job created. Next run at timestamp {}", next.unwrap_or(0))
            })),
            Err(e) => Err(anyhow::anyhow!("Failed to save job: {}", e)),
        }
    }

    fn remove(&self, params: &Value) -> Result<Value> {
        let id = params
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if id.is_empty() {
            return Ok(json!({"error": "'id' is required. Use 'list' to see job IDs."}));
        }

        match self.store.remove(id) {
            Ok(true) => Ok(json!({"ok": true, "message": format!("Job '{}' removed.", id)})),
            Ok(false) => Ok(json!({"error": format!("Job '{}' not found.", id)})),
            Err(e) => Err(anyhow::anyhow!("Failed to remove job: {}", e)),
        }
    }

    fn toggle(&self, params: &Value) -> Result<Value> {
        let id = params
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if id.is_empty() {
            return Ok(json!({"error": "'id' is required. Use 'list' to see job IDs."}));
        }

        match self.store.toggle(id) {
            Ok(Some(new_state)) => Ok(json!({
                "ok": true,
                "enabled": new_state,
                "message": format!("Job '{}' {}.", id, if new_state { "enabled" } else { "disabled" })
            })),
            Ok(None) => Ok(json!({"error": format!("Job '{}' not found.", id)})),
            Err(e) => Err(anyhow::anyhow!("Failed to toggle job: {}", e)),
        }
    }
}
