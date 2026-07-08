use anyhow::Context;
use damask_core::{DamaskId, Edge, EdgeId, Fact};
use damask_store::{DamaskProject, FactWriter};
use std::env;
use std::io::BufRead;

use crate::error::Result;

/// Map a reason template name to a JSON payload.
fn reason_to_payload(reason: &str) -> serde_json::Value {
    match reason {
        "resolved" => serde_json::json!({"summary": "Resolved — the underlying issue has been addressed"}),
        "fixed" => serde_json::json!({"summary": "Fixed — the underlying issue has been fixed"}),
        "stale" => serde_json::json!({"summary": "Stale — the code this references has since changed"}),
        "outdated" => serde_json::json!({"summary": "Outdated — the context this edge references has changed significantly"}),
        "incorrect" => serde_json::json!({"summary": "Incorrect — investigation determined this is not accurate"}),
        "duplicate" => serde_json::json!({"summary": "Duplicate — this finding is covered by another edge"}),
        _ => serde_json::json!({"summary": format!("Disputed: {reason}")}),
    }
}

/// A dispute that actually means "this is done" should be a close: closes
/// disappear from at/where/briefing, disputes barely rank-penalize. In
/// real usage ~150 of 192 disputes started "Fixed:" — agents grabbed the
/// only verb they could see. Detect that intent and teach the right verb.
fn reads_like_resolution(reason: Option<&str>, payload: &serde_json::Value) -> bool {
    if matches!(reason, Some("resolved") | Some("fixed") | Some("stale")) {
        return true;
    }
    payload
        .get("summary")
        .and_then(|s| s.as_str())
        .map(|s| {
            let l = s.trim().to_ascii_lowercase();
            l.starts_with("fixed") || l.starts_with("resolved") || l.starts_with("done")
        })
        .unwrap_or(false)
}

pub fn run(
    edge_id: Option<&str>,
    payload: Option<&str>,
    reason: Option<&str>,
    batch: bool,
    ns_override: Option<&str>,
) -> Result<()> {
    // Resolve the payload from --reason template or inline JSON
    let payload_value = if let Some(reason) = reason {
        reason_to_payload(reason)
    } else if let Some(p) = payload {
        serde_json::from_str(p).context("payload is not valid JSON")?
    } else if !batch {
        anyhow::bail!("dispute requires a payload or --reason template");
    } else {
        // In batch mode without --reason, payload is required
        anyhow::bail!("batch dispute requires --reason template or inline payload");
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
        // Read edge IDs from stdin, one per line
        let stdin = std::io::stdin();
        let mut edge_ids = Vec::new();
        for line in stdin.lock().lines() {
            let line = line.context("failed to read from stdin")?;
            let trimmed = line.trim().to_string();
            if trimmed.is_empty() {
                continue;
            }
            if !trimmed.starts_with("e_") {
                anyhow::bail!("can only dispute edges (expected e_ prefix): {trimmed}");
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
            let target = DamaskId::parse(eid)
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            let edge = Edge {
                id: EdgeId::new(),
                from: Some(target),
                to: None,
                rel: "disputed".to_string(),
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

        println!("Disputed {} edges", edge_ids.len());
        if reads_like_resolution(reason, &payload_value) {
            println!(
                "note: these read as RESOLVED. Disputes only weaken ranking; closes actually \
                 disappear from at/where/briefing. If the findings are done, close them:\n  \
                 <ids> | damask close --batch --reason resolved"
            );
        }
    } else {
        // Single edge dispute
        let edge_id = edge_id.ok_or_else(|| anyhow::anyhow!("edge_id is required (or use --batch)"))?;

        let edge_id = &super::helpers::resolve_id(&project, edge_id)?;
        if !edge_id.starts_with("e_") {
            anyhow::bail!("can only dispute edges (expected e_ prefix): {edge_id}");
        }

        let target = DamaskId::parse(edge_id)
            .map_err(|e| anyhow::anyhow!("{}", e))
            .context(format!("'{edge_id}' is not a valid edge ID"))?;

        super::helpers::print_signal_context(&project, edge_id, "disputing");

        let edge = Edge {
            id: EdgeId::new(),
            from: Some(target),
            to: None,
            rel: "disputed".to_string(),
            payload: payload_value,
            ns: ns.clone(),
            ts: chrono::Utc::now(),
            agent: super::helpers::ambient_agent(),
            session: super::helpers::ambient_session(),
        };

        let fact = Fact::Edge(edge.clone());
        let edges_file = project.edges_file(&ns);
        FactWriter::append(&edges_file, &fact).map_err(|e| anyhow::anyhow!("{}", e))?;

        println!("Disputed {} ({})", edge_id, edge.id);
        if reads_like_resolution(reason, &edge.payload) {
            println!(
                "note: this reads as RESOLVED. Disputes only weaken ranking; closes actually \
                 disappear from at/where/briefing. If the finding is done, close it:\n  \
                 damask close {edge_id} --reason resolved"
            );
        }
    }

    Ok(())
}
