use anyhow::Context;
use damask_core::PayloadEnvelope;
use damask_store::index::query::EdgeRow;
use damask_store::{update_index_with_mode, DamaskProject, GraphStats, IndexMode, IndexQuery};
use std::collections::HashMap;
use std::env;

use crate::error::Result;
use crate::output::Format;

/// Maximum edges to show per section in human output.
const SECTION_LIMIT: usize = 5;

pub(crate) struct OrientData {
    pub(crate) active_ns: String,
    pub(crate) namespace_count: usize,
    pub(crate) span_count: u64,
    pub(crate) edge_count: u64,
    pub(crate) active_edge_count: u64,
    pub(crate) endorsement_count: u64,
    pub(crate) dispute_count: u64,
    pub(crate) graph_stats: GraphStats,
    pub(crate) namespaces: Vec<NamespaceInfo>,
    /// Dynamic sections — one per rel type that has edges, sorted by count descending.
    pub(crate) sections: Vec<RelSection>,
    pub(crate) recent: Vec<EdgeSummary>,
    /// Spans whose anchor drifted (non-exact resolution) or whose file
    /// changed since annotation — their edges deserve re-confirmation.
    pub(crate) suspect_spans: Vec<SuspectSpan>,
}

pub(crate) struct SuspectSpan {
    pub(crate) path: String,
    pub(crate) lines: Option<(u32, u32)>,
    pub(crate) resolution: String,
    pub(crate) recency: String,
    pub(crate) open_edge_count: usize,
}

pub(crate) struct RelSection {
    pub(crate) rel: String,
    pub(crate) edges: Vec<EdgeSummary>,
}

pub(crate) struct NamespaceInfo {
    pub(crate) name: String,
    pub(crate) edge_count: u64,
    pub(crate) last_modified: Option<String>,
    pub(crate) rels: HashMap<String, u32>,
}

pub(crate) struct EdgeSummary {
    pub(crate) id: String,
    pub(crate) rel: String,
    pub(crate) summary: String,
    pub(crate) confidence: Option<f64>,
    pub(crate) ns: String,
    pub(crate) ts: String,
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
    uncontested: bool,
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
    if uncontested {
        let dispute_count = q.dispute_count(&edge.id).unwrap_or(0);
        if dispute_count > 0 {
            return false;
        }
    }
    true
}

pub fn run(format: Format, rel_filter: Option<&str>, tag_filter: Option<&str>, uncontested: bool, show_closed: bool) -> Result<()> {
    let data = collect(rel_filter, tag_filter, uncontested, show_closed)?;

    match format {
        Format::Human => print_human(&data),
        Format::Json => print_json(&data),
    }

    Ok(())
}

/// Gather orientation data from the index. Shared by `orient` and `briefing`.
pub(crate) fn collect(rel_filter: Option<&str>, tag_filter: Option<&str>, uncontested: bool, show_closed: bool) -> Result<OrientData> {
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
    let graph_stats = q.graph_stats().map_err(|e| anyhow::anyhow!("{}", e))?;

    let all_edges = if show_closed {
        q.all_active_edges().map_err(|e| anyhow::anyhow!("{}", e))?
    } else {
        q.all_active_open_edges().map_err(|e| anyhow::anyhow!("{}", e))?
    };

    let active_ns = project.active_ns().unwrap_or_default();
    let ns_list = project
        .list_namespaces()
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    // Collect namespace stats with rel breakdown
    let mut namespaces = Vec::new();
    for ns_name in &ns_list {
        let ns_stats = q
            .namespace_stats(ns_name)
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        // Count edges by rel for this namespace
        let mut rels: HashMap<String, u32> = HashMap::new();
        for edge in &all_edges {
            if edge.ns == *ns_name {
                *rels.entry(edge.rel.clone()).or_insert(0) += 1;
            }
        }

        namespaces.push(NamespaceInfo {
            name: ns_name.clone(),
            edge_count: ns_stats.edge_count,
            last_modified: ns_stats.last_modified,
            rels,
        });
    }

    // Filter edges before bucketing
    let has_filters = rel_filter.is_some() || tag_filter.is_some() || uncontested;
    let filtered_edges: Vec<&EdgeRow> = if has_filters {
        all_edges.iter().filter(|e| passes_filters(e, rel_filter, tag_filter, uncontested, &q)).collect()
    } else {
        all_edges.iter().collect()
    };

    // Group edges by rel type dynamically
    let mut by_rel: HashMap<String, Vec<EdgeSummary>> = HashMap::new();
    for edge in &filtered_edges {
        by_rel
            .entry(edge.rel.clone())
            .or_default()
            .push(edge_to_summary(edge));
    }

    // Sort each group by confidence descending
    for edges in by_rel.values_mut() {
        edges.sort_by(|a, b| {
            b.confidence
                .unwrap_or(0.0)
                .partial_cmp(&a.confidence.unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    // Build sections sorted by edge count descending (largest groups first)
    let mut sections: Vec<RelSection> = by_rel
        .into_iter()
        .map(|(rel, edges)| RelSection { rel, edges })
        .collect();
    sections.sort_by(|a, b| b.edges.len().cmp(&a.edges.len()));

    // Recent edges: last 5 by timestamp (from filtered set)
    let mut recent: Vec<EdgeSummary> = filtered_edges.iter().map(|e| edge_to_summary(e)).collect();
    recent.sort_by(|a, b| b.ts.cmp(&a.ts));
    recent.truncate(SECTION_LIMIT);

    // Suspect spans: drifted anchors or changed files, with open edges
    // attached — ordered by how much knowledge hangs off them.
    let mut suspect_spans: Vec<SuspectSpan> = Vec::new();
    if let Ok(all_spans) = q.all_spans_chronological() {
        for span in all_spans {
            let resolution = span.resolution.as_deref().unwrap_or("exact");
            let recency = span.recency.as_deref().unwrap_or("unknown");
            let suspect = resolution != "exact" || recency == "file_changed";
            if !suspect {
                continue;
            }
            let open_edge_count = q
                .edges_for_span_open(&span.id)
                .map(|e| e.len())
                .unwrap_or(0);
            if open_edge_count == 0 {
                continue;
            }
            suspect_spans.push(SuspectSpan {
                path: span.path.clone(),
                lines: match (span.line_start, span.line_end) {
                    (Some(s), Some(e)) => Some((s, e)),
                    _ => None,
                },
                resolution: resolution.to_string(),
                recency: recency.to_string(),
                open_edge_count,
            });
        }
    }
    suspect_spans.sort_by(|a, b| b.open_edge_count.cmp(&a.open_edge_count));

    Ok(OrientData {
        active_ns,
        namespace_count: ns_list.len(),
        span_count: stats.span_count,
        edge_count: stats.edge_count,
        active_edge_count: stats.active_edge_count,
        endorsement_count: stats.endorsement_count,
        dispute_count: stats.dispute_count,
        graph_stats,
        namespaces,
        sections,
        recent,
        suspect_spans,
    })
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
        "  {} namespaces | {} spans | {} edges ({} active, {} closed) | {} endorsements | {} disputes",
        data.namespace_count,
        data.span_count,
        data.edge_count,
        data.active_edge_count,
        data.graph_stats.closed_edges,
        data.endorsement_count,
        data.dispute_count,
    );
    if !data.active_ns.is_empty() {
        println!("  Active namespace: {}", data.active_ns);
    }

    // Namespaces with rel breakdown
    println!();
    println!("  Namespaces");
    for ns in &data.namespaces {
        let date = ns.last_modified
            .as_deref()
            .and_then(|ts| ts.split('T').next())
            .unwrap_or("?");
        let marker = if ns.name == data.active_ns { " *" } else { "" };

        // Build rel summary
        let mut rel_parts: Vec<String> = ns.rels.iter()
            .map(|(k, v)| format!("{} {}", v, k))
            .collect();
        rel_parts.sort();
        let rel_summary = if rel_parts.is_empty() {
            String::new()
        } else {
            format!(": {}", rel_parts.join(", "))
        };

        println!("    {}{marker}  ({} edges{rel_summary} — last: {date})", ns.name, ns.edge_count);
    }

    // Sections — one per rel type, sorted by count
    for section in &data.sections {
        print_section(&section.rel, &section.edges);
    }

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

fn summaries_to_json(edges: &[EdgeSummary], limit: usize) -> serde_json::Value {
    let total = edges.len();
    let shown = total.min(limit);
    let items: Vec<serde_json::Value> = edges.iter().take(limit)
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
        .collect();

    serde_json::json!({
        "total": total,
        "shown": shown,
        "edges": items,
    })
}

fn print_json(data: &OrientData) {
    let ns_list: Vec<serde_json::Value> = data.namespaces.iter().map(|ns| {
        let rels: serde_json::Value = ns.rels.iter()
            .map(|(k, v)| (k.clone(), serde_json::json!(v)))
            .collect::<serde_json::Map<String, serde_json::Value>>()
            .into();
        serde_json::json!({
            "name": ns.name,
            "edge_count": ns.edge_count,
            "last_modified": ns.last_modified,
            "rels": rels,
        })
    }).collect();

    // Build dynamic sections map
    let mut sections_map = serde_json::Map::new();
    for section in &data.sections {
        sections_map.insert(
            section.rel.clone(),
            summaries_to_json(&section.edges, SECTION_LIMIT),
        );
    }

    let output = serde_json::json!({
        "context": {
            "graph": {
                "total_edges": data.graph_stats.total_edges,
                "active_edges": data.graph_stats.active_edges,
                "closed_edges": data.graph_stats.closed_edges,
            },
        },
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
        "namespace_list": ns_list,
        "sections": sections_map,
        "recent": summaries_to_json(&data.recent, SECTION_LIMIT),
    });
    println!("{}", serde_json::to_string_pretty(&output).unwrap());
}
