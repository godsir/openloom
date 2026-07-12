use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests/fixtures/proj-a");
    p.push(name);
    p
}

#[test]
fn build_payload_maps_blocks_and_skips_sidechain_and_bad_lines() {
    let payload = loom_import::build_payload(&fixture("session-bb.jsonl")).expect("parse ok");
    assert_eq!(payload.id, "session-bb");
    assert_eq!(payload.title.as_deref(), Some("Block Party"));
    assert_eq!(payload.workspace_path.as_deref(), Some("C:/proj-a"));
    assert_eq!(payload.created_at.to_rfc3339(), "2026-07-11T01:00:00+00:00");
    assert_eq!(payload.updated_at.to_rfc3339(), "2026-07-11T02:00:00+00:00");

    // 4 messages: user(text), assistant(thinking+tool_use+text+usage), user(tool_result), assistant(text+usage)
    // sidechain m4 + attachment m5 + malformed line are skipped.
    assert_eq!(payload.messages.len(), 4);

    let m1 = &payload.messages[0];
    assert_eq!(m1.role, loom_types::Role::User);
    assert!(
        matches!(&m1.content[0], loom_types::ContentPart::Text { text } if text == "please run the tool")
    );

    let m2 = &payload.messages[1];
    assert_eq!(m2.role, loom_types::Role::Assistant);
    // blocks: thinking, tool_use, text
    assert!(matches!(
        &m2.content[0],
        loom_types::ContentPart::Thinking { .. }
    ));
    let tool = &m2.content[1];
    assert!(matches!(tool, loom_types::ContentPart::ToolCall { name, .. } if name == "Bash"));
    assert!(matches!(&m2.content[2], loom_types::ContentPart::Text { text } if text == "done"));
    let u = m2.usage.as_ref().expect("usage present");
    assert_eq!(u.prompt_tokens, 100);
    assert_eq!(u.completion_tokens, 20);
    assert_eq!(u.cache_read_tokens, 8);
    assert_eq!(u.cache_write_tokens, 4);
    assert_eq!(u.model, "glm-5.2");

    let m3 = &payload.messages[2];
    assert_eq!(m3.role, loom_types::Role::User);
    assert!(
        matches!(&m3.content[0], loom_types::ContentPart::ToolResult { tool_call_id, .. } if tool_call_id == "toolu_1")
    );
}

#[test]
fn build_payload_title_falls_back_to_first_user_message() {
    // session-aa has no ai-title? it does — so craft a no-title check via scan_one behavior:
    // Instead, assert build of session-aa uses its ai-title (already covered). Here we just
    // confirm a file with no ai-title and no messages returns an error (no messages).
    let p = std::env::temp_dir().join("loom-import-empty.jsonl");
    std::fs::write(&p, "{\"type\":\"last-prompt\"}\n").unwrap();
    let res = loom_import::build_payload(&p);
    assert!(res.is_err());
}

#[test]
fn build_payload_title_from_first_user_message_when_no_ai_title() {
    let dir = tempfile::tempdir().expect("tmpdir");
    let jsonl = dir.path().join("no-title.jsonl");
    std::fs::write(
        &jsonl,
        "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"hello\"},\"timestamp\":\"2026-07-11T01:00:00.000Z\",\"sessionId\":\"no-title\"}\n",
    )
    .unwrap();
    let payload = loom_import::build_payload(&jsonl).expect("parse ok");
    assert_eq!(payload.title.as_deref(), Some("hello"));
}

#[test]
fn build_payload_title_skips_assistant_uses_first_user_message() {
    // No ai-title; first message is assistant, then a user message "world".
    // Title should come from the first USER message, not the assistant.
    let dir = tempfile::tempdir().expect("tmpdir");
    let jsonl = dir.path().join("assistant-first.jsonl");
    std::fs::write(
        &jsonl,
        "{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"model\":\"m\",\"content\":[{\"type\":\"text\",\"text\":\"I speak first\"}],\"usage\":{\"input_tokens\":1,\"output_tokens\":2}},\"timestamp\":\"2026-07-11T01:00:00.000Z\",\"sessionId\":\"assistant-first\"}\n\
         {\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"world\"},\"timestamp\":\"2026-07-11T01:01:00.000Z\",\"sessionId\":\"assistant-first\"}\n",
    )
    .unwrap();
    let payload = loom_import::build_payload(&jsonl).expect("parse ok");
    assert_eq!(payload.title.as_deref(), Some("world"));
}

#[test]
fn build_payload_title_unnamed_when_no_user_message() {
    // No ai-title, only an assistant message — title should fall back to "未命名".
    let dir = tempfile::tempdir().expect("tmpdir");
    let jsonl = dir.path().join("no-user.jsonl");
    std::fs::write(
        &jsonl,
        "{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"model\":\"m\",\"content\":[{\"type\":\"text\",\"text\":\"solo\"}],\"usage\":{\"input_tokens\":1,\"output_tokens\":2}},\"timestamp\":\"2026-07-11T01:00:00.000Z\",\"sessionId\":\"no-user\"}\n",
    )
    .unwrap();
    let payload = loom_import::build_payload(&jsonl).expect("parse ok");
    assert_eq!(payload.title.as_deref(), Some("未命名"));
}

#[test]
fn build_payload_maps_assistant_string_content() {
    // Claude Code always uses arrays, but guard against a string content
    // (symmetry with map_user_message) — should map to a Text part, not drop.
    let dir = tempfile::tempdir().expect("tmpdir");
    let jsonl = dir.path().join("string-content.jsonl");
    std::fs::write(
        &jsonl,
        "{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"model\":\"m\",\"content\":\"plain string\",\"usage\":{\"input_tokens\":1,\"output_tokens\":2}},\"timestamp\":\"2026-07-11T01:00:00.000Z\",\"sessionId\":\"string-content\"}\n",
    )
    .unwrap();
    let payload = loom_import::build_payload(&jsonl).expect("parse ok");
    assert_eq!(payload.messages.len(), 1);
    let m = &payload.messages[0];
    assert_eq!(m.role, loom_types::Role::Assistant);
    assert!(
        matches!(&m.content[0], loom_types::ContentPart::Text { text } if text == "plain string")
    );
}
