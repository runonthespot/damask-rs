use anyhow::Context;
use damask_store::{update_index, DamaskProject, IndexQuery};
use std::env;

use crate::error::Result;
use crate::output::Format;

pub fn run(format: Format) -> Result<()> {
    let cwd = env::current_dir().context("failed to get current directory")?;
    let project = DamaskProject::discover(&cwd)
        .map_err(|e| anyhow::anyhow!("{}", e))
        .context("no .damask/ found — run `damask init` first")?;

    let db_path = project.damask_dir.join("index.db");
    let edges_dir = project.damask_dir.join("edges");
    let conn = update_index(&db_path, &edges_dir).map_err(|e| anyhow::anyhow!("{}", e))?;

    let q = IndexQuery::new(&conn);
    let stats = q.project_stats().map_err(|e| anyhow::anyhow!("{}", e))?;

    let namespaces = project
        .list_namespaces()
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let active_ns = project.active_ns().unwrap_or_default();

    match format {
        Format::Human => {
            println!();
            println!("Damask status");
            println!("\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}");
            println!(
                "  Namespaces:    {} (active: {})",
                namespaces.len(),
                if active_ns.is_empty() {
                    "none"
                } else {
                    &active_ns
                }
            );
            println!("  Spans:         {}", stats.span_count);
            println!("  Edges:         {} total", stats.edge_count);
            println!("    Active:      {}", stats.active_edge_count);
            println!("    Meta:        {}", stats.meta_edge_count);
            println!("    Superseded:  {}", stats.superseded_count);
            println!("  Endorsements:  {}", stats.endorsement_count);
            println!("  Disputes:      {}", stats.dispute_count);
            println!();

            if stats.empty_payload_count > 0 {
                println!(
                    "  \u{26A0} {} edges with empty payloads",
                    stats.empty_payload_count
                );
            }
            if stats.missing_summary_count > 0 {
                println!(
                    "  \u{26A0} {} edges missing summary",
                    stats.missing_summary_count
                );
            }
        }
        Format::Json => {
            let output = serde_json::json!({
                "namespaces": namespaces.len(),
                "active_ns": if active_ns.is_empty() { None } else { Some(&active_ns) },
                "spans": stats.span_count,
                "edges": {
                    "total": stats.edge_count,
                    "active": stats.active_edge_count,
                    "meta": stats.meta_edge_count,
                    "superseded": stats.superseded_count,
                },
                "endorsements": stats.endorsement_count,
                "disputes": stats.dispute_count,
                "empty_payload_count": stats.empty_payload_count,
                "missing_summary_count": stats.missing_summary_count,
            });
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
    }

    Ok(())
}
