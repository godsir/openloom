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
pub mod session_db;
pub mod store;
pub mod summary;

pub use extractor::{
    EntityExtractor, ExtractedEntity, ExtractedRelationship, LLM_EXTRACTION_PROMPT,
    RuleBasedEntityExtractor, parse_llm_extraction,
};
pub use graph::GraphStore;
pub use graph::{
    DEFAULT_EMBEDDING_DIM, blob_to_f32_vec, cosine_similarity, f32_slice_to_blob,
};
pub use layers::{Layer, LayerConfig};
pub use persona::{
    Approach, CommunicationStyle, Formality, Goal, GoalStatus, Preference, ProficiencyLevel,
    RichPersona, RichPersonaProvider, TechProficiency, Verbosity, WorkingStyle,
};
pub use consolidation::{ConsolidationReport, MemoryConsolidator};
pub use pattern::{
    LearningPath, SessionPatternDetector, SessionPatternReport, TimePattern, ToolPreference,
    TopicPattern,
};
pub use store::{
    AgentConfigStore, CognitionRow, CognitionSnapshot, CognitionStore, EventRow, McpConfigStore,
    McpServerRow, ModelConfigStore, NewEvent,
};
pub use summary::SummaryEngine;
