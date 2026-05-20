use chrono::Utc;
use openloom_memory::store::SessionStore;
use openloom_models::SessionInfo;
use std::path::PathBuf;
use tokio::sync::oneshot;

pub(crate) enum SessionCommand {
    Create {
        reply: oneshot::Sender<SessionInfo>,
    },
    List {
        reply: oneshot::Sender<Vec<SessionInfo>>,
    },
    UpdateCount {
        id: String,
        count: usize,
    },
}

pub(crate) fn spawn_session_thread(db_path: PathBuf) -> std::sync::mpsc::Sender<SessionCommand> {
    let (tx, rx) = std::sync::mpsc::channel::<SessionCommand>();
    std::thread::spawn(move || {
        let conn = rusqlite::Connection::open(&db_path).expect("session db open");
        conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY, created_at TEXT NOT NULL, message_count INTEGER DEFAULT 0
            );",
        )
        .unwrap();
        let store = SessionStore::new(&conn);
        for cmd in rx {
            match cmd {
                SessionCommand::Create { reply } => {
                    let id = uuid::Uuid::new_v4().to_string();
                    let info = SessionInfo {
                        id: id.clone(),
                        created_at: Utc::now(),
                        message_count: 0,
                    };
                    let _ = store.insert(&info.id, info.created_at);
                    let _ = reply.send(info);
                }
                SessionCommand::List { reply } => {
                    let sessions = store.list_all(100).unwrap_or_default();
                    let _ = reply.send(sessions);
                }
                SessionCommand::UpdateCount { id, count } => {
                    let _ = store.update_message_count(&id, count);
                }
            }
        }
    });
    tx
}
