use anyhow::{bail, Context};
use damask_core::{DamaskId, Edge, EdgeId, Freshness, Recency, Resolution, Span, SpanId};
use damask_store::index::query::EdgeRow;
use damask_store::{DamaskProject, IndexQuery};
use std::path::Path;

use crate::error::Result;

/// Resolution weight for an edge, from the stored freshness of its first
/// span endpoint (`from` preferred). 1.0 when no span is attached or the
/// lookup fails — an unanchored edge can't be stale by location.
pub fn edge_resolution_weight(q: &IndexQuery, edge: &EdgeRow) -> f64 {
    let span_id = edge
        .from_id
        .as_deref()
        .filter(|id| id.starts_with("s_"))
        .or_else(|| edge.to_id.as_deref().filter(|id| id.starts_with("s_")));
    let Some(id) = span_id else {
        return 1.0;
    };
    let Some(span) = q.span_by_id(id).ok().flatten() else {
        return 1.0;
    };
    let resolution = span
        .resolution
        .as_deref()
        .and_then(super::at::parse_resolution)
        .unwrap_or(Resolution::Exact);
    let recency = span
        .recency
        .as_deref()
        .and_then(super::at::parse_recency)
        .unwrap_or(Recency::Unknown);
    Freshness::new(resolution, recency).resolution_weight()
}

/// Ambient agent identity for provenance stamping. `DAMASK_AGENT` wins;
/// otherwise Claude Code sessions are detected via their env markers.
pub fn ambient_agent() -> Option<String> {
    std::env::var("DAMASK_AGENT")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            let in_claude = std::env::var("CLAUDECODE").is_ok()
                || std::env::var("CLAUDE_CODE_SESSION_ID").is_ok()
                || std::env::var("CLAUDE_SESSION_ID").is_ok();
            in_claude.then(|| "claude-code".to_string())
        })
}

/// Ambient session identity for provenance stamping. `DAMASK_SESSION` wins;
/// falls back to Claude Code's session id when present.
/// Claude Code exports `CLAUDE_CODE_SESSION_ID`; `CLAUDE_SESSION_ID` is kept
/// as a legacy fallback. Without this, every fact stamps `session: None` and
/// convergent verification counts N independent sessions as 1.
pub fn ambient_session() -> Option<String> {
    std::env::var("DAMASK_SESSION")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            std::env::var("CLAUDE_CODE_SESSION_ID")
                .ok()
                .filter(|s| !s.is_empty())
        })
        .or_else(|| {
            std::env::var("CLAUDE_SESSION_ID")
                .ok()
                .filter(|s| !s.is_empty())
        })
}

/// Make a path root-relative, excluding files outside the project and
/// internal bookkeeping (.damask/, .claude/, .git/, .agents/).
pub fn project_relative(path: &str, root: &Path) -> Option<String> {
    let p = Path::new(path);
    let rel = if p.is_absolute() {
        p.strip_prefix(root).ok()?
    } else {
        p
    };
    let first = rel.components().next()?;
    if matches!(
        first.as_os_str().to_str(),
        Some(".damask") | Some(".claude") | Some(".git") | Some(".agents")
    ) {
        return None;
    }
    Some(rel.to_string_lossy().to_string())
}

/// Build a cheap RankingInput: endorsement/dispute counts and decay, with
/// the supplied resolution weight.
pub fn ranking_input(
    q: &IndexQuery,
    config: &damask_core::DamaskConfig,
    edge: EdgeRow,
    resolution_weight: f64,
    now: chrono::DateTime<chrono::Utc>,
) -> damask_store::RankingInput {
    let endorsement_count = q.endorsement_count(&edge.id).unwrap_or(0);
    let dispute_count = q.dispute_count(&edge.id).unwrap_or(0);
    let effective_ts = chrono::DateTime::parse_from_rfc3339(&edge.ts)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or(now);
    let half_life = config.decay_half_life_days(&edge.ns);
    damask_store::RankingInput {
        edge,
        endorsement_count,
        dispute_count,
        effective_ts,
        half_life_days: half_life,
        now,
        resolution_weight,
        signal_density: 1.0,
    }
}

/// Ranked open edges attached to a file via its spans, optionally limited
/// to spans overlapping a 1-indexed inclusive line range.
pub fn ranked_edges_for_file(
    q: &IndexQuery,
    config: &damask_core::DamaskConfig,
    rel_path: &str,
    range: Option<(u32, u32)>,
    limit: usize,
) -> Vec<damask_store::RankedEdge> {
    let now = chrono::Utc::now();
    let spans = q.spans_for_file(rel_path).unwrap_or_default();
    let mut inputs = Vec::new();
    let mut seen_ids = std::collections::HashSet::new();
    for span in &spans {
        if let Some((start, end)) = range {
            let overlaps = match (span.line_start, span.line_end) {
                (Some(s), Some(e)) => s <= end && e >= start,
                _ => true, // whole-file span overlaps everything
            };
            if !overlaps {
                continue;
            }
        }
        let resolution_weight = {
            let resolution = span
                .resolution
                .as_deref()
                .and_then(super::at::parse_resolution)
                .unwrap_or(Resolution::Exact);
            let recency = span
                .recency
                .as_deref()
                .and_then(super::at::parse_recency)
                .unwrap_or(Recency::Unknown);
            Freshness::new(resolution, recency).resolution_weight()
        };
        for edge in q.edges_for_span_open(&span.id).unwrap_or_default() {
            if seen_ids.insert(edge.id.clone()) {
                inputs.push(ranking_input(q, config, edge, resolution_weight, now));
            }
        }
    }
    damask_store::rank_edges(inputs, limit)
}

/// Signal-density score for an edge: penalizes summaries that merely
/// restate the anchored snippet. 1.0 when no span/snippet/summary exists.
pub fn edge_signal_density(q: &IndexQuery, edge: &EdgeRow) -> f64 {
    let span_id = edge
        .from_id
        .as_deref()
        .filter(|id| id.starts_with("s_"))
        .or_else(|| edge.to_id.as_deref().filter(|id| id.starts_with("s_")));
    let snippet = span_id
        .and_then(|id| q.span_by_id(id).ok().flatten())
        .and_then(|s| s.snippet);
    let Some(snippet) = snippet else {
        return 1.0;
    };
    let payload: serde_json::Value =
        serde_json::from_str(&edge.payload).unwrap_or(serde_json::json!({}));
    let env = damask_core::PayloadEnvelope::new(&payload);
    match env.summary() {
        Some(summary) => {
            let overlap = damask_store::token_overlap_ratio(summary, &snippet);
            1.0 - (overlap * 0.5)
        }
        None => 1.0,
    }
}

/// Resolve the active namespace from an override flag, env var, or project config.
pub fn resolve_ns(project: &DamaskProject, ns_override: Option<&str>) -> Result<String> {
    if let Some(ns) = ns_override {
        return Ok(ns.to_string());
    }
    project
        .active_ns()
        .ok_or_else(|| anyhow::anyhow!("no active namespace — use `damask ns set <name>` or --ns"))
}

/// Extract snippet (first line, truncated to 80 chars) and content hash from a file region.
pub fn extract_span_content(
    path: &Path,
    start: u32,
    end: u32,
) -> Result<(Option<String>, Option<String>)> {
    let content = std::fs::read_to_string(path).context("failed to read file")?;
    let lines: Vec<&str> = content.lines().collect();

    let start_idx = (start as usize).saturating_sub(1);
    let end_idx = (end as usize).min(lines.len());

    if start_idx >= lines.len() {
        return Ok((None, None));
    }

    // Snippet: first line, truncated to 80 chars.
    let first_line = lines[start_idx];
    let snippet = if first_line.len() > 80 {
        let mut end = 77;
        while end > 0 && !first_line.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}...", &first_line[..end])
    } else {
        first_line.to_string()
    };

    // Content hash: truncated SHA-256 of span text.
    let span_text: String = lines[start_idx..end_idx].join("\n");
    let hash = damask_resolve::content_hash(&span_text);

    Ok((Some(snippet), Some(hash)))
}

/// Get the current git HEAD commit hash, or None if not in a git repo.
pub fn git_head_commit(root: &Path) -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            let hash = String::from_utf8(o.stdout).ok()?;
            let trimmed = hash.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        })
}

/// Parse an endpoint string: "_" → None, otherwise parse as DamaskId.
pub fn parse_endpoint(s: &str) -> Result<Option<DamaskId>> {
    if s == "_" {
        Ok(None)
    } else {
        let id = DamaskId::parse(s)
            .map_err(|e| anyhow::anyhow!("{}", e))
            .context(format!("'{s}' is not a valid span or edge ID"))?;
        Ok(Some(id))
    }
}

/// Resolve the payload from inline JSON, a file, stdin, or default to empty object.
pub fn resolve_payload(
    inline: Option<&str>,
    file: Option<&str>,
    stdin: bool,
) -> Result<serde_json::Value> {
    let source_count = inline.is_some() as u8 + file.is_some() as u8 + stdin as u8;
    if source_count > 1 {
        bail!("cannot specify more than one of: inline payload, --file, --stdin");
    }

    if let Some(path) = file {
        let content = std::fs::read_to_string(path)
            .context(format!("failed to read payload file: {path}"))?;
        let value: serde_json::Value =
            serde_json::from_str(&content).context("payload file is not valid JSON")?;
        return Ok(value);
    }

    if stdin {
        let mut content = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut content)
            .context("failed to read from stdin")?;
        let value: serde_json::Value =
            serde_json::from_str(&content).context("stdin is not valid JSON")?;
        return Ok(value);
    }

    if let Some(json_str) = inline {
        let value: serde_json::Value =
            serde_json::from_str(json_str).context("payload is not valid JSON")?;
        return Ok(value);
    }

    Ok(serde_json::json!({}))
}

/// Build a complete Span from parameters, computing content hash and git commit.
pub fn build_span(
    project: &DamaskProject,
    file: &str,
    start: u32,
    end: u32,
    symbol: Option<&str>,
    ns: &str,
) -> Result<Span> {
    let file_path = project.root.join(file);
    let (snippet, content_hash) = if file_path.exists() {
        extract_span_content(&file_path, start, end)?
    } else {
        (None, None)
    };

    let commit = git_head_commit(&project.root);

    Ok(Span {
        id: SpanId::new(),
        path: file.to_string(),
        lines: Some([start, end]),
        snippet,
        symbol: symbol.map(|s| s.to_string()),
        content_hash,
        commit,
        ns: ns.to_string(),
        ts: chrono::Utc::now(),
        agent: ambient_agent(),
        session: ambient_session(),
    })
}

/// Build a complete Edge from parameters.
pub fn build_edge(
    from: Option<DamaskId>,
    to: Option<DamaskId>,
    rel: &str,
    payload: serde_json::Value,
    ns: &str,
) -> Edge {
    Edge {
        id: EdgeId::new(),
        from,
        to,
        rel: rel.to_string(),
        payload,
        ns: ns.to_string(),
        ts: chrono::Utc::now(),
        agent: ambient_agent(),
        session: ambient_session(),
    }
}
