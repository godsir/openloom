//! Memory layer definitions — L0-L3 layered architecture.
//!
//! Layers control retrieval priority, retention policy, and how memories
//! flow between working, episodic, semantic, and global contexts.

/// Memory layer enumeration (L0-L3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Layer {
    /// L0 — Working memory: current conversation turn only.
    Working,
    /// L1 — Episodic memory: recent conversations, 30-day retention.
    Episodic,
    /// L2 — Semantic memory: learned facts, persistent.
    Semantic,
    /// L3 — Global memory: never pruned, highest durability.
    Global,
}

impl Layer {
    /// Parse a layer string into the Layer enum.
    pub fn from_layer_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "working" => Some(Self::Working),
            "episodic" => Some(Self::Episodic),
            "semantic" => Some(Self::Semantic),
            "global" => Some(Self::Global),
            _ => None,
        }
    }

    /// Return the layer name as a static string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::Episodic => "episodic",
            Self::Semantic => "semantic",
            Self::Global => "global",
        }
    }
}

impl std::fmt::Display for Layer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Configuration for a memory layer — controls retention, retrieval priority,
/// and capacity limits.
#[derive(Debug, Clone)]
pub struct LayerConfig {
    /// Human-readable name of the layer.
    pub name: &'static str,
    /// How many days items are retained before pruning (0 = never).
    pub retention_days: i64,
    /// Retrieval priority weight (higher = preferred).
    pub retrieval_priority: f64,
    /// Maximum number of items allowed in this layer (0 = unlimited).
    pub max_items: usize,
}

/// Return the configuration for a given memory layer.
pub fn get_layer_config(layer: Layer) -> LayerConfig {
    match layer {
        Layer::Working => LayerConfig {
            name: "Working Memory",
            retention_days: 0, // current turn only
            retrieval_priority: 1.0,
            max_items: 0, // unlimited
        },
        Layer::Episodic => LayerConfig {
            name: "Episodic Memory",
            retention_days: 30,
            retrieval_priority: 0.8,
            max_items: 0,
        },
        Layer::Semantic => LayerConfig {
            name: "Semantic Memory",
            retention_days: 0, // persistent
            retrieval_priority: 0.6,
            max_items: 0,
        },
        Layer::Global => LayerConfig {
            name: "Global Memory",
            retention_days: 0, // never pruned
            retrieval_priority: 0.4,
            max_items: 0,
        },
    }
}
