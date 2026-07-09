//! Garden the graph: find rot, propose closes, never act alone.
//!
//! Real-world stores showed the trust loop running backwards — the same
//! fixed finding re-verified and re-disputed eight times because nothing
//! low-friction turned "this is dead" into a close. `triage` groups open
//! edges by git-verified rot cause and emits ready-to-run bulk closes.
//! The sacred invariant stands: nothing closes without an explicit flag,
//! and `--close-deleted` refuses if any matched file still exists.

use anyhow::Context;
use damask_core::{DamaskId, Edge, EdgeId, Fact};
use damask_store::{update_index_with_mode, DamaskProject, FactWriter, IndexMode, IndexQuery};
use std::collections::HashMap;
use std::env;

use crate::error::Result;
use crate::output::Format;

use super::at::edge_target_span_id;

/// Disputes at or above this, with zero endorsements, mark an edge refuted.
const REFUTED_MIN_DISPUTES: u32 = 3;

struct RotEdge {
    edge_id: String,
    ns: String,
    path: String,
}

pub fn run(
    close_deleted: Option<&str>,
    close_refuted: bool,
    close_ruled_out: bool,
    format: Format,
) -> Result<()> {
    let cwd = env::current_dir().context("failed to get current directory")?;
    let project = DamaskProject::discover(&cwd)
        .map_err(|e| anyhow::anyhow!("{}", e))
        .context("no .damask/ found — run `damask init` first")?;

    let db_path = project.damask_dir.join("index.db");
    let edges_dir = project.damask_dir.join("edges");
    let conn = update_index_with_mode(&db_path, &edges_dir, IndexMode::ViewsPreferred)
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    let q = IndexQuery::new(&conn);

    let open_edges = q
        .all_active_open_edges()
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    let spans: HashMap<String, damask_store::index::query::SpanRow> = q
        .all_spans_chronological()
        .map_err(|e| anyhow::anyhow!("{}", e))?
        .into_iter()
        .map(|s| (s.id.clone(), s))
        .collect();
    let endorse_counts = q
        .endorsement_counts()
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    let dispute_counts = q.dispute_counts().map_err(|e| anyhow::anyhow!("{}", e))?;

    // Rot cause 1: anchor file no longer exists (resolution=missing AND
    // verified absent on disk right now — the index could be seconds old,
    // but a file that came back must never be bulk-closed).
    let mut deleted: Vec<RotEdge> = Vec::new();
    // Rot cause 2: refuted — repeatedly disputed, never endorsed.
    let mut refuted: Vec<RotEdge> = Vec::new();
    // Rot cause 3: status says ruled_out (schema) but the edge is still open.
    let mut ruled_out: Vec<RotEdge> = Vec::new();

    for edge in &open_edges {
        let anchor = edge_target_span_id(edge).and_then(|id| spans.get(id));
        if let Some(span) = anchor {
            if span.resolution.as_deref() == Some("missing")
                && !project.root.join(&span.path).exists()
            {
                deleted.push(RotEdge {
                    edge_id: edge.id.clone(),
                    ns: edge.ns.clone(),
                    path: span.path.clone(),
                });
            }
        }
        let payload: serde_json::Value =
            serde_json::from_str(&edge.payload).unwrap_or(serde_json::json!({}));
        if payload.get("status").and_then(|v| v.as_str()) == Some("ruled_out") {
            ruled_out.push(RotEdge {
                edge_id: edge.id.clone(),
                ns: edge.ns.clone(),
                path: anchor.map(|s| s.path.clone()).unwrap_or_default(),
            });
        }
        let disputes = dispute_counts.get(&edge.id).copied().unwrap_or(0);
        let endorsements = endorse_counts.get(&edge.id).copied().unwrap_or(0);
        if disputes >= REFUTED_MIN_DISPUTES && endorsements == 0 {
            refuted.push(RotEdge {
                edge_id: edge.id.clone(),
                ns: edge.ns.clone(),
                path: anchor.map(|s| s.path.clone()).unwrap_or_default(),
            });
        }
    }

    // Action modes.
    if let Some(prefix) = close_deleted {
        let matched: Vec<&RotEdge> = deleted
            .iter()
            .filter(|r| r.path.starts_with(prefix))
            .collect();
        if matched.is_empty() {
            println!("Nothing to close: no open edges anchored to missing files under '{prefix}'.");
            return Ok(());
        }
        // Safety: refuse if ANY file under the prefix reappeared.
        for r in &matched {
            if project.root.join(&r.path).exists() {
                anyhow::bail!(
                    "refusing --close-deleted: {} exists on disk — re-run `damask triage` for a fresh report",
                    r.path
                );
            }
        }
        let n = write_closes(&project, &matched, |r| {
            format!("Closed by triage — anchor file {} no longer exists", r.path)
        })?;
        println!("Closed {n} edges anchored to deleted files under '{prefix}'.");
        return Ok(());
    }

    if close_ruled_out {
        let matched: Vec<&RotEdge> = ruled_out.iter().collect();
        if matched.is_empty() {
            println!("Nothing to close: no open edges with status ruled_out.");
            return Ok(());
        }
        let n = write_closes(&project, &matched, |_| {
            "Closed by triage — status ruled_out: investigated and dismissed".to_string()
        })?;
        println!("Closed {n} ruled-out edges.");
        return Ok(());
    }

    if close_refuted {
        let matched: Vec<&RotEdge> = refuted.iter().collect();
        if matched.is_empty() {
            println!(
                "Nothing to close: no open edges with >= {REFUTED_MIN_DISPUTES} disputes and zero endorsements."
            );
            return Ok(());
        }
        let n = write_closes(&project, &matched, |_| {
            format!(
                "Closed by triage — refuted: >= {REFUTED_MIN_DISPUTES} disputes, zero endorsements"
            )
        })?;
        println!("Closed {n} refuted edges.");
        return Ok(());
    }

    // Report mode: group deleted anchors by directory, propose commands.
    let mut by_dir: HashMap<String, Vec<&RotEdge>> = HashMap::new();
    for r in &deleted {
        let dir = match r.path.rfind('/') {
            Some(i) => r.path[..=i].to_string(),
            None => String::new(),
        };
        by_dir.entry(dir).or_default().push(r);
    }
    let mut dirs: Vec<(String, usize)> = by_dir.iter().map(|(d, v)| (d.clone(), v.len())).collect();
    dirs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    match format {
        Format::Human => {
            if deleted.is_empty() && refuted.is_empty() && ruled_out.is_empty() {
                println!(
                    "No rot found: every open edge anchors to existing code and none are refuted."
                );
                return Ok(());
            }
            println!();
            println!("Damask triage — proposed closes (nothing is closed without a flag)");
            if !deleted.is_empty() {
                println!();
                println!("  Anchored to deleted files ({} edges):", deleted.len());
                for (dir, count) in dirs.iter().take(10) {
                    let label = if dir.is_empty() { "<repo root>" } else { dir };
                    println!("    {label}  ({count} edges)");
                    println!(
                        "      -> damask triage --close-deleted {}",
                        if dir.is_empty() {
                            "\"\"".to_string()
                        } else {
                            dir.clone()
                        }
                    );
                }
                if dirs.len() > 10 {
                    println!("    ... and {} more directories", dirs.len() - 10);
                }
            }
            if !ruled_out.is_empty() {
                println!();
                println!(
                    "  Ruled out but still open ({} edges — they rank near zero but linger):",
                    ruled_out.len()
                );
                println!("      -> damask triage --close-ruled-out");
            }
            if !refuted.is_empty() {
                println!();
                println!(
                    "  Refuted ({} edges with >= {REFUTED_MIN_DISPUTES} disputes, zero endorsements):",
                    refuted.len()
                );
                for r in refuted.iter().take(5) {
                    println!("    {}  [{}]", r.edge_id, r.ns);
                }
                if refuted.len() > 5 {
                    println!("    ... and {} more", refuted.len() - 5);
                }
                println!("      -> damask triage --close-refuted");
            }
            println!();
        }
        Format::Json => {
            let deleted_json: Vec<serde_json::Value> = dirs
                .iter()
                .map(|(dir, count)| serde_json::json!({"dir": dir, "edges": count}))
                .collect();
            let refuted_json: Vec<serde_json::Value> = refuted
                .iter()
                .map(|r| serde_json::json!({"id": r.edge_id, "ns": r.ns, "path": r.path}))
                .collect();
            println!(
                "{}",
                serde_json::json!({
                    "deleted_anchor_edges": deleted.len(),
                    "deleted_by_dir": deleted_json,
                    "refuted": refuted_json,
                    "ruled_out_open": ruled_out.len(),
                    "commands": {
                        "close_deleted": "damask triage --close-deleted <dir/>",
                        "close_refuted": "damask triage --close-refuted",
                    },
                })
            );
        }
    }

    Ok(())
}

/// Write `closed` meta-edges into each target edge's OWN namespace —
/// hand-rolled bulk closes into the wrong namespace were a real failure
/// mode in the wild.
fn write_closes(
    project: &DamaskProject,
    targets: &[&RotEdge],
    summary: impl Fn(&RotEdge) -> String,
) -> Result<usize> {
    let mut by_ns: HashMap<String, Vec<Fact>> = HashMap::new();
    for r in targets {
        let target = DamaskId::parse(&r.edge_id).map_err(|e| anyhow::anyhow!("{}", e))?;
        let edge = Edge {
            id: EdgeId::new(),
            from: Some(target),
            to: None,
            rel: "closed".to_string(),
            payload: serde_json::json!({"summary": summary(r), "tags": ["triage"]}),
            ns: r.ns.clone(),
            ts: chrono::Utc::now(),
            agent: super::helpers::ambient_agent(),
            session: super::helpers::ambient_session(),
        };
        by_ns
            .entry(r.ns.clone())
            .or_default()
            .push(Fact::Edge(edge));
    }
    let mut n = 0;
    for (ns, facts) in by_ns {
        n += facts.len();
        FactWriter::append_all(&project.edges_file(&ns), &facts)
            .map_err(|e| anyhow::anyhow!("{}", e))?;
    }
    Ok(n)
}
