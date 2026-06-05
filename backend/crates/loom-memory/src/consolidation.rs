//! Memory consolidation — periodic analysis that promotes/demotes/prunes
//! knowledge graph nodes across the L0-L3 memory layers, plus deduplication
//! and cognition merging.
//! Phase 2 structural module.

use anyhow::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::graph::GraphStore;
use crate::layers::Layer;

/// Report produced by a memory consolidation cycle.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConsolidationReport {
    // --- Layer-based fields (existing) ---
    /// Nodes promoted (e.g. episodic → semantic, semantic → global).
    pub promoted: usize,
    /// Nodes demoted (e.g. semantic → episodic).
    pub demoted: usize,
    /// Nodes pruned (removed entirely from stale episodic layer).
    pub pruned: usize,
    /// Per-layer counts after consolidation.
    pub layer_counts: Vec<(String, i64)>,
    /// Summary message for logging.
    #[serde(default)]
    pub summary: String,

    // --- Deduplication fields (Phase 2) ---
    /// Number of duplicate kg_nodes merged into survivors.
    #[serde(default)]
    pub merged_nodes: usize,
    /// Number of duplicate cognitions merged into survivors.
    #[serde(default)]
    pub merged_cognitions: usize,
    /// Number of session-scoped nodes promoted to 'episodic'.
    #[serde(default)]
    pub promoted_count: usize,
    /// Number of edges re-pointed from deleted duplicates to survivors.
    #[serde(default)]
    pub edge_rerouted: usize,
    /// Non-fatal errors encountered during a dedup consolidation cycle.
    /// A non-empty list does not mean the operation failed — partial progress
    /// is preserved and the transaction is still committed.
    #[serde(default)]
    pub errors: Vec<String>,
}

impl std::fmt::Display for ConsolidationReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "merged_nodes={} merged_cognitions={} promoted_to_episodic={} edge_rerouted={} errors={} | layer: promoted={} demoted={} pruned={}",
            self.merged_nodes,
            self.merged_cognitions,
            self.promoted_count,
            self.edge_rerouted,
            self.errors.len(),
            self.promoted,
            self.demoted,
            self.pruned,
        )
    }
}

/// Consolidation-specific tuning knobs (independent of per-layer configs).
#[derive(Debug, Clone)]
pub struct ConsolidationConfig {
    /// Min confidence for episodic → semantic promotion.
    pub promote_min_confidence: f64,
    /// Min evidence count for episodic → semantic promotion.
    pub promote_min_evidence: i64,
    /// Days of inactivity before demoting a semantic node back to episodic.
    pub demote_after_days: i64,
    /// Days of inactivity before pruning stale episodic nodes.
    pub prune_after_days: i64,
    /// Max nodes in the semantic layer before demoting overflow.
    pub semantic_cap: usize,
}

impl Default for ConsolidationConfig {
    fn default() -> Self {
        Self {
            promote_min_confidence: 0.5,
            promote_min_evidence: 3,
            demote_after_days: 60,
            prune_after_days: 90,
            semantic_cap: 500,
        }
    }
}

/// Runs consolidation cycles over the knowledge graph and cognition tables.
///
/// Two families of operations:
/// * **Layer-based** (`run_cycle`) — promotes/demotes/prunes nodes between
///   memory layers (working/episodic/semantic/global).
/// * **Dedup-based** (`consolidate_duplicates`, `consolidate_cognitions`,
///   `promote_high_confidence`, `run_consolidation_cycle`) — merges duplicate
///   nodes and cognitions and promotes session-scoped entities to the episodic
///   tier.
///
/// All methods are independently transactional.
pub struct MemoryConsolidator<'a> {
    conn: &'a Connection,
    config: ConsolidationConfig,
}

impl<'a> MemoryConsolidator<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self {
            conn,
            config: ConsolidationConfig::default(),
        }
    }

    #[allow(dead_code)]
    pub fn with_config(mut self, config: ConsolidationConfig) -> Self {
        self.config = config;
        self
    }

    // ========================================================================
    // Layer-based consolidation (existing)
    // ========================================================================

    /// Execute one L0-L3 layer consolidation cycle.
    /// Wrapped in an explicit transaction to prevent partial writes.
    pub fn run_cycle(&self) -> Result<ConsolidationReport> {
        self.conn.execute_batch("BEGIN")?;

        let graph = GraphStore::new(self.conn);
        let now = chrono::Utc::now().timestamp();

        // 1. Promote: episodic → semantic when high confidence and enough evidence.
        let promoted = self.conn.execute(
            "UPDATE kg_nodes SET layer = ?, last_updated = ?
             WHERE (layer = ? OR layer = ?)
               AND confidence >= ?
               AND evidence_count >= ?
               AND layer != ?
               AND layer != ?",
            rusqlite::params![
                Layer::Semantic.as_str(), now,
                Layer::Episodic.as_str(), Layer::Working.as_str(),
                self.config.promote_min_confidence,
                self.config.promote_min_evidence,
                Layer::Semantic.as_str(),
                Layer::Global.as_str(),
            ],
        )?;

        // 2. Promote: semantic → global for exceptional confidence.
        let promoted_to_global = self.conn.execute(
            "UPDATE kg_nodes SET layer = ?, last_updated = ?
             WHERE layer = ?
               AND confidence >= 0.9
               AND evidence_count >= 10",
            rusqlite::params![
                Layer::Global.as_str(), now,
                Layer::Semantic.as_str(),
            ],
        )?;

        // 3. Demote: semantic → episodic when stale (inactive beyond demote window).
        let cutoff_demote = now - self.config.demote_after_days * 86400;
        let demoted = self.conn.execute(
            "UPDATE kg_nodes SET layer = ?, last_updated = ?
             WHERE layer = ?
               AND last_updated < ?
               AND confidence < 0.7",
            rusqlite::params![
                Layer::Episodic.as_str(), now,
                Layer::Semantic.as_str(),
                cutoff_demote,
            ],
        )?;

        // 4. Prune: stale episodic nodes with very low confidence.
        let cutoff_prune = now - self.config.prune_after_days * 86400;
        let pruned = self.conn.execute(
            "DELETE FROM kg_nodes
             WHERE layer = ?
               AND confidence < 0.2
               AND last_updated < ?
               AND evidence_count <= 1",
            rusqlite::params![
                Layer::Episodic.as_str(),
                cutoff_prune,
            ],
        )?;

        // 4a. Cascade: clean orphaned edges, aliases, evidence after prune.
        if pruned > 0 {
            self.conn.execute_batch(
                "DELETE FROM kg_edges WHERE source_id NOT IN (SELECT id FROM kg_nodes)
                    OR target_id NOT IN (SELECT id FROM kg_nodes);
                 DELETE FROM kg_aliases WHERE node_id NOT IN (SELECT id FROM kg_nodes);
                 DELETE FROM kg_evidence WHERE node_id NOT IN (SELECT id FROM kg_nodes)
                    AND edge_id NOT IN (SELECT id FROM kg_edges);",
            )?;
        }

        // 5. Cap semantic layer.
        let semantic_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM kg_nodes WHERE layer = ?",
            rusqlite::params![Layer::Semantic.as_str()],
            |row| row.get(0),
        )?;

        let mut overflow_demoted = 0usize;
        if semantic_count > self.config.semantic_cap as i64 {
            let excess = semantic_count - self.config.semantic_cap as i64;
            overflow_demoted = self.conn.execute(
                "UPDATE kg_nodes SET layer = ?
                 WHERE id IN (
                     SELECT id FROM kg_nodes
                     WHERE layer = ?
                     ORDER BY confidence ASC, evidence_count ASC
                     LIMIT ?
                 )",
                rusqlite::params![
                    Layer::Episodic.as_str(),
                    Layer::Semantic.as_str(),
                    excess,
                ],
            )? as usize;
        }

        // 6. Collect final layer stats before commit (read inside txn is OK).
        let layer_counts = graph.get_layer_stats().unwrap_or_default();

        let total_promoted = (promoted + promoted_to_global) as usize;
        let total_demoted = (demoted + overflow_demoted as usize) as usize;

        let summary = format!(
            "Consolidation: promoted={}, demoted={}, pruned={} | layers: {:?}",
            total_promoted, total_demoted, pruned, layer_counts
        );

        self.conn.execute_batch("COMMIT")?;

        Ok(ConsolidationReport {
            promoted: total_promoted,
            demoted: total_demoted,
            pruned: pruned as usize,
            layer_counts,
            summary,
            ..Default::default()
        })
    }

    // ========================================================================
    // Phase 1 — Duplicate kg_node consolidation
    // ========================================================================

    /// Find `kg_nodes` that share the same `name` but have different `id`
    /// values, merge them into the highest-confidence survivor, reroute edges,
    /// migrate aliases, clean orphan evidence entries, and delete the
    /// duplicates.
    ///
    /// Survivor selection: highest confidence → highest evidence_count →
    /// lowest id (deterministic tiebreaker).  Evidence counts are summed into
    /// the survivor; confidence is the max of the group.
    pub fn consolidate_duplicates(&self) -> Result<ConsolidationReport> {
        // Track metrics locally; only assign to report after COMMIT succeeds
        // to avoid reporting metrics for work that was rolled back.
        let mut merged_nodes = 0usize;
        let mut edge_rerouted = 0usize;
        let mut errors: Vec<String> = Vec::new();

        // Discover duplicate groups: same name, multiple ids
        let groups = {
            let mut stmt = self.conn.prepare(
                "SELECT name, GROUP_CONCAT(id) AS ids
                 FROM kg_nodes
                 GROUP BY name
                 HAVING COUNT(*) > 1",
            )?;
            let rows: Vec<(String, String)> = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;

            if rows.is_empty() {
                return Ok(ConsolidationReport::default());
            }
            rows
        };

        self.conn.execute_batch("BEGIN")?;

        for (_name, ids_str) in &groups {
            let ids: Vec<i64> = ids_str
                .split(',')
                .filter_map(|s| s.trim().parse::<i64>().ok())
                .collect();
            if ids.len() < 2 {
                continue;
            }

            // --- Pick the survivor ---
            let survivor_id = match self.find_survivor_node(&ids) {
                Ok(id) => id,
                Err(e) => {
                    errors.push(format!("find_survivor for ids {:?}: {}", ids, e));
                    continue;
                }
            };

            let to_delete: Vec<i64> =
                ids.iter().copied().filter(|id| *id != survivor_id).collect();
            if to_delete.is_empty() {
                continue;
            }

            // --- Merge stats into survivor ---
            if let Err(e) = self.merge_node_stats(survivor_id, &to_delete) {
                errors.push(format!("merge_node_stats survivor={}: {}", survivor_id, e));
                continue;
            }

            // --- Reroute edges ---
            for old_id in &to_delete {
                match self.reroute_edges(*old_id, survivor_id) {
                    Ok(rerouted) => edge_rerouted += rerouted,
                    Err(e) => errors.push(format!(
                        "reroute_edges {} -> {}: {}",
                        old_id, survivor_id, e
                    )),
                }
            }

            // --- Migrate aliases ---
            for old_id in &to_delete {
                if let Err(e) = self.migrate_aliases(*old_id, survivor_id) {
                    errors.push(format!(
                        "migrate_aliases {} -> {}: {}",
                        old_id, survivor_id, e
                    ));
                }
            }

            // --- Clean evidence rows pointing to dupes ---
            let ph: Vec<String> =
                to_delete.iter().enumerate().map(|(i, _)| format!("?{}", i + 1)).collect();
            let ev_sql = format!(
                "DELETE FROM kg_evidence WHERE node_id IN ({})",
                ph.join(",")
            );
            if let Err(e) = self
                .conn
                .execute(&ev_sql, rusqlite::params_from_iter(&to_delete))
            {
                errors.push(format!("clean_evidence {:?}: {}", to_delete, e));
            }

            // --- Delete the duplicates (FK cascade handles remaining refs) ---
            for old_id in &to_delete {
                if let Err(e) = self
                    .conn
                    .execute("DELETE FROM kg_nodes WHERE id = ?1", params![old_id])
                {
                    errors.push(format!("delete duplicate node {}: {}", old_id, e));
                }
            }

            merged_nodes += to_delete.len();
        }

        // Commit even when there are non-fatal errors — partial progress is
        // better than rolling everything back.
        self.conn.execute_batch("COMMIT")?;

        // Only build the report after COMMIT succeeds, so metrics match
        // the database state that was actually persisted.
        Ok(ConsolidationReport {
            merged_nodes,
            edge_rerouted,
            errors,
            ..Default::default()
        })
    }

    /// Pick the best node from a group of duplicate ids.
    fn find_survivor_node(&self, ids: &[i64]) -> Result<i64> {
        let ph: Vec<String> =
            ids.iter().enumerate().map(|(i, _)| format!("?{}", i + 1)).collect();
        let sql = format!(
            "SELECT id FROM kg_nodes WHERE id IN ({})
             ORDER BY confidence DESC, evidence_count DESC, id ASC LIMIT 1",
            ph.join(",")
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let sparams: Vec<&dyn rusqlite::types::ToSql> =
            ids.iter().map(|id| id as &dyn rusqlite::types::ToSql).collect();
        Ok(stmt.query_row(sparams.as_slice(), |row| row.get(0))?)
    }

    /// Sum evidence_count and take MAX confidence from the duplicate nodes
    /// into the survivor.
    fn merge_node_stats(&self, survivor_id: i64, old_ids: &[i64]) -> Result<()> {
        let ph: Vec<String> =
            old_ids.iter().enumerate().map(|(i, _)| format!("?{}", i + 1)).collect();
        let sql = format!(
            "SELECT COALESCE(SUM(evidence_count),0), COALESCE(MAX(confidence),0)
             FROM kg_nodes WHERE id IN ({})",
            ph.join(",")
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let sparams: Vec<&dyn rusqlite::types::ToSql> =
            old_ids.iter().map(|id| id as &dyn rusqlite::types::ToSql).collect();
        let (extra_evidence, max_conf): (i64, f64) =
            stmt.query_row(sparams.as_slice(), |row| Ok((row.get(0)?, row.get(1)?)))?;

        self.conn.execute(
            "UPDATE kg_nodes
             SET evidence_count = evidence_count + ?1,
                 confidence      = MAX(confidence, ?2)
             WHERE id = ?3",
            params![extra_evidence, max_conf, survivor_id],
        )?;
        Ok(())
    }

    /// Point all edges currently referencing `old_id` to `survivor_id`
    /// instead.  Returns the total number of edge rows updated.
    fn reroute_edges(&self, old_id: i64, survivor_id: i64) -> Result<usize> {
        let mut count = 0usize;
        count += self.conn.execute(
            "UPDATE kg_edges SET source_id = ?1 WHERE source_id = ?2",
            params![survivor_id, old_id],
        )?;
        count += self.conn.execute(
            "UPDATE kg_edges SET target_id = ?1 WHERE target_id = ?2",
            params![survivor_id, old_id],
        )?;
        Ok(count)
    }

    /// Move aliases from `old_id` to `survivor_id`, silently dropping ones
    /// that already exist on the survivor (UNIQUE constraint).
    fn migrate_aliases(&self, old_id: i64, survivor_id: i64) -> Result<()> {
        // Remove aliases that would conflict (same alias already on survivor)
        self.conn.execute(
            "DELETE FROM kg_aliases
             WHERE node_id = ?1
               AND alias IN (SELECT alias FROM kg_aliases WHERE node_id = ?2)",
            params![old_id, survivor_id],
        )?;
        // Move the rest
        self.conn.execute(
            "UPDATE kg_aliases SET node_id = ?1 WHERE node_id = ?2",
            params![survivor_id, old_id],
        )?;
        Ok(())
    }

    // ========================================================================
    // Phase 2 — Duplicate cognition consolidation
    // ========================================================================

    /// Find cognitions with the same `(subject, trait)` but different rows and
    /// merge them into a single row per key.
    ///
    /// Survivor: highest confidence → highest evidence_count → most recent
    /// `last_updated` → lowest id.  Evidence counts are summed; confidence is
    /// the max.  Snapshot history rows are reassigned to the survivor before
    /// the duplicate rows are deleted.
    pub fn consolidate_cognitions(&self) -> Result<ConsolidationReport> {
        let mut report = ConsolidationReport::default();

        let groups = {
            let mut stmt = self.conn.prepare(
                "SELECT subject, trait, COUNT(*) AS cnt, GROUP_CONCAT(id) AS ids
                 FROM cognitions
                 GROUP BY subject, trait
                 HAVING cnt > 1",
            )?;
            let rows: Vec<(String, String, i64, String)> = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;

            if rows.is_empty() {
                return Ok(report);
            }
            rows
        };

        self.conn.execute_batch("BEGIN")?;

        for (_subject, _trait, _cnt, ids_str) in &groups {
            let ids: Vec<i64> = ids_str
                .split(',')
                .filter_map(|s| s.trim().parse::<i64>().ok())
                .collect();
            if ids.len() < 2 {
                continue;
            }

            // --- Pick the survivor ---
            let ph: Vec<String> =
                ids.iter().enumerate().map(|(i, _)| format!("?{}", i + 1)).collect();
            let pick_sql = format!(
                "SELECT id FROM cognitions WHERE id IN ({})
                 ORDER BY confidence DESC, evidence_count DESC, last_updated DESC, id ASC LIMIT 1",
                ph.join(",")
            );
            let survivor_id: i64 = {
                let mut stmt = self.conn.prepare(&pick_sql)?;
                let sparams: Vec<&dyn rusqlite::types::ToSql> =
                    ids.iter().map(|id| id as &dyn rusqlite::types::ToSql).collect();
                match stmt.query_row(sparams.as_slice(), |row| row.get(0)) {
                    Ok(id) => id,
                    Err(e) => {
                        report
                            .errors
                            .push(format!("pick survivor for ids {:?}: {}", ids, e));
                        continue;
                    }
                }
            };

            let to_delete: Vec<i64> =
                ids.iter().copied().filter(|id| *id != survivor_id).collect();
            if to_delete.is_empty() {
                continue;
            }

            // --- Merge stats from duplicates ---
            let del_ph: Vec<String> =
                to_delete.iter().enumerate().map(|(i, _)| format!("?{}", i + 1)).collect();
            let merge_sql = format!(
                "SELECT COALESCE(SUM(evidence_count),0), COALESCE(MAX(confidence),0),
                        COALESCE(MAX(last_updated),0)
                 FROM cognitions WHERE id IN ({})",
                del_ph.join(",")
            );
            let (extra_evidence, max_conf, max_updated): (i64, f64, i64) = {
                let mut stmt = self.conn.prepare(&merge_sql)?;
                let dparams: Vec<&dyn rusqlite::types::ToSql> =
                    to_delete.iter().map(|id| id as &dyn rusqlite::types::ToSql).collect();
                stmt.query_row(dparams.as_slice(), |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })?
            };

            self.conn.execute(
                "UPDATE cognitions
                 SET evidence_count = evidence_count + ?1,
                     confidence      = MAX(confidence, ?2),
                     last_updated    = MAX(last_updated, ?3),
                     version         = version + 1
                 WHERE id = ?4",
                params![extra_evidence, max_conf, max_updated, survivor_id],
            )?;

            // --- Reassign snapshots to survivor ---
            for old_id in &to_delete {
                let _ = self.conn.execute(
                    "UPDATE cognition_snapshots SET cognition_id = ?1 WHERE cognition_id = ?2",
                    params![survivor_id, old_id],
                );
            }

            // --- Delete the duplicates ---
            let del2: Vec<String> =
                to_delete.iter().enumerate().map(|(i, _)| format!("?{}", i + 1)).collect();
            let del_sql = format!("DELETE FROM cognitions WHERE id IN ({})", del2.join(","));
            self.conn.execute(
                &del_sql,
                rusqlite::params_from_iter(to_delete.iter().copied()),
            )?;

            report.merged_cognitions += to_delete.len();
        }

        self.conn.execute_batch("COMMIT")?;

        Ok(report)
    }

    // ========================================================================
    // Phase 3 — Promote high-confidence session nodes to 'episodic'
    // ========================================================================

    /// Promote session-scoped `kg_nodes` to the 'episodic' intermediate tier
    /// when they have confidence >= `threshold` AND evidence_count >= 3.
    ///
    /// Nodes already at 'global' or 'episodic' layer are skipped.  Returns the
    /// number of promoted rows.
    pub fn promote_high_confidence(&self, threshold: f64) -> Result<usize> {
        self.conn.execute_batch("BEGIN")?;

        let promoted = self.conn.execute(
            "UPDATE kg_nodes SET layer = 'episodic'
             WHERE layer IN ('working', 'semantic')
               AND scope NOT IN ('global')
               AND confidence >= ?1
               AND evidence_count >= 3",
            params![threshold],
        )?;

        self.conn.execute_batch("COMMIT")?;

        Ok(promoted)
    }

    // ========================================================================
    // Full dedup consolidation cycle
    // ========================================================================

    /// Run a complete dedup consolidation cycle in order:
    /// 1. Deduplicate `kg_nodes` (merge same-name nodes, reroute edges)
    /// 2. Deduplicate `cognitions` (merge same subject+trait rows)
    /// 3. Promote high-confidence session nodes to 'episodic' (threshold 0.7)
    ///
    /// Each phase is independently transactional.  Errors from one phase do
    /// **not** prevent subsequent phases from executing — partial results are
    /// accumulated into the returned report.
    pub fn run_consolidation_cycle(&self) -> Result<ConsolidationReport> {
        let mut report = ConsolidationReport::default();

        // Phase 1 — Duplicate nodes
        match self.consolidate_duplicates() {
            Ok(r) => {
                report.merged_nodes = r.merged_nodes;
                report.edge_rerouted = r.edge_rerouted;
                report.errors.extend(r.errors);
            }
            Err(e) => report
                .errors
                .push(format!("duplicate node consolidation: {}", e)),
        }

        // Phase 2 — Duplicate cognitions
        match self.consolidate_cognitions() {
            Ok(r) => {
                report.merged_cognitions = r.merged_cognitions;
                report.errors.extend(r.errors);
            }
            Err(e) => report
                .errors
                .push(format!("cognition consolidation: {}", e)),
        }

        // Phase 3 — Promote to episodic
        match self.promote_high_confidence(0.7) {
            Ok(count) => report.promoted_count = count,
            Err(e) => report.errors.push(format!("promotion: {}", e)),
        }

        report.summary = report.to_string();
        Ok(report)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::GraphStore;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(include_str!(
            "../../../../migrations/memory/V1__memory.sql"
        ))
        .unwrap();
        // memory_db.rs adds embedding column idempotently.
        conn.execute_batch("ALTER TABLE kg_nodes ADD COLUMN embedding BLOB;")
            .unwrap();
        // The layer column is added by the Phase 2 layer system but may
        // not exist in all databases.  Add it for tests.
        conn.execute_batch("ALTER TABLE kg_nodes ADD COLUMN layer TEXT DEFAULT 'semantic';")
            .ok();
        conn
    }

    fn ensure_snapshot_table(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS cognition_snapshots (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                cognition_id  INTEGER NOT NULL,
                version       INTEGER NOT NULL,
                trait         TEXT NOT NULL,
                value         TEXT NOT NULL,
                confidence    REAL,
                evidence_count INTEGER,
                snapshot_at   INTEGER NOT NULL,
                FOREIGN KEY (cognition_id) REFERENCES cognitions(id)
            );",
        )
        .unwrap();
    }

    // -----------------------------------------------------------------------
    // Duplicate node consolidation
    // -----------------------------------------------------------------------

    #[test]
    fn test_consolidate_duplicate_nodes_basic() {
        let conn = setup_db();
        let store = GraphStore::new(&conn);

        let id1 = store
            .upsert_node("Rust", "Technology", "A programming language", 0.6, "session_a", None)
            .unwrap();
        let id2 = store
            .upsert_node("Rust", "Technology", "A systems language", 0.9, "session_b", None)
            .unwrap();
        assert_ne!(id1, id2);

        // Sanity: two rows
        let cnt: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM kg_nodes WHERE name = 'Rust'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cnt, 2);

        // Attach an edge so we can verify rerouting
        let other = store
            .upsert_node("Other", "Concept", "test", 0.5, "global", None)
            .unwrap();
        conn.execute(
            "INSERT INTO kg_edges (source_id, target_id, relation_type, fact, confidence,
                                   first_seen, last_updated, scope)
             VALUES (?1, ?2, 'tests', '', 0.8, 0, 0, 'global')",
            params![id1, other],
        )
        .unwrap();

        let consolidator = MemoryConsolidator::new(&conn);
        let report = consolidator.consolidate_duplicates().unwrap();

        assert_eq!(report.merged_nodes, 1);
        assert!(report.errors.is_empty(), "unexpected errors: {:?}", report.errors);

        // One node left
        let cnt2: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM kg_nodes WHERE name = 'Rust'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cnt2, 1);

        // Edge rerouted to survivor
        let edge_cnt: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM kg_edges
                 WHERE source_id = (SELECT id FROM kg_nodes WHERE name = 'Rust')
                   AND target_id = (SELECT id FROM kg_nodes WHERE name = 'Other')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(edge_cnt, 1, "edge should point to survivor");
    }

    #[test]
    fn test_consolidate_duplicates_no_dupes() {
        let conn = setup_db();
        let store = GraphStore::new(&conn);
        store
            .upsert_node("Only", "Concept", "desc", 0.8, "global", None)
            .unwrap();

        let consolidator = MemoryConsolidator::new(&conn);
        let report = consolidator.consolidate_duplicates().unwrap();

        assert_eq!(report.merged_nodes, 0);
        assert!(report.errors.is_empty());
    }

    #[test]
    fn test_consolidate_aliases_migrated() {
        let conn = setup_db();

        // Create two nodes with same name
        conn.execute(
            "INSERT INTO kg_nodes (name, entity_type, description, confidence,
                                   evidence_count, first_seen, last_updated, scope)
             VALUES ('Dup', 'Concept', '', 0.5, 1, 0, 0, 'session_a')",
            [],
        )
        .unwrap();
        let low_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO kg_nodes (name, entity_type, description, confidence,
                                   evidence_count, first_seen, last_updated, scope)
             VALUES ('Dup', 'Concept', '', 0.9, 5, 0, 0, 'session_b')",
            [],
        )
        .unwrap();
        let high_id = conn.last_insert_rowid();

        // Add alias to the low-confidence node
        conn.execute(
            "INSERT INTO kg_aliases (node_id, alias) VALUES (?1, 'Rusty')",
            params![low_id],
        )
        .unwrap();

        let consolidator = MemoryConsolidator::new(&conn);
        let report = consolidator.consolidate_duplicates().unwrap();
        assert_eq!(report.merged_nodes, 1);

        // Alias should now point to the survivor (high_id)
        let alias_node: i64 = conn
            .query_row(
                "SELECT node_id FROM kg_aliases WHERE alias = 'Rusty'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(alias_node, high_id);
    }

    // -----------------------------------------------------------------------
    // Cognition consolidation
    // -----------------------------------------------------------------------

    #[test]
    fn test_consolidate_cognitions_basic() {
        let conn = setup_db();
        ensure_snapshot_table(&conn);

        conn.execute(
            "INSERT INTO cognitions (subject, trait, value, confidence, evidence_count,
                                     first_seen, last_updated, version, scope)
             VALUES ('Alice', 'skill', 'rust', 0.6, 2, 100, 200, 1, 'global')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO cognitions (subject, trait, value, confidence, evidence_count,
                                     first_seen, last_updated, version, scope)
             VALUES ('Alice', 'skill', 'rust-advanced', 0.9, 1, 100, 300, 2, 'session_x')",
            [],
        )
        .unwrap();

        let before: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM cognitions WHERE subject='Alice' AND trait='skill'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(before, 2);

        let consolidator = MemoryConsolidator::new(&conn);
        let report = consolidator.consolidate_cognitions().unwrap();

        assert_eq!(report.merged_cognitions, 1);
        assert!(report.errors.is_empty(), "{:?}", report.errors);

        let after: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM cognitions WHERE subject='Alice' AND trait='skill'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(after, 1);

        // Merged: confidence = MAX(0.6, 0.9) = 0.9, evidence = 2 + 1 = 3
        let (conf, ev): (f64, i64) = conn
            .query_row(
                "SELECT confidence, evidence_count FROM cognitions
                 WHERE subject='Alice' AND trait='skill'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert!((conf - 0.9).abs() < 1e-9);
        assert_eq!(ev, 3);
    }

    #[test]
    fn test_consolidate_cognitions_no_dupes() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO cognitions (subject, trait, value, confidence, evidence_count,
                                     first_seen, last_updated, version, scope)
             VALUES ('Bob', 'hobby', 'gaming', 0.7, 1, 100, 200, 1, 'global')",
            [],
        )
        .unwrap();

        let consolidator = MemoryConsolidator::new(&conn);
        let report = consolidator.consolidate_cognitions().unwrap();

        assert_eq!(report.merged_cognitions, 0);
        assert!(report.errors.is_empty());
    }

    #[test]
    fn test_consolidate_cognitions_snapshot_reassign() {
        let conn = setup_db();
        ensure_snapshot_table(&conn);

        // Two duplicate cognitions
        conn.execute(
            "INSERT INTO cognitions (subject, trait, value, confidence, evidence_count,
                                     first_seen, last_updated, version, scope)
             VALUES ('X', 'trait1', 'v1', 0.5, 1, 100, 100, 1, 'global')",
            [],
        )
        .unwrap();
        let id_a = conn.last_insert_rowid();

        conn.execute(
            "INSERT INTO cognitions (subject, trait, value, confidence, evidence_count,
                                     first_seen, last_updated, version, scope)
             VALUES ('X', 'trait1', 'v2', 1.0, 2, 200, 300, 2, 'global')",
            [],
        )
        .unwrap();
        let id_b = conn.last_insert_rowid();

        // Attach a snapshot to the low-confidence duplicate
        conn.execute(
            "INSERT INTO cognition_snapshots (cognition_id, version, trait, value, confidence,
                                              evidence_count, snapshot_at)
             VALUES (?1, 1, 'trait1', 'v0', 0.4, 1, 50)",
            params![id_a],
        )
        .unwrap();
        let snap_id = conn.last_insert_rowid();

        let consolidator = MemoryConsolidator::new(&conn);
        let report = consolidator.consolidate_cognitions().unwrap();
        assert_eq!(report.merged_cognitions, 1);

        // Snapshot should now reference the survivor (id_b)
        let ref_id: i64 = conn
            .query_row(
                "SELECT cognition_id FROM cognition_snapshots WHERE id = ?1",
                params![snap_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(ref_id, id_b);

        // Old cognition row should be gone
        let old_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM cognitions WHERE id = ?1",
                params![id_a],
                |r| r.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap_or(false);
        assert!(!old_exists);
    }

    // -----------------------------------------------------------------------
    // Promote high-confidence session nodes
    // -----------------------------------------------------------------------

    #[test]
    fn test_promote_high_confidence() {
        let conn = setup_db();
        let store = GraphStore::new(&conn);

        // Session node meeting threshold
        let id1 = store
            .upsert_node("HighConf", "Concept", "desc", 0.8, "session_a", None)
            .unwrap();
        conn.execute(
            "UPDATE kg_nodes SET evidence_count = 5 WHERE id = ?1",
            params![id1],
        )
        .unwrap();

        // Session node below threshold (low confidence)
        let _id2 = store
            .upsert_node("LowConf", "Concept", "desc", 0.5, "session_b", None)
            .unwrap();

        // Already global node (should be skipped)
        let _id3 = store
            .upsert_node("GlobalNode", "Concept", "desc", 0.9, "global", None)
            .unwrap();

        let consolidator = MemoryConsolidator::new(&conn);
        let promoted = consolidator.promote_high_confidence(0.7).unwrap();

        assert_eq!(promoted, 1);

        // Scope should remain unchanged; layer should be promoted to 'episodic'.
        let scope: String = conn
            .query_row(
                "SELECT scope FROM kg_nodes WHERE name = 'HighConf'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(scope, "session_a");

        let layer: String = conn
            .query_row(
                "SELECT layer FROM kg_nodes WHERE name = 'HighConf'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(layer, "episodic");

        let scope_low: String = conn
            .query_row(
                "SELECT scope FROM kg_nodes WHERE name = 'LowConf'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(scope_low, "session_b");

        let scope_global: String = conn
            .query_row(
                "SELECT scope FROM kg_nodes WHERE name = 'GlobalNode'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(scope_global, "global");
    }

    #[test]
    fn test_promote_high_confidence_skips_episodic() {
        let conn = setup_db();
        let store = GraphStore::new(&conn);

        // Node already at episodic layer — should be skipped
        let id = store
            .upsert_node("AlreadyEpi", "Concept", "desc", 0.9, "session_a", Some("episodic"))
            .unwrap();
        conn.execute(
            "UPDATE kg_nodes SET evidence_count = 10 WHERE id = ?1",
            params![id],
        )
        .unwrap();

        let consolidator = MemoryConsolidator::new(&conn);
        let promoted = consolidator.promote_high_confidence(0.7).unwrap();
        assert_eq!(promoted, 0, "already-episodic nodes should be skipped");

        let layer: String = conn
            .query_row(
                "SELECT layer FROM kg_nodes WHERE name = 'AlreadyEpi'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(layer, "episodic");
    }

    #[test]
    fn test_promote_insufficient_evidence() {
        let conn = setup_db();
        let store = GraphStore::new(&conn);

        let _id = store
            .upsert_node("BarelySeen", "Concept", "desc", 0.9, "session_a", None)
            .unwrap();
        // evidence_count defaults to 1 — below the 3 threshold

        let consolidator = MemoryConsolidator::new(&conn);
        let promoted = consolidator.promote_high_confidence(0.7).unwrap();
        assert_eq!(promoted, 0);

        // Scope and layer should remain unchanged.
        let scope: String = conn
            .query_row(
                "SELECT scope FROM kg_nodes WHERE name = 'BarelySeen'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(scope, "session_a");

        let layer: String = conn
            .query_row(
                "SELECT layer FROM kg_nodes WHERE name = 'BarelySeen'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(layer, "semantic");
    }

    // -----------------------------------------------------------------------
    // run_consolidation_cycle (integration)
    // -----------------------------------------------------------------------

    #[test]
    fn test_run_consolidation_cycle_integration() {
        let conn = setup_db();
        ensure_snapshot_table(&conn);
        let store = GraphStore::new(&conn);

        // --- Duplicate node ---
        store
            .upsert_node("DupNode", "Concept", "desc1", 0.5, "session_a", None)
            .unwrap();
        store
            .upsert_node("DupNode", "Concept", "desc2", 0.8, "session_b", None)
            .unwrap();

        // --- Duplicate cognition ---
        conn.execute(
            "INSERT INTO cognitions (subject, trait, value, confidence, evidence_count,
                                     first_seen, last_updated, version, scope)
             VALUES ('Bob', 'hobby', 'gaming', 0.7, 3, 100, 200, 1, 'session_a')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO cognitions (subject, trait, value, confidence, evidence_count,
                                     first_seen, last_updated, version, scope)
             VALUES ('Bob', 'hobby', 'coding', 0.5, 1, 100, 150, 1, 'session_b')",
            [],
        )
        .unwrap();

        // --- Promote-eligible node ---
        let pid = store
            .upsert_node("PromoNode", "Skill", "useful", 0.85, "session_c", None)
            .unwrap();
        conn.execute(
            "UPDATE kg_nodes SET evidence_count = 10 WHERE id = ?1",
            params![pid],
        )
        .unwrap();

        let consolidator = MemoryConsolidator::new(&conn);
        let report = consolidator.run_consolidation_cycle().unwrap();

        assert_eq!(report.merged_nodes, 1, "1 duplicate node merged");
        assert_eq!(report.merged_cognitions, 1, "1 duplicate cognition merged");
        assert_eq!(report.promoted_count, 1, "1 node promoted");
        assert!(
            report.errors.is_empty(),
            "unexpected errors: {:?}",
            report.errors
        );

        // Post-condition checks
        let node_cnt: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM kg_nodes WHERE name = 'DupNode'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(node_cnt, 1);

        let cog_cnt: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM cognitions WHERE subject='Bob' AND trait='hobby'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cog_cnt, 1);

        let promo_scope: String = conn
            .query_row(
                "SELECT scope FROM kg_nodes WHERE name = 'PromoNode'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(promo_scope, "session_c");

        let promo_layer: String = conn
            .query_row(
                "SELECT layer FROM kg_nodes WHERE name = 'PromoNode'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(promo_layer, "episodic");
    }

    #[test]
    fn test_consolidation_cycle_empty_db() {
        let conn = setup_db();
        let consolidator = MemoryConsolidator::new(&conn);
        let report = consolidator.run_consolidation_cycle().unwrap();

        assert_eq!(report.merged_nodes, 0);
        assert_eq!(report.merged_cognitions, 0);
        assert_eq!(report.promoted_count, 0);
        assert!(report.errors.is_empty());
    }

    // -----------------------------------------------------------------------
    // Existing layer-cycle tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_layer_consolidation_promote_to_semantic() {
        let conn = setup_db();

        // Insert episodic nodes that should be promoted
        conn.execute(
            "INSERT INTO kg_nodes (name, entity_type, description, confidence,
                                   evidence_count, first_seen, last_updated, scope, layer)
             VALUES ('EpiNode', 'Concept', '', 0.6, 5, 0, 0, 'global', 'episodic')",
            [],
        )
        .unwrap();

        let consolidator = MemoryConsolidator::new(&conn);
        let report = consolidator.run_cycle().unwrap();

        assert_eq!(report.promoted, 1, "should promote 1 node to semantic");
        assert_eq!(report.demoted, 0);
        assert_eq!(report.pruned, 0);

        let layer: String = conn
            .query_row(
                "SELECT layer FROM kg_nodes WHERE name = 'EpiNode'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(layer, "semantic");
    }

    #[test]
    fn test_layer_consolidation_prune_stale() {
        let conn = setup_db();
        let old_ts = chrono::Utc::now().timestamp() - 100 * 86400; // 100 days ago

        conn.execute(
            "INSERT INTO kg_nodes (name, entity_type, description, confidence,
                                   evidence_count, first_seen, last_updated, scope, layer)
             VALUES ('StaleNode', 'Concept', '', 0.1, 1, 0, ?1, 'global', 'episodic')",
            params![old_ts],
        )
        .unwrap();

        let consolidator = MemoryConsolidator::new(&conn);
        let report = consolidator.run_cycle().unwrap();

        assert_eq!(report.pruned, 1, "stale episodic node should be pruned");

        let cnt: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM kg_nodes WHERE name = 'StaleNode'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cnt, 0);
    }
}
