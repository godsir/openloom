use openloom_memory::aggregator::PatternAggregator;
use openloom_memory::extractor::RuleBasedExtractor;
use openloom_memory::pipeline::MemoryPipeline;
use openloom_memory::store::SqliteEventStore;
use std::path::PathBuf;
use std::sync::mpsc;

pub struct ProcessRequest {
    pub session_id: String,
    pub text: String,
    pub context: String,
}

/// Spawn a dedicated thread for MemoryPipeline (rusqlite Connection is not Send)
pub fn spawn_memory_thread(
    db_path: PathBuf,
    threshold: usize,
) -> mpsc::Sender<ProcessRequest> {
    let (tx, rx) = mpsc::channel::<ProcessRequest>();

    std::thread::spawn(move || {
        let extractor = RuleBasedExtractor::with_default_rules();
        let aggregator = PatternAggregator::new(threshold);
        let store = SqliteEventStore::open_with_migrations(&db_path)
            .expect("failed to open database with migrations");

        let mut pipeline = MemoryPipeline::new(extractor, aggregator, store);

        for req in rx {
            tracing::debug!(
                session = %req.session_id,
                text_len = req.text.len(),
                "memory pipeline processing"
            );
            if let Err(e) = pipeline.process(&req.session_id, &req.text, &req.context) {
                tracing::error!(
                    session = %req.session_id,
                    error = %e,
                    "memory pipeline error"
                );
            }
        }
    });

    tx
}
