# Damask: A Knowledge Fabric for Agents

**Version:** 0.7 (Draft)
**Date:** February 2025
**Authors:** Mike Renwick, with Claude

---

## 1. What Damask Is

Damask is a knowledge fabric that agents weave over information.

When an agent works with any collection of files — code, documents, research, contracts, notes, media — it discovers relationships that aren't obvious from reading any single file. Dependencies between components. Contradictions between documents. Decisions and their rationale. Failure modes learned through experience. Context that connects disparate pieces of information into a coherent understanding.

Today, all of that discovered understanding evaporates when the session ends. The next agent starts from zero.

Damask captures those discoveries as structured edges between spans of content, stored as simple fact files alongside the content itself. The knowledge accumulates over time, across sessions, across agents. Each agent that touches the material inherits what previous agents learned, adds its own discoveries, and leaves the damask richer than it found it.

The name comes from damask fabric — a reversible, patterned textile where the design is woven into the structure without altering the underlying material. Damask weaves understanding into information without modifying it.

---

## 2. The Problem

### 2.1 Agent amnesia

Modern AI agents are brilliant but amnesiac. An agent can explore a codebase, analyze a contract, review a research corpus, or audit a dataset — building a rich mental model of relationships and meaning — and then the context window closes and every insight is lost. The next session pays the same exploration tax. Every agent re-derives the same understanding from scratch.

This isn't a memory problem in the traditional sense. Agents have memory tools — markdown files, vector stores, conversation history. The problem is that these memories are disconnected from the content they describe. A note saying "clause 12 conflicts with GDPR Article 82" floats in a separate file, unlinked to either document. Search might find it. But there's no machine-navigable connection between the note and the specific passages it refers to.

### 2.2 What agents already do (and what's missing)

Agents already accumulate knowledge informally. They write to CLAUDE.md, MEMORY.md, `.context/` folders, daily notes. The behavior exists — agents naturally want to persist what they've learned.

But these markdown notes are unstructured prose floating in space. For a single agent on a small project, this is 80% as good as structured data — the agent reads the prose and understands it. But it breaks at scale: multiple agents producing overlapping notes, hundreds of findings across hundreds of files, no way to query by location, no freshness tracking, no cross-file traversal.

Damask doesn't replace markdown notes. Agents should keep writing them. Damask adds the structured, queryable, grounded layer that prose can't provide, and connects those notes back to the content they discuss through span links.

### 2.3 The missing layer

Agents have good tools for reading content and reasoning about content. What they lack is a substrate for accumulating understanding about content — a place where discoveries persist, linked to the specific material they describe, queryable by future agents.

The filesystem holds the content. The damask holds what agents have learned about the content. The understanding lives in the spaces in between.

### 2.4 Why now

Previous attempts at structured knowledge layers (Semantic Web, RDF, Linked Data) failed because they required humans to manually create and maintain structured metadata. Humans are inconsistent about metadata — every wiki, knowledge base, and tagging system eventually rots.

Agents will meticulously create edges, maintain consistent conventions, and build on each other's work — because they follow instructions literally and tirelessly. Agents are the first users disciplined enough to actually weave and maintain a knowledge fabric.

---

## 3. What Damask Is Not

Damask has a deliberately narrow scope. Clarity about what it is not prevents scope creep and keeps the substrate thin.

**Not a replacement for documentation.** Agents should still write READMEs, architecture docs, and inline comments. Damask links those documents to the content they describe, but doesn't replace the prose.

**Not a global ontology.** Damask does not prescribe a universal schema for all knowledge. Vocabulary is emergent, with conventions that communities develop. The substrate is neutral.

**Not a server or SaaS.** No daemon, no cloud dependency, no API keys, no account. A CLI binary and a folder of text files.

**Not a vector database.** Vector stores are good for fuzzy retrieval. Damask is good for precise, typed, bidirectional relationships. They are complementary, not competing.

**Not a replacement for linters, type checkers, or static analysis.** Tools that mechanically derive facts from source material should continue doing so. Damask captures what those tools cannot: cross-file judgment, failure modes, decision rationale, environmental context.

**Not an agent framework.** Damask doesn't orchestrate agents, manage sessions, or define workflows. It's a data format and a CLI. Any agent that can shell out can use it.

---

## 4. Historical Context

### 4.1 Relationship to the World Wide Web

Tim Berners-Lee's 1989 proposal, "Information Management: A Proposal," was written to solve a specific problem at CERN: when scientists left, their knowledge about the systems they worked on vanished. His solution was a system of linked documents — hypertext — that allowed people to navigate between related information.

Damask addresses the same problem for a new era: when agent sessions end, their understanding of the material they worked on vanishes. The structural parallels are direct:

| | WWW (1989) | Damask (2025) |
|---|---|---|
| **The problem** | People leave, institutional knowledge evaporates | Agent sessions end, discovered understanding evaporates |
| **The insight** | Link documents so knowledge is navigable | Link spans across files so knowledge is traversable |
| **Node** | A document (whole page) | A span (region within any file) |
| **Link** | Untyped, one-directional, no content | Typed, bidirectional, carries JSON payload |
| **Address** | URL (`http://host/path`) | Root-relative path + coordinates |
| **Protocol** | HTTP (client-server) | CLI (shell commands, stdin/stdout) |
| **Format** | HTML | JSONL (one fact per line) |
| **Distribution** | HTTP servers, DNS | Git push/pull |
| **Who creates links** | Humans, manually | Agents, as byproduct of work |
| **Persistence** | Server-dependent (link rot) | In git alongside content |

### 4.2 Where the web falls short

The web succeeded because it was simple. But its linking model has known limitations that Damask addresses:

**Links are unidirectional.** If I link to your page, you don't know about it. Damask edges are bidirectional — you can always ask "what links to this?"

**Links are untyped.** A hyperlink says "related" but not how. Damask edges carry a relationship type and a payload explaining why.

**Links are document-level.** URLs point to pages. Fragment identifiers were an afterthought. Damask spans are line-precise, pixel-precise, or millisecond-precise.

**Links carry no content.** A hyperlink is a bare pointer. Damask edges carry knowledge — the reason, context, and evidence for the connection.

**Links rot.** URLs depend on servers staying alive. Damask edges live in git alongside the content they reference.

### 4.3 The Semantic Web's failure

The Semantic Web (RDF, OWL, Linked Data) was Berners-Lee's attempt to add structured, typed links to the web. It failed because it required humans to manually create and maintain structured metadata. The tooling was complex, the formats were hostile, and the effort wasn't rewarded with immediate value.

Damask avoids these mistakes: agents create the links, not humans. The format is trivial (JSONL, not RDF/XML). Value is immediate (an agent benefits from the damask on its next session). The protocol is the shell (not SPARQL).

### 4.4 Obsidian and personal knowledge

Obsidian proved that people want a linked knowledge fabric — its graph view of connected notes is the feature people love most. But Obsidian's constraint is that humans have to create and maintain `[[links]]`. In practice, most vaults have sparse, inconsistent linking.

Damask has the same graph structure but agents do the linking. They don't get tired, don't forget, don't get overwhelmed by scale. An agent damasking a personal knowledge base would create the densely linked, richly typed graph that Obsidian promises but humans rarely deliver.

---

## 5. Design Principles

### 5.1 The Bitter Lesson

Rich Sutton's Bitter Lesson teaches that general methods leveraging computation beat hand-engineered structure. Applied to Damask: don't build domain-specific annotators into the substrate. The agent is the annotator — whether it's analyzing code, reviewing contracts, or mapping research literature.

Damask provides the thinnest possible infrastructure — spans and edges — and lets agents create whatever annotations they need. The protocol prescribes the structure. The vocabulary is emergent (with recommended conventions to bootstrap consistency).

### 5.2 RISC over CISC

Fewer, orthogonal primitives that compose well beat many specialized operations. Damask has two data types and a small set of operations. Everything else — tags, annotations, links, memory, cross-references — is composed from these primitives.

### 5.3 The Foundation Principle (Asimov)

Hari Seldon built the substrate that preserved coherence through collapse. Damask is designed to survive technology transitions, company deaths, and protocol evolution over decades:

- Text-based format (human-readable, diff-friendly)
- Append-only fact files (never require migration)
- Zero external dependencies (no server, no cloud, no API keys)
- Rides on git (the most successful version control system ever built)
- Root-relative paths (resolve in any clone)

### 5.4 The Hyperion Principle

In Dan Simmons' Hyperion Cantos, the TechnoCore AIs live in the transmission medium — the spaces between the farcaster portals. The value isn't in the nodes, it's in the connections.

Damask's value isn't in the files (agents can already read those). It's in the edges — the relationships between spans that represent discovered understanding. The filesystem holds the nouns. The damask holds the verbs.

### 5.5 Discoveries, not observations

An edge is valuable if re-deriving it costs more than reading it. Agents should annotate what surprised them — cross-file relationships, failure modes, contradictions, decision rationale, environmental context. If a machine can derive it mechanically from the source material, it shouldn't be an edge. Edges are for things that require judgment.

This is the **surprise heuristic** — a first-class design principle, not just prompt guidance. Every aspect of the system reinforces it: `damask lint` flags restatements, the ranking policy down-ranks observations, and the system prompt convention teaches agents to ask "would the next agent need to re-derive this?" before creating an edge.

### 5.6 Domain-agnostic from day one

The WWW was built at CERN for physicists but nothing about HTTP, URLs, or HTML was physics-specific. Similarly, Damask launches with code and text as its first domain, but nothing about spans, edges, JSONL, or the CLI is code-specific. The same primitives work for legal documents, research papers, business data, personal notes, media files, and anything else an agent works with.

### 5.7 Convergent verification

A single agent's edge is a claim. The same edge independently endorsed by multiple agents working on unrelated tasks is knowledge.

As agent density increases — from single sessions to swarms of hundreds or thousands across worktrees — the primary epistemic mechanism shifts from individual judgment to convergent independent verification. Damask's primitives (append-only edges, endorsements, decay) are designed so that this transition happens naturally, without protocol changes, as scale increases. The noise of many agents becomes the signal.

---

## 6. Data Model

### 6.1 Two primitives

Damask has exactly two data types:

**Span:** A reference to a region within a file.

```json
{
  "id": "s_01JKXYZ...",
  "path": "src/auth.py",
  "lines": [42, 67],
  "snippet": "def validate_token(token):",
  "symbol": "validate_token",
  "content_hash": "a3f7c2..."
}
```

- `id` is a ULID prefixed with `s_` (globally unique, sortable by time, merge-proof)
- `path` is root-relative (relative to the repo/project root, or package-relative for external references)
- `lines` is a line range (1-indexed, inclusive) — the default coordinate for text content
- `snippet` is a short text excerpt for fuzzy re-anchoring when lines shift
- `symbol` (optional) provides a semantic anchor (function name, section heading, clause number) that survives reformatting
- `content_hash` (optional) is a truncated SHA-256 of the span text, providing a durable content-derived anchor that survives line shifts
- Other coordinate systems for non-text content (see section 9)
- Spans are cheap to create and expected to drift as content changes

**Edge:** A relationship between spans, or between a span and a value, carrying a JSON payload.

```json
{
  "id": "e_01JKXYZ...",
  "from": "s_01JKXYZ...",
  "to": "s_01JKXYZ...",
  "rel": "conflicts_with",
  "payload": {
    "summary": "Liability cap may conflict with GDPR unlimited liability",
    "confidence": 0.9,
    "evidence": ["s_01JKXYZ..."],
    "action": "negotiate removal of cap for data protection claims",
    "severity": "review required",
    "reference": "GDPR Article 82"
  }
}
```

- `id` is a ULID prefixed with `e_` (globally unique, merge-proof)
- `from` and `to` are span or edge IDs (either or both can be null)
- `rel` is a string describing the relationship type
- `payload` is JSON — the actual knowledge (see §6.3 for envelope conventions). Empty payloads (`{}`) are valid JSON and accepted, but flagged by `damask lint` — an edge without a payload is a pointer without knowledge.
- The payload is the most durable part of the edge; it should carry enough context to be useful even if the spans can no longer be resolved

### 6.2 Edge as universal primitive

A tag is just an edge with one endpoint:

| Concept | from | to | rel | payload |
|---|---|---|---|---|
| Tag | span | null | `"risk"` | `{"summary":"…","confidence":0.9}` |
| Annotation | span | null | `"summary"` | `{"summary":"…"}` |
| Link | span | span | `"depends_on"` | `{}` |
| Rich link | span | span | `"conflicts_with"` | `{"summary":"…","severity":"…"}` |
| Note | null | null | `"observation"` | `{"summary":"…"}` |
| Meta-edge | edge | null | `"confidence"` | `{"score":0.85}` |

Tags, annotations, links, notes, and meta-annotations are all the same primitive. One data type, one storage format, one query model.

**Edge-to-edge constraint:** Edge-to-edge relationships should be limited to meta-properties (`supersedes`, `invalidates`, `endorsed`, `disputed`). Domain semantics should attach to spans, not other edges. `damask lint` will flag edge-to-edge links with domain-specific rel types as a code smell. This prevents deep annotation stacks and accidental graph spaghetti. Specifically: `endorsed` and `disputed` MUST reference an edge ID in their `from` field and MUST NOT reference spans directly — they are always reactions to existing edges, not independent claims about content.

### 6.3 Payload envelope

Payloads are arbitrary JSON. However, a small set of conventional keys enables consistent tooling, querying, and display across all edge types. These are not required — but `damask lint` will suggest them when absent.

| Key | Type | Purpose |
|---|---|---|
| `summary` | string | One-line human-readable description of the edge. **Strongly recommended.** This is what `damask at` displays. |
| `confidence` | number (0–1) | How certain the agent is about this edge (see §10.3 for semantics). |
| `status` | string | `"assertion"` (default if omitted), `"hypothesis"` (uncertain, under investigation), or `"ruled_out"` (investigated, not a problem). |
| `evidence` | array | Span IDs or short text snippets supporting the claim. |
| `action` | string | What should be done about this (if anything). |
| `tags` | array of strings | Freeform labels for filtering. |

**`status` vs `ruled_out` rel type:** These are orthogonal. `ruled_out` as a rel type is for edges whose primary purpose is recording closure ("we checked this, it's fine"). `status: "hypothesis"` is for any edge type where the agent is uncertain. A `risk` edge can have `status: "hypothesis"` if the agent suspects but hasn't confirmed the risk. A `ruled_out` edge always has `status: "assertion"` because the act of ruling out is itself a firm conclusion.

Agents may add any additional keys beyond the envelope. The envelope keys enable:

- `damask at` to show one-line summaries immediately
- `damask where confidence>0.8` to filter by certainty
- `damask lint` to flag edges missing a summary
- Ranking in point queries (higher confidence, actionable edges surface first)

### 6.4 Edge provenance

Every edge carries provenance metadata:

```json
{
  "id": "e_01JKXYZ...",
  "from": "s_01JKXYZ...",
  "to": "s_01JKXYZ...",
  "rel": "conflicts_with",
  "payload": {
    "summary": "Liability cap vs GDPR unlimited liability",
    "confidence": 0.9,
    "action": "negotiate removal"
  },
  "ns": "contract-review",
  "ts": "2025-01-15T10:30:00Z",
  "agent": "claude-opus-4-6",
  "session": "abc123"
}
```

- `ns` (namespace): groups edges by task or perspective (also determines which file the edge is written to)
- `ts` (timestamp): when the edge was created
- `agent`: which agent (or human) created it
- `session`: which session/run produced it

### 6.5 Recommended vocabulary

While `rel` types are freeform, a recommended vocabulary bootstraps consistency. This is a starting set — communities and domains will extend it.

**Universal (any domain)**

| rel type | When to use |
|---|---|
| `depends_on` | A depends on B to function correctly |
| `contradicts` | A and B make conflicting claims or assumptions |
| `supports` | A provides evidence or rationale for B |
| `describes` | A is a note/commentary about B |
| `supersedes` | This edge replaces an older one (meta-edge: from=new, to=old) |
| `invalidates` | This supersession was wrong; reinstate the original (meta-edge) |
| `endorsed` | Agent independently verified this edge during its work (meta-edge) |
| `disputed` | Agent found evidence this edge is wrong or outdated (meta-edge) |
| `derived_from` | A was created based on B |
| `co_change` | A and B must be updated together |

**Analysis and review**

| rel type | When to use |
|---|---|
| `risk` | Something dangerous, vulnerable, or fragile |
| `gotcha` | A non-obvious failure mode or trap |
| `decision` | Why a choice was made (what was chosen, what was rejected) |
| `perf` | Performance characteristics, scaling limits |
| `env` | Environmental assumptions or requirements |
| `implements` | Links to an external spec, standard, requirement, or ticket |
| `ruled_out` | Investigation concluded this is NOT a problem (prevents re-investigation) |

**Domain-specific (examples)**

| Domain | rel types |
|---|---|
| Legal | `amends`, `supersedes`, `conflicts_with`, `governed_by` |
| Research | `cites`, `replicates`, `contradicts`, `extends` |
| Business | `projects`, `measures`, `caused_by`, `mitigates` |
| Medical | `diagnoses`, `treats`, `contraindicated_with` |

Using canonical rel types enables queries like `damask where rel=contradicts` to work consistently across projects and community packages.

---

## 7. Storage Format

### 7.1 Edge directory

The canonical storage format is a folder of JSONL files at `.damask/edges/` in the project root. Each namespace gets its own file. Each line in each file is a self-contained fact.

```
.damask/
├── edges/
│   ├── security-audit.jsonl       ← committed to git
│   ├── onboarding.jsonl           ← committed to git
│   ├── contract-review.jsonl      ← committed to git
│   ├── literature-survey.jsonl    ← committed to git
│   ├── community-react.jsonl      ← pulled from damask-community/react
│   ├── .views/
│   │   ├── security-audit.current.jsonl  ← materialized current state
│   │   └── contract-review.current.jsonl
│   └── .private/
│       ├── security-findings.jsonl  ← gitignored (sensitive)
│       └── privileged-review.jsonl  ← gitignored (sensitive)
├── index.db                        ← gitignored (derived from edge files)
└── config.json                     ← optional settings
```

### 7.2 Fact format

Each line is a self-contained JSONL fact:

```jsonl
{"t":"span","id":"s_01JKX1A...","path":"src/auth.py","lines":[42,67],"snippet":"def validate_token(token):","symbol":"validate_token","content_hash":"a3f7c2","ns":"security-audit","ts":"2025-01-15T10:30:00Z","agent":"claude-opus-4-6"}
{"t":"edge","id":"e_01JKX1B...","from":"s_01JKX1A...","to":null,"rel":"risk","payload":{"summary":"No token expiry check","confidence":0.95,"action":"Add expiry validation","level":"high","cvss":9.1},"ns":"security-audit","ts":"2025-01-15T10:30:02Z","agent":"claude-opus-4-6"}
```

### 7.3 Format properties

- **Append-only:** Facts are never modified or deleted. New facts supersede old facts. The log only grows.
- **Self-contained:** Each line is independently parseable. No external schema required.
- **Text-based:** Human-readable, diff-friendly, git-friendly.
- **Ordered:** Facts are appended chronologically within each file.
- **Mergeable:** Merging two fact files is concatenation followed by sort-by-timestamp. No conflicts possible because facts are independently true.

### 7.4 Why a folder of files

A folder of JSONL files (one per namespace) instead of a single file:

- **Merge conflicts vanish:** Two agents working in different namespaces write to different files.
- **Private edges are a gitignore rule:** `.damask/edges/.private/` is gitignored by default. Sensitive findings never leave the machine.
- **Community packages are files:** `damask pull damask-community/react` drops a file into the edges folder. Remove it by deleting the file.
- **Sessions can be files:** An agent session produces one file, easy to review, revert, or attribute.
- **Compaction is natural:** `damask compact security-audit` produces a current-state file, archives superseded edges.
- **Scale:** Each namespace grows independently.

The damask CLI reads every `*.jsonl` in `.damask/edges/` recursively. The filename is informational for humans; the `ns`, `ts`, and `agent` fields in each fact are what the system uses.

### 7.5 Why JSONL

- Every language has a JSON parser
- Every developer can read it
- Git diffs show each added edge as a visible line
- Streaming-friendly (process line by line)
- Will be parseable in 50 years (it's just text)

### 7.6 Current-state resolution

The fact log is append-only. Queries need current truth, not archaeology.

#### The event model

Edge IDs are **immutable and unique.** There is no "edit" operation. There is no "re-emit with the same ID." Every update creates a new edge with a new `e_...` ID.

**Supersession** is a relationship, not an overwrite. To update an understanding, create a new content edge (with its own `from`/`to` span references) and a supersession meta-edge linking the new to the old:

```jsonl
{"t":"edge","id":"e_01JKX1B...","from":"s_01JKX1A...","to":null,"rel":"risk","payload":{"summary":"No token expiry check","confidence":0.95},"ns":"security-audit","ts":"2025-01-15T10:30:00Z","agent":"claude-opus-4-6"}
{"t":"edge","id":"e_01JKX9Y...","from":"s_01JKX1A...","to":null,"rel":"risk","payload":{"summary":"Token expiry added but rotation still missing","confidence":0.85},"ns":"security-audit","ts":"2025-01-20T14:00:00Z","agent":"claude-opus-4-6"}
{"t":"edge","id":"e_01JKX9Z...","from":"e_01JKX9Y...","to":"e_01JKX1B...","rel":"supersedes","payload":{"summary":"Updated after partial fix"},"ns":"security-audit","ts":"2025-01-20T14:00:01Z","agent":"claude-opus-4-6"}
```

Three facts: the original edge, the replacement edge, and a meta-edge declaring the supersession. The original remains in the log, historically true but inactive.

**Content edges always carry their own `from`/`to`.** A new edge must specify which spans it relates to, even if they're the same spans as the superseded edge. This keeps `damask at` fast — point queries never need to chase supersession chains to find endpoints. If spans have moved, the new edge points to the new locations.

**Rule:** Supersession never modifies an existing fact. A new fact asserts that it supersedes a previous fact by ID. This keeps append-only semantics sacred.

#### Current-state algorithm

Four categories of edges:

1. **Content edges** (any `rel` other than `supersedes`, `invalidates`, `endorsed`, or `disputed`): Active if not the `to` target of any `supersedes` meta-edge.
2. **Supersedes meta-edges** (`rel: "supersedes"`): Structural markers. They don't appear in `damask at` output but determine which content edges are active. A supersedes edge is effective unless it is itself the `to` target of an `invalidates` meta-edge.
3. **Invalidates meta-edges** (`rel: "invalidates"`): Override mistakes. If a supersession was wrong, an `invalidates` edge pointing to the supersedes meta-edge reinstates the original content edge.
4. **Feedback meta-edges** (`rel: "endorsed"` or `"disputed"`): Quality signals. They don't affect active/inactive status but influence ranking weight in `damask at` (see §10.3 ranking policy).

**Endorsements and disputes never change edge truth, active status, or historical record. They only influence ranking and visibility.** This invariant is sacred — Damask must never become a voting system. Supersession remains the only mechanism by which truth changes.

The algorithm:

```
for each content edge E:
    supersedes_edges = all edges where rel="supersedes" AND to=E.id
    effective_supersedes = [s for s in supersedes_edges
                           if no edge exists where rel="invalidates" AND to=s.id]
    E.active = (len(effective_supersedes) == 0)
```

No recursion. No cycles. Deterministic.

#### Conflict resolution

When two edges supersede the same prior edge (a merge scenario), both are active. This is intentional — competing supersessions represent disagreement, which is information. `damask diff` surfaces these conflicts. Resolution is a human or agent decision, not an automatic merge rule.

#### Query defaults

- `damask at`, `damask where`, and `damask follow` default to **current-state view** — only active content edges.
- `--history` flag shows the full log including superseded edges.
- `damask compact <namespace>` materializes the current state into `.damask/edges/.views/<namespace>.current.jsonl`. The CLI reads view files when present, falling back to raw logs.

**Compaction semantics:** Compaction archives superseded content edges and their supersedes meta-edges. If a supersedes meta-edge has been invalidated, it is also archived, and the original content edge is reinstated as active in the view. The view file contains only the current-state edges — active content edges and any unresolved invalidation chains.

This means users almost never see a wall of historical edges. The default experience is clean and current. History is preserved in the append log and accessible when needed.

### 7.7 Recency decay

An edge's effective ranking weight decreases gradually with time since creation or last endorsement. Decay is a ranking concern, not a storage concern — it never deletes edges or modifies the append-only log.

**Decay mechanics:**

- An endorsement resets the content edge's decay clock — knowledge that agents keep confirming stays fresh
- A dispute does not reset the decay clock — disputes flag attention, they don't refresh trust
- An unendorsed edge from 12 months ago ranks below a recently-endorsed edge from last week, even at the same confidence
- Decay applies to content edges only — meta-edges (`endorsed`, `disputed`, `supersedes`, `invalidates`) do not decay

**Decay rate is per-namespace.** Different domains have different rates of change. A fast-moving codebase needs aggressive decay; a legal corpus or research survey needs slow decay. The decay half-life is configurable in `config.json` per namespace:

```json
{
  "namespaces": {
    "security-audit": {
      "decay_half_life_days": 90,
      "description": "Code changes fast; unverified findings go stale quickly"
    },
    "contract-review": {
      "decay_half_life_days": 365,
      "description": "Legal documents change slowly; findings stay relevant longer"
    },
    "literature-survey": {
      "decay_half_life_days": 730,
      "description": "Research findings are durable"
    }
  },
  "default_decay_half_life_days": 180
}
```

The default half-life is 180 days (6 months). Namespaces without explicit configuration use the default. Setting `decay_half_life_days` to `null` disables decay for that namespace entirely.

### 7.8 Index

For query performance, Damask maintains a SQLite index at `.damask/index.db`. This is a derived artifact — always rebuildable from the edge files.

```sql
CREATE TABLE spans (
    id            TEXT PRIMARY KEY,
    path          TEXT NOT NULL,
    line_start    INTEGER,
    line_end      INTEGER,
    snippet       TEXT,
    symbol        TEXT,
    content_hash  TEXT,
    ns            TEXT NOT NULL,
    ts            TEXT NOT NULL,
    agent         TEXT,
    resolution    TEXT DEFAULT 'exact',       -- exact | relocated | unresolved
    recency       TEXT DEFAULT 'unchanged'    -- unchanged | file_changed | unknown
);

CREATE TABLE edges (
    id          TEXT PRIMARY KEY,
    from_id     TEXT,
    to_id       TEXT,
    rel         TEXT NOT NULL,
    payload     JSON NOT NULL,
    ns          TEXT NOT NULL,
    ts          TEXT NOT NULL,
    agent       TEXT,
    source_file TEXT,
    is_active   INTEGER DEFAULT 1  -- current-state flag
);

CREATE INDEX idx_edges_from ON edges(from_id);
CREATE INDEX idx_edges_to ON edges(to_id);
CREATE INDEX idx_edges_rel ON edges(rel);
CREATE INDEX idx_edges_active ON edges(is_active);
CREATE INDEX idx_spans_path ON spans(path);
CREATE INDEX idx_spans_symbol ON spans(symbol);
CREATE INDEX idx_spans_hash ON spans(content_hash);

CREATE VIRTUAL TABLE edges_fts USING fts5(rel, payload);
```

The SQLite index is gitignored. Any machine that clones the project rebuilds it from the edge files.

### 7.9 The .gitignore

`damask init` creates `.damask/.gitignore`:

```
index.db
index.db-wal
index.db-shm
edges/.private/
edges/.views/
edges/.local/
```

---

## 8. Span Resolution and Staleness

### 8.1 The mutation problem

Content changes. When a file is edited, spans pointing into it may shift or become invalid. A damask full of broken spans is misleadingly wrong.

### 8.2 Multi-anchor resolution

Spans carry multiple anchors, tried in order of precision:

1. **Path + coordinates:** Check if the file at path exists and the content at the given location matches the snippet. The fast path.
2. **Content hash:** Compare the hash of content at the specified location against `content_hash`. If it doesn't match, search the file for a region whose hash matches. Survives line shifts precisely.
3. **Symbol:** Search the file for the symbol (function name, section heading, clause number). Survives reformatting and line shifts.
4. **Snippet:** Fuzzy-search the file for the snippet text. Survives renaming and restructuring.
5. **Unresolved:** If all anchors fail, mark the span as `unresolved`. Edge payloads are preserved.

**Ambiguity:** Common snippets may appear multiple times. The symbol anchor disambiguates. When both symbol and snippet are present, the system requires both to match.

**Content hash computation:** The hash is a truncated SHA-256 of the span text (first 12 hex characters). Optionally, a context hash of surrounding content (20 lines before + span + 20 lines after) provides a secondary signal for detecting "this span still exists but moved."

### 8.3 Freshness tracking

Freshness is two independent signals, not one. Conflating them trains users to ignore warnings.

**Resolution status** — can the span be located?

| Status | Meaning |
|---|---|
| `exact` | Content at original coordinates matches all anchors |
| `relocated` | Content found via hash, symbol, or snippet at a different location |
| `unresolved` | All anchors failed; span cannot be located |

**Recency risk** — has the file changed?

| Status | Meaning |
|---|---|
| `unchanged` | File not modified since span was created |
| `file_changed` | File modified since span creation, but span still resolves |
| `unknown` | File metadata unavailable (e.g., external reference) |

These combine into clear, trustworthy display in `damask at`:

```
✅ exact, unchanged        — fully trustworthy
↪  relocated, unchanged    — span moved but content matches; coordinates updated
✅ exact, file_changed     — span still matches but file was edited elsewhere
↪  relocated, file_changed — span moved and file changed; review recommended
❌ unresolved              — span cannot be located; payload preserved
```

The key insight: a file can change without affecting a span. Marking everything "stale" when the file is touched erodes trust in the freshness system. By separating resolution from recency, `damask at` can show "this span moved to line 58 but the content is identical" (↪ relocated) rather than a blanket warning.

**Edge freshness** is derived from its spans. An edge is reported as:
- **Fresh** if all its spans resolve (`exact` or `relocated`)
- **Degraded** if any span has `file_changed` recency
- **Broken** if any span is `unresolved`

The edge's payload knowledge persists regardless of span freshness.

### 8.4 Graceful degradation

Spans are cheap. Edge payloads are valuable. The system is designed knowing that spans will break:

- **Span breaks, payload survives:** "There was a liability cap that conflicted with GDPR" is useful even if the contract has been revised.
- **Stale, not dead:** Unresolved spans are flagged, not deleted. An agent can re-evaluate.
- **Supersession:** New edges declare they supersede old edges. Old edges remain as history.

**Damask rot.** A damask that is never revisited accumulates historical truth, not current understanding. The append-only log preserves everything, but without periodic maintenance the current-state view diverges from reality. Four mechanisms combat rot: `damask compact` materializes current state, the two-axis freshness model (§8.3) makes staleness visible rather than hidden, `damask lint` flags edges that have been unresolved for longer than a configurable threshold, and recency decay (§7.7) naturally deprioritizes unverified edges over time. A healthy damask is one that is periodically touched, not one that is merely large.

---

## 9. Multi-Modal Spans

### 9.1 Beyond text

A span is a reference to a region within content. The coordinate system changes per modality; the edge model is identical.

| Content type | Span coordinates | Example | Phase |
|---|---|---|---|
| Code / text | `lines: [start, end]` | `src/auth.py` lines 42-67 | 1 |
| PDF (text-extractable) | `pages` + position | `contract.pdf` page 12, paragraph 3 | 2 |
| Spreadsheet | `sheet` + `range` (A1 notation) | `financials.xlsx` Sheet1 B2:D20 | 2 |
| Image | `region: {x, y, w, h}` | `diagram.png` region (120, 340, 200, 150) | 3 |
| Audio | `time: [start_ms, end_ms]` | `meeting.wav` 20:00-20:15 | 3 |
| Video | `time` + optional `region` | `demo.mp4` frames 3400-3500 | 3 |

**Phase 1 supports text and code spans only.** The coordinate system is extensible — `lines` is one coordinate type, others use the same span structure with different fields. Multi-modal support is explicitly deferred to maintain focus on the core experience.

**Phase 2 constraints:**
- **PDFs:** Text-extractable PDFs only (not scanned/OCR). PDFs don't have stable paragraphs — text extraction order varies across engines. Span coordinates use: `page` number + extracted text `snippet` + `content_hash` of the extracted span text. The extraction engine matters; `config.json` should document which engine was used (e.g., `poppler`, `pdfium`). The chosen engine must produce deterministic output on the same file — otherwise anchors won't round-trip across machines. Determinism is best-effort within the same engine and version; cross-engine determinism is not guaranteed. Without this, teams will see spans "randomly move" across machines. Scanned PDF support is out of scope and would require an OCR pipeline that violates the zero-infrastructure principle.
- **Spreadsheets:** Require `sheet` (name), `range` (A1 notation), and optionally a `cell_hash` (hash of cell values in range) for staleness detection.

### 9.2 Cross-modal edges

The power is linking across modalities:

```jsonl
{"t":"span","id":"s_01JKX10...","path":"meetings/standup-jan15.wav","time":[1200000,1215000],"snippet":"transcript: let's definitely revisit the liability cap"}
{"t":"span","id":"s_01JKX11...","path":"contracts/msa-2025.pdf","pages":[12],"snippet":"Liability shall not exceed"}
{"t":"edge","id":"e_01JKX12...","from":"s_01JKX10...","to":"s_01JKX11...","rel":"discusses","payload":{"summary":"Team raised concern about liability cap in standup","confidence":0.85}}
```

A meeting recording linked to a contract clause. An agent reviewing the contract finds not just the text but the conversation where the team flagged it.

### 9.3 The resolver

`damask resolve <span_id>` materializes the content a span references:

- **Text span:** return the lines
- **PDF span:** extract the passage (text-extractable only)
- **Spreadsheet span:** extract the cells
- **Image span:** crop the region (Phase 3)
- **Audio span:** slice the time range (Phase 3)

Resolution is lazy — content is only materialized when needed.

---

## 10. CLI Interface

### 10.1 Design philosophy

The CLI is the primary interface. Not MCP, not an API, not a library. A command-line tool that any agent can invoke via shell and any human can run in a terminal.

CLI was chosen over richer protocols because:

- Every agent can shell out to a CLI (universal compatibility)
- Humans can run the same commands (debuggable, inspectable)
- Unix pipes enable composition
- The shell has been the universal interface for 50+ years
- No server process, no protocol negotiation, no lifecycle management
- Agent training data is full of CLI usage (zero learning curve)

### 10.2 Commands

**Core operations**

```
damask init                              Initialize .damask/ in current directory
damask span <file> <start> <end>         Create a span, output its ID
damask edge <from> <to> <rel> [payload]  Create an edge (use "_" for null endpoints)
damask edge <from> <to> <rel> -f <file>  Payload from file (avoids shell quoting)
damask endorse <edge_id> [payload]       Signal that your work confirmed this edge
damask dispute <edge_id> <payload>       Signal that your work contradicts this edge (payload required)
```

**Queries**

```
damask at <file>:<line>                  What edges touch this location?
damask at <file>:<line> --auto           Create a span on-the-fly if none exists (ephemeral)
damask follow <id> [rel] [--depth N]     Traverse edges from a span or edge
damask search <query>                    Semantic search over edge payloads and content
damask where <predicate>                 Filter edges by properties
damask log [--ns <namespace>]            Show fact log, optionally filtered
damask status                            Damask health: counts, staleness, freshness
damask diff <ns1> <ns2>                  Compare two namespaces
```

**Provenance**

```
damask blame <span_id|edge_id>           Git-blame-style history of an edge/span's evolution
damask why <edge_id>                     Provenance story: who created, endorsed, disputed, superseded
```

**Utilities**

```
damask resolve <span_id>                 Materialize the content a span references
damask ns <name>                         Set active namespace
damask ns list [--stale]                 List namespaces with counts, last-updated, staleness
damask ns merge <source> <target>        Merge one namespace into another
damask lint                              Flag low-value edges, staleness, quality issues
damask compact [namespace]               Produce current-state view, archive old edges
damask compact --aggressive [namespace]  Archive unresolved/unendorsed/low-confidence edges
damask review                            Show new edges since last commit, ranked and grouped
damask pull <source>                     Fetch community edge package
```

### 10.3 The `damask at` experience

`damask at` is the gravitational center of the entire UX. If `at` isn't great, none of the rest matters.

**Ranking is not cosmetic. It's the trust model.** If `damask at` shows low-signal edges even 20–30% of the time, users stop reading. Signal density must be a hard invariant.

When an agent or human asks "what do we know about this location?", the answer must be immediate, ranked, high-signal, and bounded:

```
$ damask at contracts/msa-2025.pdf:12

contracts/msa-2025.pdf:page12:para3 (s_01JKX1A...) — "Liability shall not exceed..."

  ⚠ risk (0.95) ×3✓ — Liability cap below GDPR threshold [contract-review, 2025-01-15]
    action: negotiate removal of cap for data protection claims

  ✗ conflicts_with → regulations/gdpr-article-82.md:3-7 ✅ ×2✓ [contract-review, 2025-01-15]
    "GDPR imposes unlimited liability for data breaches"

  ← discusses — meetings/standup-jan15.wav:20:00-20:15 ✅ [team-notes, 2025-01-15]
    "team raised concern about liability cap"

  ⚡ amends — contracts/amendment-3.pdf:2 ↪ ×1✓ ×1✗ [contract-review, 2025-01-20]
    "amendment increases cap from $1M to $5M" [DISPUTED]

  4 edges shown (4 total). Span: ↪ relocated, file_changed
```

`×3✓` = 3 endorsements. `×1✗` = 1 dispute. `⚡` = disputed edge requiring attention.

#### Display defaults

- **Maximum 12 edges shown** by default. `--all` shows everything.
- **Grouped by rel class:** risks/gotchas first, then contradictions/decisions, then links/descriptions.
- **Freshness glyphs** (✅ ↪ ⚠ ❌) shown inline for each edge's target span.

#### Ranking policy

Edges are scored and ranked by a composite of static and dynamic signals. This is not a suggestion — it's the core trust mechanism. Ranking is how quality emerges from the swarm without human review.

**Static signals** (from the edge itself):

1. **Resolution:** `exact` > `relocated` > `unresolved` (broken edges rank last)
2. **Confidence:** Higher confidence scores rank higher
3. **Actionability:** Edges with `action` field rank above those without
4. **Rel class:** Judgment rels (`risk`, `gotcha`, `decision`, `contradicts`, `ruled_out`) rank above descriptive rels (`describes`, `supports`, `derived_from`)
5. **Signal density:** Edges whose `summary` shares >60% tokens with the span snippet are aggressively down-ranked (restatement suspicion — best-effort heuristic, will false-positive on short snippets and common phrases)
6. **Completeness:** Missing `summary` ranks below present; missing `confidence` below present

**Dynamic signals** (from other agents' interactions — see §13.5):

7. **Endorsement count:** Each `endorsed` meta-edge from a distinct agent/session boosts the edge's effective ranking weight. Three independent endorsements significantly outrank an unendorsed edge at the same confidence level. The ranking boost from endorsements is logarithmic — the first endorsement matters most, the tenth adds little. This prevents gaming through volume. At swarm scale, endorsement count from independent agents/sessions becomes the dominant ranking signal, outweighing individual confidence scores. The ranking algorithm does not change — the weights shift naturally as endorsement data accumulates.
8. **Dispute signal:** Any `disputed` meta-edge flags the edge for attention. A disputed edge is visually marked (⚡) but not removed — disputes are conversation starters, not conclusions. An edge with disputes and no endorsements ranks low. An edge with both disputes and endorsements surfaces the disagreement.
9. **Recency decay:** An edge's ranking weight decays gradually according to the namespace's configured half-life (see §7.7). Decay never removes edges — it only affects ranking position.
10. **Source:** Local edges rank above community edges by default (see §14)

`--no-rank` disables ranking for debugging, showing edges in chronological order.

The `summary` field from the payload envelope is what gets displayed in the one-line view. This is why `summary` is strongly recommended — edges without it show a truncated payload dump, which is less useful and ranks lower.

#### Confidence semantics

`confidence` reflects the agent's belief that the relationship is correct given current information. It is not a measure of importance or severity. An edge can have high confidence and low severity, or low confidence and critical severity. Ranking uses confidence as a trust signal, not a priority signal — severity and actionability are handled by `rel` type and the `action` field.

#### `--auto` span creation

`damask at file:line --auto` creates a span on-the-fly when none exists at the queried location, enabling immediate edge creation. **Auto-created spans are ephemeral** — they are stored in `.damask/edges/.local/` (gitignored) unless the agent explicitly sets a namespace with `--ns`. This prevents `damask at` from becoming a write operation that dirties committed namespaces, which teams would rightly object to.

**`.local/` is never read by default queries** unless explicitly requested via `--include-local`. It is safe to delete `.damask/edges/.local/` at any time without affecting the committed damask.

### 10.4 The `damask blame` and `damask why` experience

**`damask blame`** shows git-blame-style history of how an edge or span evolved — which commits introduced, superseded, endorsed, or disputed it:

```
$ damask blame e_01JKX1B...

e_01JKX1B... risk "No token expiry check" (0.95)
  Created:     4a7b3c2 2025-01-15 claude-opus (security-audit session abc123)
  Endorsed:    8f2d1e9 2025-01-18 claude-sonnet (refactor session def456)
  Endorsed:    a3c7f20 2025-01-22 cursor-agent (feature-work session ghi789)
  Disputed:    b9e4d33 2025-02-01 claude-opus (security-followup session jkl012)
    "Token expiry was added in commit 4a7b3c2"
  Superseded:  c1f8a45 2025-02-01 → e_01JKX9Y... "Token expiry added but rotation still missing"
```

This is `git blame` for knowledge — it shows the full lifecycle of an edge across commits, agents, and sessions. Under the hood, it correlates the edge's supersession chain and meta-edges with `git log` on the relevant JSONL files.

**`damask why`** shows the provenance story — a compact trust summary for quick judgment:

```
$ damask why e_01JKX9Y...

e_01JKX9Y... risk "Token expiry added but rotation still missing" (0.85)
  Created by claude-opus in security-audit, 2025-02-01
  Supersedes e_01JKX1B... "No token expiry check"
  Endorsed ×2 (claude-sonnet, cursor-agent) — last endorsed 2025-02-05
  Disputed ×0
  Decay: 92% weight (half-life: 90 days, last endorsed 3 days ago)
  Status: active, exact, unchanged
```

`damask why` answers "should I trust this edge?" in a single glance. It's the provenance story that makes the endorsement/dispute system legible.

### 10.5 Output formats

Human-readable by default, `--format json` for machine consumption:

```
$ damask at contracts/msa-2025.pdf:12 --format json
[
  {
    "edge_id": "e_01JKX1B...",
    "rel": "risk",
    "payload": {"summary": "Liability cap below GDPR threshold", ...},
    "span_resolution": "relocated",
    "span_recency": "file_changed",
    "endorsements": 3,
    "disputes": 0,
    "decay_weight": 0.85,
    "ns": "contract-review",
    "ts": "2025-01-15T10:30:00Z",
    "source": "local"
  },
  ...
]
```

### 10.6 Payload input

```bash
# Inline (simple)
damask edge s_01JKX1A... _ risk '{"summary":"High risk","confidence":0.9}'

# From file (complex payloads, agent-friendly)
damask edge s_01JKX1A... _ risk -f /tmp/payload.json

# From stdin (pipe-friendly)
echo '{"summary":"High risk","confidence":0.9}' | damask edge s_01JKX1A... _ risk --stdin
```

### 10.7 Composition via pipes

```bash
# Find all contradictions, follow what they depend on
damask where rel=contradicts --format json | damask follow --stdin depends_on

# Search for GDPR-related edges across the project
damask search "GDPR compliance" --format json | damask where severity=high

# Export a namespace
damask log --ns contract-review > contract-findings.jsonl
```

### 10.8 Reviewing agent work

If agents write edges, humans need to review them quickly. `damask review` shows new edges since the last git commit, grouped and ranked:

```
$ damask review

3 new edges since commit 4a7b3c2 (2 hours ago)

  security-audit:
    ⚠ risk (0.95) src/auth.py:100-115
      "Rate limiter disabled in test mode but flag leaks to production"
    ⚠ gotcha (0.9) src/auth.py:42-67
      "Token validation skips expiry in dev environment"

  onboarding:
    → co_change (0.85) src/db/connection.ts:20-35 ↔ src/db/migrations/
      "Schema changes require migration AND connection pool config review"
```

This is `git diff` for knowledge — semantic, ranked, and readable. It drives adoption by making agent discoveries reviewable in the same workflow as code review.

---

## 11. Git Integration

### 11.1 Damask lives with the content

```
my-project/
├── src/                              ← code
├── contracts/                        ← legal documents
├── docs/                             ← documentation
├── meetings/                         ← transcripts, recordings
├── .damask/
│   ├── edges/
│   │   ├── security-audit.jsonl      ← committed
│   │   ├── contract-review.jsonl     ← committed
│   │   ├── onboarding.jsonl          ← committed
│   │   ├── .views/                   ← gitignored (derived)
│   │   └── .private/
│   │       └── privileged.jsonl      ← gitignored
│   ├── index.db                      ← gitignored
│   ├── config.json
│   └── .gitignore
└── .gitignore
```

### 11.2 Normal git workflow

```bash
damask ns contract-review
damask span contracts/msa-2025.pdf 12 12
damask edge s_01JKX1A... _ risk '{"summary":"Liability cap below GDPR threshold","action":"negotiate removal"}'

git add contracts/ .damask/edges/contract-review.jsonl
git commit -m "contract review: flagged liability concerns"
git push
```

No new habits. The damask rides alongside the content in normal git workflow.

### 11.3 Collaboration

- **Clone:** Full damask, immediately resolvable (root-relative spans)
- **Branch:** Namespace-per-file means minimal merge conflicts
- **Pull request:** Edge changes visible in diffs alongside content changes
- **Blame:** `git blame` on edge files shows provenance
- **Revert:** Bad edges? Revert the commit or delete the namespace file

### 11.4 Agent sessions as commits

```
commit 4a7b3c2
Author: Claude Opus <agent@damask>
Date:   2025-01-15 10:30:00

    Contract review: MSA 2025 liability analysis

    Added 6 edges: 2 conflicts_with, 1 risk, 2 describes, 1 amends

 .damask/edges/contract-review.jsonl  |  6 ++++++
 1 file changed, 6 insertions(+)
```

Agent-to-agent communication is commits to the same project's damask.

---

## 12. Namespace Configuration

### 12.1 Namespaces as files

Each namespace maps to a JSONL file. Setting the namespace determines where edges are appended:

```bash
damask ns contract-review      # → .damask/edges/contract-review.jsonl
damask ns --private privileged  # → .damask/edges/.private/privileged.jsonl
```

### 12.2 Querying across namespaces

```bash
damask at contracts/msa.pdf:12                         # all namespaces
damask at contracts/msa.pdf:12 --ns contract-review    # one namespace
damask diff contract-review external-counsel            # compare perspectives
```

### 12.3 Private namespaces and redaction

Sensitive edges (security vulnerabilities, privileged legal analysis, compliance gaps) live in `.damask/edges/.private/`, gitignored by default.

**Redaction is deterministic and tiered.** Two levels serve different trust contexts:

**`--redact` (default, shareable):** Safe for sharing structure with collaborators who have appropriate access.

1. `summary` is preserved (it's designed to be shareable)
2. `evidence`, `reference`, and any field listed in `config.json` under `redact_fields` are removed
3. `confidence`, `action`, and `tags` are preserved
4. All other payload keys are removed
5. Span snippets are replaced with `"[redacted]"`

**`--redact=strict`:** Safe for external sharing, compliance exports, or contexts where even summaries may leak sensitive information.

1. `summary` is truncated to 60 characters maximum
2. `action` is removed
3. `evidence`, `reference`, and all custom payload keys are removed
4. `confidence` and `tags` are preserved (coarse signals only)
5. Span snippets replaced with `"[redacted]"`
6. Paths preserved by default; `--redact-paths` additionally replaces paths with `"[redacted]"` for contexts where repo structure itself is sensitive
7. `rel` type preserved (structural, not content)

Both levels produce consistent output shapes. Redacted exports are safe to share without manual review at their respective trust level.

```bash
# Share with collaborators
damask log --ns privileged --redact > for-team.jsonl

# Compliance export
damask log --ns privileged --redact=strict > for-external.jsonl
```

### 12.4 Namespace schema expectations

`.damask/config.json` can declare, per namespace, which rel types are expected, which payload keys are required, and how quickly edges decay:

```json
{
  "namespaces": {
    "security-audit": {
      "rels": ["risk", "gotcha", "depends_on", "env"],
      "required_payload": ["summary", "confidence", "action"],
      "decay_half_life_days": 90,
      "description": "Security findings — all edges must have actionable summaries"
    },
    "contract-review": {
      "rels": ["conflicts_with", "amends", "governed_by", "risk"],
      "required_payload": ["summary", "confidence"],
      "decay_half_life_days": 365,
      "description": "Legal review findings"
    }
  },
  "default_decay_half_life_days": 180,
  "redact_fields": ["evidence", "reference", "internal_notes"]
}
```

`damask lint` enforces these expectations: "in namespace security-audit, edge e_01JKX... is missing required field `action`."

This keeps the core universal while allowing teams to be strict where they care. Namespaces without config entries are unconstrained.

**Namespace schemas are guardrails, not contracts.** They are intended to improve signal quality, not to enforce completeness or correctness. If teams find themselves spending more time satisfying schema requirements than creating useful edges, the schemas are too strict.

### 12.5 Namespace management

`damask ns list` shows all namespaces with health metrics:

```
$ damask ns list

  security-audit      42 edges (38 active)  last updated 2 days ago   3 unresolved
  contract-review     28 edges (25 active)  last updated 1 week ago   0 unresolved
  onboarding          18 edges (18 active)  last updated 3 weeks ago  1 unresolved
  daily-2025-01-15     6 edges (4 active)   last updated 3 months ago 6 unresolved ⚠ stale
```

`damask ns list --stale` filters to namespaces where >50% of edges are unresolved or the namespace hasn't been touched in configurable threshold (default: 90 days).

`damask ns merge <source> <target>` moves all edges from one namespace file into another, rewriting the `ns` field. This combats namespace sprawl — teams that created `sprint-22`, `sprint-23`, `sprint-24` can consolidate into `architecture` without losing edges.

### 12.6 Namespace conventions

Not prescribed. Communities develop their own. Starting suggestions:

- **Task-based:** `security-audit`, `contract-review`, `literature-survey`
- **Temporal:** `daily-2025-01-15`, `sprint-23`
- **Agent-based:** `claude-opus`, `cursor-agent`
- **Community:** `community-react`, `community-gdpr-templates`

Avoid namespace proliferation. A project with 50 namespaces is harder to navigate than one with 5 well-scoped ones. Prefer task-based namespaces over temporal or agent-based ones unless there's a specific reason to track by time or author. Use `damask ns merge` to consolidate when sprawl occurs.

---

## 13. Agent Integration

### 13.1 The agent loop

An agent working with Damask follows a simple cycle: **read → work → react → record → commit.**

```
┌─────────────────────────────────────────┐
│  1. READ the existing damask            │
│     damask status                       │
│     damask at <files being worked on>   │
│                                         │
│  2. WORK on the task                    │
│     (normal agent work — read files,    │
│      analyze, reason, produce output)   │
│                                         │
│  3. REACT to edges you encountered      │
│     damask endorse <id>  (if confirmed) │
│     damask dispute <id>  (if wrong)     │
│     (only for edges your work verified) │
│                                         │
│  4. RECORD new discoveries              │
│     damask span <file> <start> <end>    │
│     damask edge <from> <to> <rel> ...   │
│                                         │
│  5. COMMIT (if agent has git access)    │
│     git add .damask/edges/              │
│     git commit -m "..."                 │
└─────────────────────────────────────────┘
```

**Why react comes after work, not before.** An endorsement means "I independently verified this during my work." An agent that endorses edges before doing any work is endorsing based on plausibility, not evidence. The react step must follow the work step — the agent's work is what generates the evidence to endorse or dispute.

### 13.2 System prompt convention

The following is a reference system prompt for agents working with Damask. Adapt to the specific agent framework and task.

```
## Damask Knowledge Fabric

This project uses Damask to persist cross-session understanding.

### Before starting work

Read the existing damask to inherit prior discoveries:

  damask status                          # health check: edge counts, resolution
  damask at <file>:<line>                # what's known about files you'll work on
  damask where rel=risk --resolved-only  # current risks with live spans
  damask where rel=gotcha --resolved-only # known traps with live spans

### During work

When you discover something that isn't obvious from reading a single file —
a cross-file relationship, a failure mode, a contradiction, a decision
rationale, or an environmental requirement — record it:

  damask ns <task-name>
  damask span <file> <start> <end>
  damask edge <span_id> <target_or_null> <rel> -f /tmp/payload.json

Write payloads as JSON files to avoid shell quoting issues. Always include
a "summary" field (one line, human-readable) and a "confidence" field (0-1).

### After work — react to what you read

Now that you've done your work, react to edges you encountered earlier:

  - If your work confirmed an edge is accurate: damask endorse <edge_id>
  - If your work reveals an edge is wrong: damask dispute <edge_id> '{"summary":"reason"}'
  - Don't endorse edges you merely read — only those your work independently verified
  - Disputes MUST include a reason (what evidence contradicts the edge?)

### What to record (the surprise heuristic)

Record what surprised you — what took effort to figure out, what the next
agent would need to re-derive from scratch. Specifically:

- Cross-file dependencies: "auth.py imports SECRET_KEY from settings.py"
- Failure modes: "cache TTL defaults to infinite — will OOM in production"
- Contradictions: "MSA caps liability but GDPR requires unlimited"
- Decision rationale: "chose SQLite for zero-dependency deployment"
- Co-change requirements: "user model change requires serializer + migration"
- Environmental context: "requires Python 3.11+, fails silently on 3.10"
- Aggregate patterns: "14 locations silently swallow exceptions"
- Ruled-out concerns: "investigated X — it's NOT a problem because Y" (prevents re-investigation)

### What NOT to record

- What a function/clause/paragraph says (read it)
- Anything a linter, type checker, or static analyzer can derive
- Formatting or structural observations
- Anything obvious from reading the single file

### Example session

  damask status
  # 42 edges across 3 namespaces, 6 unresolved

  damask at src/auth.py:42
  # risk: no token expiry (high confidence, 2025-01-15)
  # depends_on: settings.py SECRET_KEY

  # ... do work, discover new issue, verify existing edges ...

  damask ns security-audit
  damask span src/auth.py 100 115
  # → s_01JKXYZ...

  cat > /tmp/edge.json << 'EOF'
  {
    "summary": "Rate limiter disabled in test mode but test mode flag leaks to production",
    "confidence": 0.9,
    "action": "Add environment check to rate limiter initialization",
    "evidence": ["s_01JKX1A..."]
  }
  EOF

  damask edge s_01JKXYZ... _ gotcha -f /tmp/edge.json

  # React to edges read earlier — my refactor confirmed the dependency is real
  damask endorse e_01JKX1B... '{"summary":"Confirmed during refactor — SECRET_KEY coupling is real"}'
```

### 13.3 Integration with specific tools

**Claude Code:** Add the system prompt to `CLAUDE.md` or the project's `.claude/` configuration. Claude Code agents can shell out to `damask` commands directly.

**Cursor / Windsurf:** Add the system prompt to `.cursorrules` or equivalent. The agent invokes `damask` via terminal commands.

**Autonomous agents (CrewAI, AutoGen, LangGraph):** Wrap `damask` CLI calls as tools in the agent's tool registry. The read-work-react-record loop maps to the agent's task execution cycle.

**CI/CD pipelines:** `damask lint` and `damask status` can run as CI checks. Flag commits that add edges without summaries, or that increase staleness significantly.

### 13.4 Quality control

`damask lint` flags low-value edges. **Lint is not optional hygiene — it's how signal density is maintained.** If the lint rules are too lax, `damask at` becomes noisy. If too strict, agents stop creating edges. The defaults are calibrated for high signal.

**Hard flags** (always reported):

- Empty payloads
- Missing `summary` field
- Broken JSON
- Missing timestamps or attribution
- Violations of namespace schema expectations (from `config.json`)
- Edge-to-edge links with domain-specific rel types (should use meta rels only)

**Signal density flags** (reported as warnings, never block writes):

- **Restatement suspicion:** Summary shares >60% tokens with span snippet (likely observation, not discovery). Best-effort heuristic — will false-positive on short snippets and common phrases. Flagged as a warning for agent/human review, not treated as ground truth.
- **Speed check:** Edge created within 2 seconds of its span creation (too fast for genuine analysis — likely mechanical restatement)
- **Confidence floor:** `confidence < 0.5` without `status: "hypothesis"` in payload
- **Actionable rels without action:** `risk`, `gotcha` edges missing the `action` field

**Staleness flags:**

- Spans unresolved for longer than configurable threshold (default: 30 days)
- Edges where all spans are unresolved (candidate for archival)
- Duplicates (similar summary + same rel + overlapping spans)

### 13.5 Agent experience (AX): the feedback loop

Traditional quality control assumes human review. But if agents create edges as a byproduct of work and the damask grows across hundreds of sessions, no human is going to review 15 new edges every session. They'll do it twice, feel good about it, then stop. Review becomes shelfware — the same failure mode as PR review fatigue.

The solution: **quality emerges from use, not from review.** Agents reading existing edges during their work can signal whether those edges proved accurate, creating a feedback loop that makes the damask self-curating.

#### Endorsement and dispute

Two convenience commands create meta-edges:

```bash
damask endorse e_01JKX1B...                         # confirmed during my work
damask endorse e_01JKX1B... '{"summary":"Verified during refactor — dependency is real"}'
damask dispute e_01JKX1B... '{"summary":"Token expiry was added in commit 4a7b3c2, risk is resolved"}'
```

These produce standard meta-edges:

```jsonl
{"t":"edge","id":"e_new...","from":"e_01JKX1B...","to":null,"rel":"endorsed","payload":{"summary":"Verified during refactor"},"ns":"security-audit","ts":"...","agent":"claude-opus-4-6","session":"def456"}
{"t":"edge","id":"e_new...","from":"e_01JKX1B...","to":null,"rel":"disputed","payload":{"summary":"Token expiry added in 4a7b3c2"},"ns":"security-audit","ts":"...","agent":"claude-opus-4-6","session":"ghi789"}
```

**Endorsement** means "I independently encountered this claim during my work and it checked out." It's a soft signal — one endorsement is a data point, three from different agents is high trust.

**Dispute** means "I encountered evidence that this edge is wrong or outdated." Disputes don't remove edges — they flag them for attention and reduce ranking weight. A dispute is a conversation starter, not a conclusion.

**Why disputes require a payload but endorsements don't.** An endorsement is a binary signal — "yes, this checked out." The context of the endorsing agent's work provides implicit justification. A dispute without reasoning is noise — it says "this is wrong" without explaining why or what evidence contradicts it. Requiring a payload for disputes ensures that every dispute is actionable: the next agent (or human) encountering the disputed edge can read the dispute's summary and decide whether to supersede, re-endorse, or investigate further. A bare dispute would leave the edge in limbo with no path to resolution.

**Endorsements and disputes never change edge truth, active status, or historical record. They only influence ranking and visibility.** This invariant is sacred. Damask must never become a voting system. There must never be auto-supersession triggered by dispute count, nor auto-confirmation triggered by endorsement count. Supersession remains the only mechanism by which truth changes, and supersession is always a deliberate act by an agent or human.

#### The path from dispute to resolution

A disputed edge should lead to one of three outcomes:

1. **Supersession** — the edge was wrong, a new edge replaces it
2. **Counter-endorsement** — another agent verifies the original, the dispute was mistaken
3. **Limbo** — neither confirmed nor resolved, the edge persists with reduced ranking and a ⚡ flag

Option 3 is acceptable. Unresolved disputes don't block work — they inform judgment. Over time, decay naturally deprioritizes edges stuck in limbo.

#### Safeguards against endorsement inflation

If agents endorse everything they read, every edge gets endorsed and the signal is meaningless. The system prompt, lint rules, and data model enforce discipline:

- **Endorse only when your work independently verified the claim** — not when you read it and it seemed plausible
- **Speed check applies to endorsements:** An endorsement created within a short window of the session's first `damask at` call is flagged by `damask lint` as potentially reflexive. This is a lint warning, not a hard filter — an agent that pre-verified before calling `damask at` is not being reflexive.
- **Self-endorsement is ignored:** An agent endorsing its own edges (same `agent` + same `session`) adds no ranking weight
- **Diminishing returns:** The ranking boost from endorsements is logarithmic — the first endorsement matters most, the tenth adds little. This prevents gaming through volume.

#### From single agent to swarm

At low agent density, endorsement and dispute are quality signals — ways to maintain a clean damask without human review. At high agent density (swarms of agents across worktrees, parallel sessions, CI pipelines), they become something more fundamental: a convergent verification mechanism.

When fifty independent agents working on unrelated tasks all endorse the same edge, that convergence is a stronger epistemic signal than any individual agent's confidence score. Damask doesn't prescribe how many agents are needed for this transition — it emerges naturally from the existing primitives as usage scales. The same endorsement that helps one team stay tidy becomes the mechanism by which a swarm collectively *knows* things.

---

## 14. Cross-File and Cross-Project Edges

### 14.1 Linking notes to content

Agents write markdown notes. The damask links them to the content they discuss:

```jsonl
{"t":"span","id":"s_01JKX20...","path":"notes/review-summary.md","lines":[3,5],"snippet":"The MSA liability cap is dangerously low"}
{"t":"span","id":"s_01JKX21...","path":"contracts/msa-2025.pdf","pages":[12],"snippet":"Liability shall not exceed"}
{"t":"edge","id":"e_01JKX22...","from":"s_01JKX20...","to":"s_01JKX21...","rel":"describes","payload":{"summary":"Reviewer's summary of liability concern"}}
```

`damask at contracts/msa-2025.pdf:12` returns the clause AND the review note. Notes and source material become one fabric.

### 14.2 Cross-project references

For referencing external packages or shared resources, use prefixed paths:

```jsonl
{"t":"span","id":"s_01JKX30...","path":"react:src/ReactFiberHooks.js","lines":[1450,1520],"snippet":"function updateEffectImpl("}
{"t":"span","id":"s_01JKX31...","path":"gdpr:article-82.md","lines":[1,15],"snippet":"Right to compensation and liability"}
```

The prefix convention (`package:path`) indicates an external reference. Resolution depends on context — a React span resolves within `node_modules`, a regulation span resolves within a shared reference corpus.

**Versioning note:** Cross-project spans are inherently fragile. A span referencing `react:src/ReactFiberHooks.js:1450` may not resolve against a different version of React. Community packages should document which versions their spans target.

**When cross-project resolution fails:**
- Show the payload `summary` without span materialization
- Mark the edge as `external_unresolved` in query output
- Do not downgrade `confidence` automatically — the knowledge in the payload may still be correct even if the span can't be located
- This prevents community knowledge from looking "broken" when it's actually "informational"

### 14.3 Community damask

Community-maintained edge collections:

```
github.com/damask-community/react
github.com/damask-community/express
github.com/damask-community/gdpr-templates
github.com/damask-community/owasp-patterns
```

```bash
damask pull damask-community/react
# → .damask/edges/community-react.jsonl
```

Contributions via PR. Trust by git provenance and repo reputation. Remove by deleting the file.

**Community provenance is encoded in the fact format, not just UI.** When `damask pull` fetches a community package, each fact is annotated with:

```json
{
  "source_repo": "github.com/damask-community/react",
  "source_commit": "a3f7c2d...",
  "source_ref": "v1.2.0",
  "pulled_ts": "2025-02-01T10:00:00Z"
}
```

This prevents provenance ambiguity — you can always verify where a community edge came from, even if someone copies the file manually.

Community packages are read-only once pulled. Local edges referencing community spans create connections between your project and community knowledge without modifying the community file.

**Community edges have distinct UX treatment.** This is a trust decision, not a presentation choice:

- Community edges are **visually distinct** in `damask at` output (prefixed with source identifier)
- Community edges **never outrank local edges** by default, regardless of confidence score
- Provenance (source repo, commit) is **always visible** without extra flags
- Community edges with unresolvable spans are marked `external_unresolved` and treated as informational — their payload knowledge is displayed but they don't affect the project's overall freshness metrics

---

## 15. Querying the Damask

### 15.1 Query patterns

| Pattern | Command | What it answers |
|---|---|---|
| Point query | `damask at <file>:<location>` | What do we know about this location? |
| Traversal | `damask follow <id> [rel] [--depth N]` | What's connected to this? |
| Search | `damask search <query>` | What edges match this concept? |
| Filter | `damask where <predicate>` | What matches these properties? |
| Compare | `damask diff <ns1> <ns2>` | Where do perspectives agree or conflict? |
| Provenance | `damask blame <id>` | How did this edge evolve over time? |
| Trust | `damask why <id>` | Should I trust this edge? |
| Log | `damask log [--ns N] [--since T]` | What was recorded, when, by whom? |
| Health | `damask status` | Staleness, coverage, edge counts |

All queries default to current-state view (active, non-superseded edges only). Use `--history` for the full log.

### 15.2 Traversal example

```
$ damask follow s_01JKX1A... --depth 2

s_01JKX1A... (contracts/msa-2025.pdf:12) "Liability shall not exceed..."
├── conflicts_with → s_01JKX1B... (regulations/gdpr-article-82.md:3-7) GDPR liability
│   └── implements ← s_01JKX1C... (compliance/checklist.md:15) "GDPR compliance item 4.2"
├── amends ← s_01JKX1D... (contracts/amendment-3.pdf:2) "Increases cap to $5M"
│   └── supersedes → s_01JKX1E... (contracts/amendment-1.pdf:3) "Original $1M cap"
├── discusses ← s_01JKX1F... (meetings/standup-jan15.wav:20:00) team concern
└── risk → "Liability cap below GDPR threshold" (0.95) ×3✓ [contract-review, 2025-01-15]
```

Three hops from a contract clause: the regulation it conflicts with, the compliance checklist tracking it, the amendment history, the meeting where it was discussed, and the risk assessment. One query. The web of understanding around a single passage.

### 15.3 Freshness-aware queries

```bash
damask where rel=risk                        # default: active edges, any resolution status
damask where rel=risk --resolved-only        # only edges with exact or relocated spans
damask where rel=risk --include-unresolved   # everything including broken spans
damask where rel=contradicts --since 2025-01-01
damask where confidence>0.8                  # filter by payload envelope fields
damask where resolution=unresolved           # find all broken spans for maintenance
damask where endorsed>2                      # only well-verified edges
damask where disputed=true                   # show all disputed edges for resolution
```

---

## 16. Comparison to Existing Approaches

| Property | WWW | Obsidian | Supermemory | OpenClaw | Damask |
|---|---|---|---|---|---|
| Node granularity | Page | File | Document | File | Span (line/pixel/ms) |
| Link direction | One-way | One-way | One-way | None | Bidirectional |
| Link typing | None | None | extends/derives | None | Freeform with vocabulary |
| Link content | None | None | None | None | JSON payload with envelope |
| Who creates links | Humans | Humans | Their pipeline | Agent (prose) | Agent (structured) |
| Where data lives | Remote servers | Local files | Their cloud | Local markdown | Local JSONL in git |
| Query by location | URL only | Search | API | Search | `damask at file:line` |
| Cross-modal | Via embedding | Plugins | Text only | Text only | Native (any content type) |
| Survives company death | If server lives | Yes | No | Yes | Yes |
| Sharing model | HTTP hosting | Obsidian Publish | API | Git | Git push/pull |
| Domain specificity | None | None (personal KB) | AI apps | Code | None (universal) |
| Quality feedback | None | None | None | None | Endorsement/dispute/decay |

---

## 17. Architecture

### 17.1 System overview

```
Agent (any agent with shell access)
    │
    │  shell out to CLI
    ▼
damask CLI (Rust binary)
    │
    ├──→ .damask/edges/*.jsonl      (append facts — canonical format)
    ├──→ .damask/edges/.views/      (materialized current state — derived)
    ├──→ .damask/index.db           (SQLite — query performance, derived)
    └──→ content files              (read-only — resolve spans)
    │
    └──→ CK engine                  (optional — semantic search over edges)
```

### 17.2 Independence layers

Damask is designed with explicit independence layers. Each layer adds capability but none is required:

| Layer | Provides | Required? |
|---|---|---|
| JSONL files | Canonical storage, git integration, human readability | **Yes** (this is Damask) |
| SQLite index | Fast queries (`at`, `where`, `follow`) | Recommended (auto-built) |
| CK / search engine | Semantic search (`damask search`) | Optional |
| Tree-sitter | Symbol anchors for code spans | Optional |
| config.json | Namespace schemas, redaction rules, decay rates | Optional |

The only hard requirement is the ability to read/write JSONL files. Everything else is progressive enhancement.

### 17.3 Search integration

CK (a semantic search engine written in Rust) can provide the search layer:

- Semantic search over edge payloads: find edges by meaning
- Lexical search via Tantivy: exact keyword matching
- Hybrid search: fusion of semantic and lexical results
- Tree-sitter parsing: symbol anchors for code spans

**Search scope is payload-first.** `damask search` indexes edge payloads (summaries, actions, tags, custom fields) — not the content files themselves. Indexing full content blows up scope and violates the thin-substrate principle. Content snippets are included only when resolving spans for display.

Full-content semantic search (searching the files themselves rather than what agents have said about them) is explicitly out of scope for the Damask CLI. External tools (CK, ripgrep, IDE search) handle content search. Damask searches knowledge, not content.

### 17.4 Implementation

A Rust binary. For codebases, potentially implemented as new crates alongside CK:

- `ck-damask` — CLI, fact file parser, edge operations
- `ck-facts` — append-only JSONL reader/writer
- `ck-resolve` — span resolution, multi-anchor matching, freshness

CK's existing crates provide optional search infrastructure. But the damask CLI can also stand alone.

---

## 18. Phased Roadmap

### Phase 1: Local tool (the Foundation on Terminus)

**Goal:** A single agent on a single project finds the damask useful enough to create edges as part of its normal work.

**Build order** (priority sequence — `damask at` must be delightful before anything else matters):

1. `init` / `ns` / `span` / `edge` — core primitives
2. **`at`** — the point query, ranked and summarized (this is the product)
3. `where` — predicate filtering
4. `follow` — graph traversal
5. `endorse` / `dispute` — agent feedback loop
6. `status` + `lint` — trust and hygiene
7. `ns list` / `ns merge` — namespace management (prevents early sprawl)
8. `compact` + views — prevents log rot
9. `why` / `blame` — provenance legibility
10. `resolve` — span materialization
11. `log` — history

**Deliverables:**

- damask CLI binary (Rust)
- Core commands with ULID-based IDs
- `.damask/edges/` folder format with namespace-per-file
- Private edges via `.damask/edges/.private/`
- SQLite index with current-state resolution
- Immutable event model: supersession via new edges, deterministic current-state algorithm
- Multi-anchor span resolution with content hashes
- Two-axis freshness tracking (resolution + recency)
- Payload envelope conventions (`summary`, `confidence`, `action`)
- `damask at` with ranking policy, max-N display, freshness glyphs, endorsement/dispute counts
- `damask endorse` / `damask dispute` for agent feedback loop
- Recency decay with per-namespace half-life configuration
- `damask why` / `damask blame` for provenance legibility
- `damask ns list` / `damask ns merge` for namespace management
- `damask review` for reviewing agent-created edges
- `damask compact` with standard and `--aggressive` modes
- Namespace schema configuration
- Tiered deterministic redaction (`--redact`, `--redact=strict`)
- `damask lint` with signal density heuristics and endorsement quality checks
- System prompt conventions with worked examples (react-after-work ordering)
- Payload from file/stdin

**Success metric:** An agent's second session is measurably faster because of edges from the first session. Measured via timed A/B test: agent with damask vs agent without damask, same codebase, same task, time-to-first-useful-output.

**Initial domains:** Code, documentation, personal knowledge management.

### Phase 2: Team accretion (the Encyclopedia)

**Goal:** Multiple agents and people build a shared damask through normal git workflow.

**Deliverables:**

- `damask diff` for comparing namespaces (including supersession conflict detection)
- `damask search` (semantic, payload-first, via CK or similar engine)
- Tree-sitter symbol anchors
- `damask at --auto`
- PDF span support (text-extractable only; page + snippet + content_hash; engine documented)
- Spreadsheet span support (sheet + A1 range + cell hash)
- Context hash anchors (surrounding content hash for improved span relocation)
- CI integration guidance (`damask lint` + `damask status` as pipeline checks)
- Endorsement quality instrumentation (measure endorsement accuracy, not just count)

**Success metric:** A new team member's agent onboards significantly faster by reading the existing damask.

**Expanded domains:** Legal, research, business analysis.

### Phase 3: Community sharing (the Trade Routes)

**Goal:** Community-maintained edge collections.

**Deliverables:**

- `damask pull` with version awareness
- Cross-project span addressing
- `damask-community` GitHub org
- Domain-specific vocabulary packages
- Image, audio, and video span support
- Trust model based on git provenance

**Success metric:** A community package has 100+ edges from 10+ contributors.

### Phase 4: Global index (DamaskHub) — aspirational

**Goal:** Index every public `.damask/` on GitHub. Cross-project queries over the global knowledge graph.

**This phase is explicitly optional and conditional.** Damask is strongest as a local-first substrate. Global indexing introduces social risks (cargo-cult edges, incorrect conclusions amplified, legal concerns in security/legal domains) that may outweigh the benefits. Phase 4 proceeds only if Phases 1–3 demonstrate that edge quality holds at community scale.

**Deliverables (if pursued):**

- GitHub crawler for public `.damask/edges/` directories
- Global edge index
- `damask search --source global`
- Aggregate pattern detection
- Cryptographic signing for community trust

**Success metric:** Global queries return actionable findings from agents across thousands of projects.

---

## 19. Scenarios

### 19.1 Code: Security audit

```bash
damask ns security-audit
damask span src/auth.py 42 67                    # → s_01JKX1A...
damask edge s_01JKX1A... _ risk -f /tmp/risk.json
# payload: {"summary":"No token expiry check","confidence":0.95,"action":"Add expiry validation","cvss":9.1}

damask span src/config/settings.py 3 3           # → s_01JKX1B...
damask edge s_01JKX1A... s_01JKX1B... depends_on '{"summary":"Imports SECRET_KEY at module level"}'

# Later, another agent:
damask at src/auth.py:42
# → risk (0.95): No token expiry check — action: Add expiry validation
# → depends_on: settings.py (hardcoded key)
# Immediate understanding without re-derivation
```

### 19.2 Legal: Contract review

```bash
damask ns contract-review
damask span contracts/msa-2025.pdf 12 12         # → s_01JKX2A... (liability clause)
damask span regulations/gdpr-article-82.md 3 7   # → s_01JKX2B...
damask edge s_01JKX2A... s_01JKX2B... conflicts_with -f /tmp/conflict.json
# payload: {"summary":"MSA caps liability, GDPR imposes unlimited liability for data breaches","confidence":0.9,"action":"negotiate removal of cap for data protection claims"}

# Later, during negotiation:
damask follow s_01JKX2A... --depth 1
# → conflicts_with: GDPR Article 82 (0.9) — action: negotiate removal
# → amends: Amendment 3 (increased cap)
# → discusses: standup recording (team concern)
# Full context for the negotiation, instantly
```

### 19.3 Research: Literature review

```bash
damask ns literature-survey
damask span papers/smith-2023.pdf 8 12            # → s_01JKX3A... (methodology)
damask span papers/jones-2024.pdf 3 5             # → s_01JKX3B... (results)
damask edge s_01JKX3B... s_01JKX3A... contradicts -f /tmp/contradiction.json
# payload: {"summary":"Jones 2024 results contradict Smith methodology assumptions","confidence":0.85,"action":"flag for replication study","tags":["methodology","replication"]}

# Later, writing the review:
damask where rel=contradicts
# → Every contradiction found across all surveyed papers
# The literature review writes itself from the edges
```

### 19.4 Personal knowledge: Connecting ideas across reading

```bash
damask ns reading
damask span books/thinking-fast-slow.md 145 152   # → s_01JKX4A...
damask span articles/sutton-bitter-lesson.md 3 8   # → s_01JKX4B...
damask edge s_01JKX4A... s_01JKX4B... supports -f /tmp/insight.json
# payload: {"summary":"Sutton's bitter lesson is the ML version of Kahneman's System 1 — pattern matching beats deliberate reasoning at scale","confidence":0.8}

# A year later, writing an essay:
damask search "pattern matching vs deliberate reasoning"
# → Finds the connection across two completely different sources
```

### 19.5 Business: Quarterly analysis

```bash
damask ns q4-analysis
damask span financials/q4-2024.xlsx 1 1           # → s_01JKX5A... (revenue)
damask span strategy/2024-plan.pdf 15 18          # → s_01JKX5B... (projections)
damask edge s_01JKX5A... s_01JKX5B... contradicts -f /tmp/gap.json
# payload: {"summary":"Q4 revenue $2.1M vs projected $3.5M — 40% shortfall from delayed launch","confidence":0.95,"action":"address in board narrative"}

damask span postmortems/launch-delay.md 1 30      # → s_01JKX5C...
damask edge s_01JKX5A... s_01JKX5C... caused_by '{"summary":"Launch delay → missed Q4 window → revenue shortfall"}'

# Board prep:
damask follow s_01JKX5A... --depth 2
# → Revenue shortfall linked to projection, linked to launch delay postmortem
# Complete causal chain for the board narrative
```

### 19.6 Codebase onboarding (the Phase 1 demo)

The best first demonstration of Damask. An experienced developer's agent damasks a codebase; a new developer's agent reads it.

```bash
# Session 1: Senior developer's agent reviews the codebase
damask init
damask ns onboarding

# Agent discovers architecture
damask span src/index.ts 1 15
damask edge s_... _ decision '{"summary":"Express over Fastify — chosen for middleware ecosystem, not speed","confidence":0.95,"action":"do not migrate without team discussion
Damask 0.7 — Part 2 (continued from §19.6)
# Session 1: Senior developer's agent reviews the codebase (continued)
damask init
damask ns onboarding

# Agent discovers architecture
damask span src/index.ts 1 15
damask edge s_... _ decision '{"summary":"Express over Fastify — chosen for middleware ecosystem, not speed","confidence":0.95,"action":"do not migrate without team discussion"}'

damask span src/db/connection.ts 20 35
damask span src/db/migrations/ 1 1
damask edge s_... s_... co_change '{"summary":"Any schema change requires both a migration AND a connection pool config review","confidence":0.9}'

damask span src/auth/oauth.ts 80 120
damask edge s_... _ gotcha '{"summary":"OAuth refresh token rotation is disabled in dev — will cause silent auth failures if copied to staging","confidence":0.95,"action":"never copy dev auth config to staging"}'

# 15 more edges covering the non-obvious architecture...

git add .damask/
git commit -m "onboarding: initial codebase damask from architecture review"

# Session 2: New developer's agent starts work
damask status
# 18 edges in onboarding namespace, all resolved ✅

damask at src/auth/oauth.ts:80
# → gotcha (0.95): OAuth refresh token rotation disabled in dev
#   action: never copy dev auth config to staging

# The new agent inherits weeks of architectural understanding in seconds

# Agent does its work, confirms the gotcha is real...

# React: endorse the gotcha after independently encountering it
damask endorse e_... '{"summary":"Hit this exact issue when setting up staging env"}'
19.7 The feedback lifecycle (four-agent scenario)
This scenario demonstrates how quality emerges from the swarm without human coordination.
# === Agent A: Security auditor creates a risk edge ===
damask ns security-audit
damask span src/auth.py 42 67                    # → s_A1...
damask edge s_A1... _ risk -f /tmp/risk.json
# → e_A1... payload: {"summary":"No token expiry check","confidence":0.95,"action":"Add expiry validation"}

git add .damask/ && git commit -m "security-audit: token expiry risk"

# === Agent B: Refactoring auth module, reads the damask ===
damask at src/auth.py:42
# → risk (0.95): No token expiry check — action: Add expiry validation

# Agent B does its refactoring work, encounters the same code,
# confirms there's genuinely no expiry check...

damask endorse e_A1... '{"summary":"Confirmed during refactor — no expiry logic anywhere in auth module"}'
# Edge now: risk (0.95) ×1✓

git add .damask/ && git commit -m "refactor: auth module cleanup + endorsed token risk"

# === Agent C: Implements the fix, disputes the edge ===
# Agent C was tasked with fixing auth issues. It adds token expiry.

damask at src/auth.py:42
# → risk (0.95) ×1✓: No token expiry check — action: Add expiry validation

# Agent C implements the fix, then disputes the original edge:
damask dispute e_A1... '{"summary":"Token expiry check added in this session — see validate_token() line 55"}'
# Edge now: risk (0.95) ×1✓ ×1✗ ⚡

git add .damask/ src/auth.py && git commit -m "fix: add token expiry + dispute resolved risk"

# === Agent D: Next security scan, resolves the dispute ===
damask at src/auth.py:42
# → ⚡ risk (0.95) ×1✓ ×1✗: No token expiry check [DISPUTED]
#   dispute: "Token expiry check added — see validate_token() line 55"

# Agent D inspects the code, confirms the fix landed,
# and supersedes the original edge:

damask span src/auth.py 50 60                    # → s_D1... (the new expiry code)
damask edge s_D1... _ ruled_out -f /tmp/resolved.json
# → e_D1... payload: {"summary":"Token expiry now implemented — original risk resolved",
#                      "confidence":0.95,"original_rel":"risk","evidence":["s_A1..."]}

damask edge e_D1... e_A1... supersedes '{"summary":"Risk resolved by token expiry implementation"}'

git add .damask/ && git commit -m "security-audit: token expiry risk resolved"

# === Result ===
damask at src/auth.py:42
# → ruled_out (0.95): Token expiry now implemented — original risk resolved
# The original risk edge is superseded, the dispute is resolved,
# and the damask records both the problem and its resolution.

damask why e_D1...
# e_D1... ruled_out "Token expiry now implemented" (0.95)
#   Created by claude-opus in security-audit, 2025-02-05
#   Supersedes e_A1... "No token expiry check"
#     (which had: ×1 endorsed, ×1 disputed)
#   Endorsed ×0 (new)
#   Status: active, exact, unchanged
Four agents, zero human coordination, clean resolution. The damask records not just the current state but the full provenance: who found the risk, who confirmed it, who fixed it, and who verified the fix.

20. Operational Constraints
Production systems need explicit expectations about scale, performance, and failure modes.
20.1 Scale targets
Metric	Phase 1 target	Notes
Edges per namespace	up to 10,000 before compaction recommended	Compaction produces current-state view, archives superseded edges
Total edges per project	up to 100,000	SQLite handles this comfortably
Index rebuild time	<30 seconds at 100k edges	Full rebuild from JSONL files
damask at response time	<500ms	Point query on indexed data
damask where response time	<2 seconds at 100k edges	Predicate scan
These are targets, not hard limits. Performance degrades gracefully beyond these thresholds.
20.2 Incremental indexing
The SQLite index updates incrementally:
* On each CLI invocation, the index checks whether any JSONL files have been modified since the last index update (via file modification time)
* If so, new lines are parsed and added to the index
* Full rebuild (damask reindex) is available but rarely needed
* If the index is missing or corrupt, it is silently rebuilt from the JSONL files on next query
20.3 Failure modes
Failure	CLI behavior
Corrupt JSONL line (invalid JSON)	Skip line, warn to stderr, continue processing remaining lines
Missing file referenced by span	Mark span unresolved, preserve edge payload
Missing index.db	Rebuild silently on next query
Corrupt index.db	Delete and rebuild silently
JSONL file locked by another process	Retry with backoff; fail with clear error after 3 attempts
Empty payload	Accept (valid JSON), flag in damask lint
The principle: never lose data, always make progress. A corrupt line in one JSONL file should never prevent querying edges in other files. The JSONL files are the source of truth; everything else is derived and recoverable.
20.4 Swarm-scale considerations
Damask's primitives are designed to function identically whether one agent or ten thousand contribute edges. At swarm scale (hundreds of agents across parallel worktrees), three dynamics emerge that the spec explicitly anticipates:
Convergence replaces authority. Individual edge confidence matters less; independent endorsement count matters more. The ranking algorithm handles this without modification — endorsement signals accumulate naturally as agent density increases.
Noise becomes filterable. Many agents will produce low-quality or redundant edges. Decay, deduplication (damask lint), and endorsement-weighted ranking ensure that only independently verified knowledge surfaces in damask at. Unverified edges decay; verified edges compound.
Merge is the synchronization point. Endorsements created on parallel worktrees converge when branches merge via git. This is intentional — git merge is the trust boundary. Real-time cross-worktree endorsement synchronization is explicitly deferred (see §21) to preserve the zero-infrastructure principle.
20.5 Aggressive compaction
damask compact --aggressive [namespace] archives edges that are very likely stale or low-value, using a configurable heuristic:
Default criteria (all must be true for an edge to be archived):
* Span unresolved for >90 days
* Zero endorsements ever received
* Confidence < 0.7
These thresholds are configurable in config.json:
{
  "compact_aggressive": {
    "unresolved_days": 90,
    "max_endorsements": 0,
    "max_confidence": 0.7
  }
}
Aggressive compaction is never automatic — it must be explicitly invoked. The archived edges remain in the append-only log and can be recovered via --history queries.

21. Future Work
These are explicitly out of scope for the current design but anticipated for later phases:
IDE/editor integration. Highlight spans with incoming edges. Show edge counts in the gutter. "3 edges ×5✓" next to a function signature. This is a presentation concern, not a protocol concern — the data model supports it today.
Graph visualization. An Obsidian-style graph view but at span level, not file level. Useful for understanding the topology of a damask. Could be a web UI that reads the SQLite index.
Auto-edge suggestion. An agent that watches file changes and suggests new edges or flags unresolved ones. Runs as a git hook or CI step.
Embedding-augmented search. Store vector embeddings alongside edge payloads for hybrid semantic/structured search. Embeddings are stored externally and referenced by path — the fact log stays human-readable.
Binary payloads. Payloads are JSON text only. Embeddings and binary data are stored externally and referenced by path.
Real-time endorsement aggregation. At swarm scale, endorsements on parallel worktrees don't converge until git merge. A lightweight synchronization layer (shared index, pub/sub, or merge-on-write) could accelerate convergence for high-density swarm workflows without compromising the append-only log. This is explicitly deferred until git-based convergence proves insufficient.
Mechanically-enforced closure convention. When a risk or gotcha becomes covered by a linter rule, type check, or CI gate, agents can record this transition: rel: "ruled_out" + payload: {"summary": "now enforced by linter rule no-implicit-any", "status": "assertion", "original_rel": "risk"}. This records that a discovery has been promoted from agent knowledge to mechanical enforcement — the highest form of closure. No new rel type is needed; the existing ruled_out vocabulary with an original_relpayload field captures this cleanly.

22. Summary
Damask is:
* Two primitives: spans and edges
* A folder of JSONL files: .damask/edges/*.jsonl (one per namespace, in git)
* One CLI: damask (Rust binary)
* Zero infrastructure: no server, no cloud, no API keys
* Git-native: committed alongside content, shared via push/pull
* Agent-native: CLI interface, JSON output, system prompt conventions
* Domain-agnostic: code, legal, research, business, personal knowledge — same primitives
* Privacy-aware: .damask/edges/.private/ for sensitive findings, tiered deterministic redaction
* Quality-controlled: damask lint with signal density heuristics, payload envelope conventions, namespace schemas
* Self-curating: Agent endorsements and disputes create a feedback loop — quality emerges from use, not from review
* Swarm-ready: the same primitives that help one agent remember help ten thousand agents converge on shared understanding
* Current-state by default: immutable event model, supersession chains, queries show active edges
* Trust-aware: two-axis freshness (resolution + recency), dynamic ranking with endorsement signals and per-namespace decay, community edge provenance
* Provenance-legible: damask why and damask blame make the trust story of any edge immediately visible
* Reviewable: damask review for human oversight of agent-created edges
The value proposition: agents create edges as a byproduct of their work, and every subsequent agent benefits from the accumulated knowledge. The damask gets richer over time through the natural accumulation of discoveries.
Like the web, Damask is a linking protocol. Unlike the web, the links are typed, bidirectional, span-level, payload-carrying, and created by agents rather than humans. The web linked documents. Damask links understanding.
Start local. Accrete through git. Share through GitHub. Index globally if it works.
The filesystem holds the content. The damask holds the understanding. The spaces in between.
