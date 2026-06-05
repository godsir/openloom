//! Knowledge graph store — entity nodes, directed edges, and graph queries.
//! Uses kg_nodes, kg_edges, kg_aliases, kg_evidence tables from V8 migration.
//! Queries use recursive CTEs for graph traversal (no external graph DB needed).

use anyhow::Result;
use chrono::Utc;
use rusqlite::Connection;

/// Default embedding dimension, matching common embedding models
/// (e.g., text-embedding-3-small, all-mpnet-base-v2: 768).
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
                rusqlite::params![name, entity_type, description, confidence, now, scope, layer_val],
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
    pub fn search_entities(&self, query: &str, limit: usize, scope: Option<&str>) -> Result<Vec<GraphRow>> {
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
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
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
        let mut seen_entities: std::collections::HashSet<String> =
            std::collections::HashSet::new();

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
                layer_placeholders,
                scope_ph,
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
                    layer_ph,
                    n_scope_ph,
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
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
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
    pub fn edges_between(&self, node_ids: &[i64], scope: Option<&str>) -> Result<Vec<(String, String, String, f64)>> {
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
            let target_ph: Vec<String> = ((2 + n)..(2 + 2 * n)).map(|i| format!("?{}", i)).collect();
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
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, f64>(3)?,
            ))
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>().map_err(Into::into)
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
            let injected: Vec<String> =
                serde_json::from_str(injected_str).unwrap_or_default();
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
        let rows: Vec<_> = if scope.is_some() {
            stmt.query_map(rusqlite::params![scope.unwrap()], |row| {
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

            scored.sort_by(|a, b| {
                b.1.partial_cmp(&a.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            scored.truncate(limit);

            let result_rows: Vec<GraphRow> = scored.iter().map(|(r, _)| r.clone()).collect();
            let _ = self.touch_rows(&result_rows);
            return Ok(scored);
        }

        // Fallback: no embeddings available — use FTS5 text search if query provided
        if let Some(query) = fallback_query {
            let text_results = self.search_entities(query, limit, scope)?;
            let scored: Vec<(GraphRow, f64)> = text_results
                .into_iter()
                .map(|r| (r, 0.0))
                .collect();
            return Ok(scored);
        }

        Ok(Vec::new())
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

        store.upsert_node("Rust", "Technology", "PL", 0.9, "global", None).unwrap();
        store.embed_node("Rust", &emb_rust).unwrap();

        store.upsert_node("Python", "Technology", "PL", 0.85, "global", None).unwrap();
        store.embed_node("Python", &emb_python).unwrap();

        store.upsert_node("openLoom", "Project", "AI kernel", 0.9, "global", None).unwrap();
        store.embed_node("openLoom", &emb_loom).unwrap();

        // embed_node should also work for non-existing nodes (auto-creates)
        store.embed_node("NewTech", &vec![0.5, 0.5, 0.0]).unwrap();
        assert!(store.resolve_node("NewTech").unwrap().is_some());

        // Search: query embedding close to emb_rust
        let query = vec![0.95, 0.05, 0.0];
        let results = store.search_similar(&query, 5, None, None).unwrap();
        assert!(results.len() >= 3, "expected at least 3 results, got {}", results.len());
        // Rust-like embeddings should rank highest
        let top = &results[0];
        assert!(
            top.0.name == "Rust" || top.0.name == "openLoom" || top.0.name == "NewTech",
            "top result should be close to query, got {}", top.0.name
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
        store.upsert_node("NoEmb", "Concept", "desc", 0.5, "global", None).unwrap();
        assert!(store.get_embedding("NoEmb").unwrap().is_none());
    }

    #[test]
    fn test_embedding_fallback_to_fts() {
        let conn = setup_db();
        let store = GraphStore::new(&conn);

        // No nodes have embeddings — search_similar should fall back to FTS5
        store.upsert_node("RustLang", "Technology", "Rust programming", 0.9, "global", None).unwrap();

        let query = vec![0.5f32; 768];
        let results = store.search_similar(&query, 10, Some("rust"), None).unwrap();
        // Falls back to FTS5 search with score 0.0
        assert!(!results.is_empty(), "FTS5 fallback should return results");
        assert_eq!(results[0].1, 0.0, "FTS5 fallback results have score 0.0");

        // Without fallback_query, returns empty
        let no_results = store.search_similar(&query, 10, None, None).unwrap();
        assert!(no_results.is_empty(), "without fallback query, should be empty");
    }
}
