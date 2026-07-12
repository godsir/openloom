use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

#[test]
fn scan_reads_metadata_for_each_jsonl() {
    let summaries = loom_import::scan(&fixtures_dir()).expect("scan ok");
    // proj-a now holds two fixtures: session-aa (this test) + session-bb (build_test, Task 3).
    assert_eq!(summaries.len(), 2);
    let s = summaries
        .iter()
        .find(|s| s.session_uuid == "session-aa")
        .expect("session-aa present");
    assert_eq!(s.session_uuid, "session-aa");
    assert_eq!(s.project_dir, "proj-a");
    assert_eq!(s.title.as_deref(), Some("My Cool Chat"));
    assert_eq!(s.first_message.as_deref(), Some("hello world"));
    assert_eq!(s.message_count, 3); // 2 user + 1 assistant
    assert_eq!(s.model.as_deref(), Some("claude-sonnet-5"));
    assert_eq!(s.started_at, "2026-07-10T01:00:00.000Z");
    assert_eq!(s.last_at, "2026-07-10T02:00:00.000Z");
    assert!(!s.already_imported); // scan never marks imported
}

#[test]
fn scan_missing_dir_returns_empty() {
    let summaries = loom_import::scan(&PathBuf::from("/does/not/exist")).expect("scan ok");
    assert!(summaries.is_empty());
}
