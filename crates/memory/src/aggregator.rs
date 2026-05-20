use crate::event::Event;
use chrono::Utc;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct EventDatum {
    pub action: String,
    pub context: String,
    pub source_text: String,
    pub confidence: f64,
}

#[derive(Debug, Clone)]
struct PatternCount {
    count: usize,
    confidence_sum: f64,
}

/// Tracks event patterns and triggers when a pattern's occurrence count
/// reaches a configurable threshold.
///
/// Includes a sliding window with 24h time decay: only observations
/// within the window count toward trigger decisions. Older observations
/// are silently pruned on each `observe()` call.
pub struct PatternAggregator {
    threshold: usize,
    counts: HashMap<String, PatternCount>,
    /// Timestamped observations for sliding window (Unix epoch seconds)
    observations: HashMap<String, Vec<i64>>,
    /// Window duration in seconds (default: 86400 = 24 hours)
    window_secs: u64,
    /// Event data buffer for LLM-based cognition extraction (future)
    event_buffer: HashMap<String, Vec<EventDatum>>,
}

impl PatternAggregator {
    pub fn new(threshold: usize) -> Self {
        Self {
            threshold,
            counts: HashMap::new(),
            observations: HashMap::new(),
            window_secs: 86400, // 24-hour default window
            event_buffer: HashMap::new(),
        }
    }

    /// Builder method to customize the sliding window duration.
    pub fn with_window(mut self, window_secs: u64) -> Self {
        self.window_secs = window_secs;
        self
    }

    /// Record an observed event, incrementing the counter and logging
    /// the timestamp for sliding-window decay.
    pub fn observe(&mut self, event: &Event) {
        let action = event.action.clone();
        let entry = self.counts.entry(action.clone()).or_insert(PatternCount {
            count: 0,
            confidence_sum: 0.0,
        });
        entry.count += 1;
        entry.confidence_sum += event.confidence;

        // Record timestamp for sliding window
        let now = Utc::now().timestamp();
        let timestamps = self.observations.entry(action).or_default();
        timestamps.push(now);

        // Prune old observations outside window
        let cutoff = now - self.window_secs as i64;
        timestamps.retain(|&t| t >= cutoff);

        // Buffer event data for future LLM-based cognition extraction
        let datum = EventDatum {
            action: event.action.clone(),
            context: event.context.clone(),
            source_text: event.source_text.clone(),
            confidence: event.confidence,
        };
        self.event_buffer
            .entry(event.action.clone())
            .or_default()
            .push(datum);
    }

    pub fn count(&self, action: &str) -> usize {
        self.counts.get(action).map(|p| p.count).unwrap_or(0)
    }

    /// Returns true if the pattern count meets or exceeds the threshold
    /// AND recent observations within the sliding window also meet the threshold.
    pub fn should_trigger(&self, action: &str) -> bool {
        let total_count = self.counts.get(action).map(|p| p.count).unwrap_or(0);
        if total_count < self.threshold {
            return false;
        }
        // Also check recent observations in window
        let recent = self
            .observations
            .get(action)
            .map(|ts| ts.len())
            .unwrap_or(0);
        recent >= self.threshold
    }

    pub fn average_confidence(&self, action: &str) -> Option<f64> {
        self.counts.get(action).map(|p| {
            if p.count == 0 {
                0.0
            } else {
                p.confidence_sum / p.count as f64
            }
        })
    }

    /// Remove and return a pattern's data once triggered.
    /// Clears windowed observations and resets the counter so future
    /// observations start fresh.
    pub fn drain(&mut self, action: &str) -> Option<(usize, f64)> {
        if !self.should_trigger(action) {
            return None;
        }
        let count = self
            .observations
            .get(action)
            .map(|ts| ts.len())
            .unwrap_or(0);
        // Clear recent observations, keep the action in counts for total stats
        if let Some(timestamps) = self.observations.get_mut(action) {
            timestamps.clear();
        }
        let avg_conf = self
            .counts
            .get(action)
            .map(|p| {
                if p.count == 0 {
                    0.0
                } else {
                    p.confidence_sum / p.count as f64
                }
            })
            .unwrap_or(0.0);
        // Reset the count for this action so future observations are fresh
        self.counts.insert(
            action.to_string(),
            PatternCount {
                count: 0,
                confidence_sum: 0.0,
            },
        );
        Some((count, avg_conf))
    }

    /// List all actions whose count meets the threshold.
    pub fn active_patterns(&self) -> Vec<String> {
        self.counts
            .iter()
            .filter(|(_, p)| p.count >= self.threshold)
            .map(|(k, _)| k.clone())
            .collect()
    }

    /// Drain all buffered event data for a given action.
    /// Returns the collected events and clears the buffer for that action.
    pub fn drain_events(&mut self, action: &str) -> Vec<EventDatum> {
        self.event_buffer.remove(action).unwrap_or_default()
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
