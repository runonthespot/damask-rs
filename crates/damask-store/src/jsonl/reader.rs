use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use damask_core::Fact;

use crate::StoreError;

/// Streaming reader for `.jsonl` fact files.
/// Corrupt or unparseable lines are skipped with a warning to stderr.
pub struct FactReader {
    reader: BufReader<File>,
    path: String,
    line_num: usize,
}

impl FactReader {
    /// Open a JSONL file for reading.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let file = File::open(path).map_err(|e| StoreError::Io(e.to_string()))?;
        Ok(Self {
            reader: BufReader::new(file),
            path: path.display().to_string(),
            line_num: 0,
        })
    }

    /// Read all facts from the file, skipping corrupt lines.
    pub fn read_all(&mut self) -> Result<Vec<Fact>, StoreError> {
        let mut facts = Vec::new();
        let mut line = String::new();

        loop {
            line.clear();
            let bytes_read = self
                .reader
                .read_line(&mut line)
                .map_err(|e| StoreError::Io(e.to_string()))?;
            if bytes_read == 0 {
                break;
            }
            self.line_num += 1;

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            match serde_json::from_str::<Fact>(trimmed) {
                Ok(fact) => facts.push(fact),
                Err(e) => {
                    eprintln!(
                        "warning: {}:{}: skipping corrupt line: {}",
                        self.path, self.line_num, e
                    );
                }
            }
        }

        Ok(facts)
    }
}

impl Iterator for FactReader {
    type Item = Fact;

    fn next(&mut self) -> Option<Self::Item> {
        let mut line = String::new();
        loop {
            line.clear();
            match self.reader.read_line(&mut line) {
                Ok(0) => return None,
                Ok(_) => {
                    self.line_num += 1;
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    match serde_json::from_str::<Fact>(trimmed) {
                        Ok(fact) => return Some(fact),
                        Err(e) => {
                            eprintln!(
                                "warning: {}:{}: skipping corrupt line: {}",
                                self.path, self.line_num, e
                            );
                            continue;
                        }
                    }
                }
                Err(e) => {
                    eprintln!(
                        "warning: {}:{}: read error: {}",
                        self.path, self.line_num, e
                    );
                    return None;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FactWriter;
    use damask_core::{Edge, EdgeId, Fact, Span, SpanId};
    use std::io::Write;

    fn make_span(ns: &str) -> Fact {
        Fact::Span(Span {
            id: SpanId::new(),
            path: "src/main.rs".to_string(),
            lines: Some([1, 10]),
            snippet: None,
            symbol: None,
            content_hash: None,
            commit: None,
            ns: ns.to_string(),
            ts: chrono::Utc::now(),
            agent: None,
            session: None,
        })
    }

    fn make_edge(ns: &str) -> Fact {
        Fact::Edge(Edge {
            id: EdgeId::new(),
            from: None,
            to: None,
            rel: "risk".to_string(),
            payload: serde_json::json!({"summary": "test"}),
            ns: ns.to_string(),
            ts: chrono::Utc::now(),
            agent: None,
            session: None,
        })
    }

    #[test]
    fn write_and_read_facts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.jsonl");

        let span = make_span("ns1");
        let edge = make_edge("ns1");
        FactWriter::append(&path, &span).unwrap();
        FactWriter::append(&path, &edge).unwrap();

        let mut reader = FactReader::open(&path).unwrap();
        let facts = reader.read_all().unwrap();
        assert_eq!(facts.len(), 2);
        assert!(matches!(facts[0], Fact::Span(_)));
        assert!(matches!(facts[1], Fact::Edge(_)));
    }

    #[test]
    fn skips_corrupt_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.jsonl");

        let span = make_span("ns1");
        FactWriter::append(&path, &span).unwrap();

        // Write a corrupt line directly
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        writeln!(file, "{{this is not valid json").unwrap();

        let edge = make_edge("ns1");
        FactWriter::append(&path, &edge).unwrap();

        let mut reader = FactReader::open(&path).unwrap();
        let facts = reader.read_all().unwrap();
        assert_eq!(facts.len(), 2); // corrupt line skipped
    }

    #[test]
    fn iterator_interface() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.jsonl");

        let facts = vec![make_span("ns1"), make_edge("ns1"), make_span("ns1")];
        FactWriter::append_all(&path, &facts).unwrap();

        let reader = FactReader::open(&path).unwrap();
        let collected: Vec<_> = reader.collect();
        assert_eq!(collected.len(), 3);
    }

    #[test]
    fn empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.jsonl");
        std::fs::write(&path, "").unwrap();

        let mut reader = FactReader::open(&path).unwrap();
        let facts = reader.read_all().unwrap();
        assert!(facts.is_empty());
    }
}
