//! Span resolution, content hashing, and freshness tracking for Damask.
//!
//! This crate resolves spans against actual file content — determining whether
//! they're still valid, have moved, or are unresolvable.

mod content_hash;

pub use content_hash::content_hash;

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;

use damask_core::freshness::{Freshness, Recency, Resolution};

/// Errors produced by resolve operations.
#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("span references missing file: {0}")]
    MissingFile(String),

    #[error("git error: {0}")]
    Git(#[from] git2::Error),
}

/// Description of a span to resolve.
pub struct SpanAnchor {
    /// Root-relative file path.
    pub path: String,
    /// 1-indexed start line.
    pub line_start: Option<u32>,
    /// 1-indexed end line (inclusive).
    pub line_end: Option<u32>,
    /// Stored content hash (truncated SHA-256).
    pub content_hash: Option<String>,
    /// Symbol name for fallback matching.
    pub symbol: Option<String>,
    /// Snippet text for fuzzy fallback.
    pub snippet: Option<String>,
    /// Git commit at which the span was created.
    pub commit: Option<String>,
}

/// Result of resolution with optional relocated line/path info.
pub struct ResolveResult {
    pub freshness: Freshness,
    /// If relocated, the new line range.
    pub new_lines: Option<(u32, u32)>,
    /// If the file was renamed (git-tracked), the new root-relative path.
    pub new_path: Option<String>,
}

/// Resolve a span against the current file system state.
///
/// Implements the spec §5 resolution cascade:
/// 1. File exists?         → Missing if not
/// 2. Content hash match?  → Exact if lines match
/// 3. Search file for hash → Relocated if found elsewhere
/// 4. Symbol fallback      → Relocated if symbol found
/// 5. Snippet fallback     → Relocated if snippet fuzzy-matches
/// 6. All fail             → Unresolved
pub fn resolve_span(
    project_root: &Path,
    anchor: &SpanAnchor,
) -> Result<ResolveResult, ResolveError> {
    let file_path = project_root.join(&anchor.path);

    // Step 1: Check file exists — if not, try git rename detection before giving up
    if !file_path.exists() {
        if let Some(new_rel_path) =
            detect_rename(project_root, &anchor.path, anchor.commit.as_deref())
        {
            let new_file_path = project_root.join(&new_rel_path);
            if new_file_path.exists() {
                // File was renamed — continue cascade against the new path
                let file_lines = read_file_lines(&new_file_path)?;
                return resolve_renamed(project_root, anchor, &new_rel_path, &file_lines);
            }
        }

        return Ok(ResolveResult {
            freshness: Freshness::new(Resolution::Missing, Recency::Unknown),
            new_lines: None,
            new_path: None,
        });
    }

    let file_lines = read_file_lines(&file_path)?;

    // Step 2: Check content hash at original lines
    if let (Some(start), Some(end), Some(ref stored_hash)) =
        (anchor.line_start, anchor.line_end, &anchor.content_hash)
    {
        if let Some(extracted) = extract_lines(&file_lines, start, end) {
            let current_hash = content_hash(&extracted);
            if current_hash == *stored_hash {
                let recency = compute_recency(project_root, &anchor.path, anchor.commit.as_deref());
                return Ok(ResolveResult {
                    freshness: Freshness::new(Resolution::Exact, recency),
                    new_lines: None,
                    new_path: None,
                });
            }
        }

        // Step 3: Search entire file for content hash match (relocated lines)
        if let Some((new_start, new_end)) =
            search_file_for_hash(&file_lines, stored_hash, span_line_count(start, end), start)
        {
            let recency = compute_recency(project_root, &anchor.path, anchor.commit.as_deref());
            return Ok(ResolveResult {
                freshness: Freshness::new(Resolution::Relocated, recency),
                new_lines: Some((new_start, new_end)),
                new_path: None,
            });
        }
    }

    // Step 4: Symbol fallback
    if let Some(ref symbol) = anchor.symbol {
        if let Some((new_start, new_end)) = search_file_for_symbol(&file_lines, symbol) {
            let recency = compute_recency(project_root, &anchor.path, anchor.commit.as_deref());
            return Ok(ResolveResult {
                freshness: Freshness::new(Resolution::Relocated, recency),
                new_lines: Some((new_start, new_end)),
                new_path: None,
            });
        }
    }

    // Step 5: Snippet fuzzy match
    if let Some(ref snippet) = anchor.snippet {
        if let Some((new_start, new_end)) = search_file_for_snippet(&file_lines, snippet) {
            let recency = compute_recency(project_root, &anchor.path, anchor.commit.as_deref());
            return Ok(ResolveResult {
                freshness: Freshness::new(Resolution::Relocated, recency),
                new_lines: Some((new_start, new_end)),
                new_path: None,
            });
        }
    }

    // Step 5b: If content can't be found in-place, try git rename detection.
    if let Some(new_rel_path) = detect_rename(project_root, &anchor.path, anchor.commit.as_deref())
    {
        if new_rel_path != anchor.path {
            let new_file_path = project_root.join(&new_rel_path);
            if new_file_path.exists() {
                let file_lines = read_file_lines(&new_file_path)?;
                return resolve_renamed(project_root, anchor, &new_rel_path, &file_lines);
            }
        }
    }

    // Step 6: All anchors failed
    let recency = compute_recency(project_root, &anchor.path, anchor.commit.as_deref());
    Ok(ResolveResult {
        freshness: Freshness::new(Resolution::Unresolved, recency),
        new_lines: None,
        new_path: None,
    })
}

/// Resolve a span whose file was renamed. Runs the hash/symbol/snippet cascade
/// against the new file, returning Relocated with `new_path` set.
fn resolve_renamed(
    project_root: &Path,
    anchor: &SpanAnchor,
    new_rel_path: &str,
    file_lines: &[String],
) -> Result<ResolveResult, ResolveError> {
    let recency = compute_recency_with_commit_path(
        project_root,
        new_rel_path,
        &anchor.path,
        anchor.commit.as_deref(),
    );

    // Try content hash at original lines
    if let (Some(start), Some(end), Some(ref stored_hash)) =
        (anchor.line_start, anchor.line_end, &anchor.content_hash)
    {
        if let Some(extracted) = extract_lines(file_lines, start, end) {
            let current_hash = content_hash(&extracted);
            if current_hash == *stored_hash {
                return Ok(ResolveResult {
                    freshness: Freshness::new(Resolution::Relocated, recency),
                    new_lines: None,
                    new_path: Some(new_rel_path.to_string()),
                });
            }
        }

        // Search entire renamed file for hash match. A rename often
        // preserves structure, so the original line is still a useful prior
        // for picking among duplicate blocks.
        if let Some((new_start, new_end)) =
            search_file_for_hash(file_lines, stored_hash, span_line_count(start, end), start)
        {
            return Ok(ResolveResult {
                freshness: Freshness::new(Resolution::Relocated, recency),
                new_lines: Some((new_start, new_end)),
                new_path: Some(new_rel_path.to_string()),
            });
        }
    }

    // Symbol fallback
    if let Some(ref symbol) = anchor.symbol {
        if let Some((new_start, new_end)) = search_file_for_symbol(file_lines, symbol) {
            return Ok(ResolveResult {
                freshness: Freshness::new(Resolution::Relocated, recency),
                new_lines: Some((new_start, new_end)),
                new_path: Some(new_rel_path.to_string()),
            });
        }
    }

    // Snippet fallback
    if let Some(ref snippet) = anchor.snippet {
        if let Some((new_start, new_end)) = search_file_for_snippet(file_lines, snippet) {
            return Ok(ResolveResult {
                freshness: Freshness::new(Resolution::Relocated, recency),
                new_lines: Some((new_start, new_end)),
                new_path: Some(new_rel_path.to_string()),
            });
        }
    }

    // Renamed file found but can't locate the span within it
    Ok(ResolveResult {
        freshness: Freshness::new(Resolution::Unresolved, recency),
        new_lines: None,
        new_path: Some(new_rel_path.to_string()),
    })
}

/// Detect if a file was renamed between a commit and HEAD using git2 diff
/// with rename detection. Returns the new root-relative path if found.
fn detect_rename(project_root: &Path, old_path: &str, commit_sha: Option<&str>) -> Option<String> {
    let sha = commit_sha?;
    let repo = git2::Repository::discover(project_root).ok()?;

    let old_oid = git2::Oid::from_str(sha).ok()?;
    let old_commit = repo.find_commit(old_oid).ok()?;
    let old_tree = old_commit.tree().ok()?;

    let head = repo.head().ok()?;
    let head_tree = head.peel_to_tree().ok()?;

    let mut diff = repo
        .diff_tree_to_tree(Some(&old_tree), Some(&head_tree), None)
        .ok()?;

    // Enable rename detection
    let mut find_opts = git2::DiffFindOptions::new();
    find_opts.renames(true);
    diff.find_similar(Some(&mut find_opts)).ok()?;

    // Look for renames involving our file
    for delta in diff.deltas() {
        if delta.status() == git2::Delta::Renamed {
            if let Some(old) = delta.old_file().path() {
                if old == Path::new(old_path) {
                    return delta
                        .new_file()
                        .path()
                        .map(|p| p.to_string_lossy().to_string());
                }
            }
        }
    }

    None
}

/// Read all lines of a file into a Vec.
fn read_file_lines(path: &Path) -> Result<Vec<String>, ResolveError> {
    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);
    Ok(reader.lines().collect::<Result<Vec<_>, _>>()?)
}

/// Extract lines [start, end] (1-indexed, inclusive) and join with newline.
/// Returns None for inverted or out-of-range anchors (e.g. hand-written
/// JSONL with `"lines":[10,5]`) instead of panicking.
fn extract_lines(lines: &[String], start: u32, end: u32) -> Option<String> {
    if end < start {
        return None;
    }
    let s = start.checked_sub(1)? as usize;
    let e = end as usize;
    if s >= lines.len() || e > lines.len() {
        return None;
    }
    Some(lines[s..e].join("\n"))
}

/// Length of a 1-indexed inclusive line range; 0 for inverted ranges so the
/// hash search safely no-ops instead of underflowing.
fn span_line_count(start: u32, end: u32) -> u32 {
    end.checked_sub(start).map_or(0, |d| d + 1)
}

/// Slide a window of `span_len` lines across the file, hashing each window
/// to find where content relocated to. When the anchored block is
/// duplicated in the file, ALL windows match the hash — so pick the one
/// NEAREST the original line, not the first top-to-bottom. Code moves
/// locally far more often than globally, so nearest-to-original is the
/// best guess at which copy is "the" relocation; first-match would jump
/// to an unrelated identical block at the top of the file. `orig_start`
/// is the span's original 1-indexed start line.
///
/// This heuristic can't be perfect: if the real code moved far while an
/// identical block sits near the original, nearest picks wrong. True
/// disambiguation needs semantic identity, not textual — out of scope
/// for a resolver. Nearest is the right v1.
fn search_file_for_hash(
    lines: &[String],
    target_hash: &str,
    span_len: u32,
    orig_start: u32,
) -> Option<(u32, u32)> {
    let span_len = span_len as usize;
    if span_len == 0 || lines.len() < span_len {
        return None;
    }
    let orig_idx = orig_start.saturating_sub(1) as usize;
    let mut best: Option<(usize, usize)> = None; // (start_idx, distance)
    for start_idx in 0..=(lines.len() - span_len) {
        let window = lines[start_idx..start_idx + span_len].join("\n");
        if content_hash(&window) == target_hash {
            let dist = start_idx.abs_diff(orig_idx);
            if best.map_or(true, |(_, best_dist)| dist < best_dist) {
                best = Some((start_idx, dist));
            }
        }
    }
    best.map(|(start_idx, _)| ((start_idx + 1) as u32, (start_idx + span_len) as u32))
}

/// Search for a symbol (function/struct name) in the file.
/// Returns the line range of the first match.
fn search_file_for_symbol(lines: &[String], symbol: &str) -> Option<(u32, u32)> {
    for (i, line) in lines.iter().enumerate() {
        // Match common patterns: `fn symbol`, `struct symbol`, `impl symbol`,
        // `def symbol`, `class symbol`, `function symbol`
        let trimmed = line.trim();
        if contains_symbol_def(trimmed, symbol) {
            let start = (i + 1) as u32;
            // Estimate a reasonable end: scan forward for the next blank line or
            // next definition, cap at 50 lines
            let end = find_block_end(lines, i, 50);
            return Some((start, end as u32));
        }
    }
    None
}

/// Check if a line contains a symbol definition.
fn contains_symbol_def(line: &str, symbol: &str) -> bool {
    let prefixes = [
        "fn ",
        "struct ",
        "impl ",
        "enum ",
        "trait ",
        "type ",
        "def ",
        "class ",
        "function ",
        "const ",
        "let ",
        "pub fn ",
        "pub struct ",
        "pub enum ",
        "pub trait ",
        "pub type ",
        "pub const ",
        "async fn ",
        "pub async fn ",
    ];
    for prefix in &prefixes {
        if let Some(rest) = line.strip_prefix(prefix) {
            let ident: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if ident == symbol {
                return true;
            }
        }
    }
    false
}

/// Find the end of a code block starting at `start_idx`, up to `max_lines` forward.
fn find_block_end(lines: &[String], start_idx: usize, max_lines: usize) -> usize {
    let end_limit = (start_idx + max_lines).min(lines.len());
    let mut depth: i32 = 0;
    let mut found_open = false;

    for (offset, line) in lines[start_idx..end_limit].iter().enumerate() {
        for ch in line.chars() {
            if ch == '{' {
                depth += 1;
                found_open = true;
            } else if ch == '}' {
                depth -= 1;
            }
        }
        if found_open && depth <= 0 {
            return start_idx + offset + 1;
        }
    }
    // Didn't find matching brace — return the limit
    end_limit
}

/// Search for a snippet in the file using token overlap.
/// Returns the line range with the best match above threshold.
fn search_file_for_snippet(lines: &[String], snippet: &str) -> Option<(u32, u32)> {
    let snippet_tokens: std::collections::HashSet<&str> = snippet.split_whitespace().collect();
    if snippet_tokens.is_empty() {
        return None;
    }

    let window_size = snippet.lines().count().max(3);
    let mut best_score = 0.0f64;
    let mut best_start = 0usize;

    for start_idx in 0..lines.len().saturating_sub(window_size - 1) {
        let end_idx = (start_idx + window_size).min(lines.len());
        let window_text = lines[start_idx..end_idx].join(" ");
        let window_tokens: std::collections::HashSet<&str> =
            window_text.split_whitespace().collect();

        if window_tokens.is_empty() {
            continue;
        }

        let intersection = snippet_tokens.intersection(&window_tokens).count();
        let smaller = snippet_tokens.len().min(window_tokens.len());
        let score = intersection as f64 / smaller as f64;

        if score > best_score {
            best_score = score;
            best_start = start_idx;
        }
    }

    // Require at least 50% token overlap
    if best_score >= 0.5 {
        Some(((best_start + 1) as u32, (best_start + window_size) as u32))
    } else {
        None
    }
}

/// Recency now means one thing: **is the working tree dirty for this file?**
/// It compares on-disk content against HEAD — NOT against the span's original
/// commit. A file that merely evolved across commits but is now committed
/// clean is `Unchanged`; only an uncommitted edit is `FileChanged`.
///
/// Rationale: the RESOLUTION axis already captures whether the anchor content
/// itself moved or changed. Comparing recency against the span's old commit
/// made every long-lived file read as changed forever (false amber). Keyed to
/// HEAD instead, recency drives exactly one display state — grey/uncommitted —
/// and clears the moment the work is committed. The `commit_sha` is retained
/// in the signature for callers but no longer consulted.
fn compute_recency(project_root: &Path, file_path: &str, _commit_sha: Option<&str>) -> Recency {
    working_tree_recency(project_root, file_path)
}

/// Recency for a possibly-renamed file: still "is the working tree dirty?",
/// measured at the file's current (disk) path against HEAD.
fn compute_recency_with_commit_path(
    project_root: &Path,
    disk_path: &str,
    _commit_path: &str,
    _commit_sha: Option<&str>,
) -> Recency {
    working_tree_recency(project_root, disk_path)
}

/// Compare disk content against the HEAD blob: matches → `Unchanged`
/// (committed clean); differs or unreadable → `FileChanged` (uncommitted);
/// no git or the file isn't tracked in HEAD → `Unknown`.
fn working_tree_recency(project_root: &Path, file_path: &str) -> Recency {
    let repo = match git2::Repository::discover(project_root) {
        Ok(r) => r,
        Err(_) => return Recency::Unknown,
    };

    let head_tree = match repo.head().and_then(|h| h.peel_to_tree()) {
        Ok(t) => t,
        Err(_) => return Recency::Unknown,
    };

    let head_entry = match head_tree.get_path(Path::new(file_path)) {
        Ok(e) => e,
        Err(_) => return Recency::Unknown,
    };

    let abs_path = project_root.join(file_path);
    if let Ok(disk_content) = fs::read(&abs_path) {
        let head_blob = match repo.find_blob(head_entry.id()) {
            Ok(b) => b,
            Err(_) => return Recency::Unknown,
        };
        if disk_content == head_blob.content() {
            Recency::Unchanged
        } else {
            Recency::FileChanged
        }
    } else {
        Recency::FileChanged
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn resolve_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let anchor = SpanAnchor {
            path: "nonexistent.rs".to_string(),
            line_start: Some(1),
            line_end: Some(5),
            content_hash: None,
            symbol: None,
            snippet: None,
            commit: None,
        };
        let result = resolve_span(dir.path(), &anchor).unwrap();
        assert_eq!(result.freshness.resolution, Resolution::Missing);
    }

    #[test]
    fn resolve_exact_match() {
        let dir = tempfile::tempdir().unwrap();
        let content = "line 1\nline 2\nline 3\nline 4\nline 5";
        fs::write(dir.path().join("test.rs"), content).unwrap();

        let hash = content_hash("line 2\nline 3\nline 4");
        let anchor = SpanAnchor {
            path: "test.rs".to_string(),
            line_start: Some(2),
            line_end: Some(4),
            content_hash: Some(hash),
            symbol: None,
            snippet: None,
            commit: None,
        };
        let result = resolve_span(dir.path(), &anchor).unwrap();
        assert_eq!(result.freshness.resolution, Resolution::Exact);
    }

    #[test]
    fn resolve_relocated_by_hash() {
        let dir = tempfile::tempdir().unwrap();
        // Content shifted down by 2 lines
        let content = "new line 1\nnew line 2\nline 2\nline 3\nline 4\nline 5";
        fs::write(dir.path().join("test.rs"), content).unwrap();

        let hash = content_hash("line 2\nline 3\nline 4");
        let anchor = SpanAnchor {
            path: "test.rs".to_string(),
            line_start: Some(1), // original position
            line_end: Some(3),
            content_hash: Some(hash),
            symbol: None,
            snippet: None,
            commit: None,
        };
        let result = resolve_span(dir.path(), &anchor).unwrap();
        assert_eq!(result.freshness.resolution, Resolution::Relocated);
        assert_eq!(result.new_lines, Some((3, 5)));
    }

    #[test]
    fn resolve_by_symbol() {
        let dir = tempfile::tempdir().unwrap();
        let content = "// header\nfn validate_token() {\n    // body\n}\n";
        fs::write(dir.path().join("test.rs"), content).unwrap();

        let anchor = SpanAnchor {
            path: "test.rs".to_string(),
            line_start: Some(10), // wrong lines
            line_end: Some(15),
            content_hash: Some("badhash12345".to_string()),
            symbol: Some("validate_token".to_string()),
            snippet: None,
            commit: None,
        };
        let result = resolve_span(dir.path(), &anchor).unwrap();
        assert_eq!(result.freshness.resolution, Resolution::Relocated);
        assert!(result.new_lines.is_some());
    }

    #[test]
    fn resolve_by_snippet() {
        let dir = tempfile::tempdir().unwrap();
        let content = "alpha\nbeta\nfn check_auth token valid\ngamma\ndelta\n";
        fs::write(dir.path().join("test.rs"), content).unwrap();

        let anchor = SpanAnchor {
            path: "test.rs".to_string(),
            line_start: Some(10),
            line_end: Some(15),
            content_hash: Some("badhash12345".to_string()),
            symbol: None,
            snippet: Some("check_auth token valid".to_string()),
            commit: None,
        };
        let result = resolve_span(dir.path(), &anchor).unwrap();
        assert_eq!(result.freshness.resolution, Resolution::Relocated);
    }

    #[test]
    fn resolve_unresolved() {
        let dir = tempfile::tempdir().unwrap();
        let content = "completely different content\n";
        fs::write(dir.path().join("test.rs"), content).unwrap();

        let anchor = SpanAnchor {
            path: "test.rs".to_string(),
            line_start: Some(1),
            line_end: Some(5),
            content_hash: Some("badhash12345".to_string()),
            symbol: Some("nonexistent_function".to_string()),
            snippet: Some("xyzzy plugh nothing matches".to_string()),
            commit: None,
        };
        let result = resolve_span(dir.path(), &anchor).unwrap();
        assert_eq!(result.freshness.resolution, Resolution::Unresolved);
    }

    #[test]
    fn resolve_follows_git_rename() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Initialize a git repo
        let repo = git2::Repository::init(root).unwrap();
        let content = "line 1\nline 2\nline 3\n";
        fs::write(root.join("old_name.rs"), content).unwrap();

        // Stage and commit
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("old_name.rs")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = git2::Signature::now("test", "test@test.com").unwrap();
        let commit_oid = repo
            .commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
            .unwrap();

        // Rename the file and commit again
        fs::rename(root.join("old_name.rs"), root.join("new_name.rs")).unwrap();
        let mut index = repo.index().unwrap();
        index.remove_path(Path::new("old_name.rs")).unwrap();
        index.add_path(Path::new("new_name.rs")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let parent = repo.find_commit(commit_oid).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "rename", &tree, &[&parent])
            .unwrap();

        let hash = content_hash("line 1\nline 2\nline 3");
        let anchor = SpanAnchor {
            path: "old_name.rs".to_string(),
            line_start: Some(1),
            line_end: Some(3),
            content_hash: Some(hash),
            symbol: None,
            snippet: None,
            commit: Some(commit_oid.to_string()),
        };

        let result = resolve_span(root, &anchor).unwrap();
        assert_eq!(result.freshness.resolution, Resolution::Relocated);
        assert_eq!(result.new_path, Some("new_name.rs".to_string()));
        assert_eq!(result.freshness.recency, Recency::Unchanged);
    }

    #[test]
    fn extract_lines_basic() {
        let lines: Vec<String> = vec!["a", "b", "c", "d", "e"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(extract_lines(&lines, 2, 4), Some("b\nc\nd".to_string()));
    }

    #[test]
    fn extract_lines_out_of_range() {
        let lines: Vec<String> = vec!["a", "b"].into_iter().map(String::from).collect();
        assert_eq!(extract_lines(&lines, 1, 5), None);
    }

    #[test]
    fn extract_lines_inverted_range() {
        // Hand-written JSONL can carry "lines":[10,5]; must not panic.
        let lines: Vec<String> = (1..=20).map(|i| format!("line {}", i)).collect();
        assert_eq!(extract_lines(&lines, 10, 5), None);
        assert_eq!(extract_lines(&lines, 20, 1), None);
        // Single-line span is still valid.
        assert_eq!(extract_lines(&lines, 3, 3), Some("line 3".to_string()));
    }

    #[test]
    fn resolve_inverted_anchor_is_unresolved_not_panic() {
        // A corrupt anchor with line_end < line_start must resolve to
        // Unresolved instead of slice-panicking (and bricking every command
        // that touches the store, including the SessionStart briefing hook).
        let dir = tempfile::tempdir().unwrap();
        let content = (1..=20)
            .map(|i| format!("line {}\n", i))
            .collect::<String>();
        fs::write(dir.path().join("test.rs"), content).unwrap();

        let anchor = SpanAnchor {
            path: "test.rs".to_string(),
            line_start: Some(10),
            line_end: Some(5),
            content_hash: Some("badhash12345".to_string()),
            symbol: None,
            snippet: None,
            commit: None,
        };
        let result = resolve_span(dir.path(), &anchor).unwrap();
        assert_eq!(result.freshness.resolution, Resolution::Unresolved);
    }

    #[test]
    fn span_line_count_guards_inversion() {
        assert_eq!(span_line_count(5, 10), 6);
        assert_eq!(span_line_count(3, 3), 1);
        assert_eq!(span_line_count(10, 5), 0);
    }

    #[test]
    fn relocation_prefers_the_copy_nearest_the_original() {
        // The anchored block ("AAA\nBBB") is duplicated three times. The
        // original was at lines 6-7; the nearest surviving copy is at 9-10.
        // First-match would wrongly jump to the copy at the top (1-2).
        let lines: Vec<String> = [
            "AAA", "BBB", // 1-2: a decoy copy near the top
            "x", "y", "z", // 3-5
            "changed", "here", // 6-7: where the original was (now different)
            "q",    // 8
            "AAA", "BBB", // 9-10: the nearest surviving copy
            "AAA", "BBB", // 11-12: a farther copy
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let target = content_hash("AAA\nBBB");
        // orig_start = 6 → nearest match is 9-10, not the first (1-2).
        assert_eq!(
            search_file_for_hash(&lines, &target, 2, 6),
            Some((9, 10)),
            "must re-anchor to the duplicate nearest the original, not the first"
        );
        // A block that appears exactly once resolves unambiguously.
        let uniq = content_hash("changed\nhere");
        assert_eq!(search_file_for_hash(&lines, &uniq, 2, 6), Some((6, 7)));
        // No match at all.
        assert_eq!(search_file_for_hash(&lines, "deadbeef", 2, 6), None);
    }

    #[test]
    fn symbol_detection() {
        assert!(contains_symbol_def(
            "fn validate_token() {",
            "validate_token"
        ));
        assert!(contains_symbol_def(
            "pub fn validate_token() {",
            "validate_token"
        ));
        assert!(contains_symbol_def(
            "pub async fn validate_token() {",
            "validate_token"
        ));
        assert!(contains_symbol_def("struct MyStruct {", "MyStruct"));
        assert!(!contains_symbol_def(
            "// fn validate_token",
            "validate_token"
        ));
        assert!(!contains_symbol_def("validate_token()", "validate_token"));
    }
}
