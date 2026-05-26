//! Memory store implementation for the CLI — wraps loom-memory's SqliteEventStore.
//! Persists chat messages to message_history table, extracts cognitions from
//! conversation text, and loads persona from accumulated trait data.

use anyhow::Result;
use loom_core::MemoryStore;
use loom_memory::{CognitionStore, CognitionsPersonaProvider, GraphStore, NewEvent, SqliteEventStore};
use loom_types::{Message, PersonaProvider};

pub struct LoomMemoryStore {
    store: std::sync::Mutex<SqliteEventStore>,
}

impl LoomMemoryStore {
    pub fn open(path: &std::path::Path) -> Result<Self> {
        let store = SqliteEventStore::open(path)?;
        // Ensure a default session row exists
        store.conn().execute(
            "INSERT OR IGNORE INTO sessions (id, created_at, message_count) VALUES ('default', datetime('now'), 0)",
            [],
        )?;
        Ok(Self { store: std::sync::Mutex::new(store) })
    }
}

fn truncate(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

#[async_trait::async_trait]
impl MemoryStore for LoomMemoryStore {
    async fn save_turn(&self, session_id: &str, user_msg: &str, assistant_msg: &str, tools: usize, tokens: usize) -> Result<()> {
        {
            let store = self.store.lock().unwrap();
            let conn = store.conn();
            let now = chrono::Utc::now().to_rfc3339();

            let seq: i64 = conn.query_row(
                "SELECT COALESCE(MAX(seq), 0) + 1 FROM message_history WHERE session_id = ?1",
                rusqlite::params![session_id],
                |r| r.get(0),
            )?;
            conn.execute(
                "INSERT INTO message_history (session_id, seq, role, content, timestamp) VALUES (?1, ?2, 'user', ?3, ?4)",
                rusqlite::params![session_id, seq, user_msg, now],
            )?;
            conn.execute(
                "INSERT INTO message_history (session_id, seq, role, content, timestamp) VALUES (?1, ?2, 'assistant', ?3, ?4)",
                rusqlite::params![session_id, seq + 1, assistant_msg, now],
            )?;
            conn.execute(
                "UPDATE sessions SET message_count = message_count + 2 WHERE id = ?1",
                rusqlite::params![session_id],
            )?;

            let event = NewEvent {
                timestamp: chrono::Utc::now(),
                event_type: "chat_turn".into(),
                action: "chat".into(),
                context: format!("User: {} | Assistant: {}...", truncate(user_msg, 200), truncate(assistant_msg, 200)),
                confidence: 1.0,
                source_session: Some(session_id.to_string()),
                source_text: user_msg.to_string(),
                payload: Some(serde_json::json!({"assistant_response": assistant_msg, "tool_calls": tools, "tokens": tokens})),
            };
            store.insert_event(&event)?;
        }
        tracing::debug!(session_id, "chat turn saved");
        Ok(())
    }

    async fn load_history(&self, session_id: &str, limit: usize) -> Result<Vec<Message>> {
        let store = self.store.lock().unwrap();
        let mut stmt = store.conn().prepare(
            "SELECT role, content FROM message_history WHERE session_id = ?1 ORDER BY seq ASC LIMIT ?2"
        )?;
        let rows = stmt.query_map(rusqlite::params![session_id, limit as i64], |row| {
            let role: String = row.get(0)?;
            let content: String = row.get(1)?;
            Ok(match role.as_str() {
                "user" => Message::user(&content),
                "assistant" => Message::assistant(&content),
                _ => Message::user(&content),
            })
        })?;
        let mut msgs = Vec::new();
        for r in rows { msgs.push(r?); }
        Ok(msgs)
    }

    async fn extract_cognitions(&self, session_id: &str, text: &str) -> Result<Vec<String>> {
        let store = self.store.lock().unwrap();
        let cognition = CognitionStore::new(store.conn());
        let graph = GraphStore::new(store.conn());
        let mut triggered = Vec::new();
        let lower = text.to_lowercase();

        // 1. Technology stack keywords
        let tech_keywords = &[
            ("rust", "rust"), ("python", "python"), ("typescript", "typescript"),
            ("golang", "golang"), ("java", "java"), ("c++", "cpp"), ("c#", "csharp"),
            ("javascript", "javascript"), ("react", "react"), ("vue", "vue"),
            ("electron", "electron"), ("tauri", "tauri"), ("node", "nodejs"),
            ("docker", "docker"), ("kubernetes", "k8s"), ("sql", "sql"),
            ("postgres", "postgresql"), ("sqlite", "sqlite"), ("redis", "redis"),
            ("git", "git"), ("linux", "linux"), ("windows", "windows"),
        ];
        for (keyword, tag) in tech_keywords {
            if lower.contains(keyword) && !lower.contains(&format!("not {}", keyword)) {
                if cognition.insert("USER", &format!("uses_{}", tag), keyword, 0.5, 1, "global").is_ok() {
                    triggered.push(format!("uses_{}", tag));
                }
                // Also upsert to knowledge graph
                let _ = graph.upsert_node(keyword, "Technology", keyword, 0.5, "global");
            }
        }

        // 2. AI/ML keywords
        for kw in &["ai", "machine learning", "deep learning", "llm", "agent", "mcp", "lsp", "skill", "claude", "openai", "deepseek", "lm studio", "ollama"] {
            if lower.contains(kw) {
                if cognition.insert("USER", &format!("interest_{}", kw.replace(' ', "_")), kw, 0.5, 1, "global").is_ok() {
                    triggered.push(format!("interest_{}", kw.replace(' ', "_")));
                }
                let _ = graph.upsert_node(kw, "Concept", kw, 0.5, "global");
            }
        }

        // 3. Chinese patterns: preferences, goals, habits
        let cn_patterns = [
            ("我喜欢", "preference"), ("我想", "goal"), ("我需要", "need"),
            ("我习惯", "habit"), ("我在做", "working_on"), ("我在用", "using"),
            ("我的项目", "project"), ("我公司", "company"), ("我团队", "team"),
        ];
        for (prefix, trait_name) in &cn_patterns {
            if let Some(pos) = text.find(prefix) {
                let snippet: String = text[pos..].chars().take(30).collect();
                if cognition.insert("USER", trait_name, &snippet, 0.4, 1, "global").is_ok() {
                    triggered.push(trait_name.to_string());
                }
            }
        }

        // 4. Always record the conversation topic (first 100 chars)
        let topic: String = text.chars().take(100).collect();
        if cognition.insert("USER", "last_topic", &topic, 0.3, 1, "global").is_ok() {
            triggered.push("last_topic".to_string());
        }

        // 5. Link USER node in knowledge graph
        let _ = graph.upsert_node("USER", "Person", "The user of openLoom", 1.0, "global");

        if !triggered.is_empty() {
            tracing::info!(?triggered, session_id, "cognitions extracted");
        }
        Ok(triggered)
    }

    async fn get_persona(&self) -> Result<String> {
        let rows = {
            let store = self.store.lock().unwrap();
            let cognition = CognitionStore::new(store.conn());
            cognition.query_by_subject("USER", 20, 0)?
        };
        let provider = CognitionsPersonaProvider::new(rows);
        provider.summarize().await
    }

    async fn feed_knowledge_graph(
        &self, entities: &[loom_memory::ExtractedEntity],
        relationships: &[loom_memory::ExtractedRelationship],
    ) -> Result<(usize, usize)> {
        let store = self.store.lock().unwrap();
        let graph = loom_memory::GraphStore::new(store.conn());
        let mut node_ids = std::collections::HashMap::new();
        let mut node_count = 0;
        let mut edge_count = 0;

        for e in entities {
            if let Ok(id) = graph.upsert_node(&e.name, &e.entity_type, &e.description, e.confidence, &e.scope) {
                node_ids.insert(e.name.clone(), id);
                node_count += 1;
                for alias in &e.aliases {
                    let _ = graph.add_alias(id, alias);
                }
            }
        }
        for r in relationships {
            let src = node_ids.get(&r.source_name).copied();
            let tgt = node_ids.get(&r.target_name).copied();
            if let (Some(s), Some(t)) = (src, tgt) {
                if graph.upsert_edge(s, t, &r.relation_type, &r.fact, r.confidence, &r.scope).is_ok() {
                    edge_count += 1;
                }
            }
        }
        Ok((node_count, edge_count))
    }

    async fn save_extracted_entities(
        &self, entities: &[loom_memory::ExtractedEntity],
        _relationships: &[loom_memory::ExtractedRelationship],
    ) -> Result<()> {
        let store = self.store.lock().unwrap();
        let cognition = CognitionStore::new(store.conn());
        for e in entities {
            let clean_name = e.name.trim_matches('\'').trim_matches('"');
            let clean_desc = e.description.trim_matches('\'').trim_matches('"');
            let value = if clean_desc.is_empty() || clean_desc == clean_name {
                clean_name.to_string()
            } else {
                format!("{} ({})", clean_name, clean_desc)
            };
            let _ = cognition.insert("USER", &format!("entity_{}", e.entity_type.to_lowercase()), &value, e.confidence, 1, "global");
        }
        Ok(())
    }
}
