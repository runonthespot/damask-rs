use anyhow::Context;
use chrono::Utc;
use damask_core::PayloadEnvelope;
use damask_store::{
    rank_edges, update_index_with_mode, DamaskProject, IndexMode, IndexQuery, Predicate,
    RankedEdge, RankingInput,
};
use std::env;

use crate::error::Result;
use crate::output::Format;

#[allow(clippy::too_many_arguments)]
pub fn run(
    query: &str,
    ns: Option<&str>,
    rel: Option<&str>,
    where_preds: &[String],
    sem: bool,
    limit: usize,
    offset: usize,
    show_closed: bool,
    format: Format,
) -> Result<()> {
    let preds: Vec<Predicate> = where_preds
        .iter()
        .map(|s| Predicate::parse(s).map_err(|e| anyhow::anyhow!("{e}")))
        .collect::<Result<_>>()?;

    let cwd = env::current_dir().context("failed to get current directory")?;
    let project = DamaskProject::discover(&cwd)
        .map_err(|e| anyhow::anyhow!("{}", e))
        .context("no .damask/ found — run `damask init` first")?;

    let db_path = project.damask_dir.join("index.db");
    let edges_dir = project.damask_dir.join("edges");
    let conn = update_index_with_mode(&db_path, &edges_dir, IndexMode::ViewsPreferred)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let q = IndexQuery::new(&conn);
    let graph_stats = q.graph_stats().map_err(|e| anyhow::anyhow!("{}", e))?;
    let config = project
        .read_config()
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    // Semantic mode rides on ck when present; otherwise fall back to FTS
    // with a hint rather than failing — not everyone has both tools.
    let mut mode = "keyword";
    let candidates = if sem {
        match crate::ck::semantic_edge_hits(&project, query, 200) {
            Some(hits) => {
                mode = "semantic";
                hits.iter()
                    .filter_map(|h| q.edge_by_id(&h.edge_id).ok().flatten())
                    .filter(|e| show_closed || !e.is_closed)
                    .filter(|e| ns.map_or(true, |n| e.ns == n))
                    .filter(|e| rel.map_or(true, |r| e.rel == r))
                    .collect()
            }
            None => {
                if crate::ck::ck_available() {
                    eprintln!("note: semantic search failed; falling back to keyword search");
                } else {
                    eprintln!(
                        "note: --sem needs ck; using keyword search. {}",
                        crate::ck::CK_HINT
                    );
                }
                fts_search(&q, query, ns, rel, show_closed)?
            }
        }
    } else {
        fts_search(&q, query, ns, rel, show_closed)?
    };

    // FTS narrows by text match; the composite ranking (confidence,
    // endorsements, completeness, decay) orders by quality. --where
    // predicates filter in between.
    let now = Utc::now();
    let mut inputs = Vec::new();
    for edge in candidates {
        let endorsement_count = q
            .endorsement_count(&edge.id)
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        let dispute_count = q
            .dispute_count(&edge.id)
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        if !preds
            .iter()
            .all(|p| p.matches(&edge, endorsement_count, dispute_count))
        {
            continue;
        }

        let effective_ts = chrono::DateTime::parse_from_rfc3339(&edge.ts)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or(now);
        let half_life = config.decay_half_life_days(&edge.ns);
        let resolution_weight = super::helpers::edge_resolution_weight(&q, &edge);
        inputs.push(RankingInput {
            edge,
            endorsement_count,
            dispute_count,
            effective_ts,
            half_life_days: half_life,
            now,
            resolution_weight,
            signal_density: 1.0,
            schema_factor: 1.0,
        });
    }

    let ranked = rank_edges(inputs, usize::MAX);
    let total = ranked.len();
    let page: Vec<RankedEdge> = ranked.into_iter().skip(offset).take(limit).collect();
    let count = page.len();

    match format {
        Format::Human => {
            if count == 0 {
                println!(
                    "0 results for \"{}\" ({} active edges exist — try damask orient)",
                    query, graph_stats.active_edges
                );
                return Ok(());
            }

            println!();
            let start = offset + 1;
            let end = offset + count;
            let filters = if where_preds.is_empty() {
                String::new()
            } else {
                format!(" [{}]", where_preds.join(" AND "))
            };
            println!(
                "Search: \"{}\"{} ({mode}; showing {}-{} of {}, ranked)",
                query, filters, start, end, total
            );
            println!();

            for re in &page {
                let edge = &re.edge;
                let payload: serde_json::Value =
                    serde_json::from_str(&edge.payload).unwrap_or(serde_json::json!({}));
                let env = PayloadEnvelope::new(&payload);
                let summary = env
                    .summary()
                    .unwrap_or_else(|| damask_core::truncate_str(&edge.payload, 60));
                let date = edge.ts.split('T').next().unwrap_or(&edge.ts);

                println!(
                    "  {} [{}] ({:.2}) — {}",
                    edge.id, edge.rel, re.score, summary
                );
                println!("    [{}, {}]", edge.ns, date);
                println!();
            }

            if offset + count < total {
                let next_offset = offset + count;
                println!("  Next: damask search \"{query}\" --offset {next_offset}");
            }
        }
        Format::Json => {
            let edges_json: Vec<serde_json::Value> = page
                .iter()
                .map(|re| {
                    let edge = &re.edge;
                    let payload: serde_json::Value =
                        serde_json::from_str(&edge.payload).unwrap_or(serde_json::json!({}));
                    serde_json::json!({
                        "id": edge.id,
                        "from": edge.from_id,
                        "to": edge.to_id,
                        "rel": edge.rel,
                        "payload": payload,
                        "ns": edge.ns,
                        "ts": edge.ts,
                        "score": re.score,
                        "endorsements": re.endorsement_count,
                        "disputes": re.dispute_count,
                    })
                })
                .collect();

            let mut output = serde_json::json!({
                "context": {
                    "graph": {
                        "total_edges": graph_stats.total_edges,
                        "active_edges": graph_stats.active_edges,
                        "closed_edges": graph_stats.closed_edges,
                    },
                    "showing": {
                        "offset": offset,
                        "limit": limit,
                        "count": count,
                        "total": total,
                    },
                },
                "query": query,
                "mode": mode,
                "where": where_preds,
                "results": edges_json,
            });

            if count == 0 {
                output["context"]["hint"] = serde_json::json!(format!(
                    "0 FTS results. {} active edges exist. Try damask orient for overview.",
                    graph_stats.active_edges
                ));
            }

            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
    }

    Ok(())
}

fn fts_search(
    q: &IndexQuery,
    query: &str,
    ns: Option<&str>,
    rel: Option<&str>,
    show_closed: bool,
) -> Result<Vec<damask_store::index::query::EdgeRow>> {
    // Sanitize by default: searching `read-modify-write` — a string
    // verbatim in stored payloads — used to throw "no such column:
    // modify" because raw text hit the FTS5 syntax parser. Quoted tokens
    // behave identically to plain words for ordinary queries.
    let sanitized = super::helpers::sanitize_fts_query(query);
    if show_closed {
        q.search_fts(&sanitized, ns, rel)
            .map_err(|e| anyhow::anyhow!("{}", e))
    } else {
        q.search_fts_open(&sanitized, ns, rel)
            .map_err(|e| anyhow::anyhow!("{}", e))
    }
}
