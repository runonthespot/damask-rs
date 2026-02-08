use anyhow::{bail, Context};
use damask_core::{DamaskId, Edge, EdgeId, Fact};
use damask_store::{DamaskProject, FactWriter};
use std::env;

use crate::error::Result;
use crate::output::Format;

pub fn run(
    from: &str,
    to: &str,
    rel: &str,
    payload: Option<&str>,
    payload_file: Option<&str>,
    stdin: bool,
    ns_override: Option<&str>,
    format: Format,
) -> Result<()> {
    let cwd = env::current_dir().context("failed to get current directory")?;
    let project = DamaskProject::discover(&cwd)
        .map_err(|e| anyhow::anyhow!("{}", e))
        .context("no .damask/ found — run `damask init` first")?;

    let ns = resolve_ns(&project, ns_override)?;

    let from_id = parse_endpoint(from).context("invalid 'from' ID")?;
    let to_id = parse_endpoint(to).context("invalid 'to' ID")?;

    let payload_value = resolve_payload(payload, payload_file, stdin)?;

    let edge = Edge {
        id: EdgeId::new(),
        from: from_id,
        to: to_id,
        rel: rel.to_string(),
        payload: payload_value,
        ns: ns.clone(),
        ts: chrono::Utc::now(),
        agent: None,
        session: None,
    };

    let fact = Fact::Edge(edge.clone());
    let edges_file = project.edges_file(&ns);
    FactWriter::append(&edges_file, &fact).map_err(|e| anyhow::anyhow!("{}", e))?;

    match format {
        Format::Human => {
            println!("{}", crate::output::human::format_edge_created(&edge));
        }
        Format::Json => {
            crate::output::json::print_edge(&edge);
        }
    }

    Ok(())
}

/// Parse an endpoint string: "_" → None, otherwise parse as DamaskId.
fn parse_endpoint(s: &str) -> Result<Option<DamaskId>> {
    if s == "_" {
        Ok(None)
    } else {
        let id = DamaskId::parse(s)
            .map_err(|e| anyhow::anyhow!("{}", e))
            .context(format!("'{s}' is not a valid span or edge ID"))?;
        Ok(Some(id))
    }
}

/// Resolve the payload from inline JSON, a file, stdin, or default to empty object.
fn resolve_payload(inline: Option<&str>, file: Option<&str>, stdin: bool) -> Result<serde_json::Value> {
    let source_count = inline.is_some() as u8 + file.is_some() as u8 + stdin as u8;
    if source_count > 1 {
        bail!("cannot specify more than one of: inline payload, --file, --stdin");
    }

    if let Some(path) = file {
        let content = std::fs::read_to_string(path)
            .context(format!("failed to read payload file: {path}"))?;
        let value: serde_json::Value =
            serde_json::from_str(&content).context("payload file is not valid JSON")?;
        return Ok(value);
    }

    if stdin {
        let mut content = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut content)
            .context("failed to read from stdin")?;
        let value: serde_json::Value =
            serde_json::from_str(&content).context("stdin is not valid JSON")?;
        return Ok(value);
    }

    if let Some(json_str) = inline {
        let value: serde_json::Value =
            serde_json::from_str(json_str).context("payload is not valid JSON")?;
        return Ok(value);
    }

    Ok(serde_json::json!({}))
}

fn resolve_ns(project: &DamaskProject, ns_override: Option<&str>) -> Result<String> {
    if let Some(ns) = ns_override {
        return Ok(ns.to_string());
    }
    project
        .active_ns()
        .ok_or_else(|| anyhow::anyhow!("no active namespace — use `damask ns set <name>` or --ns"))
}
