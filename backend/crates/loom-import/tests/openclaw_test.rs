//! OpenClaw session JSONL scan + build_payload tests.

#[test]
fn openclaw_scan_and_build_payload() {
    let dir = tempfile::tempdir().expect("tmpdir");
    let sessions = dir.path().join("agents/main/sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    let jsonl = sessions.join("test-openclaw-001.jsonl");
    std::fs::write(
        &jsonl,
        concat!(
            r#"{"type":"session","version":3,"id":"test-openclaw-001","timestamp":"2026-07-10T03:03:47.000Z","cwd":"F:\\proj"}"#, "\n",
            r#"{"type":"message","message":{"role":"user","content":[{"type":"text","text":"hello openclaw"}],"timestamp":"2026-07-10T03:03:48.000Z"}}"#, "\n",
            r#"{"type":"message","message":{"role":"assistant","content":[{"type":"thinking","thinking":"th"},{"type":"text","text":"hi"}],"model":"gpt-5","usage":{"input":10,"output":5,"cacheRead":2,"cacheWrite":1},"timestamp":"2026-07-10T03:03:49.000Z"}}"#, "\n",
            r#"{"type":"message","message":{"role":"toolResult","toolCallId":"tc_1","toolName":"read","content":[{"type":"text","text":"file content"}],"timestamp":"2026-07-10T03:03:50.000Z"}}"#, "\n",
        ),
    )
    .unwrap();
    // .deleted 文件应被跳过
    std::fs::write(
        sessions.join("deleted-001.jsonl.deleted.2026-07-10T04-00-00Z"),
        "{}\n",
    )
    .unwrap();

    let summaries =
        loom_import::openclaw::scan(&dir.path().join("agents")).expect("scan ok");
    assert_eq!(summaries.len(), 1); // .deleted 跳过
    let s = &summaries[0];
    assert_eq!(s.session_uuid, "test-openclaw-001");
    assert_eq!(s.project_dir, r"F:\proj");
    assert_eq!(s.first_message.as_deref(), Some("hello openclaw"));
    assert_eq!(s.message_count, 2); // user + assistant

    let payload = loom_import::openclaw::build_payload(&jsonl).expect("parse ok");
    assert_eq!(payload.id, "test-openclaw-001");
    assert_eq!(payload.workspace_path.as_deref(), Some(r"F:\proj"));
    assert_eq!(payload.title.as_deref(), Some("hello openclaw"));
    // user(hello), assistant(thinking+hi), user(ToolResult)
    assert_eq!(payload.messages.len(), 3);
    assert_eq!(payload.messages[0].role, loom_types::Role::User);
    let m1 = &payload.messages[1];
    assert_eq!(m1.role, loom_types::Role::Assistant);
    assert!(matches!(&m1.content[0], loom_types::ContentPart::Thinking { text } if text == "th"));
    assert!(matches!(&m1.content[1], loom_types::ContentPart::Text { text } if text == "hi"));
    let u = m1.usage.as_ref().expect("usage present");
    assert_eq!(u.prompt_tokens, 10);
    assert_eq!(u.completion_tokens, 5);
    assert_eq!(u.cache_read_tokens, 2);
    assert_eq!(u.cache_write_tokens, 1);
    assert_eq!(u.model, "gpt-5");
    assert!(matches!(
        &payload.messages[2].content[0],
        loom_types::ContentPart::ToolResult { tool_call_id, name, .. }
            if tool_call_id == "tc_1" && name == "read"
    ));
}

#[test]
fn openclaw_scan_missing_dir_returns_empty() {
    let summaries =
        loom_import::openclaw::scan(std::path::Path::new("/does/not/exist")).expect("scan ok");
    assert!(summaries.is_empty());
}

#[test]
fn openclaw_build_payload_no_messages_errors() {
    let dir = tempfile::tempdir().expect("tmpdir");
    let p = dir.path().join("empty.jsonl");
    std::fs::write(&p, r#"{"type":"session","version":3,"id":"empty","timestamp":"2026-07-10T03:00:00.000Z","cwd":"F:\\proj"}"#).unwrap();
    let res = loom_import::openclaw::build_payload(&p);
    assert!(res.is_err());
}
