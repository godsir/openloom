// SPDX-License-Identifier: Apache-2.0
//! Memory kernel — event storage, cognition pipeline, knowledge graph, persona.
//!
//! Ported from `crates/memory/` with updated imports for loom-types.

pub mod aggregator;
pub mod extractor;
pub mod graph;
pub mod persona;
pub mod pipeline;
pub mod store;
pub mod summary;

pub use extractor::{
    EntityExtractor, ExtractedEntity, ExtractedRelationship, LLM_EXTRACTION_PROMPT,
    RuleBasedEntityExtractor, parse_llm_extraction,
};
pub use graph::GraphStore;
pub use persona::CognitionsPersonaProvider;
pub use pipeline::{MemoryPipeline, PipelineConfig};
pub use store::{
    AgentConfigStore, CognitionRow, CognitionSnapshot, CognitionStore, EventRow, McpConfigStore,
    McpServerRow, ModelConfigStore, NewEvent, SqliteEventStore,
};
pub use summary::SummaryEngine;
