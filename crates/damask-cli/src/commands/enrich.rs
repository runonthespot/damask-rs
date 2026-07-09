//! Join search results against the knowledge graph.
//!
//! Reads JSONL from stdin — one search result per line — and annotates each
//! with the ranked open edges whose spans overlap it. Designed as the
//! receiving end of a ck pipe:
//!
//! ```bash
//! ck --sem "authentication" --jsonl src/ | damask enrich
//! ```
//!
//! but deliberately liberal about input shape so any tool emitting
//! `{path|file, span{line_start,line_end} | line_number | line}` objects
//! works (ck is optional — damask never requires it). Lines that aren't
//! parseable results pass through untouched in JSON mode and are skipped in
//! human mode.

use anyhow::Context;
use damask_core::PayloadEnvelope;
use damask_store::{update_index_with_mode, DamaskProject, IndexMode, IndexQuery, RankedEdge};
use std::env;
use std::io::BufRead;

use crate::error::Result;
use crate::output::Format;

use super::helpers::{project_relative, ranked_edges_for_file};

/// Maximum edges attached per result.
const EDGES_PER_RESULT: usize = 5;

struct ParsedResult {
    path: String,
    range: Option<(u32, u32)>,
}

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
    let config = project
        .read_config()
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let stdin = std::io::stdin();
    let mut saw_input = false;

    for line in stdin.lock().lines() {
        let line = line.context("failed to read stdin")?;
        if line.trim().is_empty() {
            continue;
        }
        saw_input = true;

        let value: Option<serde_json::Value> = serde_json::from_str(&line).ok();
        let parsed = value.as_ref().and_then(parse_result);

        let edges: Vec<RankedEdge> = match &parsed {
            Some(r) => match project_relative(&r.path, &project.root) {
                Some(rel) => ranked_edges_for_file(&q, &config, &rel, r.range, EDGES_PER_RESULT),
                None => Vec::new(),
            },
            None => Vec::new(),
        };

        match format {
            Format::Json => {
                // Augmented passthrough: the original object plus a
                // "damask" key. Unparseable lines pass through untouched.
                match value {
                    Some(mut v) => {
                        if let Some(obj) = v.as_object_mut() {
                            obj.insert("damask".to_string(), edges_json(&edges));
                        }
                        println!("{}", serde_json::to_string(&v).unwrap());
                    }
                    None => println!("{line}"),
                }
            }
            Format::Human => {
                let Some(r) = parsed else { continue };
                let loc = match r.range {
                    Some((s, e)) if s == e => format!("{}:{s}", r.path),
                    Some((s, e)) => format!("{}:{s}-{e}", r.path),
                    None => r.path.clone(),
                };
                println!("\n{loc}");
                if edges.is_empty() {
                    println!("  (no damask annotations)");
                    continue;
                }
                for re in &edges {
                    let payload: serde_json::Value =
                        serde_json::from_str(&re.edge.payload).unwrap_or(serde_json::json!({}));
                    let envp = PayloadEnvelope::new(&payload);
                    let conf = envp
                        .confidence()
                        .map(|c| format!(" ({c:.2})"))
                        .unwrap_or_default();
                    let summary = envp
                        .summary()
                        .unwrap_or_else(|| damask_core::truncate_str(&re.edge.payload, 80));
                    println!("  [{}]{conf} {} — {}", re.edge.rel, re.edge.id, summary);
                }
            }
        }
    }

    if !saw_input {
        eprintln!("damask enrich reads search results (JSONL) on stdin, e.g.:");
        eprintln!("  ck --sem \"auth\" --jsonl src/ | damask enrich");
        if !crate::ck::ck_available() {
            eprintln!("{}", crate::ck::CK_HINT);
        }
    }

    Ok(())
}

/// Accept the shapes emitted by ck (`--jsonl` CLI and MCP variants) and
/// reasonable lookalikes from other tools.
fn parse_result(v: &serde_json::Value) -> Option<ParsedResult> {
    let path = v
        .get("path")
        .and_then(|p| p.as_str())
        .or_else(|| v.pointer("/file/path").and_then(|p| p.as_str()))
        .or_else(|| v.get("file").and_then(|p| p.as_str()))?
        .to_string();

    let span = v
        .get("span")
        .or_else(|| v.pointer("/match/span"))
        .and_then(|s| {
            let start = s.get("line_start")?.as_u64()? as u32;
            let end = s
                .get("line_end")
                .and_then(|e| e.as_u64())
                .unwrap_or(start as u64) as u32;
            Some((start, end))
        });
    let range = span.or_else(|| {
        let line = v
            .get("line_number")
            .or_else(|| v.get("line"))
            .and_then(|l| l.as_u64())? as u32;
        Some((line, line))
    });

    Some(ParsedResult { path, range })
}

fn edges_json(edges: &[RankedEdge]) -> serde_json::Value {
    let items: Vec<serde_json::Value> = edges
        .iter()
        .map(|re| {
            let payload: serde_json::Value =
                serde_json::from_str(&re.edge.payload).unwrap_or(serde_json::json!({}));
            serde_json::json!({
                "id": re.edge.id,
                "rel": re.edge.rel,
                "payload": payload,
                "ns": re.edge.ns,
                "score": re.score,
                "endorsements": re.endorsement_count,
                "disputes": re.dispute_count,
            })
        })
        .collect();
    serde_json::json!({ "edges": items })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ck_cli_jsonl_shape() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"path":"/x/src/a.rs","span":{"byte_start":1,"byte_end":2,"line_start":33,"line_end":71},"score":0.74}"#,
        )
        .unwrap();
        let r = parse_result(&v).unwrap();
        assert_eq!(r.path, "/x/src/a.rs");
        assert_eq!(r.range, Some((33, 71)));
    }

    #[test]
    fn parses_ck_mcp_shape() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"file":{"path":"/x/a.rs"},"match":{"line_number":5,"span":{"line_start":5,"line_end":9}}}"#,
        )
        .unwrap();
        let r = parse_result(&v).unwrap();
        assert_eq!(r.path, "/x/a.rs");
        assert_eq!(r.range, Some((5, 9)));
    }

    #[test]
    fn parses_grep_like_shape() {
        let v: serde_json::Value =
            serde_json::from_str(r#"{"file":"src/a.rs","line":12}"#).unwrap();
        let r = parse_result(&v).unwrap();
        assert_eq!(r.path, "src/a.rs");
        assert_eq!(r.range, Some((12, 12)));
    }

    #[test]
    fn rejects_objects_without_a_path() {
        let v: serde_json::Value = serde_json::from_str(r#"{"score":0.9}"#).unwrap();
        assert!(parse_result(&v).is_none());
    }
}
