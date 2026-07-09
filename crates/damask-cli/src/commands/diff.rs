use anyhow::Context;
use damask_core::PayloadEnvelope;
use damask_store::{update_index_with_mode, DamaskProject, IndexMode, IndexQuery};
use std::collections::HashSet;
use std::env;

use crate::error::Result;
use crate::output::Format;

pub fn run(ns_a: &str, ns_b: &str, format: Format) -> Result<()> {
    let cwd = env::current_dir().context("failed to get current directory")?;
    let project = DamaskProject::discover(&cwd)
        .map_err(|e| anyhow::anyhow!("{}", e))
        .context("no .damask/ found — run `damask init` first")?;

    let db_path = project.damask_dir.join("index.db");
    let edges_dir = project.damask_dir.join("edges");
    let conn = update_index_with_mode(&db_path, &edges_dir, IndexMode::ViewsPreferred)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let q = IndexQuery::new(&conn);
    let all_edges = q.all_active_edges().map_err(|e| anyhow::anyhow!("{}", e))?;

    let edges_a: Vec<_> = all_edges.iter().filter(|e| e.ns == ns_a).collect();
    let edges_b: Vec<_> = all_edges.iter().filter(|e| e.ns == ns_b).collect();

    let ids_a: HashSet<&str> = edges_a.iter().map(|e| e.id.as_str()).collect();
    let ids_b: HashSet<&str> = edges_b.iter().map(|e| e.id.as_str()).collect();

    let only_a: Vec<_> = edges_a
        .iter()
        .filter(|e| !ids_b.contains(e.id.as_str()))
        .collect();
    let only_b: Vec<_> = edges_b
        .iter()
        .filter(|e| !ids_a.contains(e.id.as_str()))
        .collect();
    let shared: Vec<_> = edges_a
        .iter()
        .filter(|e| ids_b.contains(e.id.as_str()))
        .collect();

    match format {
        Format::Human => {
            println!();
            println!("Diff: {} vs {}", ns_a, ns_b);
            println!();

            if !only_a.is_empty() {
                println!("  Only in {}:", ns_a);
                for edge in &only_a {
                    print_edge_summary(edge);
                }
                println!();
            }

            if !only_b.is_empty() {
                println!("  Only in {}:", ns_b);
                for edge in &only_b {
                    print_edge_summary(edge);
                }
                println!();
            }

            println!(
                "  {} unique to {}, {} unique to {}, {} shared",
                only_a.len(),
                ns_a,
                only_b.len(),
                ns_b,
                shared.len()
            );
        }
        Format::Json => {
            let to_json =
                |edges: &[&&damask_store::index::query::EdgeRow]| -> Vec<serde_json::Value> {
                    edges
                        .iter()
                        .map(|e| {
                            let payload: serde_json::Value =
                                serde_json::from_str(&e.payload).unwrap_or(serde_json::json!({}));
                            serde_json::json!({
                                "id": e.id,
                                "rel": e.rel,
                                "payload": payload,
                                "ts": e.ts,
                            })
                        })
                        .collect()
                };

            let output = serde_json::json!({
                "ns_a": ns_a,
                "ns_b": ns_b,
                "only_a": to_json(&only_a),
                "only_b": to_json(&only_b),
                "shared_count": shared.len(),
            });
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
    }

    Ok(())
}

fn print_edge_summary(edge: &damask_store::index::query::EdgeRow) {
    let payload: serde_json::Value =
        serde_json::from_str(&edge.payload).unwrap_or(serde_json::json!({}));
    let env = PayloadEnvelope::new(&payload);
    let summary = env
        .summary()
        .unwrap_or_else(|| damask_core::truncate_str(&edge.payload, 60));
    let date = edge.ts.split('T').next().unwrap_or(&edge.ts);
    println!("    {} [{}] {} — {}", edge.id, edge.rel, date, summary);
}
