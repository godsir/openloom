//! Knowledge graph wire types — serializable equivalents of loom-memory GraphRow/ScoredEntity.
//! Consumers: loom-server (RPC responses), frontend (bindings), loom-cli (display).

use serde::{Deserialize, Serialize};

/// A single entity node in the knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KgNode {
    pub node_id: i64,
    pub name: String,
    pub entity_type: String,
    pub description: String,
    pub confidence: f64,
    pub scope: String,
    /// Memory layer: "working", "episodic", "semantic", or "global".
    /// Omitted from older stores; defaults to "semantic" via serde.
    #[serde(default = "default_layer")]
    pub layer: String,
    /// Cosine similarity score (0.0-1.0), set by vector search. Omitted in
    /// regular graph queries (defaults to 0.0 via serde).
    #[serde(default)]
    pub similarity: f64,
}

fn default_layer() -> String {
    "semantic".to_string()
}

/// A directed relationship between two KG entities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KgEdge {
    pub source: String,
    pub target: String,
    pub relation_type: String,
    pub fact: String,
    pub confidence: f64,
}

/// A subgraph fragment — nodes + edges returned by neighbors/walk queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KgGraph {
    pub nodes: Vec<KgNode>,
    pub edges: Vec<KgEdge>,
}

/// Summary statistics for the knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KgStats {
    pub node_count: usize,
    pub edge_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cognition {
    pub id: i64,
    pub subject: String,
    pub trait_name: String,
    pub value: String,
    pub confidence: f64,
    pub evidence_count: usize,
    pub first_seen: i64,
    pub last_updated: i64,
    pub version: i64,
    pub scope: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CognitionHistory {
    pub id: i64,
    pub version: i64,
    pub trait_name: String,
    pub value: String,
    pub confidence: f64,
    pub evidence_count: usize,
    pub snapshot_at: i64,
}

/// Memory quality self-evaluation report — sent to the frontend settings page.
/// Canonical type live in `loom_types::kg`; `loom_memory::graph` imports it from here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryQualityReport {
    // Injection quality
    pub avg_relevance: f64,
    pub injection_count: i64,
    pub turns_with_references: i64,
    // Entity health
    pub total_entities: i64,
    pub duplicate_rate: f64,
    pub stale_entity_count: i64,
    pub avg_confidence: f64,
    // Coverage
    pub entity_types_distribution: Vec<(String, i64)>,
    pub layer_distribution: Vec<(String, i64)>,
    // Freshness
    pub entities_added_recently: i64,
    pub entities_accessed_recently: i64,
    // Consolidation
    pub consolidation_runs: i64,
    pub total_merged: i64,
    // Score
    pub health_score: f64,
}
