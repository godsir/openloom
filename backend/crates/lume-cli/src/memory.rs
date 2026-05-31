//! Memory store implementation for the CLI — wraps loom-memory's three databases.
//! Persists chat messages to message_history table, extracts cognitions from
//! conversation text, and loads persona from accumulated trait data.

use anyhow::Result;
use loom_core::MemoryStore;
use loom_memory::{
    AgentConfigStore, CognitionStore, CognitionsPersonaProvider, GraphStore, McpConfigStore,
    McpServerRow, ModelConfigStore, NewEvent,
    config_db::ConfigDb,
    memory_db::MemoryDb,
    session_db::SessionDb,
};
use loom_types::{AgentConfig, Message, ModelConfig, PersonaProvider};

pub struct LoomMemoryStore {
    pub config_db: std::sync::Mutex<ConfigDb>,
    pub memory_db: std::sync::Mutex<MemoryDb>,
    pub session_db: std::sync::Mutex<SessionDb>,
}

impl LoomMemoryStore {
    pub fn open(data_dir: &std::path::Path) -> Result<Self> {
        std::fs::create_dir_all(data_dir)?;
        let config_db = ConfigDb::open(&data_dir.join("loom.db"))?;
        let memory_db = MemoryDb::open(&data_dir.join("memory.db"))?;
        let session_db = SessionDb::open(&data_dir.join("session.db"))?;
        // Ensure a default session row exists
        session_db.conn().execute(
            "INSERT OR IGNORE INTO sessions (id, created_at, message_count) VALUES ('default', datetime('now'), 0)",
            [],
        )?;
        // Migrate from legacy memory.db if present
        Self::migrate_legacy(data_dir, &config_db, &memory_db, &session_db);
        Ok(Self {
            config_db: std::sync::Mutex::new(config_db),
            memory_db: std::sync::Mutex::new(memory_db),
            session_db: std::sync::Mutex::new(session_db),
        })
    }

    fn migrate_legacy(data_dir: &std::path::Path, config_db: &ConfigDb, memory_db: &MemoryDb, session_db: &SessionDb) {
        let backup_path = data_dir.join("memory.db备份");
        if !backup_path.exists() {
            return;
        }
        let backup = backup_path.to_string_lossy();
        tracing::info!("one-time migration from memory.db备份");

        // Copy config tables from backup to loom.db
        let _ = config_db.conn().execute(
            &format!("ATTACH DATABASE '{}' AS backup", backup), [],
        );
        config_db.conn().execute_batch(
            "INSERT OR IGNORE INTO model_configs SELECT * FROM backup.model_configs;
             INSERT OR IGNORE INTO agent_configs SELECT * FROM backup.agent_configs;
             INSERT OR IGNORE INTO mcp_servers SELECT * FROM backup.mcp_servers;"
        ).ok();
        config_db.conn().execute("DETACH backup", []).ok();

        // Copy memory tables from backup to memory.db (fresh db, so no duplicates)
        let _ = memory_db.conn().execute(
            &format!("ATTACH DATABASE '{}' AS backup", backup), [],
        );
        memory_db.conn().execute_batch(
            "INSERT OR IGNORE INTO events SELECT * FROM backup.events;
             INSERT OR IGNORE INTO cognitions SELECT * FROM backup.cognitions;
             INSERT OR IGNORE INTO cognition_snapshots SELECT * FROM backup.cognition_snapshots;
             INSERT OR IGNORE INTO kg_nodes SELECT * FROM backup.kg_nodes;
             INSERT OR IGNORE INTO kg_edges SELECT * FROM backup.kg_edges;
             INSERT OR IGNORE INTO kg_aliases SELECT * FROM backup.kg_aliases;
             INSERT OR IGNORE INTO kg_evidence SELECT * FROM backup.kg_evidence;"
        ).ok();
        // Rebuild FTS5 indexes
        memory_db.conn().execute_batch(
            "INSERT INTO events_fts (event_type, action, context)
             SELECT event_type, action, context FROM events;
             INSERT INTO kg_nodes_fts (name, description)
             SELECT name, description FROM kg_nodes;"
        ).ok();
        memory_db.conn().execute("DETACH backup", []).ok();

        // Copy session tables from backup to session.db
        let _ = session_db.conn().execute(
            &format!("ATTACH DATABASE '{}' AS backup", backup), [],
        );
        session_db.conn().execute_batch(
            "INSERT OR IGNORE INTO sessions SELECT * FROM backup.sessions;
             INSERT OR IGNORE INTO message_history SELECT * FROM backup.message_history;
             INSERT OR IGNORE INTO token_usage SELECT * FROM backup.token_usage;"
        ).ok();
        session_db.conn().execute("DETACH backup", []).ok();

        // One-time migration complete — rename backup so it's not reused
        let done_path = data_dir.join("memory.db备份.done");
        let _ = std::fs::rename(&backup_path, &done_path);
        tracing::info!("migration complete, backup renamed to .done");
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
        // Write messages to session db
        let now = chrono::Utc::now().to_rfc3339();
        let usage_meta = serde_json::json!({"prompt_tokens": prompt_tokens, "completion_tokens": completion_tokens}).to_string();
        {
            let sess = self.session_db.lock().unwrap();
            let conn = sess.conn();
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
        }
        // Write event to memory db
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
        let event_id = self.memory_db.lock().unwrap().insert_event(&event)?;
        tracing::debug!(session_id, event_id, "chat turn saved");
        Ok(event_id)
    }

    async fn delete_message(&self, session_id: &str, index: usize) -> Result<()> {
        let store = self.session_db.lock().unwrap();
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
        let store = self.session_db.lock().unwrap();
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
        let store = self.memory_db.lock().unwrap();
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
                    .insert("USER", &format!("uses_{}", tag), keyword, 0.5, 1, session_id)
                    .is_ok()
                {
                    triggered.push(format!("uses_{}", tag));
                }
                // Also upsert to knowledge graph
                let _ = graph.upsert_node(keyword, "Technology", keyword, 0.5, session_id);
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
                        session_id,
                    )
                    .is_ok()
                {
                    triggered.push(format!("interest_{}", kw.replace(' ', "_")));
                }
                let _ = graph.upsert_node(kw, "Concept", kw, 0.5, session_id);
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
                    .insert("USER", trait_name, &snippet, 0.4, 1, session_id)
                    .is_ok()
                {
                    triggered.push(trait_name.to_string());
                }
            }
        }

        // 4. Always record the conversation topic (first 100 chars)
        let topic: String = text.chars().take(100).collect();
        if cognition
            .insert("USER", "last_topic", &topic, 0.3, 1, session_id)
            .is_ok()
        {
            triggered.push("last_topic".to_string());
        }

        // 5. Link USER node in knowledge graph
        let _ = graph.upsert_node("USER", "Person", "The user of openLoom", 1.0, session_id);

        if !triggered.is_empty() {
            tracing::info!(?triggered, session_id, "cognitions extracted");
        }
        Ok(triggered)
    }

    async fn get_persona(&self) -> Result<String> {
        let rows = {
            let store = self.memory_db.lock().unwrap();
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
        scope: &str,
    ) -> Result<(usize, usize)> {
        let store = self.memory_db.lock().unwrap();
        let graph = loom_memory::GraphStore::new(store.conn());
        let mut node_ids = std::collections::HashMap::new();
        let mut node_count = 0;
        let mut edge_count = 0;

        // Insert all entities from current batch
        for e in entities {
            if let Ok(id) = graph.upsert_node(
                &e.name,
                &e.entity_type,
                &e.description,
                e.confidence,
                scope,
            ) {
                node_ids.insert(e.name.clone(), id);
                node_count += 1;
                tracing::info!(name = %e.name, scope, node_id = id, "KG node upserted with scope");
                for alias in &e.aliases {
                    let _ = graph.add_alias(id, alias);
                }
            }
        }

        // Also resolve referenced entities that may already exist in DB
        for r in relationships {
            if !node_ids.contains_key(&r.source_name) {
                if let Ok(Some(id)) = graph.resolve_node(&r.source_name) {
                    node_ids.insert(r.source_name.clone(), id);
                }
            }
            if !node_ids.contains_key(&r.target_name) {
                if let Ok(Some(id)) = graph.resolve_node(&r.target_name) {
                    node_ids.insert(r.target_name.clone(), id);
                }
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
                    graph.upsert_edge(s, t, &r.relation_type, &r.fact, r.confidence, scope)
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
        scope: &str,
    ) -> Result<()> {
        let store = self.memory_db.lock().unwrap();
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
                scope,
            );
            tracing::info!(entity = %e.name, scope, "cognition inserted with scope");
        }
        Ok(())
    }

    async fn save_agent_config(&self, config: &AgentConfig) -> Result<()> {
        let store = self.config_db.lock().unwrap();
        AgentConfigStore::new(store.conn()).upsert(config)
    }

    async fn get_agent_config(&self, name: &str) -> Result<Option<AgentConfig>> {
        let store = self.config_db.lock().unwrap();
        AgentConfigStore::new(store.conn()).get(name)
    }

    async fn list_agent_configs(&self) -> Result<Vec<AgentConfig>> {
        let store = self.config_db.lock().unwrap();
        AgentConfigStore::new(store.conn()).list()
    }

    async fn delete_agent_config(&self, name: &str) -> Result<()> {
        let store = self.config_db.lock().unwrap();
        AgentConfigStore::new(store.conn()).delete(name)
    }

    async fn save_session_agent_name(
        &self,
        session_id: &str,
        agent_config_name: &str,
    ) -> Result<()> {
        let store = self.session_db.lock().unwrap();
        AgentConfigStore::new(store.conn()).set_session_binding(session_id, agent_config_name)
    }

    async fn get_session_agent_name(&self, session_id: &str) -> Result<Option<String>> {
        let store = self.session_db.lock().unwrap();
        AgentConfigStore::new(store.conn()).get_session_binding(session_id)
    }

    async fn save_session_workspace(&self, session_id: &str, path: &str) -> Result<()> {
        let store = self.session_db.lock().unwrap();
        AgentConfigStore::new(store.conn()).set_session_workspace(session_id, path)
    }

    async fn get_session_workspace(&self, session_id: &str) -> Result<Option<String>> {
        let store = self.session_db.lock().unwrap();
        AgentConfigStore::new(store.conn()).get_session_workspace(session_id)
    }

    async fn get_default_workspace(&self) -> Result<Option<String>> {
        Ok(loom_memory::store::get_default_workspace())
    }

    async fn set_default_workspace(&self, path: &str) -> Result<()> {
        loom_memory::store::set_default_workspace(path)
    }

    async fn save_model_config(&self, config: &ModelConfig) -> Result<()> {
        let store = self.config_db.lock().unwrap();
        ModelConfigStore::new(store.conn()).upsert(config)
    }

    async fn get_model_config(&self, name: &str) -> Result<Option<ModelConfig>> {
        let store = self.config_db.lock().unwrap();
        ModelConfigStore::new(store.conn()).get(name)
    }

    async fn list_model_configs(&self) -> Result<Vec<ModelConfig>> {
        let store = self.config_db.lock().unwrap();
        ModelConfigStore::new(store.conn()).list()
    }

    async fn delete_model_config(&self, name: &str) -> Result<()> {
        let store = self.config_db.lock().unwrap();
        ModelConfigStore::new(store.conn()).delete(name)
    }

    async fn set_active_model(&self, name: &str) -> Result<()> {
        let store = self.config_db.lock().unwrap();
        ModelConfigStore::new(store.conn()).set_active(name)
    }

    async fn get_active_model(&self) -> Result<Option<ModelConfig>> {
        let store = self.config_db.lock().unwrap();
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
        let store = self.config_db.lock().unwrap();
        McpConfigStore::new(store.conn()).upsert(&row)
    }

    async fn list_mcp_servers(&self) -> Result<Vec<(lume_mcp::McpServerConfig, bool)>> {
        let store = self.config_db.lock().unwrap();
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
        let store = self.config_db.lock().unwrap();
        McpConfigStore::new(store.conn()).delete(name)
    }

    async fn query_kg_context(&self, entity_names: &[&str], limit: usize, scope: &str) -> Result<String> {
        let store = self.memory_db.lock().unwrap();
        let graph = GraphStore::new(store.conn());
        let mut lines: Vec<String> = Vec::new();
        const MIN_CONFIDENCE: f64 = 0.5;
        let scope_opt = Some(scope);

        // Always include the USER node
        if let Ok(Some(_user_id)) = graph.resolve_node("USER") {
            let neighbors = graph.neighbors("USER", scope_opt, limit)?;
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
            if let Ok(results) = graph.search_entities(name, 3, scope_opt) {
                for r in &results {
                    if r.name == "USER" || r.confidence < MIN_CONFIDENCE {
                        continue;
                    }
                    lines.push(format!(
                        "- {} is a {}: {} (confidence: {:.2})",
                        r.name, r.entity_type, r.description, r.confidence
                    ));
                    // Get immediate neighbors
                    if let Ok(neighbors) = graph.neighbors(&r.name, scope_opt, 3) {
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
        let store = self.session_db.lock().unwrap();
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
        let store = self.session_db.lock().unwrap();
        store.conn().execute(
            "INSERT OR IGNORE INTO sessions (id, created_at, message_count) VALUES (?1, datetime('now'), 0)",
            rusqlite::params![id],
        )?;
        Ok(())
    }

    async fn prune_memory(&self) -> Result<usize> {
        let store = self.memory_db.lock().unwrap();
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
        let store = self.memory_db.lock().unwrap();
        let graph = GraphStore::new(store.conn());
        let results = graph.search_entities(query, limit, None)?;
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
        let store = self.memory_db.lock().unwrap();
        GraphStore::new(store.conn()).node_count()
    }

    async fn kg_edge_count(&self) -> Result<usize> {
        let store = self.memory_db.lock().unwrap();
        GraphStore::new(store.conn()).edge_count()
    }

    async fn kg_neighbors(&self, node_name: &str, limit: usize) -> Result<loom_types::KgGraph> {
        let store = self.memory_db.lock().unwrap();
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
        scope: Option<&str>,
        limit: usize,
    ) -> Result<loom_types::KgGraph> {
        let store = self.memory_db.lock().unwrap();
        let graph = GraphStore::new(store.conn());
        let rows = graph.walk(start_name, max_depth, scope, limit)?;

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
        let store = self.session_db.lock().unwrap();
        let conn = store.conn();

        // 1. Promote high-confidence session-scoped KG nodes/edges and cognitions to global
        let graph = GraphStore::new(conn);
        let cognition = CognitionStore::new(conn);
        let promoted_nodes = graph.promote_scope_to_global(id, 0.6).unwrap_or(0);
        let promoted_cogs = cognition.promote_to_global(id, 0.6).unwrap_or(0);
        if promoted_nodes > 0 || promoted_cogs > 0 {
            tracing::info!(
                session_id = id,
                promoted_nodes,
                promoted_cogs,
                "promoted session memories to global on delete"
            );
        }

        // 2. Delete remaining session-scoped data (low confidence items)
        let _ = graph.delete_by_scope(id);
        let _ = cognition.delete_by_scope(id);

        // 3. Delete message history and session row
        conn.execute(
            "DELETE FROM message_history WHERE session_id = ?1",
            rusqlite::params![id],
        )?;
        conn.execute("DELETE FROM sessions WHERE id = ?1", rusqlite::params![id])?;
        Ok(())
    }

    async fn rename_session(&self, id: &str, title: &str) -> Result<()> {
        let store = self.session_db.lock().unwrap();
        store.conn().execute(
            "UPDATE sessions SET title = ?1 WHERE id = ?2",
            rusqlite::params![title, id],
        )?;
        Ok(())
    }

    async fn get_summary(&self, session_id: &str) -> Result<Option<String>> {
        let store = self.session_db.lock().unwrap();
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
        let store = self.session_db.lock().unwrap();
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
        let store = self.session_db.lock().unwrap();
        let result = store.conn().query_row(
            "SELECT summary_at_count FROM sessions WHERE id = ?1",
            rusqlite::params![session_id],
            |row| row.get::<_, i64>(0),
        );
        Ok(result.unwrap_or(0) as usize)
    }

    async fn get_message_count(&self, session_id: &str) -> Result<usize> {
        let store = self.session_db.lock().unwrap();
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
        let store = self.memory_db.lock().unwrap();
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

    async fn kg_edges_between(&self, node_names: &[String]) -> Result<Vec<loom_types::KgEdge>> {
        let store = self.memory_db.lock().unwrap();
        let graph = GraphStore::new(store.conn());

        // Resolve node names to IDs
        let mut node_ids = Vec::new();
        let mut name_to_id = std::collections::HashMap::new();
        for name in node_names {
            if let Ok(Some(id)) = graph.resolve_node(name) {
                node_ids.push(id);
                name_to_id.insert(name.clone(), id);
            }
        }

        if node_ids.is_empty() {
            return Ok(Vec::new());
        }

        // Get all edges between these nodes
        let edges = graph.edges_between(&node_ids)?;

        // Convert to KgEdge format
        Ok(edges
            .into_iter()
            .map(|(source, target, relation_type, confidence)| loom_types::KgEdge {
                source,
                target,
                relation_type,
                fact: String::new(),
                confidence,
            })
            .collect())
    }

    async fn kg_delete_node(&self, name: &str) -> Result<bool> {
        let store = self.memory_db.lock().unwrap();
        GraphStore::new(store.conn()).delete_node(name)
    }

    async fn kg_delete_edge(&self, source: &str, target: &str, relation: &str) -> Result<bool> {
        let store = self.memory_db.lock().unwrap();
        GraphStore::new(store.conn()).delete_edge(source, target, relation)
    }

    async fn cognition_list(
        &self,
        subject: &str,
        scope: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<loom_types::Cognition>> {
        let store = self.memory_db.lock().unwrap();
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
        let store = self.memory_db.lock().unwrap();
        let cognitions = loom_memory::CognitionStore::new(store.conn());
        cognitions.list_subjects()
    }

    async fn cognition_snapshots(
        &self,
        cognition_id: i64,
    ) -> Result<Vec<loom_types::CognitionHistory>> {
        let store = self.memory_db.lock().unwrap();
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
        let store = self.memory_db.lock().unwrap();
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
        let store = self.session_db.lock().unwrap();
        store.conn().execute(
            "INSERT INTO token_usage (session_id, model, prompt_tokens, completion_tokens, cached_tokens, cached_read_tokens, cached_write_tokens, latency_ms, context_window) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![session_id, model, prompt_tokens as i64, completion_tokens as i64, (cached_read_tokens + cached_write_tokens) as i64, cached_read_tokens as i64, cached_write_tokens as i64, latency_ms as i64, context_window as i64],
        )?;
        Ok(())
    }

    async fn get_token_summary(&self, from: &str, to: &str) -> Result<serde_json::Value> {
        let store = self.session_db.lock().unwrap();
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
            "SELECT model, SUM(prompt_tokens) as p, SUM(completion_tokens) as c, SUM(cached_tokens) as ca, SUM(cached_read_tokens) as cr, SUM(cached_write_tokens) as cw, COUNT(*) as r, COALESCE(AVG(latency_ms), 0) as l, COALESCE(AVG(CAST(prompt_tokens AS REAL) / NULLIF(context_window, 0)), 0) as cu FROM token_usage WHERE created_at >= ?1 AND created_at <= ?2 GROUP BY model ORDER BY r DESC",
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
        let store = self.session_db.lock().unwrap();
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
