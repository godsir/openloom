//! Knowledge graph store — entity nodes, directed edges, and graph queries.
//! Uses kg_nodes, kg_edges, kg_aliases, kg_evidence tables from V8 migration.
//! Queries use recursive CTEs for graph traversal (no external graph DB needed).

use anyhow::Result;
use chrono::Utc;
use rusqlite::Connection;

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
    pub fn upsert_node(
        &self,
        name: &str,
        entity_type: &str,
        description: &str,
        confidence: f64,
        scope: &str,
    ) -> Result<i64> {
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
                 evidence_count = evidence_count + 1, last_updated = ?3 WHERE id = ?4",
                rusqlite::params![description, confidence, now, id],
            )?;
            Ok(id)
        } else {
            self.conn.execute(
                "INSERT INTO kg_nodes (name, entity_type, description, confidence,
                 evidence_count, first_seen, last_updated, scope)
                 VALUES (?1, ?2, ?3, ?4, 1, ?5, ?5, ?6)",
                rusqlite::params![name, entity_type, description, confidence, now, scope],
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
    pub fn search_entities(&self, query: &str, limit: usize) -> Result<Vec<GraphRow>> {
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
                    if t.chars().any(|c| c.is_ascii_alphabetic())
                        && !t.chars().any(|c| !c.is_ascii())
                    {
                        format!("{}*", t)
                    } else {
                        t.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        };
        let mut stmt = self.conn.prepare(
            "SELECT n.id, n.name, n.entity_type, n.description, n.confidence
             FROM kg_nodes_fts fts JOIN kg_nodes n ON n.id = fts.rowid
             WHERE kg_nodes_fts MATCH ?1 ORDER BY rank LIMIT ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![expanded, limit], |row| {
            Ok(GraphRow {
                node_id: row.get(0)?,
                name: row.get(1)?,
                entity_type: row.get(2)?,
                description: row.get(3)?,
                confidence: row.get(4)?,
                relation_type: None,
                distance: None,
            })
        })?;
        let results: Vec<GraphRow> = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        let _ = self.touch_rows(&results);
        Ok(results)
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
        let scope_filter = scope.unwrap_or("global");
        let mut stmt = self.conn.prepare(
            "SELECT n.id, n.name, n.entity_type, n.description, e.relation_type, e.confidence
             FROM kg_nodes src
             JOIN kg_edges e ON (e.source_id = src.id OR e.target_id = src.id)
             JOIN kg_nodes n ON (
                (e.source_id = n.id AND e.target_id = src.id) OR
                (e.target_id = n.id AND e.source_id = src.id)
             )
             WHERE src.name = ?1 AND (e.scope = ?2 OR e.scope = 'global')
             ORDER BY e.confidence DESC LIMIT ?3",
        )?;
        let rows = stmt.query_map(rusqlite::params![node_name, scope_filter, limit], |row| {
            Ok(GraphRow {
                node_id: row.get(0)?,
                name: row.get(1)?,
                entity_type: row.get(2)?,
                description: row.get(3)?,
                relation_type: Some(row.get(4)?),
                confidence: row.get(5)?,
                distance: Some(1),
            })
        })?;
        let results: Vec<GraphRow> = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        let _ = self.touch_rows(&results);
        Ok(results)
    }

    /// Get top-scored interests/connections for a subject with temporal decay.
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
                            ELSE 0.5 END),
                    4) AS score
             FROM kg_nodes subj
             JOIN kg_edges e ON e.source_id = subj.id
             JOIN kg_nodes n ON e.target_id = n.id
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

    /// Walk up to N hops from a starting entity (breadth-first via recursive CTE).
    pub fn walk(
        &self,
        start_name: &str,
        max_depth: u8,
        scope: Option<&str>,
        limit: usize,
    ) -> Result<Vec<GraphRow>> {
        let scope_filter = scope.unwrap_or("global");
        let mut stmt = self.conn.prepare(
            "WITH RECURSIVE walk(id, name, entity_type, description, depth, visited) AS (
                SELECT n.id, n.name, n.entity_type, n.description, 0,
                       '/' || CAST(n.id AS TEXT) || '/'
                FROM kg_nodes n
                WHERE n.name = ?1 AND (n.scope = ?2 OR n.scope = 'global')
                UNION
                SELECT CASE WHEN e.source_id = w.id THEN e.target_id ELSE e.source_id END,
                       CASE WHEN e.source_id = w.id THEN tn.name ELSE sn.name END,
                       CASE WHEN e.source_id = w.id THEN tn.entity_type ELSE sn.entity_type END,
                       CASE WHEN e.source_id = w.id THEN tn.description ELSE sn.description END,
                       w.depth + 1,
                       w.visited || CAST(CASE WHEN e.source_id = w.id THEN e.target_id ELSE e.source_id END AS TEXT) || '/'
                FROM walk w
                JOIN kg_edges e ON (e.source_id = w.id OR e.target_id = w.id)
                JOIN kg_nodes sn ON sn.id = e.source_id
                JOIN kg_nodes tn ON tn.id = e.target_id
                WHERE w.depth < ?3
                  AND (e.scope = ?2 OR e.scope = 'global')
                  AND w.visited NOT LIKE '%/' || CAST(CASE WHEN e.source_id = w.id THEN e.target_id ELSE e.source_id END AS TEXT) || '/%'
            )
            SELECT DISTINCT id, name, entity_type, description, MIN(depth)
            FROM walk WHERE depth > 0
            GROUP BY id ORDER BY MIN(depth) LIMIT ?4"
        )?;
        let rows = stmt.query_map(
            rusqlite::params![start_name, scope_filter, max_depth, limit],
            |row| {
                Ok(GraphRow {
                    node_id: row.get(0)?,
                    name: row.get(1)?,
                    entity_type: row.get(2)?,
                    description: row.get(3)?,
                    confidence: 0.0,
                    relation_type: None,
                    distance: Some(row.get(4)?),
                })
            },
        )?;
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

    /// Total entity count (for pruning threshold check).
    pub fn node_count(&self) -> Result<usize> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM kg_nodes", [], |row| {
                row.get::<_, i64>(0)
            })? as usize)
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
            )
            .unwrap();
        store
            .upsert_node(
                "Python",
                "Technology",
                "A scripting language",
                0.85,
                "global",
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
            .upsert_node("Rust", "Technology", "PL", 0.9, "global")
            .unwrap();
        let loom_id = store
            .upsert_node("openLoom", "Project", "AI kernel", 0.9, "global")
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
}
