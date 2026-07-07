---
name: damask
description: This repo has a knowledge graph (.damask/) of verified findings from previous sessions. Invoke BEFORE exploring, refactoring, or debugging unfamiliar code (inherit what past agents learned instead of re-deriving it); when about to record, note, or remember a finding, risk, gotcha, or decision; after fixing a known issue (close its edge); when auditing or reviewing; or on any mention of damask, annotations, knowledge graph, "what do we know about", "why is this like this", or "record this".
allowed-tools: Bash(damask *)
---

# Damask — Knowledge Fabric for Code

This repo carries a knowledge graph in `.damask/`: risks, gotchas, and decisions pinned to exact code regions by the agents and humans who worked here before you. Anchors follow the code through edits and renames; every claim carries provenance (`why <id>`); wrong claims get disputed and sink; resolved ones get closed and vanish. What you're reading when you query it is what's still true — or honestly marked when it might not be.

The deal favours you twice. Every verified fact you inherit is exploration you don't repeat — a `damask at` on a file costs ~100 tokens and can save the twenty minutes it took someone to learn what's in it. And everything you record outlives your context window: your next session, another agent, a cheaper model all start knowing it. One `record` is seconds; the same lesson re-derived is twenty minutes, per session, forever — sometimes re-derived *wrong* (agents here once repeatedly "fixed" imports that were correct until a one-line gotcha ended it).

**Record judgment, not description.** A future agent can re-read the code; it cannot re-learn what the code cost you. Record what surprised you, what broke, what you tried that failed, why the decision went this way — with honest confidence numbers. Don't restate what any reader of the file can see.

**Trust discipline** — what keeps the graph worth reading:
- Glyphs are earned, not decorative: ✅ verified fresh · ↪ code moved · ⚠ changed since recorded · ❌ anchor gone. Weight your trust accordingly.
- Confirmed something in your own work? `endorse` it. Found it's wrong? `dispute` it — *including your own earlier claims; that is the system working, not failing*. Fixed or obsolete? `close` it (disputes only weaken ranking; closes disappear).
- Reading without signalling is freeloading: the graph self-prunes only because agents like you garden as they go.

If `damask init --claude` installed hooks, the loop runs itself: a briefing at session start, relevant edges injected when you touch annotated files (`peek`, once per session each), a single Stop-hook nudge if you edited without recording. Facts are stamped with your agent/session identity automatically. Make claims mechanically verifiable when you can: a `check` shell command in the payload lets `damask verify --auto` keep them endorsed or disputed by exit code.

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
`-m` is the summary, `-c` confidence (0.0-1.0), `--severity critical|high|medium|low` is how much it MATTERS (orthogonal to confidence). Add any domain field with `--field key=value` — every payload field is then filterable (`damask where "jurisdiction=EU"`). Severity is the default convention, not core: a namespace can assert its own schema in `.damask/config.json` — `"namespaces":{"<ns>":{"schema":{"<field>":{"enum":[...],"rank":{"<value>":1.2}}}}}` — enum values are validated at write time and rank weights shape ordering. Inline JSON payloads also work; `damask help record` has the full schema.

**4. Signal** — maintain graph quality:
```bash
damask endorse <edge_id>             # this is correct (id prefixes work: e_01KH3K)
damask close <edge_id> --reason resolved  # this is DONE — closes disappear from at/where/briefing
damask dispute <edge_id> --reason incorrect  # this is WRONG (use close for fixed things)
damask confirm <span_or_edge_id>     # drifted anchor still true — re-anchors it, clears the ⚠
damask triage                        # find rot, get ready-to-run bulk closes (never auto)
damask sweep --reanchor              # bulk-heal every drifted anchor in one pass
```
Use `close` when a finding is resolved, `dispute` only when it is wrong. `--reason` accepts the templates or any free text (`--reason "superseded by PR #42"`). Investigated a risk and dismissed it? Record it with `"status":"ruled_out"` — it sinks in every ranking, and `damask triage --close-ruled-out` retires it later.

**Fan-outs / parallel agents:** concurrent appends are torn-write-safe (single atomic write per batch — tested under 8 parallel writers). Per-agent namespaces are for ISOLATION of concerns, not safety. Never `ns set` in a parallel agent (it is a shared file); set the `DAMASK_NS` env var per process or pass `--ns` instead. For bulk writes, `damask batch --stdin` creates many facts atomically with `$N` back-references.

## Command Reference

| Command | Purpose |
|---------|---------|
| `orient` | One-shot orientation: status + top findings + recent activity |
| `at <loc>` | Edges touching a file, line, or directory (dir → per-file rollup) |
| `where <pred>...` | Filter edges, ranked + located (separate args AND-compose) |
| `search <query>` | Full-text search over payloads |
| `record <file> <start> <end> <rel> -m "..." -c 0.9` | Create span + edge atomically |
| `bootstrap` | Seed an empty graph (manifests, TODOs, co-change history) |
| `batch --stdin` | Create many facts atomically ($N back-references) |
| `endorse <id>` | Signal confirmation |
| `dispute <id> --reason <r>` | Signal contradiction (wrong ≠ resolved — resolved means `close`) |
| `close <id> --reason resolved` | Mark done: disappears from at/where/briefing |
| `confirm <id>` | Re-anchor a drifted span: still true of the code as it stands |
| `triage` | Rot report + ready-to-run bulk closes (never closes on its own) |
| `sweep [--reanchor]` | Anchor freshness report; --reanchor heals all drifted spans |
| `tag <id> <tag>...` | Add tags to an existing edge (append-only) |
| `follow <id> [rel]` | Traverse edge graph |
| `why <id>` | Provenance: who created/endorsed/disputed |
| `ns set <name>` | Switch namespace |
| `log --since <date>` | Recent facts (bounded to 50 by default) |
| `lint` | Flag quality issues |
| `help <topic>` | Detailed reference (record, batch, where, rels, patterns, quality, cold-start) |

IDs accept unique prefixes everywhere (`damask endorse e_01KH3K`). Every 0-result teaches the next query — trust the hints it prints.

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
