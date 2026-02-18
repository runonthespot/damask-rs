use anyhow::Context;
use damask_core::PayloadEnvelope;
use damask_store::index::query::EdgeRow;
use damask_store::{update_index_with_mode, DamaskProject, IndexMode, IndexQuery};
use std::env;

use crate::error::Result;
use crate::output::Format;

/// Maximum edges to show per section in human output.
const SECTION_LIMIT: usize = 5;

struct OrientData {
    active_ns: String,
    namespace_count: usize,
    span_count: u64,
    edge_count: u64,
    active_edge_count: u64,
    endorsement_count: u64,
    dispute_count: u64,
    namespaces: Vec<(String, u64, Option<String>)>, // (name, edge_count, last_modified)
    risks: Vec<EdgeSummary>,
    gotchas: Vec<EdgeSummary>,
    decisions: Vec<EdgeSummary>,
    invariants: Vec<EdgeSummary>,
    recent: Vec<EdgeSummary>,
}

struct EdgeSummary {
    id: String,
    rel: String,
    summary: String,
    confidence: Option<f64>,
    ns: String,
    ts: String,
}

fn edge_to_summary(edge: &EdgeRow) -> EdgeSummary {
    let payload: serde_json::Value =
        serde_json::from_str(&edge.payload).unwrap_or(serde_json::json!({}));
    let env = PayloadEnvelope::new(&payload);
    EdgeSummary {
        id: edge.id.clone(),
        rel: edge.rel.clone(),
        summary: env
            .summary()
            .map(|s| s.to_string())
            .unwrap_or_else(|| damask_core::truncate_str(&edge.payload, 80).to_string()),
        confidence: env.confidence(),
        ns: edge.ns.clone(),
        ts: edge.ts.clone(),
    }
}

/// Check if an edge passes the native filters.
fn passes_filters(
    edge: &EdgeRow,
    rel_filter: Option<&str>,
    tag_filter: Option<&str>,
    undisputed: bool,
    q: &IndexQuery,
) -> bool {
    if let Some(rel) = rel_filter {
        if edge.rel != rel {
            return false;
        }
    }
    if let Some(tag) = tag_filter {
        let payload: serde_json::Value =
            serde_json::from_str(&edge.payload).unwrap_or(serde_json::json!({}));
        let env = PayloadEnvelope::new(&payload);
        let tags = env.tags().unwrap_or_default();
        if !tags.iter().any(|t| *t == tag) {
            return false;
        }
    }
    if undisputed {
        let dispute_count = q.dispute_count(&edge.id).unwrap_or(0);
        if dispute_count > 0 {
            return false;
        }
    }
    true
}

pub fn run(format: Format, rel_filter: Option<&str>, tag_filter: Option<&str>, undisputed: bool) -> Result<()> {
    let cwd = env::current_dir().context("failed to get current directory")?;
    let project = DamaskProject::discover(&cwd)
        .map_err(|e| anyhow::anyhow!("{}", e))
        .context("no .damask/ found \u{2014} run `damask init` first")?;

    let db_path = project.damask_dir.join("index.db");
    let edges_dir = project.damask_dir.join("edges");
    let conn = update_index_with_mode(&db_path, &edges_dir, IndexMode::ViewsPreferred)
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let q = IndexQuery::new(&conn);
    let stats = q.project_stats().map_err(|e| anyhow::anyhow!("{}", e))?;
    let all_edges = q.all_active_edges().map_err(|e| anyhow::anyhow!("{}", e))?;

    let active_ns = project.active_ns().unwrap_or_default();
    let ns_list = project
        .list_namespaces()
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    // Collect namespace stats
    let mut namespaces = Vec::new();
    for ns_name in &ns_list {
        let ns_stats = q
            .namespace_stats(ns_name)
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        namespaces.push((
            ns_name.clone(),
            ns_stats.edge_count,
            ns_stats.last_modified,
        ));
    }

    // Filter edges before bucketing
    let has_filters = rel_filter.is_some() || tag_filter.is_some() || undisputed;
    let filtered_edges: Vec<&EdgeRow> = if has_filters {
        all_edges.iter().filter(|e| passes_filters(e, rel_filter, tag_filter, undisputed, &q)).collect()
    } else {
        all_edges.iter().collect()
    };

    // Bucket edges by rel type
    let mut risks = Vec::new();
    let mut gotchas = Vec::new();
    let mut decisions = Vec::new();
    let mut invariants = Vec::new();

    for edge in &filtered_edges {
        match edge.rel.as_str() {
            "risk" => risks.push(edge_to_summary(edge)),
            "gotcha" => gotchas.push(edge_to_summary(edge)),
            "decision" => decisions.push(edge_to_summary(edge)),
            "invariant" => invariants.push(edge_to_summary(edge)),
            _ => {}
        }
    }

    // Sort by confidence descending
    for list in [&mut risks, &mut gotchas, &mut decisions, &mut invariants] {
        list.sort_by(|a, b| {
            b.confidence
                .unwrap_or(0.0)
                .partial_cmp(&a.confidence.unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    // Recent edges: last 5 by timestamp (from filtered set)
    let mut recent: Vec<EdgeSummary> = filtered_edges.iter().map(|e| edge_to_summary(e)).collect();
    recent.sort_by(|a, b| b.ts.cmp(&a.ts));
    recent.truncate(SECTION_LIMIT);

    let data = OrientData {
        active_ns,
        namespace_count: ns_list.len(),
        span_count: stats.span_count,
        edge_count: stats.edge_count,
        active_edge_count: stats.active_edge_count,
        endorsement_count: stats.endorsement_count,
        dispute_count: stats.dispute_count,
        namespaces,
        risks,
        gotchas,
        decisions,
        invariants,
        recent,
    };

    match format {
        Format::Human => print_human(&data),
        Format::Json => print_json(&data),
    }

    Ok(())
}

fn print_human(data: &OrientData) {
    let bar = "\u{2500}".repeat(60);

    // Cold start detection
    if data.active_edge_count == 0 {
        println!();
        println!("Damask: empty graph (cold start)");
        println!("{bar}");
        println!("  No edges found. This is a fresh codebase.");
        println!("  Use `damask span` and `damask edge` to start recording.");
        println!();
        return;
    }

    println!();
    println!("Damask orientation");
    println!("{bar}");

    // Status line
    println!(
        "  {} namespaces | {} spans | {} edges ({} active) | {} endorsements | {} disputes",
        data.namespace_count,
        data.span_count,
        data.edge_count,
        data.active_edge_count,
        data.endorsement_count,
        data.dispute_count,
    );
    if !data.active_ns.is_empty() {
        println!("  Active namespace: {}", data.active_ns);
    }

    // Namespaces
    println!();
    println!("  Namespaces");
    for (name, count, last_mod) in &data.namespaces {
        let date = last_mod
            .as_deref()
            .and_then(|ts| ts.split('T').next())
            .unwrap_or("?");
        let marker = if *name == data.active_ns { " *" } else { "" };
        println!("    {name}{marker}  ({count} edges, last: {date})");
    }

    // Sections
    print_section("Risks", &data.risks);
    print_section("Gotchas", &data.gotchas);
    print_section("Decisions", &data.decisions);
    print_section("Invariants", &data.invariants);

    // Recent
    if !data.recent.is_empty() {
        println!();
        println!("  Recent");
        for e in &data.recent {
            let date = e.ts.split('T').next().unwrap_or(&e.ts);
            let trunc = damask_core::truncate_str(&e.summary, 70);
            println!("    [{date}] [{}] {trunc}", e.rel);
        }
    }

    println!();
}

fn print_section(title: &str, edges: &[EdgeSummary]) {
    if edges.is_empty() {
        return;
    }
    println!();
    let total = edges.len();
    println!("  {title} ({total})");
    for e in edges.iter().take(SECTION_LIMIT) {
        let conf = e
            .confidence
            .map(|c| format!("({:.2}) ", c))
            .unwrap_or_default();
        let trunc = damask_core::truncate_str(&e.summary, 70);
        println!("    {conf}{trunc}");
    }
    if total > SECTION_LIMIT {
        println!("    ... and {} more", total - SECTION_LIMIT);
    }
}

fn summaries_to_json(edges: &[EdgeSummary]) -> Vec<serde_json::Value> {
    edges
        .iter()
        .map(|e| {
            serde_json::json!({
                "id": e.id,
                "rel": e.rel,
                "summary": e.summary,
                "confidence": e.confidence,
                "ns": e.ns,
                "ts": e.ts,
            })
        })
        .collect()
}

fn print_json(data: &OrientData) {
    let output = serde_json::json!({
        "cold_start": data.active_edge_count == 0,
        "status": {
            "namespaces": data.namespace_count,
            "active_ns": if data.active_ns.is_empty() { None } else { Some(&data.active_ns) },
            "spans": data.span_count,
            "edges": data.edge_count,
            "active_edges": data.active_edge_count,
            "endorsements": data.endorsement_count,
            "disputes": data.dispute_count,
        },
        "namespace_list": data.namespaces.iter().map(|(name, count, last_mod)| {
            serde_json::json!({
                "name": name,
                "edge_count": count,
                "last_modified": last_mod,
            })
        }).collect::<Vec<_>>(),
        "risks": summaries_to_json(&data.risks),
        "gotchas": summaries_to_json(&data.gotchas),
        "decisions": summaries_to_json(&data.decisions),
        "invariants": summaries_to_json(&data.invariants),
        "recent": summaries_to_json(&data.recent),
    });
    println!("{}", serde_json::to_string_pretty(&output).unwrap());
}
