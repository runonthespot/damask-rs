use anyhow::Context;
use damask_resolve::{resolve_span, SpanAnchor};
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

    // Build a SpanAnchor and run the full resolution cascade
    let anchor = SpanAnchor {
        path: span.path.clone(),
        line_start: span.line_start,
        line_end: span.line_end,
        content_hash: span.content_hash.clone(),
        symbol: span.symbol.clone(),
        snippet: span.snippet.clone(),
        commit: span.commit.clone(),
    };

    match resolve_span(&project.root, &anchor) {
        Ok(result) => {
            let resolution = &result.freshness.resolution;
            let recency = &result.freshness.recency;
            let weight = result.freshness.resolution_weight();

            println!();
            println!("  Resolution: {:?}", resolution);
            println!("  Recency: {:?}", recency);
            println!("  Resolution weight: {:.2}", weight);

            // Show relocated lines if applicable
            if let Some((new_start, new_end)) = result.new_lines {
                println!(
                    "  Relocated to: {}:{}-{}",
                    span.path, new_start, new_end
                );

                // Show content at new location
                let file_path = project.root.join(&span.path);
                if file_path.exists() {
                    if let Ok(file) = fs::File::open(&file_path) {
                        let reader = BufReader::new(file);
                        let file_lines: Vec<String> =
                            reader.lines().collect::<std::io::Result<Vec<_>>>().unwrap_or_default();

                        println!();
                        println!("  Content (relocated):");
                        for (i, line) in file_lines
                            .iter()
                            .enumerate()
                            .skip((new_start - 1) as usize)
                            .take((new_end - new_start + 1) as usize)
                        {
                            println!("    {:>4} │ {}", i + 1, line);
                        }
                    }
                }
            } else if matches!(resolution, damask_core::Resolution::Exact) {
                // Show content at original location
                if let (Some(start), Some(end)) = (span.line_start, span.line_end) {
                    let file_path = project.root.join(&span.path);
                    if file_path.exists() {
                        if let Ok(file) = fs::File::open(&file_path) {
                            let reader = BufReader::new(file);
                            let file_lines: Vec<String> =
                                reader.lines().collect::<std::io::Result<Vec<_>>>().unwrap_or_default();

                            println!();
                            println!("  Content:");
                            for (i, line) in file_lines
                                .iter()
                                .enumerate()
                                .skip((start - 1) as usize)
                                .take((end - start + 1) as usize)
                            {
                                println!("    {:>4} │ {}", i + 1, line);
                            }
                        }
                    }
                }
            }
        }
        Err(e) => {
            println!();
            println!("  Resolution failed: {e}");
        }
    }

    println!();
    Ok(())
}
