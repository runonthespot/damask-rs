//! Optional pairing with ck (https://github.com/BeaconBay/ck), a semantic
//! code search engine. ck answers "where is the code"; damask answers "what
//! do we know about it". The bridge here gives damask semantic retrieval
//! over its own knowledge graph by exporting edge payloads as small text
//! files that ck can embed and search.
//!
//! ck is strictly optional: every caller must degrade gracefully (and
//! helpfully) when `ck` is not on PATH. Nothing in damask's core behavior
//! may depend on it.
//!
//! Export layout (auto-managed, never committed):
//! ```text
//! .damask/knowledge/
//!   .gitignore          # "*" — self-ignoring
//!   .stamp              # mtime watermark vs edges/*.jsonl
//!   <ns>/<edge_id>.md   # one file per active open content edge
//! ```
//! Result mapping is by construction: a ck hit's file basename IS the edge
//! ID. Searches pass --no-ignore because the directory is gitignored.

use damask_core::PayloadEnvelope;
use damask_store::{update_index_with_mode, DamaskProject, IndexMode, IndexQuery};
use std::path::Path;
use std::process::Command;

/// Is the ck binary available on PATH?
pub fn ck_available() -> bool {
    Command::new("ck")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// One-line install hint, used wherever ck would have helped.
pub const CK_HINT: &str =
    "tip: pair damask with ck for semantic knowledge search — `cargo install ck-search`";

/// A semantic hit mapped back to an edge.
pub struct SemanticHit {
    pub edge_id: String,
    pub score: f64,
}

/// Query the knowledge export with ck. Returns None when ck is unavailable
/// or anything fails — callers fall back to FTS.
pub fn semantic_edge_hits(
    project: &DamaskProject,
    query: &str,
    limit: usize,
) -> Option<Vec<SemanticHit>> {
    if !ck_available() {
        return None;
    }
    ensure_knowledge_export(project).ok()?;
    let dir = project.damask_dir.join("knowledge");

    let output = Command::new("ck")
        .arg("--sem")
        .arg(query)
        .arg("--jsonl")
        .arg("--no-ignore")
        .arg(&dir)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let mut hits: Vec<SemanticHit> = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let Some(path) = v.get("path").and_then(|p| p.as_str()) else {
            continue;
        };
        let Some(stem) = Path::new(path).file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if !stem.starts_with("e_") {
            continue;
        }
        let score = v.get("score").and_then(|s| s.as_f64()).unwrap_or(0.0);
        // ck may return several chunks of one file; keep the best.
        match hits.iter_mut().find(|h| h.edge_id == stem) {
            Some(h) => h.score = h.score.max(score),
            None => hits.push(SemanticHit {
                edge_id: stem.to_string(),
                score,
            }),
        }
    }
    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hits.truncate(limit);
    Some(hits)
}

/// Regenerate the knowledge export when any edge log is newer than the
/// stamp. Cheap to call on every semantic search.
pub fn ensure_knowledge_export(project: &DamaskProject) -> std::io::Result<()> {
    let knowledge_dir = project.damask_dir.join("knowledge");
    let edges_dir = project.damask_dir.join("edges");
    let stamp = knowledge_dir.join(".stamp");

    let newest_edge_mtime = std::fs::read_dir(&edges_dir)?
        .flatten()
        .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("jsonl"))
        .filter_map(|e| e.metadata().ok()?.modified().ok())
        .max();
    let stamp_mtime = std::fs::metadata(&stamp).and_then(|m| m.modified()).ok();
    if let (Some(newest), Some(stamped)) = (newest_edge_mtime, stamp_mtime) {
        if stamped >= newest {
            return Ok(());
        }
    }

    // Rebuild from the index so lifecycle filtering (active, open, content
    // rels only) matches what queries would return.
    let db_path = project.damask_dir.join("index.db");
    let conn = update_index_with_mode(&db_path, &edges_dir, IndexMode::ViewsPreferred)
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    let q = IndexQuery::new(&conn);
    let edges = q
        .all_active_open_edges()
        .map_err(|e| std::io::Error::other(e.to_string()))?;

    // Wipe namespace dirs (not the whole dir — ck's own .ck index cache
    // lives at the top level and survives regeneration for cache hits).
    if knowledge_dir.exists() {
        for entry in std::fs::read_dir(&knowledge_dir)?.flatten() {
            if entry.path().is_dir()
                && entry
                    .file_name()
                    .to_str()
                    .is_some_and(|n| !n.starts_with('.'))
            {
                std::fs::remove_dir_all(entry.path())?;
            }
        }
    }
    std::fs::create_dir_all(&knowledge_dir)?;
    std::fs::write(knowledge_dir.join(".gitignore"), "*\n")?;

    for edge in &edges {
        let payload: serde_json::Value =
            serde_json::from_str(&edge.payload).unwrap_or(serde_json::json!({}));
        let env = PayloadEnvelope::new(&payload);
        // Edges with no summary carry nothing worth embedding.
        let Some(summary) = env.summary() else {
            continue;
        };

        let mut doc = format!("# {}: {}\n", edge.rel, summary);
        if let Some(action) = env.action() {
            doc.push_str(&format!("\naction: {action}\n"));
        }
        for field in ["reasoning", "impact", "evidence_note"] {
            if let Some(text) = payload.get(field).and_then(|v| v.as_str()) {
                doc.push_str(&format!("\n{field}: {text}\n"));
            }
        }
        if let Some(tags) = env.tags() {
            doc.push_str(&format!("\ntags: {}\n", tags.join(", ")));
        }
        // Anchor context strengthens embeddings ("auth", "ranking", ...).
        let span_id = edge
            .from_id
            .as_deref()
            .filter(|id| id.starts_with("s_"))
            .or_else(|| edge.to_id.as_deref().filter(|id| id.starts_with("s_")));
        if let Some(span) = span_id.and_then(|id| q.span_by_id(id).ok().flatten()) {
            doc.push_str(&format!("\nlocation: {}", span.path));
            if let Some(symbol) = &span.symbol {
                doc.push_str(&format!(" ({symbol})"));
            }
            doc.push('\n');
        }

        let ns_dir = knowledge_dir.join(&edge.ns);
        std::fs::create_dir_all(&ns_dir)?;
        std::fs::write(ns_dir.join(format!("{}.md", edge.id)), doc)?;
    }

    std::fs::write(&stamp, b"")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use damask_core::{Edge, EdgeId, Fact};
    use damask_store::FactWriter;

    fn write_edge(
        project: &DamaskProject,
        ns: &str,
        rel: &str,
        payload: serde_json::Value,
    ) -> String {
        let edge = Edge {
            id: EdgeId::new(),
            from: None,
            to: None,
            rel: rel.to_string(),
            payload,
            ns: ns.to_string(),
            ts: chrono::Utc::now(),
            agent: None,
            session: None,
        };
        let id = edge.id.to_string();
        FactWriter::append(&project.edges_file(ns), &Fact::Edge(edge)).unwrap();
        id
    }

    #[test]
    fn export_writes_one_file_per_edge_keyed_by_id() {
        let tmp = tempfile::tempdir().unwrap();
        let project = DamaskProject::init(tmp.path()).unwrap();
        let id = write_edge(
            &project,
            "test",
            "risk",
            serde_json::json!({"summary":"token expiry unchecked","confidence":0.9,"tags":["auth"]}),
        );

        ensure_knowledge_export(&project).unwrap();

        let doc_path = project
            .damask_dir
            .join("knowledge")
            .join("test")
            .join(format!("{id}.md"));
        let doc = std::fs::read_to_string(&doc_path).unwrap();
        assert!(doc.contains("# risk: token expiry unchecked"));
        assert!(doc.contains("tags: auth"));
        // Self-ignoring so derived data never lands in git.
        assert_eq!(
            std::fs::read_to_string(project.damask_dir.join("knowledge/.gitignore")).unwrap(),
            "*\n"
        );
    }

    #[test]
    fn export_skips_meta_edges_and_summaryless_edges() {
        let tmp = tempfile::tempdir().unwrap();
        let project = DamaskProject::init(tmp.path()).unwrap();
        write_edge(
            &project,
            "test",
            "risk",
            serde_json::json!({"confidence": 0.5}),
        );
        let target = write_edge(
            &project,
            "test",
            "risk",
            serde_json::json!({"summary":"real finding","confidence":0.9}),
        );
        // Meta-edge endorsing the real finding: never exported.
        let meta = Edge {
            id: EdgeId::new(),
            from: Some(damask_core::DamaskId::parse(&target).unwrap()),
            to: None,
            rel: "endorsed".to_string(),
            payload: serde_json::json!({"summary":"confirmed"}),
            ns: "test".to_string(),
            ts: chrono::Utc::now(),
            agent: None,
            session: None,
        };
        FactWriter::append(&project.edges_file("test"), &Fact::Edge(meta)).unwrap();

        ensure_knowledge_export(&project).unwrap();

        let ns_dir = project.damask_dir.join("knowledge").join("test");
        let count = std::fs::read_dir(&ns_dir).unwrap().flatten().count();
        assert_eq!(count, 1, "only the summarized content edge is exported");
    }

    #[test]
    fn export_is_stamped_and_skips_when_fresh() {
        let tmp = tempfile::tempdir().unwrap();
        let project = DamaskProject::init(tmp.path()).unwrap();
        write_edge(
            &project,
            "test",
            "risk",
            serde_json::json!({"summary":"x","confidence":0.9}),
        );
        ensure_knowledge_export(&project).unwrap();
        let stamp = project.damask_dir.join("knowledge/.stamp");
        let first = std::fs::metadata(&stamp).unwrap().modified().unwrap();

        // No edge changes: second call must not rewrite.
        ensure_knowledge_export(&project).unwrap();
        let second = std::fs::metadata(&stamp).unwrap().modified().unwrap();
        assert_eq!(first, second);
    }
}
