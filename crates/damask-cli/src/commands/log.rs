use anyhow::Context;
use damask_core::PayloadEnvelope;
use damask_store::{update_index, DamaskProject, IndexQuery};
use std::env;

use crate::error::Result;
use crate::output::Format;

/// Show the fact log — bounded by default. An unbounded log's JSON output
/// once weighed more than the entire store (805KB, ~200k tokens) and
/// flooded any context window that asked "what's in the graph".
pub fn run(format: Format, limit: usize, since: Option<&str>) -> Result<()> {
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

    // Merge into one chronological stream.
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
            json: serde_json::json!({
                "type": "span",
                "id": span.id,
                "path": span.path,
                "line_start": span.line_start,
                "line_end": span.line_end,
                "ns": span.ns,
                "ts": span.ts,
            }),
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
            json: serde_json::json!({
                "type": "edge",
                "id": edge.id,
                "rel": edge.rel,
                "payload": p,
                "ns": edge.ns,
                "ts": edge.ts,
                "is_active": edge.is_active,
            }),
        });
    }

    entries.sort_by(|a, b| a.ts.cmp(&b.ts));

    // --since filter (date prefix compare on RFC3339 timestamps).
    if let Some(since_date) = since {
        entries.retain(|e| date_part(&e.ts) >= since_date);
    }

    let total = entries.len();
    // Bounded by default: keep the most recent `limit` (still printed in
    // chronological order). 0 = unlimited.
    let skipped = if limit > 0 && total > limit {
        let s = total - limit;
        entries.drain(..s);
        s
    } else {
        0
    };
    let shown = entries.len();

    match format {
        Format::Human => {
            if total == 0 {
                println!("No facts recorded yet.");
                return Ok(());
            }
            println!();
            println!(
                "Fact log ({} spans, {} edges total):",
                spans.len(),
                edges.len()
            );
            if skipped > 0 {
                println!(
                    "  ... {skipped} earlier facts hidden — `damask log --limit 0` for all, `--since YYYY-MM-DD` to filter"
                );
            }
            println!();
            for entry in &entries {
                println!("{}", entry.display);
            }
            println!();
        }
        Format::Json => {
            let facts: Vec<&serde_json::Value> = entries.iter().map(|e| &e.json).collect();
            let output = serde_json::json!({
                "context": {
                    "showing": { "limit": limit, "count": shown, "total": total },
                    "since": since,
                },
                "facts": facts,
            });
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
    }

    Ok(())
}

struct LogEntry {
    ts: String,
    display: String,
    json: serde_json::Value,
}

fn date_part(ts: &str) -> &str {
    ts.split('T').next().unwrap_or(ts)
}
