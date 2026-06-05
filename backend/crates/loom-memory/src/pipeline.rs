// SPDX-License-Identifier: Apache-2.0
//! Autonomous memory pipeline scheduler — extraction, consolidation,
//! generalization, active forgetting, and self-evaluation stages.
//!
//! Companion to the existing `MemoryConsolidator`.  While the consolidator
//! handles L0-L3 layer promotion/demotion/prune and deduplication, this
//! module adds:
//! * **Generalization** (daily) — detect entity co-mention clusters across
//!   sessions and create higher-level concept nodes.
//! * **Active forgetting** (weekly) — score and prune low-value entities
//!   while preserving high-evidence and Person entities.
//! * **Timing** — tracks when each stage was last executed so callers can
//!   check `should_run` before dispatching work.

use anyhow::Result;
use chrono::Utc;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

// ============================================================================
// Pipeline stages
// ============================================================================

/// Pipeline stage with its trigger mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineStage {
    /// Entity extraction — event-driven, every conversation turn.
    Extraction,
    /// Full consolidation (promote/demote/prune/dedup) — every 50 extractions.
    Consolidation,
    /// Generalization (higher-level concepts from entity clusters) — daily.
    Generalization,
    /// Active forgetting (prune low-value stale entities) — weekly.
    ActiveForgetting,
    /// Self-evaluation (quality scoring) — continuous, incremental per turn.
    SelfEvaluation,
}

impl PipelineStage {
    /// Human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Extraction => "extraction",
            Self::Consolidation => "consolidation",
            Self::Generalization => "generalization",
            Self::ActiveForgetting => "active_forgetting",
            Self::SelfEvaluation => "self_evaluation",
        }
    }
}

impl std::fmt::Display for PipelineStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

// ============================================================================
// Scheduler
// ============================================================================

/// Tracks when each pipeline stage was last executed and drives scheduling
/// decisions (extraction count, wall-clock intervals).
pub struct PipelineScheduler {
    last_consolidation: Instant,
    last_generalization: Instant,
    last_forgetting: Instant,
    extraction_count: u64,
}

impl Default for PipelineScheduler {
    fn default() -> Self {
        Self {
            last_consolidation: Instant::now(),
            last_generalization: Instant::now(),
            last_forgetting: Instant::now(),
            extraction_count: 0,
        }
    }
}

impl PipelineScheduler {
    /// Create a new scheduler with all timers initialised to now.
    pub fn new() -> Self {
        Self::default()
    }

    /// Check whether a pipeline stage is due.
    ///
    /// * `Extraction` and `SelfEvaluation` are **always** due (caller decides
    ///   whether to actually run them).
    /// * `Consolidation` is due every 50 extractions.
    /// * `Generalization` is due when 24 h have passed since the last run.
    /// * `ActiveForgetting` is due when 7 d have passed since the last run.
    pub fn should_run(&self, stage: PipelineStage) -> bool {
        match stage {
            PipelineStage::Extraction | PipelineStage::SelfEvaluation => true,
            PipelineStage::Consolidation => {
                self.extraction_count > 0 && self.extraction_count.is_multiple_of(50)
            }
            PipelineStage::Generalization => {
                self.last_generalization.elapsed() >= GENERALIZATION_INTERVAL
            }
            PipelineStage::ActiveForgetting => {
                self.last_forgetting.elapsed() >= FORGETTING_INTERVAL
            }
        }
    }

    /// Increment the extraction counter.  Call once per turn.
    pub fn record_extraction(&mut self) {
        self.extraction_count = self.extraction_count.wrapping_add(1);
    }

    /// Record that a stage has just completed so its timer is reset.
    pub fn record_run(&mut self, stage: PipelineStage) {
        match stage {
            PipelineStage::Consolidation => self.last_consolidation = Instant::now(),
            PipelineStage::Generalization => self.last_generalization = Instant::now(),
            PipelineStage::ActiveForgetting => self.last_forgetting = Instant::now(),
            // Extraction and SelfEvaluation are continuous — no timer to reset.
            PipelineStage::Extraction | PipelineStage::SelfEvaluation => {}
        }
    }

    /// Return the configured schedule as (stage, interval) pairs.
    pub fn get_schedule() -> Vec<(PipelineStage, Duration)> {
        vec![
            (PipelineStage::Extraction, Duration::ZERO), // every turn
            (
                PipelineStage::Consolidation,
                GENERALIZATION_INTERVAL.div_f64(24.0),
            ), // rough ~2h
            (PipelineStage::Generalization, GENERALIZATION_INTERVAL),
            (PipelineStage::ActiveForgetting, FORGETTING_INTERVAL),
            (PipelineStage::SelfEvaluation, Duration::ZERO), // continuous
        ]
    }

    /// Current extraction count.
    pub fn extraction_count(&self) -> u64 {
        self.extraction_count
    }
}

// ============================================================================
// Intervals
// ============================================================================

const GENERALIZATION_INTERVAL: Duration = Duration::from_secs(86_400); // 24 h
const FORGETTING_INTERVAL: Duration = Duration::from_secs(604_800); // 7 d

// ============================================================================
// Generalization — concept clusters
// ============================================================================

/// A detected co-mention cluster: a set of entity names that appear together
/// across multiple sessions, suggesting a shared higher-level concept.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConceptCluster {
    /// Suggested higher-level concept name.
    pub concept: String,
    /// Human-readable description of the cluster.
    pub description: String,
    /// Member entity names that form this cluster.
    pub members: Vec<String>,
    /// Number of distinct scopes (sessions) in which the members co-appear.
    pub scope_count: u32,
    /// Average confidence of the relationships linking the members.
    pub avg_confidence: f64,
}

/// A pruning entry logged during active forgetting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PruningEntry {
    pub name: String,
    pub entity_type: String,
    pub confidence: f64,
    pub evidence_count: i64,
    pub score: f64,
    pub reason: String,
    pub pruned_at: i64,
}

/// Result of a generalization run.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GeneralizationReport {
    /// Concept clusters detected.
    pub clusters: Vec<ConceptCluster>,
    /// Concept nodes created or updated in kg_nodes.
    pub created_concepts: usize,
    /// Summary for logging.
    pub summary: String,
}

/// Result of an active forgetting run.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ForgettingReport {
    /// Number of nodes pruned.
    pub pruned: usize,
    /// Number of nodes scored (examined).
    pub scored: usize,
    /// Audit log of what was pruned.
    pub audit: Vec<PruningEntry>,
    /// Summary for logging.
    pub summary: String,
}

// ============================================================================
// Concept mapping — known technology clusters
// ============================================================================

/// Static mapping: when 2+ members of a known cluster co-occur across
/// sessions, we infer the higher-level concept.
type ConceptMapping = (&'static [&'static str], &'static str, &'static str);

const CONCEPT_CLUSTERS: &[ConceptMapping] = &[
    (
        &[
            "docker",
            "kubernetes",
            "k8s",
            "terraform",
            "helm",
            "containerd",
        ],
        "CloudNative",
        "Cloud-native infrastructure, container orchestration, and IaC",
    ),
    (
        &["react", "vue", "angular", "svelte", "next.js", "nuxt"],
        "FrontendFrameworks",
        "Modern frontend web frameworks and meta-frameworks",
    ),
    (
        &["rust", "cargo", "wasm", "tokio", "serde"],
        "RustEcosystem",
        "Rust programming language and its core ecosystem",
    ),
    (
        &[
            "python",
            "pytorch",
            "tensorflow",
            "pandas",
            "numpy",
            "scikit-learn",
        ],
        "PythonML",
        "Python-based machine learning and data science stack",
    ),
    (
        &["golang", "go", "grpc", "protobuf", "gin"],
        "GoMicroservices",
        "Go-based microservice architecture and tooling",
    ),
    (
        &["typescript", "node.js", "nodejs", "npm", "deno", "bun"],
        "NodeEcosystem",
        "JavaScript/TypeScript server-side runtime ecosystem",
    ),
    (
        &[
            "postgresql",
            "postgres",
            "redis",
            "mongodb",
            "mysql",
            "sqlite",
        ],
        "DataInfrastructure",
        "Data storage, caching, and database infrastructure",
    ),
    (
        &["nginx", "haproxy", "envoy", "traefik", "caddy"],
        "EdgeGateway",
        "Reverse proxy, load balancing, and API gateway",
    ),
    (
        &["ci/cd", "jenkins", "github actions", "gitlab ci", "argo"],
        "DevOpsPipeline",
        "Continuous integration and delivery automation",
    ),
    (
        &["aws", "azure", "gcp", "alicloud"],
        "CloudProviders",
        "Public cloud infrastructure providers",
    ),
    (
        &["vim", "neovim", "vscode", "jetbrains", "intellij"],
        "DeveloperTools",
        "Code editors and integrated development environments",
    ),
    (
        &["linux", "ubuntu", "debian", "fedora", "arch", "nixos"],
        "LinuxDistributions",
        "Linux operating system distributions",
    ),
    (
        &[
            "llm",
            "gpt",
            "claude",
            "llama",
            "mistral",
            "openai",
            "anthropic",
        ],
        "LargeLanguageModels",
        "Large language models and AI platforms",
    ),
];

/// Find concept clusters by cross-referencing the knowledge graph against
/// the static concept mapping.  Returns clusters whose members co-appear
/// across multiple sessions (scopes).
pub fn detect_concept_clusters(conn: &Connection) -> Result<GeneralizationReport> {
    let mut report = GeneralizationReport::default();
    let now = Utc::now().timestamp();

    for (members, concept, description) in CONCEPT_CLUSTERS {
        let n = members.len();

        // Build placeholder strings with correct numbering:
        // source: ?1..?N, target: ?(N+1)..?(2N)
        let source_ph: Vec<String> = (0..n).map(|i| format!("?{}", i + 1)).collect();
        let target_ph: Vec<String> = (0..n).map(|i| format!("?{}", n + i + 1)).collect();

        // Count distinct scopes in which these members appear connected by edges
        let scope_sql = format!(
            "SELECT COUNT(DISTINCT e.scope)
             FROM kg_edges e
             JOIN kg_nodes sn ON sn.id = e.source_id
             JOIN kg_nodes tn ON tn.id = e.target_id
             WHERE sn.name IN ({}) AND tn.name IN ({})",
            source_ph.join(","),
            target_ph.join(",")
        );

        let mut stmt = conn.prepare(&scope_sql)?;
        // Params: source names then target names (each member appears twice)
        let scope_params: Vec<&dyn rusqlite::types::ToSql> = members
            .iter()
            .chain(members.iter())
            .map(|m| m as &dyn rusqlite::types::ToSql)
            .collect();
        let scope_count: u32 = stmt.query_row(scope_params.as_slice(), |row| row.get(0))?;

        // Only create a concept when members appear across >= 2 scopes
        if scope_count < 2 {
            continue;
        }

        // Get member names actually present in the graph
        let mem_sql = format!(
            "SELECT name FROM kg_nodes WHERE name IN ({})",
            source_ph.join(",")
        );
        let mut mem_stmt = conn.prepare(&mem_sql)?;
        let mem_params: Vec<&dyn rusqlite::types::ToSql> = members
            .iter()
            .map(|m| m as &dyn rusqlite::types::ToSql)
            .collect();
        let present_members: Vec<String> = mem_stmt
            .query_map(mem_params.as_slice(), |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        if present_members.len() < 2 {
            continue;
        }

        // Calculate average confidence of edges connecting these members
        let conf_sql = format!(
            "SELECT COALESCE(AVG(e.confidence), 0.0)
             FROM kg_edges e
             JOIN kg_nodes sn ON sn.id = e.source_id
             JOIN kg_nodes tn ON tn.id = e.target_id
             WHERE sn.name IN ({}) AND tn.name IN ({})",
            source_ph.join(","),
            target_ph.join(",")
        );
        let mut conf_stmt = conn.prepare(&conf_sql)?;
        let conf_params: Vec<&dyn rusqlite::types::ToSql> = members
            .iter()
            .chain(members.iter())
            .map(|m| m as &dyn rusqlite::types::ToSql)
            .collect();
        let avg_confidence: f64 = conf_stmt.query_row(conf_params.as_slice(), |row| row.get(0))?;

        let cluster = ConceptCluster {
            concept: concept.to_string(),
            description: description.to_string(),
            members: present_members.clone(),
            scope_count,
            avg_confidence,
        };

        // Upsert the concept node with high confidence
        conn.execute(
            "INSERT INTO kg_nodes (name, entity_type, description, confidence,
                                   evidence_count, first_seen, last_updated, scope, layer)
             VALUES (?1, 'Concept', ?2, ?3, ?4, ?5, ?5, 'global', 'semantic')
             ON CONFLICT DO NOTHING",
            params![
                concept,
                description,
                avg_confidence.max(0.7),
                present_members.len() as i64,
                now
            ],
        )?;

        // Update the concept confidence if it already exists
        conn.execute(
            "UPDATE kg_nodes SET confidence = MAX(confidence, ?1),
                                 evidence_count = evidence_count + ?2,
                                 last_updated = ?3
             WHERE name = ?4",
            params![
                avg_confidence.max(0.7),
                present_members.len() as i64,
                now,
                concept
            ],
        )?;

        report.created_concepts += 1;
        report.clusters.push(cluster);
    }

    report.summary = format!(
        "generalization: {} concept cluster(s) detected, {} concept node(s) created/updated",
        report.clusters.len(),
        report.created_concepts
    );

    Ok(report)
}

// ============================================================================
// Active forgetting
// ============================================================================

/// Scoring constants for active forgetting.
const FORGET_SCORE_THRESHOLD: f64 = 0.5; // entities below this are candidates
const FORGET_RETENTION_DAYS: i64 = 60; // must be older than this
const FORGET_MAX_PRUNE: usize = 50; // cap per run to avoid large deletions

/// Score an entity for pruning consideration.
///
/// `score = confidence * evidence_count * recency_factor`
///
/// * `last_accessed` — epoch seconds of last access; older → lower factor.
/// * Returns a score in [0.0, ∞).  Lower = more prune-worthy.
fn score_entity(confidence: f64, evidence_count: i64, last_accessed: i64, now: i64) -> f64 {
    let days_since_access = ((now - last_accessed) as f64 / 86400.0).max(0.0);
    // Recency factor: 1.0 if accessed today, decays to ~0.1 after 30 days
    let recency_factor = 1.0 / (1.0 + days_since_access * 0.1);
    confidence * (evidence_count as f64) * recency_factor
}

/// Run active forgetting: score low-value entities and prune those below
/// the threshold, respecting safety exclusions.
///
/// Safety exclusions (never pruned):
/// - `entity_type = 'Person'`
/// - `evidence_count >= 10`
/// - `scope = 'global'` AND `layer in ('semantic', 'global')`
/// - `layer = 'global'`
pub fn run_active_forgetting(conn: &Connection) -> Result<ForgettingReport> {
    let mut report = ForgettingReport::default();
    let now = Utc::now().timestamp();
    let cutoff = now - FORGET_RETENTION_DAYS * 86400;

    // Query candidates: exclude protected entities
    let candidates: Vec<(i64, String, String, f64, i64, i64)> = {
        let mut stmt = conn.prepare(
            "SELECT id, name, entity_type, confidence, evidence_count, last_accessed
             FROM kg_nodes
             WHERE entity_type != 'Person'
               AND evidence_count < 10
               AND NOT (scope = 'global' AND layer IN ('semantic', 'global'))
               AND layer != 'global'
               AND last_updated < ?1",
        )?;
        let rows = stmt.query_map(params![cutoff], |row| {
            Ok((
                row.get(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
            ))
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()?
    };

    report.scored = candidates.len();

    // Score and sort: lowest score first (most prune-worthy)
    let mut scored: Vec<_> = candidates
        .iter()
        .map(|(id, name, etype, conf, ev, la)| {
            let score = score_entity(*conf, *ev, *la, now);
            (*id, name.clone(), etype.clone(), *conf, *ev, score)
        })
        .collect();

    scored.sort_by(|a, b| a.5.partial_cmp(&b.5).unwrap_or(std::cmp::Ordering::Equal));

    // Prune the worst offenders, capped
    let mut pruned_ids = Vec::new();
    for (id, name, etype, conf, ev, score) in scored.iter().take(FORGET_MAX_PRUNE) {
        if *score >= FORGET_SCORE_THRESHOLD {
            break; // remaining are above threshold
        }

        pruned_ids.push(*id);
        report.audit.push(PruningEntry {
            name: name.clone(),
            entity_type: etype.clone(),
            confidence: *conf,
            evidence_count: *ev,
            score: *score,
            reason: format!(
                "score={:.4} < threshold={}, retention_days > {}",
                score, FORGET_SCORE_THRESHOLD, FORGET_RETENTION_DAYS
            ),
            pruned_at: now,
        });
    }

    if !pruned_ids.is_empty() {
        conn.execute_batch("BEGIN")?;

        // Delete the pruned nodes
        let ph: Vec<String> = pruned_ids
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 1))
            .collect();
        let del_sql = format!("DELETE FROM kg_nodes WHERE id IN ({})", ph.join(","));
        conn.execute(
            &del_sql,
            rusqlite::params_from_iter(pruned_ids.iter().copied()),
        )?;

        report.pruned = pruned_ids.len();

        // Cascade: clean orphaned edges, aliases, evidence
        conn.execute_batch(
            "DELETE FROM kg_edges WHERE source_id NOT IN (SELECT id FROM kg_nodes)
                OR target_id NOT IN (SELECT id FROM kg_nodes);
             DELETE FROM kg_aliases WHERE node_id NOT IN (SELECT id FROM kg_nodes);
             DELETE FROM kg_evidence WHERE node_id NOT IN (SELECT id FROM kg_nodes)
                AND edge_id NOT IN (SELECT id FROM kg_edges);",
        )?;

        conn.execute_batch("COMMIT")?;
    }

    report.summary = format!(
        "active_forgetting: scored={}, pruned={}, audit_entries={}",
        report.scored,
        report.pruned,
        report.audit.len()
    );

    Ok(report)
}

// ============================================================================
// Self-evaluation helpers
// ============================================================================

/// Compute a simple quality score for entities in a session.
/// This is a lightweight continuous evaluation that can be called
/// incrementally each turn.
///
/// Score = avg(confidence) * (count of global-scope entities) / (total entities + 1)
pub fn evaluate_session_quality(conn: &Connection, session_id: &str) -> Result<f64> {
    let total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM kg_nodes WHERE scope = ?1",
        params![session_id],
        |row| row.get(0),
    )?;

    if total == 0 {
        return Ok(0.0);
    }

    let avg_conf: f64 = conn.query_row(
        "SELECT COALESCE(AVG(confidence), 0.0) FROM kg_nodes WHERE scope = ?1",
        params![session_id],
        |row| row.get(0),
    )?;

    let global_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM kg_nodes WHERE scope = 'global'",
        [],
        |row| row.get(0),
    )?;

    let quality = avg_conf * (global_count as f64) / (total as f64 + 1.0);
    Ok(quality)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS kg_nodes (
                id             INTEGER PRIMARY KEY AUTOINCREMENT,
                name           TEXT NOT NULL UNIQUE,
                entity_type    TEXT NOT NULL DEFAULT 'Concept',
                description    TEXT NOT NULL DEFAULT '',
                confidence     REAL NOT NULL DEFAULT 0.5,
                evidence_count INTEGER NOT NULL DEFAULT 1,
                first_seen     INTEGER NOT NULL DEFAULT 0,
                last_updated   INTEGER NOT NULL DEFAULT 0,
                scope          TEXT NOT NULL DEFAULT 'global',
                access_count   INTEGER NOT NULL DEFAULT 0,
                last_accessed  INTEGER NOT NULL DEFAULT 0,
                layer          TEXT NOT NULL DEFAULT 'semantic'
            );

            CREATE TABLE IF NOT EXISTS kg_edges (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                source_id       INTEGER NOT NULL,
                target_id       INTEGER NOT NULL,
                relation_type   TEXT NOT NULL DEFAULT 'related_to',
                fact            TEXT NOT NULL DEFAULT '',
                confidence      REAL NOT NULL DEFAULT 0.5,
                evidence_count  INTEGER NOT NULL DEFAULT 1,
                first_seen      INTEGER NOT NULL DEFAULT 0,
                last_updated    INTEGER NOT NULL DEFAULT 0,
                scope           TEXT NOT NULL DEFAULT 'global'
            );

            CREATE TABLE IF NOT EXISTS kg_aliases (
                node_id INTEGER NOT NULL,
                alias   TEXT NOT NULL,
                PRIMARY KEY (node_id, alias)
            );

            CREATE TABLE IF NOT EXISTS kg_evidence (
                node_id     INTEGER,
                edge_id     INTEGER,
                cognition_id INTEGER,
                event_id    INTEGER
            );",
        )
        .unwrap();
        conn
    }

    fn insert_node(
        conn: &Connection,
        name: &str,
        entity_type: &str,
        scope: &str,
        confidence: f64,
        evidence: i64,
        layer: &str,
    ) -> i64 {
        let now = Utc::now().timestamp();
        conn.execute(
            "INSERT OR IGNORE INTO kg_nodes (name, entity_type, description, confidence, evidence_count, first_seen, last_updated, scope, last_accessed, layer)
             VALUES (?1, ?2, '', ?3, ?4, ?5, ?5, ?6, ?5, ?7)",
            params![name, entity_type, confidence, evidence, now, scope, layer],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn insert_edge(
        conn: &Connection,
        source_id: i64,
        target_id: i64,
        relation: &str,
        scope: &str,
        confidence: f64,
    ) {
        let now = Utc::now().timestamp();
        conn.execute(
            "INSERT INTO kg_edges (source_id, target_id, relation_type, confidence, first_seen, last_updated, scope)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6)",
            params![source_id, target_id, relation, confidence, now, scope],
        )
        .unwrap();
    }

    // -----------------------------------------------------------------------
    // PipelineScheduler
    // -----------------------------------------------------------------------

    #[test]
    fn test_scheduler_new_defaults() {
        let s = PipelineScheduler::new();
        assert_eq!(s.extraction_count(), 0);
        // Extraction and SelfEvaluation are continuous — always due
        assert!(s.should_run(PipelineStage::Extraction));
        assert!(s.should_run(PipelineStage::SelfEvaluation));
        // Consolidation not due until extractions accumulate to 50
        assert!(!s.should_run(PipelineStage::Consolidation));
        // Generalization and forgetting are wall-clock gated; not due
        // immediately after construction (elapsed < 24h / 7d).
        assert!(!s.should_run(PipelineStage::Generalization));
        assert!(!s.should_run(PipelineStage::ActiveForgetting));
    }

    #[test]
    fn test_scheduler_consolidation_trigger() {
        let mut s = PipelineScheduler::new();
        for _ in 0..49 {
            s.record_extraction();
        }
        assert!(!s.should_run(PipelineStage::Consolidation));
        s.record_extraction(); // 50
        assert!(s.should_run(PipelineStage::Consolidation));
        s.record_run(PipelineStage::Consolidation);
        assert_eq!(s.extraction_count(), 50);
    }

    #[test]
    fn test_scheduler_record_run_resets_timers() {
        let mut s = PipelineScheduler::new();
        // After recording a run, generalization should no longer be due
        // (elapsed time is nearly zero)
        s.record_run(PipelineStage::Generalization);
        assert!(!s.should_run(PipelineStage::Generalization));
        s.record_run(PipelineStage::ActiveForgetting);
        assert!(!s.should_run(PipelineStage::ActiveForgetting));
    }

    #[test]
    fn test_get_schedule() {
        let schedule = PipelineScheduler::get_schedule();
        assert_eq!(schedule.len(), 5);
        assert_eq!(schedule[0].0, PipelineStage::Extraction);
        assert_eq!(schedule[1].0, PipelineStage::Consolidation);
        assert_eq!(schedule[2].0, PipelineStage::Generalization);
        assert_eq!(schedule[3].0, PipelineStage::ActiveForgetting);
        assert_eq!(schedule[4].0, PipelineStage::SelfEvaluation);
    }

    // -----------------------------------------------------------------------
    // Generalization — concept cluster detection
    // -----------------------------------------------------------------------

    #[test]
    fn test_detect_concept_clusters_basic() {
        let conn = setup_db();

        // Insert entities that should cluster as CloudNative
        let docker_id = insert_node(
            &conn,
            "docker",
            "Technology",
            "session_a",
            0.8,
            3,
            "episodic",
        );
        let k8s_id = insert_node(
            &conn,
            "kubernetes",
            "Technology",
            "session_a",
            0.9,
            5,
            "episodic",
        );
        let tf_id = insert_node(
            &conn,
            "terraform",
            "Technology",
            "session_b",
            0.75,
            2,
            "episodic",
        );

        // Create cross-session edges to simulate co-mentions
        insert_edge(&conn, docker_id, k8s_id, "related_to", "session_a", 0.8);
        insert_edge(&conn, k8s_id, tf_id, "related_to", "session_b", 0.7);
        insert_edge(&conn, docker_id, tf_id, "related_to", "session_b", 0.6);

        let report = detect_concept_clusters(&conn).unwrap();
        // Should have found the CloudNative cluster (docker + kubernetes + terraform)
        assert!(
            !report.clusters.is_empty(),
            "should detect at least one cluster"
        );
        let cloud_native = report.clusters.iter().find(|c| c.concept == "CloudNative");
        assert!(
            cloud_native.is_some(),
            "CloudNative cluster should be detected"
        );
        let cn = cloud_native.unwrap();
        assert!(cn.members.len() >= 2);
        assert!(cn.scope_count >= 2);
    }

    #[test]
    fn test_detect_clusters_insufficient_scopes() {
        let conn = setup_db();

        let docker_id = insert_node(
            &conn,
            "docker",
            "Technology",
            "session_a",
            0.8,
            1,
            "episodic",
        );
        let k8s_id = insert_node(
            &conn,
            "kubernetes",
            "Technology",
            "session_a",
            0.9,
            1,
            "episodic",
        );
        // Only one scope — should not trigger cluster creation
        insert_edge(&conn, docker_id, k8s_id, "related_to", "session_a", 0.8);

        let report = detect_concept_clusters(&conn).unwrap();
        let cloud_native = report.clusters.iter().find(|c| c.concept == "CloudNative");
        assert!(
            cloud_native.is_none(),
            "cluster should not appear with only 1 scope"
        );
    }

    #[test]
    fn test_detect_clusters_empty_db() {
        let conn = setup_db();
        let report = detect_concept_clusters(&conn).unwrap();
        assert!(report.clusters.is_empty());
        assert_eq!(report.created_concepts, 0);
    }

    // -----------------------------------------------------------------------
    // Active forgetting
    // -----------------------------------------------------------------------

    #[test]
    fn test_active_forgetting_prunes_low_score() {
        let conn = setup_db();
        let old_ts = Utc::now().timestamp() - 100 * 86400; // 100 days ago

        // Low-value, old, not Person — should be pruned
        conn.execute(
            "INSERT INTO kg_nodes (name, entity_type, confidence, evidence_count,
                                   first_seen, last_updated, scope, last_accessed, layer)
             VALUES ('stale_concept', 'Concept', 0.1, 1, ?1, ?1, 'session_x', ?1, 'working')",
            params![old_ts],
        )
        .unwrap();

        // Person — should NOT be pruned
        conn.execute(
            "INSERT INTO kg_nodes (name, entity_type, confidence, evidence_count,
                                   first_seen, last_updated, scope, last_accessed, layer)
             VALUES ('John', 'Person', 0.9, 1, ?1, ?1, 'session_x', ?1, 'working')",
            params![old_ts],
        )
        .unwrap();

        // High evidence — should NOT be pruned
        conn.execute(
            "INSERT INTO kg_nodes (name, entity_type, confidence, evidence_count,
                                   first_seen, last_updated, scope, last_accessed, layer)
             VALUES ('rust', 'Technology', 0.9, 15, ?1, ?1, 'session_x', ?1, 'working')",
            params![old_ts],
        )
        .unwrap();

        // Global layer — should NOT be pruned
        conn.execute(
            "INSERT INTO kg_nodes (name, entity_type, confidence, evidence_count,
                                   first_seen, last_updated, scope, last_accessed, layer)
             VALUES ('always_present', 'Concept', 0.5, 3, ?1, ?1, 'global', ?1, 'global')",
            params![old_ts],
        )
        .unwrap();

        let report = run_active_forgetting(&conn).unwrap();
        assert_eq!(report.pruned, 1, "only stale_concept should be pruned");

        // Verify protected entities survived
        let remaining: Vec<String> = {
            let mut stmt = conn.prepare("SELECT name FROM kg_nodes").unwrap();
            stmt.query_map([], |row| row.get(0))
                .unwrap()
                .collect::<std::result::Result<Vec<_>, _>>()
                .unwrap()
        };
        assert!(remaining.contains(&"John".to_string()));
        assert!(remaining.contains(&"rust".to_string()));
        assert!(remaining.contains(&"always_present".to_string()));
        assert!(!remaining.contains(&"stale_concept".to_string()));
    }

    #[test]
    fn test_active_forgetting_all_protected() {
        let conn = setup_db();
        let now = Utc::now().timestamp();

        // All entities are protected — nothing should be pruned
        conn.execute(
            "INSERT INTO kg_nodes (name, entity_type, confidence, evidence_count,
                                   first_seen, last_updated, scope, last_accessed, layer)
             VALUES ('Alice', 'Person', 0.8, 2, ?1, ?1, 'session_a', ?1, 'episodic')",
            params![now],
        )
        .unwrap();

        let report = run_active_forgetting(&conn).unwrap();
        assert_eq!(report.pruned, 0);
        assert!(report.audit.is_empty());
    }

    #[test]
    fn test_score_entity() {
        let now = Utc::now().timestamp();
        // Recently accessed: high score
        let s1 = score_entity(0.8, 5, now, now);
        assert!(s1 > 3.0, "recent entity should score high");

        // Old with low evidence: low score
        let old_ts = now - 90 * 86400;
        let s2 = score_entity(0.2, 1, old_ts, now);
        assert!(s2 < 0.5, "stale low-confidence entity should score low");
    }

    // -----------------------------------------------------------------------
    // Self-evaluation
    // -----------------------------------------------------------------------

    #[test]
    fn test_evaluate_session_quality() {
        let conn = setup_db();
        let now = Utc::now().timestamp();

        // Session entities
        conn.execute(
            "INSERT INTO kg_nodes (name, entity_type, description, confidence, evidence_count,
                                   first_seen, last_updated, scope, last_accessed, layer)
             VALUES ('test_entity', 'Concept', '', 0.7, 3, ?1, ?1, 'session_test', ?1, 'episodic')",
            params![now],
        )
        .unwrap();

        // Global entities (for the global_count in the formula)
        conn.execute(
            "INSERT INTO kg_nodes (name, entity_type, description, confidence, evidence_count,
                                   first_seen, last_updated, scope, last_accessed, layer)
             VALUES ('global_entity', 'Concept', '', 1.0, 10, ?1, ?1, 'global', ?1, 'semantic')",
            params![now],
        )
        .unwrap();

        let quality = evaluate_session_quality(&conn, "session_test").unwrap();
        // quality = 0.7 * 1 / (1 + 1) = 0.35
        assert!(
            (quality - 0.35).abs() < 1e-9,
            "expected quality ~0.35, got {}",
            quality
        );
    }

    #[test]
    fn test_evaluate_session_quality_empty() {
        let conn = setup_db();
        let quality = evaluate_session_quality(&conn, "no_such_session").unwrap();
        assert_eq!(quality, 0.0);
    }

    // -----------------------------------------------------------------------
    // PipelineStage Display
    // -----------------------------------------------------------------------

    #[test]
    fn test_pipeline_stage_label() {
        assert_eq!(PipelineStage::Extraction.label(), "extraction");
        assert_eq!(PipelineStage::Consolidation.label(), "consolidation");
        assert_eq!(PipelineStage::Generalization.label(), "generalization");
        assert_eq!(PipelineStage::ActiveForgetting.label(), "active_forgetting");
        assert_eq!(PipelineStage::SelfEvaluation.label(), "self_evaluation");
        assert_eq!(PipelineStage::Extraction.to_string(), "extraction");
    }
}
