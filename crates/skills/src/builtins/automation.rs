use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use serde_json::{Value, json};

use crate::cron_store::{AutomationExecutor, CronJob, CronStore, NotificationStore, ScheduleType};
use crate::{Skill, SkillManifest, SkillPermissions};

pub struct AutomationSkill {
    store: Arc<CronStore>,
    notifications: Arc<NotificationStore>,
}

impl AutomationSkill {
    pub fn new(store: Arc<CronStore>, data_dir: &std::path::Path) -> Self {
        Self {
            store,
            notifications: Arc::new(NotificationStore::new(data_dir)),
        }
    }
}

#[async_trait::async_trait]
impl Skill for AutomationSkill {
    fn name(&self) -> &str {
        "automation"
    }

    fn manifest(&self) -> &SkillManifest {
        static M: std::sync::OnceLock<SkillManifest> = std::sync::OnceLock::new();
        M.get_or_init(|| SkillManifest {
            name: "automation".into(),
            description: "Create and manage scheduled automation jobs with pre-determined actions. Create notifications that fire on a schedule without needing the agent at trigger time. Use 'list' to see jobs, 'add_notify' to schedule a notification, 'remove' to delete, 'toggle' to enable/disable.".into(),
            triggers: vec![
                "自动化".into(),
                "通知".into(),
                "定时通知".into(),
                "提醒我".into(),
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
            "add_notify" => self.add_notify(&params),
            "remove" => self.remove(&params),
            "toggle" => self.toggle(&params),
            "notifications" => self.list_notifications(),
            _ => Ok(json!({
                "error": "unknown action",
                "available": ["list", "add_notify", "remove", "toggle", "notifications"]
            })),
        }
    }

    fn context_md(&self) -> &str {
        "automation: create scheduled notifications and plugin actions."
    }
}

impl AutomationSkill {
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
                let mut obj = json!({
                    "id": j.id,
                    "label": j.label,
                    "schedule_type": j.schedule_type,
                    "schedule": j.schedule,
                    "enabled": j.enabled,
                    "last_run_at": j.last_run_at,
                    "next_run_at": j.next_run_at,
                });
                if let Some(ref executor) = j.executor
                    && let Some(obj_mut) = obj.as_object_mut()
                {
                    obj_mut.insert("executor".into(), json!(executor));
                }
                obj
            })
            .collect();
        Ok(json!({"jobs": items}))
    }

    fn add_notify(&self, params: &Value) -> Result<Value> {
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
                }));
            }
        };

        let schedule = params
            .get("schedule")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if schedule.is_empty() {
            return Ok(json!({"error": "'schedule' is required."}));
        }

        let title = params
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Reminder")
            .to_string();

        let body = params
            .get("body")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let label = params
            .get("label")
            .and_then(|v| v.as_str())
            .unwrap_or(&title)
            .to_string();

        let channels: Vec<String> = params
            .get("channels")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let now = Self::now_ms();
        let job = CronJob {
            id: String::new(),
            label,
            schedule_type,
            schedule,
            prompt: format!("[Notification] {}: {}", title, body),
            enabled: true,
            created_at: now,
            last_run_at: None,
            next_run_at: None,
            consecutive_errors: 0,
            model: None,
            executor: Some(AutomationExecutor::DirectAction {
                action: "notify".into(),
                title: Some(title),
                body: if body.is_empty() { None } else { Some(body) },
                channels,
            }),
        };

        let next = job.calc_next_run(now);
        if next.is_none() && job.schedule_type == ScheduleType::At {
            return Ok(json!({
                "error": "The scheduled time is in the past. Use a future ISO datetime."
            }));
        }

        match self.store.add(job) {
            Ok(id) => Ok(json!({
                "ok": true,
                "id": id,
                "next_run_at": next,
                "message": format!("Notification scheduled. Next run at timestamp {}", next.unwrap_or(0))
            })),
            Err(e) => Err(anyhow::anyhow!("Failed to save: {}", e)),
        }
    }

    fn remove(&self, params: &Value) -> Result<Value> {
        let id = params.get("id").and_then(|v| v.as_str()).unwrap_or("");

        if id.is_empty() {
            return Ok(json!({"error": "'id' is required. Use 'list' to see job IDs."}));
        }

        match self.store.remove(id) {
            Ok(true) => Ok(json!({"ok": true, "message": format!("Job '{}' removed.", id)})),
            Ok(false) => Ok(json!({"error": format!("Job '{}' not found.", id)})),
            Err(e) => Err(anyhow::anyhow!("Failed to remove: {}", e)),
        }
    }

    fn toggle(&self, params: &Value) -> Result<Value> {
        let id = params.get("id").and_then(|v| v.as_str()).unwrap_or("");

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
            Err(e) => Err(anyhow::anyhow!("Failed to toggle: {}", e)),
        }
    }

    fn list_notifications(&self) -> Result<Value> {
        let records = self.notifications.list_unread();
        let items: Vec<Value> = records
            .iter()
            .map(|r| {
                json!({
                    "id": r.id,
                    "job_id": r.job_id,
                    "title": r.title,
                    "body": r.body,
                    "created_at": r.created_at,
                })
            })
            .collect();
        Ok(json!({"notifications": items}))
    }
}
