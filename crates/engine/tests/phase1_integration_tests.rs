use openloom_engine::Engine;
use openloom_models::{ChatMessage, Mode, ModelPreference};
use tempfile::tempdir;

fn setup_engine() -> (Engine, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let engine = Engine::new_test(db_path).unwrap();
    (engine, dir)
}

#[tokio::test]
async fn scenario_1_skill_path_file_operation() {
    let (engine, _dir) = setup_engine();
    let sid = engine.create_session().await.unwrap().id;
    let msg = ChatMessage {
        role: "user".into(),
        content: "帮我打开这个文件看看".into(),
        timestamp: chrono::Utc::now(),
    };
    let resp = engine.handle_message(msg, &sid, Mode::Code, ModelPreference::default()).await.unwrap();
    assert!(!resp.session_id.is_empty());
}

#[tokio::test]
async fn scenario_2_llm_path_chat() {
    let (engine, _dir) = setup_engine();
    let sid = engine.create_session().await.unwrap().id;
    let msg = ChatMessage {
        role: "user".into(),
        content: "你好啊，很高兴见到你".into(),
        timestamp: chrono::Utc::now(),
    };
    let resp = engine.handle_message(msg, &sid, Mode::Code, ModelPreference::default()).await.unwrap();
    assert_eq!(resp.session_id, sid);
}

#[tokio::test]
async fn scenario_3_empty_input() {
    let (engine, _dir) = setup_engine();
    let sid = engine.create_session().await.unwrap().id;
    let msg = ChatMessage {
        role: "user".into(),
        content: String::new(),
        timestamp: chrono::Utc::now(),
    };
    let resp = engine.handle_message(msg, &sid, Mode::Code, ModelPreference::default()).await;
    assert!(resp.is_ok());
}

#[tokio::test]
async fn scenario_4_consistent_classification() {
    let (engine, _dir) = setup_engine();
    let sid = engine.create_session().await.unwrap().id;
    for _ in 0..5 {
        let msg = ChatMessage {
            role: "user".into(),
            content: "帮我写一段Python代码".into(),
            timestamp: chrono::Utc::now(),
        };
        let resp = engine.handle_message(msg, &sid, Mode::Code, ModelPreference::default()).await;
        assert!(resp.is_ok());
    }
}

#[tokio::test]
async fn scenario_5_long_text() {
    let (engine, _dir) = setup_engine();
    let sid = engine.create_session().await.unwrap().id;
    let long_text = "帮我搜索 ".repeat(500);
    let msg = ChatMessage {
        role: "user".into(),
        content: long_text,
        timestamp: chrono::Utc::now(),
    };
    let resp = engine.handle_message(msg, &sid, Mode::Code, ModelPreference::default()).await;
    assert!(resp.is_ok());
}

#[tokio::test]
async fn scenario_6_memory_pipeline_non_blocking() {
    let (engine, _dir) = setup_engine();
    let sid = engine.create_session().await.unwrap().id;
    for i in 0..10 {
        let msg = ChatMessage {
            role: "user".into(),
            content: format!("消息 {}", i),
            timestamp: chrono::Utc::now(),
        };
        let resp = engine.handle_message(msg, &sid, Mode::Code, ModelPreference::default()).await;
        assert!(resp.is_ok());
    }
}

#[tokio::test]
async fn scenario_7_code_assist_skill() {
    let (engine, _dir) = setup_engine();
    let sid = engine.create_session().await.unwrap().id;
    let msg = ChatMessage {
        role: "user".into(),
        content: "修复这个bug".into(),
        timestamp: chrono::Utc::now(),
    };
    let resp = engine.handle_message(msg, &sid, Mode::Code, ModelPreference::default()).await.unwrap();
    assert!(!resp.session_id.is_empty());
}

#[tokio::test]
async fn scenario_8_session_management() {
    let (engine, _dir) = setup_engine();
    let s1 = engine.create_session().await.unwrap();
    let s2 = engine.create_session().await.unwrap();
    let sessions = engine.list_sessions().await.unwrap();
    assert!(sessions.len() >= 2);
    assert_ne!(s1.id, s2.id);
}

#[tokio::test]
async fn scenario_9_health_check() {
    let (engine, _dir) = setup_engine();
    let health = engine.health_check().await;
    assert_eq!(health.status, "ok");
}

#[tokio::test]
async fn scenario_10_event_bus_token_usage() {
    let (engine, _dir) = setup_engine();
    let mut rx = engine.subscribe();
    let sid = engine.create_session().await.unwrap().id;

    let msg = ChatMessage {
        role: "user".into(),
        content: "你好".into(),
        timestamp: chrono::Utc::now(),
    };
    engine.handle_message(msg, &sid, Mode::Code, ModelPreference::default()).await.unwrap();

    let event = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv()).await;

    assert!(event.is_ok(), "should receive TokenUsage event");
}
