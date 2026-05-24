use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use openloom_models::ChatMessage;
use openloom_skills::cron_store::{AutomationExecutor, CronStore, NotificationStore};

use super::Engine;

/// Start the cron scheduler loop. Polls every 60 seconds for due jobs
/// and executes them: agent-session jobs go through the engine,
/// direct-action notify jobs create notification records.
pub fn spawn_cron_scheduler(
    engine: Arc<Engine>,
    cron_store: Arc<CronStore>,
    notification_store: Arc<NotificationStore>,
) {
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;

        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;

            if engine.draining.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }

            let now_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;

            let due = cron_store.get_due_jobs(now_ms);
            for job in &due {
                match &job.executor {
                    Some(AutomationExecutor::DirectAction { action, title, body, .. })
                        if action == "notify" =>
                    {
                        // Direct notification — no agent needed
                        let t = title.as_deref().unwrap_or(&job.label);
                        let b = body.as_deref().unwrap_or("");
                        tracing::info!(
                            id = %job.id,
                            label = %job.label,
                            title = %t,
                            "Automation notification firing"
                        );
                        match notification_store.add(&job.id, t, b) {
                            Ok(record) => {
                                tracing::info!(notif_id = %record.id, "Notification stored");
                                let _ = cron_store.mark_run(&job.id, now_ms, true);
                            }
                            Err(e) => {
                                tracing::error!(id = %job.id, error = %e, "Failed to store notification");
                                let _ = cron_store.mark_run(&job.id, now_ms, false);
                            }
                        }
                    }
                    _ => {
                        // Legacy agent-session or unknown executor
                        tracing::info!(
                            id = %job.id,
                            label = %job.label,
                            "Cron/automation job firing (agent session)"
                        );

                        let msg = ChatMessage {
                            role: "user".into(),
                            content: format!(
                                "[定时任务: {}]\n\n{}",
                                job.label, job.prompt
                            ),
                            timestamp: chrono::Utc::now(),
            id: None,
            seq: None,
                            metadata: None,
                        };

                        let session_id = format!("cron-{}", job.id);
                        let result = engine
                            .handle_message(
                                msg,
                                &session_id,
                                openloom_models::Mode::Code,
                                openloom_models::ModelPreference::default(),
                            )
                            .await;

                        let success = result.is_ok();
                        if let Err(ref e) = result {
                            tracing::warn!(
                                id = %job.id,
                                error = %e,
                                "Cron job failed"
                            );
                        }
                        if let Err(e) = cron_store.mark_run(&job.id, now_ms, success) {
                            tracing::error!(id = %job.id, error = %e, "Failed to mark cron job run");
                        }
                    }
                }
            }
        }
    });
}
