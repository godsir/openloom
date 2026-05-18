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
        let result = pipeline.process(sid, text, ctx).unwrap();
        if let Some(cog) = result.cognition_triggered {
            triggered.push(cog.summary);
        }
    }
    triggered
}

#[test]
fn scenario_1_loss_chase_detection() {
    let (mut pipeline, _dir) = setup_pipeline(3);
    let sessions = vec![
        ("s1", "trading", "亏了20%我又加仓了，我觉得到底了"),
        ("s2", "trading", "已经连续跌了一周，但我还是补仓了"),
        ("s3", "trading", "又跌了，不甘心又买入了一些"),
    ];
    let triggered = feed_sessions(&mut pipeline, &sessions);
    assert!(!triggered.is_empty(), "应该检测到loss_chase模式");
    assert!(triggered[0].contains("赌徒补仓"), "应该识别为赌徒补仓倾向");
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
fn scenario_3_trading_style_preference() {
    let (mut pipeline, _dir) = setup_pipeline(1);
    let result = pipeline
        .process("s1", "我更喜欢短线交易，快进快出比较刺激", "trading")
        .unwrap();
    let has_pref = result
        .events
        .iter()
        .any(|e| e.action == "prefers_short_term");
    assert!(has_pref, "应该检测到短线交易偏好");
}

#[test]
fn scenario_4_emotional_state_tracking() {
    let (mut pipeline, _dir) = setup_pipeline(1);
    let result = pipeline
        .process("s1", "今天真的很沮丧，感觉做什么都不顺", "mood")
        .unwrap();
    let has_emotion = result
        .events
        .iter()
        .any(|e| e.action == "negative_emotional");
    assert!(has_emotion, "应该检测到负面情绪");
}

#[test]
fn scenario_5_mixed_signals() {
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
        "应检测到科技股偏好"
    );
    assert!(actions.contains(&"chase_high"), "应检测到追高行为");
    assert!(actions.contains(&"negative_emotional"), "应检测到负面情绪");
}

#[test]
fn scenario_6_threshold_independence() {
    let (mut pipeline_low, _dir1) = setup_pipeline(1);
    let (mut pipeline_high, _dir2) = setup_pipeline(10);

    let sessions = vec![
        ("s1", "trading", "亏了10%我又加仓了"),
        ("s2", "trading", "又跌了我又补仓了"),
    ];

    let triggered_low = feed_sessions(&mut pipeline_low, &sessions);
    let triggered_high = feed_sessions(&mut pipeline_high, &sessions);

    // threshold=1 triggers on first observation, so 2 sessions = 2 triggers
    assert!(!triggered_low.is_empty(), "低阈值应该触发");
    // threshold=10 won't trigger with only 2 observations
    assert!(triggered_high.is_empty(), "高阈值不应触发");
}

#[test]
fn scenario_7_coding_style_preference() {
    let (mut pipeline, _dir) = setup_pipeline(1);
    let result = pipeline
        .process(
            "s1",
            "我还是更喜欢用Rust写后端，Go的error handling太啰嗦了",
            "coding",
        )
        .unwrap();
    let has_pref = result
        .events
        .iter()
        .any(|e| e.action == "general_preference");
    assert!(has_pref, "应检测到通用偏好");
}

#[test]
fn scenario_8_anxiety_detection() {
    let (mut pipeline, _dir) = setup_pipeline(1);
    let result = pipeline
        .process("s1", "我最近总是睡不好，一直担心市场会崩盘", "mood")
        .unwrap();
    let has_anxiety = result.events.iter().any(|e| e.action == "anxious");
    assert!(has_anxiety, "应检测到焦虑情绪");
}

#[test]
fn scenario_9_empty_input() {
    let (mut pipeline, _dir) = setup_pipeline(1);
    let result = pipeline.process("s1", "", "empty").unwrap();
    assert!(result.events.is_empty());
    assert!(result.cognition_triggered.is_none());
}

#[test]
fn scenario_10_sqlite_persistence() {
    let (mut pipeline, _dir) = setup_pipeline(1);

    pipeline
        .process("s1", "亏了30%又加仓了", "trading")
        .unwrap();
    pipeline
        .process("s2", "又跌了我又补仓了", "trading")
        .unwrap();

    let all = pipeline.store().query_all(10).unwrap();
    assert_eq!(all.len(), 2);
    let count = pipeline.store().count_by_action("loss_chase").unwrap();
    assert_eq!(count, 2);
}
