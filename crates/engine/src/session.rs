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
    ListArchived {
        reply: oneshot::Sender<Vec<SessionInfo>>,
    },
    UpdateCount {
        id: String,
        count: usize,
    },
    Archive {
        id: String,
        reply: oneshot::Sender<bool>,
    },
    Delete {
        id: String,
        reply: oneshot::Sender<bool>,
    },
    DeleteArchived {
        id: String,
        reply: oneshot::Sender<bool>,
    },
    Restore {
        id: String,
        reply: oneshot::Sender<bool>,
    },
    Cleanup {
        max_age_days: u32,
        reply: oneshot::Sender<usize>,
    },
    Rename {
        id: String,
        title: String,
        reply: oneshot::Sender<bool>,
    },
    Pin {
        id: String,
        pinned_at: Option<String>,
        reply: oneshot::Sender<bool>,
    },
}

pub(crate) fn spawn_session_thread(db_path: PathBuf) -> std::sync::mpsc::Sender<SessionCommand> {
    let (tx, rx) = std::sync::mpsc::channel::<SessionCommand>();
    std::thread::spawn(move || {
        let conn = rusqlite::Connection::open(&db_path).expect("session db open");
        conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                created_at TEXT NOT NULL,
                message_count INTEGER DEFAULT 0,
                title TEXT,
                pinned_at TEXT,
                archived_at TEXT
            );",
        )
        .unwrap();

        // Migration: add archived_at column if missing (for existing DBs)
        let has_col: bool = conn
            .prepare("SELECT archived_at FROM sessions LIMIT 0")
            .is_ok();
        if !has_col {
            let _ = conn.execute_batch("ALTER TABLE sessions ADD COLUMN archived_at TEXT;");
        }

        // Migration: add title column if missing (for DBs created before V6)
        let has_title: bool = conn.prepare("SELECT title FROM sessions LIMIT 0").is_ok();
        if !has_title {
            let _ = conn.execute_batch("ALTER TABLE sessions ADD COLUMN title TEXT;");
        }

        // Migration: add pinned_at column if missing (for DBs created before V6)
        let has_pinned: bool = conn
            .prepare("SELECT pinned_at FROM sessions LIMIT 0")
            .is_ok();
        if !has_pinned {
            let _ = conn.execute_batch("ALTER TABLE sessions ADD COLUMN pinned_at TEXT;");
        }

        let store = SessionStore::new(&conn);
        for cmd in rx {
            match cmd {
                SessionCommand::Create { reply } => {
                    let id = uuid::Uuid::new_v4().to_string();
                    let info = SessionInfo {
                        id: id.clone(),
                        created_at: Utc::now(),
                        message_count: 0,
                        title: None,
                        pinned_at: None,
                        archived_at: None,
                    };
                    if let Err(e) = store.insert(&info.id, info.created_at) {
                        tracing::error!(session_id = %id, "failed to insert session: {e}");
                    }
                    let _ = reply.send(info);
                }
                SessionCommand::List { reply } => {
                    let sessions = match store.list_active(100) {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::error!("session list_active failed: {e}");
                            Vec::new()
                        }
                    };
                    let _ = reply.send(sessions);
                }
                SessionCommand::ListArchived { reply } => {
                    let sessions = store.list_archived(100).unwrap_or_default();
                    let _ = reply.send(sessions);
                }
                SessionCommand::UpdateCount { id, count } => {
                    let _ = store.update_message_count(&id, count);
                }
                SessionCommand::Archive { id, reply } => {
                    let now = Utc::now().to_rfc3339();
                    let ok = store.archive(&id, &now).is_ok();
                    let _ = reply.send(ok);
                }
                SessionCommand::Delete { id, reply } => {
                    let ok = store.delete(&id).is_ok();
                    let _ = reply.send(ok);
                }
                SessionCommand::DeleteArchived { id, reply } => {
                    let ok = store.delete(&id).is_ok();
                    let _ = reply.send(ok);
                }
                SessionCommand::Restore { id, reply } => {
                    let ok = store.restore(&id).is_ok();
                    let _ = reply.send(ok);
                }
                SessionCommand::Cleanup {
                    max_age_days,
                    reply,
                } => {
                    let deleted = store.cleanup_archived(max_age_days).unwrap_or(0);
                    let _ = reply.send(deleted);
                }
                SessionCommand::Rename { id, title, reply } => {
                    let ok = store.rename(&id, &title).is_ok();
                    let _ = reply.send(ok);
                }
                SessionCommand::Pin {
                    id,
                    pinned_at,
                    reply,
                } => {
                    let ok = store.pin(&id, pinned_at.as_deref()).is_ok();
                    let _ = reply.send(ok);
                }
            }
        }
    });
    tx
}
