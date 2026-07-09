use anyhow::Context;
use damask_core::{DamaskId, Edge, EdgeId, Fact};
use damask_store::{DamaskProject, FactWriter};
use std::env;
use std::io::BufRead;

use crate::error::Result;

/// Map a reason template name to a JSON payload.
fn reason_to_payload(reason: &str) -> serde_json::Value {
    match reason {
        "resolved" => {
            serde_json::json!({"summary": "Closed — resolved, the underlying issue has been addressed"})
        }
        "outdated" => {
            serde_json::json!({"summary": "Closed — outdated, the context has changed significantly"})
        }
        "incorrect" => {
            serde_json::json!({"summary": "Closed — incorrect, investigation determined this is not accurate"})
        }
        "duplicate" => {
            serde_json::json!({"summary": "Closed — duplicate, this finding is covered by another edge"})
        }
        "accepted" => {
            serde_json::json!({"summary": "Closed — accepted, acknowledged but not changing"})
        }
        _ => serde_json::json!({"summary": format!("Closed: {reason}")}),
    }
}

pub fn run(
    edge_id: Option<&str>,
    payload: Option<&str>,
    reason: Option<&str>,
    batch: bool,
    ns_override: Option<&str>,
) -> Result<()> {
    let payload_value = if let Some(reason) = reason {
        reason_to_payload(reason)
    } else if let Some(p) = payload {
        serde_json::from_str(p).context("payload is not valid JSON")?
    } else {
        serde_json::json!({"summary": "Closed"})
    };

    let cwd = env::current_dir().context("failed to get current directory")?;
    let project = DamaskProject::discover(&cwd)
        .map_err(|e| anyhow::anyhow!("{}", e))
        .context("no .damask/ found — run `damask init` first")?;

    let ns = ns_override
        .map(|s| s.to_string())
        .or_else(|| project.active_ns())
        .ok_or_else(|| {
            anyhow::anyhow!("no active namespace — use `damask ns set <name>` or --ns")
        })?;

    if batch {
        let stdin = std::io::stdin();
        let mut edge_ids = Vec::new();
        for line in stdin.lock().lines() {
            let line = line.context("failed to read from stdin")?;
            let trimmed = line.trim().to_string();
            if trimmed.is_empty() {
                continue;
            }
            if !trimmed.starts_with("e_") {
                anyhow::bail!("can only close edges (expected e_ prefix): {trimmed}");
            }
            DamaskId::parse(&trimmed)
                .map_err(|e| anyhow::anyhow!("{}", e))
                .context(format!("'{trimmed}' is not a valid edge ID"))?;
            edge_ids.push(trimmed);
        }

        if edge_ids.is_empty() {
            anyhow::bail!("no edge IDs provided on stdin");
        }

        let mut facts = Vec::new();
        for eid in &edge_ids {
            let target = DamaskId::parse(eid).map_err(|e| anyhow::anyhow!("{}", e))?;
            let edge = Edge {
                id: EdgeId::new(),
                from: Some(target),
                to: None,
                rel: "closed".to_string(),
                payload: payload_value.clone(),
                ns: ns.clone(),
                ts: chrono::Utc::now(),
                agent: super::helpers::ambient_agent(),
                session: super::helpers::ambient_session(),
            };
            facts.push(Fact::Edge(edge));
        }

        let edges_file = project.edges_file(&ns);
        FactWriter::append_all(&edges_file, &facts).map_err(|e| anyhow::anyhow!("{}", e))?;

        println!("Closed {} edges", edge_ids.len());
    } else {
        let edge_id =
            edge_id.ok_or_else(|| anyhow::anyhow!("edge_id is required (or use --batch)"))?;

        let edge_id = &super::helpers::resolve_id(&project, edge_id)?;
        if !edge_id.starts_with("e_") {
            anyhow::bail!("can only close edges (expected e_ prefix): {edge_id}");
        }

        let target = DamaskId::parse(edge_id)
            .map_err(|e| anyhow::anyhow!("{}", e))
            .context(format!("'{edge_id}' is not a valid edge ID"))?;

        super::helpers::print_signal_context(&project, edge_id, "closing");

        let edge = Edge {
            id: EdgeId::new(),
            from: Some(target),
            to: None,
            rel: "closed".to_string(),
            payload: payload_value,
            ns: ns.clone(),
            ts: chrono::Utc::now(),
            agent: super::helpers::ambient_agent(),
            session: super::helpers::ambient_session(),
        };

        let fact = Fact::Edge(edge.clone());
        let edges_file = project.edges_file(&ns);
        FactWriter::append(&edges_file, &fact).map_err(|e| anyhow::anyhow!("{}", e))?;

        println!("Closed {} ({})", edge_id, edge.id);
    }

    Ok(())
}
