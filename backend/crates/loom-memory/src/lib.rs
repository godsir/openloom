// SPDX-License-Identifier: Apache-2.0
//! Memory kernel — event storage, cognition pipeline, knowledge graph, persona.
//!
//! Ported from `crates/memory/` with updated imports for loom-types.

pub mod aggregator;
pub mod config_db;
pub mod consolidation;
pub mod extractor;
pub mod graph;
pub mod layers;
pub mod memory_db;
pub mod pattern;
pub mod persona;
pub mod pipeline;
pub mod session_db;
pub mod store;
pub mod summary;
pub mod todo_store;

pub use consolidation::{ConsolidationReport, MemoryConsolidator};
pub use extractor::{
    EntityExtractor, ExtractedEntity, ExtractedRelationship, LLM_EXTRACTION_PROMPT,
    RuleBasedEntityExtractor, parse_llm_extraction,
};
pub use graph::{
    DEFAULT_EMBEDDING_DIM, GraphStore, PruningResult, blob_to_f32_vec, compute_health_score,
    cosine_similarity, f32_slice_to_blob,
};
pub use layers::{Layer, LayerConfig};
pub use pattern::{
    LearningPath, SessionPatternDetector, SessionPatternReport, TimePattern, ToolPreference,
    TopicPattern,
};
pub use persona::{
    Approach, CommunicationStyle, Formality, Goal, GoalStatus, Preference, ProficiencyLevel,
    RichPersona, RichPersonaProvider, TechProficiency, Verbosity, WorkingStyle,
};
pub use pipeline::{
    ConceptCluster, GeneralizationReport, PipelineForgettingReport, PruningEntry,
    detect_concept_clusters, evaluate_session_quality, run_active_forgetting,
};
pub use store::{
    AgentConfigStore, CognitionRow, CognitionSnapshot, CognitionStore, EventRow, McpConfigStore,
    McpServerRow, ModelConfigStore, NewEvent, TeamConfigStore,
};
pub use summary::SummaryEngine;
pub use todo_store::{TodoItem, TodoStore};
