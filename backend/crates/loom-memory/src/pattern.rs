// SPDX-License-Identifier: Apache-2.0
//! Cross-session pattern detection.
//!
//! Analyzes knowledge graph entities, event history, and temporal data to discover
//! recurring topics, tool preferences, learning progression, and time-based
//! activity patterns that span multiple user sessions.
//!
//! All detectors handle empty databases gracefully — returning empty vectors
//! rather than errors. Query errors are logged as warnings.

use anyhow::Result;
use chrono::Timelike;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

// ============================================================================
// Output types
// ============================================================================

/// A topic that recurred across multiple sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicPattern {
    /// Entity name (topic label).
    pub topic: String,
    /// Number of distinct sessions where this topic appeared.
    pub session_count: i64,
    /// ISO-8601 timestamp of the earliest occurrence.
    pub first_seen: String,
    /// ISO-8601 timestamp of the latest occurrence.
    pub last_seen: String,
}

/// A tool the user invokes repeatedly, with usage statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPreference {
    /// Tool name as it appears in `tool_use` blocks (e.g. "Bash", "Read").
    pub tool: String,
    /// Total number of invocations across all sessions.
    pub usage_count: i64,
    /// Average confidence of the enclosing events that triggered the tool.
    pub avg_confidence: f64,
}

/// A progression of technologies / concepts within a single domain,
/// suggesting increasing depth of knowledge across sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningPath {
    /// Inferred domain label (e.g. "containers", "web", "database").
    pub domain: String,
    /// Ordered list of entity names from earliest to most recent.
    pub stages: Vec<String>,
    /// Heuristic confidence (0.0–1.0) that this actually represents a
    /// learning progression rather than random co-occurrence.
    pub confidence: f64,
}

/// An activity-time bucket indicating when the user tends to perform
/// a certain type of action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimePattern {
    /// Label for the activity (event type or action).
    pub activity: String,
    /// Hour of day (0–23) in the local timezone of the recorded timestamp.
    pub hour_bucket: i32,
    /// How many events of this activity fell in this hour bucket.
    pub frequency: i64,
}

/// Combined result of all cross-session pattern detectors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionPatternReport {
    pub topics: Vec<TopicPattern>,
    pub tools: Vec<ToolPreference>,
    pub learning: Vec<LearningPath>,
    pub time_patterns: Vec<TimePattern>,
}

// ============================================================================
// Detector
// ============================================================================

/// Cross-session pattern detector backed by the memory database
/// (`events`, `kg_nodes`, `kg_edges`, `kg_evidence` tables).
pub struct SessionPatternDetector<'a> {
    conn: &'a Connection,
}

impl<'a> SessionPatternDetector<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    // ------------------------------------------------------------------------
    // 1. Topic Frequency
    // ------------------------------------------------------------------------

    /// Find entities that appear in at least `min_sessions` distinct sessions.
    ///
    /// Uses two strategies and merges them:
    /// 1. Evidence links: kg_nodes → kg_evidence → events.source_session
    /// 2. Scope-based: kg_nodes with session-specific scopes (not "global")
    pub fn detect_topic_frequency(&self, min_sessions: usize) -> Result<Vec<TopicPattern>> {
        // Strategy A — evidence-linked nodes
        let mut stmt = self.conn.prepare(
            "SELECT n.name, n.entity_type,
                    e.source_session,
                    e.timestamp
             FROM kg_nodes n
             JOIN kg_evidence ev ON ev.node_id = n.id
             JOIN events e ON ev.event_id = e.id
             WHERE e.source_session IS NOT NULL AND e.source_session != ''
             ORDER BY n.name, n.entity_type",
        )?;

        let rows: Vec<(String, String, String, String)> = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        // Strategy B — scope-based (session-scoped nodes without evidence links)
        let mut stmt2 = self.conn.prepare(
            "SELECT name, entity_type,
                    scope,
                    datetime(first_seen, 'unixepoch') as first_ts,
                    datetime(last_updated, 'unixepoch') as last_ts
             FROM kg_nodes
             WHERE scope IS NOT NULL
               AND scope NOT IN ('global', '')
             ORDER BY name, entity_type",
        )?;

        let scope_rows: Vec<(String, String, String, String, String)> = stmt2
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        // Merge: key = (name, entity_type) → (set of sessions, earliest_ts, latest_ts)
        let mut groups: HashMap<(String, String), (HashSet<String>, String, String)> =
            HashMap::new();

        for (name, etype, session, ts) in &rows {
            let entry = groups
                .entry((name.clone(), etype.clone()))
                .or_insert_with(|| (HashSet::new(), ts.clone(), ts.clone()));
            entry.0.insert(session.clone());
            if *ts < entry.1 {
                entry.1 = ts.clone();
            }
            if *ts > entry.2 {
                entry.2 = ts.clone();
            }
        }

        for (name, etype, scope, first_ts, last_ts) in &scope_rows {
            let entry = groups
                .entry((name.clone(), etype.clone()))
                .or_insert_with(|| (HashSet::new(), first_ts.clone(), last_ts.clone()));
            entry.0.insert(scope.clone());
            if *first_ts < entry.1 {
                entry.1 = first_ts.clone();
            }
            if *last_ts > entry.2 {
                entry.2 = last_ts.clone();
            }
        }

        let mut patterns: Vec<TopicPattern> = groups
            .into_iter()
            .filter(|(_, (sessions, _, _))| sessions.len() >= min_sessions)
            .map(|((name, etype), (sessions, first, last))| TopicPattern {
                topic: if etype.is_empty() || etype == "Concept" {
                    name
                } else {
                    format!("{name} ({etype})")
                },
                session_count: sessions.len() as i64,
                first_seen: first,
                last_seen: last,
            })
            .collect();

        patterns.sort_by_key(|b| std::cmp::Reverse(b.session_count));
        Ok(patterns)
    }

    // ------------------------------------------------------------------------
    // 2. Tool Preferences
    // ------------------------------------------------------------------------

    /// Detect tool preferences by parsing `tool_use` blocks from the assistant
    /// response embedded in event payloads.
    ///
    /// The payload JSON has an `assistant_response` field containing a JSON
    /// array of Anthropic content blocks. We scan these for `tool_use` blocks
    /// and aggregate usage counts per tool name.
    pub fn detect_tool_preferences(&self) -> Result<Vec<ToolPreference>> {
        let mut stmt = self.conn.prepare(
            "SELECT payload, confidence
             FROM events
             WHERE payload IS NOT NULL AND payload != ''
             ORDER BY rowid",
        )?;

        let rows: Vec<(String, f64)> = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        // tool_name → (total_count, sum_confidence)
        let mut tool_stats: HashMap<String, (i64, f64)> = HashMap::new();

        for (payload_str, confidence) in &rows {
            // Parse the outer payload JSON
            let payload: serde_json::Value = match serde_json::from_str(payload_str) {
                Ok(v) => v,
                Err(_) => continue,
            };

            // Extract assistant_response — it is a JSON string of content blocks
            let assistant_response = match payload.get("assistant_response") {
                Some(serde_json::Value::String(s)) => s.clone(),
                _ => continue,
            };

            // Parse assistant_response as a JSON array of content blocks
            let blocks: Vec<serde_json::Value> = match serde_json::from_str(&assistant_response) {
                Ok(v) => v,
                Err(_) => continue,
            };

            // Collect tool names from tool_use blocks in this event
            let mut seen_in_event: HashSet<String> = HashSet::new();
            for block in &blocks {
                if block.get("type").and_then(|t| t.as_str()) == Some("tool_use")
                    && let Some(name) = block.get("name").and_then(|n| n.as_str())
                {
                    seen_in_event.insert(name.to_string());
                }
            }

            // Count each tool once per event (not per block)
            for tool_name in &seen_in_event {
                let entry = tool_stats.entry(tool_name.clone()).or_insert((0, 0.0));
                entry.0 += 1;
                entry.1 += *confidence;
            }

            // Also count tools even if no tool_use blocks found but tool_calls > 0
            let tool_calls_count = payload
                .get("tool_calls")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            if tool_calls_count > 0 && seen_in_event.is_empty() {
                let entry = tool_stats
                    .entry("<unknown tool>".to_string())
                    .or_insert((0, 0.0));
                entry.0 += tool_calls_count as i64;
                entry.1 += *confidence;
            }
        }

        let mut preferences: Vec<ToolPreference> = tool_stats
            .into_iter()
            .map(|(tool, (count, sum_conf))| ToolPreference {
                tool,
                usage_count: count,
                avg_confidence: if count > 0 {
                    (sum_conf / count as f64 * 100.0).round() / 100.0
                } else {
                    0.0
                },
            })
            .collect();

        preferences.sort_by_key(|b| std::cmp::Reverse(b.usage_count));
        Ok(preferences)
    }

    // ------------------------------------------------------------------------
    // 3. Learning Progression
    // ------------------------------------------------------------------------

    /// Detect learning progression by grouping technology-type entities into
    /// inferred domains and ordering them by first_seen timestamp.
    ///
    /// Heuristic: entities within the same domain that appear in chronological
    /// order suggest increasing depth. For example, "Docker basics" appearing
    /// before "Kubernetes" suggests a containers learning path.
    pub fn detect_learning_progression(&self) -> Result<Vec<LearningPath>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, entity_type, first_seen, confidence
             FROM kg_nodes
             WHERE entity_type IN ('Technology','Tool','Language','Framework','Library','Platform','Concept')
             ORDER BY first_seen ASC",
        )?;

        let entities: Vec<(String, String, i64, f64)> = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, f64>(3)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        if entities.is_empty() {
            return Ok(Vec::new());
        }

        // Map each entity to one or more domain keys via keyword matching
        let domain_keywords: Vec<(&str, &[&str])> = vec![
            (
                "containers",
                &[
                    "docker",
                    "kubernetes",
                    "k8s",
                    "container",
                    "podman",
                    "helm",
                    "rancher",
                    "openshift",
                ],
            ),
            (
                "web",
                &[
                    "html",
                    "css",
                    "javascript",
                    "react",
                    "vue",
                    "angular",
                    "svelte",
                    "next",
                    "nuxt",
                    "webpack",
                    "vite",
                    "http",
                    "rest",
                    "graphql",
                ],
            ),
            (
                "database",
                &[
                    "sql", "postgres", "mysql", "sqlite", "mongo", "redis", "database", "orm",
                    "prisma", "drizzle",
                ],
            ),
            (
                "cloud",
                &[
                    "aws",
                    "azure",
                    "gcp",
                    "cloud",
                    "serverless",
                    "lambda",
                    "s3",
                    "ec2",
                    "terraform",
                ],
            ),
            (
                "ai-ml",
                &[
                    "ai",
                    "ml",
                    "machine learning",
                    "neural",
                    "deep learning",
                    "llm",
                    "transformer",
                    "pytorch",
                    "tensorflow",
                    "gpt",
                    "bert",
                    "embedding",
                ],
            ),
            (
                "devops",
                &[
                    "ci/cd",
                    "jenkins",
                    "github actions",
                    "gitlab",
                    "ansible",
                    "pipeline",
                    "deploy",
                    "monitoring",
                    "prometheus",
                    "grafana",
                ],
            ),
            (
                "mobile",
                &[
                    "ios",
                    "android",
                    "swift",
                    "kotlin",
                    "flutter",
                    "react native",
                    "mobile",
                ],
            ),
            (
                "systems",
                &[
                    "rust",
                    "c",
                    "c++",
                    "memory",
                    "concurrency",
                    "async",
                    "thread",
                    "kernel",
                    "os",
                ],
            ),
            (
                "data",
                &[
                    "python",
                    "pandas",
                    "numpy",
                    "spark",
                    "kafka",
                    "etl",
                    "pipeline",
                    "dataframe",
                ],
            ),
            (
                "security",
                &[
                    "security",
                    "auth",
                    "oauth",
                    "jwt",
                    "tls",
                    "ssl",
                    "encrypt",
                    "vulnerability",
                ],
            ),
        ];

        // domain_key → Vec<(name, first_seen, confidence)>
        let mut domain_entities: HashMap<String, Vec<(String, i64, f64)>> = HashMap::new();

        for (name, _etype, first_seen, confidence) in &entities {
            let lower = name.to_lowercase();
            for (domain, keywords) in &domain_keywords {
                if keywords.iter().any(|kw| lower.contains(kw)) {
                    domain_entities
                        .entry(domain.to_string())
                        .or_default()
                        .push((name.clone(), *first_seen, *confidence));
                }
            }
        }

        let mut paths: Vec<LearningPath> = Vec::new();
        let min_stages_for_path: usize = 2;

        for (domain, mut stages) in domain_entities {
            // Remove duplicate names (keep earliest occurrence)
            let mut seen: HashSet<String> = HashSet::new();
            stages.retain(|(name, _, _)| seen.insert(name.clone()));
            stages.sort_by_key(|(_, ts, _)| *ts);

            if stages.len() < min_stages_for_path {
                continue;
            }

            // Confidence: avg of entity confidences, boosted by count
            let avg_conf: f64 = stages.iter().map(|(_, _, c)| c).sum::<f64>() / stages.len() as f64;
            let stage_bonus: f64 = ((stages.len() as f64 - 1.0) * 0.05).min(0.3);
            let confidence = (avg_conf + stage_bonus).min(1.0);

            paths.push(LearningPath {
                domain,
                stages: stages.into_iter().map(|(n, _, _)| n).collect(),
                confidence: (confidence * 100.0).round() / 100.0,
            });
        }

        paths.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(paths)
    }

    // ------------------------------------------------------------------------
    // 4. Time Correlations
    // ------------------------------------------------------------------------

    /// Analyze event timestamps to detect temporal activity patterns.
    /// Buckets events by local hour of day and activity type.
    pub fn detect_time_correlations(&self) -> Result<Vec<TimePattern>> {
        let mut stmt = self.conn.prepare(
            "SELECT timestamp, event_type, action
             FROM events
             WHERE timestamp IS NOT NULL AND timestamp != ''",
        )?;

        let rows: Vec<(String, String, String)> = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        if rows.is_empty() {
            return Ok(Vec::new());
        }

        // (activity_label, hour_bucket) → frequency
        let mut buckets: HashMap<(String, i32), i64> = HashMap::new();

        for (ts_str, event_type, _action) in &rows {
            // Try RFC 3339 first, then ISO 8601, then Unix timestamp as fallback
            let hour: Option<i32> = chrono::DateTime::parse_from_rfc3339(ts_str)
                .ok()
                .map(|dt| dt.hour() as i32)
                .or_else(|| {
                    chrono::NaiveDateTime::parse_from_str(ts_str, "%Y-%m-%dT%H:%M:%S")
                        .ok()
                        .map(|dt| dt.time().hour() as i32)
                })
                .or_else(|| {
                    chrono::NaiveDateTime::parse_from_str(ts_str, "%Y-%m-%d %H:%M:%S")
                        .ok()
                        .map(|dt| dt.time().hour() as i32)
                })
                .or_else(|| {
                    ts_str.parse::<i64>().ok().map(|unix| {
                        // Approximate hour from Unix timestamp
                        let secs_since_midnight = unix % 86400;
                        let local_offset = chrono::Local::now().offset().local_minus_utc() as i64;
                        let adjusted = (secs_since_midnight + local_offset) % 86400;
                        if adjusted < 0 {
                            ((adjusted + 86400) / 3600) as i32
                        } else {
                            (adjusted / 3600) as i32
                        }
                    })
                });

            let hour = match hour {
                Some(h) if (0..24).contains(&h) => h,
                _ => continue,
            };

            // Use event_type as the primary activity label; normalize chat_turn → "chat"
            let activity = if event_type.is_empty() || event_type == "chat_turn" {
                "chat".to_string()
            } else {
                event_type.clone()
            };

            *buckets.entry((activity, hour)).or_insert(0) += 1;
        }

        let mut patterns: Vec<TimePattern> = buckets
            .into_iter()
            .map(|((activity, hour_bucket), frequency)| TimePattern {
                activity,
                hour_bucket,
                frequency,
            })
            .collect();

        patterns.sort_by(|a, b| {
            b.frequency
                .cmp(&a.frequency)
                .then_with(|| a.hour_bucket.cmp(&b.hour_bucket))
        });
        Ok(patterns)
    }

    // ------------------------------------------------------------------------
    // 5. Combined report
    // ------------------------------------------------------------------------

    /// Run all four detectors and return a combined report.
    ///
    /// Individual detector failures are logged as warnings and result in
    /// empty vectors — they never cause the overall report to fail.
    pub fn detect_all(&self) -> Result<SessionPatternReport> {
        let topics = self.detect_topic_frequency(1).unwrap_or_else(|e| {
            tracing::warn!("detect_topic_frequency failed: {e:#}");
            Vec::new()
        });

        let tools = self.detect_tool_preferences().unwrap_or_else(|e| {
            tracing::warn!("detect_tool_preferences failed: {e:#}");
            Vec::new()
        });

        let learning = self.detect_learning_progression().unwrap_or_else(|e| {
            tracing::warn!("detect_learning_progression failed: {e:#}");
            Vec::new()
        });

        let time_patterns = self.detect_time_correlations().unwrap_or_else(|e| {
            tracing::warn!("detect_time_correlations failed: {e:#}");
            Vec::new()
        });

        Ok(SessionPatternReport {
            topics,
            tools,
            learning,
            time_patterns,
        })
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_memory_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        // Create minimal schema matching memory.db
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS events (
                id             INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp      TEXT NOT NULL,
                event_type     TEXT NOT NULL,
                action         TEXT NOT NULL,
                context        TEXT NOT NULL,
                confidence     REAL NOT NULL DEFAULT 1.0,
                source_session TEXT,
                source_text    TEXT NOT NULL DEFAULT '',
                payload        TEXT
            );

            CREATE TABLE IF NOT EXISTS kg_nodes (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                name            TEXT NOT NULL,
                entity_type     TEXT NOT NULL DEFAULT 'Concept',
                description     TEXT NOT NULL DEFAULT '',
                confidence      REAL NOT NULL DEFAULT 0.5,
                evidence_count  INTEGER NOT NULL DEFAULT 1,
                first_seen      INTEGER NOT NULL DEFAULT (strftime('%s','now')),
                last_updated    INTEGER NOT NULL DEFAULT (strftime('%s','now')),
                scope           TEXT NOT NULL DEFAULT 'global'
            );

            CREATE TABLE IF NOT EXISTS kg_edges (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                source_id       INTEGER NOT NULL,
                target_id       INTEGER NOT NULL,
                relation_type   TEXT NOT NULL DEFAULT 'related_to',
                fact            TEXT NOT NULL DEFAULT '',
                confidence      REAL NOT NULL DEFAULT 0.5,
                evidence_count  INTEGER NOT NULL DEFAULT 1,
                first_seen      INTEGER NOT NULL DEFAULT (strftime('%s','now')),
                last_updated    INTEGER NOT NULL DEFAULT (strftime('%s','now')),
                scope           TEXT NOT NULL DEFAULT 'global'
            );

            CREATE TABLE IF NOT EXISTS kg_evidence (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                node_id      INTEGER,
                edge_id      INTEGER,
                cognition_id INTEGER,
                event_id     INTEGER
            );",
        )
        .unwrap();
        conn
    }

    fn insert_event(
        conn: &Connection,
        ts: &str,
        event_type: &str,
        action: &str,
        session: &str,
        confidence: f64,
        payload: Option<&str>,
    ) -> i64 {
        conn.execute(
            "INSERT INTO events (timestamp, event_type, action, context, confidence, source_session, source_text, payload)
             VALUES (?1, ?2, ?3, '', ?4, ?5, '', ?6)",
            rusqlite::params![ts, event_type, action, confidence, session, payload],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn insert_node(
        conn: &Connection,
        name: &str,
        entity_type: &str,
        scope: &str,
        confidence: f64,
        first_seen: i64,
    ) -> i64 {
        conn.execute(
            "INSERT INTO kg_nodes (name, entity_type, confidence, first_seen, last_updated, scope)
             VALUES (?1, ?2, ?3, ?4, ?4, ?5)",
            rusqlite::params![name, entity_type, confidence, first_seen, scope],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn link_evidence(conn: &Connection, node_id: i64, event_id: i64) {
        conn.execute(
            "INSERT INTO kg_evidence (node_id, event_id) VALUES (?1, ?2)",
            rusqlite::params![node_id, event_id],
        )
        .unwrap();
    }

    // --- Topic Frequency ---

    #[test]
    fn test_topic_frequency_cross_session() {
        let conn = setup_memory_db();

        // Session A
        let e1 = insert_event(
            &conn,
            "2025-06-01T10:00:00+08:00",
            "chat_turn",
            "chat",
            "session-A",
            1.0,
            None,
        );
        let n1 = insert_node(&conn, "Rust", "Technology", "session-A", 0.9, 1717200000);
        link_evidence(&conn, n1, e1);

        // Session B — same topic
        let e2 = insert_event(
            &conn,
            "2025-06-02T14:00:00+08:00",
            "chat_turn",
            "chat",
            "session-B",
            1.0,
            None,
        );
        let n2 = insert_node(&conn, "Rust", "Technology", "session-B", 0.85, 1717300000);
        link_evidence(&conn, n2, e2);

        // Session C — same topic
        let e3 = insert_event(
            &conn,
            "2025-06-03T09:00:00+08:00",
            "chat_turn",
            "chat",
            "session-C",
            0.95,
            None,
        );
        let n3 = insert_node(&conn, "Rust", "Technology", "session-C", 0.95, 1717400000);
        link_evidence(&conn, n3, e3);

        let detector = SessionPatternDetector::new(&conn);
        let topics = detector.detect_topic_frequency(2).unwrap();

        assert!(!topics.is_empty(), "Should detect Rust across sessions");
        let rust = topics.iter().find(|t| t.topic.contains("Rust")).unwrap();
        assert_eq!(rust.session_count, 3);
    }

    #[test]
    fn test_topic_frequency_empty_db() {
        let conn = setup_memory_db();
        let detector = SessionPatternDetector::new(&conn);
        let topics = detector.detect_topic_frequency(2).unwrap();
        assert!(topics.is_empty());
    }

    #[test]
    fn test_topic_frequency_below_threshold() {
        let conn = setup_memory_db();

        let e1 = insert_event(
            &conn,
            "2025-06-01T10:00:00+08:00",
            "chat_turn",
            "chat",
            "session-A",
            1.0,
            None,
        );
        let n1 = insert_node(&conn, "Python", "Technology", "session-A", 0.9, 1717200000);
        link_evidence(&conn, n1, e1);

        let detector = SessionPatternDetector::new(&conn);
        let topics = detector.detect_topic_frequency(2).unwrap();
        assert!(topics.is_empty(), "Should not appear with min_sessions=2");
    }

    // --- Tool Preferences ---

    #[test]
    fn test_tool_preferences_from_payload() {
        let conn = setup_memory_db();

        let payload = serde_json::json!({
            "assistant_response": serde_json::to_string(&serde_json::json!([
                {"type": "text", "text": "Let me check the file."},
                {"type": "tool_use", "id": "toolu_01", "name": "Read", "input": {"file_path": "/tmp/x"}},
                {"type": "text", "text": "Now let me run a command."},
                {"type": "tool_use", "id": "toolu_02", "name": "Bash", "input": {"command": "ls"}}
            ])).unwrap(),
            "tool_calls": 2
        });

        insert_event(
            &conn,
            "2025-06-01T10:00:00+08:00",
            "chat_turn",
            "chat",
            "s1",
            0.9,
            Some(&payload.to_string()),
        );
        insert_event(
            &conn,
            "2025-06-01T11:00:00+08:00",
            "chat_turn",
            "chat",
            "s1",
            0.8,
            Some(&payload.to_string()),
        );

        let detector = SessionPatternDetector::new(&conn);
        let tools = detector.detect_tool_preferences().unwrap();

        assert!(!tools.is_empty(), "Should detect tool preferences");
        let read = tools.iter().find(|t| t.tool == "Read").unwrap();
        assert_eq!(read.usage_count, 2);

        let bash = tools.iter().find(|t| t.tool == "Bash").unwrap();
        assert_eq!(bash.usage_count, 2);
    }

    #[test]
    fn test_tool_preferences_empty_db() {
        let conn = setup_memory_db();
        let detector = SessionPatternDetector::new(&conn);
        let tools = detector.detect_tool_preferences().unwrap();
        assert!(tools.is_empty());
    }

    #[test]
    fn test_tool_preferences_unknown_tool() {
        let conn = setup_memory_db();

        // Payload with tool_calls > 0 but no tool_use blocks in assistant_response
        let payload = serde_json::json!({
            "assistant_response": serde_json::to_string(&serde_json::json!([
                {"type": "text", "text": "Done."}
            ])).unwrap(),
            "tool_calls": 3
        });

        insert_event(
            &conn,
            "2025-06-01T10:00:00+08:00",
            "chat_turn",
            "chat",
            "s1",
            1.0,
            Some(&payload.to_string()),
        );

        let detector = SessionPatternDetector::new(&conn);
        let tools = detector.detect_tool_preferences().unwrap();

        let unknown = tools.iter().find(|t| t.tool == "<unknown tool>");
        assert!(unknown.is_some(), "Should fall back to <unknown tool>");
        assert_eq!(unknown.unwrap().usage_count, 3);
    }

    // --- Learning Progression ---

    #[test]
    fn test_learning_progression_containers() {
        let conn = setup_memory_db();

        insert_node(&conn, "Docker basics", "Technology", "global", 0.8, 100);
        insert_node(&conn, "Docker Compose", "Tool", "global", 0.8, 200);
        insert_node(&conn, "Kubernetes", "Platform", "global", 0.9, 300);
        insert_node(&conn, "Helm charts", "Tool", "global", 0.7, 400);

        let detector = SessionPatternDetector::new(&conn);
        let paths = detector.detect_learning_progression().unwrap();

        let containers = paths.iter().find(|p| p.domain == "containers");
        assert!(containers.is_some(), "Should find containers learning path");
        let cp = containers.unwrap();
        assert!(
            cp.stages.len() >= 4,
            "Should have 4 stages, got {}",
            cp.stages.len()
        );
        // Verify chronological order
        assert_eq!(cp.stages[0], "Docker basics");
        assert_eq!(cp.stages[cp.stages.len() - 1], "Helm charts");
    }

    #[test]
    fn test_learning_progression_single_entity() {
        let conn = setup_memory_db();

        insert_node(&conn, "Rust", "Language", "global", 0.9, 100);

        let detector = SessionPatternDetector::new(&conn);
        let paths = detector.detect_learning_progression().unwrap();

        // "Rust" matches the "systems" domain, but single entity means no path
        let systems = paths.iter().find(|p| p.domain == "systems");
        assert!(
            systems.is_none(),
            "Single entity should not create a learning path"
        );
    }

    #[test]
    fn test_learning_progression_empty_db() {
        let conn = setup_memory_db();
        let detector = SessionPatternDetector::new(&conn);
        let paths = detector.detect_learning_progression().unwrap();
        assert!(paths.is_empty());
    }

    // --- Time Correlations ---

    #[test]
    fn test_time_correlations() {
        let conn = setup_memory_db();

        // Morning chats
        for _ in 0..8 {
            insert_event(
                &conn,
                "2025-06-01T09:30:00+08:00",
                "chat_turn",
                "chat",
                "s1",
                1.0,
                None,
            );
        }
        // Afternoon chats
        for _ in 0..3 {
            insert_event(
                &conn,
                "2025-06-01T14:30:00+08:00",
                "chat_turn",
                "chat",
                "s1",
                1.0,
                None,
            );
        }
        // Evening chats
        for _ in 0..2 {
            insert_event(
                &conn,
                "2025-06-01T20:00:00+08:00",
                "chat_turn",
                "chat",
                "s1",
                1.0,
                None,
            );
        }

        let detector = SessionPatternDetector::new(&conn);
        let patterns = detector.detect_time_correlations().unwrap();

        assert!(!patterns.is_empty(), "Should detect time patterns");
        let top = &patterns[0];
        assert_eq!(top.hour_bucket, 9, "Most active hour should be 9");
        assert_eq!(top.frequency, 8);
    }

    #[test]
    fn test_time_correlations_empty_db() {
        let conn = setup_memory_db();
        let detector = SessionPatternDetector::new(&conn);
        let patterns = detector.detect_time_correlations().unwrap();
        assert!(patterns.is_empty());
    }

    // --- Combined Report ---

    #[test]
    fn test_detect_all() {
        let conn = setup_memory_db();

        let payload = serde_json::json!({
            "assistant_response": serde_json::to_string(&serde_json::json!([
                {"type": "tool_use", "id": "t1", "name": "Bash", "input": {}}
            ])).unwrap(),
            "tool_calls": 1
        });

        let e1 = insert_event(
            &conn,
            "2025-06-01T10:00:00+08:00",
            "chat_turn",
            "chat",
            "s1",
            0.9,
            Some(&payload.to_string()),
        );
        let n1 = insert_node(&conn, "Rust", "Technology", "s1", 0.9, 1717200000);
        link_evidence(&conn, n1, e1);

        // Second session
        let payload2 = serde_json::json!({
            "assistant_response": serde_json::to_string(&serde_json::json!([
                {"type": "tool_use", "id": "t2", "name": "Bash", "input": {}}
            ])).unwrap(),
            "tool_calls": 1
        });
        let e2 = insert_event(
            &conn,
            "2025-06-02T11:00:00+08:00",
            "chat_turn",
            "chat",
            "s2",
            0.85,
            Some(&payload2.to_string()),
        );
        let n2 = insert_node(&conn, "Rust", "Technology", "s2", 0.85, 1717300000);
        link_evidence(&conn, n2, e2);

        insert_node(&conn, "Docker", "Technology", "global", 0.8, 100);
        insert_node(&conn, "Kubernetes", "Platform", "global", 0.9, 200);

        let detector = SessionPatternDetector::new(&conn);
        let report = detector.detect_all().unwrap();

        assert!(report.topics.iter().any(|t| t.topic.contains("Rust")));
        assert!(report.tools.iter().any(|t| t.tool == "Bash"));
        assert!(!report.time_patterns.is_empty());
    }
}
