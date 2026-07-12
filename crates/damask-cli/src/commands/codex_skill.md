---
name: damask
description: This repo has a knowledge graph (.damask/) of verified findings from previous sessions. Invoke BEFORE exploring, refactoring, or debugging unfamiliar code (inherit what past agents learned instead of re-deriving it); when about to record, note, or remember a finding, risk, gotcha, or decision; after fixing a known issue (close its edge); when auditing or reviewing; or on any mention of damask, annotations, knowledge graph, "what do we know about", "why is this like this", or "record this".
---

# Damask — Knowledge Fabric for Code

This repo carries a knowledge graph in `.damask/`: risks, gotchas, and decisions pinned to exact code regions by the agents and humans who worked here before you. Anchors follow the code through edits and renames; claims carry provenance; wrong claims get disputed and sink; resolved ones get closed and vanish. What you query is what's still true — or honestly marked when it might not be.

The deal favours you twice: every verified fact you inherit is exploration you don't repeat, and everything you record outlives your context window — your next session, another agent, a cheaper model all start knowing it. **Record judgment, not description**: what surprised you, what broke, what failed, why a decision went this way — a future agent can re-read the code, but not re-learn what it cost you. And garden as you read: `endorse` what you confirmed, `dispute` what's wrong (including your own earlier claims — that's the system working), `close` what's done. The graph stays worth reading only because agents like you signal.

## Workflow

**1. Orient** — always start here:
```bash
damask orient                        # graph stats, top findings, recent activity
damask orient --rel risk             # filter by relationship type
```
Cold start (empty graph)? Run `damask help cold-start` for the playbook.

**2. Query** — check what's known before working:
```bash
damask at src/auth.rs:50             # edges touching line 50
damask at src/auth.rs                # all edges for file
damask where "rel=risk" "tags~security"  # AND-composed predicates
damask search "authentication"       # full-text search
damask follow <span_id>             # traverse edge graph
```

**3. Record** — preserve findings as you work:
```bash
damask record src/auth.rs 42 67 risk -m "No rate limiting on login" -c 0.9 \
  --action "Add rate limiter" --symbol handle_login
```
`-m` is the summary, `-c` confidence (0.0-1.0), `--severity critical|high|medium|low` is how much it MATTERS (orthogonal to confidence). `--broadcast` marks rare repo-wide news: for 24h, hook-enabled sessions (Claude Code) receive it at their next drain point regardless of file relevance — use it like paging someone. Add any domain field with `--field key=value` — every payload field is then filterable (`damask where "jurisdiction=EU"`). Severity is the default convention, not core: a namespace can assert its own schema in `.damask/config.json` — `"namespaces":{"<ns>":{"schema":{"<field>":{"enum":[...],"rank":{"<value>":1.2}}}}}` — enum values are validated at write time and rank weights shape ordering. Inline JSON payloads also work; `damask help record` has the full schema.

**4. Signal** — maintain graph quality:
```bash
damask endorse <edge_id>             # this is correct (id prefixes work: e_01KH3K)
damask close <edge_id> --reason resolved  # this is DONE — closes disappear from at/where/briefing
damask dispute <edge_id> --reason incorrect  # this is WRONG (use close for fixed things)
damask confirm <span_or_edge_id>     # drifted anchor still true — re-anchors it, clears the ⚠
damask triage                        # find rot, get ready-to-run bulk closes (never auto)
damask sweep --reanchor              # bulk-heal every drifted anchor in one pass
```
Use `close` when a finding is resolved, `dispute` only when it is wrong. Endorse/dispute/close now SHOW you the edge's history (its claim and prior signals) as you act — read it: if you are contradicting work other sessions confirmed, the command flags the edge as contested, and your payload should cite the evidence for the contradiction. `--reason` accepts the templates or any free text (`--reason "superseded by PR #42"`). Investigated a risk and dismissed it? Record it with `"status":"ruled_out"` — it sinks in every ranking, and `damask triage --close-ruled-out` retires it later.

**Fan-outs / parallel agents:** concurrent appends are torn-write-safe (single atomic write per batch — tested under 8 parallel writers). Per-agent namespaces are for ISOLATION of concerns, not safety. Never `ns set` in a parallel agent (it is a shared file); set the `DAMASK_NS` env var per process or pass `--ns` instead. For bulk writes, `damask batch --stdin` creates many facts atomically with `$N` back-references.

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
| `sweep [--reanchor]` | Anchor freshness report; --reanchor heals all drifted spans |
| `tag <id> <tag>...` | Add tags to an existing edge (append-only) |
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
- `damask briefing --export` — read-only digest for a sandboxed/hookless agent (host runs it at launch, injects the output as context)
- `damask help record` — payload envelope, confidence scale, examples
- `damask help batch` — `$N` back-references, multi-span examples
- `damask help where` — predicate syntax, operators, lifecycle field
- `damask help rels` — full relationship type table
- `damask help patterns` — advanced audit patterns (loading census, undo archaeology, etc.)
- `damask help quality` — writing high-quality annotations
- `damask help cold-start` — structured first-pass playbook
