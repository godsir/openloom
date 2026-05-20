use openloom_inference::{CompletionRequest, InferenceEngine};
use openloom_models::{AgentState, EngineEvent};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::{RwLock, broadcast};

pub(crate) fn spawn_hub_heartbeat(
    inference: Arc<InferenceEngine>,
    agent_state: Arc<RwLock<AgentState>>,
    event_bus: broadcast::Sender<EngineEvent>,
    last_user_message: Arc<Mutex<Instant>>,
    hb_interval: u64,
    hb_idle_threshold: u64,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(hb_interval));
        // Skip first immediate tick
        interval.tick().await;

        loop {
            interval.tick().await;

            // Skip if agent is busy
            if *agent_state.read().await != AgentState::Idle {
                continue;
            }

            // Check idle time
            let idle_minutes = {
                let last = last_user_message.lock().unwrap();
                last.elapsed().as_secs() / 60
            };
            if idle_minutes < hb_idle_threshold {
                continue;
            }

            // Single-token inference check
            let prompt = format!(
                "User idle {} min. Should agent take action? Reply ONLY yes or no.",
                idle_minutes
            );
            match inference
                .complete(CompletionRequest {
                    prompt,
                    max_tokens: 1,
                    temperature: 0.0,
                    top_p: 1.0,
                    stop: vec!["\n".into()],
                    stream: false,
                })
                .await
            {
                Ok(resp) if resp.text.trim().to_lowercase().contains("yes") => {
                    let _ = event_bus.send(EngineEvent::HeartbeatTick {
                        idle_minutes,
                        event_count: 0,
                        suggested_action: None,
                    });
                }
                _ => {} // model unavailable or answered "no" -- skip
            }
        }
    });
}
