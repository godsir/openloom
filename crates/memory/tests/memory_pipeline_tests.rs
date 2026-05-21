use openloom_memory::aggregator::PatternAggregator;
use openloom_memory::extractor::RuleBasedExtractor;
use openloom_memory::pipeline::MemoryPipeline;
use openloom_memory::store::SqliteEventStore;
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

fn feed_sessions(pipeline: &mut MemoryPipeline, sessions: &[(&str, &str, &str)]) -> Vec<String> {
    let mut triggered = Vec::new();
    for (sid, ctx, text) in sessions {
        let result = pipeline.process(sid, text, ctx, "global").unwrap();
        if let Some(cog) = result.cognition_triggered {
            triggered.push(cog.summary);
        }
    }
    triggered
}

#[test]
fn scenario_1_profession_detection() {
    let (mut pipeline, _dir) = setup_pipeline(3);
    let sessions = vec![
        ("s1", "chat", "我是一名后端开发工程师"),
        ("s2", "chat", "我做了5年后端开发了"),
        ("s3", "chat", "我在做后端开发，主要写微服务"),
    ];
    let triggered = feed_sessions(&mut pipeline, &sessions);
    assert!(!triggered.is_empty(), "应该检测到profession模式");
    assert!(
        triggered[0].contains("职业"),
        "应该识别为职业身份: {}",
        triggered[0]
    );
}

#[test]
fn scenario_2_no_pattern_in_casual_chat() {
    let (mut pipeline, _dir) = setup_pipeline(2);
    let sessions = vec![
        ("s1", "casual", "今天天气真不错"),
        ("s2", "casual", "中午吃了碗面"),
        ("s3", "casual", "周末打算去爬山"),
    ];
    let triggered = feed_sessions(&mut pipeline, &sessions);
    assert!(triggered.is_empty(), "日常寒暄不应触发任何认知");
}

#[test]
fn scenario_3_interest_detection() {
    let (mut pipeline, _dir) = setup_pipeline(1);
    let result = pipeline
        .process(
            "s1",
            "我特别喜欢编程，尤其是Rust和系统编程",
            "chat",
            "global",
        )
        .unwrap();
    let has_interest = result.events.iter().any(|e| e.action == "interest_hobby");
    assert!(has_interest, "应该检测到兴趣爱好");
}

#[test]
fn scenario_4_emotional_state_tracking() {
    let (mut pipeline, _dir) = setup_pipeline(1);
    let result = pipeline
        .process("s1", "今天真的很沮丧，感觉做什么都不顺", "mood", "global")
        .unwrap();
    let has_emotion = result.events.iter().any(|e| e.action == "negative_mood");
    assert!(has_emotion, "应该检测到负面情绪");
}

#[test]
fn scenario_5_mixed_signals() {
    let (mut pipeline, _dir) = setup_pipeline(1);
    let result = pipeline
        .process(
            "s1",
            "我是一名后端开发，擅长Python和数据分析，今天工作压力大很焦虑",
            "chat",
            "global",
        )
        .unwrap();
    let actions: Vec<&str> = result.events.iter().map(|e| e.action.as_str()).collect();
    assert!(
        actions.contains(&"profession_identity"),
        "应检测到职业: {:?}",
        actions
    );
    assert!(
        actions.contains(&"skill_level"),
        "应检测到技能: {:?}",
        actions
    );
    assert!(actions.contains(&"stressed"), "应检测到压力: {:?}", actions);
}

#[test]
fn scenario_6_threshold_independence() {
    let (mut pipeline_low, _dir1) = setup_pipeline(1);
    let (mut pipeline_high, _dir2) = setup_pipeline(10);

    let sessions = vec![
        ("s1", "chat", "我是做后端开发的"),
        ("s2", "chat", "我做了很多年后端开发"),
    ];

    let triggered_low = feed_sessions(&mut pipeline_low, &sessions);
    let triggered_high = feed_sessions(&mut pipeline_high, &sessions);

    assert!(!triggered_low.is_empty(), "低阈值应该触发");
    assert!(triggered_high.is_empty(), "高阈值不应触发");
}

#[test]
fn scenario_7_preference_detection() {
    let (mut pipeline, _dir) = setup_pipeline(1);
    let result = pipeline
        .process(
            "s1",
            "我还是更喜欢用Vim写代码，VS Code太重了",
            "coding",
            "global",
        )
        .unwrap();
    let has_pref = result
        .events
        .iter()
        .any(|e| e.action == "general_preference");
    assert!(has_pref, "应检测到偏好");
}

#[test]
fn scenario_8_stress_detection() {
    let (mut pipeline, _dir) = setup_pipeline(1);
    let result = pipeline
        .process("s1", "最近工作压力大，总是焦虑睡不好", "mood", "global")
        .unwrap();
    let has_stress = result.events.iter().any(|e| e.action == "stressed");
    assert!(has_stress, "应检测到压力/焦虑");
}

#[test]
fn scenario_9_empty_input() {
    let (mut pipeline, _dir) = setup_pipeline(1);
    let result = pipeline.process("s1", "", "empty", "global").unwrap();
    assert!(result.events.is_empty());
    assert!(result.cognition_triggered.is_none());
}

#[test]
fn scenario_10_sqlite_persistence() {
    let (mut pipeline, _dir) = setup_pipeline(1);

    pipeline
        .process("s1", "我是做后端开发的工程师", "chat", "global")
        .unwrap();
    pipeline
        .process("s2", "我做了5年前端开发了", "chat", "global")
        .unwrap();

    let all = pipeline.store().query_all(10).unwrap();
    assert_eq!(all.len(), 2);
    let count = pipeline
        .store()
        .count_by_action("profession_identity")
        .unwrap();
    assert_eq!(count, 2);
}
