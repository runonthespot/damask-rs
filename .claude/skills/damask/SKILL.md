---
name: damask
description: Use damask to query, record, and manage structured knowledge about this codebase. Invoke when exploring code, recording findings, checking risks/gotchas, or auditing.
allowed-tools: Bash(damask *)
---

# Damask — Knowledge Fabric for Code

Damask is a structured annotation graph in `.damask/`. Typed, scored observations about code — risks, decisions, dependencies, gotchas — stored as append-only JSONL, queryable via CLI. Spans pin file regions (surviving renames/refactors); edges are typed relationships between them.

If `damask init --claude` installed hooks, the loop runs automatically: a graph briefing is injected at session start (no need to re-run `orient`), relevant edges appear as context when you read/edit annotated files or submit a prompt (`damask peek` — each shown at most once per session), and a Stop hook nudges once if you edited files without recording anything (or if what you recorded fails lint). Record as you go and the nudges never fire. Facts you write are stamped with your agent/session identity automatically.

Make claims mechanically verifiable when possible: add a `check` field (shell command) to the payload and `damask verify --auto` will keep it endorsed/disputed by exit code.

## Workflow

**1. Orient** — always start here:
```bash
damask orient                        # graph stats, top findings, recent activity
damask orient --rel risk             # filter by relationship type
```
Cold start (empty graph)? Seed instantly with `damask bootstrap` (manifests, TODOs, co-change history), then run `damask help cold-start` for the playbook.

**2. Query** — check what's known before working:
```bash
damask at src/auth.rs:50             # edges touching line 50
damask at src/auth.rs                # all edges for file
damask where "rel=risk" "tags~security"  # AND-composed predicates
damask search "authentication"       # full-text search
damask search --sem "<concept>"      # semantic search (uses ck if installed; falls back to keyword)
damask follow <span_id>             # traverse edge graph
```
If ck is installed, join code search with knowledge in one pipe:
`ck --sem "<concept>" --jsonl <dir> | damask enrich`

**3. Record** — preserve findings as you work:
```bash
damask record src/auth.rs 42 67 risk -m "No rate limiting on login" -c 0.9 \
  --action "Add rate limiter" --symbol handle_login
```
`-m` is the summary, `-c` confidence (0.0-1.0). Inline JSON payloads also work for richer fields — run `damask help record` for the full schema.

**4. Signal** — maintain graph quality:
```bash
damask endorse <edge_id>             # confirm
damask dispute <edge_id> '{"summary":"Fixed in PR #42"}'  # contradict
damask close <edge_id> --reason resolved  # mark resolved
```

## Command Reference

| Command | Purpose |
|---------|---------|
| `orient` | One-shot orientation: status + top findings + recent activity |
| `at <loc>` | Edges touching a file or line |
| `where <pred>...` | Filter edges by properties (AND-composed) |
| `search <query>` | Full-text search over payloads |
| `record <file> <start> <end> <rel> <payload>` | Create span + edge atomically |
| `batch --stdin` | Create multiple facts with `$N` back-references |
| `endorse <id>` | Signal confirmation |
| `dispute <id> <payload>` | Signal contradiction |
| `close <id>` | Mark resolved |
| `follow <id> [rel]` | Traverse edge graph |
| `why <id>` | Provenance: who created/endorsed/disputed |
| `ns set <name>` | Switch namespace |
| `lint` | Flag quality issues |
| `help <topic>` | Detailed reference (record, batch, where, rels, patterns, quality, cold-start) |

## Relationship Types

| Rel | When to use | from → to |
|-----|-------------|-----------|
| `risk` | Security, correctness, reliability risks | span → null |
| `gotcha` | Non-obvious pitfalls | span → null |
| `decision` | Architectural choices + rationale | span → null |
| `depends_on` | Runtime/build/init dependencies | span → span |
| `contradicts` | Two code regions that conflict | span → span |
| `describes` | Documentation-like annotations | span → null |
| `co_change` | Files that must change together | span → span |
| `implements` | Code implementing a spec/design | span → span |

Run `damask help rels` for the full table. Custom types are allowed.

## On-Demand Reference

Run `damask help <topic>` before recording or for detailed syntax:
- `damask help record` — payload envelope, confidence scale, examples
- `damask help batch` — `$N` back-references, multi-span examples
- `damask help where` — predicate syntax, operators, lifecycle field
- `damask help rels` — full relationship type table
- `damask help patterns` — advanced audit patterns (loading census, undo archaeology, etc.)
- `damask help quality` — writing high-quality annotations
- `damask help cold-start` — structured first-pass playbook
