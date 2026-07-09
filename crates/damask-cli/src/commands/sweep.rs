//! Bulk anchor maintenance: find every drifted (🟠) and gone (🔴) anchor
//! carrying open knowledge, report them, and — on request — re-anchor the
//! drifted ones mechanically (`confirm`'s append-only same-id re-emission,
//! applied wholesale). Gone anchors are triage's jurisdiction; sweep names
//! them and prints the command rather than closing anything itself.

use anyhow::Context;
use damask_core::Fact;
use damask_store::{update_index_with_mode, DamaskProject, FactWriter, IndexMode, IndexQuery};
use std::collections::HashMap;
use std::env;

use crate::error::Result;
use crate::output::Format;

use super::helpers;

pub fn run(reanchor: bool, format: Format) -> Result<()> {
    let cwd = env::current_dir().context("failed to get current directory")?;
    let project = DamaskProject::discover(&cwd)
        .map_err(|e| anyhow::anyhow!("{}", e))
        .context("no .damask/ found — run `damask init` first")?;

    let db_path = project.damask_dir.join("index.db");
    let edges_dir = project.damask_dir.join("edges");
    let conn = update_index_with_mode(&db_path, &edges_dir, IndexMode::ViewsPreferred)
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    let q = IndexQuery::new(&conn);

    // Only spans carrying open knowledge are worth sweeping.
    let open_edges = q
        .all_active_open_edges()
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    let mut open_by_span: HashMap<String, usize> = HashMap::new();
    for e in &open_edges {
        for id in [e.from_id.as_deref(), e.to_id.as_deref()]
            .into_iter()
            .flatten()
        {
            if id.starts_with("s_") {
                *open_by_span.entry(id.to_string()).or_insert(0) += 1;
            }
        }
    }

    let spans = q
        .all_spans_chronological()
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    let mut drifted = Vec::new();
    let mut gone = Vec::new();
    for s in spans {
        let Some(&n) = open_by_span.get(&s.id) else {
            continue;
        };
        match (s.resolution.as_deref(), s.recency.as_deref()) {
            (Some("missing"), _) | (Some("unresolved"), _) => gone.push((s, n)),
            (Some("relocated"), _) | (_, Some("file_changed")) => drifted.push((s, n)),
            _ => {}
        }
    }

    if reanchor {
        let mut by_ns: HashMap<String, Vec<Fact>> = HashMap::new();
        let mut count = 0;
        for (row, _) in &drifted {
            if let Some(span) = helpers::reanchor_span(&project, row) {
                by_ns
                    .entry(span.ns.clone())
                    .or_default()
                    .push(Fact::Span(span));
                count += 1;
            }
        }
        for (ns, facts) in by_ns {
            FactWriter::append_all(&project.edges_file(&ns), &facts)
                .map_err(|e| anyhow::anyhow!("{}", e))?;
        }
        match format {
            Format::Human => {
                println!(
                    "Re-anchored {count} drifted span(s) — they are fresh (\u{2705}) as of HEAD."
                );
                if !gone.is_empty() {
                    println!(
                        "{} anchor(s) are GONE and can't be re-anchored — review with `damask triage`.",
                        gone.len()
                    );
                }
            }
            Format::Json => println!(
                "{}",
                serde_json::json!({"reanchored": count, "gone": gone.len()})
            ),
        }
        return Ok(());
    }

    // Report mode.
    match format {
        Format::Human => {
            if drifted.is_empty() && gone.is_empty() {
                println!("Sweep clean: every open edge anchors to fresh, matching code. \u{2705}");
                return Ok(());
            }
            println!();
            println!("Damask sweep — anchor freshness for open knowledge");
            if !drifted.is_empty() {
                let total_edges: usize = drifted.iter().map(|(_, n)| n).sum();
                println!();
                println!(
                    "  \u{1F7E0} Drifted ({} spans, {} open edges) — content found, anchor stale:",
                    drifted.len(),
                    total_edges
                );
                for (s, n) in drifted.iter().take(8) {
                    println!(
                        "    {}:{}-{}  ({} edge{})",
                        s.path,
                        s.line_start.unwrap_or(0),
                        s.line_end.unwrap_or(0),
                        n,
                        if *n == 1 { "" } else { "s" }
                    );
                }
                if drifted.len() > 8 {
                    println!("    ... and {} more", drifted.len() - 8);
                }
                println!("    -> damask sweep --reanchor");
            }
            if !gone.is_empty() {
                let total_edges: usize = gone.iter().map(|(_, n)| n).sum();
                println!();
                println!(
                    "  \u{1F534} Gone ({} spans, {} open edges) — anchor no longer exists:",
                    gone.len(),
                    total_edges
                );
                for (s, n) in gone.iter().take(8) {
                    println!(
                        "    {}  ({} edge{})",
                        s.path,
                        n,
                        if *n == 1 { "" } else { "s" }
                    );
                }
                if gone.len() > 8 {
                    println!("    ... and {} more", gone.len() - 8);
                }
                println!("    -> damask triage   (propose closes; nothing closes without a flag)");
            }
            println!();
        }
        Format::Json => {
            println!(
                "{}",
                serde_json::json!({
                    "drifted": drifted.iter().map(|(s, n)| serde_json::json!({
                        "span": s.id, "path": s.path,
                        "line_start": s.line_start, "line_end": s.line_end,
                        "open_edges": n,
                    })).collect::<Vec<_>>(),
                    "gone": gone.iter().map(|(s, n)| serde_json::json!({
                        "span": s.id, "path": s.path, "open_edges": n,
                    })).collect::<Vec<_>>(),
                    "commands": {"reanchor": "damask sweep --reanchor", "triage": "damask triage"},
                })
            );
        }
    }
    Ok(())
}
