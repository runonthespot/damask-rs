use anyhow::Context;
use chrono::Utc;
use damask_core::{Freshness, PayloadEnvelope, Recency, Resolution};
use damask_store::{
    rank_edges, token_overlap_ratio, update_index_with_mode, DamaskProject, IndexMode, IndexQuery,
    RankedEdge, RankingInput,
};
use std::env;

use crate::error::Result;
use crate::output::glyphs;
use crate::output::render::{self, freshness_glyph};
use crate::output::Format;

/// Maximum edges displayed by default.
const DEFAULT_LIMIT: usize = 12;

#[allow(clippy::too_many_arguments)]
pub fn run(
    location: &str,
    format: Format,
    all: bool,
    no_rank: bool,
    rel_filter: Option<&str>,
    tag_filter: Option<&str>,
    uncontested: bool,
    show_closed: bool,
    offset: usize,
) -> Result<()> {
    let (file, line) = parse_location(location)?;

    let cwd = env::current_dir().context("failed to get current directory")?;
    let project = DamaskProject::discover(&cwd)
        .map_err(|e| anyhow::anyhow!("{}", e))
        .context("no .damask/ found — run `damask init` first")?;

    // Build/update the index
    let db_path = project.damask_dir.join("index.db");
    let edges_dir = project.damask_dir.join("edges");
    let conn = update_index_with_mode(&db_path, &edges_dir, IndexMode::ViewsPreferred)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

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
        // Never dead-end: a 0-result must mean "nothing there", not "you
        // asked at the wrong granularity".
        // 1) file:line with no spans at that line, but spans elsewhere in
        //    the file → show the file's annotated regions.
        if line.is_some() {
            let file_spans = q
                .spans_for_file(&file)
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            if !file_spans.is_empty() {
                match format {
                    Format::Human => {
                        println!(
                            "No spans at {location} — {} annotated region(s) elsewhere in this file:",
                            file_spans.len()
                        );
                        for s in file_spans.iter().take(8) {
                            let lines = match (s.line_start, s.line_end) {
                                (Some(a), Some(b)) => format!(":{}-{}", a, b),
                                _ => String::new(),
                            };
                            let glyph =
                                freshness_glyph(s.resolution.as_deref(), s.recency.as_deref());
                            println!("  {}{} ({}) {}", s.path, lines, s.id, glyph);
                        }
                        println!("  Next: damask at {file}");
                    }
                    Format::Json => {
                        let regions: Vec<serde_json::Value> =
                            file_spans.iter().map(render::span_json).collect();
                        println!(
                            "{}",
                            serde_json::json!({"spans": [], "edges": [], "mode": "in_file_fallback", "file_regions": regions, "next": format!("damask at {file}")})
                        );
                    }
                }
                return Ok(());
            }
        }
        // 2) path prefix (directory) rollup: per-file heat map.
        let rollup = q
            .open_edge_counts_by_path_prefix(&file)
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        if !rollup.is_empty() {
            let total: u32 = rollup.iter().map(|(_, n)| n).sum();
            match format {
                Format::Human => {
                    println!(
                        "No single span at {location} — {} open edges across {} files under it:",
                        total,
                        rollup.len()
                    );
                    for (path, n) in rollup.iter().take(10) {
                        println!("  {path}  ({n} edges)");
                    }
                    if rollup.len() > 10 {
                        println!("  ... and {} more files", rollup.len() - 10);
                    }
                    if let Some((top, _)) = rollup.first() {
                        println!("  Next: damask at {top}");
                    }
                }
                Format::Json => {
                    let files: Vec<serde_json::Value> = rollup
                        .iter()
                        .map(|(path, n)| serde_json::json!({"path": path, "edges": n}))
                        .collect();
                    println!(
                        "{}",
                        serde_json::json!({"spans": [], "edges": [], "mode": "rollup", "files": files})
                    );
                }
            }
            return Ok(());
        }
        // 3) genuinely empty: teach the record template.
        match format {
            Format::Human => {
                println!("No spans at {location}");
                println!(
                    "  Record the first: damask record {file} <start> <end> risk -m \"what you found\" -c 0.8"
                );
            }
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
    let mut span_cache: std::collections::HashMap<
        String,
        Option<damask_store::index::query::SpanRow>,
    > = std::collections::HashMap::new();

    // Helper: look up a span ID, using cache
    let mut lookup_span = |id: &str| -> Option<damask_store::index::query::SpanRow> {
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
        let edges = if show_closed {
            q.edges_for_span(&span.id)
                .map_err(|e| anyhow::anyhow!("{}", e))?
        } else {
            q.edges_for_span_open(&span.id)
                .map_err(|e| anyhow::anyhow!("{}", e))?
        };

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
                    from_id.and_then(&mut lookup_span)
                } else if to_is_queried {
                    to_id.and_then(&mut lookup_span)
                } else {
                    // Neither matches queried spans — try from_id first, then to_id
                    from_id
                        .and_then(&mut lookup_span)
                        .or_else(|| to_id.and_then(&mut lookup_span))
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

            let schema_factor = {
                let payload: serde_json::Value =
                    serde_json::from_str(&edge.payload).unwrap_or(serde_json::json!({}));
                config.schema_rank_factor(&edge.ns, &payload)
            };
            all_inputs.push(RankingInput {
                edge,
                endorsement_count,
                dispute_count,
                effective_ts,
                half_life_days: half_life,
                now,
                resolution_weight,
                signal_density,
                schema_factor,
            });
        }
    }

    // Apply native filters
    let has_filters = rel_filter.is_some() || tag_filter.is_some() || uncontested;
    if has_filters {
        all_inputs.retain(|input| {
            if let Some(rel) = rel_filter {
                if input.edge.rel != rel {
                    return false;
                }
            }
            if let Some(tag) = tag_filter {
                let payload: serde_json::Value =
                    serde_json::from_str(&input.edge.payload).unwrap_or(serde_json::json!({}));
                let env = PayloadEnvelope::new(&payload);
                let tags = env.tags().unwrap_or_default();
                if !tags.contains(&tag) {
                    return false;
                }
            }
            if uncontested && input.dispute_count > 0 {
                return false;
            }
            true
        });
    }

    let graph_stats = q.graph_stats().map_err(|e| anyhow::anyhow!("{}", e))?;
    let closed_hidden = if !show_closed {
        graph_stats.closed_edges
    } else {
        0
    };

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

    let total_before_page = ranked.len();
    // Apply offset
    let ranked: Vec<_> = ranked.into_iter().skip(offset).collect();
    let count = ranked.len();

    // Precompute target spans for freshness glyphs in human output
    let mut target_spans: std::collections::HashMap<String, damask_store::index::query::SpanRow> =
        std::collections::HashMap::new();
    for re in &ranked {
        if let Some(target_id) = edge_target_span_id(&re.edge) {
            if !target_spans.contains_key(target_id) {
                if let Some(span) = lookup_span(target_id) {
                    target_spans.insert(target_id.to_string(), span);
                }
            }
        }
    }

    match format {
        Format::Human => print_human(
            &spans,
            &ranked,
            location,
            &target_spans,
            offset,
            count,
            total_before_page,
            closed_hidden,
        ),
        Format::Json => print_json(
            &spans,
            &ranked,
            &target_spans,
            offset,
            limit,
            count,
            total_before_page,
            closed_hidden,
            &graph_stats,
        ),
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

#[allow(clippy::too_many_arguments)]
fn print_human(
    spans: &[damask_store::index::query::SpanRow],
    ranked: &[RankedEdge],
    location: &str,
    target_spans: &std::collections::HashMap<String, damask_store::index::query::SpanRow>,
    offset: usize,
    count: usize,
    total: usize,
    closed_hidden: u64,
) {
    // Print span header with freshness glyph
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

        let glyph = freshness_glyph(span.resolution.as_deref(), span.recency.as_deref());

        let glyph_suffix = if glyph.is_empty() {
            String::new()
        } else {
            format!(" {}", glyph)
        };

        println!(
            "\n{}{} ({}){}{}",
            span.path, lines, span.id, glyph_suffix, snippet
        );
        // A drifted anchor carries its own repair instructions: confirm if
        // the annotation still holds, dispute the edge if it doesn't.
        if !glyph.is_empty() && glyph != glyphs::EXACT_UNCHANGED {
            println!(
                "  drifted — still true? `damask confirm {}` · wrong now? `damask dispute <edge_id> --reason stale`",
                span.id
            );
        }
        println!();
    }

    if ranked.is_empty() {
        println!("  No edges at {location}");
        return;
    }

    for re in ranked {
        let payload: serde_json::Value =
            serde_json::from_str(&re.edge.payload).unwrap_or(serde_json::json!({}));
        let env = PayloadEnvelope::new(&payload);

        let cluster = render::signal_cluster(&env, re.endorsement_count, re.dispute_count);

        // Summary
        let summary = env
            .summary()
            .unwrap_or_else(|| damask_core::truncate_str(re.edge.payload.as_str(), 60));

        let target_glyph = edge_target_span_id(&re.edge)
            .and_then(|id| target_spans.get(id))
            .map(|span| freshness_glyph(span.resolution.as_deref(), span.recency.as_deref()))
            .unwrap_or("");
        let target_suffix = if target_glyph.is_empty() {
            String::new()
        } else {
            format!(" {}", target_glyph)
        };

        // Namespace + date
        let date = re.edge.ts.split('T').next().unwrap_or(&re.edge.ts);

        println!(
            "  {}{}{}{} — {}",
            render::rel_glyph(&re.edge.rel),
            re.edge.rel,
            cluster,
            target_suffix,
            summary,
        );

        // Action line
        if let Some(action) = env.action() {
            println!("    action: {}", action);
        }

        // The edge ID closes the loop: an agent that just read a claim can
        // endorse/dispute/close it without a second lookup query.
        println!("    [{}, {}] {}", re.edge.ns, date, re.edge.id);
        println!();
    }

    let start = offset + 1;
    let end = offset + count;
    let closed_hint = if closed_hidden > 0 {
        format!(" ({} closed hidden, use --show-closed)", closed_hidden)
    } else {
        String::new()
    };
    if count < total {
        println!(
            "Showing {}-{} of {} edges{}",
            start, end, total, closed_hint
        );
        let next_offset = offset + count;
        println!("  Next: damask at {} --offset {next_offset}", location);
    } else {
        println!("  {} edges shown{}", count, closed_hint);
    }
}

pub(crate) fn parse_resolution(s: &str) -> Option<Resolution> {
    match s {
        "exact" => Some(Resolution::Exact),
        "relocated" => Some(Resolution::Relocated),
        "unresolved" => Some(Resolution::Unresolved),
        "missing" => Some(Resolution::Missing),
        _ => None,
    }
}

pub(crate) fn parse_recency(s: &str) -> Option<Recency> {
    match s {
        "unchanged" => Some(Recency::Unchanged),
        "file_changed" => Some(Recency::FileChanged),
        "unknown" => Some(Recency::Unknown),
        _ => None,
    }
}

pub(crate) fn edge_target_span_id(edge: &damask_store::index::query::EdgeRow) -> Option<&str> {
    edge.to_id
        .as_deref()
        .filter(|id| id.starts_with("s_"))
        .or_else(|| edge.from_id.as_deref().filter(|id| id.starts_with("s_")))
}

#[allow(clippy::too_many_arguments)]
fn print_json(
    spans: &[damask_store::index::query::SpanRow],
    ranked: &[RankedEdge],
    target_spans: &std::collections::HashMap<String, damask_store::index::query::SpanRow>,
    offset: usize,
    limit: usize,
    count: usize,
    total: usize,
    closed_hidden: u64,
    graph_stats: &damask_store::GraphStats,
) {
    let spans_json: Vec<serde_json::Value> = spans.iter().map(render::span_json).collect();

    let edges_json: Vec<serde_json::Value> = ranked
        .iter()
        .map(|re| {
            let anchor = edge_target_span_id(&re.edge).and_then(|id| target_spans.get(id));
            render::edge_json(re, anchor)
        })
        .collect();

    let output = serde_json::json!({
        "context": {
            "graph": {
                "total_edges": graph_stats.total_edges,
                "active_edges": graph_stats.active_edges,
                "closed_edges": graph_stats.closed_edges,
            },
            "query": {
                "closed_hidden": closed_hidden,
            },
            "showing": {
                "offset": offset,
                "limit": limit,
                "count": count,
                "total": total,
            },
        },
        "spans": spans_json,
        "edges": edges_json,
    });

    println!("{}", serde_json::to_string_pretty(&output).unwrap());
}
