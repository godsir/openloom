use crate::aggregator::PatternAggregator;
use crate::event::Event;
use crate::extractor::RuleBasedExtractor;
use crate::store::SqliteEventStore;
use anyhow::Result;

/// A cognitive insight derived from accumulated behavior patterns.
#[derive(Debug, Clone)]
pub struct CognitionUpdate {
    pub action: String,
    pub trait_name: String,
    pub evidence_count: usize,
    pub confidence: f64,
    pub summary: String,
    pub reasoning: Option<String>,
}

/// The result of processing a single conversation turn through the memory pipeline.
#[derive(Debug)]
pub struct PipelineResult {
    pub events: Vec<Event>,
    pub cognition_triggered: Option<CognitionUpdate>,
}

/// Orchestrates the three-stage memory pipeline:
/// text → events → pattern aggregation → storage → cognition updates.
pub struct MemoryPipeline {
    extractor: RuleBasedExtractor,
    aggregator: PatternAggregator,
    store: SqliteEventStore,
}

impl MemoryPipeline {
    pub fn new(
        extractor: RuleBasedExtractor,
        aggregator: PatternAggregator,
        store: SqliteEventStore,
    ) -> Self {
        Self {
            extractor,
            aggregator,
            store,
        }
    }

    /// Process a single conversation turn through the full pipeline.
    pub fn process(
        &mut self,
        session_id: &str,
        text: &str,
        context: &str,
    ) -> Result<PipelineResult> {
        // Stage 1: Extract events from text
        let mut events = self.extractor.extract(text, context);
        for event in &mut events {
            event.source_session = Some(session_id.to_string());
        }

        // Store events in database
        let mut event_ids = Vec::new();
        for event in &events {
            let id = self.store.insert(event)?;
            event_ids.push(id);
        }

        // Update event IDs in returned events
        for (event, id) in events.iter_mut().zip(event_ids) {
            event.id = Some(id);
        }

        // Stage 2: Aggregate patterns — check for cognition triggers
        let mut cognition = None;
        for event in &events {
            self.aggregator.observe(event);
            if self.aggregator.should_trigger(&event.action) {
                let (count, avg_conf) = self.aggregator.drain(&event.action).unwrap_or_default();
                cognition = Some(CognitionUpdate {
                    action: event.action.clone(),
                    trait_name: self.action_to_trait(&event.action),
                    evidence_count: count,
                    confidence: avg_conf,
                    summary: self.generate_summary(&event.action, count, avg_conf),
                    reasoning: None,
                });
            }
        }

        Ok(PipelineResult {
            events,
            cognition_triggered: cognition,
        })
    }

    /// Access the underlying event store.
    pub fn store(&self) -> &SqliteEventStore {
        &self.store
    }

    fn action_to_trait(&self, action: &str) -> String {
        match action {
            "loss_chase" => "risk_tendency".into(),
            "chase_high" => "entry_timing".into(),
            "avoid_stop_loss" => "risk_management".into(),
            "prefers_short_term" => "trading_style".into(),
            "prefers_long_term" => "trading_style".into(),
            "prefers_tech_stocks" => "sector_preference".into(),
            "negative_emotional" => "emotional_state".into(),
            "positive_emotional" => "emotional_state".into(),
            "anxious" => "emotional_state".into(),
            _ => "general_behavior".into(),
        }
    }

    fn generate_summary(&self, action: &str, count: usize, confidence: f64) -> String {
        match action {
            "loss_chase" => format!(
                "用户存在赌徒补仓倾向：在亏损状态下多次加仓（{}次观察，置信度{:.0}%）",
                count,
                confidence * 100.0
            ),
            "chase_high" => format!(
                "用户有追高行为模式：在股价上涨时追买（{}次观察，置信度{:.0}%）",
                count,
                confidence * 100.0
            ),
            "avoid_stop_loss" => format!(
                "用户倾向于不止损：面对亏损选择扛单（{}次观察，置信度{:.0}%）",
                count,
                confidence * 100.0
            ),
            "prefers_short_term" => format!(
                "用户偏好短线交易风格（{}次表达，置信度{:.0}%）",
                count,
                confidence * 100.0
            ),
            "prefers_long_term" => format!(
                "用户偏好长线价值投资（{}次表达，置信度{:.0}%）",
                count,
                confidence * 100.0
            ),
            "prefers_tech_stocks" => format!(
                "用户偏好科技/成长股投资（{}次表达，置信度{:.0}%）",
                count,
                confidence * 100.0
            ),
            "negative_emotional" => format!(
                "用户在交易中出现负面情绪（{}次观察，置信度{:.0}%）",
                count,
                confidence * 100.0
            ),
            "positive_emotional" => format!(
                "用户在交易中表现出正面情绪（{}次观察，置信度{:.0}%）",
                count,
                confidence * 100.0
            ),
            "anxious" => format!(
                "用户对市场波动表现出焦虑情绪（{}次观察，置信度{:.0}%）",
                count,
                confidence * 100.0
            ),
            _ => format!(
                "检测到行为模式: {}（{}次观察，置信度{:.0}%）",
                action,
                count,
                confidence * 100.0
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aggregator::PatternAggregator;
    use crate::extractor::RuleBasedExtractor;
    use tempfile::tempdir;

    fn setup_pipeline(threshold: usize) -> (MemoryPipeline, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let extractor = RuleBasedExtractor::with_default_rules();
        let aggregator = PatternAggregator::new(threshold);
        let store = SqliteEventStore::open(&db_path).unwrap();
        let pipeline = MemoryPipeline::new(extractor, aggregator, store);
        (pipeline, dir)
    }

    #[test]
    fn test_pipeline_end_to_end() {
        let (mut pipeline, _dir) = setup_pipeline(3);

        let sessions = vec![
            ("session_1", "亏了20%我还加仓了，我觉得会涨回来"),
            ("session_2", "又跌了，但我还是补仓了"),
            ("session_3", "这次真亏麻了，但我不甘心又加仓了"),
        ];

        let mut triggered_count = 0;
        for (sid, text) in &sessions {
            let result = pipeline.process(sid, text, "trading").unwrap();
            if result.cognition_triggered.is_some() {
                triggered_count += 1;
            }
        }

        assert_eq!(
            triggered_count, 1,
            "3rd observation should trigger cognition"
        );

        let stored = pipeline.store().query_all(10).unwrap();
        assert_eq!(stored.len(), 3);
    }

    #[test]
    fn test_pipeline_no_trigger_below_threshold() {
        let (mut pipeline, _dir) = setup_pipeline(5);

        let result = pipeline
            .process("s1", "亏了10%我又加仓了", "trading")
            .unwrap();
        assert!(result.cognition_triggered.is_none());
        assert!(!result.events.is_empty());
    }

    #[test]
    fn test_cognition_summary_is_chinese() {
        let (mut pipeline, _dir) = setup_pipeline(1);

        let result = pipeline
            .process("s1", "亏了30%我又加仓了", "trading")
            .unwrap();
        let cog = result.cognition_triggered.as_ref().unwrap();
        assert!(
            cog.summary.contains("赌徒补仓"),
            "Summary should explain pattern in Chinese"
        );
        assert_eq!(cog.trait_name, "risk_tendency");
        assert!(cog.confidence > 0.0);
    }

    #[test]
    fn test_mixed_events_different_patterns() {
        let (mut pipeline, _dir) = setup_pipeline(1);

        let result = pipeline
            .process(
                "s1",
                "我很喜欢科技股，这次AI芯片又追高了，亏了很多很难过",
                "trading",
            )
            .unwrap();

        let actions: Vec<&str> = result.events.iter().map(|e| e.action.as_str()).collect();
        assert!(
            actions.contains(&"prefers_tech_stocks"),
            "should detect tech stock preference"
        );
        assert!(
            actions.contains(&"chase_high"),
            "should detect chase-high behavior"
        );
        assert!(
            actions.contains(&"negative_emotional"),
            "should detect negative emotion"
        );
    }

    #[test]
    fn test_ten_scenario_fixtures() {
        let (mut pipeline, _dir) = setup_pipeline(3);

        let fixtures = include_str!("../../../tests/fixtures/trading_scenarios.txt");
        let mut total_events = 0;
        let mut cognition_count = 0;
        let mut triggered_traits: Vec<String> = Vec::new();

        for line in fixtures.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let parts: Vec<&str> = line.splitn(3, '|').collect();
            if parts.len() < 3 {
                continue;
            }

            let (sid, ctx, text) = (parts[0], parts[1], parts[2]);

            match pipeline.process(sid, text, ctx) {
                Ok(result) => {
                    total_events += result.events.len();
                    if let Some(cog) = result.cognition_triggered {
                        cognition_count += 1;
                        triggered_traits
                            .push(format!("{}={}", cog.trait_name, cog.action));
                    }
                }
                Err(e) => panic!("Pipeline error on line '{}': {}", line, e),
            }
        }

        assert!(total_events > 0, "Should extract events from scenarios");
        assert!(cognition_count > 0, "Should trigger at least one cognition");
        eprintln!("Events: {}, Cognitions: {}", total_events, cognition_count);
        eprintln!("Triggered traits: {:?}", triggered_traits);
    }
}
