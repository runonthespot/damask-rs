use anyhow::Context;
use damask_store::{
    lint_edges, update_index_with_mode, DamaskProject, IndexMode, IndexQuery, LintInput, Severity,
};
use std::env;

use crate::error::Result;
use crate::output::Format;

pub fn run(format: Format) -> Result<()> {
    let cwd = env::current_dir().context("failed to get current directory")?;
    let project = DamaskProject::discover(&cwd)
        .map_err(|e| anyhow::anyhow!("{}", e))
        .context("no .damask/ found — run `damask init` first")?;

    let db_path = project.damask_dir.join("index.db");
    let edges_dir = project.damask_dir.join("edges");
    let conn = update_index_with_mode(&db_path, &edges_dir, IndexMode::ViewsPreferred)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let q = IndexQuery::new(&conn);
    // Lint the LIVE graph only — closed edges are resolved/retired and their
    // stale anchors are irrelevant (nagging about a closed edge's drift is
    // noise). Matches the open-set every other read surface uses.
    let all_edges = q
        .all_active_open_edges()
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    // Build lint inputs with span snippets and resolution data.
    // Try from_id first; if it's not a span, fall back to to_id.
    let inputs: Vec<LintInput> = all_edges
        .into_iter()
        .map(|edge| {
            let span_data = edge
                .from_id
                .as_deref()
                .and_then(|id| {
                    if id.starts_with("s_") {
                        q.span_by_id(id).ok().flatten()
                    } else {
                        None
                    }
                })
                .or_else(|| {
                    edge.to_id.as_deref().and_then(|id| {
                        if id.starts_with("s_") {
                            q.span_by_id(id).ok().flatten()
                        } else {
                            None
                        }
                    })
                });
            let span_snippet = span_data.as_ref().and_then(|s| s.snippet.clone());
            let resolution = span_data.as_ref().and_then(|s| s.resolution.clone());
            LintInput {
                edge,
                span_snippet,
                resolution,
            }
        })
        .collect();

    let issues = lint_edges(&inputs);

    match format {
        Format::Human => {
            if issues.is_empty() {
                println!("No lint issues found.");
                return Ok(());
            }

            let errors = issues
                .iter()
                .filter(|i| i.severity == Severity::Error)
                .count();
            let warnings = issues
                .iter()
                .filter(|i| i.severity == Severity::Warning)
                .count();

            println!();
            for issue in &issues {
                let prefix = match issue.severity {
                    Severity::Error => "\u{274C}",
                    Severity::Warning => "\u{26A0}",
                };
                println!("  {} {} [{}]", prefix, issue.message, issue.edge_id);
            }
            println!();

            // Find top issue rule
            let mut rule_counts: std::collections::HashMap<&str, usize> =
                std::collections::HashMap::new();
            for issue in &issues {
                *rule_counts.entry(issue.rule).or_insert(0) += 1;
            }
            let top_rule = rule_counts.iter().max_by_key(|(_, c)| **c);
            let top_hint = if let Some((rule, count)) = top_rule {
                if *count > 5 {
                    format!("\n  Top issue: {} ({} warnings)", rule, count)
                } else {
                    String::new()
                }
            } else {
                String::new()
            };

            println!(
                "  {} issues ({} errors, {} warnings) across {} edges{}",
                issues.len(),
                errors,
                warnings,
                inputs.len(),
                top_hint,
            );
        }
        Format::Json => {
            let issues_json: Vec<serde_json::Value> = issues
                .iter()
                .map(|i| {
                    serde_json::json!({
                        "edge_id": i.edge_id,
                        "severity": match i.severity {
                            Severity::Error => "error",
                            Severity::Warning => "warning",
                        },
                        "rule": i.rule,
                        "message": i.message,
                    })
                })
                .collect();
            let output = serde_json::json!({
                "issues": issues_json,
                "error_count": issues.iter().filter(|i| i.severity == Severity::Error).count(),
                "warning_count": issues.iter().filter(|i| i.severity == Severity::Warning).count(),
            });
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
    }

    Ok(())
}
