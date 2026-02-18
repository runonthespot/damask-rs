use anyhow::Context;
use chrono::Utc;
use damask_core::PayloadEnvelope;
use damask_store::{
    rank_edges, update_index_with_mode, DamaskProject, IndexMode, IndexQuery, RankingInput,
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

    if recent_edges.is_empty() {
        match format {
            Format::Human => println!("No new edges to review."),
            Format::Json => println!("{{\"edges\":[]}}"),
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

            RankingInput {
                edge,
                endorsement_count,
                dispute_count,
                effective_ts,
                half_life_days: half_life,
                now,
                resolution_weight: 1.0,
                signal_density: 1.0,
            }
        })
        .collect();

    let ranked = rank_edges(inputs, 50);

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
                "since": since_ts,
                "edges": edges_json,
            });
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
    }

    Ok(())
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
