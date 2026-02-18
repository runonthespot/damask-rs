---
name: damask
description: Use damask to query, record, and manage structured knowledge about this codebase. Invoke when exploring code, recording findings, checking risks/gotchas, or auditing.
---

# Damask — Knowledge Fabric for Code

## Quick Start (60 seconds)

```bash
damask init                          # create .damask/ in your repo
damask ns set my-audit               # create a namespace for your work
damask orient                        # see what's already known (or cold start)
damask record src/auth.rs 42 67 risk \
  '{"summary":"No rate limiting on login","confidence":0.9,"action":"Add rate limiter middleware"}' \
  --symbol handle_login              # record a finding
damask at src/auth.rs:50             # query what's known about a location
```

## Overview

Damask is a structured annotation graph that lives alongside this repo in `.damask/`. It stores typed, scored observations about code — risks, decisions, dependencies, gotchas — and queries them back. Not comments that rot, but a queryable graph of institutional knowledge that decays gracefully and tracks its own freshness.

## Core Concepts

**Spans** pin a region of a source file (path + line range + content hash). They survive code movement via a resolution cascade: content hash match → hash search (relocated) → symbol fallback → snippet fuzzy match → git rename detection.

**Edges** are typed relationships between spans. Each edge carries a JSON payload with summary, confidence, action, tags, and evidence. Edges can be endorsed (confirmed), disputed (contradicted), or superseded (replaced).

**Namespaces** isolate layers of edges — per-audit, per-agent, per-task. Switch with `damask ns set <name>` or `--ns <name>`.

Everything is **append-only JSONL** in `.damask/edges/`. A SQLite index is built on-the-fly. Stale observations decay via a configurable half-life.

## Step 1: Orient

**Always start here.** One command gives you the full picture:

```bash
damask orient                              # status + risks + gotchas + decisions + recent activity
damask orient --rel risk --undisputed      # untriaged risks only
damask orient --tag security               # only edges tagged "security"
```

This returns graph stats, namespace list, top risks/gotchas/decisions (sorted by confidence), and recent activity. Use `--format json` for machine-readable output. Use `--rel`, `--tag`, and `--undisputed` flags to filter the orient view.

**Cold start vs warm start**: If `damask orient` reports an empty graph (cold_start=true), run the Cold Start Playbook below before doing anything else. If the graph has edges, read the orient output, then proceed to Step 2.

### Cold Start Playbook

When the graph is empty, do a structured first pass to give future agents (and yourself) something to work with. Work through these in order, recording as you go via `damask batch`. Aim for breadth over depth — flag things for later investigation rather than fully analyzing each one.

**1. Identify the skeleton** — Read the top-level directory structure, build files (`Cargo.toml`, `package.json`, `go.mod`, etc.), and any existing README or ARCHITECTURE docs. Record `describes` edges for:
- Entry points (main, server start, CLI dispatch)
- Module/package boundaries and their responsibilities
- Build targets and how they relate

**2. Trace the critical paths** — Skim the code for the highest-risk areas. Record `risk` or `gotcha` edges for anything that looks concerning:
- Authentication and authorization flows
- Data validation boundaries (user input, API inputs, deserialization)
- Secret/credential handling (hardcoded values, env vars, config files)
- Error handling patterns (swallowed errors, panics, missing error propagation)
- Concurrency and shared mutable state

**3. Map key dependencies** — Record `depends_on` edges between components that must coordinate:
- Database/storage access patterns
- External service calls and their failure modes
- Initialization ordering constraints
- Config values that affect runtime behavior

**4. Note architectural decisions** — Record `decision` edges for choices visible in the code:
- Framework and library selections (especially non-obvious ones)
- Patterns used (middleware chains, plugin systems, event buses)
- Anything with a comment explaining "why" — capture it before it rots

**5. Batch it** — Combine your findings into a single `damask batch` call. Example structure:

```bash
damask batch --stdin <<'EOF'
[
  {"span": {"path":"src/main.rs", "start":1, "end":15, "symbol":"main"}},
  {"edge": {"from":"$0", "to":"_", "rel":"describes", "payload":{
    "summary":"CLI entry point — dispatches to subcommands via clap",
    "confidence":0.95, "tags":["architecture","entry-point"]
  }}},
  {"span": {"path":"src/auth/mod.rs", "start":42, "end":67, "symbol":"verify_request"}},
  {"edge": {"from":"$2", "to":"_", "rel":"risk", "payload":{
    "summary":"Auth middleware skips verification for paths matching SKIP_AUTH_ROUTES",
    "confidence":0.8, "action":"Audit SKIP_AUTH_ROUTES for overly broad patterns",
    "tags":["security","auth"], "status":"hypothesis"
  }}}
]
EOF
```

After the batch completes, run `damask orient` again — it should now show a useful starting picture.

For deeper exploration after orienting:

```bash
damask where "rel=risk"                          # all risks (not just top 5)
damask where "rel=risk" "tags~security"          # AND-composed: risks tagged security
damask where "rel=risk" "lifecycle=untriaged"    # only untriaged risks
damask where "confidence>0.8"                    # high-confidence findings across all rels
damask where "tags~security"                     # edges with tags containing "security"
damask where "summary~injection"                 # edges mentioning injection
damask where "lifecycle=superseded"              # show superseded (inactive) edges
damask where "rel=risk" --since 2025-06-01       # risks created after a date
damask search "authentication"                   # full-text search over payloads
damask lint                                      # quality issues in the graph
```

If you're about to work on a specific file, check what's known:

```bash
damask at src/auth.rs:50                         # edges touching line 50
damask at src/auth.rs                            # all edges for the file
damask at src/auth.rs --tag security --undisputed  # security findings, undisputed only
```

## Step 2: Work with Context

As you explore and modify code, use damask to inform your decisions:

```bash
damask follow <span_id>              # traverse the edge graph from a span
damask follow <span_id> risk         # follow only risk edges
damask why <edge_id>                 # provenance: who created/endorsed/disputed
damask resolve <span_id>             # materialize span content, check freshness
```

### Filtering (the `where` command)

Predicates support these operators: `=`, `!=`, `>`, `<`, `>=`, `<=`, `~` (contains).

Filterable fields: `rel`, `ns`, `agent`, `endorsed`, `disputed`, `confidence`, `status`, `summary`, `tags`, `lifecycle`.

The `lifecycle` virtual field is computed from edge state: `untriaged` (active, no endorsements or disputes), `endorsed` (active, has endorsements), `disputed` (active, has disputes), `superseded` (inactive).

Multiple predicates are AND-composed:

```bash
damask where "rel=risk"                          # exact match
damask where "rel=risk" "tags~security"          # AND: risks with security tag
damask where "rel=risk" "lifecycle=untriaged"    # unresolved risks
damask where "rel!=describes"                    # negation
damask where "confidence>=0.9"                   # numeric comparison
damask where "endorsed>0"                        # has at least one endorsement
damask where "disputed=true"                     # boolean: is it disputed?
damask where "tags~auth"                         # any tag containing "auth"
damask where "tags=security"                     # exact tag match
damask where "summary~SQL"                       # substring in summary
damask where "lifecycle=superseded"              # show superseded (inactive) edges
damask where "rel=risk" --since 2025-06-01       # temporal filter
```

Unknown fields produce helpful errors listing valid fields and examples.

## Step 3: Record Findings

When you discover something worth preserving, record it. **The quality of what you record determines the value of the graph.** Write payloads that a future agent (or human) encountering this code for the first time could act on without re-doing your analysis.

### One-shot (preferred): `damask record`

Creates a span and edge in a single call:

```bash
damask record src/auth/token.rs 142 178 risk \
  '{"summary":"JWT validation accepts expired tokens — no exp claim check","confidence":0.95,"action":"Add exp validation in verify_token() before line 155","tags":["security","jwt","authentication"],"reasoning":"verify_token() checks signature and issuer but never reads the exp claim. Tokens with exp in the past are accepted. This was likely missed because the test fixtures use tokens without exp.","impact":"Any leaked or intercepted token remains valid forever. Combined with the 30-day refresh cycle, this creates a wide exploitation window.","reproduction":"Create a token with exp set to a past date, call verify_token() — it returns Ok."}' \
  --symbol verify_token
```

**Syntax**: `damask record <file> <start> <end> <rel> <payload> [--symbol <sym>] [--to <id>]`
- `--to` defaults to `_` (null) — most findings are dangling edges (span → null)
- `--symbol` anchors the span to a function/class name for refactoring resilience
- JSON output (`--format json`) returns `[span, edge]` array

### Batch: `damask batch`

Create multiple facts atomically with `$N` back-references. Use this when you have multiple related findings, or need to record relationships between code regions:

**Example: Recording a dependency between two code regions:**

```bash
damask batch --stdin <<'EOF'
[
  {"span": {"path":"src/auth/token.rs", "start":142, "end":178, "symbol":"verify_token"}},
  {"span": {"path":"src/auth/config.rs", "start":22, "end":35, "symbol":"TOKEN_CONFIG"}},
  {"edge": {"from":"$0", "to":"$1", "rel":"depends_on", "payload":{
    "summary":"verify_token() reads signing key from TOKEN_CONFIG but does not validate config is loaded",
    "confidence":0.85,
    "tags":["initialization","auth"],
    "reasoning":"If verify_token() is called before config initialization completes (e.g. during eager request handling), TOKEN_CONFIG may contain default/empty values, causing signature verification to silently pass with an empty key.",
    "action":"Add config-loaded assertion at top of verify_token() or make TOKEN_CONFIG a OnceCell that panics on uninitialized access"
  }}},
  {"edge": {"from":"$0", "to":"_", "rel":"risk", "payload":{
    "summary":"verify_token() can silently accept any token if called before config init",
    "confidence":0.80,
    "status":"hypothesis",
    "tags":["security","initialization","race-condition"],
    "action":"Verify whether any code path can reach verify_token() before config init"
  }}}
]
EOF
```

**Example: Recording contradictions between code regions:**

```bash
damask batch --stdin <<'EOF'
[
  {"span": {"path":"src/config.rs", "start":45, "end":52}},
  {"span": {"path":"src/server.rs", "start":112, "end":118}},
  {"edge": {"from":"$0", "to":"$1", "rel":"contradicts", "payload":{
    "summary":"Config declares max_connections=100 but server.rs hardcodes pool size to 50",
    "confidence":0.90,
    "tags":["config","database"],
    "action":"Align pool size with config value or document why they differ"
  }}}
]
EOF
```

- `$N` refers to the fact at index N (zero-based, must reference an earlier item)
- All-or-nothing: validates everything before writing
- Also accepts `--file batch.json` instead of `--stdin`

### Two-step: `damask span` + `damask edge`

For cases where you need the span and edge separately (e.g., creating edges to existing spans):

```bash
damask span src/auth/token.rs 142 178 --symbol verify_token
# Returns: s_01JKX...

damask edge <span_id> _ risk '{"summary":"No token expiry check","confidence":0.95}'
```

**Edge syntax**: `damask edge <from> <to> <rel> <payload>`
- Use `_` for null endpoints
- Payload is inline JSON or use `-f payload.json` or `--stdin`

### Relationship Types

| Rel | Class | When to use | `from` → `to` |
|-----|-------|-------------|----------------|
| `risk` | Judgment | Security, correctness, or reliability risks | span → null |
| `gotcha` | Judgment | Non-obvious pitfalls future developers will hit | span → null |
| `decision` | Judgment | Architectural choices and their rationale | span → null |
| `contradicts` | Judgment | Two code regions that conflict with each other | span → span |
| `ruled_out` | Judgment | Alternatives considered and rejected | span → null |
| `conflicts_with` | Judgment | Semantic conflicts between components | span → span |
| `depends_on` | Descriptive | Runtime, build, or initialization dependencies | span → span |
| `supports` | Descriptive | Evidence supporting a claim | span → edge |
| `describes` | Descriptive | Documentation-like annotations | span → null |
| `derived_from` | Descriptive | Lineage / provenance | span → span |
| `co_change` | Descriptive | Files that must change together | span → span |
| `implements` | Descriptive | Code that implements a spec or design | span → span |
| `env` | Descriptive | Environment-specific behavior | span → null |
| `perf` | Descriptive | Performance characteristics | span → null |

Custom rel types are allowed and rank between Judgment and Descriptive.

**When to use relational edges (span → span)**: If your finding is about how two pieces of code interact — a dependency, contradiction, co-change requirement, or implementation relationship — link them. Future agents traversing the graph with `damask follow` will discover these connections.

### Payload Envelope

Always include `summary` and `confidence`. Add more fields to make findings actionable:

```json
{
  "summary": "Short, specific, actionable description — what + why it matters",
  "confidence": 0.85,
  "status": "assertion",
  "action": "Concrete next step someone should take",
  "tags": ["lowercase", "hyphenated", "for-filtering"],
  "evidence": ["s_01JKX...", "e_01JKY..."],
  "reasoning": "Why you believe this — what you checked, what led you here",
  "impact": "What happens if this is ignored — scope and severity",
  "reproduction": "How to verify this finding independently"
}
```

**Standard fields** (read by damask for ranking, filtering, display):
- **summary**: One line. Be specific: "JWT exp claim not checked in verify_token()" not "auth issue"
- **confidence**: 0.0–1.0 — how certain are you that this finding is correct?
  - 0.9+ = verified (you read the code, traced the logic, tested it)
  - 0.7–0.9 = strong analysis (you read the code, reasoning is sound but untested)
  - 0.5–0.7 = informed hypothesis (code suggests this but you didn't fully verify)
  - <0.5 = speculation (pattern match, heuristic, or incomplete analysis)
- **status**: `"assertion"` (default), `"hypothesis"` (speculative — flag for later verification), `"ruled_out"` (you investigated and determined this is NOT an issue)
- **action**: What should be done. Be specific: "Add `if token.exp < now()` check at line 155" not "fix this"
- **tags**: Lowercase, hyphenated. Used for filtering with `damask where "tags=security"` or `damask where "tags~auth"`
- **evidence**: IDs of spans or edges that support this finding

**Extended fields** (stored in payload, not read by damask, but invaluable for future agents):
- **reasoning**: Your analytical chain. What did you look at? What alternatives did you consider? Why did you reach this conclusion? A future agent should be able to evaluate your reasoning without re-doing the analysis.
- **impact**: Severity and blast radius. "Tokens never expire" is the finding; "any leaked token grants permanent access across all services" is the impact. Distinguish from confidence: a low-confidence, high-impact risk is very different from a high-confidence, low-impact one.
- **reproduction**: Steps to independently verify. "Create token with `exp: 0`, call `verify_token()`, observe it returns `Ok`."

### Recording Negative Evidence

When you investigate something and determine it's NOT an issue, record that too. Negative evidence prevents future agents from re-investigating the same concern:

```bash
# Using status: "ruled_out" — you checked and it's fine
damask record src/db/pool.rs 45 67 risk \
  '{"summary":"Connection pool has no size limit — investigated, bounded by MAX_CONNECTIONS config","confidence":0.90,"status":"ruled_out","tags":["database","resource-limits"],"reasoning":"Pool wraps deadpool which reads max_size from config. Default is 32. Verified in test_pool_config()."}' \
  --symbol create_pool
```

```bash
# Disputing an existing edge — someone else flagged it, you verified it's wrong
damask dispute <edge_id> '{"summary":"SQL injection not possible — query uses parameterized statements via sqlx::query!() macro which validates at compile time"}'
```

## Step 4: Endorse, Dispute, Supersede

When you encounter existing edges during your work:

```bash
# Confirm an edge your work validated
damask endorse <edge_id>

# Contradict an edge with evidence (payload required — explain why)
damask dispute <edge_id> '{"summary":"Token expiry was added in commit abc123 — exp claim now checked at line 155"}'

# Use a reason template instead of raw JSON
damask dispute <edge_id> --reason mitigated    # mitigated | stale | false-positive | duplicate

# Batch dispute: resolve multiple edges at once
echo "e_1\ne_2\ne_3" | damask dispute --batch --reason stale

# Supersede: record the new finding, then link it to the old edge
damask record src/auth/token.rs 142 178 risk \
  '{"summary":"Refresh tokens still lack rotation despite exp fix","confidence":0.9}' \
  --symbol verify_token
# Returns: new_edge_id — now create the supersedes link
damask edge <new_edge_id> <old_edge_id> supersedes '{"summary":"Expiry check added in abc123 but refresh token rotation still missing"}'
```

**Quality signals matter.** Endorsed edges rank higher in `damask at` and `damask orient`. Disputed edges rank lower. Unendorsed low-confidence edges decay fastest. The graph self-cleans over time — endorse what matters.

## Output Formats

All commands support `--format json` for machine consumption:

```bash
damask at src/auth.rs:50 --format json
damask where "rel=risk" --format json
damask orient --format json
```

## Freshness Indicators

Spans track whether their anchored code has moved or changed:
- **Exact + Unchanged** = content matches, file unchanged
- **Relocated** = content found at different lines (or renamed file)
- **File Changed** = file modified since span was created
- **Unresolved** = content can't be located in the file
- **Missing** = file no longer exists

Edges attached to stale spans are automatically down-ranked.

## Writing High-Quality Annotations

1. **Be specific**: Pin narrow line ranges, not whole files. A span covering 5 lines is more useful than one covering 200.
2. **Use symbols**: `--symbol fn_name` survives refactoring better than line numbers alone.
3. **Set confidence honestly**: 0.95 means "I traced the code path and verified this." Don't inflate.
4. **Separate confidence from severity**: A 0.6 confidence finding with catastrophic impact is worth recording. Use `impact` in the payload to capture severity; use `confidence` only for how sure you are.
5. **Include reasoning**: A future agent should understand *why* you believe this, not just *what* you believe.
6. **Make actions concrete**: "Add expiry check at line 155" not "fix authentication."
7. **Link related code**: Use `depends_on`, `contradicts`, `co_change` to connect code regions. Isolated findings are less valuable than connected ones.
8. **Record negative evidence**: If you checked something and it's fine, say so with `status: "ruled_out"`. This saves future agents from repeating your work.
9. **Tag consistently**: Use lowercase, hyphenated tags for filterability.
10. **Endorse what you verify**: Endorsements are the signal that separates noise from knowledge.
11. **Dispute rather than ignore**: A disputed edge with rationale is more valuable than silence.

## Advanced Patterns

Beyond recording individual risks and gotchas, damask can model entire UX flows, track consistency across a codebase, and build compound taxonomies. These patterns use the same primitives (spans, edges, tags) but combine them to answer higher-level questions.

### Loading Census

Catalog every moment a user waits. Tag each loading state by **strategy** and **location**, then use `damask where` to find patterns:

```bash
# Record each loading state with compound tags
damask record src/pages/Chat.tsx 112 130 describes \
  '{"summary":"[Send Message]: immediate user bubble → empty assistant bubble → progressive token streaming via rAF","confidence":0.95,"tags":["loading-census","streaming","mutation"]}' \
  --symbol handleSend

damask record src/pages/Apps.tsx 18 25 describes \
  '{"summary":"[Apps Page]: full-page Loader2 spinner blocks all interaction until apps fetch completes","confidence":0.95,"tags":["loading-census","spinner","page-load"]}' \
  --symbol AppsPage

# Find all loading states where the user sees nothing
damask where "tags~loading-census" # all documented loading moments
damask where "tags~nothing"        # blank screens — likely the worst UX
damask where "tags~progressive"    # best practice implementations
```

**Strategy tags**: `nothing` (blank/white), `spinner` (blocking), `frozen` (unresponsive), `progressive` (incremental), `streaming` (real-time).
**Location tags**: `auth`, `page-load`, `list-load`, `detail-load`, `mutation`, `file-op`, `search`.

This builds an **observable UX map** — run `damask where "tags~nothing"` to find every place a user stares at a blank screen.

### Undo Archaeology

Audit every destructive action for its protection level. Inconsistencies in confirmation patterns are a top source of user frustration and data loss:

```bash
damask batch --stdin <<'EOF'
[
  {"span": {"path":"src/pages/Chat.tsx", "start":89, "end":105, "symbol":"handleDelete"}},
  {"edge": {"from":"$0", "to":"_", "rel":"describes", "payload":{
    "summary":"[Delete Conversation]: ConfirmDialog with danger variant + irreversibility warning",
    "confidence":0.95, "tags":["undo-archaeology","confirmed","delete"]
  }}},
  {"span": {"path":"src/components/PipelineCanvas.tsx", "start":201, "end":208, "symbol":"removeNode"}},
  {"edge": {"from":"$2", "to":"_", "rel":"risk", "payload":{
    "summary":"[Delete Pipeline Node]: zero confirmation — node removed on single click",
    "confidence":0.95, "tags":["undo-archaeology","unprotected","delete"],
    "action":"Add confirmation or undo toast — destructive action with no recovery path"
  }}},
  {"edge": {"from":"$0", "to":"$2", "rel":"contradicts", "payload":{
    "summary":"Conversation delete requires ConfirmDialog but pipeline node delete has no confirmation — inconsistent protection for same action class",
    "confidence":0.95, "tags":["undo-archaeology","consistency"]
  }}}
]
EOF
```

**Protection tags**: `confirmed` (dialog/prompt), `unprotected` (no confirmation), `undoable` (has undo/restore).

The `contradicts` edge between the two spans makes the inconsistency queryable — `damask where "tags~undo-archaeology"` shows the full picture.

### Error Personality

Audit how the app talks to users when things go wrong. Inconsistent error tone erodes trust:

```bash
damask record src/pages/Chat.tsx 645 652 describes \
  '{"summary":"[Chat Error]: apologetic tone with guidance — \"Something went wrong. Please try again.\"","confidence":0.95,"tags":["error-personality","tone-apologetic","user-facing"]}' \
  --symbol handleError

damask record src/api/documents.ts 78 85 describes \
  '{"summary":"[Document Fetch Error]: leaks HTTP status code to console — \"Request failed with status 403\"","confidence":0.9,"tags":["error-personality","tone-technical","internal-leak"]}' \
  --symbol fetchDocument

# Find all terse errors that need better messaging
damask where "tags~tone-blunt"
# Find all errors that leak internals to users
damask where "tags~internal-leak"
```

**Tone tags**: `tone-apologetic` (empathetic + guidance), `tone-blunt` (bare error, no help), `tone-technical` (leaks internals).

### Consistency Audits with `contradicts`

Use `contradicts` edges to flag places where the same concept is implemented differently across components. Unlike `co_change` (which says "these must change together"), `contradicts` says "these are inconsistent and shouldn't be":

```bash
damask batch --stdin <<'EOF'
[
  {"span": {"path":"src/pages/Pipelines.tsx", "start":45, "end":62}},
  {"span": {"path":"src/pages/Chat.tsx", "start":180, "end":188}},
  {"edge": {"from":"$0", "to":"$1", "rel":"contradicts", "payload":{
    "summary":"PipelinesPage has illustrated empty state (icon + explanation + CTA) while ChatPage has minimal empty state (text only, no illustration) — inconsistent zero-data experience",
    "confidence":0.9, "tags":["empty-state","consistency","ux"]
  }}}
]
EOF

# Find all consistency violations
damask where "rel=contradicts"
```

This scales to any dimension of consistency: empty states, loading strategies, error handling, confirmation patterns, icon usage, color semantics.

### Flow Tracing with `depends_on` Chains

Model multi-step request or interaction flows as chains of `depends_on` edges. Each span is a step, and `damask follow` traverses the entire path:

```bash
damask batch --stdin <<'EOF'
[
  {"span": {"path":"src/api/chat.ts", "start":45, "end":58, "symbol":"sendMessage"}},
  {"span": {"path":"src/server/routes/chat.py", "start":112, "end":145, "symbol":"chat_stream"}},
  {"span": {"path":"src/server/agents/base.py", "start":67, "end":98, "symbol":"_agent_stream"}},
  {"span": {"path":"src/server/retrieval/hybrid.py", "start":23, "end":55, "symbol":"retrieve"}},
  {"edge": {"from":"$0", "to":"$1", "rel":"depends_on", "payload":{
    "summary":"Frontend sendMessage() calls /api/chat via SSE stream",
    "confidence":0.95, "tags":["flow","chat-send"]
  }}},
  {"edge": {"from":"$1", "to":"$2", "rel":"depends_on", "payload":{
    "summary":"chat_stream delegates to _agent_stream for LLM orchestration",
    "confidence":0.95, "tags":["flow","chat-send"]
  }}},
  {"edge": {"from":"$2", "to":"$3", "rel":"depends_on", "payload":{
    "summary":"_agent_stream calls HybridRetriever for RAG context before LLM call",
    "confidence":0.95, "tags":["flow","chat-send","rag"]
  }}}
]
EOF

# Traverse the full flow from the entry point
damask follow <sendMessage_span_id>
# See just the RAG-related steps
damask follow <sendMessage_span_id> depends_on
```

This gives `damask follow` something to traverse — starting from the API entry point, an agent can discover every downstream dependency in the request path.

### Resolution Tracking with Meta-Edges

When you fix an issue that was recorded as a risk, `dispute` the original edge to create a resolution trail. The `from` field points to the edge being resolved:

```bash
# Original risk was recorded earlier as e_01KH3KP24M...
damask dispute e_01KH3KP24M... \
  '{"summary":"Fixed: changed user.user_id to user.id across all persona queries — PR #247"}'

# Or use a reason template
damask dispute e_01KH3KP24M... --reason mitigated

# Find all unresolved risks using lifecycle
damask where "rel=risk" "lifecycle=untriaged"   # unresolved risks
damask where "lifecycle=disputed"                # disputed edges
damask where "lifecycle=superseded"              # superseded edges
```

Over time this builds an audit trail: what was found, when it was fixed, and by whom. `damask orient` automatically down-ranks disputed edges so they fade from the "active risks" view.

### Compound Tag Taxonomies

Use multi-dimensional tags to enable powerful cross-cutting queries without creating extra edges:

```
tags: ["loading-census", "spinner", "page-load", "auth"]
tags: ["undo-archaeology", "unprotected", "delete", "pipeline"]
tags: ["error-personality", "tone-blunt", "http-error", "api"]
```

Each tag dimension is filterable independently:
- `damask where "tags~loading-census"` — all loading states
- `damask where "tags~spinner"` — just spinner implementations
- `damask where "tags~auth"` — everything auth-related, across all audit types

Design tags as `[audit-type, classification, location-or-action]` for maximum queryability.
