use anyhow::Result;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;

const V4_MIGRATION: &str = r#"
CREATE TABLE IF NOT EXISTS thread_todos (
    id              TEXT PRIMARY KEY,
    session_id      TEXT NOT NULL,
    content         TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'pending'
        CHECK(status IN ('pending', 'in_progress', 'completed')),
    plan_id         TEXT,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_thread_todos_session ON thread_todos(session_id);
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub id: String,
    pub session_id: String,
    pub content: String,
    pub status: String,
    pub plan_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub struct TodoStore {
    conn: Mutex<Connection>,
}

impl TodoStore {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA busy_timeout=5000;
             PRAGMA foreign_keys=ON;",
        )?;
        conn.execute_batch(V4_MIGRATION)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Replace all todos for a session — deletes existing and inserts new in a transaction.
    pub fn replace_todos(&self, session_id: &str, todos: &[TodoItem]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        tx.execute("DELETE FROM thread_todos WHERE session_id = ?1", params![session_id])?;
        for todo in todos {
            tx.execute(
                "INSERT INTO thread_todos (id, session_id, content, status, plan_id, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    todo.id,
                    todo.session_id,
                    todo.content,
                    todo.status,
                    todo.plan_id,
                    todo.created_at,
                    todo.updated_at,
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// List all todos for a session, ordered by creation time.
    pub fn list_todos(&self, session_id: &str) -> Result<Vec<TodoItem>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, session_id, content, status, plan_id, created_at, updated_at
             FROM thread_todos
             WHERE session_id = ?1
             ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map(params![session_id], |row| {
            Ok(TodoItem {
                id: row.get(0)?,
                session_id: row.get(1)?,
                content: row.get(2)?,
                status: row.get(3)?,
                plan_id: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })?;
        let mut todos = Vec::new();
        for row in rows {
            todos.push(row?);
        }
        Ok(todos)
    }

    /// Update the status of a single todo.
    pub fn update_todo_status(&self, session_id: &str, todo_id: &str, status: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE thread_todos SET status = ?1, updated_at = datetime('now') WHERE id = ?2 AND session_id = ?3",
            params![status, todo_id, session_id],
        )?;
        Ok(())
    }

    /// Delete all todos for a session.
    pub fn clear_todos(&self, session_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM thread_todos WHERE session_id = ?1",
            params![session_id],
        )?;
        Ok(())
    }
}
