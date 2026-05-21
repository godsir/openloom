use crate::aggregator::PatternAggregator;
use crate::event::Event;
use crate::extractor::RuleBasedExtractor;
use crate::store::SqliteEventStore;
use anyhow::Result;

/// The cognition extraction strategy.
///
/// `RuleBased` uses pattern-count threshold triggers defined in `PatternAggregator`
/// and `action_to_trait` mappings. This is the default and currently the only variant.
///
/// `LlmBased` (future) will use a local 8B model via llama-cpp-2 for deeper
/// behavioral inference once GGUF loading is stable on all platforms.
pub enum CognitionExtractor {
    RuleBased,
    // Note: LlmBased variant requires llama-cpp-2 which is feature-gated in inference crate.
    // We keep only RuleBased here. The LlmBased branch will be added when 8B model loading works.
}

/// A cognitive insight derived from accumulated behavior patterns.
#[derive(Debug, Clone)]
pub struct CognitionUpdate {
    pub action: String,
    pub trait_name: String,
    pub evidence_count: usize,
    pub confidence: f64,
    pub summary: String,
    pub reasoning: Option<String>,
    pub scope: String,
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
    /// Cognition extraction strategy (defaults to RuleBased when None)
    #[allow(dead_code)]
    cognition: Option<CognitionExtractor>,
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
            cognition: None,
        }
    }

    /// Create a pipeline with an explicit cognition extraction strategy.
    pub fn new_with_extractor(
        extractor: RuleBasedExtractor,
        aggregator: PatternAggregator,
        store: SqliteEventStore,
        cognition: Option<CognitionExtractor>,
    ) -> Self {
        Self {
            extractor,
            aggregator,
            store,
            cognition,
        }
    }

    /// Process a single conversation turn through the full pipeline.
    pub fn process(
        &mut self,
        session_id: &str,
        text: &str,
        context: &str,
        project_scope: &str,
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
                let _events = self.aggregator.drain_events(&event.action); // collected for future LLM batch

                cognition = Some(CognitionUpdate {
                    action: event.action.clone(),
                    trait_name: self.action_to_trait(&event.action),
                    evidence_count: count,
                    confidence: avg_conf,
                    summary: self.generate_summary(&event.action, count, avg_conf),
                    reasoning: None,
                    scope: self
                        .action_to_scope(&event.action, project_scope)
                        .to_string(),
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
            "interest_hobby" => "interests".into(),
            "profession_identity" => "profession".into(),
            "skill_level" => "skills".into(),
            "general_preference" | "dislike" | "communication_style" => "preferences".into(),
            "positive_mood" | "negative_mood" | "stressed" => "emotional_state".into(),
            "goal_expressed" => "goals".into(),
            "work_habit" => "work_style".into(),
            "knowledge_domain" => "knowledge".into(),
            "tech_stack_preference" => "tech_stack".into(),
            "code_style_dislike" | "code_quality_preference" => "coding_style".into(),
            "project_context" => "project".into(),
            _ => "general_behavior".into(),
        }
    }

    fn action_to_scope<'a>(&self, action: &str, project_scope: &'a str) -> &'a str {
        match action {
            "tech_stack_preference"
            | "code_style_dislike"
            | "code_quality_preference"
            | "project_context"
            | "goal_expressed" => project_scope,
            _ => "global",
        }
    }

    fn generate_summary(&self, action: &str, count: usize, confidence: f64) -> String {
        match action {
            "interest_hobby" => format!(
                "用户对某些爱好/兴趣表现出持续热情（{}次提及，置信度{:.0}%）",
                count,
                confidence * 100.0
            ),
            "profession_identity" => format!(
                "用户透露了职业身份信息（{}次提及，置信度{:.0}%）",
                count,
                confidence * 100.0
            ),
            "skill_level" => format!(
                "用户描述了技能水平（{}次提及，置信度{:.0}%）",
                count,
                confidence * 100.0
            ),
            "general_preference" => format!(
                "用户表达了偏好倾向（{}次表达，置信度{:.0}%）",
                count,
                confidence * 100.0
            ),
            "dislike" => format!(
                "用户表达了不喜欢/反感的事物（{}次表达，置信度{:.0}%）",
                count,
                confidence * 100.0
            ),
            "positive_mood" => format!(
                "用户表现出积极情绪（{}次观察，置信度{:.0}%）",
                count,
                confidence * 100.0
            ),
            "negative_mood" => format!(
                "用户表现出消极情绪（{}次观察，置信度{:.0}%）",
                count,
                confidence * 100.0
            ),
            "stressed" => format!(
                "用户表现出焦虑/压力状态（{}次观察，置信度{:.0}%）",
                count,
                confidence * 100.0
            ),
            "goal_expressed" => format!(
                "用户表达了目标或计划（{}次提及，置信度{:.0}%）",
                count,
                confidence * 100.0
            ),
            "work_habit" => format!(
                "检测到用户工作习惯模式（{}次观察，置信度{:.0}%）",
                count,
                confidence * 100.0
            ),
            "knowledge_domain" => format!(
                "用户关注特定知识领域（{}次提及，置信度{:.0}%）",
                count,
                confidence * 100.0
            ),
            "communication_style" => format!(
                "用户有特定的沟通偏好（{}次表达，置信度{:.0}%）",
                count,
                confidence * 100.0
            ),
            "tech_stack_preference" => format!(
                "用户有技术栈偏好（{}次提及，置信度{:.0}%）",
                count,
                confidence * 100.0
            ),
            "code_style_dislike" => format!(
                "用户有代码风格禁忌（{}次提及，置信度{:.0}%）",
                count,
                confidence * 100.0
            ),
            "code_quality_preference" => format!(
                "用户关注代码质量实践（{}次提及，置信度{:.0}%）",
                count,
                confidence * 100.0
            ),
            "project_context" => format!(
                "用户提供了项目上下文信息（{}次提及，置信度{:.0}%）",
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
            ("session_1", "我是一名后端开发，主要写服务端"),
            ("session_2", "我做了5年后端开发了"),
            ("session_3", "我是做后端工程师的"),
        ];

        let mut triggered_count = 0;
        for (sid, text) in &sessions {
            let result = pipeline.process(sid, text, "chat", "global").unwrap();
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
            .process("s1", "我喜欢用Python做数据分析", "chat", "global")
            .unwrap();
        assert!(result.cognition_triggered.is_none());
        assert!(!result.events.is_empty());
    }

    #[test]
    fn test_cognition_summary_is_chinese() {
        let (mut pipeline, _dir) = setup_pipeline(1);

        let result = pipeline
            .process("s1", "我是做后端开发的工程师", "chat", "global")
            .unwrap();
        let cog = result.cognition_triggered.as_ref().unwrap();
        assert!(cog.summary.contains("职业"));
        assert_eq!(cog.trait_name, "profession");
        assert!(cog.confidence > 0.0);
    }

    #[test]
    fn test_mixed_events_different_patterns() {
        let (mut pipeline, _dir) = setup_pipeline(1);

        let result = pipeline
            .process(
                "s1",
                "我是一名后端开发，擅长Python和数据分析，今天工作特别累压力大",
                "chat",
                "global",
            )
            .unwrap();

        let actions: Vec<&str> = result.events.iter().map(|e| e.action.as_str()).collect();
        assert!(
            actions.contains(&"profession_identity"),
            "should detect profession"
        );
        assert!(actions.contains(&"skill_level"), "should detect skill");
        assert!(actions.contains(&"stressed"), "should detect stress");
    }
}
