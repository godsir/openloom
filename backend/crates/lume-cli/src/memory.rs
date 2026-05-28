//! Memory store implementation for the CLI — wraps loom-memory's SqliteEventStore.
//! Persists chat messages to message_history table, extracts cognitions from
//! conversation text, and loads persona from accumulated trait data.

use anyhow::Result;
use loom_core::MemoryStore;
use loom_memory::{
    AgentConfigStore, CognitionStore, CognitionsPersonaProvider, GraphStore, ModelConfigStore,
    NewEvent, SqliteEventStore,
};
use loom_types::{AgentConfig, Message, ModelConfig, PersonaProvider};

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
        Ok(Self {
            store: std::sync::Mutex::new(store),
        })
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
    async fn save_turn(
        &self,
        session_id: &str,
        user_msg: &str,
        assistant_msg: &str,
        tools: usize,
        prompt_tokens: usize,
        completion_tokens: usize,
    ) -> Result<i64> {
        let event_id = {
            let store = self.store.lock().unwrap();
            let conn = store.conn();
            let now = chrono::Utc::now().to_rfc3339();
            let usage_meta = serde_json::json!({"prompt_tokens": prompt_tokens, "completion_tokens": completion_tokens}).to_string();

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
                "INSERT INTO message_history (session_id, seq, role, content, timestamp, metadata) VALUES (?1, ?2, 'assistant', ?3, ?4, ?5)",
                rusqlite::params![session_id, seq + 1, assistant_msg, now, usage_meta],
            )?;
            conn.execute(
                "UPDATE sessions SET message_count = message_count + 2 WHERE id = ?1",
                rusqlite::params![session_id],
            )?;

            let event = NewEvent {
                timestamp: chrono::Utc::now(),
                event_type: "chat_turn".into(),
                action: "chat".into(),
                context: format!(
                    "User: {} | Assistant: {}...",
                    truncate(user_msg, 200),
                    truncate(assistant_msg, 200)
                ),
                confidence: 1.0,
                source_session: Some(session_id.to_string()),
                source_text: user_msg.to_string(),
                payload: Some(
                    serde_json::json!({"assistant_response": assistant_msg, "tool_calls": tools, "prompt_tokens": prompt_tokens, "completion_tokens": completion_tokens}),
                ),
            };
            store.insert_event(&event)?
        };
        tracing::debug!(session_id, event_id, "chat turn saved");
        Ok(event_id)
    }

    async fn load_history(&self, session_id: &str, limit: usize) -> Result<Vec<Message>> {
        let store = self.store.lock().unwrap();
        let mut stmt = store.conn().prepare(
            "SELECT role, content, metadata FROM message_history WHERE session_id = ?1 ORDER BY seq ASC LIMIT ?2"
        )?;
        let rows = stmt.query_map(rusqlite::params![session_id, limit as i64], |row| {
            let role: String = row.get(0)?;
            let content: String = row.get(1)?;
            let metadata: Option<String> = row.get(2)?;
            let usage = metadata.and_then(|m| {
                let v: serde_json::Value = serde_json::from_str(&m).ok()?;
                Some(loom_types::TokenUsage {
                    prompt_tokens: v["prompt_tokens"].as_u64()? as usize,
                    completion_tokens: v["completion_tokens"].as_u64()? as usize,
                    cached_tokens: 0,
                    latency_ms: 0,
                })
            });
            // Try to parse content as JSON (structured ContentParts). Fall back to plain text.
            let parts: Vec<loom_types::ContentPart> =
                serde_json::from_str(&content).unwrap_or_else(|_| {
                    vec![loom_types::ContentPart::Text { text: content.clone() }]
                });
            let role_enum = match role.as_str() {
                "user" => loom_types::Role::User,
                "assistant" => loom_types::Role::Assistant,
                "system" => loom_types::Role::System,
                _ => loom_types::Role::User,
            };
            Ok(Message {
                role: role_enum,
                content: parts,
                timestamp: chrono::Utc::now(),
                usage,
            })
        })?;
        let mut msgs = Vec::new();
        for r in rows {
            msgs.push(r?);
        }
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
            ("rust", "rust"),
            ("python", "python"),
            ("typescript", "typescript"),
            ("golang", "golang"),
            ("java", "java"),
            ("c++", "cpp"),
            ("c#", "csharp"),
            ("javascript", "javascript"),
            ("react", "react"),
            ("vue", "vue"),
            ("electron", "electron"),
            ("tauri", "tauri"),
            ("node", "nodejs"),
            ("docker", "docker"),
            ("kubernetes", "k8s"),
            ("sql", "sql"),
            ("postgres", "postgresql"),
            ("sqlite", "sqlite"),
            ("redis", "redis"),
            ("git", "git"),
            ("linux", "linux"),
            ("windows", "windows"),
        ];
        for (keyword, tag) in tech_keywords {
            if lower.contains(keyword) && !lower.contains(&format!("not {}", keyword)) {
                if cognition
                    .insert("USER", &format!("uses_{}", tag), keyword, 0.5, 1, "global")
                    .is_ok()
                {
                    triggered.push(format!("uses_{}", tag));
                }
                // Also upsert to knowledge graph
                let _ = graph.upsert_node(keyword, "Technology", keyword, 0.5, "global");
            }
        }

        // 2. AI/ML keywords
        for kw in &[
            "ai",
            "machine learning",
            "deep learning",
            "llm",
            "agent",
            "mcp",
            "lsp",
            "skill",
            "claude",
            "openai",
            "deepseek",
            "lm studio",
            "ollama",
        ] {
            if lower.contains(kw) {
                if cognition
                    .insert(
                        "USER",
                        &format!("interest_{}", kw.replace(' ', "_")),
                        kw,
                        0.5,
                        1,
                        "global",
                    )
                    .is_ok()
                {
                    triggered.push(format!("interest_{}", kw.replace(' ', "_")));
                }
                let _ = graph.upsert_node(kw, "Concept", kw, 0.5, "global");
            }
        }

        // 3. Chinese patterns: preferences, goals, habits
        let cn_patterns = [
            ("我喜欢", "preference"),
            ("我想", "goal"),
            ("我需要", "need"),
            ("我习惯", "habit"),
            ("我在做", "working_on"),
            ("我在用", "using"),
            ("我的项目", "project"),
            ("我公司", "company"),
            ("我团队", "team"),
        ];
        for (prefix, trait_name) in &cn_patterns {
            if let Some(pos) = text.find(prefix) {
                let snippet: String = text[pos..].chars().take(30).collect();
                if cognition
                    .insert("USER", trait_name, &snippet, 0.4, 1, "global")
                    .is_ok()
                {
                    triggered.push(trait_name.to_string());
                }
            }
        }

        // 4. Always record the conversation topic (first 100 chars)
        let topic: String = text.chars().take(100).collect();
        if cognition
            .insert("USER", "last_topic", &topic, 0.3, 1, "global")
            .is_ok()
        {
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
        &self,
        entities: &[loom_memory::ExtractedEntity],
        relationships: &[loom_memory::ExtractedRelationship],
        source_event_id: i64,
    ) -> Result<(usize, usize)> {
        let store = self.store.lock().unwrap();
        let graph = loom_memory::GraphStore::new(store.conn());
        let mut node_ids = std::collections::HashMap::new();
        let mut node_count = 0;
        let mut edge_count = 0;

        for e in entities {
            if let Ok(id) = graph.upsert_node(
                &e.name,
                &e.entity_type,
                &e.description,
                e.confidence,
                &e.scope,
            ) {
                node_ids.insert(e.name.clone(), id);
                node_count += 1;
                for alias in &e.aliases {
                    let _ = graph.add_alias(id, alias);
                }
                // Wire evidence to the most recent event for this session
                // (best-effort; evidence is non-critical for operation)
            }
        }
        // Wire evidence for nodes
        for (_, node_id) in &node_ids {
            let _ = graph.link_evidence_node(*node_id, source_event_id);
        }
        // Wire evidence for edges
        for r in relationships {
            let src = node_ids.get(&r.source_name).copied();
            let tgt = node_ids.get(&r.target_name).copied();
            if let (Some(s), Some(t)) = (src, tgt) {
                if let Ok(edge_id) =
                    graph.upsert_edge(s, t, &r.relation_type, &r.fact, r.confidence, &r.scope)
                {
                    edge_count += 1;
                    let _ = graph.link_evidence_edge(edge_id, source_event_id);
                }
            }
        }
        Ok((node_count, edge_count))
    }

    async fn save_extracted_entities(
        &self,
        entities: &[loom_memory::ExtractedEntity],
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
            let _ = cognition.insert(
                "USER",
                &format!("entity_{}", e.entity_type.to_lowercase()),
                &value,
                e.confidence,
                1,
                "global",
            );
        }
        Ok(())
    }

    async fn save_agent_config(&self, config: &AgentConfig) -> Result<()> {
        let store = self.store.lock().unwrap();
        AgentConfigStore::new(store.conn()).upsert(config)
    }

    async fn get_agent_config(&self, name: &str) -> Result<Option<AgentConfig>> {
        let store = self.store.lock().unwrap();
        AgentConfigStore::new(store.conn()).get(name)
    }

    async fn list_agent_configs(&self) -> Result<Vec<AgentConfig>> {
        let store = self.store.lock().unwrap();
        AgentConfigStore::new(store.conn()).list()
    }

    async fn delete_agent_config(&self, name: &str) -> Result<()> {
        let store = self.store.lock().unwrap();
        AgentConfigStore::new(store.conn()).delete(name)
    }

    async fn save_session_agent_name(
        &self,
        session_id: &str,
        agent_config_name: &str,
    ) -> Result<()> {
        let store = self.store.lock().unwrap();
        AgentConfigStore::new(store.conn()).set_session_binding(session_id, agent_config_name)
    }

    async fn get_session_agent_name(&self, session_id: &str) -> Result<Option<String>> {
        let store = self.store.lock().unwrap();
        AgentConfigStore::new(store.conn()).get_session_binding(session_id)
    }

    async fn save_model_config(&self, config: &ModelConfig) -> Result<()> {
        let store = self.store.lock().unwrap();
        ModelConfigStore::new(store.conn()).upsert(config)
    }

    async fn get_model_config(&self, name: &str) -> Result<Option<ModelConfig>> {
        let store = self.store.lock().unwrap();
        ModelConfigStore::new(store.conn()).get(name)
    }

    async fn list_model_configs(&self) -> Result<Vec<ModelConfig>> {
        let store = self.store.lock().unwrap();
        ModelConfigStore::new(store.conn()).list()
    }

    async fn delete_model_config(&self, name: &str) -> Result<()> {
        let store = self.store.lock().unwrap();
        ModelConfigStore::new(store.conn()).delete(name)
    }

    async fn set_active_model(&self, name: &str) -> Result<()> {
        let store = self.store.lock().unwrap();
        ModelConfigStore::new(store.conn()).set_active(name)
    }

    async fn get_active_model(&self) -> Result<Option<ModelConfig>> {
        let store = self.store.lock().unwrap();
        ModelConfigStore::new(store.conn()).get_active()
    }

    async fn query_kg_context(&self, entity_names: &[&str], limit: usize) -> Result<String> {
        let store = self.store.lock().unwrap();
        let graph = GraphStore::new(store.conn());
        let mut lines: Vec<String> = Vec::new();
        const MIN_CONFIDENCE: f64 = 0.5;

        // Always include the USER node
        if let Ok(Some(_user_id)) = graph.resolve_node("USER") {
            let neighbors = graph.neighbors("USER", None, limit)?;
            for n in &neighbors {
                if n.confidence < MIN_CONFIDENCE {
                    continue;
                }
                let relation = n.relation_type.as_deref().unwrap_or("related_to");
                lines.push(format!(
                    "- USER {} {} (confidence: {:.2})",
                    relation, n.name, n.confidence
                ));
            }
        }

        // Query each entity name
        for name in entity_names {
            if *name == "USER" || name.is_empty() {
                continue;
            }
            if let Ok(results) = graph.search_entities(name, 3) {
                for r in &results {
                    if r.name == "USER" || r.confidence < MIN_CONFIDENCE {
                        continue;
                    }
                    lines.push(format!(
                        "- {} is a {}: {} (confidence: {:.2})",
                        r.name, r.entity_type, r.description, r.confidence
                    ));
                    // Get immediate neighbors
                    if let Ok(neighbors) = graph.neighbors(&r.name, None, 3) {
                        for n in &neighbors {
                            if n.name == "USER" || n.name == r.name {
                                continue;
                            }
                            if n.confidence < MIN_CONFIDENCE {
                                continue;
                            }
                            let relation = n.relation_type.as_deref().unwrap_or("related_to");
                            lines.push(format!("  └ {} {} {}", r.name, relation, n.name));
                        }
                    }
                }
            }
        }

        lines.dedup();
        if lines.is_empty() {
            Ok(String::new())
        } else {
            Ok(format!("## Knowledge Graph\n{}", lines.join("\n")))
        }
    }

    async fn list_sessions(&self) -> Result<Vec<(String, String, usize, Option<String>)>> {
        let store = self.store.lock().unwrap();
        let mut stmt = store.conn().prepare(
            "SELECT id, created_at, message_count, title FROM sessions ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)? as usize,
                row.get(3)?,
            ))
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    async fn ensure_session(&self, id: &str) -> Result<()> {
        let store = self.store.lock().unwrap();
        store.conn().execute(
            "INSERT OR IGNORE INTO sessions (id, created_at, message_count) VALUES (?1, datetime('now'), 0)",
            rusqlite::params![id],
        )?;
        Ok(())
    }

    async fn prune_memory(&self) -> Result<usize> {
        let store = self.store.lock().unwrap();
        let graph = GraphStore::new(store.conn());
        // Only prune when entity count exceeds threshold
        let count = graph.node_count()?;
        if count <= 500 {
            return Ok(0);
        }
        let pruned = graph.prune_stale(30, 100)?;
        if pruned > 0 {
            tracing::info!(pruned, total = count, "memory pruned");
        }
        Ok(pruned)
    }

    async fn search_knowledge(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<(String, String, String, f64)>> {
        let store = self.store.lock().unwrap();
        let graph = GraphStore::new(store.conn());
        let results = graph.search_entities(query, limit)?;
        Ok(results
            .iter()
            .map(|r| {
                (
                    r.name.clone(),
                    r.entity_type.clone(),
                    r.description.clone(),
                    r.confidence,
                )
            })
            .collect())
    }

    async fn kg_node_count(&self) -> Result<usize> {
        let store = self.store.lock().unwrap();
        GraphStore::new(store.conn()).node_count()
    }

    async fn kg_edge_count(&self) -> Result<usize> {
        let store = self.store.lock().unwrap();
        GraphStore::new(store.conn()).edge_count()
    }

    async fn kg_neighbors(&self, node_name: &str, limit: usize) -> Result<loom_types::KgGraph> {
        let store = self.store.lock().unwrap();
        let graph = GraphStore::new(store.conn());
        let rows = graph.neighbors(node_name, None, limit)?;
        let nodes: Vec<loom_types::KgNode> = rows
            .iter()
            .map(|r| loom_types::KgNode {
                node_id: r.node_id,
                name: r.name.clone(),
                entity_type: r.entity_type.clone(),
                description: r.description.clone(),
                confidence: r.confidence,
            })
            .collect();
        let edges: Vec<loom_types::KgEdge> = rows
            .iter()
            .filter_map(|r| {
                r.relation_type.as_ref().map(|rel| loom_types::KgEdge {
                    source: node_name.to_string(),
                    target: r.name.clone(),
                    relation_type: rel.clone(),
                    fact: String::new(),
                    confidence: r.confidence,
                })
            })
            .collect();
        Ok(loom_types::KgGraph { nodes, edges })
    }

    async fn kg_walk(
        &self,
        start_name: &str,
        max_depth: u8,
        limit: usize,
    ) -> Result<loom_types::KgGraph> {
        let store = self.store.lock().unwrap();
        let graph = GraphStore::new(store.conn());
        let rows = graph.walk(start_name, max_depth, None, limit)?;
        let nodes: Vec<loom_types::KgNode> = rows
            .iter()
            .map(|r| loom_types::KgNode {
                node_id: r.node_id,
                name: r.name.clone(),
                entity_type: r.entity_type.clone(),
                description: r.description.clone(),
                confidence: r.confidence,
            })
            .collect();
        Ok(loom_types::KgGraph {
            nodes,
            edges: Vec::new(),
        })
    }

    async fn delete_session(&self, id: &str) -> Result<()> {
        let store = self.store.lock().unwrap();
        store
            .conn()
            .execute("DELETE FROM sessions WHERE id = ?1", rusqlite::params![id])?;
        Ok(())
    }

    async fn get_summary(&self, session_id: &str) -> Result<Option<String>> {
        let store = self.store.lock().unwrap();
        let result = store.conn().query_row(
            "SELECT summary FROM sessions WHERE id = ?1",
            rusqlite::params![session_id],
            |row| row.get::<_, String>(0),
        );
        match result {
            Ok(s) if s.is_empty() => Ok(None),
            Ok(s) => Ok(Some(s)),
            Err(_) => Ok(None),
        }
    }

    async fn save_summary(&self, session_id: &str, summary: &str) -> Result<()> {
        let store = self.store.lock().unwrap();
        // Also record the current message count so we know when to re-summarize
        let count: i64 = store
            .conn()
            .query_row(
                "SELECT message_count FROM sessions WHERE id = ?1",
                rusqlite::params![session_id],
                |row| row.get(0),
            )
            .unwrap_or(0);
        store.conn().execute(
            "UPDATE sessions SET summary = ?1, summary_at_count = ?2 WHERE id = ?3",
            rusqlite::params![summary, count, session_id],
        )?;
        Ok(())
    }

    async fn get_summary_at_count(&self, session_id: &str) -> Result<usize> {
        let store = self.store.lock().unwrap();
        let result = store.conn().query_row(
            "SELECT summary_at_count FROM sessions WHERE id = ?1",
            rusqlite::params![session_id],
            |row| row.get::<_, i64>(0),
        );
        Ok(result.unwrap_or(0) as usize)
    }

    async fn get_message_count(&self, session_id: &str) -> Result<usize> {
        let store = self.store.lock().unwrap();
        let result = store.conn().query_row(
            "SELECT message_count FROM sessions WHERE id = ?1",
            rusqlite::params![session_id],
            |row| row.get::<_, i64>(0),
        );
        Ok(result.unwrap_or(0) as usize)
    }

    async fn kg_list_nodes(&self, limit: usize, offset: usize) -> Result<Vec<loom_types::KgNode>> {
        let store = self.store.lock().unwrap();
        let graph = GraphStore::new(store.conn());
        let rows = graph.list_nodes(limit, offset)?;
        Ok(rows.iter().map(|r| loom_types::KgNode {
            node_id: r.node_id,
            name: r.name.clone(),
            entity_type: r.entity_type.clone(),
            description: r.description.clone(),
            confidence: r.confidence,
        }).collect())
    }

    async fn kg_delete_node(&self, name: &str) -> Result<bool> {
        let store = self.store.lock().unwrap();
        GraphStore::new(store.conn()).delete_node(name)
    }

    async fn kg_delete_edge(&self, source: &str, target: &str, relation: &str) -> Result<bool> {
        let store = self.store.lock().unwrap();
        GraphStore::new(store.conn()).delete_edge(source, target, relation)
    }
}
