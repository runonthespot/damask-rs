use anyhow::Context;
use damask_store::{update_index, DamaskProject, FactReader, FactWriter, IndexQuery};
use std::env;

use crate::app::NsAction;
use crate::error::Result;
use crate::output::Format;

pub fn run(action: NsAction, format: Format) -> Result<()> {
    let cwd = env::current_dir().context("failed to get current directory")?;
    let project = DamaskProject::discover(&cwd)
        .map_err(|e| anyhow::anyhow!("{}", e))
        .context("no .damask/ found — run `damask init` first")?;

    match action {
        NsAction::Set { name } => {
            project
                .set_active_ns(&name)
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            println!("Active namespace: {name}");
        }
        NsAction::List { stale } => {
            let namespaces = project
                .list_namespaces()
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            let active = project.active_ns();

            if namespaces.is_empty() {
                println!("No namespaces yet. Create edges to get started.");
                return Ok(());
            }

            // Build index for health metrics
            let db_path = project.damask_dir.join("index.db");
            let edges_dir = project.damask_dir.join("edges");
            let conn = update_index(&db_path, &edges_dir).map_err(|e| anyhow::anyhow!("{}", e))?;
            let q = IndexQuery::new(&conn);

            match format {
                Format::Human => {
                    for ns in &namespaces {
                        let stats = q
                            .namespace_stats(ns)
                            .map_err(|e| anyhow::anyhow!("{}", e))?;

                        // If --stale, skip namespaces with recent activity
                        if stale {
                            let is_stale = stats.last_modified.as_deref().map_or(true, |ts| {
                                let age_days = chrono::DateTime::parse_from_rfc3339(ts)
                                    .ok()
                                    .map(|dt| {
                                        (chrono::Utc::now() - dt.with_timezone(&chrono::Utc))
                                            .num_days()
                                    })
                                    .unwrap_or(999);
                                age_days > 30
                            });
                            if !is_stale {
                                continue;
                            }
                        }

                        let marker = if active.as_deref() == Some(ns.as_str()) {
                            " *"
                        } else {
                            ""
                        };
                        let last_mod = stats
                            .last_modified
                            .as_deref()
                            .and_then(|ts| ts.split('T').next())
                            .unwrap_or("never");
                        let ratio = if stats.endorsement_count + stats.dispute_count > 0 {
                            format!(
                                " ({}/{} endorse/dispute)",
                                stats.endorsement_count, stats.dispute_count
                            )
                        } else {
                            String::new()
                        };
                        println!(
                            "  {ns}{marker}  {} edges  last: {last_mod}{ratio}",
                            stats.edge_count
                        );
                    }
                }
                Format::Json => {
                    let ns_json: Vec<serde_json::Value> = namespaces
                        .iter()
                        .map(|ns| {
                            let stats = q.namespace_stats(ns).ok();
                            serde_json::json!({
                                "name": ns,
                                "is_active": active.as_deref() == Some(ns.as_str()),
                                "edge_count": stats.as_ref().map(|s| s.edge_count).unwrap_or(0),
                                "last_modified": stats.as_ref().and_then(|s| s.last_modified.clone()),
                                "endorsements": stats.as_ref().map(|s| s.endorsement_count).unwrap_or(0),
                                "disputes": stats.as_ref().map(|s| s.dispute_count).unwrap_or(0),
                            })
                        })
                        .collect();
                    let json = serde_json::json!({
                        "namespaces": ns_json,
                        "active": active,
                    });
                    println!("{}", serde_json::to_string_pretty(&json)?);
                }
            }
        }
        NsAction::Merge { source, target } => {
            if source == target {
                anyhow::bail!("source and target namespaces must be different");
            }

            let source_path = project.edges_file(&source);
            let target_path = project.edges_file(&target);

            if !source_path.exists() {
                anyhow::bail!("source namespace '{source}' has no JSONL file");
            }

            // Read all facts from source
            let mut reader =
                FactReader::open(&source_path).map_err(|e| anyhow::anyhow!("{}", e))?;
            let facts = reader.read_all().map_err(|e| anyhow::anyhow!("{}", e))?;

            if facts.is_empty() {
                println!("Source namespace '{source}' is empty — nothing to merge.");
                return Ok(());
            }

            // Rewrite the ns field on each fact to the target namespace
            let retagged: Vec<_> = facts
                .into_iter()
                .map(|mut fact| {
                    fact.set_ns(target.clone());
                    fact
                })
                .collect();

            // Append retagged facts to target
            FactWriter::append_all(&target_path, &retagged)
                .map_err(|e| anyhow::anyhow!("{}", e))?;

            println!(
                "Merged {} facts from '{source}' into '{target}'.",
                retagged.len()
            );
            println!("Source file preserved. Remove manually if desired:");
            println!("  rm {}", source_path.display());
        }
    }

    Ok(())
}
