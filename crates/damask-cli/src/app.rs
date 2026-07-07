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
  damask record src/auth.rs 42 67 risk '{...}'  # pin + annotate in one shot
  damask at src/auth.rs:50             # what do we know about line 50?
  damask tui                           # browse everything interactively

For bulk operations, use batch to create multiple facts atomically:
  damask batch --stdin < facts.json    # spans + edges with $N back-references

All output supports --format json for machine consumption."
)]
#[command(version, propagate_version = true, disable_help_subcommand = true)]
pub struct Cli {
    /// Output format.
    #[arg(long, global = true, value_enum, default_value_t = Format::Human)]
    pub format: Format,

    /// Override the active namespace.
    #[arg(long, global = true)]
    pub ns: Option<String>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Initialize .damask/ in current directory (auto-detects AI agents).
    Init {
        /// Scaffold Claude Code integration (.claude/skills/damask/).
        #[arg(long)]
        claude: bool,
        /// Scaffold OpenAI Codex CLI integration (.agents/skills/damask/).
        #[arg(long)]
        codex: bool,
        /// Skip automatic agent detection.
        #[arg(long)]
        no_agents: bool,
    },

    /// Show detailed reference for a topic (record, batch, where, rels, patterns, quality, cold-start).
    Help {
        /// Topic name (omit to list available topics).
        topic: Option<String>,
    },

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

    /// Create a span and edge in one shot.
    Record {
        /// Root-relative file path.
        file: String,

        /// Start line (1-indexed).
        start: u32,

        /// End line (1-indexed, inclusive).
        end: u32,

        /// Relationship type (e.g., "risk", "depends_on").
        rel: String,

        /// JSON payload (inline).
        payload: String,

        /// Symbol anchor (function name, section heading).
        #[arg(long)]
        symbol: Option<String>,

        /// Target span or edge ID (default: null).
        #[arg(long, default_value = "_")]
        to: String,
    },

    /// Create multiple facts atomically from a JSON batch (stdin or file).
    Batch {
        /// Read batch from stdin.
        #[arg(long, conflicts_with = "file")]
        stdin: bool,

        /// Read batch from a JSON file.
        #[arg(short = 'f', long = "file", conflicts_with = "stdin")]
        file: Option<String>,
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

        /// Filter by relationship type.
        #[arg(long)]
        rel: Option<String>,

        /// Filter by exact tag match.
        #[arg(long)]
        tag: Option<String>,

        /// Show only edges with 0 disputes.
        #[arg(long)]
        uncontested: bool,

        /// Include closed edges in results.
        #[arg(long)]
        show_closed: bool,

        /// Skip this many results before displaying.
        #[arg(long, default_value_t = 0)]
        offset: usize,
    },

    /// Filter edges by properties (multiple predicates are AND-composed).
    Where {
        /// Predicates: field=value, field>value, etc. Multiple predicates are AND-composed.
        #[arg(required = true)]
        predicates: Vec<String>,

        /// Only show edges created since this date (YYYY-MM-DD).
        #[arg(long)]
        since: Option<String>,

        /// Maximum results to display.
        #[arg(long, default_value_t = 50)]
        limit: usize,

        /// Skip this many results before displaying.
        #[arg(long, default_value_t = 0)]
        offset: usize,

        /// Include closed edges in results.
        #[arg(long)]
        show_closed: bool,

        /// Result order: rank (freshness- and trust-weighted, default) or ts (newest first).
        #[arg(long, value_enum, default_value_t = crate::commands::where_cmd::WhereSort::Rank)]
        sort: crate::commands::where_cmd::WhereSort,
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
        /// Edge ID to endorse (required unless --batch).
        edge_id: Option<String>,

        /// Optional payload.
        payload: Option<String>,

        /// Batch mode: read edge IDs from stdin, one per line.
        #[arg(long)]
        batch: bool,
    },

    /// Signal that your work contradicts this edge (payload required).
    Dispute {
        /// Edge ID to dispute (required unless --batch).
        edge_id: Option<String>,

        /// Dispute payload (required unless --reason).
        payload: Option<String>,

        /// Use a reason template instead of raw JSON payload.
        #[arg(long, value_parser = ["resolved", "outdated", "incorrect", "duplicate"])]
        reason: Option<String>,

        /// Batch mode: read edge IDs from stdin, one per line.
        #[arg(long)]
        batch: bool,
    },

    /// Mark an edge as closed (resolved). Creates a rel=closed meta-edge.
    Close {
        /// Edge ID to close (required unless --batch).
        edge_id: Option<String>,

        /// Optional JSON payload.
        payload: Option<String>,

        /// Use a reason template instead of raw JSON payload.
        #[arg(long, value_parser = ["resolved", "outdated", "incorrect", "duplicate", "accepted"])]
        reason: Option<String>,

        /// Batch mode: read edge IDs from stdin, one per line.
        #[arg(long)]
        batch: bool,
    },

    /// One-shot orientation: status, risks, gotchas, decisions, recent activity.
    Orient {
        /// Filter by relationship type.
        #[arg(long)]
        rel: Option<String>,

        /// Filter by exact tag match.
        #[arg(long)]
        tag: Option<String>,

        /// Show only edges with 0 disputes.
        #[arg(long)]
        uncontested: bool,

        /// Include closed edges in results.
        #[arg(long)]
        show_closed: bool,
    },

    /// Compact warm-start briefing (designed for Claude Code SessionStart hook).
    ///
    /// Prints a token-capped markdown digest of the knowledge graph for
    /// injection into an agent's context. Silent (exit 0) when no .damask/
    /// is found — safe to run unconditionally as a hook.
    Briefing,

    /// Session-end harvest check (designed for Claude Code Stop hook).
    ///
    /// Reads the Stop hook JSON from stdin, scans the session transcript,
    /// and if files were edited but nothing was recorded in damask, asks the
    /// agent (once) to preserve durable findings before finishing. Fails
    /// open: any error allows the stop.
    Harvest {
        /// Path to a transcript JSONL (testing; default: read hook JSON from stdin).
        #[arg(long)]
        transcript: Option<String>,
    },

    /// Point-of-use context hook (designed for Claude Code PostToolUse + UserPromptSubmit).
    ///
    /// Reads hook JSON from stdin and injects the top-ranked edges for the
    /// file just touched (PostToolUse on Read/Edit/Write) or for the prompt's
    /// keywords (UserPromptSubmit). A per-session seen-cache ensures each
    /// edge is injected at most once. Fails open on any error.
    Peek {
        /// Inject context for a file (testing; default: read hook JSON from stdin).
        #[arg(long, conflicts_with = "prompt")]
        file: Option<String>,

        /// Inject context for a prompt (testing).
        #[arg(long)]
        prompt: Option<String>,

        /// Session ID for the seen-cache (testing).
        #[arg(long)]
        session: Option<String>,
    },

    /// Damask health: counts, staleness, freshness.
    Status,

    /// Flag low-value edges, staleness, quality issues.
    Lint,

    /// Run `check` commands from edge payloads; report pass/fail.
    ///
    /// An edge payload may carry a `check` field — a shell command whose
    /// exit code revalidates the claim. Exits non-zero if any check fails.
    Verify {
        /// Record outcomes: endorse passing edges, dispute failing ones.
        #[arg(long)]
        auto: bool,

        /// Per-check timeout in seconds.
        #[arg(long, default_value_t = 30)]
        timeout: u64,
    },

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
    Review {
        /// PR-comment-ready markdown output (for CI).
        #[arg(long)]
        markdown: bool,
    },

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

        /// Predicate filters applied to matches (repeatable, AND-composed).
        #[arg(long = "where", value_name = "PRED")]
        where_preds: Vec<String>,

        /// Semantic search via ck (falls back to keyword search if ck is absent).
        #[arg(long)]
        sem: bool,

        /// Maximum results to display.
        #[arg(long, default_value_t = 50)]
        limit: usize,

        /// Skip this many results before displaying.
        #[arg(long, default_value_t = 0)]
        offset: usize,

        /// Include closed edges in results.
        #[arg(long)]
        show_closed: bool,
    },

    /// Annotate piped search results (JSONL) with known edges.
    ///
    /// Reads one JSON result per line on stdin (ck --jsonl output or any
    /// {path, span|line} object) and joins each against the graph:
    /// `ck --sem "auth" --jsonl src/ | damask enrich`
    Enrich,

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
