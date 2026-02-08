use anyhow::Context;
use damask_core::{Freshness, Recency, Resolution};
use damask_store::{update_index, DamaskProject, IndexQuery};
use std::env;
use std::fs;
use std::io::{BufRead, BufReader};

use crate::error::Result;

pub fn run(span_id: &str) -> Result<()> {
    if !span_id.starts_with("s_") {
        anyhow::bail!("not a valid span ID: {span_id}");
    }

    let cwd = env::current_dir().context("failed to get current directory")?;
    let project = DamaskProject::discover(&cwd)
        .map_err(|e| anyhow::anyhow!("{}", e))
        .context("no .damask/ found — run `damask init` first")?;

    let db_path = project.damask_dir.join("index.db");
    let edges_dir = project.damask_dir.join("edges");
    let conn = update_index(&db_path, &edges_dir).map_err(|e| anyhow::anyhow!("{}", e))?;

    let q = IndexQuery::new(&conn);

    let span = q
        .span_by_id(span_id)
        .map_err(|e| anyhow::anyhow!("{}", e))?
        .ok_or_else(|| anyhow::anyhow!("span not found: {span_id}"))?;

    println!();
    println!("Resolve: {} → {}", span_id, span.path);

    let lines = match (span.line_start, span.line_end) {
        (Some(s), Some(e)) => format!(":{}-{}", s, e),
        _ => String::new(),
    };
    println!("  Location: {}{}", span.path, lines);

    if let Some(ref hash) = span.content_hash {
        println!("  Content hash: {hash}");
    }

    // Use the resolution and recency already computed during index build.
    // The index ran the full resolve_span() cascade; re-resolving here would
    // produce misleading results because the index stores relocated line numbers
    // (a relocated span would re-resolve as Exact at its new location).
    let resolution = span
        .resolution
        .as_deref()
        .and_then(parse_resolution)
        .unwrap_or(Resolution::Exact);
    let recency = span
        .recency
        .as_deref()
        .and_then(parse_recency)
        .unwrap_or(Recency::Unknown);
    let freshness = Freshness::new(resolution, recency);
    let weight = freshness.resolution_weight();

    println!();
    println!("  Resolution: {:?}", resolution);
    println!("  Recency: {:?}", recency);
    println!("  Resolution weight: {:.2}", weight);

    // Show content at the indexed location (which is the relocated location
    // for relocated spans, or the original location for exact spans).
    if let (Some(start), Some(end)) = (span.line_start, span.line_end) {
        let file_path = project.root.join(&span.path);
        if file_path.exists() {
            if let Ok(file) = fs::File::open(&file_path) {
                let reader = BufReader::new(file);
                let file_lines: Vec<String> =
                    reader.lines().collect::<std::io::Result<Vec<_>>>().unwrap_or_default();

                let label = if resolution == Resolution::Relocated {
                    "Content (relocated)"
                } else {
                    "Content"
                };

                println!();
                println!("  {}:", label);
                for (i, line) in file_lines
                    .iter()
                    .enumerate()
                    .skip((start - 1) as usize)
                    .take((end - start + 1) as usize)
                {
                    println!("    {:>4} │ {}", i + 1, line);
                }
            }
        } else {
            println!();
            println!("  File not found: {}", span.path);
        }
    }

    println!();
    Ok(())
}

fn parse_resolution(s: &str) -> Option<Resolution> {
    match s {
        "exact" => Some(Resolution::Exact),
        "relocated" => Some(Resolution::Relocated),
        "unresolved" => Some(Resolution::Unresolved),
        "missing" => Some(Resolution::Missing),
        _ => None,
    }
}

fn parse_recency(s: &str) -> Option<Recency> {
    match s {
        "unchanged" => Some(Recency::Unchanged),
        "file_changed" => Some(Recency::FileChanged),
        "unknown" => Some(Recency::Unknown),
        _ => None,
    }
}
