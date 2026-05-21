use openloom_models::PersonaProvider;
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct CognitionsPersonaProvider {
    db_path: PathBuf,
    project_scope: String,
    cache: Mutex<Option<String>>,
}

impl CognitionsPersonaProvider {
    pub fn new(db_path: PathBuf, project_scope: String) -> Self {
        Self {
            db_path,
            project_scope,
            cache: Mutex::new(None),
        }
    }
}

#[async_trait::async_trait]
impl PersonaProvider for CognitionsPersonaProvider {
    async fn summarize(&self) -> anyhow::Result<String> {
        let mut cache = self.cache.lock().unwrap();
        if let Some(ref cached) = *cache {
            return Ok(cached.clone());
        }

        let conn = Connection::open(&self.db_path)?;

        let now = chrono::Utc::now().timestamp();
        let mut stmt = conn.prepare(
            "SELECT trait, value, confidence, evidence_count, last_updated, source, scope
             FROM cognitions WHERE subject = 'USER' AND (scope = 'global' OR scope = ?1)",
        )?;

        struct ScoredRow {
            value: String,
            score: f64,
        }

        let mut rows: Vec<ScoredRow> = Vec::new();
        let query_rows = stmt.query_map(rusqlite::params![&self.project_scope], |row| {
            let trait_name: String = row.get(0)?;
            let value: String = row.get(1)?;
            let confidence: f64 = row.get(2)?;
            let evidence_count: i64 = row.get(3)?;
            let last_updated: i64 = row.get(4)?;
            let source: String = row.get(5)?;
            let scope: String = row.get(6)?;
            Ok((
                trait_name,
                value,
                confidence,
                evidence_count,
                last_updated,
                source,
                scope,
            ))
        })?;

        for row in query_rows {
            let (trait_name, value, confidence, evidence_count, last_updated, source, scope) = row?;
            let days_since = ((now - last_updated) as f64 / 86400.0).max(0.0);
            let recency_decay = (-days_since / 30.0).exp();
            let base_score = confidence * (1.0 + (evidence_count.max(0) as f64).ln());
            let weighted_score = base_score * recency_decay;
            let source_priority = if source == "observed" { 1.0 } else { 0.0 };
            let scope_bonus = if scope == self.project_scope {
                3.0
            } else {
                0.0
            };
            let final_score = weighted_score + source_priority * 5.0 + scope_bonus;

            rows.push(ScoredRow {
                value: format!("{}：{}", trait_name, value),
                score: final_score,
            });
        }

        rows.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        rows.truncate(5);

        let summary = if rows.is_empty() {
            String::new()
        } else {
            let parts: Vec<String> = rows.iter().map(|r| r.value.clone()).collect();
            format!("用户画像：{}。", parts.join("；"))
        };

        *cache = Some(summary.clone());
        Ok(summary)
    }

    fn invalidate(&self) {
        self.cache.lock().unwrap().take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn setup_cognitions_table(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS cognitions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                subject TEXT NOT NULL,
                trait TEXT NOT NULL,
                value TEXT NOT NULL,
                confidence REAL,
                evidence_count INTEGER,
                first_seen INTEGER,
                last_updated INTEGER,
                version INTEGER DEFAULT 1,
                source TEXT NOT NULL DEFAULT 'observed',
                scope TEXT NOT NULL DEFAULT 'global'
            );",
        )
        .unwrap();
    }

    #[test]
    fn test_persona_empty_returns_empty_string() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let conn = Connection::open(&db_path).unwrap();
        setup_cognitions_table(&conn);

        let provider = CognitionsPersonaProvider::new(db_path.clone(), "global".into());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let summary = rt.block_on(provider.summarize()).unwrap();
        assert!(summary.is_empty());
    }

    #[test]
    fn test_persona_with_cognitions() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let conn = Connection::open(&db_path).unwrap();
        setup_cognitions_table(&conn);
        let now = chrono::Utc::now().timestamp();

        conn.execute(
            "INSERT INTO cognitions (subject, trait, value, confidence, evidence_count, first_seen, last_updated, source)
             VALUES ('USER', 'risk_tendency', '用户存在赌徒补仓倾向', 0.91, 5, ?1, ?1, 'observed')",
            rusqlite::params![now],
        ).unwrap();
        conn.execute(
            "INSERT INTO cognitions (subject, trait, value, confidence, evidence_count, first_seen, last_updated, source)
             VALUES ('USER', 'trading_style', '用户偏好短线交易', 0.85, 3, ?1, ?1, 'observed')",
            rusqlite::params![now],
        ).unwrap();

        let provider = CognitionsPersonaProvider::new(db_path.clone(), "global".into());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let summary = rt.block_on(provider.summarize()).unwrap();
        assert!(summary.contains("risk_tendency"));
        assert!(summary.contains("trading_style"));
        assert!(summary.starts_with("用户画像："));
    }

    #[test]
    fn test_persona_cache_hit() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let conn = Connection::open(&db_path).unwrap();
        setup_cognitions_table(&conn);
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "INSERT INTO cognitions (subject, trait, value, confidence, evidence_count, first_seen, last_updated, source)
             VALUES ('USER', 'risk_tendency', '赌徒补仓', 0.91, 5, ?1, ?1, 'observed')",
            rusqlite::params![now],
        ).unwrap();

        let provider = CognitionsPersonaProvider::new(db_path.clone(), "global".into());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let s1 = rt.block_on(provider.summarize()).unwrap();
        let s2 = rt.block_on(provider.summarize()).unwrap();
        assert_eq!(s1, s2);
    }

    #[test]
    fn test_persona_invalidate() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let conn = Connection::open(&db_path).unwrap();
        setup_cognitions_table(&conn);
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "INSERT INTO cognitions (subject, trait, value, confidence, evidence_count, first_seen, last_updated, source)
             VALUES ('USER', 'risk_tendency', '赌徒补仓', 0.91, 5, ?1, ?1, 'observed')",
            rusqlite::params![now],
        ).unwrap();

        let provider = CognitionsPersonaProvider::new(db_path.clone(), "global".into());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let s1 = rt.block_on(provider.summarize()).unwrap();
        provider.invalidate();
        let s2 = rt.block_on(provider.summarize()).unwrap();
        assert_eq!(s1, s2);
    }

    #[test]
    fn test_persona_mixed_sources_observed_first() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let conn = Connection::open(&db_path).unwrap();
        setup_cognitions_table(&conn);
        let now = chrono::Utc::now().timestamp();
        // inferred (lower priority) but higher raw score
        conn.execute(
            "INSERT INTO cognitions (subject, trait, value, confidence, evidence_count, first_seen, last_updated, source)
             VALUES ('USER', 'inferred_trait', '推断特质', 0.99, 20, ?1, ?1, 'inferred')",
            rusqlite::params![now],
        ).unwrap();
        // observed (higher priority) but lower raw score
        conn.execute(
            "INSERT INTO cognitions (subject, trait, value, confidence, evidence_count, first_seen, last_updated, source)
             VALUES ('USER', 'observed_trait', '观察特质', 0.5, 2, ?1, ?1, 'observed')",
            rusqlite::params![now],
        ).unwrap();

        let provider = CognitionsPersonaProvider::new(db_path.clone(), "global".into());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let summary = rt.block_on(provider.summarize()).unwrap();
        // Both should appear (both in top 5), observed should come first
        let obs_pos = summary.find("observed_trait").unwrap();
        let inf_pos = summary.find("inferred_trait").unwrap();
        assert!(
            obs_pos < inf_pos,
            "observed trait should appear before inferred"
        );
    }

    #[test]
    fn test_persona_scope_isolation() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let conn = Connection::open(&db_path).unwrap();
        setup_cognitions_table(&conn);
        let now = chrono::Utc::now().timestamp();

        // Global cognition — visible to all projects
        conn.execute(
            "INSERT INTO cognitions (subject, trait, value, confidence, evidence_count, first_seen, last_updated, source, scope)
             VALUES ('USER', 'profession', '后端工程师', 0.9, 5, ?1, ?1, 'observed', 'global')",
            rusqlite::params![now],
        ).unwrap();
        // Project A cognition
        conn.execute(
            "INSERT INTO cognitions (subject, trait, value, confidence, evidence_count, first_seen, last_updated, source, scope)
             VALUES ('USER', 'tech_stack', '用React+TypeScript', 0.85, 3, ?1, ?1, 'observed', 'project:F:/projectA')",
            rusqlite::params![now],
        ).unwrap();
        // Project B cognition
        conn.execute(
            "INSERT INTO cognitions (subject, trait, value, confidence, evidence_count, first_seen, last_updated, source, scope)
             VALUES ('USER', 'tech_stack', '用Rust+Axum', 0.85, 3, ?1, ?1, 'observed', 'project:F:/projectB')",
            rusqlite::params![now],
        ).unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();

        // Provider for project A should see global + A, not B
        let provider_a =
            CognitionsPersonaProvider::new(db_path.clone(), "project:F:/projectA".into());
        let summary_a = rt.block_on(provider_a.summarize()).unwrap();
        assert!(
            summary_a.contains("profession"),
            "global cognition should be visible"
        );
        assert!(
            summary_a.contains("React"),
            "project A cognition should be visible"
        );
        assert!(
            !summary_a.contains("Axum"),
            "project B cognition should NOT be visible"
        );

        // Provider for project B should see global + B, not A
        let provider_b =
            CognitionsPersonaProvider::new(db_path.clone(), "project:F:/projectB".into());
        let summary_b = rt.block_on(provider_b.summarize()).unwrap();
        assert!(
            summary_b.contains("profession"),
            "global cognition should be visible"
        );
        assert!(
            summary_b.contains("Axum"),
            "project B cognition should be visible"
        );
        assert!(
            !summary_b.contains("React"),
            "project A cognition should NOT be visible"
        );
    }

    #[test]
    fn test_persona_project_scope_bonus() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let conn = Connection::open(&db_path).unwrap();
        setup_cognitions_table(&conn);
        let now = chrono::Utc::now().timestamp();

        // Global cognition with high confidence
        conn.execute(
            "INSERT INTO cognitions (subject, trait, value, confidence, evidence_count, first_seen, last_updated, source, scope)
             VALUES ('USER', 'knowledge', '了解AI', 0.95, 10, ?1, ?1, 'observed', 'global')",
            rusqlite::params![now],
        ).unwrap();
        // Project cognition with lower confidence
        conn.execute(
            "INSERT INTO cognitions (subject, trait, value, confidence, evidence_count, first_seen, last_updated, source, scope)
             VALUES ('USER', 'tech_stack', '用Vue3', 0.7, 3, ?1, ?1, 'observed', 'project:F:/myApp')",
            rusqlite::params![now],
        ).unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let provider = CognitionsPersonaProvider::new(db_path.clone(), "project:F:/myApp".into());
        let summary = rt.block_on(provider.summarize()).unwrap();
        // Both should appear — project scope gets bonus so it ranks well
        assert!(summary.contains("Vue3"));
        assert!(summary.contains("AI"));
    }
}
