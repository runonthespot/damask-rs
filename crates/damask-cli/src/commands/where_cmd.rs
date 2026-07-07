use anyhow::Context;
use damask_core::PayloadEnvelope;
use damask_store::{needs_inactive_edges, update_index_with_mode, DamaskProject, GraphStats, IndexMode, IndexQuery, Predicate};
use std::env;

use crate::error::Result;
use crate::output::Format;

pub fn run(predicate_strs: &[String], since: Option<&str>, limit: usize, offset: usize, show_closed: bool, format: Format, ns: Option<&str>) -> Result<()> {
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
        q.all_active_edges_ns(ns).map_err(|e| anyhow::anyhow!("{}", e))?
    } else {
        q.all_active_open_edges_ns(ns).map_err(|e| anyhow::anyhow!("{}", e))?
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
        if preds.iter().all(|p| p.matches(edge, endorsement_count, dispute_count)) {
            matched.push((edge, endorsement_count, dispute_count));
        }
    }

    let total = matched.len();

    // Apply offset + limit
    let page: Vec<_> = matched.into_iter().skip(offset).take(limit).collect();
    let count = page.len();

    let predicate_display = predicate_strs.join(" AND ");

    match format {
        Format::Human => print_human(&page, &predicate_display, offset, limit, count, total, closed_hidden as usize, &preds, &all_edges, &q),
        Format::Json => print_json(&page, offset, limit, count, total, closed_hidden, &graph_stats, &preds, &all_edges, &q),
    }

    Ok(())
}

fn print_human(
    matched: &[(&damask_store::index::query::EdgeRow, u32, u32)],
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
            println!("  Nearest miss: {}={} ({} edges at this level)", near_miss.field, near_miss.nearest_value, near_miss.count_at_nearest);
        }
        if total > 0 {
            println!("  {} total matched before pagination", total);
        }
        let active = all_edges.len();
        println!("  {} active edges exist — try broadening your query", active);
        return;
    }

    println!();
    for (edge, endorsement_count, dispute_count) in matched {
        let payload: serde_json::Value =
            serde_json::from_str(&edge.payload).unwrap_or(serde_json::json!({}));
        let env = PayloadEnvelope::new(&payload);

        // Confidence
        let conf = env
            .confidence()
            .map(|c| format!(" ({:.2})", c))
            .unwrap_or_default();

        // Endorsement/dispute counts
        let endorsement_str = if *endorsement_count > 0 {
            format!(" \u{00D7}{}\u{2713}", endorsement_count)
        } else {
            String::new()
        };
        let dispute_str = if *dispute_count > 0 {
            format!(" \u{00D7}{}\u{2717}", dispute_count)
        } else {
            String::new()
        };

        // Summary
        let summary = env
            .summary()
            .unwrap_or_else(|| damask_core::truncate_str(edge.payload.as_str(), 60));

        // From info
        let from_str = edge.from_id.as_deref().unwrap_or("_");

        let date = edge.ts.split('T').next().unwrap_or(&edge.ts);

        println!(
            "  {} [{}]{}{}{} — {}",
            edge.id, edge.rel, conf, endorsement_str, dispute_str, summary,
        );
        println!("    from: {}  [{}, {}]", from_str, edge.ns, date);
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
    println!("Showing {}-{} of {} edges matching: {}{}", start, end, total, predicate, closed_hint);

    // Next-page hint
    if offset + count < total {
        let next_offset = offset + count;
        let pred_args = predicate.replace(" AND ", "\" \"");
        println!("  Next: damask where \"{pred_args}\" --offset {next_offset}");
    }
}

fn print_json(
    matched: &[(&damask_store::index::query::EdgeRow, u32, u32)],
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
        .map(|(edge, endorsements, disputes)| {
            let payload: serde_json::Value =
                serde_json::from_str(&edge.payload).unwrap_or(serde_json::json!({}));
            serde_json::json!({
                "id": edge.id,
                "from": edge.from_id,
                "to": edge.to_id,
                "rel": edge.rel,
                "payload": payload,
                "ns": edge.ns,
                "ts": edge.ts,
                "endorsements": endorsements,
                "disputes": disputes,
            })
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
            && matches!(p.op, CompareOp::Gt | CompareOp::Gte | CompareOp::Lt | CompareOp::Lte)
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
        values.iter().copied().filter(|&v| v <= threshold).fold(None, |acc: Option<f64>, v| {
            Some(acc.map_or(v, |a: f64| a.max(v)))
        })?
    } else {
        // Looking for values < threshold but none found; find min value above threshold
        values.iter().copied().filter(|&v| v >= threshold).fold(None, |acc: Option<f64>, v| {
            Some(acc.map_or(v, |a: f64| a.min(v)))
        })?
    };

    let count_at_nearest = values.iter().filter(|&&v| (v - nearest).abs() < f64::EPSILON).count();

    Some(NearMiss {
        field: numeric_pred.field.clone(),
        operator: op_str.to_string(),
        threshold,
        nearest_value: nearest,
        count_at_nearest,
    })
}
