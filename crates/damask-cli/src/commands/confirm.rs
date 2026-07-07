//! Re-anchor a drifted span: "I checked — this is still true of the code
//! as it stands now."
//!
//! Since the index tracks code state, a span's *effective* location stays
//! current automatically — but its stored anchor (content hash + commit)
//! keeps failing to match, so the ⚠/↪ glyph never clears. `confirm`
//! re-emits the span fact with the SAME id and a fresh anchor at the
//! effective location (append-only re-anchoring: the index keeps the
//! latest fact per id, and every edge pointing at the span stays valid).
//! Given an edge id, it re-anchors the edge's span AND endorses the edge.
//!
//! This replaces the 3-write span+edge+supersedes ceremony that appeared
//! exactly once in 1,481 real-world facts.

use anyhow::{bail, Context};
use damask_core::{DamaskId, Fact, Span};
use damask_store::{update_index_with_mode, DamaskProject, FactWriter, IndexMode, IndexQuery};
use std::env;

use crate::error::Result;
use crate::output::Format;

use super::at::edge_target_span_id;
use super::helpers;

pub fn run(id: &str, format: Format) -> Result<()> {
    let cwd = env::current_dir().context("failed to get current directory")?;
    let project = DamaskProject::discover(&cwd)
        .map_err(|e| anyhow::anyhow!("{}", e))
        .context("no .damask/ found — run `damask init` first")?;

    // Accept unique id prefixes.
    let id = &helpers::resolve_id(&project, id)?;

    let db_path = project.damask_dir.join("index.db");
    let edges_dir = project.damask_dir.join("edges");
    let conn = update_index_with_mode(&db_path, &edges_dir, IndexMode::ViewsPreferred)
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    let q = IndexQuery::new(&conn);

    // Accept a span id directly, or an edge id (confirm its anchor + endorse it).
    let (span_row, edge_to_endorse) = if id.starts_with("e_") {
        let edge = q
            .edge_by_id(id)
            .map_err(|e| anyhow::anyhow!("{}", e))?
            .ok_or_else(|| anyhow::anyhow!("no edge with id {id}"))?;
        let span_id = edge_target_span_id(&edge)
            .ok_or_else(|| anyhow::anyhow!("edge {id} has no span anchor to confirm"))?
            .to_string();
        let span = q
            .span_by_id(&span_id)
            .map_err(|e| anyhow::anyhow!("{}", e))?
            .ok_or_else(|| anyhow::anyhow!("anchor span {span_id} not found"))?;
        (span, Some(edge))
    } else if id.starts_with("s_") {
        let span = q
            .span_by_id(id)
            .map_err(|e| anyhow::anyhow!("{}", e))?
            .ok_or_else(|| anyhow::anyhow!("no span with id {id}"))?;
        (span, None)
    } else {
        bail!("'{id}' is not a span (s_) or edge (e_) id");
    };

    // The effective location is what the resolution cascade last computed.
    let (Some(start), Some(end)) = (span_row.line_start, span_row.line_end) else {
        bail!("span {} has no line range to re-anchor", span_row.id);
    };
    let file_path = project.root.join(&span_row.path);
    if !file_path.exists() {
        bail!(
            "anchor file {} no longer exists — there is nothing to confirm.\n  \
             If the finding is resolved or obsolete, close its edges instead:\n  \
             damask at {} (find the edge ids), then damask close <id> --reason resolved",
            span_row.path,
            span_row.path
        );
    }

    let already_fresh = span_row.resolution.as_deref() == Some("exact")
        && span_row.recency.as_deref() == Some("unchanged");

    let mut refreshed = false;
    if !already_fresh {
        // Fresh anchor at the effective location: recomputed snippet +
        // content hash, current HEAD. Same id — append-only re-anchoring.
        let (snippet, content_hash) = helpers::extract_span_content(&file_path, start, end)?;
        let span_id = match DamaskId::parse(&span_row.id).map_err(|e| anyhow::anyhow!("{}", e))? {
            DamaskId::Span(s) => s,
            _ => bail!("{} is not a span id", span_row.id),
        };
        let refreshed_span = Span {
            id: span_id,
            path: span_row.path.clone(),
            lines: Some([start, end]),
            snippet,
            symbol: span_row.symbol.clone(),
            content_hash,
            commit: helpers::git_head_commit(&project.root),
            ns: span_row.ns.clone(),
            ts: chrono::Utc::now(),
            agent: helpers::ambient_agent(),
            session: helpers::ambient_session(),
        };
        FactWriter::append(
            &project.edges_file(&span_row.ns),
            &Fact::Span(refreshed_span),
        )
        .map_err(|e| anyhow::anyhow!("{}", e))?;
        refreshed = true;
    }

    // Confirming an edge is also an endorsement of its content.
    let mut endorsed = None;
    if let Some(edge) = &edge_to_endorse {
        let meta = helpers::build_edge(
            Some(DamaskId::parse(&edge.id).map_err(|e| anyhow::anyhow!("{}", e))?),
            None,
            "endorsed",
            serde_json::json!({"summary": "Confirmed against current code via damask confirm"}),
            &edge.ns,
        );
        endorsed = Some(meta.id.to_string());
        FactWriter::append(&project.edges_file(&edge.ns), &Fact::Edge(meta))
            .map_err(|e| anyhow::anyhow!("{}", e))?;
    }

    match format {
        Format::Human => {
            if refreshed {
                println!(
                    "Re-anchored {} at {}:{}-{} \u{2705} (was {})",
                    span_row.id,
                    span_row.path,
                    start,
                    end,
                    span_row.resolution.as_deref().unwrap_or("unknown"),
                );
            } else {
                println!(
                    "{} is already fresh \u{2705} ({}:{}-{})",
                    span_row.id, span_row.path, start, end
                );
            }
            if let Some(eid) = &endorsed {
                println!("Endorsed {} ({})", edge_to_endorse.as_ref().unwrap().id, eid);
            }
        }
        Format::Json => {
            println!(
                "{}",
                serde_json::json!({
                    "span": span_row.id,
                    "path": span_row.path,
                    "lines": [start, end],
                    "reanchored": refreshed,
                    "endorsed": endorsed,
                })
            );
        }
    }

    Ok(())
}
