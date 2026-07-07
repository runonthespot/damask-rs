//! Tag an existing edge — append-only, same-id re-emission.
//!
//! Follows `confirm`'s precedent: the fact is re-emitted with the SAME id
//! and its tags unioned; the index keeps the latest fact per id, every
//! reference (endorsements, disputes, from/to) stays valid, and the log
//! remains a faithful append-only history of what changed when. Semantic
//! payload changes still require supersede — tags are metadata, not
//! meaning.

use anyhow::{bail, Context};
use damask_core::Fact;
use damask_store::{
    update_index_with_mode, DamaskProject, FactReader, FactWriter, IndexMode, IndexQuery,
};
use std::env;

use crate::error::Result;
use crate::output::Format;

use super::helpers;

pub fn run(edge_id: &str, new_tags: &[String], format: Format) -> Result<()> {
    let cwd = env::current_dir().context("failed to get current directory")?;
    let project = DamaskProject::discover(&cwd)
        .map_err(|e| anyhow::anyhow!("{}", e))
        .context("no .damask/ found — run `damask init` first")?;

    let edge_id = helpers::resolve_id(&project, edge_id)?;
    if !edge_id.starts_with("e_") {
        bail!("can only tag edges (expected e_ prefix): {edge_id}");
    }

    // Locate the edge's namespace via the index, then re-read its latest
    // fact from the log so nothing (agent, session, endpoints) is lost.
    let db_path = project.damask_dir.join("index.db");
    let edges_dir = project.damask_dir.join("edges");
    let conn = update_index_with_mode(&db_path, &edges_dir, IndexMode::ViewsPreferred)
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    let q = IndexQuery::new(&conn);
    let row = q
        .edge_by_id(&edge_id)
        .map_err(|e| anyhow::anyhow!("{}", e))?
        .ok_or_else(|| anyhow::anyhow!("no edge with id {edge_id}"))?;

    let log_file = project.edges_file(&row.ns);
    let mut reader = FactReader::open(&log_file).map_err(|e| anyhow::anyhow!("{}", e))?;
    let facts = reader.read_all().map_err(|e| anyhow::anyhow!("{}", e))?;
    let mut edge = facts
        .iter()
        .rev()
        .find_map(|f| match f {
            Fact::Edge(e) if e.id.as_str() == edge_id => Some(e.clone()),
            _ => None,
        })
        .ok_or_else(|| {
            anyhow::anyhow!("edge {edge_id} is indexed but not found in {} log", row.ns)
        })?;

    let Some(obj) = edge.payload.as_object_mut() else {
        bail!("edge {edge_id} has a non-object payload; cannot tag");
    };
    let mut tags: Vec<String> = obj
        .get("tags")
        .and_then(|t| t.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let mut added = Vec::new();
    for t in new_tags {
        let t = t.trim_start_matches('#').trim().to_string();
        if !t.is_empty() && !tags.contains(&t) {
            tags.push(t.clone());
            added.push(t);
        }
    }
    if added.is_empty() {
        println!("No new tags — {} already has: {}", edge_id, tags.join(", "));
        return Ok(());
    }
    obj.insert("tags".to_string(), serde_json::json!(tags));

    FactWriter::append(&log_file, &Fact::Edge(edge)).map_err(|e| anyhow::anyhow!("{}", e))?;

    match format {
        Format::Human => println!(
            "Tagged {} {} (now: {})",
            edge_id,
            added.iter().map(|t| format!("#{t}")).collect::<Vec<_>>().join(" "),
            tags.iter().map(|t| format!("#{t}")).collect::<Vec<_>>().join(" "),
        ),
        Format::Json => println!(
            "{}",
            serde_json::json!({"id": edge_id, "added": added, "tags": tags})
        ),
    }
    Ok(())
}
