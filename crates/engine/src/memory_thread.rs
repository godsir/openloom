use openloom_memory::aggregator::PatternAggregator;
use openloom_memory::extractor::RuleBasedExtractor;
use openloom_memory::pipeline::MemoryPipeline;
use openloom_memory::store::{CognitionStore, SqliteEventStore};
use openloom_models::EngineEvent;
use std::path::PathBuf;
use std::sync::mpsc;
use tokio::sync::broadcast;

pub struct ProcessRequest {
    pub session_id: String,
    pub text: String,
    pub context: String,
}

/// Spawn a dedicated thread for MemoryPipeline (rusqlite Connection is not Send).
/// Returns a channel sender for submitting requests, and broadcasts cognition
/// updates back to the Engine via event_tx.
pub fn spawn_memory_thread(
    db_path: PathBuf,
    threshold: usize,
    event_tx: broadcast::Sender<EngineEvent>,
    _summarizer_path: Option<PathBuf>,
    project_scope: String,
) -> mpsc::Sender<ProcessRequest> {
    let (tx, rx) = mpsc::channel::<ProcessRequest>();

    std::thread::spawn(move || {
        let extractor = RuleBasedExtractor::with_default_rules();
        let aggregator = PatternAggregator::new(threshold);
        let store = SqliteEventStore::open_with_migrations(&db_path)
            .expect("failed to open database with migrations");

        let mut pipeline = MemoryPipeline::new_with_extractor(
            extractor, aggregator, store,
            None, // cognition: RuleBased for now, LlmBased when 8B loads
        );

        for req in rx {
            tracing::debug!(
                session = %req.session_id,
                text_len = req.text.len(),
                "memory pipeline processing"
            );
            match pipeline.process(&req.session_id, &req.text, &req.context, &project_scope) {
                Ok(result) => {
                    if let Some(cog) = result.cognition_triggered {
                        tracing::info!(
                            trait_name = %cog.trait_name,
                            confidence = cog.confidence,
                            scope = %cog.scope,
                            "cognition triggered"
                        );
                        let _ = event_tx.send(EngineEvent::CognitionUpdated {
                            trait_name: cog.trait_name.clone(),
                            old_value: String::new(),
                            new_value: cog.summary.clone(),
                            confidence: cog.confidence,
                        });
                        let cognition_store = CognitionStore::new(pipeline.store().conn());
                        let _ = cognition_store.insert(
                            "USER",
                            &cog.trait_name,
                            &cog.summary,
                            cog.confidence,
                            cog.evidence_count,
                            &cog.scope,
                        );
                    }
                }
                Err(e) => {
                    tracing::error!(
                        session = %req.session_id,
                        error = %e,
                        "memory pipeline error"
                    );
                }
            }
        }
    });

    tx
}
