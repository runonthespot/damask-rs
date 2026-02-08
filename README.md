# Damask

A knowledge fabric for AI agents — structured memory layered over your codebase.

Damask lets AI agents (and humans) attach structured observations to code and query them back. Think of it as a typed annotation layer that lives alongside your repo in `.damask/`.

## Core concepts

| Concept | What it is |
|---|---|
| **Span** | A pinned region of a source file (path + line range + content hash) |
| **Edge** | A typed, scored relationship between spans — `risk`, `depends_on`, `describes`, `hypothesis`, etc. |
| **Namespace** | An isolated layer of edges (per-audit, per-agent, per-task) |
| **Payload** | JSON metadata on an edge: `summary`, `confidence`, `action`, tags, etc. |

Everything is stored as **append-only JSONL** in `.damask/edges/`, with a SQLite index built on-the-fly for queries. Edges can be endorsed, disputed, or superseded — stale observations decay naturally.

## Install

```bash
cargo install --path crates/damask-cli
```

Requires Rust 1.75+.

## Quick start

```bash
# Initialize in your repo
damask init

# Create a namespace for your work
damask ns set security-audit

# Pin a code region
damask span src/auth.rs 42 67 --symbol validate_token

# Attach a finding
damask edge <span_id> _ risk '{"summary":"No token expiry check","confidence":0.95,"action":"Add expiry validation"}'

# Query: what do we know about line 50?
damask at src/auth.rs:50

# Filter: show all high-confidence risks
damask where "rel=risk"
damask where "confidence>0.8"

# Traverse the graph from a span
damask follow <span_id>

# Browse everything interactively
damask tui
```

## Commands

```
damask init         Initialize .damask/ in current directory
damask ns           Set, list, or merge namespaces
damask span         Create a span referencing a file region
damask edge         Create an edge between spans/edges
damask at           What edges touch this location?
damask where        Filter edges by properties (rel, confidence, tags, ...)
damask follow       Traverse edges from a span or edge
damask endorse      Confirm an edge (adds endorsement meta-edge)
damask dispute      Contradict an edge (adds dispute meta-edge)
damask why          Provenance: who created, endorsed, disputed, superseded
damask blame        Git-blame-style history of a span or edge
damask resolve      Materialize the content a span references
damask log          Chronological fact log
damask review       New edges since last commit, ranked and grouped
damask compact      Remove inactive edges, shrink JSONL files
damask status       Project health: counts, staleness, freshness
damask lint         Flag low-value edges and quality issues
damask tui          Interactive terminal UI
```

All commands support `--format json` for machine consumption and `--ns <name>` to override the active namespace.

## Architecture

Five crates, layered with no circular dependencies:

```
damask-cli          Binary + clap commands
damask-tui          ratatui terminal UI
damask-store        JSONL I/O, SQLite index, ranking, predicates
damask-resolve      Content hashing + freshness detection
damask-core         Pure types (Span, Edge, Fact, IDs) — no I/O
```

## Storage format

```
.damask/
  config.json           # project config (half_life_days, freshness weights)
  edges/
    <namespace>.jsonl   # append-only facts (spans + edges)
  index.db              # auto-built SQLite index (gitignored)
```

Each line in a JSONL file is a tagged JSON object:

```json
{"t":"span","id":"s_01JKX...","path":"src/auth.rs","lines":[42,67],"ns":"security-audit","ts":"2025-01-15T10:30:00Z"}
{"t":"edge","id":"e_01JKX...","from":"s_01JKX...","to":null,"rel":"risk","payload":{"summary":"No token expiry","confidence":0.95},"ns":"security-audit","ts":"2025-01-15T10:30:02Z"}
```

## License

MIT
