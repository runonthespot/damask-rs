use std::path::{Path, PathBuf};

use rusqlite::Connection;
use walkdir::WalkDir;

use damask_core::Fact;
use damask_resolve::{resolve_span, SpanAnchor};

use crate::index::schema::create_schema;
use crate::jsonl::FactReader;
use crate::state::compute_active_state;
use crate::StoreError;

/// Controls which JSONL sources the index should read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexMode {
    /// Read raw append-only logs only (ignore `.views/`).
    FullLog,
    /// Prefer `.views/<ns>.current.jsonl` when present, falling back to raw logs.
    ViewsPreferred,
}

/// Rebuild the entire SQLite index from JSONL files.
pub fn rebuild_index(db_path: &Path, edges_dir: &Path) -> Result<Connection, StoreError> {
    rebuild_index_with_mode(db_path, edges_dir, IndexMode::FullLog)
}

/// Rebuild the entire SQLite index from JSONL files using the given mode.
pub fn rebuild_index_with_mode(
    db_path: &Path,
    edges_dir: &Path,
    mode: IndexMode,
) -> Result<Connection, StoreError> {
    // Remove existing DB if present
    if db_path.exists() {
        std::fs::remove_file(db_path).map_err(|e| StoreError::Io(e.to_string()))?;
    }

    let conn = Connection::open(db_path).map_err(|e| StoreError::Io(e.to_string()))?;
    create_schema(&conn)?;

    // Derive project root: edges_dir is .damask/edges, so root is two levels up
    let project_root = edges_dir
        .parent()
        .and_then(|p| p.parent())
        .unwrap_or(edges_dir);

    // Read all facts from all JSONL files
    let jsonl_files = list_jsonl_files(edges_dir, mode);
    let all_facts = read_facts_from_files(&jsonl_files)?;
    insert_facts(&conn, &all_facts, project_root)?;

    // Store mtimes for incremental updates
    store_file_mtimes(&conn, &jsonl_files)?;

    // Compute active state
    compute_active_state(&conn)?;

    Ok(conn)
}

/// Incrementally update the index — only re-read JSONL files that changed.
/// Falls back to full rebuild if the DB doesn't exist or schema is missing.
pub fn update_index(db_path: &Path, edges_dir: &Path) -> Result<Connection, StoreError> {
    update_index_with_mode(db_path, edges_dir, IndexMode::FullLog)
}

/// Incrementally update the index — only re-read JSONL files that changed.
/// Falls back to full rebuild if the DB doesn't exist or schema is missing.
pub fn update_index_with_mode(
    db_path: &Path,
    edges_dir: &Path,
    mode: IndexMode,
) -> Result<Connection, StoreError> {
    if !db_path.exists() {
        return rebuild_index_with_mode(db_path, edges_dir, mode);
    }

    let conn = Connection::open(db_path).map_err(|e| StoreError::Io(e.to_string()))?;

    // Verify schema exists and is current — if not, full rebuild.
    // Check for index_meta table AND source_file column on spans (added in current schema).
    if !schema_is_current(&conn) {
        drop(conn);
        return rebuild_index_with_mode(db_path, edges_dir, mode);
    }

    let jsonl_files = list_jsonl_files(edges_dir, mode);
    let changed_files = find_changed_files(&conn, &jsonl_files)?;

    // Detect deleted JSONL files before the early-return: mtime entries whose
    // paths no longer exist on disk. Must run even when no files changed.
    let deleted_files = find_deleted_files(&conn, &jsonl_files)?;

    if changed_files.is_empty() && deleted_files.is_empty() {
        // Nothing changed and nothing deleted — return existing index
        return Ok(conn);
    }

    // Derive project root
    let project_root = edges_dir
        .parent()
        .and_then(|p| p.parent())
        .unwrap_or(edges_dir);

    // Purge rows from deleted files
    if !deleted_files.is_empty() {
        delete_facts_for_files(&conn, &deleted_files)?;
        remove_file_mtimes(&conn, &deleted_files)?;
    }

    // Delete rows from changed files before re-inserting (prevents ghost rows)
    if !changed_files.is_empty() {
        delete_facts_for_files(&conn, &changed_files)?;

        // Re-read only changed files and insert their facts
        let new_facts = read_facts_from_files(&changed_files)?;
        insert_facts(&conn, &new_facts, project_root)?;

        // Update mtimes for changed files
        store_file_mtimes(&conn, &changed_files)?;
    }

    // Recompute active state (cheap — just 3 SQL statements)
    compute_active_state(&conn)?;

    Ok(conn)
}

/// List JSONL files in the edges directory based on index mode.
fn list_jsonl_files(edges_dir: &Path, mode: IndexMode) -> Vec<std::path::PathBuf> {
    if !edges_dir.exists() {
        return Vec::new();
    }

    let entries = WalkDir::new(edges_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension().is_some_and(|ext| ext == "jsonl") && e.file_type().is_file()
        })
        .filter(|e| {
            !e.path()
                .components()
                .any(|c| c.as_os_str() == ".private" || c.as_os_str() == ".local")
        });

    match mode {
        IndexMode::FullLog => entries
            .filter(|e| {
                !e.path()
                    .components()
                    .any(|c| c.as_os_str() == ".views")
            })
            .map(|e| e.into_path())
            .collect(),
        IndexMode::ViewsPreferred => {
            let mut views_by_ns: std::collections::HashMap<String, PathBuf> =
                std::collections::HashMap::new();
            let mut raw_by_ns: std::collections::HashMap<String, PathBuf> =
                std::collections::HashMap::new();

            for entry in entries {
                let path = entry.path();
                let is_view = path
                    .components()
                    .any(|c| c.as_os_str() == ".views");

                if is_view {
                    if let Some(ns) = view_namespace(path) {
                        views_by_ns.insert(ns, path.to_path_buf());
                    }
                    continue;
                }

                if let Some(ns) = raw_namespace(path) {
                    raw_by_ns.insert(ns, path.to_path_buf());
                }
            }

            let mut files = Vec::new();
            for path in views_by_ns.values() {
                files.push(path.clone());
            }
            for (ns, path) in raw_by_ns {
                if !views_by_ns.contains_key(&ns) {
                    files.push(path);
                }
            }

            files.sort_by(|a, b| a.display().to_string().cmp(&b.display().to_string()));
            files
        }
    }
}

/// Namespace for a view file `.views/<ns>.current.jsonl`.
fn view_namespace(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_string_lossy();
    if !name.ends_with(".current.jsonl") {
        return None;
    }
    let ns = name.trim_end_matches(".current.jsonl");
    if ns.is_empty() {
        None
    } else {
        Some(ns.to_string())
    }
}

/// Namespace for a raw log file `<ns>.jsonl`.
fn raw_namespace(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_string_lossy();
    if !name.ends_with(".jsonl") {
        return None;
    }
    let ns = name.trim_end_matches(".jsonl");
    if ns.is_empty() {
        None
    } else {
        Some(ns.to_string())
    }
}

/// Find which JSONL files have changed since the last index build.
fn find_changed_files(
    conn: &Connection,
    jsonl_files: &[std::path::PathBuf],
) -> Result<Vec<std::path::PathBuf>, StoreError> {
    let mut changed = Vec::new();

    for path in jsonl_files {
        let path_str = path.display().to_string();

        let current_mtime = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .map(|t| {
                t.duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
            })
            .unwrap_or(0);

        let stored_mtime: Option<u64> = conn
            .query_row(
                "SELECT CAST(value AS INTEGER) FROM index_meta WHERE key = ?1",
                rusqlite::params![format!("mtime:{}", path_str)],
                |row| row.get(0),
            )
            .ok();

        if stored_mtime != Some(current_mtime) {
            changed.push(path.clone());
        }
    }

    Ok(changed)
}

/// Store file mtimes in the index_meta table for future incremental checks.
fn store_file_mtimes(
    conn: &Connection,
    files: &[std::path::PathBuf],
) -> Result<(), StoreError> {
    let mut stmt = conn
        .prepare("INSERT OR REPLACE INTO index_meta (key, value) VALUES (?1, ?2)")
        .map_err(|e| StoreError::Io(e.to_string()))?;

    for path in files {
        let path_str = path.display().to_string();
        let mtime = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .map(|t| {
                t.duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
            })
            .unwrap_or(0);

        stmt.execute(rusqlite::params![
            format!("mtime:{}", path_str),
            mtime.to_string()
        ])
        .map_err(|e| StoreError::Io(e.to_string()))?;
    }

    Ok(())
}

/// Check that the DB has the current schema: index_meta table exists and
/// the spans table has the source_file column. Returns false if the DB
/// needs a full rebuild (missing tables or outdated schema).
fn schema_is_current(conn: &Connection) -> bool {
    let has_index_meta: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='index_meta'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(false);

    if !has_index_meta {
        return false;
    }

    // Check that both spans and edges have the source_file column (added in current schema)
    let has_spans_source: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('spans') WHERE name='source_file'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(false);

    let has_edges_source: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('edges') WHERE name='source_file'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(false);

    // Check that edges_fts virtual table exists (needed for search)
    let has_fts: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='edges_fts'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(false);

    has_spans_source && has_edges_source && has_fts
}

/// Delete all spans and edges that originated from the given source files.
fn delete_facts_for_files(conn: &Connection, files: &[PathBuf]) -> Result<(), StoreError> {
    let mut del_spans = conn
        .prepare("DELETE FROM spans WHERE source_file = ?1")
        .map_err(|e| StoreError::Io(e.to_string()))?;
    let mut del_edges = conn
        .prepare("DELETE FROM edges WHERE source_file = ?1")
        .map_err(|e| StoreError::Io(e.to_string()))?;

    for path in files {
        let path_str = path.display().to_string();
        del_spans
            .execute(rusqlite::params![path_str])
            .map_err(|e| StoreError::Io(e.to_string()))?;
        del_edges
            .execute(rusqlite::params![path_str])
            .map_err(|e| StoreError::Io(e.to_string()))?;
    }

    Ok(())
}

/// Find JSONL files tracked in index_meta that no longer exist on disk.
fn find_deleted_files(
    conn: &Connection,
    current_files: &[PathBuf],
) -> Result<Vec<PathBuf>, StoreError> {
    let current_set: std::collections::HashSet<String> =
        current_files.iter().map(|p| p.display().to_string()).collect();

    let mut stmt = conn
        .prepare("SELECT key FROM index_meta WHERE key LIKE 'mtime:%'")
        .map_err(|e| StoreError::Io(e.to_string()))?;

    let rows = stmt
        .query_map([], |row| {
            let key: String = row.get(0)?;
            Ok(key)
        })
        .map_err(|e| StoreError::Io(e.to_string()))?;

    let mut deleted = Vec::new();
    for row in rows {
        let key = row.map_err(|e| StoreError::Io(e.to_string()))?;
        if let Some(path_str) = key.strip_prefix("mtime:") {
            if !current_set.contains(path_str) {
                deleted.push(PathBuf::from(path_str));
            }
        }
    }

    Ok(deleted)
}

/// Remove mtime entries for files that no longer exist.
fn remove_file_mtimes(conn: &Connection, files: &[PathBuf]) -> Result<(), StoreError> {
    let mut stmt = conn
        .prepare("DELETE FROM index_meta WHERE key = ?1")
        .map_err(|e| StoreError::Io(e.to_string()))?;

    for path in files {
        let key = format!("mtime:{}", path.display());
        stmt.execute(rusqlite::params![key])
            .map_err(|e| StoreError::Io(e.to_string()))?;
    }

    Ok(())
}

/// Read all facts from a list of JSONL files, tracking which file each came from.
fn read_facts_from_files(files: &[PathBuf]) -> Result<Vec<(Fact, PathBuf)>, StoreError> {
    let mut all_facts = Vec::new();

    for path in files {
        let mut reader = FactReader::open(path)?;
        let facts = reader.read_all()?;
        for fact in facts {
            all_facts.push((fact, path.clone()));
        }
    }

    Ok(all_facts)
}

/// Insert facts into the SQLite database, resolving spans against the project root.
/// Each fact is paired with the source JSONL file path for provenance tracking.
fn insert_facts(
    conn: &Connection,
    facts: &[(Fact, PathBuf)],
    project_root: &Path,
) -> Result<(), StoreError> {
    let mut span_stmt = conn
        .prepare(
            "INSERT OR REPLACE INTO spans (id, path, line_start, line_end, snippet, symbol, content_hash, [commit], ns, ts, agent, session, resolution, recency, source_file)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        )
        .map_err(|e| StoreError::Io(e.to_string()))?;

    let mut edge_stmt = conn
        .prepare(
            "INSERT OR REPLACE INTO edges (id, from_id, to_id, rel, payload, ns, ts, agent, session, source_file)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )
        .map_err(|e| StoreError::Io(e.to_string()))?;

    for (fact, source_path) in facts {
        let source_file_str = source_path.display().to_string();
        match fact {
            Fact::Span(s) => {
                // Resolve the span against current file system state
                let anchor = SpanAnchor {
                    path: s.path.clone(),
                    line_start: s.lines.map(|l| l[0]),
                    line_end: s.lines.map(|l| l[1]),
                    content_hash: s.content_hash.clone(),
                    symbol: s.symbol.clone(),
                    snippet: s.snippet.clone(),
                    commit: s.commit.clone(),
                };

                let (resolution_str, recency_str, effective_line_start, effective_line_end, effective_path) =
                    match resolve_span(project_root, &anchor) {
                        Ok(result) => {
                            let res = match result.freshness.resolution {
                                damask_core::Resolution::Exact => "exact",
                                damask_core::Resolution::Relocated => "relocated",
                                damask_core::Resolution::Unresolved => "unresolved",
                                damask_core::Resolution::Missing => "missing",
                            };
                            let rec = match result.freshness.recency {
                                damask_core::Recency::Unchanged => "unchanged",
                                damask_core::Recency::FileChanged => "file_changed",
                                damask_core::Recency::Unknown => "unknown",
                            };
                            // Use relocated lines if available, otherwise original
                            let (ls, le) = match result.new_lines {
                                Some((new_start, new_end)) => {
                                    (Some(new_start), Some(new_end))
                                }
                                None => (s.lines.map(|l| l[0]), s.lines.map(|l| l[1])),
                            };
                            // Use renamed path if the file was git-renamed
                            let path = result.new_path.unwrap_or_else(|| s.path.clone());
                            (Some(res.to_string()), Some(rec.to_string()), ls, le, path)
                        }
                        Err(_) => (
                            None,
                            None,
                            s.lines.map(|l| l[0]),
                            s.lines.map(|l| l[1]),
                            s.path.clone(),
                        ),
                    };

                span_stmt
                    .execute(rusqlite::params![
                        s.id.as_str(),
                        effective_path,
                        effective_line_start,
                        effective_line_end,
                        s.snippet,
                        s.symbol,
                        s.content_hash,
                        s.commit,
                        s.ns,
                        s.ts.to_rfc3339(),
                        s.agent,
                        s.session,
                        resolution_str,
                        recency_str,
                        source_file_str,
                    ])
                    .map_err(|e| StoreError::Io(e.to_string()))?;
            }
            Fact::Edge(e) => {
                let from_str = e.from.as_ref().map(|id| id.to_string());
                let to_str = e.to.as_ref().map(|id| id.to_string());
                let payload_json = serde_json::to_string(&e.payload).map_err(StoreError::Json)?;

                edge_stmt
                    .execute(rusqlite::params![
                        e.id.as_str(),
                        from_str,
                        to_str,
                        e.rel,
                        payload_json,
                        e.ns,
                        e.ts.to_rfc3339(),
                        e.agent,
                        e.session,
                        source_file_str,
                    ])
                    .map_err(|e| StoreError::Io(e.to_string()))?;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::schema::create_schema;

    #[test]
    fn schema_is_current_with_full_schema() {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        assert!(schema_is_current(&conn));
    }

    #[test]
    fn schema_is_current_rejects_empty_db() {
        let conn = Connection::open_in_memory().unwrap();
        assert!(!schema_is_current(&conn));
    }

    #[test]
    fn schema_is_current_rejects_missing_source_file_on_spans() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "
            CREATE TABLE spans (id TEXT PRIMARY KEY, path TEXT NOT NULL, ns TEXT NOT NULL, ts TEXT NOT NULL);
            CREATE TABLE edges (id TEXT PRIMARY KEY, rel TEXT NOT NULL, payload TEXT NOT NULL, ns TEXT NOT NULL, ts TEXT NOT NULL, source_file TEXT);
            CREATE TABLE index_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            CREATE VIRTUAL TABLE edges_fts USING fts5(payload, content=edges, content_rowid=rowid);
            ",
        ).unwrap();
        assert!(!schema_is_current(&conn));
    }

    #[test]
    fn schema_is_current_rejects_missing_fts() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "
            CREATE TABLE spans (id TEXT PRIMARY KEY, path TEXT NOT NULL, ns TEXT NOT NULL, ts TEXT NOT NULL, source_file TEXT);
            CREATE TABLE edges (id TEXT PRIMARY KEY, rel TEXT NOT NULL, payload TEXT NOT NULL, ns TEXT NOT NULL, ts TEXT NOT NULL, source_file TEXT);
            CREATE TABLE index_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            ",
        ).unwrap();
        assert!(!schema_is_current(&conn));
    }
}
