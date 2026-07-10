use anyhow::Context;
use damask_core::PayloadEnvelope;
use damask_store::{
    needs_inactive_edges, rank_edges, update_index_with_mode, DamaskProject, GraphStats, IndexMode,
    IndexQuery, Predicate, RankedEdge,
};
use std::collections::HashMap;
use std::env;

use crate::error::Result;
use crate::output::Format;

use super::at::edge_target_span_id;
use crate::output::render::{self, freshness_glyph};
use super::helpers;

/// Result ordering for `where`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum WhereSort {
    /// Freshness- and trust-weighted ranking (same scorer as `at`).
    Rank,
    /// Newest first by creation timestamp.
    Ts,
}

#[allow(clippy::too_many_arguments)]
pub fn run(
    predicate_strs: &[String],
    since: Option<&str>,
    limit: usize,
    offset: usize,
    show_closed: bool,
    sort: WhereSort,
    format: Format,
    ns: Option<&str>,
) -> Result<()> {
    // A boolean operator INSIDE one predicate arg silently matched nothing
    // (exit 0, zero rows) — the classic "the graph taught me it's empty"
    // failure. Catch it and show the multi-arg form.
    for s in predicate_strs {
        if s.contains(" AND ") || s.contains(" OR ") || s.contains("&&") {
            anyhow::bail!(
                "predicates AND-compose as SEPARATE arguments — operators inside one \
                 argument are not parsed.\n  Instead of: damask where \"{s}\"\n  \
                 Use:        damask where {}",
                s.split(" AND ")
                    .flat_map(|p| p.split(" OR "))
                    .flat_map(|p| p.split("&&"))
                    .map(|p| format!("\"{}\"", p.trim()))
                    .collect::<Vec<_>>()
                    .join(" ")
            );
        }
    }
    let preds: Vec<Predicate> = predicate_strs
        .iter()
        .map(|s| Predicate::parse(s).map_err(|e| anyhow::anyhow!("{e}")))
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let cwd = env::current_dir().context("failed to get current directory")?;
    let project = DamaskProject::discover(&cwd)
        .map_err(|e| anyhow::anyhow!("{}", e))
        .context("no .damask/ found — run `damask init` first")?;

    let db_path = project.damask_dir.join("index.db");
    let edges_dir = project.damask_dir.join("edges");
    let conn = update_index_with_mode(&db_path, &edges_dir, IndexMode::ViewsPreferred)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let q = IndexQuery::new(&conn);
    let graph_stats = q.graph_stats().map_err(|e| anyhow::anyhow!("{}", e))?;

    // Use all_edges_ns (includes inactive) when predicates need superseded/closed edges
    let all_edges = if needs_inactive_edges(&preds) {
        q.all_edges_ns(ns).map_err(|e| anyhow::anyhow!("{}", e))?
    } else if show_closed {
        q.all_active_edges_ns(ns)
            .map_err(|e| anyhow::anyhow!("{}", e))?
    } else {
        q.all_active_open_edges_ns(ns)
            .map_err(|e| anyhow::anyhow!("{}", e))?
    };

    // Count closed edges hidden (for diagnostics)
    let closed_hidden = if !show_closed && !needs_inactive_edges(&preds) {
        graph_stats.closed_edges
    } else {
        0
    };

    let mut matched = Vec::new();
    for edge in &all_edges {
        // Apply --since filter
        if let Some(since_date) = since {
            let edge_date = edge.ts.split('T').next().unwrap_or(&edge.ts);
            if edge_date < since_date {
                continue;
            }
        }

        let endorsement_count = q
            .endorsement_count(&edge.id)
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        let dispute_count = q
            .dispute_count(&edge.id)
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        // AND-compose: all predicates must match
        if preds
            .iter()
            .all(|p| p.matches(edge, endorsement_count, dispute_count))
        {
            matched.push((edge.clone(), endorsement_count, dispute_count));
        }
    }

    let total = matched.len();

    // Order results: the same freshness- and trust-weighted scorer `at`
    // uses (so triage leads with live, verified findings), or newest-first.
    let config = project
        .read_config()
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    let now = chrono::Utc::now();
    let ranked: Vec<RankedEdge> = match sort {
        WhereSort::Rank => {
            let inputs: Vec<_> = matched
                .into_iter()
                .map(|(edge, _, _)| {
                    let weight = helpers::edge_resolution_weight(&q, &edge);
                    let density = helpers::edge_signal_density(&q, &edge);
                    let mut input = helpers::ranking_input(&q, &config, edge, weight, now);
                    input.signal_density = density;
                    input
                })
                .collect();
            rank_edges(inputs, usize::MAX)
        }
        WhereSort::Ts => {
            let mut v: Vec<RankedEdge> = matched
                .into_iter()
                .map(|(edge, endorsement_count, dispute_count)| RankedEdge {
                    edge,
                    score: 0.0,
                    endorsement_count,
                    dispute_count,
                })
                .collect();
            v.sort_by(|a, b| b.edge.ts.cmp(&a.edge.ts));
            v
        }
    };

    // Apply offset + limit
    let page: Vec<&RankedEdge> = ranked.iter().skip(offset).take(limit).collect();
    let count = page.len();

    // Anchor spans for location + freshness display (effective path/lines:
    // the index stores the post-rename, post-relocation values).
    let mut anchor_spans: HashMap<String, damask_store::index::query::SpanRow> = HashMap::new();
    for re in &page {
        if let Some(span_id) = edge_target_span_id(&re.edge) {
            if !anchor_spans.contains_key(span_id) {
                if let Some(span) = q.span_by_id(span_id).ok().flatten() {
                    anchor_spans.insert(span_id.to_string(), span);
                }
            }
        }
    }

    let predicate_display = predicate_strs.join(" AND ");

    match format {
        Format::Human => print_human(
            &page,
            &anchor_spans,
            &predicate_display,
            offset,
            limit,
            count,
            total,
            closed_hidden as usize,
            &preds,
            &all_edges,
            &q,
        ),
        Format::Json => print_json(
            &page,
            &anchor_spans,
            offset,
            limit,
            count,
            total,
            closed_hidden,
            &graph_stats,
            &preds,
            &all_edges,
            &q,
        ),
    }

    Ok(())
}

/// Location suffix for an edge's anchor span: `src/auth.rs:42-67 ⚠`.
/// Empty for unanchored edges.
fn anchor_display(
    edge: &damask_store::index::query::EdgeRow,
    anchor_spans: &HashMap<String, damask_store::index::query::SpanRow>,
) -> String {
    let Some(span) = edge_target_span_id(edge).and_then(|id| anchor_spans.get(id)) else {
        return String::new();
    };
    let lines = match (span.line_start, span.line_end) {
        (Some(s), Some(e)) => format!(":{}-{}", s, e),
        _ => String::new(),
    };
    let glyph = freshness_glyph(span.resolution.as_deref(), span.recency.as_deref());
    let glyph_suffix = if glyph.is_empty() {
        String::new()
    } else {
        format!(" {}", glyph)
    };
    format!(" {}{}{}", span.path, lines, glyph_suffix)
}

#[allow(clippy::too_many_arguments)]
fn print_human(
    matched: &[&RankedEdge],
    anchor_spans: &HashMap<String, damask_store::index::query::SpanRow>,
    predicate: &str,
    offset: usize,
    _limit: usize,
    count: usize,
    total: usize,
    closed_hidden: usize,
    preds: &[Predicate],
    all_edges: &[damask_store::index::query::EdgeRow],
    q: &IndexQuery,
) {
    if count == 0 {
        println!("0 edges matching: {predicate}");
        // Near-miss diagnostics for numeric fields
        if let Some(near_miss) = compute_near_miss(preds, all_edges, q) {
            println!(
                "  Nearest miss: {}={} ({} edges at this level)",
                near_miss.field, near_miss.nearest_value, near_miss.count_at_nearest
            );
        }
        if total > 0 {
            println!("  {} total matched before pagination", total);
        }
        let active = all_edges.len();
        println!(
            "  {} active edges exist — try broadening your query",
            active
        );
        return;
    }

    println!();
    for re in matched {
        let edge = &re.edge;
        let payload: serde_json::Value =
            serde_json::from_str(&edge.payload).unwrap_or(serde_json::json!({}));
        let env = PayloadEnvelope::new(&payload);

        let cluster = render::signal_cluster(&env, re.endorsement_count, re.dispute_count);

        // Summary
        let summary = env
            .summary()
            .unwrap_or_else(|| damask_core::truncate_str(edge.payload.as_str(), 60));

        // Anchor location + freshness, straight from the spans table.
        let anchor = anchor_display(edge, anchor_spans);

        let date = edge.ts.split('T').next().unwrap_or(&edge.ts);

        println!(
            "  {} [{}]{}{} — {}",
            edge.id, edge.rel, cluster, anchor, summary,
        );
        if anchor.is_empty() {
            let from_str = edge.from_id.as_deref().unwrap_or("_");
            println!("    from: {}  [{}, {}]", from_str, edge.ns, date);
        } else {
            println!("    [{}, {}]", edge.ns, date);
        }
        println!();
    }

    // Footer with pagination info
    let start = offset + 1;
    let end = offset + count;
    let closed_hint = if closed_hidden > 0 {
        format!(" ({} closed hidden, use --show-closed)", closed_hidden)
    } else {
        String::new()
    };
    println!(
        "Showing {}-{} of {} edges matching: {}{}",
        start, end, total, predicate, closed_hint
    );

    // Next-page hint
    if offset + count < total {
        let next_offset = offset + count;
        let pred_args = predicate.replace(" AND ", "\" \"");
        println!("  Next: damask where \"{pred_args}\" --offset {next_offset}");
    }
}

#[allow(clippy::too_many_arguments)]
fn print_json(
    matched: &[&RankedEdge],
    anchor_spans: &HashMap<String, damask_store::index::query::SpanRow>,
    offset: usize,
    limit: usize,
    count: usize,
    total: usize,
    closed_hidden: u64,
    graph_stats: &GraphStats,
    preds: &[Predicate],
    all_edges: &[damask_store::index::query::EdgeRow],
    q: &IndexQuery,
) {
    let edges_json: Vec<serde_json::Value> = matched
        .iter()
        .map(|re| {
            let anchor = edge_target_span_id(&re.edge).and_then(|id| anchor_spans.get(id));
            render::edge_json(re, anchor)
        })
        .collect();

    let mut output = serde_json::json!({
        "context": {
            "graph": {
                "total_edges": graph_stats.total_edges,
                "active_edges": graph_stats.active_edges,
                "closed_edges": graph_stats.closed_edges,
            },
            "query": {
                "total_matched": total,
                "closed_hidden": closed_hidden,
            },
            "showing": {
                "offset": offset,
                "limit": limit,
                "count": count,
                "total": total,
            },
        },
        "edges": edges_json,
    });

    // Add near-miss diagnostics for zero-result queries
    if count == 0 {
        if let Some(near_miss) = compute_near_miss(preds, all_edges, q) {
            output["context"]["near_miss"] = serde_json::json!({
                "field": near_miss.field,
                "operator": near_miss.operator,
                "threshold": near_miss.threshold,
                "nearest_value": near_miss.nearest_value,
                "count_at_nearest": near_miss.count_at_nearest,
            });
        }
    }

    println!("{}", serde_json::to_string_pretty(&output).unwrap());
}

struct NearMiss {
    field: String,
    operator: String,
    threshold: f64,
    nearest_value: f64,
    count_at_nearest: usize,
}

/// When a query returns 0 results and has a numeric predicate, find the closest non-matching value.
fn compute_near_miss(
    preds: &[Predicate],
    all_edges: &[damask_store::index::query::EdgeRow],
    q: &IndexQuery,
) -> Option<NearMiss> {
    use damask_store::predicate::CompareOp;

    // Find the first numeric-field predicate (confidence or endorsed)
    let numeric_pred = preds.iter().find(|p| {
        matches!(p.field.as_str(), "confidence" | "endorsed" | "disputed")
            && matches!(
                p.op,
                CompareOp::Gt | CompareOp::Gte | CompareOp::Lt | CompareOp::Lte
            )
    })?;

    let threshold: f64 = numeric_pred.value.parse().ok()?;
    let op_str = match numeric_pred.op {
        CompareOp::Gt => ">",
        CompareOp::Gte => ">=",
        CompareOp::Lt => "<",
        CompareOp::Lte => "<=",
        _ => return None,
    };

    // Collect all values for the field
    let mut values: Vec<f64> = Vec::new();
    for edge in all_edges {
        let val = match numeric_pred.field.as_str() {
            "confidence" => {
                let payload: serde_json::Value =
                    serde_json::from_str(&edge.payload).unwrap_or(serde_json::json!({}));
                let env = PayloadEnvelope::new(&payload);
                env.confidence()
            }
            "endorsed" => {
                let c = q.endorsement_count(&edge.id).unwrap_or(0);
                Some(c as f64)
            }
            "disputed" => {
                let c = q.dispute_count(&edge.id).unwrap_or(0);
                Some(c as f64)
            }
            _ => None,
        };
        if let Some(v) = val {
            values.push(v);
        }
    }

    if values.is_empty() {
        return None;
    }

    // Find the closest value that doesn't match the predicate
    let is_gt = matches!(numeric_pred.op, CompareOp::Gt | CompareOp::Gte);
    let nearest = if is_gt {
        // Looking for values > threshold but none found; find max value below threshold
        values
            .iter()
            .copied()
            .filter(|&v| v <= threshold)
            .fold(None, |acc: Option<f64>, v| {
                Some(acc.map_or(v, |a: f64| a.max(v)))
            })?
    } else {
        // Looking for values < threshold but none found; find min value above threshold
        values
            .iter()
            .copied()
            .filter(|&v| v >= threshold)
            .fold(None, |acc: Option<f64>, v| {
                Some(acc.map_or(v, |a: f64| a.min(v)))
            })?
    };

    let count_at_nearest = values
        .iter()
        .filter(|&&v| (v - nearest).abs() < f64::EPSILON)
        .count();

    Some(NearMiss {
        field: numeric_pred.field.clone(),
        operator: op_str.to_string(),
        threshold,
        nearest_value: nearest,
        count_at_nearest,
    })
}
