use std::collections::HashSet;

use rusqlite::{Connection, Row};

use crate::StoreError;

/// A row from the edges table with additional computed fields.
#[derive(Debug, Clone)]
pub struct EdgeRow {
    pub id: String,
    pub from_id: Option<String>,
    pub to_id: Option<String>,
    pub rel: String,
    pub payload: String,
    pub ns: String,
    pub ts: String,
    pub agent: Option<String>,
    pub is_active: bool,
}

/// A row from the spans table.
#[derive(Debug, Clone)]
pub struct SpanRow {
    pub id: String,
    pub path: String,
    pub line_start: Option<u32>,
    pub line_end: Option<u32>,
    pub snippet: Option<String>,
    pub symbol: Option<String>,
    pub content_hash: Option<String>,
    pub commit: Option<String>,
    pub ns: String,
    pub ts: String,
    pub resolution: Option<String>,
    pub recency: Option<String>,
}

/// Column list for span SELECT queries.
const SPAN_COLS: &str = "id, path, line_start, line_end, snippet, symbol, content_hash, [commit], ns, ts, resolution, recency";

/// Column list for edge SELECT queries.
const EDGE_COLS: &str = "id, from_id, to_id, rel, payload, ns, ts, agent, is_active";

/// Map a rusqlite row to a SpanRow (columns must match SPAN_COLS order).
fn row_to_span(row: &Row<'_>) -> rusqlite::Result<SpanRow> {
    Ok(SpanRow {
        id: row.get(0)?,
        path: row.get(1)?,
        line_start: row.get(2)?,
        line_end: row.get(3)?,
        snippet: row.get(4)?,
        symbol: row.get(5)?,
        content_hash: row.get(6)?,
        commit: row.get(7)?,
        ns: row.get(8)?,
        ts: row.get(9)?,
        resolution: row.get(10)?,
        recency: row.get(11)?,
    })
}

/// Map a rusqlite row to an EdgeRow (columns must match EDGE_COLS order).
fn row_to_edge(row: &Row<'_>) -> rusqlite::Result<EdgeRow> {
    Ok(EdgeRow {
        id: row.get(0)?,
        from_id: row.get(1)?,
        to_id: row.get(2)?,
        rel: row.get(3)?,
        payload: row.get(4)?,
        ns: row.get(5)?,
        ts: row.get(6)?,
        agent: row.get(7)?,
        is_active: row.get(8)?,
    })
}

/// Collect rows from a query_map iterator into a Vec, mapping errors.
fn collect_rows<T>(
    rows: rusqlite::Rows<'_>,
    mapper: fn(&Row<'_>) -> rusqlite::Result<T>,
) -> Result<Vec<T>, StoreError> {
    let mut result = Vec::new();
    let mut rows = rows;
    while let Some(row) = rows.next().map_err(|e| StoreError::Io(e.to_string()))? {
        result.push(mapper(row).map_err(|e| StoreError::Io(e.to_string()))?);
    }
    Ok(result)
}

/// Aggregate statistics for `damask status`.
#[derive(Debug, Default)]
pub struct ProjectStats {
    pub span_count: u64,
    pub edge_count: u64,
    pub active_edge_count: u64,
    pub meta_edge_count: u64,
    pub superseded_count: u64,
    pub endorsement_count: u64,
    pub dispute_count: u64,
    pub empty_payload_count: u64,
    pub missing_summary_count: u64,
}

/// Per-namespace health metrics.
#[derive(Debug, Default)]
pub struct NamespaceStats {
    pub edge_count: u64,
    pub last_modified: Option<String>,
    pub endorsement_count: u64,
    pub dispute_count: u64,
}

/// Query interface for the SQLite index.
pub struct IndexQuery<'a> {
    conn: &'a Connection,
}

impl<'a> IndexQuery<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Find all spans that touch a given file and line.
    pub fn spans_at(&self, path: &str, line: u32) -> Result<Vec<SpanRow>, StoreError> {
        let sql = format!(
            "SELECT {SPAN_COLS} FROM spans
             WHERE path = ?1
               AND (line_start IS NULL OR (line_start <= ?2 AND line_end >= ?2))"
        );
        let mut stmt = self
            .conn
            .prepare(&sql)
            .map_err(|e| StoreError::Io(e.to_string()))?;
        let rows = stmt
            .query(rusqlite::params![path, line])
            .map_err(|e| StoreError::Io(e.to_string()))?;
        collect_rows(rows, row_to_span)
    }

    /// Find all spans for a given file (any line).
    pub fn spans_for_file(&self, path: &str) -> Result<Vec<SpanRow>, StoreError> {
        let sql = format!("SELECT {SPAN_COLS} FROM spans WHERE path = ?1");
        let mut stmt = self
            .conn
            .prepare(&sql)
            .map_err(|e| StoreError::Io(e.to_string()))?;
        let rows = stmt
            .query(rusqlite::params![path])
            .map_err(|e| StoreError::Io(e.to_string()))?;
        collect_rows(rows, row_to_span)
    }

    /// Find all active edges that reference a given span ID (as from or to).
    pub fn edges_for_span(&self, span_id: &str) -> Result<Vec<EdgeRow>, StoreError> {
        let sql = format!(
            "SELECT {EDGE_COLS} FROM edges
             WHERE is_active = 1 AND (from_id = ?1 OR to_id = ?1)"
        );
        let mut stmt = self
            .conn
            .prepare(&sql)
            .map_err(|e| StoreError::Io(e.to_string()))?;
        let rows = stmt
            .query(rusqlite::params![span_id])
            .map_err(|e| StoreError::Io(e.to_string()))?;
        collect_rows(rows, row_to_edge)
    }

    /// Count endorsements for a given edge ID.
    /// Meta-edges use from_id = target edge (the edge being endorsed).
    pub fn endorsement_count(&self, edge_id: &str) -> Result<u32, StoreError> {
        let count: u32 = self
            .conn
            .query_row(
                "SELECT COUNT(DISTINCT COALESCE(agent || ':' || session, agent, id))
                 FROM edges WHERE rel = 'endorsed' AND from_id = ?1",
                rusqlite::params![edge_id],
                |row| row.get(0),
            )
            .map_err(|e| StoreError::Io(e.to_string()))?;
        Ok(count)
    }

    /// Count disputes for a given edge ID.
    /// Meta-edges use from_id = target edge (the edge being disputed).
    pub fn dispute_count(&self, edge_id: &str) -> Result<u32, StoreError> {
        let count: u32 = self
            .conn
            .query_row(
                "SELECT COUNT(DISTINCT COALESCE(agent || ':' || session, agent, id))
                 FROM edges WHERE rel = 'disputed' AND from_id = ?1",
                rusqlite::params![edge_id],
                |row| row.get(0),
            )
            .map_err(|e| StoreError::Io(e.to_string()))?;
        Ok(count)
    }

    /// Get the most recent endorsement timestamp for an edge.
    /// Meta-edges use from_id = target edge.
    pub fn latest_endorsement_ts(&self, edge_id: &str) -> Result<Option<String>, StoreError> {
        let result: Option<String> = self
            .conn
            .query_row(
                "SELECT MAX(ts) FROM edges WHERE rel = 'endorsed' AND from_id = ?1",
                rusqlite::params![edge_id],
                |row| row.get(0),
            )
            .map_err(|e| StoreError::Io(e.to_string()))?;
        Ok(result)
    }

    /// Compute aggregate project statistics.
    pub fn project_stats(&self) -> Result<ProjectStats, StoreError> {
        let count = |sql: &str| -> Result<u64, StoreError> {
            self.conn
                .query_row(sql, [], |row| row.get(0))
                .map_err(|e| StoreError::Io(e.to_string()))
        };

        Ok(ProjectStats {
            span_count: count("SELECT COUNT(*) FROM spans")?,
            edge_count: count("SELECT COUNT(*) FROM edges")?,
            active_edge_count: count("SELECT COUNT(*) FROM edges WHERE is_active = 1")?,
            meta_edge_count: count(
                "SELECT COUNT(*) FROM edges WHERE rel IN ('supersedes','invalidates','endorsed','disputed')",
            )?,
            superseded_count: count(
                "SELECT COUNT(*) FROM edges WHERE is_active = 0 AND rel NOT IN ('supersedes','invalidates','endorsed','disputed')",
            )?,
            endorsement_count: count("SELECT COUNT(*) FROM edges WHERE rel = 'endorsed'")?,
            dispute_count: count("SELECT COUNT(*) FROM edges WHERE rel = 'disputed'")?,
            empty_payload_count: count(
                "SELECT COUNT(*) FROM edges WHERE is_active = 1 AND (payload = '{}' OR payload = '') AND rel NOT IN ('supersedes','invalidates','endorsed','disputed')",
            )?,
            missing_summary_count: count(
                "SELECT COUNT(*) FROM edges WHERE is_active = 1 AND payload != '{}' AND payload NOT LIKE '%\"summary\"%' AND rel NOT IN ('supersedes','invalidates','endorsed','disputed')",
            )?,
        })
    }

    /// Return all active content edges (for `where` predicate filtering).
    pub fn all_active_edges(&self) -> Result<Vec<EdgeRow>, StoreError> {
        let sql = format!("SELECT {EDGE_COLS} FROM edges WHERE is_active = 1");
        let mut stmt = self
            .conn
            .prepare(&sql)
            .map_err(|e| StoreError::Io(e.to_string()))?;
        let rows = stmt
            .query([])
            .map_err(|e| StoreError::Io(e.to_string()))?;
        collect_rows(rows, row_to_edge)
    }

    /// Look up a single span by ID.
    pub fn span_by_id(&self, id: &str) -> Result<Option<SpanRow>, StoreError> {
        let sql = format!("SELECT {SPAN_COLS} FROM spans WHERE id = ?1");
        let result = self.conn.query_row(&sql, rusqlite::params![id], row_to_span);

        match result {
            Ok(row) => Ok(Some(row)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(StoreError::Io(e.to_string())),
        }
    }

    /// Look up a single edge by ID.
    pub fn edge_by_id(&self, id: &str) -> Result<Option<EdgeRow>, StoreError> {
        let sql = format!("SELECT {EDGE_COLS} FROM edges WHERE id = ?1");
        let result = self.conn.query_row(&sql, rusqlite::params![id], row_to_edge);

        match result {
            Ok(row) => Ok(Some(row)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(StoreError::Io(e.to_string())),
        }
    }

    /// Find all active edges where from_id matches (outgoing edges).
    pub fn edges_from(&self, id: &str) -> Result<Vec<EdgeRow>, StoreError> {
        let sql = format!(
            "SELECT {EDGE_COLS} FROM edges WHERE is_active = 1 AND from_id = ?1"
        );
        let mut stmt = self
            .conn
            .prepare(&sql)
            .map_err(|e| StoreError::Io(e.to_string()))?;
        let rows = stmt
            .query(rusqlite::params![id])
            .map_err(|e| StoreError::Io(e.to_string()))?;
        collect_rows(rows, row_to_edge)
    }

    /// Find all edges targeting a given ID (for `why` and `blame`).
    /// Includes both active and inactive edges (to show full provenance).
    pub fn edges_targeting(&self, id: &str) -> Result<Vec<EdgeRow>, StoreError> {
        let sql = format!(
            "SELECT {EDGE_COLS} FROM edges WHERE to_id = ?1 ORDER BY ts"
        );
        let mut stmt = self
            .conn
            .prepare(&sql)
            .map_err(|e| StoreError::Io(e.to_string()))?;
        let rows = stmt
            .query(rusqlite::params![id])
            .map_err(|e| StoreError::Io(e.to_string()))?;
        collect_rows(rows, row_to_edge)
    }

    /// Return all edges ordered by timestamp (for `damask log`).
    pub fn all_edges_chronological(&self) -> Result<Vec<EdgeRow>, StoreError> {
        let sql = format!("SELECT {EDGE_COLS} FROM edges ORDER BY ts");
        let mut stmt = self
            .conn
            .prepare(&sql)
            .map_err(|e| StoreError::Io(e.to_string()))?;
        let rows = stmt
            .query([])
            .map_err(|e| StoreError::Io(e.to_string()))?;
        collect_rows(rows, row_to_edge)
    }

    /// Full-text search over edge payloads using FTS5.
    pub fn search_fts(
        &self,
        query: &str,
        ns: Option<&str>,
        rel: Option<&str>,
    ) -> Result<Vec<EdgeRow>, StoreError> {
        let mut sql = format!(
            "SELECT {EDGE_COLS} FROM edges
             JOIN edges_fts ON edges.rowid = edges_fts.rowid
             WHERE edges_fts MATCH ?1 AND edges.is_active = 1"
        );
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(query.to_string())];
        let mut idx = 2;

        if let Some(ns_val) = ns {
            sql.push_str(&format!(" AND edges.ns = ?{idx}"));
            params.push(Box::new(ns_val.to_string()));
            idx += 1;
        }
        if let Some(rel_val) = rel {
            sql.push_str(&format!(" AND edges.rel = ?{idx}"));
            params.push(Box::new(rel_val.to_string()));
        }

        sql.push_str(" ORDER BY rank");

        let mut stmt = self
            .conn
            .prepare(&sql)
            .map_err(|e| StoreError::Io(e.to_string()))?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let rows = stmt
            .query(param_refs.as_slice())
            .map_err(|e| StoreError::Io(e.to_string()))?;
        collect_rows(rows, row_to_edge)
    }

    /// Return namespace-level statistics: edge count, last modified, endorsement/dispute counts.
    pub fn namespace_stats(&self, ns: &str) -> Result<NamespaceStats, StoreError> {
        let edge_count: u64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM edges WHERE ns = ?1 AND is_active = 1",
                rusqlite::params![ns],
                |row| row.get(0),
            )
            .map_err(|e| StoreError::Io(e.to_string()))?;

        let last_modified: Option<String> = self
            .conn
            .query_row(
                "SELECT MAX(ts) FROM edges WHERE ns = ?1",
                rusqlite::params![ns],
                |row| row.get(0),
            )
            .map_err(|e| StoreError::Io(e.to_string()))?;

        let endorsement_count: u64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM edges WHERE ns = ?1 AND rel = 'endorsed'",
                rusqlite::params![ns],
                |row| row.get(0),
            )
            .map_err(|e| StoreError::Io(e.to_string()))?;

        let dispute_count: u64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM edges WHERE ns = ?1 AND rel = 'disputed'",
                rusqlite::params![ns],
                |row| row.get(0),
            )
            .map_err(|e| StoreError::Io(e.to_string()))?;

        Ok(NamespaceStats {
            edge_count,
            last_modified,
            endorsement_count,
            dispute_count,
        })
    }

    /// Return all spans ordered by timestamp.
    pub fn all_spans_chronological(&self) -> Result<Vec<SpanRow>, StoreError> {
        let sql = format!("SELECT {SPAN_COLS} FROM spans ORDER BY ts");
        let mut stmt = self
            .conn
            .prepare(&sql)
            .map_err(|e| StoreError::Io(e.to_string()))?;
        let rows = stmt
            .query([])
            .map_err(|e| StoreError::Io(e.to_string()))?;
        collect_rows(rows, row_to_span)
    }

    /// BFS traversal from a starting ID, returning a tree structure.
    ///
    /// - `start_id`: span or edge ID to start from
    /// - `rel_filter`: optional rel type to restrict traversal
    /// - `max_depth`: maximum traversal depth
    pub fn follow(
        &self,
        start_id: &str,
        rel_filter: Option<&str>,
        max_depth: u32,
    ) -> Result<TraversalNode, StoreError> {
        let mut visited = HashSet::new();
        self.follow_recursive(start_id, rel_filter, max_depth, 0, &mut visited)
    }

    fn follow_recursive(
        &self,
        id: &str,
        rel_filter: Option<&str>,
        max_depth: u32,
        current_depth: u32,
        visited: &mut HashSet<String>,
    ) -> Result<TraversalNode, StoreError> {
        visited.insert(id.to_string());

        // Determine if this is a span or edge
        let (node_kind, display) = if id.starts_with("s_") {
            if let Some(span) = self.span_by_id(id)? {
                let lines = match (span.line_start, span.line_end) {
                    (Some(s), Some(e)) => format!(":{}-{}", s, e),
                    _ => String::new(),
                };
                let snippet = span
                    .snippet
                    .as_deref()
                    .map(|s| format!(" \"{}\"", s))
                    .unwrap_or_default();
                (NodeKind::Span, format!("{}{}{}", span.path, lines, snippet))
            } else {
                (NodeKind::Span, format!("{id} (not found)"))
            }
        } else if id.starts_with("e_") {
            if let Some(edge) = self.edge_by_id(id)? {
                let payload: serde_json::Value =
                    serde_json::from_str(&edge.payload).unwrap_or(serde_json::json!({}));
                let env = damask_core::PayloadEnvelope::new(&payload);
                let summary = env
                    .summary()
                    .unwrap_or_else(|| damask_core::truncate_str(edge.payload.as_str(), 60))
                    .to_string();
                (NodeKind::Edge, format!("[{}] {}", edge.rel, summary))
            } else {
                (NodeKind::Edge, format!("{id} (not found)"))
            }
        } else {
            return Err(StoreError::Io(format!("invalid ID for follow: {id}")));
        };

        let mut children = Vec::new();

        if current_depth < max_depth {
            // Find outgoing edges
            let edges = self.edges_from(id)?;
            for edge in edges {
                // Apply rel filter
                if let Some(filter) = rel_filter {
                    if edge.rel != filter {
                        continue;
                    }
                }

                // Determine the target
                let target = if let Some(ref to_id) = edge.to_id {
                    if visited.contains(to_id.as_str()) {
                        // Cycle: show the ID but don't recurse
                        Some(TraversalNode {
                            id: to_id.clone(),
                            kind: if to_id.starts_with("s_") {
                                NodeKind::Span
                            } else {
                                NodeKind::Edge
                            },
                            display: format!("{to_id} (cycle)"),
                            children: vec![],
                        })
                    } else {
                        Some(self.follow_recursive(
                            to_id,
                            rel_filter,
                            max_depth,
                            current_depth + 1,
                            visited,
                        )?)
                    }
                } else {
                    None // Null target — edge is a leaf
                };

                children.push(TraversalChild { edge, target });
            }
        }

        Ok(TraversalNode {
            id: id.to_string(),
            kind: node_kind,
            display,
            children,
        })
    }
}

/// Kind of node in the traversal tree.
#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind {
    Span,
    Edge,
}

/// A node in the traversal tree returned by `follow`.
#[derive(Debug, Clone)]
pub struct TraversalNode {
    pub id: String,
    pub kind: NodeKind,
    pub display: String,
    pub children: Vec<TraversalChild>,
}

/// A child entry in the traversal tree: an edge pointing to an optional target.
#[derive(Debug, Clone)]
pub struct TraversalChild {
    pub edge: EdgeRow,
    /// The target node. `None` if the edge has a null `to` (leaf edge with payload).
    pub target: Option<TraversalNode>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::schema::create_schema;
    use crate::state::compute_active_state;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        conn
    }

    fn insert_span(conn: &Connection, id: &str, path: &str, start: u32, end: u32) {
        conn.execute(
            "INSERT INTO spans (id, path, line_start, line_end, ns, ts) VALUES (?1, ?2, ?3, ?4, 'test', '2025-01-01T00:00:00Z')",
            rusqlite::params![id, path, start, end],
        )
        .unwrap();
    }

    fn insert_edge(conn: &Connection, id: &str, from: Option<&str>, to: Option<&str>, rel: &str) {
        conn.execute(
            "INSERT INTO edges (id, from_id, to_id, rel, payload, ns, ts) VALUES (?1, ?2, ?3, ?4, '{}', 'test', '2025-01-01T00:00:00Z')",
            rusqlite::params![id, from, to, rel],
        )
        .unwrap();
    }

    #[test]
    fn spans_at_finds_matching() {
        let conn = setup_db();
        insert_span(&conn, "s_1", "src/main.rs", 1, 10);
        insert_span(&conn, "s_2", "src/main.rs", 15, 20);
        insert_span(&conn, "s_3", "src/other.rs", 1, 5);

        let q = IndexQuery::new(&conn);
        let spans = q.spans_at("src/main.rs", 5).unwrap();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].id, "s_1");
    }

    #[test]
    fn edges_for_span_returns_active_only() {
        let conn = setup_db();
        insert_span(&conn, "s_1", "src/main.rs", 1, 10);
        insert_edge(&conn, "e_1", Some("s_1"), None, "risk");
        insert_edge(&conn, "e_2", Some("s_1"), None, "risk");
        insert_edge(&conn, "e_sup", Some("e_2"), Some("e_1"), "supersedes");

        compute_active_state(&conn).unwrap();

        let q = IndexQuery::new(&conn);
        let edges = q.edges_for_span("s_1").unwrap();

        // Only e_2 should be active (e_1 superseded, e_sup is meta)
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].id, "e_2");
    }

    #[test]
    fn all_active_edges_returns_only_active() {
        let conn = setup_db();
        insert_span(&conn, "s_1", "src/main.rs", 1, 10);
        insert_edge(&conn, "e_1", Some("s_1"), None, "risk");
        insert_edge(&conn, "e_2", Some("s_1"), None, "describes");

        // Deactivate e_1
        conn.execute("UPDATE edges SET is_active = 0 WHERE id = 'e_1'", [])
            .unwrap();

        let q = IndexQuery::new(&conn);
        let edges = q.all_active_edges().unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].id, "e_2");
    }

    #[test]
    fn span_by_id_found() {
        let conn = setup_db();
        insert_span(&conn, "s_1", "src/main.rs", 1, 10);

        let q = IndexQuery::new(&conn);
        let span = q.span_by_id("s_1").unwrap();
        assert!(span.is_some());
        assert_eq!(span.unwrap().path, "src/main.rs");
    }

    #[test]
    fn span_by_id_not_found() {
        let conn = setup_db();
        let q = IndexQuery::new(&conn);
        assert!(q.span_by_id("s_nonexistent").unwrap().is_none());
    }

    #[test]
    fn follow_simple_tree() {
        let conn = setup_db();
        insert_span(&conn, "s_1", "src/main.rs", 1, 10);
        insert_span(&conn, "s_2", "src/other.rs", 1, 5);
        insert_edge(&conn, "e_1", Some("s_1"), Some("s_2"), "depends_on");
        insert_edge(&conn, "e_2", Some("s_1"), None, "risk");

        let q = IndexQuery::new(&conn);
        let tree = q.follow("s_1", None, 2).unwrap();

        assert_eq!(tree.id, "s_1");
        assert_eq!(tree.kind, NodeKind::Span);
        assert_eq!(tree.children.len(), 2);
    }

    #[test]
    fn follow_with_rel_filter() {
        let conn = setup_db();
        insert_span(&conn, "s_1", "src/main.rs", 1, 10);
        insert_span(&conn, "s_2", "src/other.rs", 1, 5);
        insert_edge(&conn, "e_1", Some("s_1"), Some("s_2"), "depends_on");
        insert_edge(&conn, "e_2", Some("s_1"), None, "risk");

        let q = IndexQuery::new(&conn);
        let tree = q.follow("s_1", Some("risk"), 2).unwrap();

        assert_eq!(tree.children.len(), 1);
        assert_eq!(tree.children[0].edge.rel, "risk");
    }

    #[test]
    fn follow_respects_depth_limit() {
        let conn = setup_db();
        insert_span(&conn, "s_1", "a.rs", 1, 1);
        insert_span(&conn, "s_2", "b.rs", 1, 1);
        insert_span(&conn, "s_3", "c.rs", 1, 1);
        insert_edge(&conn, "e_1", Some("s_1"), Some("s_2"), "depends_on");
        insert_edge(&conn, "e_2", Some("s_2"), Some("s_3"), "depends_on");

        let q = IndexQuery::new(&conn);

        // Depth 1: should see s_2 but not recurse into its children
        let tree = q.follow("s_1", None, 1).unwrap();
        assert_eq!(tree.children.len(), 1);
        let target = tree.children[0].target.as_ref().unwrap();
        assert_eq!(target.id, "s_2");
        assert_eq!(target.children.len(), 0); // depth limit reached
    }

    #[test]
    fn follow_handles_cycles() {
        let conn = setup_db();
        insert_span(&conn, "s_1", "a.rs", 1, 1);
        insert_span(&conn, "s_2", "b.rs", 1, 1);
        insert_edge(&conn, "e_1", Some("s_1"), Some("s_2"), "depends_on");
        insert_edge(&conn, "e_2", Some("s_2"), Some("s_1"), "depends_on");

        let q = IndexQuery::new(&conn);
        let tree = q.follow("s_1", None, 5).unwrap();

        // Should not infinite loop; s_1 marked as visited
        assert_eq!(tree.children.len(), 1);
        let target = tree.children[0].target.as_ref().unwrap();
        assert_eq!(target.id, "s_2");
        // s_2's edge back to s_1 should show "(cycle)"
        assert_eq!(target.children.len(), 1);
        let cycle_target = target.children[0].target.as_ref().unwrap();
        assert!(cycle_target.display.contains("cycle"));
    }

    #[test]
    fn endorsement_counting() {
        let conn = setup_db();
        insert_edge(&conn, "e_1", Some("s_1"), None, "risk");
        // Meta-edges use from_id = target edge being endorsed
        conn.execute(
            "INSERT INTO edges (id, from_id, to_id, rel, payload, ns, ts, agent, session)
             VALUES ('e_end1', 'e_1', NULL, 'endorsed', '{}', 'test', '2025-01-02T00:00:00Z', 'agent-a', 'sess1')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO edges (id, from_id, to_id, rel, payload, ns, ts, agent, session)
             VALUES ('e_end2', 'e_1', NULL, 'endorsed', '{}', 'test', '2025-01-03T00:00:00Z', 'agent-b', 'sess2')",
            [],
        ).unwrap();

        let q = IndexQuery::new(&conn);
        assert_eq!(q.endorsement_count("e_1").unwrap(), 2);
        assert_eq!(q.dispute_count("e_1").unwrap(), 0);
    }
}
