use anyhow::Context;
use damask_core::{DamaskId, Edge, EdgeId, Fact};
use damask_store::{DamaskProject, FactWriter};
use std::env;

use crate::error::Result;

pub fn run(edge_id: &str, payload: &str) -> Result<()> {
    // Validate the target edge ID
    let target = DamaskId::parse(edge_id)
        .map_err(|e| anyhow::anyhow!("{}", e))
        .context(format!("'{edge_id}' is not a valid edge ID"))?;

    if !edge_id.starts_with("e_") {
        anyhow::bail!("can only dispute edges (expected e_ prefix): {edge_id}");
    }

    // Payload is required for disputes
    let payload_value: serde_json::Value =
        serde_json::from_str(payload).context("payload is not valid JSON")?;

    let cwd = env::current_dir().context("failed to get current directory")?;
    let project = DamaskProject::discover(&cwd)
        .map_err(|e| anyhow::anyhow!("{}", e))
        .context("no .damask/ found — run `damask init` first")?;

    let ns = project.active_ns().ok_or_else(|| {
        anyhow::anyhow!("no active namespace — use `damask ns set <name>` or --ns")
    })?;

    let edge = Edge {
        id: EdgeId::new(),
        from: None,
        to: Some(target),
        rel: "disputed".to_string(),
        payload: payload_value,
        ns: ns.clone(),
        ts: chrono::Utc::now(),
        agent: None,
        session: None,
    };

    let fact = Fact::Edge(edge.clone());
    let edges_file = project.edges_file(&ns);
    FactWriter::append(&edges_file, &fact).map_err(|e| anyhow::anyhow!("{}", e))?;

    println!("Disputed {} ({})", edge_id, edge.id);

    Ok(())
}
