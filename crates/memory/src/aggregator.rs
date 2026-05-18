use crate::event::Event;
use std::collections::HashMap;

#[derive(Debug, Clone)]
struct PatternCount {
    count: usize,
    confidence_sum: f64,
}

/// Tracks event patterns and triggers when a pattern's occurrence count
/// reaches a configurable threshold.
pub struct PatternAggregator {
    threshold: usize,
    patterns: HashMap<String, PatternCount>,
}

impl PatternAggregator {
    pub fn new(threshold: usize) -> Self {
        Self {
            threshold,
            patterns: HashMap::new(),
        }
    }

    /// Record an observed event, incrementing the count for its action.
    pub fn observe(&mut self, event: &Event) {
        let entry = self
            .patterns
            .entry(event.action.clone())
            .or_insert(PatternCount {
                count: 0,
                confidence_sum: 0.0,
            });
        entry.count += 1;
        entry.confidence_sum += event.confidence;
    }

    pub fn count(&self, action: &str) -> usize {
        self.patterns.get(action).map(|p| p.count).unwrap_or(0)
    }

    /// Returns true if the pattern count meets or exceeds the threshold.
    pub fn should_trigger(&self, action: &str) -> bool {
        self.patterns
            .get(action)
            .map(|p| p.count >= self.threshold)
            .unwrap_or(false)
    }

    pub fn average_confidence(&self, action: &str) -> Option<f64> {
        self.patterns.get(action).map(|p| {
            if p.count == 0 {
                0.0
            } else {
                p.confidence_sum / p.count as f64
            }
        })
    }

    /// Remove and return a pattern's data once triggered.
    pub fn drain(&mut self, action: &str) -> Option<(usize, f64)> {
        self.patterns.remove(action).map(|p| {
            let avg = if p.count == 0 {
                0.0
            } else {
                p.confidence_sum / p.count as f64
            };
            (p.count, avg)
        })
    }

    /// List all actions whose count meets the threshold.
    pub fn active_patterns(&self) -> Vec<String> {
        self.patterns
            .iter()
            .filter(|(_, p)| p.count >= self.threshold)
            .map(|(k, _)| k.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{Event, EventType};

    fn make_event(action: &str, confidence: f64) -> Event {
        Event::new(
            EventType::BehaviorPattern,
            action,
            "test",
            confidence,
            "source",
        )
    }

    #[test]
    fn test_threshold_not_met() {
        let mut agg = PatternAggregator::new(5);
        agg.observe(&make_event("loss_chase", 0.85));
        agg.observe(&make_event("loss_chase", 0.80));
        agg.observe(&make_event("loss_chase", 0.90));
        assert!(!agg.should_trigger("loss_chase"));
    }

    #[test]
    fn test_threshold_met() {
        let mut agg = PatternAggregator::new(3);
        agg.observe(&make_event("loss_chase", 0.85));
        agg.observe(&make_event("loss_chase", 0.80));
        agg.observe(&make_event("loss_chase", 0.90));
        assert!(agg.should_trigger("loss_chase"));
    }

    #[test]
    fn test_average_confidence() {
        let mut agg = PatternAggregator::new(2);
        agg.observe(&make_event("loss_chase", 0.80));
        agg.observe(&make_event("loss_chase", 0.90));
        let avg = agg.average_confidence("loss_chase");
        assert!(avg.is_some());
        assert!((avg.unwrap() - 0.85).abs() < 0.01);
    }

    #[test]
    fn test_drain_resets_counter() {
        let mut agg = PatternAggregator::new(2);
        agg.observe(&make_event("loss_chase", 0.80));
        agg.observe(&make_event("loss_chase", 0.90));
        assert!(agg.should_trigger("loss_chase"));
        agg.drain("loss_chase");
        assert!(!agg.should_trigger("loss_chase"));
    }

    #[test]
    fn test_list_active_patterns() {
        let mut agg = PatternAggregator::new(2);
        agg.observe(&make_event("loss_chase", 0.80));
        agg.observe(&make_event("loss_chase", 0.90));
        agg.observe(&make_event("chase_high", 0.85));

        let active = agg.active_patterns();
        assert!(active.contains(&"loss_chase".to_string()));
        assert!(!active.contains(&"chase_high".to_string()));
    }
}
