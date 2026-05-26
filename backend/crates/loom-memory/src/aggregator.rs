//! Pattern aggregator — tracks event frequency and triggers cognition generation.

use std::collections::HashMap;

/// Aggregates repeated event patterns and triggers when threshold is reached.
pub struct PatternAggregator {
    counts: HashMap<String, usize>,
    threshold: usize,
}

impl PatternAggregator {
    pub fn new(threshold: usize) -> Self {
        Self { counts: HashMap::new(), threshold }
    }

    /// Record an event action and return true if threshold is reached.
    pub fn record(&mut self, action: &str) -> bool {
        let count = self.counts.entry(action.to_string()).or_insert(0);
        *count += 1;
        *count >= self.threshold
    }

    /// Drain all accumulated counts.
    pub fn drain(&mut self) {
        self.counts.clear();
    }
}

impl Default for PatternAggregator {
    fn default() -> Self {
        Self::new(3)
    }
}
