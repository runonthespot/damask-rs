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
    pub fn append(path: &Path, fact: &Fact) -> Result<(), StoreError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| StoreError::Io(e.to_string()))?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| StoreError::Io(e.to_string()))?;

        let json = serde_json::to_string(fact).map_err(StoreError::Json)?;
        writeln!(file, "{}", json).map_err(|e| StoreError::Io(e.to_string()))?;

        Ok(())
    }

    /// Append multiple facts to the given JSONL file.
    pub fn append_all(path: &Path, facts: &[Fact]) -> Result<(), StoreError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| StoreError::Io(e.to_string()))?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| StoreError::Io(e.to_string()))?;

        for fact in facts {
            let json = serde_json::to_string(fact).map_err(StoreError::Json)?;
            writeln!(file, "{}", json).map_err(|e| StoreError::Io(e.to_string()))?;
        }

        Ok(())
    }

    /// Write facts to a file, truncating any existing content.
    /// Use this instead of `append_all` when rewriting a file (e.g. compact).
    pub fn write_all(path: &Path, facts: &[Fact]) -> Result<(), StoreError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| StoreError::Io(e.to_string()))?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)
            .map_err(|e| StoreError::Io(e.to_string()))?;

        for fact in facts {
            let json = serde_json::to_string(fact).map_err(StoreError::Json)?;
            writeln!(file, "{}", json).map_err(|e| StoreError::Io(e.to_string()))?;
        }

        Ok(())
    }
}
