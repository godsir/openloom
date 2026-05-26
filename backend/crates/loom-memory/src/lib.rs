// SPDX-License-Identifier: Apache-2.0
//! Memory kernel — event storage, cognition pipeline, knowledge graph, persona.
//!
//! Ported from `crates/memory/` with updated imports for loom-types.

pub mod store;
pub mod pipeline;
pub mod extractor;
pub mod aggregator;
pub mod persona;
pub mod graph;

pub use store::{AgentConfigStore, CognitionRow, CognitionSnapshot, CognitionStore, EventRow, NewEvent, SqliteEventStore};
pub use pipeline::{MemoryPipeline, PipelineConfig};
pub use graph::GraphStore;
pub use extractor::{EntityExtractor, ExtractedEntity, ExtractedRelationship, LLM_EXTRACTION_PROMPT, RuleBasedEntityExtractor, parse_llm_extraction};
pub use persona::CognitionsPersonaProvider;
