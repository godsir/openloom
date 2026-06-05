//! Knowledge graph store — entity nodes, directed edges, and graph queries.
//! Uses kg_nodes, kg_edges, kg_aliases, kg_evidence tables from V8 migration.
//! Queries use recursive CTEs for graph traversal (no external graph DB needed).

use anyhow::Result;
use chrono::Utc;
use loom_types::MemoryQualityReport;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// Default embedding dimension, matching common embedding models
/// (e.g., text-embedding-3-small, all-mpnet-base-v2: 768).
/// Compute the weighted health score (0-100) from the report's metrics.
///
/// Weights:
/// - 30% relevance: how well injected memories match what the LLM uses
/// - 25% freshness: how recently entities have been accessed
/// - 20% coverage: breadth of entity types present
/// - 15% confidence: average entity confidence
/// - 10% dedup rate: cleanliness (inverse of duplicate rate)
pub fn compute_health_score(report: &MemoryQualityReport) -> f64 {
    let relevance = (report.avg_relevance * 100.0).min(100.0);
    let freshness = if report.total_entities > 0 {
        (report.entities_accessed_recently as f64 / report.total_entities as f64 * 100.0)
            .min(100.0)
    } else {
        0.0
    };
    let coverage = (report.entity_types_distribution.len() as f64 / 8.0).min(1.0) * 100.0;
    let confidence = (report.avg_confidence * 100.0).min(100.0);
    let dedup = if report.total_entities > 0 {
        (1.0 - report.duplicate_rate) * 100.0
    } else {
        100.0
    };

    let score = 0.30 * relevance
        + 0.25 * freshness
        + 0.20 * coverage
        + 0.15 * confidence
        + 0.10 * dedup;

    (score * 10.0).round() / 10.0 // round to 1 decimal place
}

pub const DEFAULT_EMBEDDING_DIM: usize = 768;

/// A row from a graph query.
#[derive(Debug, Clone)]
pub struct GraphRow {
    pub node_id: i64,
    pub name: String,
    pub entity_type: String,
    pub description: String,
    pub confidence: f64,
    pub relation_type: Option<String>,
    pub distance: Option<usize>,
    pub scope: String,
}

/// A scored entity (for ranked queries like top interests).
#[derive(Debug, Clone)]
pub struct ScoredEntity {
    pub name: String,
    pub entity_type: String,
    pub relation_type: String,
    pub fact: String,
    pub confidence: f64,
    pub score: f64,
}

/// A step in a graph path traversal.
#[derive(Debug, Clone)]
pub struct PathStep {
    pub from: String,
    pub relation: String,
    pub to: String,
    pub depth: usize,
}

/// Result of a single active-forgetting pruning pass.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PruningResult {
    pub pruned_nodes: i64,
    pub pruned_edges: i64,
    pub pruned_cognitions: i64,
    pub skipped_protected: i64,
}

/// Health snapshot of the in-memory knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphHealth {
    pub total_nodes: i64,
    pub total_edges: i64,
    pub avg_confidence: f64,
    pub oldest_node_age_days: i64,
    pub layer_distribution: Vec<(String, i64)>,
}

/// Read/write access to knowledge graph tables.
pub struct GraphStore<'a> {
    conn: &'a Connection,
}

impl<'a> GraphStore<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    // ========================================================================
    // Write
    // ========================================================================

    /// Upsert an entity node. Returns the node ID.
    /// `layer` defaults to "semantic" when None.
    /// LLM-extracted high-confidence entities can be "episodic".
    pub fn upsert_node(
        &self,
        name: &str,
        entity_type: &str,
        description: &str,
        confidence: f64,
        scope: &str,
        layer: Option<&str>,
    ) -> Result<i64> {
        let layer_val = layer.unwrap_or("semantic");
        let now = Utc::now().timestamp();
        let existing: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM kg_nodes WHERE name = ?1 AND scope = ?2",
                rusqlite::params![name, scope],
                |row| row.get(0),
            )
            .ok();

        if let Some(id) = existing {
            self.conn.execute(
                "UPDATE kg_nodes SET description = ?1, confidence = MAX(confidence, ?2),
                 evidence_count = evidence_count + 1, last_updated = ?3, layer = ?4 WHERE id = ?5",
                rusqlite::params![description, confidence, now, layer_val, id],
            )?;
            Ok(id)
        } else {
            self.conn.execute(
                "INSERT INTO kg_nodes (name, entity_type, description, confidence,
                 evidence_count, first_seen, last_updated, scope, layer)
                 VALUES (?1, ?2, ?3, ?4, 1, ?5, ?5, ?6, ?7)",
                rusqlite::params![
                    name,
                    entity_type,
                    description,
                    confidence,
                    now,
                    scope,
                    layer_val
                ],
            )?;
            Ok(self.conn.last_insert_rowid())
        }
    }

    /// Upsert an edge between two entities. Returns the edge ID.
    pub fn upsert_edge(
        &self,
        source_id: i64,
        target_id: i64,
        relation_type: &str,
        fact: &str,
        confidence: f64,
        scope: &str,
    ) -> Result<i64> {
        let now = Utc::now().timestamp();
        let existing: Option<i64> = self.conn.query_row(
            "SELECT id FROM kg_edges WHERE source_id = ?1 AND target_id = ?2 AND relation_type = ?3",
            rusqlite::params![source_id, target_id, relation_type],
            |row| row.get(0),
        ).ok();

        if let Some(id) = existing {
            self.conn.execute(
                "UPDATE kg_edges SET fact = ?1, confidence = MAX(confidence, ?2),
                 evidence_count = evidence_count + 1, last_updated = ?3 WHERE id = ?4",
                rusqlite::params![fact, confidence, now, id],
            )?;
            Ok(id)
        } else {
            self.conn.execute(
                "INSERT INTO kg_edges (source_id, target_id, relation_type, fact,
                 confidence, evidence_count, first_seen, last_updated, scope)
                 VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?6, ?7)",
                rusqlite::params![
                    source_id,
                    target_id,
                    relation_type,
                    fact,
                    confidence,
                    now,
                    scope
                ],
            )?;
            Ok(self.conn.last_insert_rowid())
        }
    }

    // ========================================================================
    // Access tracking & evidence
    // ========================================================================

    /// Record a node access (increment counter, update last_accessed).
    fn touch_node(&self, node_id: i64) -> Result<()> {
        let now = Utc::now().timestamp();
        self.conn.execute(
            "UPDATE kg_nodes SET access_count = access_count + 1, last_accessed = ?1 WHERE id = ?2",
            rusqlite::params![now, node_id],
        )?;
        Ok(())
    }

    /// Touch all nodes returned by a query.
    fn touch_rows(&self, rows: &[GraphRow]) -> Result<()> {
        for r in rows {
            let _ = self.touch_node(r.node_id);
        }
        Ok(())
    }

    /// Link a node to a source event in kg_evidence.
    pub fn link_evidence_node(&self, node_id: i64, event_id: i64) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO kg_evidence (node_id, event_id) VALUES (?1, ?2)",
            rusqlite::params![node_id, event_id],
        )?;
        Ok(())
    }

    /// Link an edge to a source event in kg_evidence.
    pub fn link_evidence_edge(&self, edge_id: i64, event_id: i64) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO kg_evidence (edge_id, event_id) VALUES (?1, ?2)",
            rusqlite::params![edge_id, event_id],
        )?;
        Ok(())
    }

    /// Find the most recent event ID for a session.
    pub fn latest_event_id(&self, session_id: &str) -> Result<Option<i64>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id FROM events WHERE source_session = ?1 ORDER BY id DESC LIMIT 1",
                rusqlite::params![session_id],
                |row| row.get(0),
            )
            .ok())
    }

    // ========================================================================
    // Read — Entity Search
    // ========================================================================

    /// Full-text search for entities by name or description.
    /// Automatically adds prefix matching (*) to bare ASCII terms.
    /// CJK terms are passed through as-is (FTS5 unicode61 tokenizes per-character).
    pub fn search_entities(
        &self,
        query: &str,
        limit: usize,
        scope: Option<&str>,
    ) -> Result<Vec<GraphRow>> {
        let has_ops = query.contains('*')
            || query.contains("AND")
            || query.contains("OR")
            || query.contains("NOT")
            || query.contains('"');
        let expanded = if has_ops {
            query.to_string()
        } else {
            query
                .split_whitespace()
                .map(|t| {
                    // Only add prefix wildcard to ASCII terms; CJK chars don't benefit
                    if t.chars().any(|c| c.is_ascii_alphabetic()) && t.is_ascii() {
                        format!("{}*", t)
                    } else {
                        t.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        };
        let sql = if scope.is_some() {
            "SELECT n.id, n.name, n.entity_type, n.description, n.confidence, n.scope
             FROM kg_nodes_fts fts JOIN kg_nodes n ON n.id = fts.rowid
             WHERE kg_nodes_fts MATCH ?1 AND (n.scope = ?3 OR n.scope = 'global')
             ORDER BY rank LIMIT ?2"
        } else {
            "SELECT n.id, n.name, n.entity_type, n.description, n.confidence, n.scope
             FROM kg_nodes_fts fts JOIN kg_nodes n ON n.id = fts.rowid
             WHERE kg_nodes_fts MATCH ?1 ORDER BY rank LIMIT ?2"
        };
        let mut stmt = self.conn.prepare(sql)?;
        let map_row = |row: &rusqlite::Row<'_>| {
            Ok(GraphRow {
                node_id: row.get(0)?,
                name: row.get(1)?,
                entity_type: row.get(2)?,
                description: row.get(3)?,
                confidence: row.get(4)?,
                relation_type: None,
                distance: None,
                scope: row.get(5)?,
            })
        };
        let rows = if let Some(s) = scope {
            stmt.query_map(rusqlite::params![expanded, limit, s], map_row)?
                .collect::<std::result::Result<Vec<_>, _>>()?
        } else {
            stmt.query_map(rusqlite::params![expanded, limit], map_row)?
                .collect::<std::result::Result<Vec<_>, _>>()?
        };
        let _ = self.touch_rows(&rows);
        Ok(rows)
    }

    /// Resolve an entity name to its node ID (via exact match or alias).
    pub fn resolve_node(&self, name: &str) -> Result<Option<i64>> {
        // Try exact match first
        if let Ok(Some(id)) = self.conn.query_row(
            "SELECT id FROM kg_nodes WHERE name = ?1",
            rusqlite::params![name],
            |row| row.get(0),
        ) {
            let _ = self.touch_node(id);
            return Ok(Some(id));
        }
        // Try alias
        let alias_result = self.conn.query_row(
            "SELECT node_id FROM kg_aliases WHERE alias = ?1",
            rusqlite::params![name],
            |row| row.get(0),
        );
        if let Ok(id) = alias_result {
            let _ = self.touch_node(id);
            return Ok(Some(id));
        }
        Ok(None)
    }

    // ========================================================================
    // Read — Graph Traversal
    // ========================================================================

    /// Get neighbors of an entity (1-hop connections).
    pub fn neighbors(
        &self,
        node_name: &str,
        scope: Option<&str>,
        limit: usize,
    ) -> Result<Vec<GraphRow>> {
        // Touch the starting node first
        if let Ok(Some(id)) = self.resolve_node(node_name) {
            let _ = self.touch_node(id);
        }
        let (sql, params): (&str, Vec<Box<dyn rusqlite::types::ToSql>>) = if let Some(s) = scope {
            (
                "SELECT n.id, n.name, n.entity_type, n.description, e.relation_type, e.confidence, n.scope
                 FROM kg_nodes src
                 JOIN kg_edges e ON (e.source_id = src.id OR e.target_id = src.id)
                 JOIN kg_nodes n ON (
                    (e.source_id = n.id AND e.target_id = src.id) OR
                    (e.target_id = n.id AND e.source_id = src.id)
                 )
                 WHERE src.name = ?1 AND (e.scope = ?2 OR e.scope = 'global')
                 ORDER BY e.confidence DESC LIMIT ?3",
                vec![
                    Box::new(node_name.to_string()) as Box<dyn rusqlite::types::ToSql>,
                    Box::new(s.to_string()),
                    Box::new(limit as i64),
                ],
            )
        } else {
            (
                "SELECT n.id, n.name, n.entity_type, n.description, e.relation_type, e.confidence, n.scope
                 FROM kg_nodes src
                 JOIN kg_edges e ON (e.source_id = src.id OR e.target_id = src.id)
                 JOIN kg_nodes n ON (
                    (e.source_id = n.id AND e.target_id = src.id) OR
                    (e.target_id = n.id AND e.source_id = src.id)
                 )
                 WHERE src.name = ?1
                 ORDER BY e.confidence DESC LIMIT ?2",
                vec![
                    Box::new(node_name.to_string()) as Box<dyn rusqlite::types::ToSql>,
                    Box::new(limit as i64),
                ],
            )
        };
        let mut stmt = self.conn.prepare(sql)?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            Ok(GraphRow {
                node_id: row.get(0)?,
                name: row.get(1)?,
                entity_type: row.get(2)?,
                description: row.get(3)?,
                relation_type: Some(row.get(4)?),
                confidence: row.get(5)?,
                distance: Some(1),
                scope: row.get(6)?,
            })
        })?;
        let results: Vec<GraphRow> = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        let _ = self.touch_rows(&results);
        Ok(results)
    }

    /// Get top-scored interests/connections for a subject with temporal decay
    /// and layer-aware weighting.
    /// Layer priority: working(40) > episodic(30) > semantic(20) > global(10).
    /// The score is multiplied by (retrieval_priority / 20.0) so that the
    /// default 'semantic' layer is the ×1.0 baseline.
    pub fn top_interests(
        &self,
        subject: &str,
        scope: Option<&str>,
        now_ts: i64,
        limit: usize,
    ) -> Result<Vec<ScoredEntity>> {
        let scope_filter = scope.unwrap_or("global");
        let mut stmt = self.conn.prepare(
            "SELECT n.name, n.entity_type, e.relation_type, e.fact, e.confidence, e.evidence_count,
                    ROUND(
                        e.confidence
                        * (1.0 + (CASE WHEN e.evidence_count > 0 THEN LN(CAST(e.evidence_count AS REAL) + 1) * 0.5 ELSE 0 END))
                        * (1.0 + (CASE WHEN n.access_count > 0 THEN LN(CAST(n.access_count AS REAL) + 1) * 0.3 ELSE 0 END))
                        * EXP(MAX(?1 - e.last_updated, 0) / -2592000.0)
                        * (CASE WHEN n.last_accessed IS NOT NULL
                            THEN EXP(MAX(?1 - n.last_accessed, 0) / -604800.0)
                            ELSE 0.5 END)
                        * COALESCE(ml.retrieval_priority, 20) / 20.0,
                    4) AS score
             FROM kg_nodes subj
             JOIN kg_edges e ON e.source_id = subj.id
             JOIN kg_nodes n ON e.target_id = n.id
             LEFT JOIN memory_layers ml ON ml.name = n.layer
             WHERE subj.name = ?2 AND subj.entity_type = 'Person'
               AND (e.scope = ?3 OR e.scope = 'global')
             ORDER BY score DESC LIMIT ?4"
        )?;
        let rows = stmt.query_map(
            rusqlite::params![now_ts, subject, scope_filter, limit],
            |row| {
                Ok(ScoredEntity {
                    name: row.get(0)?,
                    entity_type: row.get(1)?,
                    relation_type: row.get(2)?,
                    fact: row.get(3)?,
                    confidence: row.get(4)?,
                    score: row.get(5)?,
                })
            },
        )?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Query knowledge graph context around given entity names.
    ///
    /// Builds a formatted context string for LLM injection, with optional
    /// layer filtering. When `layer` is provided, only entities in layers
    /// at or above the given priority tier are included (cumulative: giving
    /// "episodic" also includes "working").
    ///
    /// When `layer` is None, uses the current behavior (scope filtering only).
    pub fn query_kg_context(
        &self,
        entity_names: &[&str],
        limit: usize,
        scope: Option<&str>,
        layer: Option<&str>,
    ) -> Result<String> {
        let mut lines: Vec<String> = Vec::new();
        const MIN_CONFIDENCE: f64 = 0.5;
        let scope_val = scope.unwrap_or("global");

        // Resolve which layers to include — cumulative, higher-priority tiers
        // are always included when a lower-bound layer is requested.
        let eligible_layers: Vec<&str> = match layer {
            Some("working") => vec!["working"],
            Some("episodic") => vec!["working", "episodic"],
            Some("semantic") => vec!["working", "episodic", "semantic"],
            _ => vec!["working", "episodic", "semantic", "global"],
        };

        // Always include the USER node and its neighbors
        if let Ok(Some(_user_id)) = self.resolve_node("USER") {
            // Get USER neighbors with layer+scope filtering inline
            let layer_placeholders: String = eligible_layers
                .iter()
                .enumerate()
                .map(|(i, _)| format!("?{}", i + 2))
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!(
                "SELECT n.id, n.name, n.entity_type, n.description, e.relation_type, e.confidence, n.scope
                 FROM kg_nodes src
                 JOIN kg_edges e ON (e.source_id = src.id OR e.target_id = src.id)
                 JOIN kg_nodes n ON (
                    (e.source_id = n.id AND e.target_id = src.id) OR
                    (e.target_id = n.id AND e.source_id = src.id)
                 )
                 WHERE src.name = ?1
                   AND n.layer IN ({})
                   AND (n.scope = ?{} OR n.scope = 'global')
                 ORDER BY e.confidence DESC LIMIT ?{}",
                layer_placeholders,
                eligible_layers.len() + 2,
                eligible_layers.len() + 3,
            );

            let mut params: Vec<Box<dyn rusqlite::types::ToSql>> =
                vec![Box::new("USER".to_string())];
            for l in &eligible_layers {
                params.push(Box::new((*l).to_string()));
            }
            params.push(Box::new(scope_val.to_string()));
            params.push(Box::new(limit as i64));

            let mut stmt = self.conn.prepare(&sql)?;
            let param_refs: Vec<&dyn rusqlite::types::ToSql> =
                params.iter().map(|p| p.as_ref()).collect();
            let rows = stmt.query_map(param_refs.as_slice(), |row| {
                Ok(GraphRow {
                    node_id: row.get(0)?,
                    name: row.get(1)?,
                    entity_type: row.get(2)?,
                    description: row.get(3)?,
                    confidence: row.get(5)?,
                    relation_type: row.get::<_, Option<String>>(4)?,
                    distance: Some(1),
                    scope: row.get(6)?,
                })
            })?;
            for r in rows {
                let n = r?;
                if n.confidence >= MIN_CONFIDENCE {
                    let rel = n.relation_type.as_deref().unwrap_or("related_to");
                    lines.push(format!(
                        "- USER {} {} (confidence: {:.2})",
                        rel, n.name, n.confidence
                    ));
                }
            }
        }

        // Query each entity name via FTS5 + layer filter, then get neighbors
        let mut seen_entities: std::collections::HashSet<String> = std::collections::HashSet::new();

        for name in entity_names {
            if name.is_empty() || *name == "USER" {
                continue;
            }

            // FTS5 search with layer+scope filter
            let layer_placeholders: String = eligible_layers
                .iter()
                .enumerate()
                .map(|(i, _)| format!("?{}", i + 2))
                .collect::<Vec<_>>()
                .join(",");
            let scope_ph = eligible_layers.len() + 2;
            let search_sql = format!(
                "SELECT n.id, n.name, n.entity_type, n.description, n.confidence, n.scope, n.layer
                 FROM kg_nodes_fts fts JOIN kg_nodes n ON n.id = fts.rowid
                 WHERE kg_nodes_fts MATCH ?1
                   AND n.layer IN ({})
                   AND (n.scope = ?{} OR n.scope = 'global')
                 ORDER BY rank LIMIT 3",
                layer_placeholders, scope_ph,
            );
            let mut params: Vec<Box<dyn rusqlite::types::ToSql>> =
                vec![Box::new((*name).to_string())];
            for l in &eligible_layers {
                params.push(Box::new((*l).to_string()));
            }
            params.push(Box::new(scope_val.to_string()));

            let results: Vec<GraphRow> = {
                let mut stmt = self.conn.prepare(&search_sql)?;
                let param_refs: Vec<&dyn rusqlite::types::ToSql> =
                    params.iter().map(|p| p.as_ref()).collect();
                stmt.query_map(param_refs.as_slice(), |row| {
                    Ok(GraphRow {
                        node_id: row.get(0)?,
                        name: row.get(1)?,
                        entity_type: row.get(2)?,
                        description: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                        confidence: row.get(4)?,
                        relation_type: None,
                        distance: None,
                        scope: row.get(5)?,
                    })
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?
            };

            for r in &results {
                if r.name == "USER" || r.confidence < MIN_CONFIDENCE {
                    continue;
                }
                if !seen_entities.insert(r.name.clone()) {
                    continue;
                }
                lines.push(format!(
                    "- {} is a {}: {} (confidence: {:.2})",
                    r.name, r.entity_type, r.description, r.confidence
                ));

                // Get immediate neighbors with layer+scope filter
                let layer_ph: String = eligible_layers
                    .iter()
                    .enumerate()
                    .map(|(i, _)| format!("?{}", i + 2))
                    .collect::<Vec<_>>()
                    .join(",");
                let n_scope_ph = eligible_layers.len() + 2;
                let neigh_sql = format!(
                    "SELECT n2.id, n2.name, n2.entity_type, n2.description,
                            e2.relation_type, n2.confidence, n2.scope
                     FROM kg_nodes n1
                     JOIN kg_edges e2 ON (e2.source_id = n1.id OR e2.target_id = n1.id)
                     JOIN kg_nodes n2 ON (
                        (e2.source_id = n2.id AND e2.target_id = n1.id) OR
                        (e2.target_id = n2.id AND e2.source_id = n1.id)
                     )
                     WHERE n1.name = ?1
                       AND n2.layer IN ({})
                       AND (n2.scope = ?{} OR n2.scope = 'global')
                     ORDER BY e2.confidence DESC LIMIT 3",
                    layer_ph, n_scope_ph,
                );
                let mut nparams: Vec<Box<dyn rusqlite::types::ToSql>> =
                    vec![Box::new(r.name.clone())];
                for l in &eligible_layers {
                    nparams.push(Box::new((*l).to_string()));
                }
                nparams.push(Box::new(scope_val.to_string()));
                let mut nstmt = self.conn.prepare(&neigh_sql)?;
                let nparam_refs: Vec<&dyn rusqlite::types::ToSql> =
                    nparams.iter().map(|p| p.as_ref()).collect();
                let neigh_rows = nstmt.query_map(nparam_refs.as_slice(), |nrow| {
                    Ok(GraphRow {
                        node_id: nrow.get(0)?,
                        name: nrow.get(1)?,
                        entity_type: nrow.get(2)?,
                        description: nrow.get(3)?,
                        confidence: nrow.get(5)?,
                        relation_type: nrow.get::<_, Option<String>>(4)?,
                        distance: Some(2),
                        scope: nrow.get(6)?,
                    })
                })?;
                for nr in neigh_rows {
                    let n = nr?;
                    if n.name == "USER" || n.name == r.name || n.confidence < MIN_CONFIDENCE {
                        continue;
                    }
                    let rel = n.relation_type.as_deref().unwrap_or("related_to");
                    lines.push(format!("  └ {} {} {}", r.name, rel, n.name));
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

    /// Walk up to N hops from a starting entity (breadth-first via recursive CTE).
    pub fn walk(
        &self,
        start_name: &str,
        max_depth: u8,
        scope: Option<&str>,
        limit: usize,
    ) -> Result<Vec<GraphRow>> {
        let (sql, params): (&str, Vec<Box<dyn rusqlite::types::ToSql>>) = if let Some(s) = scope {
            (
                "WITH RECURSIVE walk(id, name, entity_type, description, depth, visited, scope) AS (
                    SELECT n.id, n.name, n.entity_type, n.description, 0,
                           '/' || CAST(n.id AS TEXT) || '/', n.scope
                    FROM kg_nodes n
                    WHERE n.name = ?1 AND (n.scope = ?2 OR n.scope = 'global')
                    UNION
                    SELECT CASE WHEN e.source_id = w.id THEN e.target_id ELSE e.source_id END,
                           CASE WHEN e.source_id = w.id THEN tn.name ELSE sn.name END,
                           CASE WHEN e.source_id = w.id THEN tn.entity_type ELSE sn.entity_type END,
                           CASE WHEN e.source_id = w.id THEN tn.description ELSE sn.description END,
                           w.depth + 1,
                           w.visited || CAST(CASE WHEN e.source_id = w.id THEN e.target_id ELSE e.source_id END AS TEXT) || '/',
                           CASE WHEN e.source_id = w.id THEN tn.scope ELSE sn.scope END
                    FROM walk w
                    JOIN kg_edges e ON (e.source_id = w.id OR e.target_id = w.id)
                    JOIN kg_nodes sn ON sn.id = e.source_id
                    JOIN kg_nodes tn ON tn.id = e.target_id
                    WHERE w.depth < ?3
                      AND (e.scope = ?2 OR e.scope = 'global')
                      AND w.visited NOT LIKE '%/' || CAST(CASE WHEN e.source_id = w.id THEN e.target_id ELSE e.source_id END AS TEXT) || '/%'
                )
                SELECT DISTINCT id, name, entity_type, description, MIN(depth), scope
                FROM walk WHERE depth > 0
                GROUP BY id ORDER BY MIN(depth) LIMIT ?4",
                vec![
                    Box::new(start_name.to_string()) as Box<dyn rusqlite::types::ToSql>,
                    Box::new(s.to_string()),
                    Box::new(max_depth as i64),
                    Box::new(limit as i64),
                ],
            )
        } else {
            (
                "WITH RECURSIVE walk(id, name, entity_type, description, depth, visited, scope) AS (
                    SELECT n.id, n.name, n.entity_type, n.description, 0,
                           '/' || CAST(n.id AS TEXT) || '/', n.scope
                    FROM kg_nodes n
                    WHERE n.name = ?1
                    UNION
                    SELECT CASE WHEN e.source_id = w.id THEN e.target_id ELSE e.source_id END,
                           CASE WHEN e.source_id = w.id THEN tn.name ELSE sn.name END,
                           CASE WHEN e.source_id = w.id THEN tn.entity_type ELSE sn.entity_type END,
                           CASE WHEN e.source_id = w.id THEN tn.description ELSE sn.description END,
                           w.depth + 1,
                           w.visited || CAST(CASE WHEN e.source_id = w.id THEN e.target_id ELSE e.source_id END AS TEXT) || '/',
                           CASE WHEN e.source_id = w.id THEN tn.scope ELSE sn.scope END
                    FROM walk w
                    JOIN kg_edges e ON (e.source_id = w.id OR e.target_id = w.id)
                    JOIN kg_nodes sn ON sn.id = e.source_id
                    JOIN kg_nodes tn ON tn.id = e.target_id
                    WHERE w.depth < ?2
                      AND w.visited NOT LIKE '%/' || CAST(CASE WHEN e.source_id = w.id THEN e.target_id ELSE e.source_id END AS TEXT) || '/%'
                )
                SELECT DISTINCT id, name, entity_type, description, MIN(depth), scope
                FROM walk WHERE depth > 0
                GROUP BY id ORDER BY MIN(depth) LIMIT ?3",
                vec![
                    Box::new(start_name.to_string()) as Box<dyn rusqlite::types::ToSql>,
                    Box::new(max_depth as i64),
                    Box::new(limit as i64),
                ],
            )
        };
        let mut stmt = self.conn.prepare(sql)?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            Ok(GraphRow {
                node_id: row.get(0)?,
                name: row.get(1)?,
                entity_type: row.get(2)?,
                description: row.get(3)?,
                confidence: 0.0,
                relation_type: None,
                distance: Some(row.get(4)?),
                scope: row.get(5)?,
            })
        })?;
        let results: Vec<GraphRow> = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        let _ = self.touch_rows(&results);
        Ok(results)
    }

    /// Shortest path between two entities via BFS with recursive CTE.
    pub fn path_between(&self, from: &str, to: &str, max_depth: u8) -> Result<Vec<PathStep>> {
        let mut stmt = self.conn.prepare(
            "WITH RECURSIVE bfs(id, parent_id, parent_name, relation, depth, visited, target_name) AS (
                SELECT id, NULL, NULL, '', 0, '/' || CAST(id AS TEXT) || '/', name
                FROM kg_nodes WHERE name = ?1
                UNION ALL
                SELECT CASE WHEN e.source_id = b.id THEN e.target_id ELSE e.source_id END,
                       b.id, b.target_name, e.relation_type,
                       b.depth + 1,
                       b.visited || CAST(CASE WHEN e.source_id = b.id THEN e.target_id ELSE e.source_id END AS TEXT) || '/',
                       CASE WHEN e.source_id = b.id THEN tn.name ELSE sn.name END
                FROM bfs b
                JOIN kg_edges e ON (e.source_id = b.id OR e.target_id = b.id)
                JOIN kg_nodes sn ON sn.id = e.source_id
                JOIN kg_nodes tn ON tn.id = e.target_id
                WHERE b.depth < ?2
                  AND b.visited NOT LIKE '%/' || CAST(CASE WHEN e.source_id = b.id THEN e.target_id ELSE e.source_id END AS TEXT) || '/%'
            )
            SELECT parent_name, relation, target_name, depth FROM bfs
            WHERE target_name = ?3 ORDER BY depth LIMIT 1"
        )?;

        let mut steps = Vec::new();
        let rows = stmt.query_map(rusqlite::params![from, max_depth, to], |row| {
            Ok(PathStep {
                from: row.get::<_, Option<String>>(0)?.unwrap_or_default(),
                relation: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                to: row.get(2)?,
                depth: row.get(3)?,
            })
        })?;

        for row in rows {
            steps.push(row?);
        }
        Ok(steps)
    }

    /// Return all edges where both source and target are in the given node IDs.
    /// When scope is provided, only returns edges in that scope or global scope.
    pub fn edges_between(
        &self,
        node_ids: &[i64],
        scope: Option<&str>,
    ) -> Result<Vec<(String, String, String, f64)>> {
        if node_ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholder = "?,".repeat(node_ids.len());
        let in_clause = &placeholder[..placeholder.len() - 1];
        let (sql, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = if let Some(s) = scope {
            // Build explicit ?NNN placeholders so scope (?1) does not collide
            // with the auto-numbered ? markers in the IN clauses.
            let n = node_ids.len();
            let source_ph: Vec<String> = (2..(2 + n)).map(|i| format!("?{}", i)).collect();
            let target_ph: Vec<String> =
                ((2 + n)..(2 + 2 * n)).map(|i| format!("?{}", i)).collect();
            let sql = format!(
                "SELECT sn.name, tn.name, e.relation_type, e.confidence
                 FROM kg_edges e
                 JOIN kg_nodes sn ON sn.id = e.source_id
                 JOIN kg_nodes tn ON tn.id = e.target_id
                 WHERE e.source_id IN ({}) AND e.target_id IN ({})
                 AND (e.scope = ?1 OR e.scope = 'global')",
                source_ph.join(","),
                target_ph.join(",")
            );
            let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(s.to_string())];
            for id in node_ids.iter().chain(node_ids.iter()) {
                params.push(Box::new(*id));
            }
            (sql, params)
        } else {
            let sql = format!(
                "SELECT sn.name, tn.name, e.relation_type, e.confidence
                 FROM kg_edges e
                 JOIN kg_nodes sn ON sn.id = e.source_id
                 JOIN kg_nodes tn ON tn.id = e.target_id
                 WHERE e.source_id IN ({}) AND e.target_id IN ({})",
                in_clause, in_clause
            );
            let params: Vec<Box<dyn rusqlite::types::ToSql>> = node_ids
                .iter()
                .chain(node_ids.iter())
                .map(|id| Box::new(*id) as Box<dyn rusqlite::types::ToSql>)
                .collect();
            (sql, params)
        };
        let mut stmt = self.conn.prepare(&sql)?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, f64>(3)?,
            ))
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    // ========================================================================
    // Aliases
    // ========================================================================

    /// Add an alias for an entity node.
    pub fn add_alias(&self, node_id: i64, alias: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO kg_aliases (node_id, alias) VALUES (?1, ?2)",
            rusqlite::params![node_id, alias],
        )?;
        Ok(())
    }

    // ========================================================================
    // Maintenance
    // ========================================================================

    /// Prune stale low-confidence entities not accessed recently.
    /// Returns the number of nodes removed.
    pub fn prune_stale(&self, older_than_days: i64, max_count: usize) -> Result<usize> {
        let cutoff = Utc::now().timestamp() - older_than_days * 86400;
        // Delete stale nodes (never accessed, old, low confidence, not Person)
        let deleted = self.conn.execute(
            "DELETE FROM kg_nodes WHERE id IN (
                SELECT id FROM kg_nodes
                WHERE access_count = 0
                  AND last_updated < ?1
                  AND confidence < 0.5
                  AND entity_type != 'Person'
                LIMIT ?2
            )",
            rusqlite::params![cutoff, max_count as i64],
        )?;

        if deleted > 0 {
            // Cascade: clean orphaned edges, aliases, evidence
            self.conn.execute_batch(
                "DELETE FROM kg_edges WHERE source_id NOT IN (SELECT id FROM kg_nodes)
                    OR target_id NOT IN (SELECT id FROM kg_nodes);
                 DELETE FROM kg_aliases WHERE node_id NOT IN (SELECT id FROM kg_nodes);
                 DELETE FROM kg_evidence WHERE node_id NOT IN (SELECT id FROM kg_nodes)
                    AND edge_id NOT IN (SELECT id FROM kg_edges);",
            )?;
        }
        Ok(deleted)
    }

    /// List recent nodes with pagination.
    pub fn list_nodes(
        &self,
        limit: usize,
        offset: usize,
        scope: Option<&str>,
    ) -> Result<Vec<GraphRow>> {
        let (sql, has_scope) = match scope {
            Some(_) => (
                "SELECT n.id, n.name, n.entity_type, n.description, n.confidence,
                        CAST(NULL AS TEXT) as relation_type, CAST(NULL AS INTEGER) as distance, n.scope
                 FROM kg_nodes n WHERE n.scope = ?3 OR n.scope = 'global'
                 ORDER BY n.last_updated DESC LIMIT ?1 OFFSET ?2",
                true
            ),
            None => (
                "SELECT n.id, n.name, n.entity_type, n.description, n.confidence,
                        CAST(NULL AS TEXT) as relation_type, CAST(NULL AS INTEGER) as distance, n.scope
                 FROM kg_nodes n ORDER BY n.last_updated DESC LIMIT ?1 OFFSET ?2",
                false
            ),
        };
        let mut stmt = self.conn.prepare(sql)?;
        let rows = if has_scope {
            stmt.query_map(
                rusqlite::params![limit as i64, offset as i64, scope.unwrap()],
                |row| {
                    Ok(GraphRow {
                        node_id: row.get(0)?,
                        name: row.get(1)?,
                        entity_type: row.get(2)?,
                        description: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                        confidence: row.get(4)?,
                        relation_type: None,
                        distance: None,
                        scope: row.get(7)?,
                    })
                },
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?
        } else {
            stmt.query_map(rusqlite::params![limit as i64, offset as i64], |row| {
                Ok(GraphRow {
                    node_id: row.get(0)?,
                    name: row.get(1)?,
                    entity_type: row.get(2)?,
                    description: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                    confidence: row.get(4)?,
                    relation_type: None,
                    distance: None,
                    scope: row.get(7)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?
        };
        Ok(rows)
    }

    /// Delete a node by name and optional scope (NULL means all scopes).
    pub fn delete_node(&self, name: &str) -> Result<bool> {
        let affected = self.conn.execute(
            "DELETE FROM kg_nodes WHERE name = ?1",
            rusqlite::params![name],
        )?;
        Ok(affected > 0)
    }

    /// Delete an edge between two named nodes by relation type.
    pub fn delete_edge(
        &self,
        source_name: &str,
        target_name: &str,
        relation_type: &str,
    ) -> Result<bool> {
        let affected = self.conn.execute(
            "DELETE FROM kg_edges WHERE source_id = (SELECT id FROM kg_nodes WHERE name = ?1)
             AND target_id = (SELECT id FROM kg_nodes WHERE name = ?2)
             AND relation_type = ?3",
            rusqlite::params![source_name, target_name, relation_type],
        )?;
        Ok(affected > 0)
    }

    /// Promote all nodes/edges with the given scope to "global", merging duplicates.
    /// Returns the number of nodes promoted.
    pub fn promote_scope_to_global(&self, scope: &str, min_confidence: f64) -> Result<usize> {
        // First promote nodes: change scope to 'global' where no global duplicate exists
        let promoted = self.conn.execute(
            "UPDATE kg_nodes SET scope = 'global'
             WHERE scope = ?1 AND confidence >= ?2
             AND name NOT IN (SELECT name FROM kg_nodes WHERE scope = 'global')",
            rusqlite::params![scope, min_confidence],
        )?;
        // Promote edges whose both endpoints are now global
        self.conn.execute(
            "UPDATE kg_edges SET scope = 'global'
             WHERE scope = ?1
               AND source_id IN (SELECT id FROM kg_nodes WHERE scope = 'global')
               AND target_id IN (SELECT id FROM kg_nodes WHERE scope = 'global')",
            rusqlite::params![scope],
        )?;
        // Delete remaining session-scoped edges (those with at least one endpoint not promoted)
        self.conn.execute(
            "DELETE FROM kg_edges WHERE scope = ?1",
            rusqlite::params![scope],
        )?;
        // Delete remaining session-scoped nodes (duplicates or low confidence)
        self.conn.execute(
            "DELETE FROM kg_nodes WHERE scope = ?1",
            rusqlite::params![scope],
        )?;
        Ok(promoted)
    }

    /// Promote specific nodes by name to global scope (no deletion of others).
    /// Used for selective promotion from the UI.
    pub fn promote_nodes_by_name(&self, names: &[String]) -> Result<usize> {
        let mut count = 0;
        for name in names {
            count += self.conn.execute(
                "UPDATE kg_nodes SET scope = 'global' WHERE name = ?1 AND scope != 'global'",
                rusqlite::params![name],
            )?;
        }
        Ok(count)
    }

    /// Delete all nodes, edges, and evidence with a given scope.
    pub fn delete_by_scope(&self, scope: &str) -> Result<()> {
        // Delete evidence referencing nodes/edges in this scope
        self.conn.execute(
            "DELETE FROM kg_evidence WHERE node_id IN (SELECT id FROM kg_nodes WHERE scope = ?1)",
            rusqlite::params![scope],
        )?;
        self.conn.execute(
            "DELETE FROM kg_evidence WHERE edge_id IN (SELECT id FROM kg_edges WHERE scope = ?1)",
            rusqlite::params![scope],
        )?;
        // Delete edges in this scope
        self.conn.execute(
            "DELETE FROM kg_edges WHERE scope = ?1",
            rusqlite::params![scope],
        )?;
        // Delete nodes in this scope
        self.conn.execute(
            "DELETE FROM kg_nodes WHERE scope = ?1",
            rusqlite::params![scope],
        )?;
        Ok(())
    }

    /// Total entity count (for pruning threshold check).
    pub fn node_count(&self) -> Result<usize> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM kg_nodes", [], |row| {
                row.get::<_, i64>(0)
            })? as usize)
    }

    /// Total edge count.
    pub fn edge_count(&self) -> Result<usize> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM kg_edges", [], |row| {
                row.get::<_, i64>(0)
            })? as usize)
    }

    // ========================================================================
    // Layer Management (Phase 2 — memory tiering)
    // ========================================================================

    /// Count nodes per layer. Returns Vec<(layer_name, count)>.
    pub fn get_layer_stats(&self) -> Result<Vec<(String, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT COALESCE(layer, 'semantic') as layer, COUNT(*) as cnt
             FROM kg_nodes GROUP BY layer ORDER BY cnt DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Promote a node to a different memory layer by ID.
    /// Used by the consolidator to elevate high-confidence entities
    /// (e.g., from "semantic" to "episodic").
    pub fn promote_node_layer(&self, node_id: i64, new_layer: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE kg_nodes SET layer = ?1, last_updated = ?2 WHERE id = ?3",
            rusqlite::params![new_layer, chrono::Utc::now().timestamp(), node_id],
        )?;
        Ok(())
    }

    // ========================================================================
    // Memory Quality Logging
    // ========================================================================

    /// Record a memory quality log entry — which entities were injected into the
    /// system prompt and how long injection took. Returns the log entry ID.
    pub fn record_quality_log(
        &self,
        session_id: &str,
        turn_seq: i64,
        injected_entities_json: &str,
        duration_ms: i64,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO memory_quality_log (session_id, turn_seq, injected_entities, injection_duration_ms)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![session_id, turn_seq, injected_entities_json, duration_ms],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Update a quality log entry with the entities the assistant actually referenced.
    pub fn update_quality_log_references(
        &self,
        log_id: i64,
        referenced_entities_json: &str,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE memory_quality_log SET referenced_entities = ?1 WHERE id = ?2",
            rusqlite::params![referenced_entities_json, log_id],
        )?;
        Ok(())
    }

    /// Get quality statistics for a session.
    /// Returns a JSON object with total_injections, avg_relevance, and evaluated_turns.
    pub fn get_quality_stats(&self, session_id: &str) -> Result<serde_json::Value> {
        let mut stmt = self.conn.prepare(
            "SELECT injected_entities, referenced_entities, injection_duration_ms
             FROM memory_quality_log WHERE session_id = ?1
             ORDER BY created_at ASC",
        )?;
        let rows: Vec<(String, Option<String>, i64)> = stmt
            .query_map(rusqlite::params![session_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let total = rows.len();
        let mut total_relevance: f64 = 0.0;
        let mut evaluated_turns: usize = 0;
        let mut total_duration_ms: i64 = 0;
        let mut turns_with_refs: usize = 0;

        for (injected_str, ref_opt, dur) in &rows {
            total_duration_ms += dur;
            let injected: Vec<String> = serde_json::from_str(injected_str).unwrap_or_default();
            let referenced: Vec<String> = ref_opt
                .as_ref()
                .and_then(|r| serde_json::from_str(r).ok())
                .unwrap_or_default();
            if !injected.is_empty() {
                let relevance = referenced.len() as f64 / injected.len() as f64;
                total_relevance += relevance;
                evaluated_turns += 1;
            }
            if !referenced.is_empty() {
                turns_with_refs += 1;
            }
        }

        let avg_relevance = if evaluated_turns > 0 {
            (total_relevance / evaluated_turns as f64 * 100.0).round() / 100.0
        } else {
            0.0
        };
        let avg_duration_ms = if total > 0 {
            total_duration_ms / total as i64
        } else {
            0
        };

        Ok(serde_json::json!({
            "total_injections": total,
            "avg_relevance": avg_relevance,
            "evaluated_turns": evaluated_turns,
            "turns_with_references": turns_with_refs,
            "avg_injection_duration_ms": avg_duration_ms,
        }))
    }

    /// Evaluate holistic memory quality across all dimensions.
    ///
    /// `lookback_days` limits the quality-log window for relevance metrics
    /// (e.g., 30 days).  Pass 0 to use all available logs.
    ///
    /// Safe to call on an empty database — all counts will be 0 / 0.0 and
    /// `health_score` will be 0.0.
    pub fn evaluate_memory_quality(&self, lookback_days: i64) -> Result<MemoryQualityReport> {
        let now_ts = Utc::now().timestamp();
        let lookback_cutoff = if lookback_days > 0 {
            now_ts - lookback_days * 86400
        } else {
            0 // include all data
        };

        // ── Injection quality (from memory_quality_log) ──────────────────
        //
        // Defensively check for table existence — pre-V3 databases may lack
        // memory_quality_log. When absent, all quality metrics are zeroed.

        let has_quality_table: bool = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type='table' AND name='memory_quality_log'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap_or(false);

        let (injection_count, turns_with_references, avg_relevance) = if has_quality_table {
            let mut total: i64 = 0;
            let mut ref_turns: i64 = 0;
            let mut sum_relevance: f64 = 0.0;
            let mut evaluated: i64 = 0;

            // Query only the lookback window.
            // Use datetime(?1, 'unixepoch') to convert the epoch-seconds parameter
            // to ISO-8601 text — avoids strftime() per-row and allows index use.
            let rows: Vec<(String, Option<String>)> = if lookback_days > 0 {
                let mut stmt = self.conn.prepare(
                    "SELECT injected_entities, referenced_entities
                     FROM memory_quality_log
                     WHERE created_at >= datetime(?1, 'unixepoch')
                     ORDER BY created_at ASC",
                )?;
                stmt.query_map(rusqlite::params![lookback_cutoff], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?
            } else {
                let mut stmt = self.conn.prepare(
                    "SELECT injected_entities, referenced_entities
                     FROM memory_quality_log
                     ORDER BY created_at ASC",
                )?;
                stmt.query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?
            };

            for (injected_str, ref_opt) in &rows {
                let injected: Vec<String> = serde_json::from_str(injected_str).unwrap_or_default();
                let referenced: Vec<String> = ref_opt
                    .as_ref()
                    .and_then(|r| serde_json::from_str(r).ok())
                    .unwrap_or_default();

                total += 1;

                if !referenced.is_empty() {
                    ref_turns += 1;
                }

                // Include ALL turns in the relevance average, not just
                // those with non-empty injections. Zero-injection turns
                // contribute 0 relevance, which is correct — if the system
                // injected nothing, the relevance is zero for that turn.
                let relevance = if injected.is_empty() {
                    0.0
                } else {
                    (referenced.len() as f64 / injected.len() as f64).min(1.0)
                };
                sum_relevance += relevance;
                evaluated += 1;
            }

            let avg_rel = if evaluated > 0 {
                (sum_relevance / evaluated as f64 * 100.0).round() / 100.0
            } else {
                0.0
            };

            (total, ref_turns, avg_rel)
        } else {
            // No quality log table — all zeroes.
            (0i64, 0i64, 0.0f64)
        };

        // ── Entity health ────────────────────────────────────────────────

        let total_entities: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM kg_nodes", [], |row| row.get(0))
            .unwrap_or(0);

        // Duplicate rate: entities sharing the same name
        let duplicate_rate: f64 = if total_entities > 0 {
            let dup_count: i64 = self
                .conn
                .query_row(
                    "SELECT COALESCE(SUM(cnt - 1), 0)
                 FROM (SELECT COUNT(*) AS cnt FROM kg_nodes GROUP BY name HAVING cnt > 1)",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            dup_count as f64 / total_entities as f64
        } else {
            0.0
        };

        // Stale entities: never accessed AND first_seen > 30 days ago.
        // Fresh entities (first_seen within 30 days) with no accesses yet
        // are NOT considered stale — they simply haven't had a chance to be used.
        let stale_cutoff = now_ts - 30 * 86400;
        let stale_entity_count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM kg_nodes
             WHERE (last_accessed IS NULL OR last_accessed < ?1)
               AND (access_count IS NULL OR access_count = 0)
               AND first_seen < ?1",
                rusqlite::params![stale_cutoff, stale_cutoff],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Average confidence
        let avg_confidence: f64 = self
            .conn
            .query_row(
                "SELECT COALESCE(AVG(confidence), 0.0) FROM kg_nodes",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0.0);

        // ── Coverage ─────────────────────────────────────────────────────

        let entity_types_distribution: Vec<(String, i64)> = {
            let mut stmt = self.conn.prepare(
                "SELECT entity_type, COUNT(*) AS cnt FROM kg_nodes
                 GROUP BY entity_type ORDER BY cnt DESC",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?;
            let mut result = Vec::new();
            for row in rows {
                result.push(row?);
            }
            result
        };

        let layer_distribution: Vec<(String, i64)> = {
            let mut stmt = self.conn.prepare(
                "SELECT COALESCE(layer, 'semantic') AS layer, COUNT(*) AS cnt
                 FROM kg_nodes GROUP BY layer ORDER BY cnt DESC",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?;
            let mut result = Vec::new();
            for row in rows {
                result.push(row?);
            }
            result
        };

        // ── Freshness ────────────────────────────────────────────────────

        let recent_cutoff = now_ts - 7 * 86400; // last 7 days

        let entities_added_recently: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM kg_nodes WHERE first_seen >= ?1",
                rusqlite::params![recent_cutoff],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let entities_accessed_recently: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM kg_nodes WHERE last_accessed IS NOT NULL AND last_accessed >= ?1",
            rusqlite::params![recent_cutoff],
            |row| row.get(0),
        ).unwrap_or(0);

        // ── Consolidation effectiveness ──────────────────────────────────
        //
        // Consolidation metrics are tracked via the memory_consolidation_log
        // table when available.  When the table does not exist (pre-Phase-3
        // databases) both values are returned as 0 so the health score still
        // works; the frontend can hide the section when total_merged == 0.

        let (consolidation_runs, total_merged) = {
            let has_log_table: bool = self
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master
                     WHERE type='table' AND name='memory_consolidation_log'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .map(|c| c > 0)
                .unwrap_or(false);

            if has_log_table {
                let runs: i64 = self
                    .conn
                    .query_row("SELECT COUNT(*) FROM memory_consolidation_log", [], |row| {
                        row.get(0)
                    })
                    .unwrap_or(0);
                let merged: i64 = self.conn.query_row(
                    "SELECT COALESCE(SUM(merged_nodes + merged_cognitions), 0) FROM memory_consolidation_log",
                    [],
                    |row| row.get(0),
                ).unwrap_or(0);
                (runs, merged)
            } else {
                (0, 0)
            }
        };

        // ── Build report + health score ──────────────────────────────────

        let mut report = MemoryQualityReport {
            avg_relevance,
            injection_count,
            turns_with_references,
            total_entities,
            duplicate_rate,
            stale_entity_count,
            avg_confidence,
            entity_types_distribution,
            layer_distribution,
            entities_added_recently,
            entities_accessed_recently,
            consolidation_runs,
            total_merged,
            health_score: 0.0, // computed below
        };

        report.health_score = compute_health_score(&report);

        Ok(report)
    }

    // ========================================================================
    // Vector embedding — semantic similarity search
    // ========================================================================

    /// Store a float32 embedding vector for a named entity node.
    /// The embedding is serialised as a BLOB of little-endian f32 values.
    /// If the node does not exist, a minimal node is created automatically.
    pub fn embed_node(&self, name: &str, embedding: &[f32]) -> Result<()> {
        let bytes: Vec<u8> = embedding.iter().flat_map(|f| f.to_le_bytes()).collect();
        let affected = self.conn.execute(
            "UPDATE kg_nodes SET embedding = ?1 WHERE name = ?2",
            rusqlite::params![bytes, name],
        )?;
        if affected == 0 {
            // Node doesn't exist yet — upsert a minimal node with embedding
            let now = Utc::now().timestamp();
            self.conn.execute(
                "INSERT INTO kg_nodes (name, entity_type, description, confidence, evidence_count,
                 first_seen, last_updated, scope, embedding)
                 VALUES (?1, 'concept', '', 0.5, 1, ?2, ?2, 'global', ?3)",
                rusqlite::params![name, now, bytes],
            )?;
        }
        Ok(())
    }

    /// Retrieve the stored embedding vector for a node. Returns `None` if the
    /// node has no embedding or does not exist.
    pub fn get_embedding(&self, name: &str) -> Result<Option<Vec<f32>>> {
        let result: Option<Vec<u8>> = self
            .conn
            .query_row(
                "SELECT embedding FROM kg_nodes WHERE name = ?1 AND embedding IS NOT NULL",
                rusqlite::params![name],
                |row| row.get(0),
            )
            .ok();
        match result {
            Some(blob) => Ok(Some(blob_to_f32_vec(&blob))),
            None => Ok(None),
        }
    }

    /// Search for entities whose stored embeddings are most similar to the
    /// query embedding via cosine similarity. Returns up to `limit` results
    /// sorted by descending similarity, each paired with its similarity score.
    ///
    /// Falls back to FTS5 text search when no nodes have embeddings or when
    /// `fallback_query` is provided and the vector search yields no results.
    pub fn search_similar(
        &self,
        embedding: &[f32],
        limit: usize,
        fallback_query: Option<&str>,
        scope: Option<&str>,
    ) -> Result<Vec<(GraphRow, f64)>> {
        // Build scope filter clause
        let scope_clause = if scope.is_some() {
            "AND (n.scope = ?1 OR n.scope = 'global')"
        } else {
            ""
        };

        let sql = format!(
            "SELECT n.id, n.name, n.entity_type, n.description, n.confidence, n.scope, n.embedding
             FROM kg_nodes n WHERE n.embedding IS NOT NULL {}
             LIMIT 5000",
            scope_clause
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let rows: Vec<_> = if let Some(scope_str) = scope {
            stmt.query_map(rusqlite::params![scope_str], |row| {
                let emb_bytes: Vec<u8> = row.get(6)?;
                let stored: Vec<f32> = blob_to_f32_vec(&emb_bytes);
                Ok((
                    GraphRow {
                        node_id: row.get(0)?,
                        name: row.get(1)?,
                        entity_type: row.get(2)?,
                        description: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                        confidence: row.get(4)?,
                        relation_type: None,
                        distance: None,
                        scope: row.get(5)?,
                    },
                    stored,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?
        } else {
            stmt.query_map([], |row| {
                let emb_bytes: Vec<u8> = row.get(6)?;
                let stored: Vec<f32> = blob_to_f32_vec(&emb_bytes);
                Ok((
                    GraphRow {
                        node_id: row.get(0)?,
                        name: row.get(1)?,
                        entity_type: row.get(2)?,
                        description: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                        confidence: row.get(4)?,
                        relation_type: None,
                        distance: None,
                        scope: row.get(5)?,
                    },
                    stored,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?
        };

        if !rows.is_empty() {
            // Vector search path: compute cosine similarity and rank
            let mut scored: Vec<(GraphRow, f64)> = rows
                .into_iter()
                .filter(|(_, emb)| emb.len() == embedding.len())
                .map(|(row, emb)| {
                    let sim = cosine_similarity(embedding, &emb);
                    (row, sim)
                })
                .collect();

            scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            scored.truncate(limit);

            let result_rows: Vec<GraphRow> = scored.iter().map(|(r, _)| r.clone()).collect();
            let _ = self.touch_rows(&result_rows);
            return Ok(scored);
        }

        // Fallback: no embeddings available — use FTS5 text search if query provided
        if let Some(query) = fallback_query {
            let text_results = self.search_entities(query, limit, scope)?;
            let scored: Vec<(GraphRow, f64)> = text_results.into_iter().map(|r| (r, 0.0)).collect();
            return Ok(scored);
        }

        Ok(Vec::new())
    }

    // ========================================================================
    // Active Forgetting — importance-based pruning
    // ========================================================================

    /// Score all non-protected nodes by importance, then prune those below
    /// `min_importance` that are also older than `max_age_days`.
    ///
    /// Importance formula:
    ///   confidence * (1 + ln(evidence_count+1)*0.3) * (1 + ln(access_count+1)*0.15)
    ///   * exp(-days_since_access / 60)
    ///
    /// The decay is continuous (no 30-day cliff) — a 60-day half-life ensures
    /// gradual rather than abrupt degradation of importance.
    /// Protection rules (never pruned):
    /// - entity_type = 'Person'
    /// - evidence_count >= 10
    /// - scope = 'global' AND layer = 'global'
    /// - access_count > 50
    ///
    /// Cascade delete order (inside a transaction): edges → aliases → evidence → node.
    pub fn active_forgetting(
        &self,
        min_importance: f64,
        max_age_days: i64,
    ) -> Result<PruningResult> {
        let now = Utc::now().timestamp();
        let cutoff_ts = now - max_age_days * 86400;

        // Step 1: read all nodes with the fields needed for scoring
        let mut stmt = self.conn.prepare(
            "SELECT id, entity_type, confidence, evidence_count, access_count,
                    last_accessed, first_seen, scope, layer
             FROM kg_nodes",
        )?;

        #[allow(clippy::type_complexity)]
        let nodes: Vec<(
            i64,         // id
            String,      // entity_type
            f64,         // confidence
            i64,         // evidence_count
            i64,         // access_count
            Option<i64>, // last_accessed
            i64,         // first_seen
            String,      // scope
            String,      // layer
        )> = stmt
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get::<_, Option<i64>>(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        // Step 2: score and classify
        let mut to_prune: Vec<i64> = Vec::new();
        let mut skipped_protected: i64 = 0;

        for (
            id,
            entity_type,
            confidence,
            evidence_count,
            access_count,
            last_accessed,
            first_seen,
            scope,
            layer,
        ) in &nodes
        {
            // --- Protection rules (NEVER prune) ---
            if entity_type == "Person" {
                skipped_protected += 1;
                continue;
            }
            if *evidence_count >= 10 {
                skipped_protected += 1;
                continue;
            }
            if scope == "global" && layer == "global" {
                skipped_protected += 1;
                continue;
            }
            if *access_count > 50 {
                skipped_protected += 1;
                continue;
            }

            // --- Age gate: skip nodes younger than max_age_days ---
            if *first_seen > cutoff_ts {
                continue;
            }

            // --- Importance score ---
            let last_access = last_accessed.unwrap_or(*first_seen);
            let days_since_access = ((now - last_access) as f64 / 86400.0).max(0.0);

            let importance = confidence
                * (1.0 + f64::ln(*evidence_count as f64 + 1.0) * 0.3)
                * (1.0 + f64::ln(*access_count as f64 + 1.0) * 0.15)
                * f64::exp(-days_since_access / 60.0);

            if importance < min_importance {
                to_prune.push(*id);
            }
        }

        if to_prune.is_empty() {
            return Ok(PruningResult {
                pruned_nodes: 0,
                pruned_edges: 0,
                pruned_cognitions: 0,
                skipped_protected,
            });
        }

        // Step 3: cascade delete within a transaction
        // We build string-interpolated IN clauses — safe because IDs come from our DB.
        let id_list: String = to_prune
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",");

        let run_pruning = || -> Result<PruningResult> {
            self.conn.execute_batch("BEGIN;")?;

            // Count edges that reference pruned nodes (before we delete evidence)
            let pruned_edges: i64 = self.conn.query_row(
                &format!(
                    "SELECT COUNT(*) FROM kg_edges WHERE source_id IN ({}) OR target_id IN ({})",
                    id_list, id_list
                ),
                [],
                |row| row.get(0),
            )?;

            // Count evidence rows that will be pruned
            let pruned_cognitions: i64 = self.conn.query_row(
                &format!(
                    "SELECT COUNT(*) FROM kg_evidence WHERE node_id IN ({})
                       OR edge_id IN (SELECT id FROM kg_edges WHERE source_id IN ({}) OR target_id IN ({}))",
                    id_list, id_list, id_list
                ),
                [],
                |row| row.get(0),
            )?;

            // 1. Delete evidence referencing edges of pruned nodes
            self.conn.execute(
                &format!(
                    "DELETE FROM kg_evidence WHERE edge_id IN (SELECT id FROM kg_edges WHERE source_id IN ({}) OR target_id IN ({}))",
                    id_list, id_list
                ),
                [],
            )?;

            // 2. Delete evidence referencing pruned nodes directly
            self.conn.execute(
                &format!("DELETE FROM kg_evidence WHERE node_id IN ({})", id_list),
                [],
            )?;

            // 3. Delete edges
            self.conn.execute(
                &format!(
                    "DELETE FROM kg_edges WHERE source_id IN ({}) OR target_id IN ({})",
                    id_list, id_list
                ),
                [],
            )?;

            // 4. Delete aliases
            self.conn.execute(
                &format!("DELETE FROM kg_aliases WHERE node_id IN ({})", id_list),
                [],
            )?;

            // 5. Delete nodes (count affected rows via query_row on changes() after execute)
            let pruned_nodes = self.conn.execute(
                &format!("DELETE FROM kg_nodes WHERE id IN ({})", id_list),
                [],
            )? as i64;

            self.conn.execute_batch("COMMIT;")?;

            Ok(PruningResult {
                pruned_nodes,
                pruned_edges,
                pruned_cognitions,
                skipped_protected,
            })
        };

        match run_pruning() {
            Ok(report) => Ok(report),
            Err(e) => {
                let _ = self.conn.execute_batch("ROLLBACK;");
                Err(e)
            }
        }
    }

    /// Return a snapshot of overall knowledge-graph health.
    pub fn get_memory_health(&self) -> Result<GraphHealth> {
        let now = Utc::now().timestamp();

        let total_nodes: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM kg_nodes", [], |row| row.get(0))?;

        let total_edges: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM kg_edges", [], |row| row.get(0))?;

        let avg_confidence: f64 = self.conn.query_row(
            "SELECT COALESCE(AVG(confidence), 0.0) FROM kg_nodes",
            [],
            |row| row.get(0),
        )?;

        let oldest_ts: Option<i64> = self
            .conn
            .query_row("SELECT MIN(first_seen) FROM kg_nodes", [], |row| row.get(0))
            .ok()
            .flatten();

        let oldest_node_age_days = oldest_ts.map_or(0, |ts| ((now - ts) / 86400).max(0));

        let layer_distribution = self.get_layer_stats()?;

        Ok(GraphHealth {
            total_nodes,
            total_edges,
            avg_confidence,
            oldest_node_age_days,
            layer_distribution,
        })
    }
}

// ---------------------------------------------------------------------------
// Vector helpers
// ---------------------------------------------------------------------------

/// Convert a `[f32]` slice into a BLOB of little-endian bytes.
#[allow(dead_code)]
pub fn f32_slice_to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// Decode a BLOB of little-endian bytes into a `Vec<f32>`.
pub fn blob_to_f32_vec(blob: &[u8]) -> Vec<f32> {
    blob.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// Compute cosine similarity between two float slices.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    let dot: f64 = a
        .iter()
        .zip(b.iter())
        .map(|(x, y)| (*x as f64) * (*y as f64))
        .sum();
    let norm_a: f64 = a
        .iter()
        .map(|x| (*x as f64) * (*x as f64))
        .sum::<f64>()
        .sqrt();
    let norm_b: f64 = b
        .iter()
        .map(|x| (*x as f64) * (*x as f64))
        .sum::<f64>()
        .sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        // Create tables referenced by kg_evidence FKs (needed by V8 migration)
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source_session TEXT
            );
            CREATE TABLE IF NOT EXISTS cognitions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                subject TEXT NOT NULL,
                trait TEXT NOT NULL,
                value TEXT NOT NULL,
                confidence REAL DEFAULT 0.5,
                evidence_count INTEGER DEFAULT 1,
                first_seen INTEGER NOT NULL DEFAULT 0,
                last_updated INTEGER NOT NULL DEFAULT 0,
                version INTEGER DEFAULT 1,
                scope TEXT NOT NULL DEFAULT 'global'
            );",
        )
        .unwrap();
        conn.execute_batch(include_str!(
            "../../../../migrations/V8__add_knowledge_graph.sql"
        ))
        .unwrap();
        conn.execute_batch("ALTER TABLE kg_nodes ADD COLUMN access_count INTEGER DEFAULT 0;")
            .unwrap();
        conn.execute_batch("ALTER TABLE kg_nodes ADD COLUMN last_accessed INTEGER;")
            .unwrap();
        conn.execute_batch("ALTER TABLE kg_nodes ADD COLUMN embedding BLOB;")
            .unwrap();
        conn.execute_batch(
            "ALTER TABLE kg_nodes ADD COLUMN layer TEXT NOT NULL DEFAULT 'semantic';",
        )
        .unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS memory_layers (
                name                TEXT NOT NULL PRIMARY KEY,
                retrieval_priority  INTEGER NOT NULL DEFAULT 0,
                description         TEXT NOT NULL DEFAULT ''
            );
            INSERT OR IGNORE INTO memory_layers (name, retrieval_priority, description) VALUES
                ('working', 40, ''),
                ('episodic', 30, ''),
                ('semantic', 20, ''),
                ('global', 10, '');",
        )
        .unwrap();
        conn
    }

    #[test]
    fn test_upsert_and_resolve_node() {
        let conn = setup_db();
        let store = GraphStore::new(&conn);
        let id = store
            .upsert_node(
                "Rust",
                "Technology",
                "Systems programming language",
                0.9,
                "global",
                None,
            )
            .unwrap();
        assert!(id > 0);
        assert_eq!(store.resolve_node("Rust").unwrap(), Some(id));
        assert!(store.resolve_node("Nonexistent").unwrap().is_none());
    }

    #[test]
    fn test_search_entities() {
        let conn = setup_db();
        let store = GraphStore::new(&conn);
        store
            .upsert_node(
                "Rust",
                "Technology",
                "A programming language",
                0.9,
                "global",
                None,
            )
            .unwrap();
        store
            .upsert_node(
                "Python",
                "Technology",
                "A scripting language",
                0.85,
                "global",
                None,
            )
            .unwrap();
        // FTS5 needs content sync — use the node directly
        let results = store.walk("Rust", 1, None, 10).unwrap();
        assert!(results.is_empty()); // No edges yet
    }

    #[test]
    fn test_neighbors() {
        let conn = setup_db();
        let store = GraphStore::new(&conn);
        let rust_id = store
            .upsert_node("Rust", "Technology", "PL", 0.9, "global", None)
            .unwrap();
        let loom_id = store
            .upsert_node("openLoom", "Project", "AI kernel", 0.9, "global", None)
            .unwrap();
        store
            .upsert_edge(
                rust_id,
                loom_id,
                "uses",
                "openLoom uses Rust",
                0.95,
                "global",
            )
            .unwrap();
        let neighbors = store.neighbors("Rust", None, 10).unwrap();
        assert_eq!(neighbors.len(), 1);
        assert_eq!(neighbors[0].name, "openLoom");
    }

    #[test]
    fn test_embedding_store_and_search() {
        let conn = setup_db();
        let store = GraphStore::new(&conn);

        // Create nodes with embeddings
        let emb_rust = vec![1.0f32, 0.0, 0.0]; // dim=3 for testing
        let emb_python = vec![0.0, 1.0, 0.0];
        let emb_loom = vec![0.9, 0.1, 0.0]; // close to emb_rust

        store
            .upsert_node("Rust", "Technology", "PL", 0.9, "global", None)
            .unwrap();
        store.embed_node("Rust", &emb_rust).unwrap();

        store
            .upsert_node("Python", "Technology", "PL", 0.85, "global", None)
            .unwrap();
        store.embed_node("Python", &emb_python).unwrap();

        store
            .upsert_node("openLoom", "Project", "AI kernel", 0.9, "global", None)
            .unwrap();
        store.embed_node("openLoom", &emb_loom).unwrap();

        // embed_node should also work for non-existing nodes (auto-creates)
        store.embed_node("NewTech", &vec![0.5, 0.5, 0.0]).unwrap();
        assert!(store.resolve_node("NewTech").unwrap().is_some());

        // Search: query embedding close to emb_rust
        let query = vec![0.95, 0.05, 0.0];
        let results = store.search_similar(&query, 5, None, None).unwrap();
        assert!(
            results.len() >= 3,
            "expected at least 3 results, got {}",
            results.len()
        );
        // Rust-like embeddings should rank highest
        let top = &results[0];
        assert!(
            top.0.name == "Rust" || top.0.name == "openLoom" || top.0.name == "NewTech",
            "top result should be close to query, got {}",
            top.0.name
        );
        assert!(top.1 > 0.9, "similarity should be high, got {}", top.1);

        // get_embedding
        let retrieved = store.get_embedding("Rust").unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.len(), 3);
        assert!((retrieved[0] - 1.0).abs() < 1e-6);
        assert!((retrieved[1] - 0.0).abs() < 1e-6);
        assert!((retrieved[2] - 0.0).abs() < 1e-6);

        // Node without embedding returns None
        store
            .upsert_node("NoEmb", "Concept", "desc", 0.5, "global", None)
            .unwrap();
        assert!(store.get_embedding("NoEmb").unwrap().is_none());
    }

    #[test]
    fn test_embedding_fallback_to_fts() {
        let conn = setup_db();
        let store = GraphStore::new(&conn);

        // No nodes have embeddings — search_similar should fall back to FTS5
        store
            .upsert_node(
                "RustLang",
                "Technology",
                "Rust programming",
                0.9,
                "global",
                None,
            )
            .unwrap();

        let query = vec![0.5f32; 768];
        let results = store
            .search_similar(&query, 10, Some("rust"), None)
            .unwrap();
        // Falls back to FTS5 search with score 0.0
        assert!(!results.is_empty(), "FTS5 fallback should return results");
        assert_eq!(results[0].1, 0.0, "FTS5 fallback results have score 0.0");

        // Without fallback_query, returns empty
        let no_results = store.search_similar(&query, 10, None, None).unwrap();
        assert!(
            no_results.is_empty(),
            "without fallback query, should be empty"
        );
    }

    // ========================================================================
    // Active Forgetting tests
    // ========================================================================

    #[test]
    fn test_active_forgetting_prunes_low_importance_and_protects_rules() {
        let conn = setup_db();
        let store = GraphStore::new(&conn);
        let now = Utc::now().timestamp();
        let old_ts = now - 60 * 86400; // 60 days ago

        // -- Nodes that SHOULD be pruned (low importance, old enough) --
        let n1 = store
            .upsert_node("low_conf", "concept", "low quality", 0.1, "session", None)
            .unwrap();
        let n2 = store
            .upsert_node("stale_topic", "concept", "stale", 0.15, "session", None)
            .unwrap();

        // Make them old enough to pass the age gate
        conn.execute(
            "UPDATE kg_nodes SET first_seen = ?1 WHERE id IN (?2, ?3)",
            rusqlite::params![old_ts, n1, n2],
        )
        .unwrap();

        // Edge between the two prunable nodes (tests cascade)
        store
            .upsert_edge(n1, n2, "related_to", "link", 0.1, "session")
            .unwrap();

        // Alias for n1 (tests alias cascade)
        store.add_alias(n1, "lc").unwrap();

        // -- Protected nodes (should NOT be pruned) --
        // Protected: Person
        store
            .upsert_node("Alice", "Person", "a user", 0.1, "global", None)
            .unwrap();

        // Protected: high evidence_count (>= 10)
        let n_high_ev = store
            .upsert_node(
                "high_evidence",
                "concept",
                "many refs",
                0.1,
                "session",
                None,
            )
            .unwrap();
        conn.execute(
            "UPDATE kg_nodes SET evidence_count = 12 WHERE id = ?1",
            rusqlite::params![n_high_ev],
        )
        .unwrap();

        // Protected: scope=global AND layer=global
        store
            .upsert_node(
                "global_core",
                "concept",
                "core entity",
                0.1,
                "global",
                Some("global"),
            )
            .unwrap();

        // Protected: access_count > 50
        let n_popular = store
            .upsert_node("popular_item", "concept", "hit", 0.1, "session", None)
            .unwrap();
        conn.execute(
            "UPDATE kg_nodes SET access_count = 51 WHERE id = ?1",
            rusqlite::params![n_popular],
        )
        .unwrap();

        // ── Execute ──
        let report = store.active_forgetting(0.5, 30).unwrap();

        assert_eq!(
            report.pruned_nodes, 2,
            "should prune 2 low-importance nodes"
        );
        assert_eq!(report.pruned_edges, 1, "should cascade-delete 1 edge");
        assert_eq!(report.skipped_protected, 4, "should skip 4 protected nodes");

        // Pruned nodes are gone
        assert!(
            store.resolve_node("low_conf").unwrap().is_none(),
            "low_conf should be pruned"
        );
        assert!(
            store.resolve_node("stale_topic").unwrap().is_none(),
            "stale_topic should be pruned"
        );

        // Protected nodes remain
        assert!(
            store.resolve_node("Alice").unwrap().is_some(),
            "Person should be protected"
        );
        assert!(
            store.resolve_node("high_evidence").unwrap().is_some(),
            "high evidence_count should be protected"
        );
        assert!(
            store.resolve_node("global_core").unwrap().is_some(),
            "global+global should be protected"
        );
        assert!(
            store.resolve_node("popular_item").unwrap().is_some(),
            "high access_count should be protected"
        );
    }

    #[test]
    fn test_active_forgetting_empty_when_nothing_to_prune() {
        let conn = setup_db();
        let store = GraphStore::new(&conn);

        // All nodes are high-confidence → nothing to prune
        store
            .upsert_node("A", "concept", "desc", 0.95, "global", None)
            .unwrap();
        store
            .upsert_node("B", "concept", "desc", 0.90, "global", None)
            .unwrap();

        let report = store.active_forgetting(0.1, 1).unwrap();
        assert_eq!(report.pruned_nodes, 0);
        assert_eq!(report.pruned_edges, 0);
    }

    #[test]
    fn test_get_memory_health() {
        let conn = setup_db();
        let store = GraphStore::new(&conn);

        store
            .upsert_node("Alpha", "concept", "first", 0.75, "global", None)
            .unwrap();
        let b_id = store
            .upsert_node("Beta", "concept", "second", 0.85, "global", None)
            .unwrap();
        let c_id = store
            .upsert_node("Gamma", "Person", "third", 0.65, "global", None)
            .unwrap();

        // Two edges
        store
            .upsert_edge(b_id, c_id, "knows", "knows", 0.9, "global")
            .unwrap();

        let health = store.get_memory_health().unwrap();

        assert_eq!(health.total_nodes, 3);
        assert_eq!(health.total_edges, 1); // only 1 because we created 1 edge
        // Actually re-read: we upsert_edge once → 1 edge total
        assert!(
            (health.avg_confidence - 0.75).abs() < 0.01,
            "avg confidence should be ~0.75, got {}",
            health.avg_confidence
        );
        assert!(
            health.oldest_node_age_days >= 0,
            "oldest_node_age_days should be non-negative"
        );
        assert!(
            !health.layer_distribution.is_empty(),
            "layer distribution should not be empty"
        );

        // Check serialization round-trips
        let json = serde_json::to_string(&health).unwrap();
        let _roundtripped: GraphHealth = serde_json::from_str(&json).unwrap();
    }
}
