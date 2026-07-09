use anyhow::Context;
use damask_core::PayloadEnvelope;
use damask_store::{update_index, DamaskProject, IndexQuery};
use std::env;

use crate::error::Result;
use crate::output::Format;

pub fn run(id: &str, format: Format) -> Result<()> {
    if !id.starts_with("s_") && !id.starts_with("e_") {
        anyhow::bail!("not a valid span or edge ID: {id}");
    }

    let cwd = env::current_dir().context("failed to get current directory")?;
    let project = DamaskProject::discover(&cwd)
        .map_err(|e| anyhow::anyhow!("{}", e))
        .context("no .damask/ found — run `damask init` first")?;

    let db_path = project.damask_dir.join("index.db");
    let edges_dir = project.damask_dir.join("edges");
    let conn = update_index(&db_path, &edges_dir).map_err(|e| anyhow::anyhow!("{}", e))?;

    let q = IndexQuery::new(&conn);

    match format {
        Format::Human => print_blame_human(&q, id),
        Format::Json => print_blame_json(&q, id),
    }
}

fn print_blame_human(q: &IndexQuery, id: &str) -> Result<()> {
    println!();
    println!("Blame: {id}");
    println!();

    if id.starts_with("s_") {
        if let Some(span) = q.span_by_id(id).map_err(|e| anyhow::anyhow!("{}", e))? {
            let lines = match (span.line_start, span.line_end) {
                (Some(s), Some(e)) => format!(":{}-{}", s, e),
                _ => String::new(),
            };
            let date = span.ts.split('T').next().unwrap_or(&span.ts);
            println!("  Span: {}{} [{}]", span.path, lines, date);

            let edges = q.edges_for_span(id).map_err(|e| anyhow::anyhow!("{}", e))?;
            if edges.is_empty() {
                println!("  No edges reference this span.");
            } else {
                println!("  Edges ({}):", edges.len());
                for edge in &edges {
                    let p: serde_json::Value =
                        serde_json::from_str(&edge.payload).unwrap_or(serde_json::json!({}));
                    let env = PayloadEnvelope::new(&p);
                    let summary = env.summary().unwrap_or("(no summary)");
                    let date = edge.ts.split('T').next().unwrap_or(&edge.ts);
                    let agent = edge.agent.as_deref().unwrap_or("unknown");
                    println!(
                        "    {} [{}] {} {} — {}",
                        edge.id, edge.rel, date, agent, summary
                    );
                }
            }
        } else {
            println!("  Span not found: {id}");
        }
    } else if let Some(edge) = q.edge_by_id(id).map_err(|e| anyhow::anyhow!("{}", e))? {
        let p: serde_json::Value =
            serde_json::from_str(&edge.payload).unwrap_or(serde_json::json!({}));
        let env = PayloadEnvelope::new(&p);
        let summary = env.summary().unwrap_or("(no summary)");
        let date = edge.ts.split('T').next().unwrap_or(&edge.ts);
        let agent = edge.agent.as_deref().unwrap_or("unknown");
        let active = if edge.is_active { "" } else { " (superseded)" };

        println!(
            "  {} [{}] {} {} — {}{}",
            edge.id, edge.rel, date, agent, summary, active
        );

        let targeting = q
            .edges_targeting(id)
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        let superseded_by: Vec<_> = targeting.iter().filter(|e| e.rel == "supersedes").collect();

        if !superseded_by.is_empty() {
            println!();
            println!("  Supersession chain:");
            for sup in &superseded_by {
                if let Some(ref from_id) = sup.from_id {
                    if let Some(new_edge) = q
                        .edge_by_id(from_id)
                        .map_err(|e| anyhow::anyhow!("{}", e))?
                    {
                        let np: serde_json::Value = serde_json::from_str(&new_edge.payload)
                            .unwrap_or(serde_json::json!({}));
                        let ne = PayloadEnvelope::new(&np);
                        let ns = ne.summary().unwrap_or("(no summary)");
                        let nd = new_edge.ts.split('T').next().unwrap_or(&new_edge.ts);
                        let na = new_edge.agent.as_deref().unwrap_or("unknown");
                        println!(
                            "    \u{2192} {} [{}] {} {} — {}",
                            new_edge.id, new_edge.rel, nd, na, ns
                        );
                    }
                }
            }
        }
    } else {
        println!("  Edge not found: {id}");
    }

    println!();
    Ok(())
}

fn print_blame_json(q: &IndexQuery, id: &str) -> Result<()> {
    if id.starts_with("s_") {
        let span = q.span_by_id(id).map_err(|e| anyhow::anyhow!("{}", e))?;
        let edges = q.edges_for_span(id).map_err(|e| anyhow::anyhow!("{}", e))?;

        let edges_json: Vec<serde_json::Value> = edges
            .iter()
            .map(|e| {
                let payload: serde_json::Value =
                    serde_json::from_str(&e.payload).unwrap_or(serde_json::json!({}));
                serde_json::json!({
                    "id": e.id,
                    "rel": e.rel,
                    "payload": payload,
                    "ts": e.ts,
                    "agent": e.agent,
                })
            })
            .collect();

        let output = serde_json::json!({
            "id": id,
            "type": "span",
            "span": span.map(|s| serde_json::json!({
                "path": s.path,
                "line_start": s.line_start,
                "line_end": s.line_end,
                "ts": s.ts,
            })),
            "edges": edges_json,
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        let edge = q.edge_by_id(id).map_err(|e| anyhow::anyhow!("{}", e))?;
        let targeting = q
            .edges_targeting(id)
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        let superseded_by: Vec<serde_json::Value> = targeting
            .iter()
            .filter(|e| e.rel == "supersedes")
            .filter_map(|e| e.from_id.as_ref())
            .map(|from_id| serde_json::json!(from_id))
            .collect();

        let output = serde_json::json!({
            "id": id,
            "type": "edge",
            "edge": edge.map(|e| {
                let payload: serde_json::Value =
                    serde_json::from_str(&e.payload).unwrap_or(serde_json::json!({}));
                serde_json::json!({
                    "rel": e.rel,
                    "payload": payload,
                    "ts": e.ts,
                    "agent": e.agent,
                    "is_active": e.is_active,
                })
            }),
            "superseded_by": superseded_by,
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    }

    Ok(())
}
