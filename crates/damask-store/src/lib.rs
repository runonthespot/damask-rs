//! JSONL storage, SQLite index, and query engine for Damask.
//!
//! This crate handles all persistence: reading/writing JSONL fact files,
//! managing the `.damask/` project directory, and the SQLite index.

pub mod decay;
pub mod index;
pub mod jsonl;
pub mod lint;
pub mod predicate;
pub mod project;
pub mod ranking;
pub mod state;

pub use index::query::{GraphStats, NamespaceStats, NodeKind, ProjectStats, SpanRow, TraversalChild, TraversalNode};
pub use index::{
    rebuild_index,
    rebuild_index_with_mode,
    update_index,
    update_index_with_mode,
    IndexMode,
    IndexQuery,
};
pub use jsonl::{FactReader, FactWriter};
pub use lint::{lint_edges, token_overlap_ratio, LintInput, LintIssue, Severity};
pub use predicate::{needs_inactive_edges, Predicate};
pub use project::DamaskProject;
pub use ranking::{rank_edges, RankedEdge, RankingInput};

/// Errors produced by store operations.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("I/O error: {0}")]
    Io(String),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("already initialized: {0}")]
    AlreadyInitialized(String),

    #[error("no .damask/ directory found")]
    NotFound,
}
