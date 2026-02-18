use anyhow::Context;
use damask_core::PayloadEnvelope;
use damask_store::{update_index_with_mode, DamaskProject, IndexMode, IndexQuery};
use std::env;

use crate::error::Result;
use crate::output::Format;

pub fn run(query: &str, ns: Option<&str>, rel: Option<&str>, format: Format) -> Result<()> {
    let cwd = env::current_dir().context("failed to get current directory")?;
    let project = DamaskProject::discover(&cwd)
        .map_err(|e| anyhow::anyhow!("{}", e))
        .context("no .damask/ found — run `damask init` first")?;

    let db_path = project.damask_dir.join("index.db");
    let edges_dir = project.damask_dir.join("edges");
    let conn = update_index_with_mode(&db_path, &edges_dir, IndexMode::ViewsPreferred)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let q = IndexQuery::new(&conn);

    let results = q
        .search_fts(query, ns, rel)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    match format {
        Format::Human => {
            if results.is_empty() {
                println!("No results for \"{query}\"");
                return Ok(());
            }

            println!();
            println!("Search: \"{}\" ({} results)", query, results.len());
            println!();

            for edge in &results {
                let payload: serde_json::Value =
                    serde_json::from_str(&edge.payload).unwrap_or(serde_json::json!({}));
                let env = PayloadEnvelope::new(&payload);
                let summary = env
                    .summary()
                    .unwrap_or_else(|| damask_core::truncate_str(&edge.payload, 60));
                let date = edge.ts.split('T').next().unwrap_or(&edge.ts);

                println!("  {} [{}] — {}", edge.id, edge.rel, summary);
                println!("    [{}, {}]", edge.ns, date);
                println!();
            }
        }
        Format::Json => {
            let edges_json: Vec<serde_json::Value> = results
                .iter()
                .map(|edge| {
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
                    })
                })
                .collect();

            let output = serde_json::json!({
                "query": query,
                "results": edges_json,
            });
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
    }

    Ok(())
}
