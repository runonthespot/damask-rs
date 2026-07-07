use anyhow::Context;
use chrono::Utc;
use damask_core::PayloadEnvelope;
use damask_store::{
    rank_edges, update_index_with_mode, DamaskProject, IndexMode, IndexQuery, RankingInput,
};
use std::env;

use crate::error::Result;
use crate::output::Format;

pub fn run(format: Format, markdown: bool) -> Result<()> {
    let cwd = env::current_dir().context("failed to get current directory")?;
    let project = DamaskProject::discover(&cwd)
        .map_err(|e| anyhow::anyhow!("{}", e))
        .context("no .damask/ found — run `damask init` first")?;

    let db_path = project.damask_dir.join("index.db");
    let edges_dir = project.damask_dir.join("edges");
    let conn = update_index_with_mode(&db_path, &edges_dir, IndexMode::ViewsPreferred)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let config = project
        .read_config()
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let q = IndexQuery::new(&conn);

    // Determine the cutoff timestamp from last git commit
    let since_ts = last_commit_timestamp(&project.root);

    let edges = q.all_active_edges().map_err(|e| anyhow::anyhow!("{}", e))?;

    // Filter edges to those created since the cutoff
    let recent_edges: Vec<_> = if let Some(ref cutoff) = since_ts {
        edges
            .into_iter()
            .filter(|e| e.ts.as_str() >= cutoff.as_str())
            .collect()
    } else {
        edges
    };

    let graph_stats = q.graph_stats().map_err(|e| anyhow::anyhow!("{}", e))?;

    if recent_edges.is_empty() {
        if markdown {
            println!("_No new damask annotations since the last commit._");
            return Ok(());
        }
        match format {
            Format::Human => println!("No new edges to review."),
            Format::Json => {
                let output = serde_json::json!({
                    "context": {
                        "graph": {
                            "total_edges": graph_stats.total_edges,
                            "active_edges": graph_stats.active_edges,
                        },
                        "since": since_ts,
                        "hint": format!("No edges since last commit. {} active edges exist.", graph_stats.active_edges),
                    },
                    "edges": [],
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            }
        }
        return Ok(());
    }

    // Rank the edges
    let now = Utc::now();
    let inputs: Vec<RankingInput> = recent_edges
        .into_iter()
        .map(|edge| {
            let endorsement_count = q.endorsement_count(&edge.id).unwrap_or(0);
            let dispute_count = q.dispute_count(&edge.id).unwrap_or(0);
            let effective_ts = chrono::DateTime::parse_from_rfc3339(&edge.ts)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or(now);
            let half_life = config.decay_half_life_days(&edge.ns);
            let resolution_weight = super::helpers::edge_resolution_weight(&q, &edge);
            let signal_density = super::helpers::edge_signal_density(&q, &edge);

            RankingInput {
                edge,
                endorsement_count,
                dispute_count,
                effective_ts,
                half_life_days: half_life,
                now,
                resolution_weight,
                signal_density,
                schema_factor: 1.0,
            }
        })
        .collect();

    let ranked = rank_edges(inputs, 50);

    if markdown {
        print_markdown(&q, &ranked, since_ts.as_deref());
        return Ok(());
    }

    match format {
        Format::Human => {
            println!();
            if let Some(ref cutoff) = since_ts {
                let date = cutoff.split('T').next().unwrap_or(cutoff);
                println!("Review: {} edges since {}", ranked.len(), date);
            } else {
                println!("Review: {} active edges (no git history)", ranked.len());
            }
            println!();

            // Group by span
            let mut by_span: std::collections::BTreeMap<
                String,
                Vec<&damask_store::RankedEdge>,
            > = std::collections::BTreeMap::new();
            let mut orphans = Vec::new();

            for re in &ranked {
                if let Some(ref from_id) = re.edge.from_id {
                    if from_id.starts_with("s_") {
                        by_span.entry(from_id.clone()).or_default().push(re);
                        continue;
                    }
                }
                orphans.push(re);
            }

            for (span_id, span_edges) in &by_span {
                if let Ok(Some(span)) = q.span_by_id(span_id) {
                    let lines = match (span.line_start, span.line_end) {
                        (Some(s), Some(e)) => format!(":{}-{}", s, e),
                        _ => String::new(),
                    };
                    println!("  {} {}{}", span_id, span.path, lines);
                } else {
                    println!("  {} (span not found)", span_id);
                }

                for re in span_edges {
                    let p: serde_json::Value =
                        serde_json::from_str(&re.edge.payload).unwrap_or(serde_json::json!({}));
                    let env = PayloadEnvelope::new(&p);
                    let summary = env.summary().unwrap_or("(no summary)");
                    let date = re.edge.ts.split('T').next().unwrap_or(&re.edge.ts);
                    let agent = re.edge.agent.as_deref().unwrap_or("unknown");
                    println!(
                        "    {} [{}] {:.2} {} {} — {}",
                        re.edge.id, re.edge.rel, re.score, date, agent, summary
                    );
                }
                println!();
            }

            if !orphans.is_empty() {
                println!("  Edges without span origin:");
                for re in &orphans {
                    let p: serde_json::Value =
                        serde_json::from_str(&re.edge.payload).unwrap_or(serde_json::json!({}));
                    let env = PayloadEnvelope::new(&p);
                    let summary = env.summary().unwrap_or("(no summary)");
                    let date = re.edge.ts.split('T').next().unwrap_or(&re.edge.ts);
                    println!(
                        "    {} [{}] {:.2} {} — {}",
                        re.edge.id, re.edge.rel, re.score, date, summary
                    );
                }
                println!();
            }
        }
        Format::Json => {
            let edges_json: Vec<serde_json::Value> = ranked
                .iter()
                .map(|re| {
                    let payload: serde_json::Value =
                        serde_json::from_str(&re.edge.payload).unwrap_or(serde_json::json!({}));
                    serde_json::json!({
                        "id": re.edge.id,
                        "from": re.edge.from_id,
                        "to": re.edge.to_id,
                        "rel": re.edge.rel,
                        "payload": payload,
                        "ns": re.edge.ns,
                        "ts": re.edge.ts,
                        "score": re.score,
                        "endorsements": re.endorsement_count,
                        "disputes": re.dispute_count,
                    })
                })
                .collect();

            let output = serde_json::json!({
                "context": {
                    "graph": {
                        "total_edges": graph_stats.total_edges,
                        "active_edges": graph_stats.active_edges,
                    },
                    "since": since_ts,
                },
                "edges": edges_json,
            });
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
    }

    Ok(())
}

/// PR-comment-ready markdown: new annotations grouped by file, with
/// confidence, agent provenance, and actions. Designed for CI:
/// `damask review --markdown | gh pr comment <n> --body-file -`
fn print_markdown(
    q: &IndexQuery,
    ranked: &[damask_store::RankedEdge],
    since: Option<&str>,
) {
    let since_label = since
        .and_then(|s| s.split('T').next().map(String::from))
        .unwrap_or_else(|| "repo start".to_string());
    println!(
        "## Damask review — {} annotation{} since {}\n",
        ranked.len(),
        if ranked.len() == 1 { "" } else { "s" },
        since_label
    );

    // Group by file path via the edge's span endpoint.
    let mut by_file: std::collections::BTreeMap<String, Vec<&damask_store::RankedEdge>> =
        std::collections::BTreeMap::new();
    for re in ranked {
        let span = re
            .edge
            .from_id
            .as_deref()
            .filter(|id| id.starts_with("s_"))
            .or_else(|| re.edge.to_id.as_deref().filter(|id| id.starts_with("s_")))
            .and_then(|id| q.span_by_id(id).ok().flatten());
        let key = match &span {
            Some(s) => match (s.line_start, s.line_end) {
                (Some(a), Some(b)) => format!("`{}` lines {a}–{b}", s.path),
                _ => format!("`{}`", s.path),
            },
            None => "_(no span)_".to_string(),
        };
        by_file.entry(key).or_default().push(re);
    }

    for (file, edges) in &by_file {
        println!("### {file}\n");
        for re in edges {
            let payload: serde_json::Value =
                serde_json::from_str(&re.edge.payload).unwrap_or(serde_json::json!({}));
            let env = PayloadEnvelope::new(&payload);
            let conf = env
                .confidence()
                .map(|c| format!(", confidence {c:.2}"))
                .unwrap_or_default();
            let agent = re
                .edge
                .agent
                .as_deref()
                .map(|a| format!(" — _{a}_"))
                .unwrap_or_default();
            println!(
                "- **{}**{conf}: {}{agent} `{}`",
                re.edge.rel,
                env.summary().unwrap_or("(no summary)"),
                re.edge.id
            );
            if let Some(action) = env.action() {
                println!("  - action: {action}");
            }
        }
        println!();
    }

    println!(
        "_Generated by `damask review --markdown`. Confirm with `damask endorse <id>`, \
         contest with `damask dispute <id>`._"
    );
}

/// Get the timestamp of the last git commit, for filtering "new since last commit".
fn last_commit_timestamp(project_root: &std::path::Path) -> Option<String> {
    let repo = git2::Repository::discover(project_root).ok()?;
    let head = repo.head().ok()?;
    let commit = head.peel_to_commit().ok()?;
    let time = commit.time();
    let secs = time.seconds();
    let dt = chrono::DateTime::from_timestamp(secs, 0)?;
    Some(dt.to_rfc3339())
}
