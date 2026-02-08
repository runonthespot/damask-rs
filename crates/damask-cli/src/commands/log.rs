use anyhow::Context;
use damask_core::PayloadEnvelope;
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

    let spans = q
        .all_spans_chronological()
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    let edges = q
        .all_edges_chronological()
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    match format {
        Format::Human => {
            if spans.is_empty() && edges.is_empty() {
                println!("No facts recorded yet.");
                return Ok(());
            }

            println!();
            println!("Fact log ({} spans, {} edges):", spans.len(), edges.len());
            println!();

            let mut entries: Vec<LogEntry> = Vec::new();

            for span in &spans {
                let lines = match (span.line_start, span.line_end) {
                    (Some(s), Some(e)) => format!(":{}-{}", s, e),
                    _ => String::new(),
                };
                entries.push(LogEntry {
                    ts: span.ts.clone(),
                    display: format!(
                        "  {} span {} {}{}",
                        date_part(&span.ts),
                        span.id,
                        span.path,
                        lines
                    ),
                });
            }

            for edge in &edges {
                let p: serde_json::Value =
                    serde_json::from_str(&edge.payload).unwrap_or(serde_json::json!({}));
                let env = PayloadEnvelope::new(&p);
                let summary = env.summary().unwrap_or("");
                let agent = edge.agent.as_deref().unwrap_or("");
                let active = if edge.is_active { "" } else { " (inactive)" };
                entries.push(LogEntry {
                    ts: edge.ts.clone(),
                    display: format!(
                        "  {} edge {} [{}] {}{} {}",
                        date_part(&edge.ts),
                        edge.id,
                        edge.rel,
                        agent,
                        active,
                        summary
                    ),
                });
            }

            entries.sort_by(|a, b| a.ts.cmp(&b.ts));

            for entry in &entries {
                println!("{}", entry.display);
            }

            println!();
        }
        Format::Json => {
            let spans_json: Vec<serde_json::Value> = spans
                .iter()
                .map(|s| {
                    serde_json::json!({
                        "type": "span",
                        "id": s.id,
                        "path": s.path,
                        "line_start": s.line_start,
                        "line_end": s.line_end,
                        "ns": s.ns,
                        "ts": s.ts,
                    })
                })
                .collect();

            let edges_json: Vec<serde_json::Value> = edges
                .iter()
                .map(|e| {
                    let payload: serde_json::Value =
                        serde_json::from_str(&e.payload).unwrap_or(serde_json::json!({}));
                    serde_json::json!({
                        "type": "edge",
                        "id": e.id,
                        "rel": e.rel,
                        "payload": payload,
                        "ns": e.ns,
                        "ts": e.ts,
                        "is_active": e.is_active,
                    })
                })
                .collect();

            let output = serde_json::json!({
                "spans": spans_json,
                "edges": edges_json,
            });
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
    }

    Ok(())
}

struct LogEntry {
    ts: String,
    display: String,
}

fn date_part(ts: &str) -> &str {
    ts.split('T').next().unwrap_or(ts)
}
