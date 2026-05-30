//! Memory store implementation for the CLI — wraps loom-memory's SqliteEventStore.
//! Persists chat messages to message_history table, extracts cognitions from
//! conversation text, and loads persona from accumulated trait data.

use anyhow::Result;
use loom_core::MemoryStore;
use loom_memory::{
    AgentConfigStore, CognitionStore, CognitionsPersonaProvider, GraphStore, McpConfigStore,
    McpServerRow, ModelConfigStore, NewEvent, SqliteEventStore,
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

    async fn delete_message(&self, session_id: &str, index: usize) -> Result<()> {
        let store = self.store.lock().unwrap();
        let conn = store.conn();
        // Delete the message at the given index by seq order
        let deleted = conn.execute(
            "DELETE FROM message_history WHERE id IN (
                SELECT id FROM message_history WHERE session_id = ?1 ORDER BY seq ASC LIMIT 1 OFFSET ?2
            )",
            rusqlite::params![session_id, index as i64],
        )?;
        if deleted > 0 {
            conn.execute(
                "UPDATE sessions SET message_count = MAX(0, message_count - 1) WHERE id = ?1",
                rusqlite::params![session_id],
            )?;
        }
        tracing::info!(session_id, index, deleted, "delete_message");
        Ok(())
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
            let parts: Vec<loom_types::ContentPart> = serde_json::from_str(&content)
                .unwrap_or_else(|_| {
                    vec![loom_types::ContentPart::Text {
                        text: content.clone(),
                    }]
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
            cognition.query_by_subject("USER", None, 20, 0)?
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

    async fn save_mcp_server(
        &self,
        config: &lume_mcp::McpServerConfig,
        autostart: bool,
    ) -> Result<()> {
        let row = McpServerRow {
            name: config.name.clone(),
            transport: config.transport.clone(),
            command: config.command.clone(),
            args_json: serde_json::to_string(&config.args).unwrap_or_else(|_| "[]".into()),
            url: config.url.clone(),
            headers_json: serde_json::to_string(&config.headers).unwrap_or_else(|_| "{}".into()),
            env_json: serde_json::to_string(&config.env).unwrap_or_else(|_| "{}".into()),
            cwd: config.cwd.clone(),
            startup_timeout_secs: config.startup_timeout_secs,
            tool_timeout_secs: config.tool_timeout_secs,
            enabled_tools_json: config
                .enabled_tools
                .as_ref()
                .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "[]".into())),
            disabled_tools_json: config
                .disabled_tools
                .as_ref()
                .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "[]".into())),
            autostart,
        };
        let store = self.store.lock().unwrap();
        McpConfigStore::new(store.conn()).upsert(&row)
    }

    async fn list_mcp_servers(&self) -> Result<Vec<(lume_mcp::McpServerConfig, bool)>> {
        let store = self.store.lock().unwrap();
        let rows = McpConfigStore::new(store.conn()).list()?;
        let configs = rows
            .into_iter()
            .map(|r| {
                let args: Vec<String> = serde_json::from_str(&r.args_json).unwrap_or_default();
                let headers = serde_json::from_str(&r.headers_json).unwrap_or_default();
                let env = serde_json::from_str(&r.env_json).unwrap_or_default();
                let enabled_tools = r
                    .enabled_tools_json
                    .as_deref()
                    .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok());
                let disabled_tools = r
                    .disabled_tools_json
                    .as_deref()
                    .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok());
                let config = lume_mcp::McpServerConfig {
                    name: r.name,
                    transport: r.transport,
                    command: r.command,
                    args,
                    url: r.url,
                    headers,
                    env,
                    cwd: r.cwd,
                    startup_timeout_secs: r.startup_timeout_secs,
                    tool_timeout_secs: r.tool_timeout_secs,
                    enabled_tools,
                    disabled_tools,
                };
                (config, r.autostart)
            })
            .collect();
        Ok(configs)
    }

    async fn delete_mcp_server(&self, name: &str) -> Result<()> {
        let store = self.store.lock().unwrap();
        McpConfigStore::new(store.conn()).delete(name)
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
                scope: r.scope.clone(),
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

        // Also include the start node itself
        let start_id: Option<i64> = graph.resolve_node(start_name)?;
        let mut all_ids: Vec<i64> = rows.iter().map(|r| r.node_id).collect();
        if let Some(sid) = start_id {
            if !all_ids.contains(&sid) {
                all_ids.insert(0, sid);
            }
        }

        let nodes: Vec<loom_types::KgNode> = if let Some(sid) = start_id {
            // Add start node to the result
            let start_node = loom_types::KgNode {
                node_id: sid,
                name: start_name.to_string(),
                entity_type: rows.first().map(|r| r.entity_type.clone()).unwrap_or_default(),
                description: String::new(),
                confidence: 1.0,
                scope: "global".to_string(),
            };
            let mut n = vec![start_node];
            n.extend(rows.iter().map(|r| loom_types::KgNode {
                node_id: r.node_id,
                name: r.name.clone(),
                entity_type: r.entity_type.clone(),
                description: r.description.clone(),
                confidence: r.confidence,
                scope: r.scope.clone(),
            }));
            n
        } else {
            rows.iter()
                .map(|r| loom_types::KgNode {
                    node_id: r.node_id,
                    name: r.name.clone(),
                    entity_type: r.entity_type.clone(),
                    description: r.description.clone(),
                    confidence: r.confidence,
                    scope: r.scope.clone(),
                })
                .collect()
        };

        // Get edges between all found nodes
        let edge_rows = graph.edges_between(&all_ids)?;
        let edges: Vec<loom_types::KgEdge> = edge_rows
            .into_iter()
            .map(|(src, tgt, rel, conf)| loom_types::KgEdge {
                source: src,
                target: tgt,
                relation_type: rel,
                fact: String::new(),
                confidence: conf,
            })
            .collect();

        Ok(loom_types::KgGraph { nodes, edges })
    }

    async fn delete_session(&self, id: &str) -> Result<()> {
        let store = self.store.lock().unwrap();
        store.conn().execute(
            "DELETE FROM message_history WHERE session_id = ?1",
            rusqlite::params![id],
        )?;
        store
            .conn()
            .execute("DELETE FROM sessions WHERE id = ?1", rusqlite::params![id])?;
        Ok(())
    }

    async fn rename_session(&self, id: &str, title: &str) -> Result<()> {
        let store = self.store.lock().unwrap();
        store.conn().execute(
            "UPDATE sessions SET title = ?1 WHERE id = ?2",
            rusqlite::params![title, id],
        )?;
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

    async fn kg_list_nodes(
        &self,
        limit: usize,
        offset: usize,
        scope: Option<&str>,
    ) -> Result<Vec<loom_types::KgNode>> {
        let store = self.store.lock().unwrap();
        let graph = GraphStore::new(store.conn());
        let rows = graph.list_nodes(limit, offset, scope)?;
        Ok(rows
            .iter()
            .map(|r| loom_types::KgNode {
                node_id: r.node_id,
                name: r.name.clone(),
                entity_type: r.entity_type.clone(),
                description: r.description.clone(),
                confidence: r.confidence,
                scope: r.scope.clone(),
            })
            .collect())
    }

    async fn kg_delete_node(&self, name: &str) -> Result<bool> {
        let store = self.store.lock().unwrap();
        GraphStore::new(store.conn()).delete_node(name)
    }

    async fn kg_delete_edge(&self, source: &str, target: &str, relation: &str) -> Result<bool> {
        let store = self.store.lock().unwrap();
        GraphStore::new(store.conn()).delete_edge(source, target, relation)
    }

    async fn cognition_list(
        &self,
        subject: &str,
        scope: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<loom_types::Cognition>> {
        let store = self.store.lock().unwrap();
        let cognitions = loom_memory::CognitionStore::new(store.conn());
        let rows = cognitions.query_by_subject(subject, scope, limit, offset)?;
        Ok(rows
            .into_iter()
            .map(|r| loom_types::Cognition {
                id: r.id,
                subject: r.subject,
                trait_name: r.trait_name,
                value: r.value,
                confidence: r.confidence,
                evidence_count: r.evidence_count,
                first_seen: r.first_seen,
                last_updated: r.last_updated,
                version: r.version,
                scope: r.scope,
            })
            .collect())
    }

    async fn cognition_list_subjects(&self) -> Result<Vec<String>> {
        let store = self.store.lock().unwrap();
        let cognitions = loom_memory::CognitionStore::new(store.conn());
        cognitions.list_subjects()
    }

    async fn cognition_snapshots(
        &self,
        cognition_id: i64,
    ) -> Result<Vec<loom_types::CognitionHistory>> {
        let store = self.store.lock().unwrap();
        let cognitions = loom_memory::CognitionStore::new(store.conn());
        let snapshots = cognitions.snapshots_for(cognition_id)?;
        Ok(snapshots
            .into_iter()
            .map(|s| loom_types::CognitionHistory {
                id: s.id,
                version: s.version,
                trait_name: s.trait_name,
                value: s.value,
                confidence: s.confidence,
                evidence_count: s.evidence_count,
                snapshot_at: s.snapshot_at,
            })
            .collect())
    }

    async fn kg_prune(&self, older_than_days: i64) -> Result<usize> {
        let store = self.store.lock().unwrap();
        let graph = GraphStore::new(store.conn());
        graph.prune_stale(older_than_days, 1000)
    }

    async fn record_token_usage(
        &self,
        session_id: &str,
        model: &str,
        prompt_tokens: usize,
        completion_tokens: usize,
        cached_read_tokens: usize,
        cached_write_tokens: usize,
        latency_ms: u64,
        context_window: usize,
    ) -> Result<()> {
        let store = self.store.lock().unwrap();
        store.conn().execute(
            "INSERT INTO token_usage (session_id, model, prompt_tokens, completion_tokens, cached_tokens, cached_read_tokens, cached_write_tokens, latency_ms, context_window) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![session_id, model, prompt_tokens as i64, completion_tokens as i64, (cached_read_tokens + cached_write_tokens) as i64, cached_read_tokens as i64, cached_write_tokens as i64, latency_ms as i64, context_window as i64],
        )?;
        Ok(())
    }

    async fn get_token_summary(&self, from: &str, to: &str) -> Result<serde_json::Value> {
        let store = self.store.lock().unwrap();
        let conn = store.conn();

        let totals: (i64, i64, i64, i64, i64, i64, f64) = conn.query_row(
            "SELECT COALESCE(SUM(prompt_tokens), 0), COALESCE(SUM(completion_tokens), 0), COALESCE(SUM(cached_tokens), 0), COALESCE(SUM(cached_read_tokens), 0), COALESCE(SUM(cached_write_tokens), 0), COUNT(*), COALESCE(AVG(latency_ms), 0) FROM token_usage WHERE created_at >= ?1 AND created_at <= ?2",
            rusqlite::params![from, to],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?)),
        )?;

        let cache_hit_rate = if totals.0 > 0 {
            totals.3 as f64 / totals.0 as f64
        } else {
            0.0
        };

        let mut stmt = conn.prepare(
            "SELECT model, SUM(prompt_tokens) as p, SUM(completion_tokens) as c, SUM(cached_tokens) as ca, SUM(cached_read_tokens) as cr, SUM(cached_write_tokens) as cw, COUNT(*) as r, AVG(latency_ms) as l, AVG(CAST(prompt_tokens AS REAL) / NULLIF(context_window, 0)) as cu FROM token_usage WHERE created_at >= ?1 AND created_at <= ?2 GROUP BY model ORDER BY r DESC",
        )?;
        let by_model: Vec<serde_json::Value> = stmt
            .query_map(rusqlite::params![from, to], |row| {
                Ok(serde_json::json!({
                    "model": row.get::<_, String>(0)?,
                    "prompt": row.get::<_, i64>(1)?,
                    "completion": row.get::<_, i64>(2)?,
                    "cached": row.get::<_, i64>(3)?,
                    "cached_read": row.get::<_, i64>(4)?,
                    "cached_write": row.get::<_, i64>(5)?,
                    "requests": row.get::<_, i64>(6)?,
                    "avg_latency_ms": (row.get::<_, f64>(7)? * 10.0).round() / 10.0,
                    "avg_context_utilization": (row.get::<_, f64>(8)? * 100.0).round() / 100.0,
                }))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(serde_json::json!({
            "total_prompt_tokens": totals.0,
            "total_completion_tokens": totals.1,
            "total_cached_tokens": totals.2,
            "total_cached_read_tokens": totals.3,
            "total_cached_write_tokens": totals.4,
            "total_requests": totals.5,
            "avg_latency_ms": (totals.6 * 10.0).round() / 10.0,
            "cache_hit_rate": (cache_hit_rate * 100.0).round() / 100.0,
            "by_model": by_model,
        }))
    }

    async fn get_token_history(
        &self,
        from: &str,
        to: &str,
        granularity: &str,
    ) -> Result<serde_json::Value> {
        let store = self.store.lock().unwrap();
        let conn = store.conn();

        let date_format = match granularity {
            "hour" => "%Y-%m-%d %H:00",
            "week" => "%Y-%W",
            _ => "%Y-%m-%d",
        };

        let sql = format!(
            "SELECT strftime('{}', created_at) as bucket, model, SUM(prompt_tokens) as p, SUM(completion_tokens) as c, SUM(cached_tokens) as ca, COUNT(*) as cnt FROM token_usage WHERE created_at >= ?1 AND created_at <= ?2 GROUP BY bucket, model ORDER BY bucket ASC",
            date_format
        );

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params![from, to], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
            ))
        })?;

        let mut buckets: std::collections::BTreeMap<String, serde_json::Value> =
            std::collections::BTreeMap::new();
        for row in rows {
            let (bucket, model, p, c, ca, cnt) = row?;
            let entry = buckets.entry(bucket.clone()).or_insert_with(|| {
                serde_json::json!({
                    "date": bucket,
                    "prompt": 0,
                    "completion": 0,
                    "cached": 0,
                    "requests": 0,
                    "by_model": {},
                })
            });
            entry["prompt"] = serde_json::json!(entry["prompt"].as_i64().unwrap_or(0) + p);
            entry["completion"] = serde_json::json!(entry["completion"].as_i64().unwrap_or(0) + c);
            entry["cached"] = serde_json::json!(entry["cached"].as_i64().unwrap_or(0) + ca);
            entry["requests"] = serde_json::json!(entry["requests"].as_i64().unwrap_or(0) + cnt);
            entry["by_model"][&model] = serde_json::json!({
                "prompt": p,
                "completion": c,
                "requests": cnt,
            });
        }

        Ok(serde_json::json!({
            "points": buckets.into_values().collect::<Vec<_>>(),
        }))
    }
}
