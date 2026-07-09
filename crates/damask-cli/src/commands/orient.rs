use anyhow::Context;
use damask_core::PayloadEnvelope;
use damask_store::index::query::{EdgeRow, SpanRow};
use damask_store::{
    rank_edges, update_index_with_mode, DamaskProject, GraphStats, IndexMode, IndexQuery,
    RankingInput,
};
use std::collections::HashMap;
use std::env;

use crate::error::Result;
use crate::output::Format;

use super::at::{edge_target_span_id, freshness_glyph};
use super::helpers;

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
    /// Open edges considered for trust accounting.
    pub(crate) open_edge_total: usize,
    /// Of those, edges anchored to missing/unresolvable code.
    pub(crate) stale_anchored: usize,
}

pub(crate) struct SuspectSpan {
    pub(crate) path: String,
    pub(crate) lines: Option<(u32, u32)>,
    pub(crate) resolution: String,
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
    pub(crate) endorsements: u32,
    pub(crate) disputes: u32,
    /// Anchor location `path:start-end` from the spans table (effective,
    /// post-rename/relocation values), when the edge has a span endpoint.
    pub(crate) anchor: Option<String>,
    /// Freshness glyph for the anchor span ("" when unknown/unanchored).
    pub(crate) glyph: &'static str,
}

fn edge_to_summary(
    edge: &EdgeRow,
    spans: &HashMap<String, SpanRow>,
    endorse_counts: &HashMap<String, u32>,
    dispute_counts: &HashMap<String, u32>,
) -> EdgeSummary {
    let payload: serde_json::Value =
        serde_json::from_str(&edge.payload).unwrap_or(serde_json::json!({}));
    let env = PayloadEnvelope::new(&payload);
    let anchor_span = edge_target_span_id(edge).and_then(|id| spans.get(id));
    let anchor = anchor_span.map(|s| match (s.line_start, s.line_end) {
        (Some(a), Some(b)) => format!("{}:{}-{}", s.path, a, b),
        _ => s.path.clone(),
    });
    let glyph = anchor_span
        .map(|s| freshness_glyph(s.resolution.as_deref(), s.recency.as_deref()))
        .unwrap_or("");
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
        endorsements: endorse_counts.get(&edge.id).copied().unwrap_or(0),
        disputes: dispute_counts.get(&edge.id).copied().unwrap_or(0),
        anchor,
        glyph,
    }
}

/// Check if an edge passes the native filters.
fn passes_filters(
    edge: &EdgeRow,
    rel_filter: Option<&str>,
    tag_filter: Option<&str>,
    uncontested: bool,
    dispute_counts: &HashMap<String, u32>,
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
        if !tags.contains(&tag) {
            return false;
        }
    }
    if uncontested && dispute_counts.get(&edge.id).copied().unwrap_or(0) > 0 {
        return false;
    }
    true
}

pub fn run(
    format: Format,
    rel_filter: Option<&str>,
    tag_filter: Option<&str>,
    uncontested: bool,
    show_closed: bool,
) -> Result<()> {
    let data = collect(rel_filter, tag_filter, uncontested, show_closed)?;

    match format {
        Format::Human => print_human(&data),
        Format::Json => print_json(&data),
    }

    Ok(())
}

/// Gather orientation data from the index. Shared by `orient` and `briefing`.
pub(crate) fn collect(
    rel_filter: Option<&str>,
    tag_filter: Option<&str>,
    uncontested: bool,
    show_closed: bool,
) -> Result<OrientData> {
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
        q.all_active_open_edges()
            .map_err(|e| anyhow::anyhow!("{}", e))?
    };

    // Bulk lookups — one query each, instead of per-edge/per-span loops.
    let spans_map: HashMap<String, SpanRow> = q
        .all_spans_chronological()
        .map_err(|e| anyhow::anyhow!("{}", e))?
        .into_iter()
        .map(|s| (s.id.clone(), s))
        .collect();
    let endorse_counts = q
        .endorsement_counts()
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    let dispute_counts = q.dispute_counts().map_err(|e| anyhow::anyhow!("{}", e))?;
    let config = project
        .read_config()
        .map_err(|e| anyhow::anyhow!("{}", e))?;

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
        all_edges
            .iter()
            .filter(|e| passes_filters(e, rel_filter, tag_filter, uncontested, &dispute_counts))
            .collect()
    } else {
        all_edges.iter().collect()
    };

    // Group edges by rel type dynamically
    let mut by_rel: HashMap<String, Vec<&EdgeRow>> = HashMap::new();
    for edge in &filtered_edges {
        by_rel.entry(edge.rel.clone()).or_default().push(edge);
    }

    // Rank each group with the same freshness- and trust-weighted scorer
    // `at` uses — NOT raw stored confidence, which lets months-dead,
    // many-times-disputed findings lead every section forever.
    let now = chrono::Utc::now();
    let rank_group = |edges: Vec<&EdgeRow>| -> Vec<EdgeSummary> {
        let inputs: Vec<RankingInput> = edges
            .into_iter()
            .map(|edge| {
                let anchor_span = edge_target_span_id(edge).and_then(|id| spans_map.get(id));
                let resolution_weight = anchor_span
                    .map(helpers::span_freshness_weight)
                    .unwrap_or(1.0);
                let signal_density = helpers::payload_signal_density(
                    anchor_span.and_then(|s| s.snippet.as_deref()),
                    &edge.payload,
                );
                let effective_ts = chrono::DateTime::parse_from_rfc3339(&edge.ts)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or(now);
                let schema_factor = {
                    let payload: serde_json::Value =
                        serde_json::from_str(&edge.payload).unwrap_or(serde_json::json!({}));
                    config.schema_rank_factor(&edge.ns, &payload)
                };
                RankingInput {
                    edge: edge.clone(),
                    endorsement_count: endorse_counts.get(&edge.id).copied().unwrap_or(0),
                    dispute_count: dispute_counts.get(&edge.id).copied().unwrap_or(0),
                    effective_ts,
                    half_life_days: config.decay_half_life_days(&edge.ns),
                    now,
                    resolution_weight,
                    signal_density,
                    schema_factor,
                }
            })
            .collect();
        rank_edges(inputs, usize::MAX)
            .iter()
            .map(|re| edge_to_summary(&re.edge, &spans_map, &endorse_counts, &dispute_counts))
            .collect()
    };

    // Build sections sorted by edge count descending (largest groups first)
    let mut sections: Vec<RelSection> = by_rel
        .into_iter()
        .map(|(rel, edges)| RelSection {
            rel,
            edges: rank_group(edges),
        })
        .collect();
    sections.sort_by(|a, b| b.edges.len().cmp(&a.edges.len()));

    // Recent edges: last 5 by timestamp (from filtered set)
    let mut recent: Vec<EdgeSummary> = filtered_edges
        .iter()
        .map(|e| edge_to_summary(e, &spans_map, &endorse_counts, &dispute_counts))
        .collect();
    recent.sort_by(|a, b| b.ts.cmp(&a.ts));
    recent.truncate(SECTION_LIMIT);

    // Open-edge counts per span and trust accounting, derived from the
    // already-loaded edge set in one pass — no per-span queries.
    let mut open_edges_by_span: HashMap<&str, usize> = HashMap::new();
    let mut open_edge_total = 0usize;
    let mut stale_anchored = 0usize;
    for edge in &all_edges {
        if edge.is_closed {
            continue;
        }
        open_edge_total += 1;
        for endpoint in [edge.from_id.as_deref(), edge.to_id.as_deref()] {
            if let Some(id) = endpoint.filter(|id| id.starts_with("s_")) {
                *open_edges_by_span.entry(id).or_insert(0) += 1;
            }
        }
        if let Some(span) = edge_target_span_id(edge).and_then(|id| spans_map.get(id)) {
            if matches!(
                span.resolution.as_deref(),
                Some("missing") | Some("unresolved")
            ) {
                stale_anchored += 1;
            }
        }
    }

    // Suspect spans: drifted anchors or changed files, with open edges
    // attached — ordered by how much knowledge hangs off them.
    let mut suspect_spans: Vec<SuspectSpan> = Vec::new();
    for span in spans_map.values() {
        let resolution = span.resolution.as_deref().unwrap_or("exact");
        let recency = span.recency.as_deref().unwrap_or("unknown");
        let suspect = resolution != "exact" || recency == "file_changed";
        if !suspect {
            continue;
        }
        let open_edge_count = open_edges_by_span
            .get(span.id.as_str())
            .copied()
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
            open_edge_count,
        });
    }
    suspect_spans.sort_by(|a, b| {
        b.open_edge_count
            .cmp(&a.open_edge_count)
            .then_with(|| a.path.cmp(&b.path))
    });

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
        open_edge_total,
        stale_anchored,
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
        println!("  Seed instantly: `damask bootstrap` (manifests, TODOs, co-change history)");
        println!("  Record findings: `damask record <file> <start> <end> <rel> -m \"...\" -c 0.8`");
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
    print_trust_line(data);

    // Namespaces with rel breakdown
    println!();
    println!("  Namespaces");
    for ns in &data.namespaces {
        let date = ns
            .last_modified
            .as_deref()
            .and_then(|ts| ts.split('T').next())
            .unwrap_or("?");
        let marker = if ns.name == data.active_ns { " *" } else { "" };

        // Build rel summary
        let mut rel_parts: Vec<String> = ns
            .rels
            .iter()
            .map(|(k, v)| format!("{} {}", v, k))
            .collect();
        rel_parts.sort();
        let rel_summary = if rel_parts.is_empty() {
            String::new()
        } else {
            format!(": {}", rel_parts.join(", "))
        };

        println!(
            "    {}{marker}  ({} edges{rel_summary} — last: {date})",
            ns.name, ns.edge_count
        );
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

/// One-line trust warning when a meaningful share of open edges anchor to
/// code that no longer exists — a store that would otherwise present as
/// pristine while recommending dead findings.
fn print_trust_line(data: &OrientData) {
    if data.open_edge_total == 0 || data.stale_anchored == 0 {
        return;
    }
    let ratio = data.stale_anchored as f64 / data.open_edge_total as f64;
    if ratio > 0.2 {
        println!(
            "  \u{26A0} trust: {}/{} open edges anchor to missing or unresolvable code — review with `damask lint`",
            data.stale_anchored, data.open_edge_total,
        );
    }
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
        let glyph = if e.glyph.is_empty() {
            String::new()
        } else {
            format!("{} ", e.glyph)
        };
        let marks = format!(
            "{}{}",
            if e.endorsements > 0 {
                format!(" \u{00D7}{}\u{2713}", e.endorsements)
            } else {
                String::new()
            },
            if e.disputes > 0 {
                format!(" \u{00D7}{}\u{2717}", e.disputes)
            } else {
                String::new()
            },
        );
        let anchor = e
            .anchor
            .as_deref()
            .map(|a| format!(" @ {}", a))
            .unwrap_or_default();
        let trunc = damask_core::truncate_str(&e.summary, 70);
        println!("    {conf}{glyph}{trunc}{marks}{anchor}");
    }
    if total > SECTION_LIMIT {
        println!("    ... and {} more", total - SECTION_LIMIT);
    }
}

fn summaries_to_json(edges: &[EdgeSummary], limit: usize) -> serde_json::Value {
    let total = edges.len();
    let shown = total.min(limit);
    let items: Vec<serde_json::Value> = edges
        .iter()
        .take(limit)
        .map(|e| {
            serde_json::json!({
                "id": e.id,
                "rel": e.rel,
                "summary": e.summary,
                "confidence": e.confidence,
                "ns": e.ns,
                "ts": e.ts,
                "endorsements": e.endorsements,
                "disputes": e.disputes,
                "anchor": e.anchor,
                "freshness": if e.glyph.is_empty() { None } else { Some(e.glyph) },
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
    let ns_list: Vec<serde_json::Value> = data
        .namespaces
        .iter()
        .map(|ns| {
            let rels: serde_json::Value = ns
                .rels
                .iter()
                .map(|(k, v)| (k.clone(), serde_json::json!(v)))
                .collect::<serde_json::Map<String, serde_json::Value>>()
                .into();
            serde_json::json!({
                "name": ns.name,
                "edge_count": ns.edge_count,
                "last_modified": ns.last_modified,
                "rels": rels,
            })
        })
        .collect();

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
        "trust": {
            "open_edges": data.open_edge_total,
            "stale_anchored": data.stale_anchored,
        },
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
