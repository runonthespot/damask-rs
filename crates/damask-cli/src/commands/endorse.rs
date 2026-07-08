use anyhow::Context;
use damask_core::{DamaskId, Edge, EdgeId, Fact};
use damask_store::{DamaskProject, FactWriter};
use std::env;
use std::io::BufRead;

use crate::error::Result;

pub fn run(
    edge_id: Option<&str>,
    payload: Option<&str>,
    batch: bool,
    ns_override: Option<&str>,
) -> Result<()> {
    let payload_value = if let Some(json_str) = payload {
        serde_json::from_str(json_str).context("payload is not valid JSON")?
    } else {
        serde_json::json!({})
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
                anyhow::bail!("can only endorse edges (expected e_ prefix): {trimmed}");
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
                rel: "endorsed".to_string(),
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

        println!("Endorsed {} edges", edge_ids.len());
    } else {
        let edge_id =
            edge_id.ok_or_else(|| anyhow::anyhow!("edge_id is required (or use --batch)"))?;

        let edge_id = &super::helpers::resolve_id(&project, edge_id)?;
        if !edge_id.starts_with("e_") {
            anyhow::bail!("can only endorse edges (expected e_ prefix): {edge_id}");
        }

        let target = DamaskId::parse(edge_id)
            .map_err(|e| anyhow::anyhow!("{}", e))
            .context(format!("'{edge_id}' is not a valid edge ID"))?;

        super::helpers::print_signal_context(&project, edge_id, "endorsing");

        let edge = Edge {
            id: EdgeId::new(),
            from: Some(target),
            to: None,
            rel: "endorsed".to_string(),
            payload: payload_value,
            ns: ns.clone(),
            ts: chrono::Utc::now(),
            agent: super::helpers::ambient_agent(),
            session: super::helpers::ambient_session(),
        };

        let fact = Fact::Edge(edge.clone());
        let edges_file = project.edges_file(&ns);
        FactWriter::append(&edges_file, &fact).map_err(|e| anyhow::anyhow!("{}", e))?;

        println!("Endorsed {} ({})", edge_id, edge.id);
    }

    Ok(())
}
