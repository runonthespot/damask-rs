use clap::{Parser, Subcommand};

use crate::output::Format;

#[derive(Parser)]
#[command(
    name = "damask",
    about = "A knowledge fabric for AI agents — structured memory layered over your codebase",
    long_about = "\
Damask is a knowledge fabric that lets AI agents (and humans) attach structured \
observations to code. It works by creating spans (anchored file regions) and edges \
(typed relationships like risks, dependencies, and descriptions) stored as append-only \
JSONL alongside your repo.

Key concepts:
  Span    A pinned region of a file (path + line range + content hash)
  Edge    A typed, scored relationship between spans (risk, depends_on, describes, ...)
  Namespace  An isolated layer of edges (e.g. per-audit, per-agent, per-task)

Typical workflow:
  damask init                          # set up .damask/ in your repo
  damask ns set security-audit         # create/activate a namespace
  damask span src/auth.rs 42 67        # pin a code region
  damask edge <span_id> _ risk '{...}' # attach a finding
  damask at src/auth.rs:50             # what do we know about line 50?
  damask tui                           # browse everything interactively

All output supports --format json for machine consumption."
)]
#[command(version, propagate_version = true)]
pub struct Cli {
    /// Output format.
    #[arg(long, value_enum, default_value_t = Format::Human)]
    pub format: Format,

    /// Override the active namespace.
    #[arg(long)]
    pub ns: Option<String>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Initialize .damask/ in current directory.
    Init,

    /// Set or list namespaces.
    Ns {
        #[command(subcommand)]
        action: NsAction,
    },

    /// Create a span referencing a file region.
    Span {
        /// Root-relative file path.
        file: String,

        /// Start line (1-indexed).
        start: u32,

        /// End line (1-indexed, inclusive).
        end: u32,

        /// Symbol anchor (function name, section heading).
        #[arg(long)]
        symbol: Option<String>,
    },

    /// Create an edge between spans/edges.
    Edge {
        /// Source span or edge ID (use "_" for null).
        from: String,

        /// Target span or edge ID (use "_" for null).
        to: String,

        /// Relationship type (e.g., "risk", "depends_on").
        rel: String,

        /// JSON payload (inline or omit for empty).
        payload: Option<String>,

        /// Read payload from file instead of inline.
        #[arg(short = 'f', long = "file")]
        payload_file: Option<String>,

        /// Read payload from stdin.
        #[arg(long)]
        stdin: bool,
    },

    /// What edges touch this location? (THE product)
    At {
        /// Location: file:line or file.
        location: String,

        /// Show all edges (no limit).
        #[arg(long)]
        all: bool,

        /// Skip ranking, sort chronologically.
        #[arg(long)]
        no_rank: bool,
    },

    /// Filter edges by properties.
    Where {
        /// Predicate: field=value, field>value, etc.
        predicate: String,

        /// Only show edges created since this date (YYYY-MM-DD).
        #[arg(long)]
        since: Option<String>,

        /// Maximum results to display.
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },

    /// Traverse edges from a span or edge.
    Follow {
        /// Starting span or edge ID.
        id: String,

        /// Relationship type filter.
        rel: Option<String>,

        /// Maximum traversal depth.
        #[arg(long, default_value_t = 3)]
        depth: u32,
    },

    /// Signal that your work confirmed this edge.
    Endorse {
        /// Edge ID to endorse.
        edge_id: String,

        /// Optional payload.
        payload: Option<String>,
    },

    /// Signal that your work contradicts this edge (payload required).
    Dispute {
        /// Edge ID to dispute.
        edge_id: String,

        /// Dispute payload (required).
        payload: String,
    },

    /// Damask health: counts, staleness, freshness.
    Status,

    /// Flag low-value edges, staleness, quality issues.
    Lint,

    /// Produce current-state view, archive old edges.
    Compact {
        /// Namespace to compact (or all).
        namespace: Option<String>,

        /// Archive unresolved/unendorsed/low-confidence edges.
        #[arg(long)]
        aggressive: bool,
    },

    /// Provenance story: who created, endorsed, disputed, superseded.
    Why {
        /// Edge ID.
        edge_id: String,
    },

    /// Git-blame-style history of an edge/span's evolution.
    Blame {
        /// Span or edge ID.
        id: String,
    },

    /// Materialize the content a span references.
    Resolve {
        /// Span ID.
        span_id: String,
    },

    /// Show fact log, optionally filtered.
    Log,

    /// Show new edges since last commit, ranked and grouped.
    Review,

    /// Full-text search over edge payloads.
    Search {
        /// Search query.
        query: String,

        /// Filter by namespace.
        #[arg(long)]
        ns: Option<String>,

        /// Filter by relation type.
        #[arg(long)]
        rel: Option<String>,
    },

    /// Compare two namespaces.
    Diff {
        /// First namespace.
        ns_a: String,

        /// Second namespace.
        ns_b: String,
    },

    /// Interactive terminal UI.
    Tui,
}

#[derive(Subcommand)]
pub enum NsAction {
    /// Set the active namespace.
    Set {
        /// Namespace name.
        name: String,
    },

    /// List all namespaces.
    List {
        /// Show only stale namespaces.
        #[arg(long)]
        stale: bool,
    },

    /// Merge one namespace into another.
    Merge {
        /// Source namespace.
        source: String,
        /// Target namespace.
        target: String,
    },
}
