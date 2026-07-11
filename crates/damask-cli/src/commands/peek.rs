//! Point-of-use context injection.
//!
//! One hook command, three Claude Code events:
//! - `PostToolUse` on Read/Edit/Write/MultiEdit/NotebookEdit — injects the
//!   top-ranked edges for the file the agent just touched, at the exact
//!   moment they matter.
//! - `UserPromptSubmit` — matches the prompt's keywords against the FTS
//!   index and injects relevant edges before exploration starts.
//! - `Stop` — the broadcast backstop: if broadcast-flagged edges landed
//!   during the session and were never drained by the events above (agent
//!   was composing its final answer, no more tool calls), block the stop
//!   once with a reconciliation ask. PostToolUse delivery is the latency
//!   optimization; Stop delivery is the guarantee.
//!
//! Broadcast edges (`payload.broadcast: true`, within a 24h window) ride
//! along on every event, repo-wide: their relevance is temporal, not
//! spatial — "the world changed since you started."
//!
//! A session-scoped seen-cache (`.damask/.session/<id>.seen`) guarantees an
//! edge is injected at most once per session, so repeated reads of the same
//! file don't repeat context and a delivered broadcast never blocks a stop.
//! Like all hook commands, every error path is silent exit 0 — a hook must
//! never break a session.

use chrono::Utc;
use damask_core::PayloadEnvelope;
use damask_store::index::query::EdgeRow;
use damask_store::{
    rank_edges, update_index_with_mode, DamaskProject, IndexMode, IndexQuery, RankedEdge,
    RankingInput,
};
use std::collections::HashSet;
use std::env;
use std::io::Read as _;
use std::path::PathBuf;

use crate::error::Result;

use super::helpers::project_relative;

/// Tools whose PostToolUse event carries a file worth annotating.
const CONTEXT_TOOLS: &[&str] = &["Read", "Edit", "Write", "MultiEdit", "NotebookEdit"];

/// Maximum edges injected per event.
const MAX_INJECT: usize = 3;
/// Maximum broadcast edges appended per event.
const MAX_BROADCAST: usize = 3;
/// Summary truncation width.
const SUMMARY_WIDTH: usize = 120;
/// Candidate pool ranked before seen-filtering.
const CANDIDATE_LIMIT: usize = 12;
/// Seen-cache files older than this are pruned.
const SEEN_TTL: std::time::Duration = std::time::Duration::from_secs(7 * 24 * 60 * 60);
/// Maximum keywords fed to FTS from a prompt.
const MAX_KEYWORDS: usize = 8;

const STOPWORDS: &[&str] = &[
    "this", "that", "with", "from", "what", "when", "where", "does", "have", "will", "about",
    "should", "could", "would", "there", "their", "then", "than", "them", "they", "please", "into",
    "just", "like", "make", "need", "want", "your", "file", "code", "also", "some", "more", "over",
    "very", "been", "being", "were", "each", "which", "while", "after", "before", "because",
    "through", "look", "show", "help", "work", "change", "update",
];

enum Mode {
    File(String),
    Prompt(String),
    Stop,
}

pub fn run(
    file: Option<&str>,
    prompt: Option<&str>,
    stop: bool,
    session: Option<&str>,
) -> Result<()> {
    // Manual mode (testing) via flags; hook mode reads JSON from stdin.
    let (mode, session_id, hook_event) = if let Some(f) = file {
        (Mode::File(f.to_string()), session.map(String::from), None)
    } else if let Some(p) = prompt {
        (Mode::Prompt(p.to_string()), session.map(String::from), None)
    } else if stop {
        (Mode::Stop, session.map(String::from), None)
    } else {
        let mut input = String::new();
        if std::io::stdin().read_to_string(&mut input).is_err() {
            return Ok(());
        }
        let Ok(hook) = serde_json::from_str::<serde_json::Value>(&input) else {
            return Ok(());
        };
        let session_id = hook
            .get("session_id")
            .and_then(|v| v.as_str())
            .map(String::from);
        match hook.get("hook_event_name").and_then(|v| v.as_str()) {
            Some("PostToolUse") => {
                let tool = hook.get("tool_name").and_then(|v| v.as_str()).unwrap_or("");
                if !CONTEXT_TOOLS.contains(&tool) {
                    return Ok(());
                }
                let path = hook
                    .pointer("/tool_input/file_path")
                    .or_else(|| hook.pointer("/tool_input/notebook_path"))
                    .and_then(|v| v.as_str());
                match path {
                    Some(p) => (Mode::File(p.to_string()), session_id, Some("PostToolUse")),
                    None => return Ok(()),
                }
            }
            Some("UserPromptSubmit") => match hook.get("prompt").and_then(|v| v.as_str()) {
                Some(p) => (
                    Mode::Prompt(p.to_string()),
                    session_id,
                    Some("UserPromptSubmit"),
                ),
                None => return Ok(()),
            },
            Some("Stop") => {
                // Never block twice in a stop chain (mirrors harvest).
                if hook
                    .get("stop_hook_active")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
                {
                    return Ok(());
                }
                (Mode::Stop, session_id, Some("Stop"))
            }
            _ => return Ok(()),
        }
    };

    // Fail open at every step.
    let Ok(cwd) = env::current_dir() else {
        return Ok(());
    };
    let Ok(project) = DamaskProject::discover(&cwd) else {
        return Ok(());
    };
    let db_path = project.damask_dir.join("index.db");
    let edges_dir = project.damask_dir.join("edges");
    let Ok(conn) = update_index_with_mode(&db_path, &edges_dir, IndexMode::ViewsPreferred) else {
        return Ok(());
    };
    let Ok(config) = project.read_config() else {
        return Ok(());
    };
    let q = IndexQuery::new(&conn);
    let seen = load_seen(&project, session_id.as_deref());

    // Broadcasts ride along on every event; unseen ones only.
    let broadcasts: Vec<EdgeRow> = super::helpers::fresh_broadcasts(&q, Utc::now())
        .into_iter()
        .filter(|e| !seen.contains(&e.id))
        .take(MAX_BROADCAST)
        .collect();

    // Stop boundary: the backstop. Anything still unseen here was recorded
    // after the agent's last drain — block once so the final answer can
    // reconcile against it. Delivered broadcasts are already in the
    // seen-cache, so a well-drained session stops silently.
    if matches!(mode, Mode::Stop) {
        if broadcasts.is_empty() {
            return Ok(());
        }
        let reason = render_stop_reason(&broadcasts);
        let output = serde_json::json!({
            "decision": "block",
            "reason": reason,
        });
        println!("{}", serde_json::to_string(&output).unwrap());
        record_seen(
            &project,
            session_id.as_deref(),
            broadcasts.iter().map(|e| e.id.as_str()),
        );
        return Ok(());
    }

    let (candidates, subject) = match &mode {
        Mode::File(path) => {
            let Some(rel) = project_relative(path, &project.root) else {
                return Ok(());
            };
            (
                super::helpers::ranked_edges_for_file(&q, &config, &rel, None, CANDIDATE_LIMIT),
                Some(rel),
            )
        }
        Mode::Prompt(text) => (prompt_candidates(&project, &q, &config, text), None),
        Mode::Stop => unreachable!("handled above"),
    };

    let fresh: Vec<&RankedEdge> = candidates
        .iter()
        .filter(|r| !seen.contains(&r.edge.id))
        .take(MAX_INJECT)
        .collect();
    let fresh_ids: HashSet<&str> = fresh.iter().map(|r| r.edge.id.as_str()).collect();
    let broadcasts: Vec<&EdgeRow> = broadcasts
        .iter()
        .filter(|e| !fresh_ids.contains(e.id.as_str()))
        .collect();
    if fresh.is_empty() && broadcasts.is_empty() {
        return Ok(());
    }

    let mut text = if fresh.is_empty() {
        String::new()
    } else {
        render(&q, &fresh, subject.as_deref())
    };
    if !broadcasts.is_empty() {
        text.push_str(&render_broadcasts(&broadcasts));
    }
    match hook_event {
        Some(event) => {
            let output = serde_json::json!({
                "hookSpecificOutput": {
                    "hookEventName": event,
                    "additionalContext": text,
                }
            });
            println!("{}", serde_json::to_string(&output).unwrap());
        }
        None => print!("{text}"),
    }

    record_seen(
        &project,
        session_id.as_deref(),
        fresh
            .iter()
            .map(|r| r.edge.id.as_str())
            .chain(broadcasts.iter().map(|e| e.id.as_str())),
    );

    Ok(())
}

/// One display line for a broadcast edge.
fn broadcast_line(edge: &EdgeRow) -> String {
    let payload: serde_json::Value =
        serde_json::from_str(&edge.payload).unwrap_or(serde_json::json!({}));
    let env = PayloadEnvelope::new(&payload);
    let conf = env
        .confidence()
        .map(|c| format!(" ({c:.2})"))
        .unwrap_or_default();
    let summary = env
        .summary()
        .map(|s| s.to_string())
        .unwrap_or_else(|| damask_core::truncate_str(&edge.payload, SUMMARY_WIDTH).to_string());
    let trunc = damask_core::truncate_str(&summary, SUMMARY_WIDTH);
    format!("- {}{conf}: {trunc} [{}]\n", edge.rel, edge.id)
}

/// Non-blocking broadcast section appended to file/prompt injections.
fn render_broadcasts(edges: &[&EdgeRow]) -> String {
    let mut text = String::from("[damask] Broadcast — repo-wide notice(s) from the last 24h:\n");
    for edge in edges {
        text.push_str(&broadcast_line(edge));
    }
    text.push_str(
        "If one affects your current work, act on it; `damask endorse <id>` confirms, `damask dispute <id>` contests.\n",
    );
    text
}

/// Blocking Stop-boundary ask: a reconciliation question, not a reading
/// assignment — the agent must check the notices against its conclusions,
/// not merely acknowledge them.
fn render_stop_reason(edges: &[EdgeRow]) -> String {
    let mut lines = String::new();
    for edge in edges {
        lines.push_str(&broadcast_line(edge));
    }
    format!(
        "Damask broadcast: {} repo-wide notice(s) landed after you last checked the graph:\n\n{lines}\n\
         Before finishing, check whether any of these change the work or conclusions of this session. \
         If one does, address it now. If your session confirms a notice, `damask endorse <edge_id>`; \
         if it contradicts what you found, `damask dispute <edge_id> --reason ...`. \
         If none apply, simply finish — you will not be asked again.",
        edges.len()
    )
}

/// Ranked open edges relevant to the prompt. FTS keyword matching by
/// default (a hook must stay fast); semantic matching via ck when
/// `DAMASK_PEEK_SEMANTIC=1` and ck is installed (~0.5s warm).
fn prompt_candidates(
    project: &DamaskProject,
    q: &IndexQuery,
    config: &damask_core::DamaskConfig,
    prompt: &str,
) -> Vec<RankedEdge> {
    let now = Utc::now();

    if std::env::var("DAMASK_PEEK_SEMANTIC").as_deref() == Ok("1") {
        if let Some(hits) = crate::ck::semantic_edge_hits(project, prompt, CANDIDATE_LIMIT) {
            let inputs: Vec<RankingInput> = hits
                .iter()
                .filter_map(|h| q.edge_by_id(&h.edge_id).ok().flatten())
                .filter(|e| !e.is_closed)
                .map(|edge| {
                    let weight = super::helpers::edge_resolution_weight(q, &edge);
                    super::helpers::ranking_input(q, config, edge, weight, now)
                })
                .collect();
            return rank_edges(inputs, CANDIDATE_LIMIT);
        }
    }

    let keywords = extract_keywords(prompt);
    if keywords.is_empty() {
        return Vec::new();
    }
    let query = keywords.join(" OR ");
    let inputs: Vec<RankingInput> = q
        .search_fts_open(&query, None, None)
        .unwrap_or_default()
        .into_iter()
        .take(CANDIDATE_LIMIT * 2)
        .map(|edge| {
            let weight = super::helpers::edge_resolution_weight(q, &edge);
            super::helpers::ranking_input(q, config, edge, weight, now)
        })
        .collect();
    rank_edges(inputs, CANDIDATE_LIMIT)
}

/// Lowercased alphanumeric tokens (length ≥ 4), stopwords removed, deduped.
/// Tokens are FTS5-safe by construction.
fn extract_keywords(prompt: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for token in prompt.split(|c: char| !c.is_ascii_alphanumeric()) {
        let t = token.to_ascii_lowercase();
        if t.len() < 4 || STOPWORDS.contains(&t.as_str()) || !seen.insert(t.clone()) {
            continue;
        }
        out.push(t);
        if out.len() >= MAX_KEYWORDS {
            break;
        }
    }
    out
}

/// Explicit staleness marker for an edge's anchor span. Words, not just
/// glyphs: injected context is consumed by models that act on text.
fn staleness_marker(q: &IndexQuery, edge: &EdgeRow) -> &'static str {
    let span = super::at::edge_target_span_id(edge).and_then(|id| q.span_by_id(id).ok().flatten());
    let Some(span) = span else {
        return "";
    };
    crate::output::render::freshness_words(span.resolution.as_deref(), span.recency.as_deref())
}

fn render(q: &IndexQuery, edges: &[&RankedEdge], subject: Option<&str>) -> String {
    let mut text = match subject {
        Some(path) => format!("[damask] Known annotations for {path}:\n"),
        None => "[damask] Possibly relevant knowledge for this request:\n".to_string(),
    };
    for re in edges {
        let payload: serde_json::Value =
            serde_json::from_str(&re.edge.payload).unwrap_or(serde_json::json!({}));
        let env = PayloadEnvelope::new(&payload);
        let conf = env
            .confidence()
            .map(|c| format!(" ({c:.2})"))
            .unwrap_or_default();
        let disputed = if re.dispute_count > 0 {
            " [disputed]"
        } else {
            ""
        };
        let stale = staleness_marker(q, &re.edge);
        let summary = env.summary().map(|s| s.to_string()).unwrap_or_else(|| {
            damask_core::truncate_str(&re.edge.payload, SUMMARY_WIDTH).to_string()
        });
        let trunc = damask_core::truncate_str(&summary, SUMMARY_WIDTH);
        text.push_str(&format!(
            "- {}{conf}{stale}{disputed}: {trunc} [{}]\n",
            re.edge.rel, re.edge.id
        ));
    }
    match subject {
        Some(path) => text.push_str(&format!("More: `damask at {path}`\n")),
        None => text.push_str("Query further with `damask search \"<terms>\"`.\n"),
    }
    text
}

// ---------------------------------------------------------------------------
// Session seen-cache
// ---------------------------------------------------------------------------

fn seen_path(project: &DamaskProject, session_id: Option<&str>) -> Option<PathBuf> {
    let sid: String = session_id?
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if sid.is_empty() {
        return None;
    }
    Some(
        project
            .damask_dir
            .join(".session")
            .join(format!("{sid}.seen")),
    )
}

fn load_seen(project: &DamaskProject, session_id: Option<&str>) -> HashSet<String> {
    seen_path(project, session_id)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|c| c.lines().map(String::from).collect())
        .unwrap_or_default()
}

fn record_seen<'a>(
    project: &DamaskProject,
    session_id: Option<&str>,
    ids: impl Iterator<Item = &'a str>,
) {
    let Some(path) = seen_path(project, session_id) else {
        return;
    };
    let Some(dir) = path.parent() else {
        return;
    };
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    // Self-ignoring directory: works even in projects whose .damask/.gitignore
    // predates the session cache.
    let marker = dir.join(".gitignore");
    if !marker.exists() {
        let _ = std::fs::write(&marker, "*\n");
    }
    prune_stale(dir);

    let mut content: String = ids.map(|id| format!("{id}\n")).collect();
    if let Ok(existing) = std::fs::read_to_string(&path) {
        content = existing + &content;
    }
    let _ = std::fs::write(&path, content);
}

/// Remove seen-cache files from long-finished sessions.
fn prune_stale(dir: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let now = std::time::SystemTime::now();
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) != Some("seen") {
            continue;
        }
        let stale = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|m| now.duration_since(m).ok())
            .is_some_and(|age| age > SEEN_TTL);
        if stale {
            let _ = std::fs::remove_file(p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keywords_drop_stopwords_and_short_tokens() {
        let kw = extract_keywords("Please fix the auth timeout in token validation");
        assert_eq!(kw, vec!["auth", "timeout", "token", "validation"]);
    }

    #[test]
    fn keywords_dedupe_and_cap() {
        let kw =
            extract_keywords("auth auth auth alpha beta gamma delta epsilon zeta theta iota kappa");
        assert_eq!(kw.len(), MAX_KEYWORDS);
        assert_eq!(kw.iter().filter(|k| *k == "auth").count(), 1);
    }

    #[test]
    fn keywords_are_fts_safe() {
        // Punctuation and operators must not leak into tokens.
        let kw = extract_keywords("what's NEAR(\"this\") AND co-change OR src/auth.rs?");
        for k in &kw {
            assert!(
                k.chars().all(|c| c.is_ascii_alphanumeric()),
                "unsafe token: {k}"
            );
        }
    }
}
