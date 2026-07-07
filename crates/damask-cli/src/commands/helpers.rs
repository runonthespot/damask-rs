use anyhow::{bail, Context};
use damask_core::{DamaskId, Edge, EdgeId, Freshness, Recency, Resolution, Span, SpanId};
use damask_store::index::query::{EdgeRow, SpanRow};
use damask_store::{DamaskProject, IndexQuery};
use std::path::Path;

use crate::error::Result;

/// Resolution weight from a span row's stored freshness columns.
pub fn span_freshness_weight(span: &SpanRow) -> f64 {
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

/// Signal density from a snippet + payload pair: penalizes summaries that
/// merely restate the anchored snippet. 1.0 when either side is absent.
pub fn payload_signal_density(snippet: Option<&str>, payload: &str) -> f64 {
    let Some(snippet) = snippet else {
        return 1.0;
    };
    let payload: serde_json::Value =
        serde_json::from_str(payload).unwrap_or(serde_json::json!({}));
    let env = damask_core::PayloadEnvelope::new(&payload);
    match env.summary() {
        Some(summary) => {
            let overlap = damask_store::token_overlap_ratio(summary, snippet);
            1.0 - (overlap * 0.5)
        }
        None => 1.0,
    }
}

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
    span_freshness_weight(&span)
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
    payload_signal_density(snippet.as_deref(), &edge.payload)
}

/// Re-emit a span fact with the SAME id and a fresh anchor at its
/// effective location (append-only re-anchoring — `confirm`'s core, shared
/// with `sweep`). Returns None when the file/range can't anchor.
pub fn reanchor_span(
    project: &DamaskProject,
    row: &damask_store::index::query::SpanRow,
) -> Option<Span> {
    let (start, end) = (row.line_start?, row.line_end?);
    let file_path = project.root.join(&row.path);
    if !file_path.exists() {
        return None;
    }
    let (snippet, content_hash) = extract_span_content(&file_path, start, end).ok()?;
    let span_id = match DamaskId::parse(&row.id).ok()? {
        DamaskId::Span(s) => s,
        _ => return None,
    };
    Some(Span {
        id: span_id,
        path: row.path.clone(),
        lines: Some([start, end]),
        snippet,
        symbol: row.symbol.clone(),
        content_hash,
        commit: git_head_commit(&project.root),
        ns: row.ns.clone(),
        ts: chrono::Utc::now(),
        agent: ambient_agent(),
        session: ambient_session(),
    })
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

/// Resolve a possibly-abbreviated span/edge id: a full valid id passes
/// through untouched (no index open); otherwise a case-insensitive
/// unique-prefix match against the index. 28-char ULIDs that must
/// round-trip byte-perfect are a syntax cliff — `e_01KH3K` should just
/// work, and an ambiguous or unknown prefix should say so.
pub fn resolve_id(project: &DamaskProject, input: &str) -> Result<String> {
    if DamaskId::parse(input).is_ok() {
        return Ok(input.to_string());
    }
    if !(input.starts_with("s_") || input.starts_with("e_")) {
        bail!("'{input}' is not a valid span or edge ID (expected s_/e_ prefix)");
    }
    let db_path = project.damask_dir.join("index.db");
    let edges_dir = project.damask_dir.join("edges");
    let conn = damask_store::update_index_with_mode(
        &db_path,
        &edges_dir,
        damask_store::IndexMode::ViewsPreferred,
    )
    .map_err(|e| anyhow::anyhow!("{}", e))?;
    let q = IndexQuery::new(&conn);
    let matches = q
        .ids_with_prefix(input)
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    match matches.len() {
        0 => bail!("no span or edge id starts with '{input}'"),
        1 => Ok(matches[0].clone()),
        n => bail!(
            "id prefix '{input}' is ambiguous ({n}+ matches): {}",
            matches.join(", ")
        ),
    }
}

/// FTS5-safe query: every whitespace token is double-quoted so payload
/// text like `read-modify-write` or `f(x)` matches literally instead of
/// being parsed as FTS syntax ("no such column: modify"). Tokens compose
/// with FTS5's implicit AND, same as unquoted plain words.
pub fn sanitize_fts_query(query: &str) -> String {
    query
        .split_whitespace()
        .map(|t| format!("\"{}\"", t.replace('"', "")))
        .collect::<Vec<_>>()
        .join(" ")
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

/// Validate payload fields damask consumes downstream. Catches the silent
/// poisons at write time: an out-of-range confidence ranks above every
/// legitimate fact forever; a string confidence silently vanishes from
/// numeric predicates; and `lint` flags neither.
pub fn validate_payload(value: &serde_json::Value) -> Result<()> {
    let Some(obj) = value.as_object() else {
        return Ok(());
    };
    if let Some(c) = obj.get("confidence") {
        match c.as_f64() {
            Some(f) if (0.0..=1.0).contains(&f) => {}
            Some(f) => {
                let hint = if f > 1.0 && f <= 10.0 {
                    format!(" — did you mean {}?", f / 10.0)
                } else if f > 10.0 && f <= 100.0 {
                    format!(" — did you mean {}?", f / 100.0)
                } else {
                    String::new()
                };
                bail!("confidence must be between 0.0 and 1.0 (got {f}{hint})");
            }
            None => {
                if let Some(s) = c.as_str() {
                    if s.parse::<f64>().is_ok() {
                        bail!(
                            "confidence must be a JSON number, not a string \
                             (got \"{s}\" — remove the quotes)"
                        );
                    }
                }
                bail!("confidence must be a number between 0.0 and 1.0 (got {c})");
            }
        }
    }
    if let Some(s) = obj.get("summary") {
        if !s.is_string() {
            bail!("summary must be a string (got {s})");
        }
    }
    if let Some(t) = obj.get("tags") {
        if !t.is_array() {
            bail!("tags must be an array of strings (got {t})");
        }
    }
    Ok(())
}

/// Payload from JSON sources merged with flag-provided fields (flags win),
/// then validated. The flag path is the one a model guesses from `git
/// commit -m` muscle memory — it must succeed on attempt 1, and a failed
/// JSON payload must teach it.
#[allow(clippy::too_many_arguments)]
pub fn compose_payload(
    inline: Option<&str>,
    file: Option<&str>,
    stdin: bool,
    summary: Option<&str>,
    confidence: Option<f64>,
    action: Option<&str>,
    severity: Option<&str>,
    tags: &[String],
) -> Result<serde_json::Value> {
    let mut value = resolve_payload(inline, file, stdin).map_err(|e| {
        if inline.is_some() {
            anyhow::anyhow!(
                "{e:#}\n  tip: you can skip JSON entirely — \
                 use -m \"what you found\" -c 0.9 instead of a JSON payload"
            )
        } else {
            e
        }
    })?;

    let has_flags = summary.is_some()
        || confidence.is_some()
        || action.is_some()
        || severity.is_some()
        || !tags.is_empty();
    if has_flags {
        let Some(obj) = value.as_object_mut() else {
            bail!("payload must be a JSON object to combine with -m/-c/--action/--tag flags");
        };
        if let Some(s) = summary {
            obj.insert("summary".to_string(), serde_json::json!(s));
        }
        if let Some(c) = confidence {
            obj.insert("confidence".to_string(), serde_json::json!(c));
        }
        if let Some(a) = action {
            obj.insert("action".to_string(), serde_json::json!(a));
        }
        if let Some(sv) = severity {
            obj.insert("severity".to_string(), serde_json::json!(sv));
        }
        if !tags.is_empty() {
            obj.insert("tags".to_string(), serde_json::json!(tags));
        }
    }

    validate_payload(&value)?;
    Ok(value)
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
///
/// When the file exists, the line range is bounds-checked against it: a
/// hallucinated range would otherwise create a span that silently skips
/// the resolution cascade forever.
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
        let line_count = std::fs::read_to_string(&file_path)
            .context("failed to read file")?
            .lines()
            .count() as u32;
        if start > line_count {
            bail!("line {start} is past the end of {file} ({line_count} lines)");
        }
        if end > line_count {
            bail!(
                "line range {start}-{end} exceeds {file} ({line_count} lines) — \
                 use {start}-{line_count} to annotate through the end of the file"
            );
        }
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
