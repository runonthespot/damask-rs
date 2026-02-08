use anyhow::Context;
use damask_core::PayloadEnvelope;
use damask_store::{update_index, DamaskProject, IndexQuery};
use std::env;

use crate::error::Result;
use crate::output::Format;

pub fn run(edge_id: &str, format: Format) -> Result<()> {
    if !edge_id.starts_with("e_") {
        anyhow::bail!("not a valid edge ID: {edge_id}");
    }

    let cwd = env::current_dir().context("failed to get current directory")?;
    let project = DamaskProject::discover(&cwd)
        .map_err(|e| anyhow::anyhow!("{}", e))
        .context("no .damask/ found — run `damask init` first")?;

    let db_path = project.damask_dir.join("index.db");
    let edges_dir = project.damask_dir.join("edges");
    let conn = update_index(&db_path, &edges_dir).map_err(|e| anyhow::anyhow!("{}", e))?;

    let q = IndexQuery::new(&conn);

    let edge = q
        .edge_by_id(edge_id)
        .map_err(|e| anyhow::anyhow!("{}", e))?
        .ok_or_else(|| anyhow::anyhow!("edge not found: {edge_id}"))?;

    let targeting = q
        .edges_targeting(edge_id)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let endorsements: Vec<_> = targeting.iter().filter(|e| e.rel == "endorsed").collect();
    let disputes: Vec<_> = targeting.iter().filter(|e| e.rel == "disputed").collect();
    let superseded_by: Vec<_> = targeting.iter().filter(|e| e.rel == "supersedes").collect();

    match format {
        Format::Human => {
            let payload: serde_json::Value =
                serde_json::from_str(&edge.payload).unwrap_or(serde_json::json!({}));
            let env = PayloadEnvelope::new(&payload);

            println!();
            println!("Edge: {} [{}]", edge.id, edge.rel);
            if let Some(summary) = env.summary() {
                println!("  {summary}");
            }
            let date = edge.ts.split('T').next().unwrap_or(&edge.ts);
            let agent = edge.agent.as_deref().unwrap_or("unknown");
            println!("  Created: {date} by {agent}");
            println!("  Namespace: {}", edge.ns);
            println!("  Active: {}", if edge.is_active { "yes" } else { "no" });
            println!();

            if !endorsements.is_empty() {
                println!("  Endorsements ({}):", endorsements.len());
                for e in &endorsements {
                    let p: serde_json::Value =
                        serde_json::from_str(&e.payload).unwrap_or(serde_json::json!({}));
                    let pe = PayloadEnvelope::new(&p);
                    let summary = pe.summary().unwrap_or("");
                    let d = e.ts.split('T').next().unwrap_or(&e.ts);
                    let a = e.agent.as_deref().unwrap_or("unknown");
                    println!("    \u{2713} {d} {a} {summary}");
                }
                println!();
            }

            if !disputes.is_empty() {
                println!("  Disputes ({}):", disputes.len());
                for e in &disputes {
                    let p: serde_json::Value =
                        serde_json::from_str(&e.payload).unwrap_or(serde_json::json!({}));
                    let pe = PayloadEnvelope::new(&p);
                    let summary = pe.summary().unwrap_or("");
                    let d = e.ts.split('T').next().unwrap_or(&e.ts);
                    let a = e.agent.as_deref().unwrap_or("unknown");
                    println!("    \u{2717} {d} {a} {summary}");
                }
                println!();
            }

            if !superseded_by.is_empty() {
                println!("  Superseded by:");
                for e in &superseded_by {
                    let from = e.from_id.as_deref().unwrap_or("?");
                    let d = e.ts.split('T').next().unwrap_or(&e.ts);
                    println!("    \u{2192} {from} ({d})");
                }
                println!();
            }

            if endorsements.is_empty() && disputes.is_empty() && superseded_by.is_empty() {
                println!("  No endorsements, disputes, or supersessions.");
            }
        }
        Format::Json => {
            let payload: serde_json::Value =
                serde_json::from_str(&edge.payload).unwrap_or(serde_json::json!({}));

            let endorsements_json: Vec<serde_json::Value> = endorsements
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "id": e.id,
                        "ts": e.ts,
                        "agent": e.agent,
                    })
                })
                .collect();

            let disputes_json: Vec<serde_json::Value> = disputes
                .iter()
                .map(|e| {
                    let p: serde_json::Value =
                        serde_json::from_str(&e.payload).unwrap_or(serde_json::json!({}));
                    serde_json::json!({
                        "id": e.id,
                        "ts": e.ts,
                        "agent": e.agent,
                        "payload": p,
                    })
                })
                .collect();

            let superseded_json: Vec<serde_json::Value> = superseded_by
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "from": e.from_id,
                        "ts": e.ts,
                    })
                })
                .collect();

            let output = serde_json::json!({
                "id": edge.id,
                "rel": edge.rel,
                "payload": payload,
                "ns": edge.ns,
                "ts": edge.ts,
                "agent": edge.agent,
                "is_active": edge.is_active,
                "endorsements": endorsements_json,
                "disputes": disputes_json,
                "superseded_by": superseded_json,
            });
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
    }

    Ok(())
}
