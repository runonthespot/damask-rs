use anyhow::Context;
use chrono::Utc;
use damask_core::{Freshness, PayloadEnvelope, Recency, Resolution};
use damask_store::{
    rank_edges, token_overlap_ratio, update_index, DamaskProject, IndexQuery, RankedEdge,
    RankingInput,
};
use std::env;

use crate::error::Result;
use crate::output::glyphs;
use crate::output::Format;

/// Maximum edges displayed by default.
const DEFAULT_LIMIT: usize = 12;

pub fn run(location: &str, format: Format, all: bool, no_rank: bool) -> Result<()> {
    let (file, line) = parse_location(location)?;

    let cwd = env::current_dir().context("failed to get current directory")?;
    let project = DamaskProject::discover(&cwd)
        .map_err(|e| anyhow::anyhow!("{}", e))
        .context("no .damask/ found — run `damask init` first")?;

    // Build/update the index
    let db_path = project.damask_dir.join("index.db");
    let edges_dir = project.damask_dir.join("edges");
    let conn = update_index(&db_path, &edges_dir).map_err(|e| anyhow::anyhow!("{}", e))?;

    let config = project
        .read_config()
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let q = IndexQuery::new(&conn);

    // Find spans at this location
    let spans = if let Some(line) = line {
        q.spans_at(&file, line)
            .map_err(|e| anyhow::anyhow!("{}", e))?
    } else {
        q.spans_for_file(&file)
            .map_err(|e| anyhow::anyhow!("{}", e))?
    };

    if spans.is_empty() {
        match format {
            Format::Human => println!("No spans at {location}"),
            Format::Json => println!("{{\"spans\":[],\"edges\":[]}}"),
        }
        return Ok(());
    }

    // Collect all edges for all matching spans
    let now = Utc::now();
    let mut all_inputs = Vec::new();
    let mut seen_edge_ids = std::collections::HashSet::new();

    // Build set of queried span IDs for context resolution
    let queried_span_ids: std::collections::HashSet<&str> =
        spans.iter().map(|s| s.id.as_str()).collect();

    // Cache span lookups to avoid redundant DB queries
    let mut span_cache: std::collections::HashMap<String, Option<damask_store::index::query::SpanRow>> =
        std::collections::HashMap::new();

    // Helper: look up a span ID, using cache
    let mut lookup_span =
        |id: &str| -> Option<damask_store::index::query::SpanRow> {
            span_cache
                .entry(id.to_string())
                .or_insert_with(|| {
                    if id.starts_with("s_") {
                        q.span_by_id(id).ok().flatten()
                    } else {
                        None
                    }
                })
                .clone()
        };

    for span in &spans {
        let edges = q
            .edges_for_span(&span.id)
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        for edge in edges {
            if seen_edge_ids.contains(&edge.id) {
                continue;
            }
            seen_edge_ids.insert(edge.id.clone());

            let endorsement_count = q
                .endorsement_count(&edge.id)
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            let dispute_count = q
                .dispute_count(&edge.id)
                .map_err(|e| anyhow::anyhow!("{}", e))?;

            // Effective timestamp: latest endorsement or edge creation
            let effective_ts = q
                .latest_endorsement_ts(&edge.id)
                .map_err(|e| anyhow::anyhow!("{}", e))?
                .and_then(|ts| chrono::DateTime::parse_from_rfc3339(&ts).ok())
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|| {
                    chrono::DateTime::parse_from_rfc3339(&edge.ts)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or(now)
                });

            let half_life = config.decay_half_life_days(&edge.ns);

            // Determine the contextually relevant span for this edge.
            // Prefer the span ID that matches one of the queried spans;
            // otherwise try from_id then to_id.
            let context_span = {
                let from_id = edge.from_id.as_deref();
                let to_id = edge.to_id.as_deref();

                // Check if either side matches the queried spans
                let from_is_queried = from_id.is_some_and(|id| queried_span_ids.contains(id));
                let to_is_queried = to_id.is_some_and(|id| queried_span_ids.contains(id));

                if from_is_queried {
                    from_id.and_then(|id| lookup_span(id))
                } else if to_is_queried {
                    to_id.and_then(|id| lookup_span(id))
                } else {
                    // Neither matches queried spans — try from_id first, then to_id
                    from_id
                        .and_then(|id| lookup_span(id))
                        .or_else(|| to_id.and_then(|id| lookup_span(id)))
                }
            };

            // Compute resolution weight from span freshness
            let resolution_weight = context_span
                .as_ref()
                .map(|span| {
                    let resolution = span
                        .resolution
                        .as_deref()
                        .and_then(parse_resolution)
                        .unwrap_or(Resolution::Exact);
                    let recency = span
                        .recency
                        .as_deref()
                        .and_then(parse_recency)
                        .unwrap_or(Recency::Unknown);
                    Freshness::new(resolution, recency).resolution_weight()
                })
                .unwrap_or(1.0);

            // Compute signal density from token overlap
            let signal_density = context_span
                .and_then(|span| span.snippet)
                .map(|snippet| {
                    let payload: serde_json::Value =
                        serde_json::from_str(&edge.payload).unwrap_or(serde_json::json!({}));
                    let env = PayloadEnvelope::new(&payload);
                    if let Some(summary) = env.summary() {
                        let overlap = token_overlap_ratio(summary, &snippet);
                        // High overlap = restatement = lower density
                        1.0 - (overlap * 0.5)
                    } else {
                        1.0
                    }
                })
                .unwrap_or(1.0);

            all_inputs.push(RankingInput {
                edge,
                endorsement_count,
                dispute_count,
                effective_ts,
                half_life_days: half_life,
                now,
                resolution_weight,
                signal_density,
            });
        }
    }

    let limit = if all { usize::MAX } else { DEFAULT_LIMIT };

    let ranked = if no_rank {
        // Sort chronologically instead of by score
        let mut ranked: Vec<RankedEdge> = all_inputs
            .drain(..)
            .map(|input| RankedEdge {
                edge: input.edge,
                score: 0.0,
                endorsement_count: input.endorsement_count,
                dispute_count: input.dispute_count,
            })
            .collect();
        ranked.sort_by(|a, b| a.edge.ts.cmp(&b.edge.ts));
        ranked.truncate(limit);
        ranked
    } else {
        rank_edges(all_inputs, limit)
    };

    match format {
        Format::Human => print_human(&spans, &ranked, location),
        Format::Json => print_json(&spans, &ranked),
    }

    Ok(())
}

/// Parse "file:line" or just "file".
fn parse_location(s: &str) -> Result<(String, Option<u32>)> {
    if let Some((file, line_str)) = s.rsplit_once(':') {
        if let Ok(line) = line_str.parse::<u32>() {
            return Ok((file.to_string(), Some(line)));
        }
    }
    // No valid :line suffix — treat the whole thing as a file path
    Ok((s.to_string(), None))
}

fn print_human(
    spans: &[damask_store::index::query::SpanRow],
    ranked: &[RankedEdge],
    location: &str,
) {
    // Print span header
    for span in spans {
        let lines = match (span.line_start, span.line_end) {
            (Some(s), Some(e)) => format!(":{}-{}", s, e),
            _ => String::new(),
        };
        let snippet = span
            .snippet
            .as_deref()
            .map(|s| format!(" — \"{}\"", s))
            .unwrap_or_default();
        println!("\n{}{} ({}){}\n", span.path, lines, span.id, snippet);
    }

    if ranked.is_empty() {
        println!("  No edges at {location}");
        return;
    }

    for re in ranked {
        let payload: serde_json::Value =
            serde_json::from_str(&re.edge.payload).unwrap_or(serde_json::json!({}));
        let env = PayloadEnvelope::new(&payload);

        // Rel glyph
        let rel_glyph = match re.edge.rel.as_str() {
            "risk" => "\u{26A0} ",        // ⚠
            "gotcha" => "\u{26A0} ",      // ⚠
            "contradicts" => "\u{2717} ", // ✗
            "conflicts_with" => "\u{2717} ",
            _ => "  ",
        };

        // Dispute marker
        let dispute_marker = if re.dispute_count > 0 {
            format!(" {}", glyphs::DISPUTED)
        } else {
            String::new()
        };

        // Confidence
        let conf = env
            .confidence()
            .map(|c| format!(" ({:.2})", c))
            .unwrap_or_default();

        // Endorsement/dispute counts
        let endorsement_str = if re.endorsement_count > 0 {
            format!(" \u{00D7}{}\u{2713}", re.endorsement_count)
        } else {
            String::new()
        };
        let dispute_str = if re.dispute_count > 0 {
            format!(" \u{00D7}{}\u{2717}", re.dispute_count)
        } else {
            String::new()
        };

        // Summary
        let summary = env
            .summary()
            .unwrap_or_else(|| damask_core::truncate_str(re.edge.payload.as_str(), 60));

        // Namespace + date
        let date = re.edge.ts.split('T').next().unwrap_or(&re.edge.ts);

        println!(
            "  {}{}{}{}{}{} — {}",
            rel_glyph, re.edge.rel, conf, endorsement_str, dispute_str, dispute_marker, summary,
        );

        // Action line
        if let Some(action) = env.action() {
            println!("    action: {}", action);
        }

        println!("    [{}, {}]", re.edge.ns, date);
        println!();
    }

    let total = ranked.len();
    println!("  {} edges shown", total);
}

fn parse_resolution(s: &str) -> Option<Resolution> {
    match s {
        "exact" => Some(Resolution::Exact),
        "relocated" => Some(Resolution::Relocated),
        "unresolved" => Some(Resolution::Unresolved),
        "missing" => Some(Resolution::Missing),
        _ => None,
    }
}

fn parse_recency(s: &str) -> Option<Recency> {
    match s {
        "unchanged" => Some(Recency::Unchanged),
        "file_changed" => Some(Recency::FileChanged),
        "unknown" => Some(Recency::Unknown),
        _ => None,
    }
}

fn print_json(spans: &[damask_store::index::query::SpanRow], ranked: &[RankedEdge]) {
    let spans_json: Vec<serde_json::Value> = spans
        .iter()
        .map(|s| {
            serde_json::json!({
                "id": s.id,
                "path": s.path,
                "line_start": s.line_start,
                "line_end": s.line_end,
                "snippet": s.snippet,
            })
        })
        .collect();

    let edges_json: Vec<serde_json::Value> = ranked
        .iter()
        .map(|re| {
            let payload: serde_json::Value =
                serde_json::from_str(&re.edge.payload).unwrap_or(serde_json::json!({}));
            serde_json::json!({
                "id": re.edge.id,
                "from": re.edge.from_id,
                "to": re.edge.to_id,
                "rel": re.edge.rel,
                "payload": payload,
                "ns": re.edge.ns,
                "ts": re.edge.ts,
                "score": re.score,
                "endorsements": re.endorsement_count,
                "disputes": re.dispute_count,
            })
        })
        .collect();

    let output = serde_json::json!({
        "spans": spans_json,
        "edges": edges_json,
    });

    println!("{}", serde_json::to_string_pretty(&output).unwrap());
}
