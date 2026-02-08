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

/// Result of resolution with optional relocated line info.
pub struct ResolveResult {
    pub freshness: Freshness,
    /// If relocated, the new line range.
    pub new_lines: Option<(u32, u32)>,
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
pub fn resolve_span(project_root: &Path, anchor: &SpanAnchor) -> Result<ResolveResult, ResolveError> {
    let file_path = project_root.join(&anchor.path);

    // Step 1: Check file exists
    if !file_path.exists() {
        return Ok(ResolveResult {
            freshness: Freshness::new(Resolution::Missing, Recency::Unknown),
            new_lines: None,
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
                });
            }
        }

        // Step 3: Search entire file for content hash match (relocated lines)
        if let Some((new_start, new_end)) = search_file_for_hash(&file_lines, stored_hash, end - start + 1) {
            let recency = compute_recency(project_root, &anchor.path, anchor.commit.as_deref());
            return Ok(ResolveResult {
                freshness: Freshness::new(Resolution::Relocated, recency),
                new_lines: Some((new_start, new_end)),
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
            });
        }
    }

    // Step 6: All anchors failed
    let recency = compute_recency(project_root, &anchor.path, anchor.commit.as_deref());
    Ok(ResolveResult {
        freshness: Freshness::new(Resolution::Unresolved, recency),
        new_lines: None,
    })
}

/// Read all lines of a file into a Vec.
fn read_file_lines(path: &Path) -> Result<Vec<String>, ResolveError> {
    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);
    Ok(reader.lines().collect::<Result<Vec<_>, _>>()?)
}

/// Extract lines [start, end] (1-indexed, inclusive) and join with newline.
fn extract_lines(lines: &[String], start: u32, end: u32) -> Option<String> {
    let s = start.checked_sub(1)? as usize;
    let e = end as usize;
    if s >= lines.len() || e > lines.len() {
        return None;
    }
    Some(lines[s..e].join("\n"))
}

/// Slide a window of `span_len` lines across the file, hashing each window
/// to find where content relocated to.
fn search_file_for_hash(lines: &[String], target_hash: &str, span_len: u32) -> Option<(u32, u32)> {
    let span_len = span_len as usize;
    if span_len == 0 || lines.len() < span_len {
        return None;
    }
    for start_idx in 0..=(lines.len() - span_len) {
        let window = lines[start_idx..start_idx + span_len].join("\n");
        let hash = content_hash(&window);
        if hash == target_hash {
            return Some(((start_idx + 1) as u32, (start_idx + span_len) as u32));
        }
    }
    None
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
    let prefixes = ["fn ", "struct ", "impl ", "enum ", "trait ", "type ",
                    "def ", "class ", "function ", "const ", "let ", "pub fn ",
                    "pub struct ", "pub enum ", "pub trait ", "pub type ",
                    "pub const ", "async fn ", "pub async fn "];
    for prefix in &prefixes {
        if let Some(rest) = line.strip_prefix(prefix) {
            let ident: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
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

    for i in start_idx..end_limit {
        for ch in lines[i].chars() {
            if ch == '{' {
                depth += 1;
                found_open = true;
            } else if ch == '}' {
                depth -= 1;
            }
        }
        if found_open && depth <= 0 {
            return i + 1;
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
        let window_tokens: std::collections::HashSet<&str> = window_text.split_whitespace().collect();

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

/// Compute recency by checking if the file has changed since the span's commit.
fn compute_recency(project_root: &Path, file_path: &str, commit_sha: Option<&str>) -> Recency {
    let Some(sha) = commit_sha else {
        return Recency::Unknown;
    };

    // Try to open the git repo
    let repo = match git2::Repository::discover(project_root) {
        Ok(r) => r,
        Err(_) => return Recency::Unknown,
    };

    // Parse the commit
    let oid = match git2::Oid::from_str(sha) {
        Ok(o) => o,
        Err(_) => return Recency::Unknown,
    };

    let commit = match repo.find_commit(oid) {
        Ok(c) => c,
        Err(_) => return Recency::Unknown,
    };

    // Get the file blob at that commit
    let tree = match commit.tree() {
        Ok(t) => t,
        Err(_) => return Recency::Unknown,
    };

    let entry = match tree.get_path(Path::new(file_path)) {
        Ok(e) => e,
        Err(_) => return Recency::Unknown,
    };

    // Get current HEAD's tree
    let head = match repo.head() {
        Ok(h) => h,
        Err(_) => return Recency::Unknown,
    };

    let head_commit = match head.peel_to_commit() {
        Ok(c) => c,
        Err(_) => return Recency::Unknown,
    };

    let head_tree = match head_commit.tree() {
        Ok(t) => t,
        Err(_) => return Recency::Unknown,
    };

    let head_entry = match head_tree.get_path(Path::new(file_path)) {
        Ok(e) => e,
        Err(_) => return Recency::FileChanged, // File removed since commit
    };

    // Compare blob OIDs
    if entry.id() == head_entry.id() {
        Recency::Unchanged
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
    fn symbol_detection() {
        assert!(contains_symbol_def("fn validate_token() {", "validate_token"));
        assert!(contains_symbol_def("pub fn validate_token() {", "validate_token"));
        assert!(contains_symbol_def("pub async fn validate_token() {", "validate_token"));
        assert!(contains_symbol_def("struct MyStruct {", "MyStruct"));
        assert!(!contains_symbol_def("// fn validate_token", "validate_token"));
        assert!(!contains_symbol_def("validate_token()", "validate_token"));
    }
}
