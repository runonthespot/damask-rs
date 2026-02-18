use anyhow::Context;
use damask_core::{DamaskId, Edge, EdgeId, Fact};
use damask_store::{DamaskProject, FactWriter};
use std::env;

use crate::error::Result;

pub fn run(edge_id: &str, payload: Option<&str>, ns_override: Option<&str>) -> Result<()> {
    // Validate the target edge ID
    let target = DamaskId::parse(edge_id)
        .map_err(|e| anyhow::anyhow!("{}", e))
        .context(format!("'{edge_id}' is not a valid edge ID"))?;

    if !edge_id.starts_with("e_") {
        anyhow::bail!("can only endorse edges (expected e_ prefix): {edge_id}");
    }

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

    let payload_value = if let Some(json_str) = payload {
        serde_json::from_str(json_str).context("payload is not valid JSON")?
    } else {
        serde_json::json!({})
    };

    let edge = Edge {
        id: EdgeId::new(),
        from: Some(target),
        to: None,
        rel: "endorsed".to_string(),
        payload: payload_value,
        ns: ns.clone(),
        ts: chrono::Utc::now(),
        agent: None,
        session: None,
    };

    let fact = Fact::Edge(edge.clone());
    let edges_file = project.edges_file(&ns);
    FactWriter::append(&edges_file, &fact).map_err(|e| anyhow::anyhow!("{}", e))?;

    println!("Endorsed {} ({})", edge_id, edge.id);

    Ok(())
}
