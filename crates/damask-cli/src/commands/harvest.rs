//! Session-end harvest check.
//!
//! Designed to run as a Claude Code `Stop` hook. Reads the hook JSON from
//! stdin, scans the session transcript, and — if the agent edited files but
//! recorded nothing in damask — blocks the stop once with a targeted nudge
//! to preserve durable findings. The agent is free to finish immediately if
//! nothing durable was learned; the hook never blocks twice
//! (`stop_hook_active` guards re-entry).
//!
//! Fails open in every error case: a hook must never break a session, so any
//! missing/garbled input results in a silent exit 0 (stop allowed).

use std::env;
use std::io::Read as _;
use std::path::{Path, PathBuf};

use damask_store::{update_index_with_mode, DamaskProject, IndexMode, IndexQuery};

use crate::error::Result;

use super::helpers::project_relative;

/// Tool names whose use counts as editing a file.
const EDIT_TOOLS: &[&str] = &["Edit", "Write", "MultiEdit", "NotebookEdit"];

/// Damask subcommands that count as recording knowledge.
const WRITE_SUBCOMMANDS: &[&str] = &[
    "record", "edge", "span", "batch", "endorse", "dispute", "close", "confirm",
];

/// Damask subcommands that signal on EXISTING edges — the gardening verbs.
const SIGNAL_SUBCOMMANDS: &[&str] = &["endorse", "dispute", "close", "confirm"];

/// Maximum edited files listed in the nudge.
const MAX_FILES_IN_REASON: usize = 10;

/// Maximum open findings listed in the reconcile nudge.
const MAX_FINDINGS_IN_RECONCILE: usize = 8;

#[derive(Default)]
struct SessionActivity {
    /// Root-relative edited files, deduplicated, in first-touch order.
    edited_files: Vec<String>,
    /// True if the session ran any damask write command.
    recorded: bool,
    /// True if the session signalled on existing edges (endorse/dispute/close/confirm).
    signalled: bool,
    /// Earliest transcript timestamp (ISO-8601 UTC) — the session window start.
    window_start: Option<String>,
}

pub fn run(transcript_override: Option<&str>) -> Result<()> {
    let transcript_path = match transcript_override {
        Some(p) => PathBuf::from(p),
        None => {
            let mut input = String::new();
            if std::io::stdin().read_to_string(&mut input).is_err() {
                return Ok(());
            }
            let Ok(hook) = serde_json::from_str::<serde_json::Value>(&input) else {
                return Ok(());
            };
            // Never block twice: a stop that follows our own block passes through.
            if hook
                .get("stop_hook_active")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                return Ok(());
            }
            match hook.get("transcript_path").and_then(|v| v.as_str()) {
                Some(p) => PathBuf::from(p),
                None => return Ok(()),
            }
        }
    };

    let Ok(cwd) = env::current_dir() else {
        return Ok(());
    };
    let Ok(project) = DamaskProject::discover(&cwd) else {
        return Ok(());
    };

    let Some(activity) = scan_transcript(&transcript_path, &project.root) else {
        return Ok(());
    };

    // Findings were recorded: shift from quantity to quality.
    if activity.recorded {
        // Inbound gate first (field-driven): a session that edits annotated
        // files and never signals is the fix-without-close leak — the fix
        // that resolves a finding usually refactors away its anchor, and
        // the edge then rots open forever. One reconciliation ask.
        if !activity.signalled {
            if let Some(reason) = reconcile_reason(
                &project,
                &activity.edited_files,
                activity.window_start.as_deref(),
            ) {
                let output = serde_json::json!({
                    "decision": "block",
                    "reason": reason,
                });
                println!("{}", serde_json::to_string(&output).unwrap());
                return Ok(());
            }
        }
        // Then quality: lint what this session wrote and nudge once if
        // anything is seriously deficient.
        if let Some(reason) = quality_reason(&project, activity.window_start.as_deref()) {
            let output = serde_json::json!({
                "decision": "block",
                "reason": reason,
            });
            println!("{}", serde_json::to_string(&output).unwrap());
        }
        return Ok(());
    }

    // Read-only session: nothing to nudge about.
    if activity.edited_files.is_empty() {
        return Ok(());
    }

    let reason = build_reason(&project, &activity.edited_files);
    let output = serde_json::json!({
        "decision": "block",
        "reason": reason,
    });
    println!("{}", serde_json::to_string(&output).unwrap());

    Ok(())
}

/// True if a shell command invokes damask with a write subcommand.
///
/// Token-based rather than substring matching: agents invoke damask as
/// `damask`, `./target/debug/damask`, or with global flags between the
/// binary and the subcommand (`damask --ns x record …`), so requiring the
/// literal text "damask record" misses real recordings. A false positive
/// merely skips the nudge, so loose matching errs the right way.
fn is_damask_write(cmd: &str) -> bool {
    invokes_damask_with(cmd, WRITE_SUBCOMMANDS)
}

/// True if a shell command signals on existing edges (gardening verbs).
fn is_damask_signal(cmd: &str) -> bool {
    invokes_damask_with(cmd, SIGNAL_SUBCOMMANDS)
}

fn invokes_damask_with(cmd: &str, subcommands: &[&str]) -> bool {
    let tokens: Vec<&str> = cmd.split_whitespace().collect();
    let invokes_damask = tokens
        .iter()
        .any(|t| *t == "damask" || t.ends_with("/damask"));
    invokes_damask && tokens.iter().any(|t| subcommands.contains(t))
}

/// Parse a Claude Code transcript (JSONL) for file edits and damask writes.
/// Returns None if the transcript is unreadable.
fn scan_transcript(transcript_path: &Path, root: &Path) -> Option<SessionActivity> {
    let content = std::fs::read_to_string(transcript_path).ok()?;
    let mut activity = SessionActivity::default();

    for line in content.lines() {
        let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if let Some(ts) = entry.get("timestamp").and_then(|v| v.as_str()) {
            // ISO-8601 UTC timestamps compare lexicographically.
            let earlier = activity.window_start.as_deref().map_or(true, |w| ts < w);
            if earlier {
                activity.window_start = Some(ts.to_string());
            }
        }
        if entry.get("type").and_then(|v| v.as_str()) != Some("assistant") {
            continue;
        }
        let Some(items) = entry.pointer("/message/content").and_then(|v| v.as_array()) else {
            continue;
        };
        for item in items {
            if item.get("type").and_then(|v| v.as_str()) != Some("tool_use") {
                continue;
            }
            let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if EDIT_TOOLS.contains(&name) {
                let path = item
                    .pointer("/input/file_path")
                    .or_else(|| item.pointer("/input/notebook_path"))
                    .and_then(|v| v.as_str());
                if let Some(rel) = path.and_then(|p| project_relative(p, root)) {
                    if !activity.edited_files.contains(&rel) {
                        activity.edited_files.push(rel);
                    }
                }
            } else if name == "Bash" {
                let cmd = item
                    .pointer("/input/command")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if is_damask_write(cmd) {
                    activity.recorded = true;
                }
                if is_damask_signal(cmd) {
                    activity.signalled = true;
                }
            }
        }
    }

    Some(activity)
}

/// Lint edges created during the session window; serious issues become a
/// one-shot nudge. Returns None (stop allowed) when clean or on any error.
fn quality_reason(project: &DamaskProject, window_start: Option<&str>) -> Option<String> {
    let window_start = window_start?;

    let db_path = project.damask_dir.join("index.db");
    let edges_dir = project.damask_dir.join("edges");
    let conn = update_index_with_mode(&db_path, &edges_dir, IndexMode::ViewsPreferred).ok()?;
    let q = IndexQuery::new(&conn);

    // ISO-8601 UTC timestamps compare lexicographically.
    let session_edges: Vec<damask_store::index::query::EdgeRow> = q
        .all_active_open_edges_ns(None)
        .ok()?
        .into_iter()
        .filter(|e| e.ts.as_str() >= window_start)
        .collect();
    if session_edges.is_empty() {
        return None;
    }

    let inputs: Vec<damask_store::LintInput> = session_edges
        .into_iter()
        .map(|edge| damask_store::LintInput {
            edge,
            span_snippet: None,
            resolution: None,
        })
        .collect();

    // Only hard errors warrant interrupting a stop; warnings surface in
    // `damask lint` without blocking anyone.
    let issues: Vec<damask_store::LintIssue> = damask_store::lint_edges(&inputs)
        .into_iter()
        .filter(|i| matches!(i.severity, damask_store::Severity::Error))
        .collect();
    if issues.is_empty() {
        return None;
    }

    let mut lines = String::new();
    for issue in issues.iter().take(5) {
        lines.push_str(&format!("- {} — {}\n", issue.edge_id, issue.message));
    }
    if issues.len() > 5 {
        lines.push_str(&format!(
            "- … and {} more (`damask lint`)\n",
            issues.len() - 5
        ));
    }

    Some(format!(
        "Damask harvest: you recorded findings this session, but {} of them have quality problems:\n\n{lines}\n\
         Edges are append-only — to fix one, close it and re-record properly:\n\
         - `damask close <edge_id> --reason incorrect`\n\
         - `damask record <file> <start> <end> <rel> '{{\"summary\":\"...\",\"confidence\":0.8}}'`\n\n\
         Low-quality edges rank poorly and waste future agents' attention. \
         If you believe these are fine as-is, simply finish — you will not be asked again.",
        issues.len()
    ))
}

/// Open findings anchored to files this session edited, when the session
/// never signalled. Only findings that PRE-DATE the session count — a
/// session must not be nudged to reconcile the edges it just recorded.
/// Returns None (stop allowed) when the edited files carry no open
/// pre-existing edges or on any error.
fn reconcile_reason(
    project: &DamaskProject,
    edited_files: &[String],
    window_start: Option<&str>,
) -> Option<String> {
    if edited_files.is_empty() {
        return None;
    }
    let window_start = window_start?;
    let db_path = project.damask_dir.join("index.db");
    let edges_dir = project.damask_dir.join("edges");
    let conn = update_index_with_mode(&db_path, &edges_dir, IndexMode::ViewsPreferred).ok()?;
    let q = IndexQuery::new(&conn);

    // (file, edge_id, rel, summary) for open edges on edited files.
    let mut findings: Vec<(String, String, String, String)> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for file in edited_files {
        for span in q.spans_for_file(file).ok()? {
            for edge in q.edges_for_span_open(&span.id).ok()? {
                // ISO-8601 UTC timestamps compare lexicographically.
                if edge.ts.as_str() >= window_start || !seen.insert(edge.id.clone()) {
                    continue;
                }
                let payload: serde_json::Value =
                    serde_json::from_str(&edge.payload).unwrap_or(serde_json::json!({}));
                let summary = damask_core::PayloadEnvelope::new(&payload)
                    .summary()
                    .unwrap_or("")
                    .to_string();
                findings.push((
                    file.clone(),
                    edge.id,
                    edge.rel,
                    damask_core::truncate_str(&summary, 90).to_string(),
                ));
            }
        }
    }
    if findings.is_empty() {
        return None;
    }

    let total = findings.len();
    let mut lines = String::new();
    for (file, id, rel, summary) in findings.into_iter().take(MAX_FINDINGS_IN_RECONCILE) {
        lines.push_str(&format!("- {id} [{rel}] on {file} — {summary}\n"));
    }
    if total > MAX_FINDINGS_IN_RECONCILE {
        lines.push_str(&format!(
            "- … and {} more (`damask at <file>`)\n",
            total - MAX_FINDINGS_IN_RECONCILE
        ));
    }

    Some(format!(
        "Damask harvest: you edited files that carry {total} open finding(s), and this session \
         never signalled on any edge:\n\n{lines}\n\
         Did your changes resolve, confirm, or contradict any of these?\n\
         - Fixed/obsolete: `damask close <edge_id> --reason resolved`\n\
         - Still true of the new code: `damask endorse <edge_id>` (or `damask confirm <span_id>` if the anchor drifted)\n\
         - Wrong: `damask dispute <edge_id> --reason ...`\n\n\
         Unclosed findings on refactored code rot open forever — this is the #1 source of stale \
         knowledge in production graphs. If none of these were affected by your changes, simply \
         finish — you will not be asked again."
    ))
}

/// Count active open edges attached to a file via its spans, if the index is usable.
fn known_edge_count(q: &IndexQuery, file: &str) -> Option<usize> {
    let spans = q.spans_for_file(file).ok()?;
    let mut edge_ids: Vec<String> = Vec::new();
    for span in &spans {
        for edge in q.edges_for_span_open(&span.id).ok()? {
            if !edge_ids.contains(&edge.id) {
                edge_ids.push(edge.id);
            }
        }
    }
    Some(edge_ids.len())
}

fn build_reason(project: &DamaskProject, edited_files: &[String]) -> String {
    // Index access is best-effort: without it we still nudge, just without counts.
    let db_path = project.damask_dir.join("index.db");
    let edges_dir = project.damask_dir.join("edges");
    let conn = update_index_with_mode(&db_path, &edges_dir, IndexMode::ViewsPreferred).ok();

    let mut file_lines = String::new();
    for file in edited_files.iter().take(MAX_FILES_IN_REASON) {
        let annotation = conn
            .as_ref()
            .and_then(|c| known_edge_count(&IndexQuery::new(c), file))
            .map(|n| match n {
                0 => " (no damask edges yet)".to_string(),
                1 => format!(" (1 known edge — `damask at {file}`)"),
                n => format!(" ({n} known edges — `damask at {file}`)"),
            })
            .unwrap_or_default();
        file_lines.push_str(&format!("- {file}{annotation}\n"));
    }
    if edited_files.len() > MAX_FILES_IN_REASON {
        file_lines.push_str(&format!(
            "- … and {} more\n",
            edited_files.len() - MAX_FILES_IN_REASON
        ));
    }

    format!(
        "Damask harvest: you edited files this session but recorded nothing in the knowledge graph.\n\n\
         Files touched:\n{file_lines}\n\
         Before finishing, preserve anything durable you learned:\n\
         - New risk/gotcha/decision/dependency: `damask record <file> <start> <end> <rel> '{{\"summary\":\"...\",\"confidence\":0.8}}'`\n\
         - Your work confirmed an existing edge: `damask endorse <edge_id>`\n\
         - Your work contradicted or resolved one: `damask dispute <edge_id> --reason incorrect` / `damask close <edge_id> --reason resolved`\n\n\
         If nothing durable was learned this session, simply finish — you will not be asked again."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_transcript(dir: &Path, lines: &[serde_json::Value]) -> PathBuf {
        let path = dir.join("transcript.jsonl");
        let content: String = lines.iter().map(|l| format!("{l}\n")).collect();
        std::fs::write(&path, content).unwrap();
        path
    }

    fn tool_use_line(name: &str, input: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "type": "assistant",
            "message": {
                "role": "assistant",
                "content": [{"type": "tool_use", "name": name, "input": input}]
            }
        })
    }

    #[test]
    fn scan_detects_edits_and_dedups() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let transcript = write_transcript(
            root,
            &[
                tool_use_line(
                    "Edit",
                    serde_json::json!({"file_path": root.join("src/a.rs").to_string_lossy()}),
                ),
                tool_use_line(
                    "Write",
                    serde_json::json!({"file_path": root.join("src/a.rs").to_string_lossy()}),
                ),
                tool_use_line(
                    "Edit",
                    serde_json::json!({"file_path": root.join("src/b.rs").to_string_lossy()}),
                ),
            ],
        );
        let activity = scan_transcript(&transcript, root).unwrap();
        assert_eq!(activity.edited_files, vec!["src/a.rs", "src/b.rs"]);
        assert!(!activity.recorded);
    }

    #[test]
    fn scan_detects_damask_writes() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let transcript = write_transcript(
            root,
            &[
                tool_use_line(
                    "Edit",
                    serde_json::json!({"file_path": root.join("src/a.rs").to_string_lossy()}),
                ),
                tool_use_line(
                    "Bash",
                    serde_json::json!({"command": "damask record src/a.rs 1 5 risk '{\"summary\":\"x\"}'"}),
                ),
            ],
        );
        let activity = scan_transcript(&transcript, root).unwrap();
        assert!(activity.recorded);
    }

    #[test]
    fn write_detection_survives_flags_and_paths() {
        // Global flags between binary and subcommand.
        assert!(is_damask_write(
            "damask --ns decisions record src/a.rs 1 5 risk '{}'"
        ));
        // Non-PATH invocation.
        assert!(is_damask_write(
            "./target/debug/damask record src/a.rs 1 5 risk '{}'"
        ));
        // Piped batch.
        assert!(is_damask_write("echo 'e_1' | damask endorse --batch"));
        // Read-only commands don't count, even querying a file named record.rs.
        assert!(!is_damask_write("damask at src/record.rs:10"));
        assert!(!is_damask_write("damask orient"));
        // Write subcommand without damask doesn't count.
        assert!(!is_damask_write("git record something"));
    }

    #[test]
    fn scan_ignores_readonly_damask_commands() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let transcript = write_transcript(
            root,
            &[tool_use_line(
                "Bash",
                serde_json::json!({"command": "damask at src/a.rs:10"}),
            )],
        );
        let activity = scan_transcript(&transcript, root).unwrap();
        assert!(!activity.recorded);
        assert!(activity.edited_files.is_empty());
    }

    #[test]
    fn scan_excludes_internal_and_external_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let transcript = write_transcript(
            root,
            &[
                tool_use_line(
                    "Edit",
                    serde_json::json!({"file_path": root.join(".damask/edges/x.jsonl").to_string_lossy()}),
                ),
                tool_use_line(
                    "Edit",
                    serde_json::json!({"file_path": root.join(".claude/settings.json").to_string_lossy()}),
                ),
                tool_use_line(
                    "Edit",
                    serde_json::json!({"file_path": "/somewhere/else/entirely.rs"}),
                ),
            ],
        );
        let activity = scan_transcript(&transcript, root).unwrap();
        assert!(activity.edited_files.is_empty());
    }

    #[test]
    fn scan_skips_corrupt_lines() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let path = root.join("transcript.jsonl");
        std::fs::write(
            &path,
            format!(
                "not json at all\n{}\n",
                tool_use_line(
                    "Edit",
                    serde_json::json!({"file_path": root.join("src/a.rs").to_string_lossy()})
                )
            ),
        )
        .unwrap();
        let activity = scan_transcript(&path, root).unwrap();
        assert_eq!(activity.edited_files, vec!["src/a.rs"]);
    }

    #[test]
    fn relative_paths_resolve_against_root() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        assert_eq!(
            project_relative("src/a.rs", root),
            Some("src/a.rs".to_string())
        );
        assert_eq!(project_relative(".damask/edges/x.jsonl", root), None);
    }
}
