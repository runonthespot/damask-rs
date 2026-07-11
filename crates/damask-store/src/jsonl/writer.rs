use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use damask_core::Fact;

use crate::StoreError;

/// Append-only writer for `.jsonl` fact files.
pub struct FactWriter;

impl FactWriter {
    /// Append a single fact to the given JSONL file.
    /// Creates the file (and parent directories) if they don't exist.
    ///
    /// The serialized line (JSON + trailing newline) is issued as a single
    /// `write_all` on the O_APPEND fd so concurrent writers never tear lines.
    pub fn append(path: &Path, fact: &Fact) -> Result<(), StoreError> {
        Self::append_all(path, std::slice::from_ref(fact))
    }

    /// Append multiple facts to the given JSONL file.
    ///
    /// The whole batch is serialized into one newline-terminated buffer and
    /// issued as a single `write_all`, so a batch from one process lands
    /// contiguously even while other processes append concurrently.
    pub fn append_all(path: &Path, facts: &[Fact]) -> Result<(), StoreError> {
        if facts.is_empty() {
            return Ok(());
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| StoreError::Io(e.to_string()))?;
        }

        let mut buf = String::new();
        for fact in facts {
            let json = serde_json::to_string(fact).map_err(StoreError::Json)?;
            buf.push_str(&json);
            buf.push('\n');
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| StoreError::Io(e.to_string()))?;

        file.write_all(buf.as_bytes())
            .map_err(|e| StoreError::Io(e.to_string()))?;

        Ok(())
    }

    /// Write facts to a file, replacing any existing content.
    /// Use this instead of `append_all` when rewriting a file (e.g. compact).
    ///
    /// Write-then-rename: a concurrent reader (or the indexer) must never
    /// observe a half-written file, and truncate-in-place exposes exactly
    /// that window. The temp name carries the pid so concurrent rewriters
    /// never collide; rename is atomic on POSIX.
    pub fn write_all(path: &Path, facts: &[Fact]) -> Result<(), StoreError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| StoreError::Io(e.to_string()))?;
        }

        let mut buf = String::new();
        for fact in facts {
            let json = serde_json::to_string(fact).map_err(StoreError::Json)?;
            buf.push_str(&json);
            buf.push('\n');
        }

        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "facts.jsonl".to_string());
        let tmp = path.with_file_name(format!(".{name}.tmp.{}", std::process::id()));

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&tmp)
            .map_err(|e| StoreError::Io(e.to_string()))?;
        file.write_all(buf.as_bytes())
            .map_err(|e| StoreError::Io(e.to_string()))?;
        file.sync_all().map_err(|e| StoreError::Io(e.to_string()))?;
        drop(file);

        fs::rename(&tmp, path).map_err(|e| {
            let _ = fs::remove_file(&tmp);
            StoreError::Io(e.to_string())
        })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FactReader;
    use damask_core::{Edge, EdgeId, Fact};

    fn make_edge(summary: &str) -> Fact {
        Fact::Edge(Edge {
            id: EdgeId::new(),
            from: None,
            to: None,
            rel: "risk".to_string(),
            payload: serde_json::json!({"summary": summary, "confidence": 0.9}),
            ns: "test".to_string(),
            ts: chrono::Utc::now(),
            agent: None,
            session: None,
        })
    }

    /// Concurrent appenders must never tear lines: every fact written must
    /// survive as exactly one valid JSONL line. Before the single-write_all
    /// fix, the JSON and its newline were separate write() syscalls and
    /// parallel writers lost ~25% of facts to interleaved '}{'-style tears.
    #[test]
    fn concurrent_appends_never_tear_lines() {
        const WRITERS: usize = 8;
        const FACTS_PER_WRITER: usize = 50;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("concurrent.jsonl");

        let handles: Vec<_> = (0..WRITERS)
            .map(|w| {
                let path = path.clone();
                std::thread::spawn(move || {
                    for i in 0..FACTS_PER_WRITER {
                        let fact = make_edge(&format!("writer {} fact {}", w, i));
                        FactWriter::append(&path, &fact).unwrap();
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }

        // Every physical line must be intact — not merely "parseable lines
        // survive": torn lines would be silently skipped by the reader.
        let raw = fs::read_to_string(&path).unwrap();
        let lines: Vec<_> = raw.lines().collect();
        assert_eq!(
            lines.len(),
            WRITERS * FACTS_PER_WRITER,
            "torn or lost lines detected"
        );
        for line in &lines {
            serde_json::from_str::<Fact>(line).expect("every line must parse");
        }

        let mut reader = FactReader::open(&path).unwrap();
        let facts = reader.read_all().unwrap();
        assert_eq!(facts.len(), WRITERS * FACTS_PER_WRITER);
    }

    /// A multi-fact batch is one write: no other writer's line can land
    /// between two facts of the same batch.
    #[test]
    fn batch_appends_land_contiguously() {
        const BATCHES: usize = 6;
        const BATCH_SIZE: usize = 20;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("batches.jsonl");

        let handles: Vec<_> = (0..BATCHES)
            .map(|b| {
                let path = path.clone();
                std::thread::spawn(move || {
                    let facts: Vec<_> = (0..BATCH_SIZE)
                        .map(|i| make_edge(&format!("batch {} item {}", b, i)))
                        .collect();
                    FactWriter::append_all(&path, &facts).unwrap();
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }

        let raw = fs::read_to_string(&path).unwrap();
        let batch_of = |line: &str| -> usize {
            let fact: Fact = serde_json::from_str(line).expect("line must parse");
            match fact {
                Fact::Edge(e) => {
                    let s = e.payload["summary"].as_str().unwrap().to_string();
                    s.split_whitespace().nth(1).unwrap().parse().unwrap()
                }
                _ => panic!("expected edge"),
            }
        };

        let batch_ids: Vec<usize> = raw.lines().map(batch_of).collect();
        assert_eq!(batch_ids.len(), BATCHES * BATCH_SIZE);
        // Each batch's lines must be contiguous in the file.
        let mut seen = std::collections::HashSet::new();
        let mut prev = usize::MAX;
        for id in batch_ids {
            if id != prev {
                assert!(seen.insert(id), "batch {} interleaved with another", id);
                prev = id;
            }
        }
    }

    /// write_all replaces via temp-file + rename: the target is complete at
    /// every instant a reader could open it, and no temp files leak.
    #[test]
    fn write_all_replaces_atomically_and_cleans_up() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("view.jsonl");
        FactWriter::write_all(&path, &[make_edge("first")]).unwrap();
        FactWriter::write_all(&path, &[make_edge("second"), make_edge("third")]).unwrap();

        let raw = fs::read_to_string(&path).unwrap();
        assert_eq!(raw.lines().count(), 2);
        assert!(raw.contains("second"));
        assert!(!raw.contains("first"));

        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".tmp."))
            .collect();
        assert!(leftovers.is_empty(), "temp files leaked: {leftovers:?}");
    }

    #[test]
    fn append_all_empty_is_noop() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("noop.jsonl");
        FactWriter::append_all(&path, &[]).unwrap();
        assert!(!path.exists());
    }
}
