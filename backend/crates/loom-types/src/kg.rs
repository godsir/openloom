//! Knowledge graph wire types — serializable equivalents of loom-memory GraphRow/ScoredEntity.
//! Consumers: loom-server (RPC responses), frontend (bindings), lume-cli (display).

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
