//! Memory store implementation for the CLI — wraps loom-memory's three databases.
//! Persists chat messages to message_history table, extracts cognitions from
//! conversation text, and loads persona from accumulated trait data.

use anyhow::Result;
use loom_core::MemoryStore;
use loom_memory::{
    AgentConfigStore, CognitionStore, GraphStore, McpConfigStore, McpServerRow, ModelConfigStore,
    NewEvent, RichPersonaProvider, TeamConfigStore, config_db::ConfigDb, memory_db::MemoryDb,
    session_db::SessionDb,
};
use loom_types::{
    AgentConfig, ImportOutcome, ImportPayload, Message, ModelConfig, PersonaProvider, TeamConfig,
};

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
            "INSERT OR IGNORE INTO sessions (id, created_at, updated_at, message_count) VALUES ('default', datetime('now'), datetime('now'), 0)",
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

    fn migrate_legacy(
        data_dir: &std::path::Path,
        config_db: &ConfigDb,
        memory_db: &MemoryDb,
        session_db: &SessionDb,
    ) {
        let backup_path = data_dir.join("memory.db备份");
        if !backup_path.exists() {
            return;
        }
        let backup = backup_path.to_string_lossy();
        tracing::info!("one-time migration from memory.db备份");

        // Copy config tables from backup to loom.db
        let _ = config_db
            .conn()
            .execute(&format!("ATTACH DATABASE '{}' AS backup", backup), []);
        config_db
            .conn()
            .execute_batch(
                "INSERT OR IGNORE INTO model_configs SELECT * FROM backup.model_configs;
             INSERT OR IGNORE INTO agent_configs SELECT * FROM backup.agent_configs;
             INSERT OR IGNORE INTO mcp_servers SELECT * FROM backup.mcp_servers;",
            )
            .ok();
        config_db.conn().execute("DETACH backup", []).ok();

        // Copy memory tables from backup to memory.db (fresh db, so no duplicates)
        let _ = memory_db
            .conn()
            .execute(&format!("ATTACH DATABASE '{}' AS backup", backup), []);
        memory_db
            .conn()
            .execute_batch(
                "INSERT OR IGNORE INTO events SELECT * FROM backup.events;
             INSERT OR IGNORE INTO cognitions SELECT * FROM backup.cognitions;
             INSERT OR IGNORE INTO cognition_snapshots SELECT * FROM backup.cognition_snapshots;
             INSERT OR IGNORE INTO kg_nodes SELECT * FROM backup.kg_nodes;
             INSERT OR IGNORE INTO kg_edges SELECT * FROM backup.kg_edges;
             INSERT OR IGNORE INTO kg_aliases SELECT * FROM backup.kg_aliases;
             INSERT OR IGNORE INTO kg_evidence SELECT * FROM backup.kg_evidence;",
            )
            .ok();
        // Rebuild FTS5 indexes
        memory_db
            .conn()
            .execute_batch(
                "INSERT INTO events_fts (event_type, action, context)
             SELECT event_type, action, context FROM events;
             INSERT INTO kg_nodes_fts (name, description)
             SELECT name, description FROM kg_nodes;",
            )
            .ok();
        memory_db.conn().execute("DETACH backup", []).ok();

        // Copy session tables from backup to session.db
        let _ = session_db
            .conn()
            .execute(&format!("ATTACH DATABASE '{}' AS backup", backup), []);
        session_db
            .conn()
            .execute_batch(
                "INSERT OR IGNORE INTO sessions SELECT * FROM backup.sessions;
             INSERT OR IGNORE INTO message_history SELECT * FROM backup.message_history;
             INSERT OR IGNORE INTO token_usage SELECT * FROM backup.token_usage;",
            )
            .ok();
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

/// Extract a CJK-friendly snippet from `text` starting at `pos`, respecting
/// sentence boundaries (。！？) within `cap` chars so the snippet reads naturally.
fn cjk_snippet(text: &str, pos: usize, cap: usize) -> String {
    let tail: String = text[pos..].chars().take(cap).collect();
    // Find the last sentence boundary within the snippet.
    // Use char-indexed slicing to avoid UTF-8 byte-boundary panics with
    // multi-byte CJK punctuation (。=3B, ！=3B, ？=3B).
    if let Some((char_idx, _boundary_char)) = tail.rmatch_indices(['。', '！', '？', '\n']).next()
    {
        let byte_len: usize = tail
            .char_indices()
            .take(char_idx + 1)
            .last()
            .map(|(bi, c)| bi + c.len_utf8())
            .unwrap_or(tail.len());
        tail[..byte_len].to_string()
    } else {
        tail
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
        cached_read_tokens: usize,
        cached_write_tokens: usize,
        context_window: usize,
        model: &str,
        tool_msgs_json: &[String],
        skip_user: bool,
    ) -> Result<i64> {
        // Write messages to session db
        let now = chrono::Utc::now().to_rfc3339();
        let usage_meta = serde_json::json!({
            "model": model,
            "prompt_tokens": prompt_tokens,
            "completion_tokens": completion_tokens,
            "cached_tokens": cached_read_tokens + cached_write_tokens,
            "cache_read_tokens": cached_read_tokens,
            "cache_write_tokens": cached_write_tokens,
            "context_window": context_window,
        })
        .to_string();
        {
            let sess = self.session_db.lock().expect("lock poisoned");
            let conn = sess.conn();
            let seq: i64 = conn.query_row(
                "SELECT COALESCE(MAX(seq), 0) + 1 FROM message_history WHERE session_id = ?1",
                rusqlite::params![session_id],
                |r| r.get(0),
            )?;
            // When skip_user is true, the user message was already persisted
            // at the start of the turn (via save_interrupted_turn). Only
            // insert assistant + tool messages here, using seq for assistant
            // and seq+1+i for tools.
            let (assistant_seq, tool_base_seq, count_increment) = if skip_user {
                (seq, seq + 1, 1_i64)
            } else {
                conn.execute(
                    "INSERT INTO message_history (session_id, seq, role, content, timestamp) VALUES (?1, ?2, 'user', ?3, ?4)",
                    rusqlite::params![session_id, seq, user_msg, now],
                )?;
                (seq + 1, seq + 2, 2_i64)
            };
            conn.execute(
                "INSERT INTO message_history (session_id, seq, role, content, timestamp, metadata) VALUES (?1, ?2, 'assistant', ?3, ?4, ?5)",
                rusqlite::params![session_id, assistant_seq, assistant_msg, now, usage_meta],
            )?;
            // Persist tool messages
            let mut tool_count: i64 = 0;
            for (i, tool_json) in tool_msgs_json.iter().enumerate() {
                conn.execute(
                    "INSERT INTO message_history (session_id, seq, role, content, timestamp) VALUES (?1, ?2, 'tool', ?3, ?4)",
                    rusqlite::params![session_id, tool_base_seq + i as i64, tool_json, now],
                )?;
                tool_count += 1;
            }
            conn.execute(
                "UPDATE sessions SET message_count = message_count + ?1, updated_at = datetime('now') WHERE id = ?2",
                rusqlite::params![count_increment + tool_count, session_id],
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
        let event_id = self
            .memory_db
            .lock()
            .expect("lock poisoned")
            .insert_event(&event)?;
        tracing::debug!(session_id, event_id, "chat turn saved");
        Ok(event_id)
    }

    async fn save_interrupted_turn(&self, session_id: &str, user_msg: &str) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        {
            let sess = self.session_db.lock().expect("lock poisoned");
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
                "UPDATE sessions SET message_count = message_count + 1, updated_at = datetime('now') WHERE id = ?1",
                rusqlite::params![session_id],
            )?;
        }
        Ok(())
    }

    async fn append_message(
        &self,
        session_id: &str,
        role: &str,
        content_json: &str,
        metadata_json: Option<&str>,
    ) -> Result<i64> {
        let now = chrono::Utc::now().to_rfc3339();
        let sess = self.session_db.lock().expect("lock poisoned");
        let conn = sess.conn();
        let seq: i64 = conn.query_row(
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM message_history WHERE session_id = ?1",
            rusqlite::params![session_id],
            |r| r.get(0),
        )?;
        if let Some(meta) = metadata_json {
            conn.execute(
                "INSERT INTO message_history (session_id, seq, role, content, timestamp, metadata) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![session_id, seq, role, content_json, now, meta],
            )?;
        } else {
            conn.execute(
                "INSERT INTO message_history (session_id, seq, role, content, timestamp) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![session_id, seq, role, content_json, now],
            )?;
        }
        conn.execute(
            "UPDATE sessions SET message_count = message_count + 1, updated_at = datetime('now') WHERE id = ?1",
            rusqlite::params![session_id],
        )?;
        Ok(seq)
    }

    async fn update_message(
        &self,
        session_id: &str,
        seq: i64,
        content_json: &str,
        metadata_json: Option<&str>,
    ) -> Result<()> {
        let sess = self.session_db.lock().expect("lock poisoned");
        let conn = sess.conn();
        if let Some(meta) = metadata_json {
            conn.execute(
                "UPDATE message_history SET content = ?1, metadata = ?2 WHERE session_id = ?3 AND seq = ?4",
                rusqlite::params![content_json, meta, session_id, seq],
            )?;
        } else {
            conn.execute(
                "UPDATE message_history SET content = ?1 WHERE session_id = ?2 AND seq = ?3",
                rusqlite::params![content_json, session_id, seq],
            )?;
        }
        Ok(())
    }

    async fn delete_message(&self, session_id: &str, index: usize) -> Result<()> {
        let store = self.session_db.lock().expect("lock poisoned");
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
        let store = self.session_db.lock().expect("lock poisoned");
        // 取最近 limit 条，再按 seq 升序返回（正序展示）。子查询 DESC 取最近，外层 ASC 正序。
        let mut stmt = store.conn().prepare(
            "SELECT role, content, metadata, timestamp FROM (
                SELECT role, content, metadata, timestamp, seq FROM message_history
                WHERE session_id = ?1 ORDER BY seq DESC LIMIT ?2
            ) ORDER BY seq ASC",
        )?;
        let rows = stmt.query_map(rusqlite::params![session_id, limit as i64], |row| {
            let role: String = row.get(0)?;
            let content: String = row.get(1)?;
            let metadata: Option<String> = row.get(2)?;
            let ts_str: String = row.get(3)?;
            let usage = metadata.and_then(|m| {
                let v: serde_json::Value = serde_json::from_str(&m).ok()?;
                Some(loom_types::TokenUsage {
                    prompt_tokens: v["prompt_tokens"].as_u64()? as usize,
                    completion_tokens: v["completion_tokens"].as_u64()? as usize,
                    model: v["model"].as_str().unwrap_or("").to_string(),
                    cached_tokens: v["cached_tokens"].as_u64().unwrap_or(0) as usize,
                    cache_read_tokens: v["cache_read_tokens"].as_u64().unwrap_or(0) as usize,
                    cache_write_tokens: v["cache_write_tokens"].as_u64().unwrap_or(0) as usize,
                    context_window: v["context_window"].as_u64().unwrap_or(0) as usize,
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
                "tool" => loom_types::Role::Tool,
                _ => loom_types::Role::User,
            };
            Ok(Message {
                role: role_enum,
                content: parts,
                timestamp: chrono::DateTime::parse_from_rfc3339(&ts_str)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now()),
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
        let store = self.memory_db.lock().expect("lock poisoned");
        let cognition = CognitionStore::new(store.conn());
        let graph = GraphStore::new(store.conn());
        let mut triggered = Vec::new();
        let lower = text.to_lowercase();

        // 1. Technology stack keywords
        let tech_keywords: &[(&str, &str)] = &[
            // ── Programming Languages ──────────────────────────────
            ("rust", "rust"),
            ("python", "python"),
            ("typescript", "typescript"),
            ("golang", "golang"),
            ("java", "java"),
            ("c++", "cpp"),
            ("c#", "csharp"),
            ("javascript", "javascript"),
            ("kotlin", "kotlin"),
            ("swift", "swift"),
            ("elixir", "elixir"),
            ("scala", "scala"),
            ("zig", "zig"),
            ("lua", "lua"),
            ("ruby", "ruby"),
            ("php", "php"),
            ("perl", "perl"),
            ("haskell", "haskell"),
            ("nix", "nix"),
            // ── Frameworks & Libraries ─────────────────────────────
            ("react", "react"),
            ("vue", "vue"),
            ("angular", "angular"),
            ("svelte", "svelte"),
            ("next.js", "nextjs"),
            ("nuxt", "nuxt"),
            ("electron", "electron"),
            ("tauri", "tauri"),
            ("fastapi", "fastapi"),
            ("django", "django"),
            ("flask", "flask"),
            ("spring", "spring"),
            ("axum", "axum"),
            ("tokio", "tokio"),
            // ── Databases ──────────────────────────────────────────
            ("postgres", "postgresql"),
            ("postgresql", "postgresql"),
            ("sql", "sql"),
            ("sqlite", "sqlite"),
            ("mysql", "mysql"),
            ("redis", "redis"),
            ("mongodb", "mongodb"),
            ("elasticsearch", "elasticsearch"),
            ("neo4j", "neo4j"),
            ("clickhouse", "clickhouse"),
            ("duckdb", "duckdb"),
            ("milvus", "milvus"),
            ("qdrant", "qdrant"),
            // ── DevOps & Infrastructure ────────────────────────────
            ("docker", "docker"),
            ("kubernetes", "k8s"),
            ("k8s", "k8s"),
            ("terraform", "terraform"),
            ("nginx", "nginx"),
            ("grafana", "grafana"),
            ("prometheus", "prometheus"),
            ("kafka", "kafka"),
            ("rabbitmq", "rabbitmq"),
            ("node", "nodejs"),
            ("git", "git"),
            ("github", "github"),
            ("linux", "linux"),
            ("windows", "windows"),
            ("macos", "macos"),
            // ── Concepts ───────────────────────────────────────────
            ("graphql", "graphql"),
            ("grpc", "grpc"),
            ("webassembly", "wasm"),
            ("wasm", "wasm"),
            ("microservices", "microservices"),
        ];
        for (keyword, tag) in tech_keywords {
            if lower.contains(keyword) && !lower.contains(&format!("not {}", keyword)) {
                if cognition
                    .insert(
                        "USER",
                        &format!("uses_{}", tag),
                        keyword,
                        0.5,
                        1,
                        session_id,
                    )
                    .is_ok()
                {
                    triggered.push(format!("uses_{}", tag));
                }
                // Also upsert to knowledge graph
                let _ = graph.upsert_node(keyword, "Technology", keyword, 0.5, session_id, None);
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
            "rag",
            "embedding",
            "transformer",
            "diffusion",
            "fine-tuning",
            "ft",
            "prompt engineering",
            "claude",
            "openai",
            "deepseek",
            "qwen",
            "glm",
            "lm studio",
            "ollama",
            "langchain",
            "copilot",
            "chatgpt",
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
                let _ = graph.upsert_node(kw, "Concept", kw, 0.5, session_id, None);
            }
        }

        // 3. Chinese patterns: preferences, goals, habits
        let cn_patterns: &[(&str, &str)] = &[
            ("我喜欢", "preference"),
            ("我讨厌", "dislike"),
            ("我不喜欢", "dislike"),
            ("我不用", "avoid"),
            ("我想", "goal"),
            ("我需要", "need"),
            ("我觉得", "opinion"),
            ("我认为", "opinion"),
            ("我打算", "plan"),
            ("我希望", "wish"),
            ("我计划", "plan"),
            ("我习惯", "habit"),
            ("我在做", "working_on"),
            ("我在用", "using"),
            ("我之前用", "used_before"),
            ("我的项目", "project"),
            ("我公司", "company"),
            ("我团队", "team"),
            ("我关注", "following"),
            ("我建议", "suggestion"),
            ("我在学", "learning"),
            ("我想学", "learning"),
        ];
        for (prefix, trait_name) in cn_patterns {
            if let Some(pos) = text.find(prefix) {
                let snippet = cjk_snippet(text, pos, 50);
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
        let _ = graph.upsert_node(
            "USER",
            "Person",
            "The user of openLoom",
            1.0,
            session_id,
            None,
        );

        if !triggered.is_empty() {
            tracing::info!(?triggered, session_id, "cognitions extracted");
        }
        Ok(triggered)
    }

    async fn get_persona(&self) -> Result<String> {
        let provider = {
            let store = self.memory_db.lock().expect("lock poisoned");
            RichPersonaProvider::assemble(store.conn())?
        };
        provider.summarize().await
    }

    /// Phase 2: Rich persona formatted as a structured Markdown block for
    /// system prompt injection. Wraps the base persona with section headers
    /// and confidence metadata for better LLM comprehension.
    async fn get_rich_persona(&self) -> Result<Option<String>> {
        let provider = {
            let store = self.memory_db.lock().expect("lock poisoned");
            RichPersonaProvider::assemble(store.conn())?
        };
        let persona = provider.persona();

        // Return None if no meaningful data gathered yet
        if persona.tech_stack.is_empty()
            && persona.preferences.is_empty()
            && persona.goals.is_empty()
        {
            return Ok(None);
        }

        let base = RichPersonaProvider::format_for_prompt(persona);

        // Wrap with a structured Markdown header
        let mut lines: Vec<String> = Vec::new();
        lines.push("## User Profile (Rich)".to_string());
        lines.push(base);

        // Append a compact tech stack table if non-empty
        if !persona.tech_stack.is_empty() {
            lines.push("\n### Technology Proficiency".to_string());
            for t in persona.tech_stack.iter().take(10) {
                lines.push(format!(
                    "- **{}**: {} (confidence: {:.2}, evidence: {})",
                    t.name,
                    t.level.as_str(),
                    t.confidence,
                    t.evidence_count
                ));
            }
        }

        // Append active goals
        let active_goals: Vec<_> = persona
            .goals
            .iter()
            .filter(|g| matches!(g.status, loom_memory::persona::GoalStatus::Active))
            .collect();
        if !active_goals.is_empty() {
            lines.push("\n### Active Goals".to_string());
            for g in active_goals.iter().take(5) {
                lines.push(format!("- {} (priority: {})", g.description, g.priority));
            }
        }

        Ok(Some(lines.join("\n")))
    }

    /// Phase 2: Rich persona as structured JSON — for client-side rendering.
    async fn get_rich_persona_structured(&self) -> Result<Option<serde_json::Value>> {
        let provider = {
            let store = self.memory_db.lock().expect("lock poisoned");
            RichPersonaProvider::assemble(store.conn())?
        };
        let persona = provider.persona();

        // Return None if no meaningful data gathered yet
        if persona.tech_stack.is_empty()
            && persona.preferences.is_empty()
            && persona.goals.is_empty()
        {
            return Ok(None);
        }

        serde_json::to_value(persona)
            .map(Some)
            .map_err(|e| anyhow::anyhow!("Failed to serialize persona: {}", e))
    }

    async fn feed_knowledge_graph(
        &self,
        entities: &[loom_memory::ExtractedEntity],
        relationships: &[loom_memory::ExtractedRelationship],
        source_event_id: i64,
        scope: &str,
    ) -> Result<(usize, usize)> {
        let store = self.memory_db.lock().expect("lock poisoned");
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
                None,
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
            if !node_ids.contains_key(&r.source_name)
                && let Ok(Some(id)) = graph.resolve_node(&r.source_name)
            {
                node_ids.insert(r.source_name.clone(), id);
            }
            if !node_ids.contains_key(&r.target_name)
                && let Ok(Some(id)) = graph.resolve_node(&r.target_name)
            {
                node_ids.insert(r.target_name.clone(), id);
            }
        }

        // Wire evidence for nodes
        for node_id in node_ids.values() {
            let _ = graph.link_evidence_node(*node_id, source_event_id);
        }
        // Wire evidence for edges
        for r in relationships {
            let src = node_ids.get(&r.source_name).copied();
            let tgt = node_ids.get(&r.target_name).copied();
            if let (Some(s), Some(t)) = (src, tgt)
                && let Ok(edge_id) =
                    graph.upsert_edge(s, t, &r.relation_type, &r.fact, r.confidence, scope)
            {
                edge_count += 1;
                let _ = graph.link_evidence_edge(edge_id, source_event_id);
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
        let store = self.memory_db.lock().expect("lock poisoned");
        let cognition = CognitionStore::new(store.conn());
        for e in entities {
            let clean_name = e.name.trim_matches('\'').trim_matches('"');
            let clean_desc = e.description.trim_matches('\'').trim_matches('"');
            let value = if clean_desc.is_empty() || clean_desc == clean_name {
                clean_name.to_string()
            } else {
                format!("{} ({})", clean_name, clean_desc)
            };
            let trait_name = match e.entity_type.to_lowercase().as_str() {
                "technology" => format!("uses_{}", clean_name.to_lowercase()),
                "interest" => format!("interest_{}", clean_name.to_lowercase()),
                other => format!("entity_{}", other),
            };
            let _ = cognition.insert("USER", &trait_name, &value, e.confidence, 1, scope);
            tracing::info!(entity = %e.name, scope, "cognition inserted with scope");
        }
        Ok(())
    }

    async fn save_agent_config(&self, config: &AgentConfig) -> Result<()> {
        let store = self.config_db.lock().expect("lock poisoned");
        AgentConfigStore::new(store.conn()).upsert(config)
    }

    async fn get_agent_config(&self, name: &str) -> Result<Option<AgentConfig>> {
        let store = self.config_db.lock().expect("lock poisoned");
        AgentConfigStore::new(store.conn()).get(name)
    }

    async fn list_agent_configs(&self) -> Result<Vec<AgentConfig>> {
        let store = self.config_db.lock().expect("lock poisoned");
        AgentConfigStore::new(store.conn()).list()
    }

    async fn delete_agent_config(&self, name: &str) -> Result<()> {
        let store = self.config_db.lock().expect("lock poisoned");
        AgentConfigStore::new(store.conn()).delete(name)
    }

    // Team config CRUD
    async fn save_team_config(&self, config: &TeamConfig) -> Result<()> {
        let store = self.config_db.lock().expect("lock poisoned");
        TeamConfigStore::new(store.conn()).save_team_config(config)
    }

    async fn get_team_config(&self, id: &str) -> Result<Option<TeamConfig>> {
        let store = self.config_db.lock().expect("lock poisoned");
        TeamConfigStore::new(store.conn()).get_team_config(id)
    }

    async fn list_team_configs(&self) -> Result<Vec<TeamConfig>> {
        let store = self.config_db.lock().expect("lock poisoned");
        TeamConfigStore::new(store.conn()).list_team_configs()
    }

    async fn delete_team_config(&self, id: &str) -> Result<()> {
        let store = self.config_db.lock().expect("lock poisoned");
        TeamConfigStore::new(store.conn()).delete_team_config(id)
    }

    async fn save_session_agent_name(
        &self,
        session_id: &str,
        agent_config_name: &str,
    ) -> Result<()> {
        let store = self.session_db.lock().expect("lock poisoned");
        AgentConfigStore::new(store.conn()).set_session_binding(session_id, agent_config_name)
    }

    async fn get_session_agent_name(&self, session_id: &str) -> Result<Option<String>> {
        let store = self.session_db.lock().expect("lock poisoned");
        AgentConfigStore::new(store.conn()).get_session_binding(session_id)
    }

    async fn save_session_team_id(&self, session_id: &str, team_id: &str) -> Result<()> {
        let store = self.session_db.lock().expect("lock poisoned");
        AgentConfigStore::new(store.conn()).set_session_team_binding(session_id, team_id)
    }

    async fn get_session_team_id(&self, session_id: &str) -> Result<Option<String>> {
        let store = self.session_db.lock().expect("lock poisoned");
        AgentConfigStore::new(store.conn()).get_session_team_binding(session_id)
    }

    async fn save_session_workspace(&self, session_id: &str, path: &str) -> Result<()> {
        let store = self.session_db.lock().expect("lock poisoned");
        AgentConfigStore::new(store.conn()).set_session_workspace(session_id, path)
    }

    async fn get_session_workspace(&self, session_id: &str) -> Result<Option<String>> {
        let store = self.session_db.lock().expect("lock poisoned");
        AgentConfigStore::new(store.conn()).get_session_workspace(session_id)
    }

    async fn get_default_workspace(&self) -> Result<Option<String>> {
        Ok(loom_memory::store::get_default_workspace())
    }

    async fn set_default_workspace(&self, path: &str) -> Result<()> {
        loom_memory::store::set_default_workspace(path)
    }

    async fn set_session_memory_enabled(&self, session_id: &str, enabled: bool) -> Result<()> {
        let store = self.session_db.lock().expect("lock poisoned");
        store.conn().execute(
            "UPDATE sessions SET memory_enabled = ?1 WHERE id = ?2",
            rusqlite::params![enabled as i32, session_id],
        )?;
        Ok(())
    }

    async fn get_session_memory_enabled(&self, session_id: &str) -> Result<Option<bool>> {
        let store = self.session_db.lock().expect("lock poisoned");
        let result = store.conn().query_row(
            "SELECT memory_enabled FROM sessions WHERE id = ?1",
            rusqlite::params![session_id],
            |row| row.get::<_, i32>(0),
        );
        match result {
            Ok(1) => Ok(Some(true)),
            Ok(0) => Ok(Some(false)),
            Ok(_) => Ok(Some(true)), // default
            Err(_) => Ok(None),
        }
    }

    async fn save_model_config(&self, config: &ModelConfig) -> Result<()> {
        let store = self.config_db.lock().expect("lock poisoned");
        ModelConfigStore::new(store.conn()).upsert(config)
    }

    async fn get_model_config(&self, name: &str) -> Result<Option<ModelConfig>> {
        let store = self.config_db.lock().expect("lock poisoned");
        ModelConfigStore::new(store.conn()).get(name)
    }

    async fn list_model_configs(&self) -> Result<Vec<ModelConfig>> {
        let store = self.config_db.lock().expect("lock poisoned");
        ModelConfigStore::new(store.conn()).list()
    }

    async fn delete_model_config(&self, name: &str) -> Result<()> {
        let store = self.config_db.lock().expect("lock poisoned");
        ModelConfigStore::new(store.conn()).delete(name)
    }

    async fn set_active_model(&self, name: &str) -> Result<()> {
        let store = self.config_db.lock().expect("lock poisoned");
        ModelConfigStore::new(store.conn()).set_active(name)
    }

    async fn get_active_model(&self) -> Result<Option<ModelConfig>> {
        let store = self.config_db.lock().expect("lock poisoned");
        ModelConfigStore::new(store.conn()).get_active()
    }

    async fn save_mcp_server(
        &self,
        config: &loom_mcp::McpServerConfig,
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
        let store = self.config_db.lock().expect("lock poisoned");
        McpConfigStore::new(store.conn()).upsert(&row)
    }

    async fn list_mcp_servers(&self) -> Result<Vec<(loom_mcp::McpServerConfig, bool)>> {
        let store = self.config_db.lock().expect("lock poisoned");
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
                let config = loom_mcp::McpServerConfig {
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
        let store = self.config_db.lock().expect("lock poisoned");
        McpConfigStore::new(store.conn()).delete(name)
    }

    async fn query_kg_context(
        &self,
        entity_names: &[&str],
        limit: usize,
        scope: &str,
    ) -> Result<String> {
        let store = self.memory_db.lock().expect("lock poisoned");
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

    async fn query_kg_context_layered(
        &self,
        entity_names: &[&str],
        limit: usize,
        scope: &str,
        layer: Option<&str>,
    ) -> Result<String> {
        let store = self.memory_db.lock().expect("lock poisoned");
        let graph = GraphStore::new(store.conn());
        graph.query_kg_context(entity_names, limit, Some(scope), layer)
    }

    async fn list_sessions(
        &self,
    ) -> Result<Vec<(String, String, usize, Option<String>, Option<String>)>> {
        let store = self.session_db.lock().expect("lock poisoned");
        let mut stmt = store.conn().prepare(
            "SELECT id, created_at, message_count, title, updated_at FROM sessions ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)? as usize,
                row.get(3)?,
                row.get(4)?,
            ))
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    async fn ensure_session(&self, id: &str) -> Result<()> {
        let store = self.session_db.lock().expect("lock poisoned");
        store.conn().execute(
            "INSERT OR IGNORE INTO sessions (id, created_at, updated_at, message_count) VALUES (?1, datetime('now'), datetime('now'), 0)",
            rusqlite::params![id],
        )?;
        Ok(())
    }

    async fn prune_memory(&self) -> Result<usize> {
        let store = self.memory_db.lock().expect("lock poisoned");
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
        let store = self.memory_db.lock().expect("lock poisoned");
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
        let store = self.memory_db.lock().expect("lock poisoned");
        GraphStore::new(store.conn()).node_count()
    }

    async fn kg_edge_count(&self) -> Result<usize> {
        let store = self.memory_db.lock().expect("lock poisoned");
        GraphStore::new(store.conn()).edge_count()
    }

    async fn kg_neighbors(
        &self,
        node_name: &str,
        limit: usize,
        scope: Option<&str>,
    ) -> Result<loom_types::KgGraph> {
        let store = self.memory_db.lock().expect("lock poisoned");
        let graph = GraphStore::new(store.conn());
        let rows = graph.neighbors(node_name, scope, limit)?;
        let nodes: Vec<loom_types::KgNode> = rows
            .iter()
            .map(|r| loom_types::KgNode {
                node_id: r.node_id,
                name: r.name.clone(),
                entity_type: r.entity_type.clone(),
                description: r.description.clone(),
                confidence: r.confidence,
                scope: r.scope.clone(),
                layer: "semantic".to_string(),
                similarity: 0.0,
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
        let store = self.memory_db.lock().expect("lock poisoned");
        let graph = GraphStore::new(store.conn());
        let rows = graph.walk(start_name, max_depth, scope, limit)?;

        // Also include the start node itself
        let start_id: Option<i64> = graph.resolve_node(start_name)?;
        let mut all_ids: Vec<i64> = rows.iter().map(|r| r.node_id).collect();
        if let Some(sid) = start_id
            && !all_ids.contains(&sid)
        {
            all_ids.insert(0, sid);
        }

        let nodes: Vec<loom_types::KgNode> = if let Some(sid) = start_id {
            // Try to find the start node's scope from the walk result or resolve_node
            let start_scope = rows
                .first()
                .map(|r| r.scope.clone())
                .or_else(|| {
                    // If walk returned no rows, query the node directly for its scope
                    store
                        .conn()
                        .query_row(
                            "SELECT scope FROM kg_nodes WHERE id = ?1",
                            rusqlite::params![sid],
                            |row| row.get::<_, String>(0),
                        )
                        .ok()
                })
                .unwrap_or_else(|| "global".to_string());
            let start_node = loom_types::KgNode {
                node_id: sid,
                name: start_name.to_string(),
                entity_type: rows
                    .first()
                    .map(|r| r.entity_type.clone())
                    .unwrap_or_default(),
                description: String::new(),
                confidence: 1.0,
                scope: start_scope,
                layer: "semantic".to_string(),
                similarity: 0.0,
            };
            let mut n = vec![start_node];
            n.extend(rows.iter().map(|r| loom_types::KgNode {
                node_id: r.node_id,
                name: r.name.clone(),
                entity_type: r.entity_type.clone(),
                description: r.description.clone(),
                confidence: r.confidence,
                scope: r.scope.clone(),
                layer: "semantic".to_string(),
                similarity: 0.0,
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
                    layer: "semantic".to_string(),
                    similarity: 0.0,
                })
                .collect()
        };

        // Get edges between all found nodes, filtered by the same scope
        let edge_rows = graph.edges_between(&all_ids, scope)?;
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
        // 1. Promote high-confidence session-scoped KG nodes/edges and cognitions to global.
        //    These tables live in memory.db, NOT session.db.
        {
            let store = self.memory_db.lock().expect("lock poisoned");
            let conn = store.conn();
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
            // Delete remaining session-scoped data (low confidence items)
            let _ = graph.delete_by_scope(id);
            let _ = cognition.delete_by_scope(id);
        }

        // 2. Delete message history and session row from session.db.
        {
            let store = self.session_db.lock().expect("lock poisoned");
            let conn = store.conn();
            conn.execute(
                "DELETE FROM message_history WHERE session_id = ?1",
                rusqlite::params![id],
            )?;
            conn.execute("DELETE FROM sessions WHERE id = ?1", rusqlite::params![id])?;
        }
        Ok(())
    }

    /// Promote high-confidence session-scoped memories to global scope
    /// WITHOUT deleting remaining session-scoped data.
    /// Returns (promoted_nodes, promoted_cognitions).
    async fn promote_to_global(
        &self,
        session_id: &str,
        min_confidence: f64,
    ) -> Result<(usize, usize)> {
        let store = self.memory_db.lock().expect("lock poisoned");
        let conn = store.conn();

        // Promote KG nodes: change scope from session_id to 'global'
        // for high-confidence nodes that don't already have a global duplicate.
        let promoted_nodes = conn.execute(
            "UPDATE kg_nodes SET scope = 'global'
             WHERE scope = ?1 AND confidence >= ?2
             AND name NOT IN (SELECT name FROM kg_nodes WHERE scope = 'global')",
            rusqlite::params![session_id, min_confidence],
        )?;

        // Promote KG edges whose both endpoints are now global.
        let _ = conn.execute(
            "UPDATE kg_edges SET scope = 'global'
             WHERE scope = ?1
               AND source_id IN (SELECT id FROM kg_nodes WHERE scope = 'global')
               AND target_id IN (SELECT id FROM kg_nodes WHERE scope = 'global')",
            rusqlite::params![session_id],
        )?;

        // Promote cognitions: change scope from session_id to 'global'
        // for high-confidence cognitions that don't already have a global duplicate.
        let promoted_cogs = conn.execute(
            "UPDATE cognitions SET scope = 'global'
             WHERE scope = ?1 AND confidence >= ?2
             AND (subject || '|' || trait) NOT IN
                 (SELECT subject || '|' || trait FROM cognitions WHERE scope = 'global')",
            rusqlite::params![session_id, min_confidence],
        )?;

        if promoted_nodes > 0 || promoted_cogs > 0 {
            tracing::info!(
                session_id,
                promoted_nodes,
                promoted_cogs,
                min_confidence,
                "promoted session memories to global"
            );
        }

        Ok((promoted_nodes, promoted_cogs))
    }

    async fn promote_selected(
        &self,
        node_names: &[String],
        cognition_ids: &[i64],
    ) -> Result<(usize, usize)> {
        let store = self.memory_db.lock().expect("lock poisoned");
        let conn = store.conn();
        let graph = GraphStore::new(conn);
        let cognition = CognitionStore::new(conn);
        let promoted_nodes = graph.promote_nodes_by_name(node_names)?;
        let promoted_cogs = cognition.promote_cognitions_by_id(cognition_ids)?;
        if promoted_nodes > 0 || promoted_cogs > 0 {
            tracing::info!(
                promoted_nodes,
                promoted_cogs,
                "promoted selected memories to global"
            );
        }
        Ok((promoted_nodes, promoted_cogs))
    }

    async fn rename_session(&self, id: &str, title: &str) -> Result<()> {
        let store = self.session_db.lock().expect("lock poisoned");
        store.conn().execute(
            "UPDATE sessions SET title = ?1, updated_at = datetime('now') WHERE id = ?2",
            rusqlite::params![title, id],
        )?;
        Ok(())
    }

    async fn import_session(
        &self,
        payload: &ImportPayload,
        replace: bool,
    ) -> Result<ImportOutcome> {
        let sess = self.session_db.lock().expect("lock poisoned");
        let conn = sess.conn();

        let existed: bool = conn
            .query_row(
                "SELECT 1 FROM sessions WHERE id = ?1",
                rusqlite::params![payload.id],
                |_| Ok(true),
            )
            .unwrap_or(false);

        if existed && !replace {
            return Ok(ImportOutcome::AlreadyExists);
        }
        if existed && replace {
            conn.execute(
                "DELETE FROM message_history WHERE session_id = ?1",
                rusqlite::params![payload.id],
            )?;
        }

        let created = payload.created_at.format("%Y-%m-%d %H:%M:%S").to_string();
        let updated = payload.updated_at.format("%Y-%m-%d %H:%M:%S").to_string();
        conn.execute(
            "INSERT OR IGNORE INTO sessions (id, created_at, updated_at, message_count, title, workspace_path)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                payload.id,
                created,
                updated,
                payload.messages.len() as i64,
                payload.title,
                payload.workspace_path,
            ],
        )?;
        // If the row pre-existed without replace (shouldn't reach here) or INSERT OR IGNORE no-op'd,
        // still sync the metadata columns so re-imports refresh title/timestamps/counts.
        conn.execute(
            "UPDATE sessions SET created_at = ?2, updated_at = ?3, message_count = ?4, title = ?5, workspace_path = ?6
             WHERE id = ?1",
            rusqlite::params![
                payload.id,
                created,
                updated,
                payload.messages.len() as i64,
                payload.title,
                payload.workspace_path,
            ],
        )?;

        for (i, msg) in payload.messages.iter().enumerate() {
            let seq = (i as i64) + 1;
            let role = msg.role.as_str();
            let content = serde_json::to_string(&msg.content)?;
            let ts = msg.timestamp.to_rfc3339();
            let usage_meta = msg.usage.as_ref().map(|u| {
                serde_json::json!({
                    "model": u.model,
                    "prompt_tokens": u.prompt_tokens,
                    "completion_tokens": u.completion_tokens,
                    "cached_tokens": u.cache_read_tokens + u.cache_write_tokens,
                    "cache_read_tokens": u.cache_read_tokens,
                    "cache_write_tokens": u.cache_write_tokens,
                    "context_window": u.context_window,
                })
                .to_string()
            });
            if let Some(meta) = usage_meta {
                conn.execute(
                    "INSERT INTO message_history (session_id, seq, role, content, timestamp, metadata) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    rusqlite::params![payload.id, seq, role, content, ts, meta],
                )?;
            } else {
                conn.execute(
                    "INSERT INTO message_history (session_id, seq, role, content, timestamp) VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![payload.id, seq, role, content, ts],
                )?;
            }
        }
        Ok(if existed {
            ImportOutcome::Replaced
        } else {
            ImportOutcome::Created
        })
    }

    async fn get_summary(&self, session_id: &str) -> Result<Option<String>> {
        let store = self.session_db.lock().expect("lock poisoned");
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

    async fn save_summary(
        &self,
        session_id: &str,
        summary: &str,
        at_count: usize,
        model_name: &str,
    ) -> Result<()> {
        let store = self.session_db.lock().expect("lock poisoned");
        store.conn().execute(
            "UPDATE sessions SET summary = ?1, summary_at_count = ?2, summary_model = ?3, summary_updated_at = datetime('now') WHERE id = ?4",
            rusqlite::params![summary, at_count as i64, model_name, session_id],
        )?;
        Ok(())
    }

    async fn get_summary_at_count(&self, session_id: &str) -> Result<usize> {
        let store = self.session_db.lock().expect("lock poisoned");
        let result = store.conn().query_row(
            "SELECT summary_at_count FROM sessions WHERE id = ?1",
            rusqlite::params![session_id],
            |row| row.get::<_, i64>(0),
        );
        Ok(result.unwrap_or(0) as usize)
    }

    async fn get_message_count(&self, session_id: &str) -> Result<usize> {
        let store = self.session_db.lock().expect("lock poisoned");
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
        let store = self.memory_db.lock().expect("lock poisoned");
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
                layer: r.layer.clone(),
                similarity: 0.0,
            })
            .collect())
    }

    async fn kg_edges_between(
        &self,
        node_names: &[String],
        scope: Option<&str>,
    ) -> Result<Vec<loom_types::KgEdge>> {
        let store = self.memory_db.lock().expect("lock poisoned");
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

        // Get all edges between these nodes, filtered by scope
        let edges = graph.edges_between(&node_ids, scope)?;

        // Convert to KgEdge format
        Ok(edges
            .into_iter()
            .map(
                |(source, target, relation_type, confidence)| loom_types::KgEdge {
                    source,
                    target,
                    relation_type,
                    fact: String::new(),
                    confidence,
                },
            )
            .collect())
    }

    async fn kg_delete_node(&self, name: &str) -> Result<bool> {
        let store = self.memory_db.lock().expect("lock poisoned");
        GraphStore::new(store.conn()).delete_node(name)
    }

    async fn kg_delete_edge(&self, source: &str, target: &str, relation: &str) -> Result<bool> {
        let store = self.memory_db.lock().expect("lock poisoned");
        GraphStore::new(store.conn()).delete_edge(source, target, relation)
    }

    async fn cognition_list(
        &self,
        subject: &str,
        scope: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<loom_types::Cognition>> {
        let store = self.memory_db.lock().expect("lock poisoned");
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
        let store = self.memory_db.lock().expect("lock poisoned");
        let cognitions = loom_memory::CognitionStore::new(store.conn());
        cognitions.list_subjects()
    }

    async fn cognition_snapshots(
        &self,
        cognition_id: i64,
    ) -> Result<Vec<loom_types::CognitionHistory>> {
        let store = self.memory_db.lock().expect("lock poisoned");
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

    async fn cognition_delete(&self, id: i64) -> Result<bool> {
        let store = self.memory_db.lock().expect("lock poisoned");
        loom_memory::CognitionStore::new(store.conn()).delete(id)
    }

    async fn kg_prune(&self, older_than_days: i64) -> Result<usize> {
        let store = self.memory_db.lock().expect("lock poisoned");
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
        let store = self.session_db.lock().expect("lock poisoned");
        let model = model.trim();
        store.conn().execute(
            "INSERT INTO token_usage (session_id, model, prompt_tokens, completion_tokens, cached_tokens, cached_read_tokens, cached_write_tokens, latency_ms, context_window) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![session_id, model, prompt_tokens as i64, completion_tokens as i64, (cached_read_tokens + cached_write_tokens) as i64, cached_read_tokens as i64, cached_write_tokens as i64, latency_ms as i64, context_window as i64],
        )?;
        Ok(())
    }

    async fn get_token_summary(&self, from: &str, to: &str) -> Result<serde_json::Value> {
        let store = self.session_db.lock().expect("lock poisoned");
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
            "SELECT TRIM(model) as model, SUM(prompt_tokens) as p, SUM(completion_tokens) as c, SUM(cached_tokens) as ca, SUM(cached_read_tokens) as cr, SUM(cached_write_tokens) as cw, COUNT(*) as r, COALESCE(AVG(latency_ms), 0) as l, COALESCE(AVG(CASE WHEN context_window > 0 THEN CAST(prompt_tokens AS REAL) / context_window ELSE CAST(prompt_tokens AS REAL) / 100000.0 END), 0) as cu FROM token_usage WHERE created_at >= ?1 AND created_at <= ?2 GROUP BY TRIM(model) ORDER BY r DESC",
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
        let store = self.session_db.lock().expect("lock poisoned");
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

    async fn reset_token_usage(&self) -> Result<()> {
        let store = self.session_db.lock().expect("lock poisoned");
        store.conn().execute("DELETE FROM token_usage", [])?;
        Ok(())
    }

    // ── Vector embedding & semantic similarity search ──────────────────────

    async fn embed_entity(&self, name: &str, embedding: Vec<f32>) -> Result<()> {
        let store = self.memory_db.lock().expect("lock poisoned");
        let graph = loom_memory::GraphStore::new(store.conn());
        graph.embed_node(name, &embedding)
    }

    async fn search_similar_entities(
        &self,
        query: &str,
        embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<loom_types::KgNode>> {
        let store = self.memory_db.lock().expect("lock poisoned");
        let graph = loom_memory::GraphStore::new(store.conn());
        // Use the query text as a FTS5 fallback when embeddings are not yet
        // available for the query (embedding vector is empty or absent).
        let fallback = if query.is_empty() { None } else { Some(query) };
        let rows = graph.search_similar(embedding, limit, fallback, None)?;
        Ok(rows
            .iter()
            .map(|(row, score)| loom_types::KgNode {
                node_id: row.node_id,
                name: row.name.clone(),
                entity_type: row.entity_type.clone(),
                description: row.description.clone(),
                confidence: row.confidence,
                scope: row.scope.clone(),
                layer: "semantic".to_string(),
                similarity: *score,
            })
            .collect())
    }

    // ── Memory quality feedback loop ───────────────────────────────────────

    async fn record_memory_quality(
        &self,
        session_id: &str,
        turn_seq: i64,
        injected: &[String],
        duration_ms: i64,
    ) -> Result<i64> {
        let injected_json = serde_json::to_string(injected).unwrap_or_else(|_| "[]".to_string());
        // Compute turn_seq from session DB if not explicitly provided
        let effective_seq = if turn_seq > 0 {
            turn_seq
        } else {
            let sess = self.session_db.lock().expect("lock poisoned");
            sess.conn().query_row(
                "SELECT COALESCE(MAX(seq), 0) FROM message_history WHERE session_id = ?1 AND role = 'user'",
                rusqlite::params![session_id],
                |r| r.get::<_, i64>(0),
            ).unwrap_or(0)
        };
        let mem = self.memory_db.lock().expect("lock poisoned");
        let graph = GraphStore::new(mem.conn());
        graph.record_quality_log(session_id, effective_seq, &injected_json, duration_ms)
    }

    async fn update_quality_references(&self, log_id: i64, referenced: &[String]) -> Result<()> {
        let referenced_json =
            serde_json::to_string(referenced).unwrap_or_else(|_| "[]".to_string());
        let mem = self.memory_db.lock().expect("lock poisoned");
        let graph = GraphStore::new(mem.conn());
        graph.update_quality_log_references(log_id, &referenced_json)
    }

    async fn memory_quality_report(
        &self,
        lookback_days: i64,
    ) -> Result<loom_types::MemoryQualityReport> {
        let mem = self.memory_db.lock().expect("lock poisoned");
        let graph = GraphStore::new(mem.conn());
        let report = graph.evaluate_memory_quality(lookback_days)?;
        Ok(loom_types::MemoryQualityReport {
            avg_relevance: report.avg_relevance,
            injection_count: report.injection_count,
            turns_with_references: report.turns_with_references,
            total_entities: report.total_entities,
            duplicate_rate: report.duplicate_rate,
            stale_entity_count: report.stale_entity_count,
            avg_confidence: report.avg_confidence,
            entity_types_distribution: report.entity_types_distribution,
            layer_distribution: report.layer_distribution,
            entities_added_recently: report.entities_added_recently,
            entities_accessed_recently: report.entities_accessed_recently,
            consolidation_runs: report.consolidation_runs,
            total_merged: report.total_merged,
            health_score: report.health_score,
        })
    }

    // ── Phase 2: Memory consolidation, persona, patterns, layers ─────────────

    async fn run_consolidation_cycle(&self) -> Result<String> {
        let mem = self.memory_db.lock().expect("lock poisoned");
        let consolidator = loom_memory::MemoryConsolidator::new(mem.conn());
        // Run dedup-based consolidation (merge duplicate kg_nodes, cognitions,
        // promote session-scoped nodes to episodic).
        let report = consolidator.run_consolidation_cycle()?;
        let json = serde_json::to_string(&report)
            .unwrap_or_else(|_| r#"{"summary":"serialisation error"}"#.into());
        tracing::info!(summary = %report.summary, "consolidation cycle completed");
        Ok(json)
    }

    async fn detect_patterns(&self) -> Result<String> {
        let mem = self.memory_db.lock().expect("lock poisoned");
        let detector = loom_memory::SessionPatternDetector::new(mem.conn());
        let report = detector.detect_all()?;
        let json = serde_json::to_string(&report).unwrap_or_else(|_| r#"{}"#.into());
        tracing::info!(
            topics = report.topics.len(),
            tools = report.tools.len(),
            "pattern detection completed"
        );
        Ok(json)
    }

    async fn get_layer_stats(&self) -> Result<Vec<(String, i64)>> {
        let mem = self.memory_db.lock().expect("lock poisoned");
        let graph = loom_memory::GraphStore::new(mem.conn());
        graph.get_layer_stats()
    }

    async fn promote_to_layer(&self, node_name: &str, layer: &str) -> Result<()> {
        let mem = self.memory_db.lock().expect("lock poisoned");
        let graph = loom_memory::GraphStore::new(mem.conn());
        // Resolve node name to id, then promote
        if let Some(node_id) = graph.resolve_node(node_name)? {
            graph.promote_node_layer(node_id, layer)?;
            tracing::info!(node = %node_name, layer, "promote_to_layer");
        } else {
            anyhow::bail!("node '{}' not found for layer promotion", node_name);
        }
        Ok(())
    }

    // ── Phase 3: Full pipeline operations ───────────────────────────────────

    async fn run_forgetting_cycle(&self, min_importance: f64, max_age_days: i64) -> Result<String> {
        use loom_core::ForgettingReport;
        let mem = self.memory_db.lock().expect("lock poisoned");
        let conn = mem.conn();
        let graph = loom_memory::GraphStore::new(conn);

        // Use GraphStore::active_forgetting which applies safety protection rules:
        // - entity_type='Person' → never pruned
        // - evidence_count >= 10 → never pruned
        // - scope='global' AND layer='global' → never pruned
        // - access_count > 50 → never pruned
        let report = graph.active_forgetting(min_importance, max_age_days)?;

        let output = ForgettingReport {
            cycle_timestamp: chrono::Utc::now().to_rfc3339(),
            nodes_removed: report.pruned_nodes as usize,
            edges_removed: report.pruned_edges as usize,
            cognitions_removed: report.pruned_cognitions as usize,
            skipped_protected: report.skipped_protected,
            min_importance_threshold: min_importance,
            max_age_days,
            summary: format!(
                "Forgetting cycle: removed {} nodes, {} edges, {} cognitions (min_importance={}, max_age={}d); skipped {} protected",
                report.pruned_nodes,
                report.pruned_edges,
                report.pruned_cognitions,
                min_importance,
                max_age_days,
                report.skipped_protected
            ),
        };
        let json = serde_json::to_string(&output)
            .unwrap_or_else(|_| r#"{"summary":"serialisation error"}"#.into());
        tracing::info!(
            nodes = report.pruned_nodes,
            edges = report.pruned_edges,
            cognitions = report.pruned_cognitions,
            skipped_protected = report.skipped_protected,
            "forgetting cycle completed"
        );
        Ok(json)
    }

    async fn get_memory_health(&self) -> Result<String> {
        use loom_core::MemoryHealth;
        let mem = self.memory_db.lock().expect("lock poisoned");
        let conn = mem.conn();
        let graph = loom_memory::GraphStore::new(conn);

        let total_nodes: usize = conn
            .query_row("SELECT COUNT(*) FROM kg_nodes", [], |r| r.get(0))
            .unwrap_or(0);
        let total_edges: usize = conn
            .query_row("SELECT COUNT(*) FROM kg_edges", [], |r| r.get(0))
            .unwrap_or(0);
        let total_cognitions: usize = conn
            .query_row("SELECT COUNT(*) FROM cognitions", [], |r| r.get(0))
            .unwrap_or(0);

        // Stale nodes: last_updated more than 90 days ago
        let stale_nodes: usize = conn
            .query_row(
                "SELECT COUNT(*) FROM kg_nodes WHERE last_updated < datetime('now', '-90 days')",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);

        // Orphan nodes: nodes with no edges
        let orphan_nodes: usize = conn
            .query_row(
                "SELECT COUNT(*) FROM kg_nodes n WHERE NOT EXISTS (SELECT 1 FROM kg_edges e WHERE e.source_id = n.id OR e.target_id = n.id)",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);

        // Layer distribution
        let layer_distribution = graph.get_layer_stats().unwrap_or_default();

        // Fragmentation score: ratio of orphan nodes to total
        let fragmentation_score = if total_nodes > 0 {
            orphan_nodes as f64 / total_nodes as f64
        } else {
            0.0
        };

        let status = if fragmentation_score > 0.5 {
            "critical"
        } else if fragmentation_score > 0.3 {
            "degraded"
        } else {
            "healthy"
        };

        let health = MemoryHealth {
            total_nodes,
            total_edges,
            total_cognitions,
            stale_nodes,
            orphan_nodes,
            layer_distribution,
            fragmentation_score: (fragmentation_score * 100.0).round() / 100.0,
            status: status.to_string(),
            checked_at: chrono::Utc::now().to_rfc3339(),
        };
        let json =
            serde_json::to_string(&health).unwrap_or_else(|_| r#"{"status":"unknown"}"#.into());
        tracing::info!(
            nodes = total_nodes,
            edges = total_edges,
            status = %status,
            fragmentation = (fragmentation_score * 100.0),
            "memory health check completed"
        );
        Ok(json)
    }

    async fn evaluate_quality(&self, lookback_days: i64) -> Result<String> {
        use loom_core::QualityEvaluation;
        let mem = self.memory_db.lock().expect("lock poisoned");
        let conn = mem.conn();

        // Total injected entities (sum of json_array_length) in lookback window
        let total_injected_entities: usize = conn
            .query_row(
                "SELECT COALESCE(SUM(json_array_length(injected_entities)), 0) FROM memory_quality_log WHERE created_at >= datetime('now', '-' || ?1 || ' days') AND injected_entities IS NOT NULL AND injected_entities != ''",
                rusqlite::params![lookback_days],
                |r| r.get(0),
            )
            .unwrap_or(0);

        // Total referenced entities in lookback window
        let total_references: usize = conn
            .query_row(
                "SELECT COALESCE(SUM(json_array_length(referenced_entities)), 0) FROM memory_quality_log WHERE created_at >= datetime('now', '-' || ?1 || ' days') AND referenced_entities IS NOT NULL AND referenced_entities != ''",
                rusqlite::params![lookback_days],
                |r| r.get(0),
            )
            .unwrap_or(0);

        let recall_rate = if total_injected_entities > 0 {
            (total_references as f64 / total_injected_entities as f64).min(1.0)
        } else {
            0.0
        };

        // Top entities: most recently updated, highest confidence
        let top_entities: Vec<String> = {
            let mut stmt = conn
                .prepare(
                    "SELECT name FROM kg_nodes WHERE scope = 'global' ORDER BY confidence DESC, last_updated DESC LIMIT 10",
                )
                .ok();
            let mut names = Vec::new();
            if let Some(ref mut s) = stmt
                && let Ok(rows) = s.query_map([], |row| row.get::<_, String>(0))
            {
                for name in rows.flatten() {
                    names.push(name);
                }
            }
            names
        };

        // Stale entities: not referenced anywhere recently
        let stale_entities: Vec<String> = {
            let mut stmt = conn
                .prepare(
                    "SELECT name FROM kg_nodes WHERE last_updated < datetime('now', '-' || ?1 || ' days') AND scope = 'global' ORDER BY last_updated ASC LIMIT 10",
                )
                .ok();
            let mut names = Vec::new();
            if let Some(ref mut s) = stmt
                && let Ok(rows) = s.query_map(rusqlite::params![lookback_days], |row| {
                    row.get::<_, String>(0)
                })
            {
                for name in rows.flatten() {
                    names.push(name);
                }
            }
            names
        };

        let quality_score = recall_rate * 100.0;

        let mut recommendations: Vec<String> = Vec::new();
        if recall_rate < 0.3 {
            recommendations.push("Recall rate low: consider reducing entity extraction noise or increasing entity relevance thresholds.".into());
        }
        if stale_entities.len() > 5 {
            recommendations.push(format!(
                "{} stale entities detected: run forgetting cycle or manual prune.",
                stale_entities.len()
            ));
        }

        let report = QualityEvaluation {
            lookback_days,
            total_injections: total_injected_entities,
            total_references,
            recall_rate: (recall_rate * 100.0).round() / 100.0,
            top_entities,
            stale_entities,
            quality_score: (quality_score * 100.0).round() / 100.0,
            recommendations,
            evaluated_at: chrono::Utc::now().to_rfc3339(),
        };
        let json =
            serde_json::to_string(&report).unwrap_or_else(|_| r#"{"recall_rate":0.0}"#.into());
        tracing::info!(
            injected_entities = total_injected_entities,
            references = total_references,
            recall_rate = recall_rate,
            "quality evaluation completed"
        );
        Ok(json)
    }

    async fn get_pipeline_status(&self) -> Result<String> {
        let mem = self.memory_db.lock().expect("lock poisoned");
        let conn = mem.conn();

        let node_count: usize = conn
            .query_row("SELECT COUNT(*) FROM kg_nodes", [], |r| r.get(0))
            .unwrap_or(0);
        let edge_count: usize = conn
            .query_row("SELECT COUNT(*) FROM kg_edges", [], |r| r.get(0))
            .unwrap_or(0);
        let cognition_count: usize = conn
            .query_row("SELECT COUNT(*) FROM cognitions", [], |r| r.get(0))
            .unwrap_or(0);

        // Recent extraction count (last 24h) — count memory_quality_log entries
        // (the actual injection log table used by the extraction pipeline),
        // falling back to recent chat_turn events when the quality log is empty.
        let recent_extractions: usize = {
            let ql_count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM memory_quality_log WHERE created_at >= datetime('now', '-1 day')",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            if ql_count > 0 {
                ql_count as usize
            } else {
                // Fallback: count recent chat turns via events table
                conn.query_row(
                    "SELECT COUNT(*) FROM events WHERE event_type = 'chat_turn' AND timestamp >= ?1",
                    rusqlite::params![chrono::Utc::now().timestamp() - 86400],
                    |r| r.get(0),
                ).unwrap_or(0) as usize
            }
        };

        // Last consolidation (most recent kg_nodes update to 'global' scope)
        let last_consolidation: String = conn
            .query_row(
                "SELECT COALESCE(MAX(last_updated), 'never') FROM kg_nodes WHERE scope = 'global'",
                [],
                |r| r.get(0),
            )
            .unwrap_or_else(|_| "never".to_string());

        let status = serde_json::json!({
            "status": "active",
            "node_count": node_count,
            "edge_count": edge_count,
            "cognition_count": cognition_count,
            "recent_extractions_24h": recent_extractions,
            "last_consolidation": last_consolidation,
            "checked_at": chrono::Utc::now().to_rfc3339(),
        });
        let json =
            serde_json::to_string(&status).unwrap_or_else(|_| r#"{"status":"error"}"#.into());
        Ok(json)
    }
}

#[cfg(test)]
mod import_tests {
    use super::*;
    use loom_import::build_payload;
    use loom_types::{ContentPart, ImportOutcome, Message, Role};

    fn store_in_tmp() -> LoomMemoryStore {
        let dir = tempfile::tempdir().expect("tmpdir");
        LoomMemoryStore::open(dir.path()).expect("open")
    }

    fn sample_payload(id: &str) -> loom_types::ImportPayload {
        loom_types::ImportPayload {
            id: id.into(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            title: Some("Imported".into()),
            workspace_path: Some("C:/proj".into()),
            messages: vec![
                Message {
                    role: Role::User,
                    content: vec![ContentPart::Text { text: "hi".into() }],
                    timestamp: chrono::Utc::now(),
                    usage: None,
                },
                Message {
                    role: Role::Assistant,
                    content: vec![ContentPart::Text {
                        text: "hello".into(),
                    }],
                    timestamp: chrono::Utc::now(),
                    usage: Some(loom_types::inference::TokenUsage {
                        prompt_tokens: 10,
                        completion_tokens: 5,
                        model: "glm-5.2".into(),
                        ..Default::default()
                    }),
                },
            ],
        }
    }

    #[tokio::test]
    async fn import_persists_session_and_messages() {
        let store = store_in_tmp();
        let payload = sample_payload("s1");
        let outcome = store.import_session(&payload, false).await.expect("import");
        assert_eq!(outcome, ImportOutcome::Created);

        let msgs = store.load_history("s1", 1000).await.expect("load");
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, Role::User);
        assert_eq!(msgs[1].role, Role::Assistant);
        let u = msgs[1].usage.as_ref().expect("usage");
        assert_eq!(u.prompt_tokens, 10);
        assert_eq!(u.model, "glm-5.2");
    }

    #[tokio::test]
    async fn import_is_idempotent_without_replace() {
        let store = store_in_tmp();
        let payload = sample_payload("s2");
        assert_eq!(
            store.import_session(&payload, false).await.unwrap(),
            ImportOutcome::Created
        );
        assert_eq!(
            store.import_session(&payload, false).await.unwrap(),
            ImportOutcome::AlreadyExists
        );
        let msgs = store.load_history("s2", 1000).await.unwrap();
        assert_eq!(msgs.len(), 2, "no duplicate messages");
    }

    #[tokio::test]
    async fn import_replace_rebuilds_messages() {
        let store = store_in_tmp();
        let mut payload = sample_payload("s3");
        assert_eq!(
            store.import_session(&payload, false).await.unwrap(),
            ImportOutcome::Created
        );
        // mutate: add a third message, replace
        payload.messages.push(Message::user("extra"));
        assert_eq!(
            store.import_session(&payload, true).await.unwrap(),
            ImportOutcome::Replaced
        );
        let msgs = store.load_history("s3", 1000).await.unwrap();
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[2].text_content(), "extra");
    }

    #[tokio::test]
    async fn build_payload_then_import_roundtrips() {
        // Ensures the parser output survives a persist+load cycle.
        let dir = tempfile::tempdir().expect("tmpdir");
        let jsonl = dir.path().join("x.jsonl");
        std::fs::write(
            &jsonl,
            "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"round\"},\"timestamp\":\"2026-07-11T01:00:00.000Z\",\"cwd\":\"C:/p\",\"sessionId\":\"x\"}\n\
             {\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"model\":\"m\",\"content\":[{\"type\":\"text\",\"text\":\"trip\"}],\"usage\":{\"input_tokens\":1,\"output_tokens\":2}},\"timestamp\":\"2026-07-11T01:01:00.000Z\",\"sessionId\":\"x\"}\n",
        )
        .unwrap();
        let payload = build_payload(&jsonl).expect("build");
        let store = LoomMemoryStore::open(dir.path()).expect("open");
        store.import_session(&payload, false).await.expect("import");
        let msgs = store.load_history(&payload.id, 1000).await.expect("load");
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[1].text_content(), "trip");
    }
}
