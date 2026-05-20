use openloom_models::{EngineEvent, PersonaProvider};
use std::sync::Arc;
use tokio::sync::broadcast;

pub(crate) fn spawn(persona: Arc<dyn PersonaProvider>, event_bus: broadcast::Sender<EngineEvent>) {
    let mut rx = event_bus.subscribe();
    tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            if matches!(event, EngineEvent::CognitionUpdated { .. }) {
                persona.invalidate();
            }
        }
    });
}
