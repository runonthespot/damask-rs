use anyhow::Context;
use damask_core::PayloadEnvelope;
use damask_store::{needs_inactive_edges, update_index_with_mode, DamaskProject, IndexMode, IndexQuery, Predicate};
use std::env;

use crate::error::Result;
use crate::output::Format;

pub fn run(predicate_strs: &[String], since: Option<&str>, limit: usize, format: Format, ns: Option<&str>) -> Result<()> {
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

    // Use all_edges_ns (includes inactive) when predicates need superseded edges
    let all_edges = if needs_inactive_edges(&preds) {
        q.all_edges_ns(ns).map_err(|e| anyhow::anyhow!("{}", e))?
    } else {
        q.all_active_edges_ns(ns).map_err(|e| anyhow::anyhow!("{}", e))?
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

    // Apply limit
    matched.truncate(limit);

    let predicate_display = predicate_strs.join(" AND ");

    match format {
        Format::Human => print_human(&matched, &predicate_display),
        Format::Json => print_json(&matched),
    }

    Ok(())
}

fn print_human(matched: &[(&damask_store::index::query::EdgeRow, u32, u32)], predicate: &str) {
    if matched.is_empty() {
        println!("No edges matching: {predicate}");
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

    println!("  {} edges matching: {}", matched.len(), predicate);
}

fn print_json(matched: &[(&damask_store::index::query::EdgeRow, u32, u32)]) {
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

    let output = serde_json::json!({ "edges": edges_json });
    println!("{}", serde_json::to_string_pretty(&output).unwrap());
}
