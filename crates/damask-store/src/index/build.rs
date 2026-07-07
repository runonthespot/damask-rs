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

    let conn = open_connection(db_path)?;
    create_schema(&conn)?;

    // Derive project root: edges_dir is .damask/edges, so root is two levels up
    let project_root = edges_dir
        .parent()
        .and_then(|p| p.parent())
        .unwrap_or(edges_dir);

    // Read all facts from all JSONL files
    let jsonl_files = list_jsonl_files(edges_dir, mode);
    let all_facts = read_facts_from_files(&jsonl_files)?;
    insert_facts(&conn, &all_facts, project_root, &ReuseMap::new())?;

    // Store mtimes + code-state fingerprints for incremental updates
    store_file_mtimes(&conn, &jsonl_files)?;
    store_code_state(&conn, project_root)?;

    // Compute active state
    compute_active_state(&conn)?;

    Ok(conn)
}

/// Open the index with a busy timeout so concurrent damask processes wait
/// for each other's updates instead of failing with SQLITE_BUSY.
fn open_connection(db_path: &Path) -> Result<Connection, StoreError> {
    let conn = Connection::open(db_path).map_err(|e| StoreError::Io(e.to_string()))?;
    conn.busy_timeout(std::time::Duration::from_millis(5000))
        .map_err(|e| StoreError::Io(e.to_string()))?;
    Ok(conn)
}

/// Incrementally update the index — only re-read JSONL files that changed.
/// Falls back to full rebuild if the DB doesn't exist or schema is missing.
pub fn update_index(db_path: &Path, edges_dir: &Path) -> Result<Connection, StoreError> {
    update_index_with_mode(db_path, edges_dir, IndexMode::FullLog)
}

/// Incrementally update the index. Two independent triggers:
///
/// - **Knowledge changed** (JSONL mtime): re-read only the changed files,
///   and re-resolve only spans whose content is new — unchanged spans
///   reuse their stored resolution instead of paying a per-span git
///   inspection (one append no longer re-resolves the whole namespace).
/// - **Code changed** (per-source-file fingerprint or git HEAD): re-resolve
///   only the spans anchored to files that moved — `at` stays truthful
///   immediately after mid-session edits and renames, without waiting for
///   an unrelated graph write.
///
/// Falls back to full rebuild if the DB doesn't exist or schema is missing.
/// The whole update runs inside one IMMEDIATE transaction with a busy
/// timeout, so N concurrent stale processes serialize and share one
/// rebuild instead of stampeding.
pub fn update_index_with_mode(
    db_path: &Path,
    edges_dir: &Path,
    mode: IndexMode,
) -> Result<Connection, StoreError> {
    if !db_path.exists() {
        return rebuild_index_with_mode(db_path, edges_dir, mode);
    }

    let conn = open_connection(db_path)?;

    // Verify schema exists and is current — if not, full rebuild.
    // Check for index_meta table AND source_file column on spans (added in current schema).
    if !schema_is_current(&conn) {
        drop(conn);
        return rebuild_index_with_mode(db_path, edges_dir, mode);
    }

    // Derive project root
    let project_root = edges_dir
        .parent()
        .and_then(|p| p.parent())
        .unwrap_or(edges_dir);

    // Cheap pre-check without the write lock.
    let jsonl_files = list_jsonl_files(edges_dir, mode);
    if find_changed_files(&conn, &jsonl_files)?.is_empty()
        && find_deleted_files(&conn, &jsonl_files)?.is_empty()
        && code_changes(&conn, project_root)?.is_none()
    {
        return Ok(conn);
    }

    // Work is needed: take the write lock, then RE-CHECK — a concurrent
    // process may have completed the same update while we waited.
    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|e| StoreError::Io(e.to_string()))?;

    let result = apply_update(&conn, edges_dir, project_root, mode);
    match result {
        Ok(()) => {
            conn.execute_batch("COMMIT")
                .map_err(|e| StoreError::Io(e.to_string()))?;
            Ok(conn)
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}

/// The locked portion of an incremental update. Re-detects work under the
/// lock, then applies knowledge-side and code-side refreshes.
fn apply_update(
    conn: &Connection,
    edges_dir: &Path,
    project_root: &Path,
    mode: IndexMode,
) -> Result<(), StoreError> {
    let jsonl_files = list_jsonl_files(edges_dir, mode);
    let changed_files = find_changed_files(conn, &jsonl_files)?;
    let deleted_files = find_deleted_files(conn, &jsonl_files)?;
    let code_delta = code_changes(conn, project_root)?;

    if changed_files.is_empty() && deleted_files.is_empty() && code_delta.is_none() {
        return Ok(()); // another process did the work while we waited
    }

    // Purge rows from deleted files
    if !deleted_files.is_empty() {
        delete_facts_for_files(conn, &deleted_files)?;
        remove_file_mtimes(conn, &deleted_files)?;
    }

    // Knowledge side: re-read changed JSONL, reusing stored resolutions
    // for spans whose content is unchanged.
    if !changed_files.is_empty() {
        let reuse = snapshot_resolutions(conn, &changed_files)?;
        delete_facts_for_files(conn, &changed_files)?;

        let new_facts = read_facts_from_files(&changed_files)?;
        insert_facts(conn, &new_facts, project_root, &reuse)?;

        store_file_mtimes(conn, &changed_files)?;
    }

    // Code side: re-resolve only spans anchored to files that moved (or,
    // when HEAD moved, spans that weren't cleanly resolved — a rename
    // commit can make a missing span resolvable again).
    if let Some(delta) = code_delta {
        refresh_spans(conn, project_root, &delta)?;
    }

    // Fingerprints must be recorded after refresh (effective paths may
    // have changed), and always — new spans introduce new source files.
    store_code_state(conn, project_root)?;

    // Recompute active state (cheap — just 3 SQL statements)
    compute_active_state(conn)?;

    Ok(())
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

        // Nanosecond precision: agents append and re-query within the same
        // second, so whole-second mtimes miss writes that follow an index
        // update too quickly.
        let current_mtime = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .map(|t| {
                t.duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos() as u64
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
                    .as_nanos() as u64
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

// ---------------------------------------------------------------------------
// Code-state fingerprints: the index is stale when the CODE moves, not just
// when the knowledge log does.
// ---------------------------------------------------------------------------

/// What changed on the code side since the last index update.
struct CodeDelta {
    /// Root-relative annotated paths whose fingerprint changed (edited,
    /// deleted, or reappeared).
    changed_paths: std::collections::HashSet<String>,
    /// Git HEAD moved (commit, checkout, rebase, merge).
    head_moved: bool,
}

/// Fingerprint of a source file: "mtime_ns:size", or "absent".
fn file_fingerprint(path: &Path) -> String {
    match std::fs::metadata(path) {
        Ok(m) => {
            let mtime = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0);
            format!("{}:{}", mtime, m.len())
        }
        Err(_) => "absent".to_string(),
    }
}

/// Current git HEAD, or "none" outside a repo — a stable comparable token.
fn git_head_token(project_root: &Path) -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(project_root)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "none".to_string())
}

/// Distinct annotated source paths (effective, root-relative) in the index.
fn annotated_paths(conn: &Connection) -> Result<Vec<String>, StoreError> {
    let mut stmt = conn
        .prepare("SELECT DISTINCT path FROM spans")
        .map_err(|e| StoreError::Io(e.to_string()))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| StoreError::Io(e.to_string()))?;
    let mut paths = Vec::new();
    for row in rows {
        paths.push(row.map_err(|e| StoreError::Io(e.to_string()))?);
    }
    Ok(paths)
}

/// Detect code-side drift: annotated files whose fingerprint changed, or a
/// moved git HEAD. Returns None when nothing moved. A missing fingerprint
/// (pre-fingerprint index) counts as changed, so old indexes self-heal
/// with one targeted refresh.
fn code_changes(conn: &Connection, project_root: &Path) -> Result<Option<CodeDelta>, StoreError> {
    let stored_head: Option<String> = conn
        .query_row(
            "SELECT value FROM index_meta WHERE key = 'head'",
            [],
            |row| row.get(0),
        )
        .ok();
    let head_moved = stored_head.as_deref() != Some(git_head_token(project_root).as_str());

    let mut changed_paths = std::collections::HashSet::new();
    let mut stmt = conn
        .prepare("SELECT value FROM index_meta WHERE key = ?1")
        .map_err(|e| StoreError::Io(e.to_string()))?;
    for path in annotated_paths(conn)? {
        let stored: Option<String> = stmt
            .query_row(rusqlite::params![format!("src:{path}")], |row| row.get(0))
            .ok();
        let current = file_fingerprint(&project_root.join(&path));
        if stored.as_deref() != Some(current.as_str()) {
            changed_paths.insert(path);
        }
    }

    if changed_paths.is_empty() && !head_moved {
        Ok(None)
    } else {
        Ok(Some(CodeDelta {
            changed_paths,
            head_moved,
        }))
    }
}

/// Record the current code state: one fingerprint per annotated path plus
/// git HEAD. Old `src:` entries are cleared first so unannotated files
/// don't accumulate.
fn store_code_state(conn: &Connection, project_root: &Path) -> Result<(), StoreError> {
    conn.execute("DELETE FROM index_meta WHERE key LIKE 'src:%'", [])
        .map_err(|e| StoreError::Io(e.to_string()))?;
    let mut stmt = conn
        .prepare("INSERT OR REPLACE INTO index_meta (key, value) VALUES (?1, ?2)")
        .map_err(|e| StoreError::Io(e.to_string()))?;
    for path in annotated_paths(conn)? {
        let fp = file_fingerprint(&project_root.join(&path));
        stmt.execute(rusqlite::params![format!("src:{path}"), fp])
            .map_err(|e| StoreError::Io(e.to_string()))?;
    }
    stmt.execute(rusqlite::params!["head", git_head_token(project_root)])
        .map_err(|e| StoreError::Io(e.to_string()))?;
    Ok(())
}

/// Stored resolution state snapshot, keyed by span id, used to skip
/// re-resolving spans whose content didn't change across a JSONL re-read.
type ReuseMap = std::collections::HashMap<String, ReusedResolution>;

struct ReusedResolution {
    content_hash: Option<String>,
    resolution: Option<String>,
    recency: Option<String>,
    path: String,
    line_start: Option<u32>,
    line_end: Option<u32>,
}

/// Snapshot resolution state for spans that came from the given JSONL
/// files, before their rows are deleted for re-insertion.
fn snapshot_resolutions(conn: &Connection, files: &[PathBuf]) -> Result<ReuseMap, StoreError> {
    let mut map = ReuseMap::new();
    let mut stmt = conn
        .prepare(
            "SELECT id, content_hash, resolution, recency, path, line_start, line_end
             FROM spans WHERE source_file = ?1",
        )
        .map_err(|e| StoreError::Io(e.to_string()))?;
    for path in files {
        let rows = stmt
            .query_map(rusqlite::params![path.display().to_string()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    ReusedResolution {
                        content_hash: row.get(1)?,
                        resolution: row.get(2)?,
                        recency: row.get(3)?,
                        path: row.get(4)?,
                        line_start: row.get(5)?,
                        line_end: row.get(6)?,
                    },
                ))
            })
            .map_err(|e| StoreError::Io(e.to_string()))?;
        for row in rows {
            let (id, reused) = row.map_err(|e| StoreError::Io(e.to_string()))?;
            map.insert(id, reused);
        }
    }
    Ok(map)
}

/// Re-resolve exactly the spans affected by a code delta and update their
/// rows in place: spans anchored to changed files always; when HEAD moved,
/// also spans that weren't cleanly resolved (rename detection may now
/// succeed).
fn refresh_spans(
    conn: &Connection,
    project_root: &Path,
    delta: &CodeDelta,
) -> Result<usize, StoreError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, path, line_start, line_end, snippet, symbol, content_hash, [commit], resolution
             FROM spans",
        )
        .map_err(|e| StoreError::Io(e.to_string()))?;
    struct Row {
        id: String,
        path: String,
        line_start: Option<u32>,
        line_end: Option<u32>,
        snippet: Option<String>,
        symbol: Option<String>,
        content_hash: Option<String>,
        commit: Option<String>,
        resolution: Option<String>,
    }
    let rows = stmt
        .query_map([], |row| {
            Ok(Row {
                id: row.get(0)?,
                path: row.get(1)?,
                line_start: row.get(2)?,
                line_end: row.get(3)?,
                snippet: row.get(4)?,
                symbol: row.get(5)?,
                content_hash: row.get(6)?,
                commit: row.get(7)?,
                resolution: row.get(8)?,
            })
        })
        .map_err(|e| StoreError::Io(e.to_string()))?;

    let mut affected = Vec::new();
    for row in rows {
        let r = row.map_err(|e| StoreError::Io(e.to_string()))?;
        let clean = matches!(r.resolution.as_deref(), Some("exact"));
        if delta.changed_paths.contains(&r.path) || (delta.head_moved && !clean) {
            affected.push(r);
        }
    }

    let mut update = conn
        .prepare(
            "UPDATE spans SET resolution = ?2, recency = ?3, path = ?4,
                              line_start = ?5, line_end = ?6
             WHERE id = ?1",
        )
        .map_err(|e| StoreError::Io(e.to_string()))?;

    let count = affected.len();
    for r in &affected {
        let anchor = SpanAnchor {
            path: r.path.clone(),
            line_start: r.line_start,
            line_end: r.line_end,
            content_hash: r.content_hash.clone(),
            symbol: r.symbol.clone(),
            snippet: r.snippet.clone(),
            commit: r.commit.clone(),
        };
        let (resolution, recency, ls, le, path) = resolve_to_columns(project_root, &anchor);
        // Params must match the SET order: ?2=resolution ?3=recency ?4=path ?5/?6=lines.
        update
            .execute(rusqlite::params![r.id, resolution, recency, path, ls, le])
            .map_err(|e| StoreError::Io(e.to_string()))?;
    }
    Ok(count)
}

/// Run the resolution cascade for an anchor and flatten the result into
/// span-table column values (resolution, recency, effective lines/path).
fn resolve_to_columns(
    project_root: &Path,
    anchor: &SpanAnchor,
) -> (Option<String>, Option<String>, Option<u32>, Option<u32>, String) {
    match resolve_span(project_root, anchor) {
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
            let (ls, le) = match result.new_lines {
                Some((s, e)) => (Some(s), Some(e)),
                None => (anchor.line_start, anchor.line_end),
            };
            let path = result.new_path.unwrap_or_else(|| anchor.path.clone());
            (Some(res.to_string()), Some(rec.to_string()), ls, le, path)
        }
        Err(_) => (
            None,
            None,
            anchor.line_start,
            anchor.line_end,
            anchor.path.clone(),
        ),
    }
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

    // Check that edges has the is_closed column (added for close command)
    let has_is_closed: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('edges') WHERE name='is_closed'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(false);

    has_spans_source && has_edges_source && has_fts && has_is_closed
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
///
/// Spans present in `reuse` with an unchanged content_hash keep their
/// stored resolution instead of re-running the cascade — this is what
/// makes one appended fact cost one resolution, not a whole namespace's.
/// (Code-side staleness is handled separately by `refresh_spans`.)
fn insert_facts(
    conn: &Connection,
    facts: &[(Fact, PathBuf)],
    project_root: &Path,
    reuse: &ReuseMap,
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

    // A span id appearing more than once in this batch is an append-only
    // re-anchoring (`damask confirm`): the later fact carries a fresh
    // anchor that may share the old content_hash, so reuse must not
    // resurrect the superseded resolution — resolve those fresh.
    let mut span_id_counts: std::collections::HashMap<&str, u32> =
        std::collections::HashMap::new();
    for (fact, _) in facts {
        if let Fact::Span(s) = fact {
            *span_id_counts.entry(s.id.as_str()).or_insert(0) += 1;
        }
    }

    for (fact, source_path) in facts {
        let source_file_str = source_path.display().to_string();
        match fact {
            Fact::Span(s) => {
                let (resolution_str, recency_str, effective_line_start, effective_line_end, effective_path) =
                    match reuse.get(s.id.as_str()).filter(|r| {
                        span_id_counts.get(s.id.as_str()) == Some(&1)
                            && r.resolution.is_some()
                            && r.content_hash == s.content_hash
                    }) {
                        Some(r) => (
                            r.resolution.clone(),
                            r.recency.clone(),
                            r.line_start,
                            r.line_end,
                            r.path.clone(),
                        ),
                        None => {
                            // New or changed span: run the resolution cascade.
                            let anchor = SpanAnchor {
                                path: s.path.clone(),
                                line_start: s.lines.map(|l| l[0]),
                                line_end: s.lines.map(|l| l[1]),
                                content_hash: s.content_hash.clone(),
                                symbol: s.symbol.clone(),
                                snippet: s.snippet.clone(),
                                commit: s.commit.clone(),
                            };
                            resolve_to_columns(project_root, &anchor)
                        }
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
    use crate::jsonl::FactWriter;
    use damask_core::{Fact, Span, SpanId};

    fn git(dir: &Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?} failed");
    }

    fn make_span(path: &str, start: u32, end: u32, hash: &str, commit: Option<String>) -> Span {
        Span {
            id: SpanId::new(),
            path: path.to_string(),
            lines: Some([start, end]),
            snippet: None,
            symbol: None,
            content_hash: Some(hash.to_string()),
            commit,
            ns: "test".to_string(),
            ts: chrono::Utc::now(),
            agent: None,
            session: None,
        }
    }

    fn span_row(conn: &Connection) -> (String, Option<u32>, Option<u32>, Option<String>) {
        conn.query_row(
            "SELECT path, line_start, line_end, resolution FROM spans LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap()
    }

    /// A mid-session source edit must refresh the anchor on the next index
    /// update — WITHOUT any JSONL write. This was the flagship lie: `at`
    /// served wrong line numbers until an unrelated graph write.
    #[test]
    fn source_edit_refreshes_anchor_without_graph_write() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        git(root, &["init", "-q"]);
        git(root, &["config", "user.email", "t@t"]);
        git(root, &["config", "user.name", "t"]);
        std::fs::write(root.join("src.rs"), "alpha\nbeta\ngamma\n").unwrap();
        git(root, &["add", "-A"]);
        git(root, &["commit", "-qm", "init"]);

        let edges_dir = root.join(".damask/edges");
        std::fs::create_dir_all(&edges_dir).unwrap();
        let hash = damask_resolve::content_hash("beta");
        let span = make_span("src.rs", 2, 2, &hash, None);
        FactWriter::append(&edges_dir.join("test.jsonl"), &Fact::Span(span)).unwrap();

        let db = root.join(".damask/index.db");
        let conn = update_index(&db, &edges_dir).unwrap();
        let (_, ls, _, res) = span_row(&conn);
        assert_eq!(ls, Some(2));
        assert_eq!(res.as_deref(), Some("exact"));
        drop(conn);

        // Prepend a line: "beta" moves to line 3. No JSONL change at all.
        std::fs::write(root.join("src.rs"), "zero\nalpha\nbeta\ngamma\n").unwrap();

        let conn = update_index(&db, &edges_dir).unwrap();
        let (_, ls, le, res) = span_row(&conn);
        assert_eq!(res.as_deref(), Some("relocated"), "edit must be noticed");
        assert_eq!(ls, Some(3), "anchor must follow the content");
        assert_eq!(le, Some(3));
    }

    /// A committed git rename must update the span's effective path on the
    /// next index update — without a graph write. Previously `at <new
    /// path>` said "No spans" until the namespace JSONL happened to change.
    #[test]
    fn git_rename_updates_effective_path_without_graph_write() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        git(root, &["init", "-q"]);
        git(root, &["config", "user.email", "t@t"]);
        git(root, &["config", "user.name", "t"]);
        std::fs::write(root.join("old.rs"), "line 1\nline 2\nline 3\n").unwrap();
        git(root, &["add", "-A"]);
        git(root, &["commit", "-qm", "init"]);
        let head = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(root)
            .output()
            .unwrap();
        let commit = String::from_utf8_lossy(&head.stdout).trim().to_string();

        let edges_dir = root.join(".damask/edges");
        std::fs::create_dir_all(&edges_dir).unwrap();
        let hash = damask_resolve::content_hash("line 1\nline 2\nline 3");
        let span = make_span("old.rs", 1, 3, &hash, Some(commit));
        FactWriter::append(&edges_dir.join("test.jsonl"), &Fact::Span(span)).unwrap();

        let db = root.join(".damask/index.db");
        let conn = update_index(&db, &edges_dir).unwrap();
        let (path, _, _, res) = span_row(&conn);
        assert_eq!(path, "old.rs");
        assert_eq!(res.as_deref(), Some("exact"));
        drop(conn);

        git(root, &["mv", "old.rs", "new.rs"]);
        git(root, &["commit", "-qm", "rename"]);

        let conn = update_index(&db, &edges_dir).unwrap();
        let (path, ls, le, res) = span_row(&conn);
        assert_eq!(path, "new.rs", "anchor must follow the rename");
        assert_eq!(res.as_deref(), Some("relocated"));
        assert_eq!((ls, le), (Some(1), Some(3)));
    }

    /// Appending to a namespace must NOT re-resolve unchanged spans: their
    /// stored resolution survives the JSONL re-read via the reuse map.
    #[test]
    fn append_reuses_stored_resolutions() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        git(root, &["init", "-q"]);
        git(root, &["config", "user.email", "t@t"]);
        git(root, &["config", "user.name", "t"]);
        std::fs::write(root.join("src.rs"), "alpha\nbeta\ngamma\n").unwrap();
        git(root, &["add", "-A"]);
        git(root, &["commit", "-qm", "init"]);

        let edges_dir = root.join(".damask/edges");
        std::fs::create_dir_all(&edges_dir).unwrap();
        let jsonl = edges_dir.join("test.jsonl");
        let hash = damask_resolve::content_hash("beta");
        FactWriter::append(&jsonl, &Fact::Span(make_span("src.rs", 2, 2, &hash, None))).unwrap();

        let db = root.join(".damask/index.db");
        let conn = update_index(&db, &edges_dir).unwrap();
        drop(conn);

        // Doctor the stored resolution to a sentinel value the resolver
        // would never compute. Code and HEAD stay untouched, so the code
        // side has no reason to refresh this span — if the sentinel
        // survives the append, the JSONL re-read reused it; if the update
        // had re-resolved the whole namespace (old behavior), it would be
        // overwritten with "exact".
        let conn = Connection::open(&db).unwrap();
        conn.execute("UPDATE spans SET resolution = 'sentinel'", [])
            .unwrap();
        drop(conn);

        FactWriter::append(
            &jsonl,
            &Fact::Span(make_span(
                "src.rs",
                1,
                1,
                &damask_resolve::content_hash("alpha"),
                None,
            )),
        )
        .unwrap();

        let conn = update_index(&db, &edges_dir).unwrap();
        let total: u32 = conn
            .query_row("SELECT COUNT(*) FROM spans", [], |row| row.get(0))
            .unwrap();
        assert_eq!(total, 2, "append must add the new span");
        let survived: u32 = conn
            .query_row(
                "SELECT COUNT(*) FROM spans WHERE resolution = 'sentinel'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            survived, 1,
            "unchanged span must reuse its stored resolution, not re-resolve"
        );
    }

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
