use anyhow::Context;
use damask_core::Fact;
use damask_store::{update_index, DamaskProject, FactReader, FactWriter, IndexQuery};
use std::collections::HashSet;
use std::env;

use crate::error::Result;

/// Compact writes a filtered view to `.damask/edges/.views/<ns>.current.jsonl`.
/// Source JSONL logs are never modified — they are append-only.
/// The `.views/` directory is for external consumers; the index always builds
/// from the raw JSONL logs (list_jsonl_files already filters `.views/`).
pub fn run(namespace: Option<&str>, aggressive: bool) -> Result<()> {
    let cwd = env::current_dir().context("failed to get current directory")?;
    let project = DamaskProject::discover(&cwd)
        .map_err(|e| anyhow::anyhow!("{}", e))
        .context("no .damask/ found — run `damask init` first")?;

    let db_path = project.damask_dir.join("index.db");
    let edges_dir = project.damask_dir.join("edges");
    let conn = update_index(&db_path, &edges_dir).map_err(|e| anyhow::anyhow!("{}", e))?;

    let q = IndexQuery::new(&conn);

    // Determine which namespaces to compact
    let namespaces = if let Some(ns) = namespace {
        vec![ns.to_string()]
    } else {
        project
            .list_namespaces()
            .map_err(|e| anyhow::anyhow!("{}", e))?
    };

    if namespaces.is_empty() {
        println!("No namespaces to compact.");
        return Ok(());
    }

    // Build the set of inactive edge IDs from the index
    let all_edges = q
        .all_edges_chronological()
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let inactive_ids: HashSet<String> = all_edges
        .iter()
        .filter(|e| !e.is_active)
        .map(|e| e.id.clone())
        .collect();

    // In aggressive mode, also filter out unendorsed low-confidence edges
    let aggressive_ids: HashSet<String> = if aggressive {
        all_edges
            .iter()
            .filter(|e| {
                if !e.is_active {
                    return false; // already in inactive_ids
                }
                let p: serde_json::Value =
                    serde_json::from_str(&e.payload).unwrap_or(serde_json::json!({}));
                let conf = p.get("confidence").and_then(|v| v.as_f64()).unwrap_or(1.0);
                let endorsed = q.endorsement_count(&e.id).unwrap_or(0);
                conf < 0.5 && endorsed == 0
            })
            .map(|e| e.id.clone())
            .collect()
    } else {
        HashSet::new()
    };

    // Ensure .views/ directory exists
    let views_dir = edges_dir.join(".views");
    std::fs::create_dir_all(&views_dir).context("failed to create .views/ directory")?;

    let mut total_removed = 0;
    let mut total_kept = 0;

    for ns in &namespaces {
        let jsonl_path = project.edges_file(ns);
        if !jsonl_path.exists() {
            continue;
        }

        let mut reader = FactReader::open(&jsonl_path).map_err(|e| anyhow::anyhow!("{}", e))?;
        let facts = reader.read_all().map_err(|e| anyhow::anyhow!("{}", e))?;

        let mut kept = Vec::new();
        let mut removed = 0;

        for fact in &facts {
            let should_remove = match fact {
                Fact::Edge(edge) => {
                    let id = edge.id.to_string();

                    // Preserve endorsement/dispute signals for active edges
                    if edge.rel == "endorsed" || edge.rel == "disputed" {
                        let target_id = edge.from.as_ref().map(|t| t.to_string());
                        match target_id {
                            Some(tid) => {
                                inactive_ids.contains(&tid) || aggressive_ids.contains(&tid)
                            }
                            None => true, // malformed meta-edge; drop from view
                        }
                    } else {
                        inactive_ids.contains(&id) || aggressive_ids.contains(&id)
                    }
                }
                Fact::Span(_) => false, // never remove spans
            };

            if should_remove {
                removed += 1;
            } else {
                kept.push(fact.clone());
            }
        }

        // Always write the view so external consumers get a fresh snapshot
        let view_path = views_dir.join(format!("{}.current.jsonl", ns));
        FactWriter::write_all(&view_path, &kept).map_err(|e| anyhow::anyhow!("{}", e))?;

        total_removed += removed;
        total_kept += kept.len();
    }

    println!();
    if aggressive {
        println!("Compact (aggressive): {} namespaces", namespaces.len());
    } else {
        println!("Compact: {} namespaces", namespaces.len());
    }
    println!("  Kept: {} facts", total_kept);
    println!("  Removed: {} inactive/filtered edges", total_removed);
    println!("  Views written to: .damask/edges/.views/");
    println!();

    Ok(())
}
