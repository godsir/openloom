use std::path::PathBuf;

pub(crate) struct TokenUsageRecord {
    pub(crate) session_id: String,
    pub(crate) model: String,
    pub(crate) prompt_tokens: usize,
    pub(crate) completion_tokens: usize,
    pub(crate) cached_tokens: usize,
    pub(crate) latency_ms: u64,
}

pub(crate) fn spawn_token_store_thread(
    db_path: PathBuf,
) -> std::sync::mpsc::Sender<TokenUsageRecord> {
    let (tx, rx) = std::sync::mpsc::channel::<TokenUsageRecord>();
    std::thread::spawn(move || {
        let conn = match rusqlite::Connection::open(&db_path) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("token_store thread: cannot open db: {}", e);
                return;
            }
        };
        let _ = conn.execute_batch("PRAGMA journal_mode=WAL;");
        let store = openloom_memory::store::TokenStore::new(&conn);
        for record in rx {
            let _ = store.insert(
                &record.session_id,
                &record.model,
                record.prompt_tokens,
                record.completion_tokens,
                record.cached_tokens,
                record.latency_ms,
            );
        }
    });
    tx
}
