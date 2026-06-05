//! Memory pipeline types — canonical definitions shared between loom-core and loom-memory.
//! Consumers: loom-core (orchestrator, MemoryStore trait), loom-memory (pipeline), loom-cli.

use serde::{Deserialize, Serialize};

/// Pipeline stage identifiers used by the orchestrator to decide what to run next.
/// Canonical source of truth — all crates import this definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PipelineStage {
    Extraction,
    Generalization,
    Consolidation,
    Forgetting,
    QualityAudit,
}

impl std::fmt::Display for PipelineStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PipelineStage::Extraction => write!(f, "Extraction"),
            PipelineStage::Generalization => write!(f, "Generalization"),
            PipelineStage::Consolidation => write!(f, "Consolidation"),
            PipelineStage::Forgetting => write!(f, "Forgetting"),
            PipelineStage::QualityAudit => write!(f, "QualityAudit"),
        }
    }
}

/// Reports the entities and relationships removed during a forgetting cycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForgettingReport {
    pub cycle_timestamp: String,
    pub nodes_removed: usize,
    pub edges_removed: usize,
    pub cognitions_removed: usize,
    pub min_importance_threshold: f64,
    pub max_age_days: i64,
    pub skipped_protected: i64,
    pub summary: String,
}

/// Health snapshot of the memory system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryHealth {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub total_cognitions: usize,
    pub stale_nodes: usize,
    pub orphan_nodes: usize,
    pub layer_distribution: Vec<(String, i64)>,
    pub fragmentation_score: f64,
    pub status: String,
    pub checked_at: String,
}

/// Lightweight quality evaluation for the evaluate_quality pipeline stage.
/// Distinct from MemoryQualityReport (kg.rs) which is the holistic health report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityEvaluation {
    pub lookback_days: i64,
    pub total_injections: usize,
    pub total_references: usize,
    pub recall_rate: f64,
    pub top_entities: Vec<String>,
    pub stale_entities: Vec<String>,
    pub quality_score: f64,
    pub recommendations: Vec<String>,
    pub evaluated_at: String,
}

/// Detected behavioral patterns for self-evolution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorProfile {
    pub preferred_tools: Vec<(String, usize)>,
    pub frequent_topics: Vec<(String, usize)>,
    pub active_hours: Vec<(u32, usize)>,
    pub avg_turn_tokens: usize,
    pub skill_usage: Vec<(String, usize)>,
    pub extracted_at: String,
}
