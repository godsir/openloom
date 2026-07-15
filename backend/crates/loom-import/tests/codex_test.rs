//! Codex rollout JSONL scan + build_payload tests.

#[test]
fn codex_scan_and_build_payload() {
    let dir = tempfile::tempdir().expect("tmpdir");
    let sessions = dir.path().join("sessions/2026/07/10");
    std::fs::create_dir_all(&sessions).unwrap();
    let rollout = sessions.join("rollout-2026-07-10T03-03-47-test-codex-001.jsonl");
    std::fs::write(
        &rollout,
        concat!(
            r#"{"timestamp":"2026-07-10T03:03:47.277Z","type":"session_meta","payload":{"session_id":"test-codex-001","cwd":"F:\\proj","cli_version":"0.144.0"}}"#, "\n",
            r#"{"timestamp":"2026-07-10T03:03:48.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"hello codex"}]}}"#, "\n",
            r#"{"timestamp":"2026-07-10T03:03:49.000Z","type":"response_item","payload":{"type":"reasoning","id":"rs_1","summary":[{"type":"summary_text","text":"thinking"}]}}"#, "\n",
            r#"{"timestamp":"2026-07-10T03:03:50.000Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"hi there"}]}}"#, "\n",
            r#"{"timestamp":"2026-07-10T03:03:51.000Z","type":"response_item","payload":{"type":"function_call","name":"exec_command","arguments":"{\"cmd\":\"ls\"}","call_id":"call_1"}}"#, "\n",
            r#"{"timestamp":"2026-07-10T03:03:52.000Z","type":"response_item","payload":{"type":"function_call_output","call_id":"call_1","output":"file1\nfile2"}}"#, "\n",
            r#"{"timestamp":"2026-07-10T03:03:53.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<environment_context>injected</environment_context>"}]}}"#, "\n",
        ),
    )
    .unwrap();

    // archived_sessions 也应被扫到
    let arch = dir.path().join("archived_sessions");
    std::fs::create_dir_all(&arch).unwrap();
    std::fs::write(
        arch.join("rollout-2026-07-10T04-00-00-test-codex-002.jsonl"),
        concat!(
            r#"{"timestamp":"2026-07-10T04:00:00.000Z","type":"session_meta","payload":{"session_id":"test-codex-002","cwd":"F:\\proj2"}}"#, "\n",
            r#"{"timestamp":"2026-07-10T04:00:01.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"second"}]}}"#, "\n",
        ),
    )
    .unwrap();

    let summaries = loom_import::codex::scan(dir.path()).expect("scan ok");
    assert_eq!(summaries.len(), 2);
    let s = summaries
        .iter()
        .find(|s| s.session_uuid == "test-codex-001")
        .expect("test-codex-001 present");
    assert_eq!(s.project_dir, r"F:\proj");
    assert_eq!(s.first_message.as_deref(), Some("hello codex")); // <env> 不作 first_message
    assert_eq!(s.message_count, 3); // hello(user) + hi(assistant) + <env>(user)

    let payload = loom_import::codex::build_payload(&rollout).expect("parse ok");
    assert_eq!(payload.id, "test-codex-001");
    assert_eq!(payload.workspace_path.as_deref(), Some(r"F:\proj"));
    assert_eq!(payload.title.as_deref(), Some("hello codex"));
    // user(hello), assistant(reasoning), assistant(hi), assistant(ToolCall), user(ToolResult)
    // <env> user 被过滤
    assert_eq!(payload.messages.len(), 5);
    assert_eq!(payload.messages[0].role, loom_types::Role::User);
    assert!(matches!(
        &payload.messages[1].content[0],
        loom_types::ContentPart::Thinking { text } if text == "thinking"
    ));
    assert_eq!(payload.messages[2].role, loom_types::Role::Assistant);
    assert!(matches!(&payload.messages[2].content[0], loom_types::ContentPart::Text { text } if text == "hi there"));
    assert!(matches!(
        &payload.messages[3].content[0],
        loom_types::ContentPart::ToolCall { name, .. } if name == "exec_command"
    ));
    assert!(matches!(
        &payload.messages[4].content[0],
        loom_types::ContentPart::ToolResult { tool_call_id, .. } if tool_call_id == "call_1"
    ));
}

#[test]
fn codex_scan_missing_dir_returns_empty() {
    let summaries =
        loom_import::codex::scan(std::path::Path::new("/does/not/exist")).expect("scan ok");
    assert!(summaries.is_empty());
}

#[test]
fn codex_build_payload_no_messages_errors() {
    let dir = tempfile::tempdir().expect("tmpdir");
    let p = dir.path().join("rollout-x-test.jsonl");
    std::fs::write(&p, r#"{"timestamp":"2026-07-10T03:00:00.000Z","type":"session_meta","payload":{"session_id":"test-empty","cwd":"F:\\proj"}}"#).unwrap();
    let res = loom_import::codex::build_payload(&p);
    assert!(res.is_err());
}
