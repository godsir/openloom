//! Memory pipeline — orchestrates event extraction, aggregation, cognition storage,
//! and entity extraction feeding into the knowledge graph.

use std::sync::Mutex;

use anyhow::Result;
use chrono::Utc;

use crate::aggregator::PatternAggregator;
use crate::store::{CognitionStore, NewEvent, SqliteEventStore};

pub struct MemoryPipeline {
    store: SqliteEventStore,
    aggregator: Mutex<PatternAggregator>,
}

/// Configuration for the memory pipeline.
pub struct PipelineConfig {
    pub pattern_threshold: usize,
    pub auto_extract_kg: bool,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            pattern_threshold: 3,
            auto_extract_kg: true,
        }
    }
}

impl MemoryPipeline {
    pub fn new(store: SqliteEventStore) -> Self {
        Self {
            store,
            aggregator: Mutex::new(PatternAggregator::new(3)),
        }
    }

    pub fn with_config(store: SqliteEventStore, config: PipelineConfig) -> Self {
        Self {
            store,
            aggregator: Mutex::new(PatternAggregator::new(config.pattern_threshold)),
        }
    }

    pub fn store(&self) -> &SqliteEventStore {
        &self.store
    }

    /// Process user text through the pipeline.
    /// Returns cognition trait names that were triggered (if any).
    pub async fn process_text(
        &self,
        text: &str,
        session_id: &str,
        user_id: &str,
    ) -> Result<Vec<String>> {
        let now = Utc::now();
        let mut triggered = Vec::new();

        // Stage 1: Record the raw event
        let event = NewEvent {
            timestamp: now,
            event_type: "user_message".into(),
            action: "chat".into(),
            context: text.to_string(),
            confidence: 1.0,
            source_session: Some(session_id.to_string()),
            source_text: text.to_string(),
            payload: None,
        };
        self.store.insert_event(&event)?;

        // Stage 2: Check for pattern triggers via aggregator
        if self.aggregator.lock().unwrap().record("chat") {
            triggered.push("chat_frequency".to_string());
        }

        // Stage 3: If text mentions known topics, record as cognition
        let lower = text.to_lowercase();
        for keyword in &[
            "rust",
            "python",
            "typescript",
            "golang",
            "ai",
            "machine learning",
            "openloom",
            "mcp",
            "lsp",
            "agent",
            "skill",
            "plugin",
        ] {
            if lower.contains(keyword) {
                let cognition = CognitionStore::new(self.store.conn());
                let trait_name = format!("interest_{}", keyword.replace(' ', "_"));
                if let Ok(id) = cognition.insert(user_id, &trait_name, keyword, 0.6, 1, "global") {
                    tracing::debug!(%id, trait_name, keyword, "cognition inserted");
                    triggered.push(trait_name);
                }
            }
        }

        Ok(triggered)
    }
}
