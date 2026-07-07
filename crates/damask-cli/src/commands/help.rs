use crate::error::Result;

/// Known help topics and their content.
const TOPICS: &[(&str, &str)] = &[
    ("record", HELP_RECORD),
    ("batch", HELP_BATCH),
    ("where", HELP_WHERE),
    ("rels", HELP_RELS),
    ("cold-start", HELP_COLD_START),
    ("patterns", HELP_PATTERNS),
    ("quality", HELP_QUALITY),
    ("hooks", HELP_HOOKS),
];

pub fn run(topic: Option<&str>) -> Result<()> {
    match topic {
        None => {
            println!("Available help topics:\n");
            for (name, _) in TOPICS {
                println!("  damask help {name}");
            }
            println!("\nRun `damask help <topic>` for detailed reference.");
            Ok(())
        }
        Some(t) => {
            if let Some((_, content)) = TOPICS.iter().find(|(name, _)| *name == t) {
                print!("{content}");
                Ok(())
            } else {
                eprintln!("Unknown topic: {t}\n");
                eprintln!("Available topics:");
                for (name, _) in TOPICS {
                    eprintln!("  {name}");
                }
                Err(anyhow::anyhow!("unknown help topic: {t}"))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Help content — extracted from the full skill reference
// ---------------------------------------------------------------------------

const HELP_RECORD: &str = r#"# Recording Findings

## One-shot (preferred): `damask record`

Creates a span and edge in a single call:

```bash
damask record src/auth/token.rs 142 178 risk \
  '{"summary":"JWT validation accepts expired tokens","confidence":0.95,"action":"Add exp validation in verify_token()","tags":["security","jwt"]}' \
  --symbol verify_token
```

**Syntax**: `damask record <file> <start> <end> <rel> <payload> [--symbol <sym>] [--to <id>]`
- `--to` defaults to `_` (null) — most findings are dangling edges (span → null)
- `--symbol` anchors the span to a function/class name for refactoring resilience
- JSON output (`--format json`) returns `[span, edge]` array

## Two-step: `damask span` + `damask edge`

```bash
damask span src/auth/token.rs 142 178 --symbol verify_token
# Returns: s_01JKX...

damask edge <span_id> _ risk '{"summary":"No token expiry check","confidence":0.95}'
```

## Payload Envelope

Always include `summary` and `confidence`. Add more fields to make findings actionable:

```json
{
  "summary": "Short, specific, actionable — what + why it matters",
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

**Standard fields** (read by damask for ranking/filtering):
- **summary**: One line. Be specific: "JWT exp claim not checked in verify_token()" not "auth issue"
- **confidence**: 0.0–1.0
  - 0.9+ = verified (traced the code, tested it)
  - 0.7–0.9 = strong analysis (read the code, untested)
  - 0.5–0.7 = informed hypothesis (code suggests this)
  - <0.5 = speculation
- **status**: `"assertion"` (default), `"hypothesis"`, `"ruled_out"`
- **severity**: `"critical"`, `"high"`, `"medium"`, `"low"` — how much it MATTERS, orthogonal to confidence (how sure you are). Filterable: `where "severity=critical"`; nudges ranking modestly.
- **action**: Be specific: "Add `if token.exp < now()` check at line 155" not "fix this"
- **tags**: Lowercase, hyphenated. Used for `damask where "tags=security"`
- **evidence**: IDs of spans or edges that support this finding

**Extended fields** (stored in payload, invaluable for future agents):
- **reasoning**: Your analytical chain — what you looked at, why you reached this conclusion
- **impact**: Severity and blast radius, distinct from confidence
- **reproduction**: Steps to independently verify
- **check**: Shell command whose exit code revalidates this claim — makes the
  edge mechanically verifiable via `damask verify` (see `damask help hooks`)

## Recording Negative Evidence

```bash
damask record src/db/pool.rs 45 67 risk \
  '{"summary":"Connection pool has no size limit — investigated, bounded by MAX_CONNECTIONS","confidence":0.90,"status":"ruled_out","tags":["database"]}' \
  --symbol create_pool
```

## Endorsing, Disputing, Closing

```bash
damask endorse <edge_id>                              # confirm
damask dispute <edge_id> '{"summary":"Fixed in..."}'  # contradict
damask close <edge_id> --reason resolved              # mark resolved
damask dispute <edge_id> --reason incorrect            # reason template

# Batch operations
echo "e_1\ne_2" | damask close --batch --reason resolved
echo "e_1\ne_2" | damask dispute --batch --reason outdated
```
"#;

const HELP_BATCH: &str = r#"# Batch Operations

Create multiple facts atomically with `$N` back-references.

## Recording related findings

```bash
damask batch --stdin <<'EOF'
[
  {"span": {"path":"src/auth/token.rs", "start":142, "end":178, "symbol":"verify_token"}},
  {"span": {"path":"src/auth/config.rs", "start":22, "end":35, "symbol":"TOKEN_CONFIG"}},
  {"edge": {"from":"$0", "to":"$1", "rel":"depends_on", "payload":{
    "summary":"verify_token() reads signing key from TOKEN_CONFIG but does not validate config is loaded",
    "confidence":0.85,
    "tags":["initialization","auth"],
    "action":"Add config-loaded assertion at top of verify_token()"
  }}},
  {"edge": {"from":"$0", "to":"_", "rel":"risk", "payload":{
    "summary":"verify_token() can silently accept any token if called before config init",
    "confidence":0.80,
    "status":"hypothesis",
    "tags":["security","initialization"]
  }}}
]
EOF
```

## Recording contradictions

```bash
damask batch --stdin <<'EOF'
[
  {"span": {"path":"src/config.rs", "start":45, "end":52}},
  {"span": {"path":"src/server.rs", "start":112, "end":118}},
  {"edge": {"from":"$0", "to":"$1", "rel":"contradicts", "payload":{
    "summary":"Config declares max_connections=100 but server.rs hardcodes pool size to 50",
    "confidence":0.90,
    "tags":["config","database"],
    "action":"Align pool size with config value"
  }}}
]
EOF
```

## Rules
- `$N` refers to the fact at index N (zero-based, must reference an earlier item)
- All-or-nothing: validates everything before writing
- Also accepts `--file batch.json` instead of `--stdin`
"#;

const HELP_WHERE: &str = r#"# Filtering with `where`

Predicates support: `=`, `!=`, `>`, `<`, `>=`, `<=`, `~` (contains).

Filterable fields: `rel`, `ns`, `agent`, `endorsed`, `disputed`, `confidence`, `status`, `summary`, `tags`, `lifecycle`.

The `lifecycle` virtual field is computed from edge state:
- `active` — no endorsements or disputes
- `endorsed` — has endorsements
- `disputed` — has disputes
- `superseded` — inactive
- `closed` — explicitly closed

Multiple predicates are AND-composed.

## Examples

```bash
damask where "rel=risk"                          # exact match
damask where "rel=risk" "tags~security"          # AND: risks with security tag
damask where "rel=risk" "lifecycle=active"       # unresolved risks
damask where "rel!=describes"                    # negation
damask where "confidence>=0.9"                   # numeric comparison
damask where "endorsed>0"                        # at least one endorsement
damask where "disputed=true"                     # boolean
damask where "tags~auth"                         # contains
damask where "tags=security"                     # exact tag match
damask where "summary~SQL"                       # substring in summary
damask where "lifecycle=superseded"              # superseded edges
damask where "rel=risk" --since 2025-06-01       # temporal filter
```

Unknown fields produce helpful errors listing valid fields and examples.

## Pagination

```bash
damask where "rel=risk" --limit 10               # first 10
damask where "rel=risk" --limit 10 --offset 10   # next 10
```

JSON output includes: `{"showing": {"total": 57, "offset": 10, "limit": 10, "count": 10}}`
"#;

const HELP_RELS: &str = r#"# Relationship Types

| Rel | Class | When to use | from → to |
|-----|-------|-------------|-----------|
| risk | Judgment | Security, correctness, or reliability risks | span → null |
| gotcha | Judgment | Non-obvious pitfalls | span → null |
| decision | Judgment | Architectural choices and rationale | span → null |
| contradicts | Judgment | Two code regions that conflict | span → span |
| ruled_out | Judgment | Alternatives considered and rejected | span → null |
| conflicts_with | Judgment | Semantic conflicts between components | span → span |
| depends_on | Descriptive | Runtime, build, or init dependencies | span → span |
| supports | Descriptive | Evidence supporting a claim | span → edge |
| describes | Descriptive | Documentation-like annotations | span → null |
| derived_from | Descriptive | Lineage / provenance | span → span |
| co_change | Descriptive | Files that must change together | span → span |
| implements | Descriptive | Code that implements a spec/design | span → span |
| env | Descriptive | Environment-specific behavior | span → null |
| perf | Descriptive | Performance characteristics | span → null |

Custom rel types are allowed and rank between Judgment and Descriptive.

**When to use relational edges (span → span)**: If your finding is about how two pieces of code interact — a dependency, contradiction, co-change requirement — link them. Future agents traversing with `damask follow` will discover these connections.
"#;

const HELP_COLD_START: &str = r#"# Cold Start Playbook

When `damask orient` reports an empty graph, do a structured first pass. Aim for breadth over depth — flag things for later investigation.

## 1. Identify the skeleton
Read top-level directory, build files, and any README/ARCHITECTURE docs. Record `describes` edges for:
- Entry points (main, server start, CLI dispatch)
- Module/package boundaries and their responsibilities
- Build targets and how they relate

## 2. Trace the critical paths
Skim for areas that matter most. Record `risk` or `gotcha` edges for:
- Error handling patterns (swallowed errors, panics, missing propagation)
- Data flow boundaries (validation, transformation, serialization)
- Concurrency and shared mutable state
- Configuration and initialization ordering
- Domain-specific invariants the code assumes but doesn't check

## 3. Map key dependencies
Record `depends_on` edges between components that must coordinate:
- Database/storage access patterns
- External service calls and their failure modes
- Initialization ordering constraints

## 4. Note architectural decisions
Record `decision` edges for choices visible in the code:
- Framework/library selections (especially non-obvious ones)
- Patterns used (middleware chains, plugin systems, event buses)
- Anything with a comment explaining "why"

## 5. Batch it
Combine findings into a single `damask batch` call:

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
    "summary":"Auth middleware skips verification for SKIP_AUTH_ROUTES",
    "confidence":0.8, "action":"Audit SKIP_AUTH_ROUTES for overly broad patterns",
    "tags":["security","auth"], "status":"hypothesis"
  }}}
]
EOF
```

After the batch, run `damask orient` again to verify the graph has useful content.
"#;

const HELP_PATTERNS: &str = r#"# Advanced Patterns

## Loading Census

Catalog every moment a user waits. Tag by strategy and location:

```bash
damask record src/pages/Chat.tsx 112 130 describes \
  '{"summary":"[Send Message]: progressive token streaming via rAF","confidence":0.95,"tags":["loading-census","streaming","mutation"]}' \
  --symbol handleSend
```

**Strategy tags**: `nothing` (blank), `spinner` (blocking), `frozen`, `progressive`, `streaming`.
**Location tags**: `auth`, `page-load`, `list-load`, `detail-load`, `mutation`, `file-op`, `search`.

Query: `damask where "tags~loading-census"`, `damask where "tags~nothing"`.

## Undo Archaeology

Audit destructive actions for protection level. Use `contradicts` to flag inconsistencies:

```bash
damask batch --stdin <<'EOF'
[
  {"span": {"path":"src/pages/Chat.tsx", "start":89, "end":105, "symbol":"handleDelete"}},
  {"edge": {"from":"$0", "to":"_", "rel":"describes", "payload":{
    "summary":"[Delete Conversation]: ConfirmDialog with danger variant",
    "confidence":0.95, "tags":["undo-archaeology","confirmed","delete"]
  }}},
  {"span": {"path":"src/components/Canvas.tsx", "start":201, "end":208, "symbol":"removeNode"}},
  {"edge": {"from":"$2", "to":"_", "rel":"risk", "payload":{
    "summary":"[Delete Node]: no confirmation — removed on single click",
    "confidence":0.95, "tags":["undo-archaeology","unprotected","delete"]
  }}},
  {"edge": {"from":"$0", "to":"$2", "rel":"contradicts", "payload":{
    "summary":"Inconsistent protection for same action class",
    "confidence":0.95, "tags":["undo-archaeology","consistency"]
  }}}
]
EOF
```

**Protection tags**: `confirmed`, `unprotected`, `undoable`.

## Error Personality

Audit error tone consistency:

**Tone tags**: `tone-apologetic` (empathetic), `tone-blunt` (bare error), `tone-technical` (leaks internals).

## Flow Tracing with `depends_on` Chains

Model multi-step flows as chains of `depends_on` edges, then traverse with `damask follow`:

```bash
damask batch --stdin <<'EOF'
[
  {"span": {"path":"src/api/chat.ts", "start":45, "end":58, "symbol":"sendMessage"}},
  {"span": {"path":"src/server/routes/chat.py", "start":112, "end":145, "symbol":"chat_stream"}},
  {"edge": {"from":"$0", "to":"$1", "rel":"depends_on", "payload":{
    "summary":"sendMessage() calls /api/chat via SSE",
    "confidence":0.95, "tags":["flow","chat-send"]
  }}}
]
EOF
```

## Compound Tag Taxonomies

Design tags as `[audit-type, classification, location]` for cross-cutting queries:

```
tags: ["loading-census", "spinner", "page-load", "auth"]
tags: ["undo-archaeology", "unprotected", "delete", "pipeline"]
```

Each dimension filterable independently via `damask where "tags~..."`.
"#;

const HELP_QUALITY: &str = r#"# Writing High-Quality Annotations

1. **Be specific**: Pin narrow line ranges, not whole files. 5 lines > 200 lines.
2. **Use symbols**: `--symbol fn_name` survives refactoring better than line numbers.
3. **Set confidence honestly**: 0.95 means "I traced the code path and verified this."
4. **Separate confidence from severity**: A 0.6 confidence + catastrophic impact is worth recording. Use `impact` for severity; `confidence` only for certainty.
5. **Include reasoning**: A future agent should understand *why*, not just *what*.
6. **Make actions concrete**: "Add expiry check at line 155" not "fix authentication."
7. **Link related code**: Use `depends_on`, `contradicts`, `co_change` to connect regions.
8. **Record negative evidence**: `status: "ruled_out"` saves future agents from re-investigating.
9. **Tag consistently**: Lowercase, hyphenated.
10. **Endorse what you verify**: Endorsements separate noise from knowledge.
11. **Dispute rather than ignore**: A disputed edge with rationale > silence.
"#;

const HELP_HOOKS: &str = r#"# Agent Hooks (Claude Code)

`damask init --claude` installs two hooks into `.claude/settings.json` so the
knowledge loop runs without agent discipline:

## SessionStart → `damask briefing`

Injects a compact markdown digest of the graph into the agent's context at
the start of every session (warm start). Top findings per rel type, recent
activity, and query pointers — capped to a small token budget. Silent when
no `.damask/` exists, so it is safe to run anywhere.

```bash
damask briefing                  # raw markdown (hook stdout is injected)
damask briefing --format json    # SessionStart hookSpecificOutput envelope
```

## PostToolUse / UserPromptSubmit → `damask peek`

Point-of-use injection. After the agent reads or edits a file, the top
ranked edges for that file are injected as context — knowledge arrives at
the exact moment it matters. On each user prompt, the prompt's keywords are
matched against the FTS index and relevant edges injected before exploration
starts. A per-session seen-cache (`.damask/.session/`) guarantees each edge
is injected at most once per session.

```bash
damask peek --file src/auth.rs --session s1   # manual file-mode run
damask peek --prompt "auth timeout"           # manual prompt-mode run
```

## Stop → `damask harvest`

Reads the Stop hook JSON from stdin, scans the session transcript, and — if
the agent edited files but ran no damask write command — blocks the stop
once with a nudge listing the touched files and what the graph already knows
about them. The agent records durable findings (or simply finishes if there
is nothing durable). If the session DID record, the new edges are linted
instead, and serious quality problems (empty payloads, missing summaries)
trigger one fix-it nudge. Guaranteed single-shot: `stop_hook_active`
prevents re-blocking, and every error path allows the stop.

```bash
damask harvest --transcript session.jsonl   # manual run against a transcript
```

## Installed configuration

```json
{
  "hooks": {
    "SessionStart": [
      {"matcher": "startup|resume|clear",
       "hooks": [{"type": "command", "command": "damask briefing"}]}
    ],
    "PostToolUse": [
      {"matcher": "Read|Edit|Write|MultiEdit|NotebookEdit",
       "hooks": [{"type": "command", "command": "damask peek"}]}
    ],
    "UserPromptSubmit": [
      {"hooks": [{"type": "command", "command": "damask peek"}]}
    ],
    "Stop": [
      {"hooks": [{"type": "command", "command": "damask harvest"}]}
    ]
  }
}
```

All entries merge non-destructively into existing settings and are
idempotent. Remove entries from `.claude/settings.json` to disable.

## Provenance

Facts written from a Claude Code session are stamped `agent: claude-code`
and `session: <session id>` automatically. Override with `DAMASK_AGENT` /
`DAMASK_SESSION` env vars. Query by author: `damask where "agent=claude-code"`.

## CI: post new annotations on a PR

```yaml
- run: damask review --markdown > damask-review.md
- run: gh pr comment "$PR_NUMBER" --body-file damask-review.md
```

## Verifiable claims

Give an edge payload a `check` field (a shell command); `damask verify`
re-runs every check and reports pass/fail, `damask verify --auto` endorses
passes and disputes failures (once per outcome). Checks run with `sh -c`
from the repo root — same trust level as a Makefile. Run from CI to keep
mechanically-checkable knowledge calibrated.

```bash
damask record src/db.rs 10 20 risk \
  '{"summary":"Pool size unbounded","confidence":0.9,"check":"grep -q MAX_CONNECTIONS src/db.rs && exit 1 || exit 0"}'
damask verify --auto
```
"#;
