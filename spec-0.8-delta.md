# Damask 0.8 — Delta from 0.7

These are new and revised sections for the 0.8 spec, based on insights from the genesis conversation. Sections marked [NEW] are entirely new. Sections marked [REVISED] replace or extend their 0.7 counterparts.

---

## [REVISED] 1. What Damask Is

Damask is an annotation graph protocol that agents weave over information.

When an agent works with any collection of files — code, documents, research, contracts, notes, media — it discovers relationships that aren't obvious from reading any single file. Dependencies between components. Contradictions between documents. Decisions and their rationale. Failure modes learned through experience. Context that connects disparate pieces of information into a coherent understanding.

Today, all of that discovered understanding evaporates when the session ends. The next agent starts from zero, paying the same exploration tax in tokens, compute, and time.

Damask captures those discoveries as structured edges between spans of content, stored as simple fact files alongside the content itself. The knowledge accumulates over time, across sessions, across agents, across projects. Each agent that touches the material inherits what previous agents learned, adds its own discoveries, and leaves the damask richer than it found it.

Damask is a protocol, not a product. The CLI is the first implementation. The JSONL format is the substrate. Code is the first domain. But nothing about spans, edges, or the fact format is code-specific — the same primitives work for legal documents, research papers, business data, personal notes, and any other artifact an agent works with.

The name comes from damask fabric — a reversible, patterned textile where the design is woven into the structure without altering the underlying material. Damask weaves understanding into information without modifying it.

---

## [NEW] 2.5 The cold start tax

Every agent session against a codebase begins with exploration. The agent reads files, searches for patterns, traces dependencies, builds a mental model. This cold start costs tokens — conservatively 100,000-300,000 tokens per session in file reads, search results, and reasoning.

This cost is paid every session. A developer running ten Claude Code sessions per day against the same codebase pays the cold start tax ten times. Multiply by millions of users and the aggregate waste is staggering — not just in dollars, but in inference compute, energy, and time.

Damask eliminates the cold start. An agent's second session inherits what the first session learned. `damask at` on the files about to be touched costs 5,000-15,000 tokens and delivers ranked, typed, confidence-scored understanding. That's a 90-95% reduction in cold start tokens per session.

At scale, this changes the economics of AI-assisted development:

- **For users:** Faster time-to-productivity, lower token costs, less waiting
- **For inference providers:** Dramatically lower compute per useful session. The same GPU fleet serves more users at better quality.
- **For the environment:** Every redundant token is wasted electricity. Eliminating cold start waste across millions of daily sessions is a measurable reduction in AI's energy footprint.

The cold start tax is invisible because everyone pays it. Damask makes it visible by showing what a session looks like without it.

---

## [NEW] 2.6 Comprehension, not recall

The market is crowded with "agent memory" solutions — vector stores, conversation history, RAG pipelines, embedding databases. They all solve the same problem: retrieving what was previously *said*.

Damask solves a different problem: persisting what was previously *understood*.

A vector store that embeds conversation fragments can tell you "someone mentioned token expiry in auth.rs." Damask tells you "there is a risk (confidence 0.95, endorsed by 3 independent agents) that token validation at auth.rs:42-67 has no expiry check, this depends on settings.py SECRET_KEY, and the recommended action is to add expiry validation."

The difference is structure. Vector recall gives you fragments. Damask gives you typed relationships, confidence scores, endorsement signals, decay-aware ranking, and provenance. It gives you a *model of understanding*, not a *log of interaction*.

This distinction matters because it determines what the agent can do with the knowledge. Retrieved conversation fragments go into the context window as noise to be re-interpreted. Structured damask edges go in as pre-digested understanding that can be acted on immediately.

---

## [NEW] 2.7 Why context windows aren't the answer

Context windows will grow to millions of tokens and beyond. Does structured querying still matter when an agent can inhale the entire codebase?

For a single small project, possibly not. But real-world software isn't a single project. A mid-size Node.js application has 1,500+ packages in node_modules. A Python ML project pulls in PyTorch, transformers, and dozens of supporting libraries. The full dependency graph of a production system — including the code you depend on, the code *that* code depends on, and all the implicit contracts between them — exceeds any foreseeable context window.

This is a graph problem, not a context problem. The question isn't "can you fit it all in the window?" It's "can you traverse relationships across boundaries?" A damask edge that says "this Express middleware at line 42 depends on a rate-limiting library that silently fails when Redis is unavailable" threads through three packages. No context window answers that query by loading all three packages. But a damask graph that spans the dependency tree does.

Context windows solve the single-file comprehension problem. Damask solves the cross-boundary understanding problem. They are complementary, not competing.

---

## [REVISED] 5.1 The Bitter Lesson

Rich Sutton's Bitter Lesson teaches that general methods leveraging computation beat hand-engineered structure. Applied to Damask: don't build domain-specific annotators into the substrate. The agent is the annotator — whether it's analyzing code, reviewing contracts, or mapping research literature.

Damask provides the thinnest possible infrastructure — spans and edges — and lets agents create whatever annotations they need. The protocol prescribes the structure. The vocabulary is emergent (with recommended conventions to bootstrap consistency).

**Applied to ranking:** The 10-signal ranking function (§10.3) is the right v1 — explicit, interpretable, debuggable. But it should be understood as temporary infrastructure, not a permanent design. The graph structure (spans, edges, relationships, endorsements, timestamps) is the durable asset. The ranking function that operates over it is swappable. As damask graphs accumulate at scale, a learned ranking function (cross-encoder, fine-tuned on endorsement/dispute/decay signals) will outperform hand-tuned weights. The protocol must not couple to any particular ranking implementation.

Do not over-engineer the ranking. Accumulate the graph. Let the ranking become learned when the data justifies it.

---

## [NEW] 5.8 Zero bootstrap cost

A critical design validation: when a Claude Code instance was given the damask CLI, a codebase, and no further instructions about what to annotate or how to organize namespaces, it autonomously produced five well-organized namespaces with 72 typed, scored observations covering architecture, risks, decisions, invariants, and spec gaps.

No schema was taught. No namespace conventions were prescribed. The agent naturally emitted structured comprehension when given the right primitive.

This is the strongest evidence that damask's format matches how agents already think. The bootstrap cost is zero — point an agent at a codebase with damask available, and it produces a knowledge graph as a byproduct of exploration. This eliminates the adoption barrier that killed previous structured knowledge systems (RDF, wiki tagging, knowledge bases). Those required humans to do unnatural work. Damask asks agents to do what they already do, but into a structured format instead of prose.

---

## [REVISED] 6.1 Span — with git anchoring

**Span:** A reference to a region within a file, optionally anchored to a specific version.

```json
{
  "id": "s_01JKXYZ...",
  "path": "src/auth.py",
  "lines": [42, 67],
  "commit": "a3f7c2d",
  "snippet": "def validate_token(token):",
  "symbol": "validate_token",
  "content_hash": "a3f7c2..."
}
```

- `id` is a ULID prefixed with `s_` (globally unique, sortable by time, merge-proof)
- `path` is root-relative (relative to the repo/project root, or package-relative for external references)
- `lines` is a line range (1-indexed, inclusive) — the default coordinate for text content
- `commit` (optional) is the git commit hash at which this span was created. Enables git-based resolution cascade (see §8.2). Omit for non-versioned content.
- `snippet` is a short text excerpt for fuzzy re-anchoring when lines shift
- `symbol` (optional) provides a semantic anchor (function name, section heading, clause number) that survives reformatting
- `content_hash` (optional) is a truncated SHA-256 of the span text, providing a durable content-derived anchor that survives line shifts
- Other coordinate systems for non-text content (see section 9)
- Spans are cheap to create and expected to drift as content changes

The `commit` field is the key addition in 0.8. It transforms span resolution from "search the current file and hope for a match" to "ask git where this content went." See §8.2 for the full resolution cascade.

---

## [REVISED] 8. Span Resolution and Staleness

### 8.1 The mutation problem

Content changes. When a file is edited, spans pointing into it may shift or become invalid. A damask full of broken spans is misleadingly wrong.

Previous approaches to this problem either ignore staleness (annotations silently become wrong) or treat any change as total invalidation (everything is always "stale," so users stop trusting the signal). Damask takes a third approach: **graceful degradation through a resolution cascade**, where each level of degradation is explicitly named and visible.

### 8.2 Git-anchored resolution cascade

When a span carries a `commit` field, resolution proceeds through four levels, each less precise but still useful. Git's own machinery — diff, blame, rename detection — handles the hard work:

**Level 1: Exact**
Content at `commit:path:lines` matches the current HEAD at `path:lines`. Content hash confirms. The span is fully trustworthy.

**Level 2: Relocated within file**
The file at `path` exists in HEAD but the content at `lines` has changed. Resolution falls back to content hash, symbol, and snippet search within the same file. If found at different lines, the span is relocated. Coordinates are updated in the index (not in the JSONL — the log is immutable).

```
commit:path:lines  →  fails (lines changed)
commit:path        →  file exists, search within file
                       found at new lines via content_hash/symbol/snippet
                       → relocated
```

**Level 3: Relocated across files**
The file at `path` no longer exists in HEAD, or content cannot be found within it. Resolution uses `git log --follow` and `git diff --find-renames` to trace where the file moved. If the file was renamed or moved, search for the span content within the new file.

```
commit:path        →  fails (file moved/renamed)
commit:(new path)  →  git rename detection finds destination
                       search within new file for content
                       → relocated (path + lines updated in index)
```

**Level 4: Unresolved**
All anchors fail. The span cannot be located in the current HEAD. The edge payload survives — "there was a token expiry risk in the auth module" is still useful information even if the specific code can no longer be pointed at.

```
all anchors fail   →  unresolved
                       payload preserved, knowledge persists
                       edge ranked lower but not hidden
```

**The key insight: each level degrades precision but preserves knowledge.** An unresolved span is not a dead span. The edge's summary, confidence, action, and provenance remain intact. Freshness status tells the user exactly how much to trust the location, while the payload tells them what was understood.

### 8.3 Resolution without git

For content not under version control (legal documents, research papers, standalone files), resolution falls back to the 0.7 multi-anchor approach:

1. **Path + coordinates:** Check if the file exists and content at the location matches the snippet
2. **Content hash:** Search the file for a region whose hash matches
3. **Symbol:** Search for the symbol (function name, section heading, clause number)
4. **Snippet:** Fuzzy-search for the snippet text
5. **Unresolved:** All anchors fail, payload preserved

The git-based cascade is strictly additive — it provides better resolution for versioned content without removing capabilities for unversioned content.

### 8.4 Time-travel queries

Git-anchored spans enable a capability not possible in 0.7: querying the damask at a historical point.

```bash
damask at src/auth.rs:42 --at v2.0
```

This resolves spans against the tagged commit rather than HEAD. "What did we know about this function at the time of release?" The damask becomes a knowledge graph over the project's *history*, not just its current state. This is valuable for audits, post-mortems, and understanding how understanding evolved.

### 8.5 Freshness tracking

[Unchanged from 0.7 §8.3, but the two-axis model now maps cleanly to the resolution cascade:]

| Cascade level | Resolution | Recency | Display |
|---|---|---|---|
| Level 1: exact match | `exact` | `unchanged` | Fully trustworthy |
| Level 2: found in same file | `relocated` | `file_changed` | Span moved, review recommended |
| Level 3: found in renamed file | `relocated` | `file_changed` | File moved, span re-anchored |
| Level 4: all anchors fail | `unresolved` | — | Payload preserved, location lost |

### 8.6 Graceful degradation as design principle

The resolution cascade embodies a broader principle: **knowledge should degrade gracefully, not fail catastrophically.**

A span that was created at `commit:path:lines` and can now only be resolved to `(new path):(new lines)` has lost precision but retained utility. An edge whose span is fully unresolved has lost location but retained its payload — the summary, confidence, action, and provenance are still useful.

This is why the edge payload should carry enough context to be useful even without a resolvable span. The `summary` field isn't just for display — it's the durable knowledge that survives when everything else degrades.

---

## [NEW] 14.4 Cross-codebase damask graphs

### The dependency graph problem

A mid-size Node.js application has 1,500+ packages in node_modules. Each package has its own internal architecture, its own risks, its own undocumented behaviors. An agent working on the application needs to understand not just the application code but the contracts and failure modes of its dependencies.

No context window — 1M, 10M, or beyond — holds this entire graph. But a damask layer over each dependency, with cross-project edges threading between them, makes the relevant knowledge queryable without loading the full source.

### Cross-project edge threading

Consider an Express application that depends on a rate-limiting library:

```jsonl
{"t":"span","id":"s_app1...","path":"src/middleware/auth.ts","lines":[42,67],"commit":"abc123","snippet":"app.use(rateLimiter({"}
{"t":"span","id":"s_dep1...","path":"rate-limiter:src/index.ts","lines":[120,145],"commit":"v2.3.0","snippet":"if (!redis.isConnected)"}
{"t":"edge","id":"e_cross1...","from":"s_app1...","to":"s_dep1...","rel":"depends_on","payload":{"summary":"Rate limiter silently disables when Redis is unavailable — auth middleware has no fallback","confidence":0.90,"action":"Add Redis health check to startup"}}
```

An agent working on the application discovers that the rate limiter fails silently. The edge threads from the application span to the dependency span, carrying the discovered understanding. A future agent running `damask at src/middleware/auth.ts:42` gets this cross-boundary insight without reading the rate-limiter source.

### Community damask as dependency knowledge

Community-maintained damask packages (§14.3) become most valuable in this context. A `damask-community/express` package that documents Express's common gotchas, a `damask-community/react` package that maps React's hook dependency rules — these are the dependency graph knowledge that individual developers discover and rediscover.

When an agent pulls `damask-community/express` and the project's own damask has edges pointing into Express, the two graphs stitch together. The agent inherits both the community's understanding of Express and the project's specific usage of it.

### Resolution of cross-project spans

Cross-project spans use the prefixed path convention (`package:path`). Resolution depends on context:

- For npm dependencies: resolve within `node_modules/package/`
- For Python packages: resolve within the installed package location
- For versioned references: the `commit` field can carry a version tag (`v2.3.0`) instead of a git hash

When cross-project resolution fails, the edge is marked `external_unresolved` and the payload is still displayed. Knowledge about a dependency doesn't become worthless because the dependency was upgraded — the summary "rate limiter silently disables without Redis" is useful regardless of whether the specific line can be pointed at.

---

## [NEW] 19.8 The accretion model

### How understanding accumulates globally

When a developer uses Claude Code with damask, three things happen:

1. The agent creates edges as a byproduct of its work (zero additional effort)
2. The developer commits `.damask/` alongside their code (natural git workflow)
3. Anyone who clones the repo inherits the accumulated understanding

This is the accretion model: understanding accumulates through normal use and propagates through normal distribution (git push/pull).

At small scale, this saves individual developers from cold start waste. At large scale, something qualitative emerges: **a global graph of structured comprehension over open source code, committed alongside the code itself on GitHub.**

This graph doesn't exist today. GitHub has code. Stack Overflow has Q&A. Documentation sites have prose. Nobody has the understanding layer — the risks, decisions, invariants, cross-file dependencies, and failure modes that agents and humans discover through working with code.

If `.damask/` directories become common in GitHub repositories, the accumulated knowledge becomes:

- **A reusable asset** for anyone cloning the repo (immediate onboarding)
- **A training signal** for future models (structured comprehension data, not just code)
- **A quality indicator** for the project (richly damasked repos are better understood than un-damasked ones)
- **A visible record** of which agents produced the deepest understanding

### The training data flywheel

Every damask graph is a structured record of how code is comprehended — not written (GitHub has that), not discussed (Stack Overflow has that), but *understood*. Risks identified with confidence scores. Decisions with rationale. Invariants discovered through analysis. Dependencies mapped through exploration. Hypotheses tested, endorsed, disputed, resolved.

This dataset does not exist at scale. As damask usage grows, it produces the first corpus of typed, scored, location-anchored comprehension data. Models trained on this data would arrive at new codebases with structured intuitions — knowing that auth modules tend to have token expiry risks, that ORMs hide N+1 queries, that config files drift from documentation. Not because they memorized specific code, but because they learned patterns of understanding from millions of damask graphs.

This is the long-term value that compounds: agents populate damask, damask data improves agents, improved agents populate better damask.

### Sustainability

Every redundant cold start token is wasted energy. Across millions of daily agent sessions, the aggregate waste in inference compute is measurable in megawatt-hours. Damask reduces this waste at the source — not by making inference more efficient, but by eliminating the need for redundant inference entirely.

This makes damask relevant to the growing conversation about AI's environmental footprint. The most effective way to reduce AI's energy consumption is not more efficient hardware — it's eliminating unnecessary computation. The cold start tax is unnecessary computation at massive scale.

---

## [NEW] 20.6 Convergent verification in practice

Section 5.7 describes convergent verification as a theoretical property. The genesis conversation provided an empirical demonstration.

Two independent Claude instances — with no shared context — were given the damask CLI and the same codebase. Both independently:

- Identified the same architectural patterns
- Flagged the same risks (stubbed ranking signals, full index rebuild, untested TUI)
- Arrived at the same strategic conclusions (protocol not product, training data flywheel, design decisions being non-obvious and mostly correct)
- Identified the same primary weakness (anchoring fragility)

Neither instance was told what the other concluded. The convergence of their independent analyses is itself evidence that damask's endorsement model works: when multiple agents independently confirm the same observations, the signal is qualitatively stronger than any single agent's confidence score.

This suggests that at swarm scale (hundreds of agents across worktrees), the endorsement count on an edge is a more reliable quality signal than individual confidence — not because any single endorsement is authoritative, but because independent convergence is hard to fake and easy to measure.

---

## [REVISED] 22. Summary

Damask is:

- **A protocol:** Two primitives (spans and edges), one format (JSONL), one distribution mechanism (git). Code is the first domain, not the only domain.
- **Zero-bootstrap:** Agents naturally populate damask as a byproduct of work, without schema instruction or explicit guidance.
- **Cold-start eliminating:** 90-95% reduction in exploration tokens for returning sessions. Every subsequent agent inherits what previous agents learned.
- **Git-anchored:** Spans carry optional commit hashes, enabling git-native resolution cascade with graceful degradation through rename detection and blame.
- **Comprehension, not recall:** Stores typed, scored, ranked understanding — not conversation fragments.
- **Graph-native:** Traverses relationships across files, across packages, across codebases. Solves cross-boundary understanding that context windows cannot.
- **Self-curating:** Endorsements and disputes create a feedback loop. Quality emerges from convergent verification, not from human review.
- **Gracefully degrading:** When anchors break, precision degrades but knowledge persists. An unresolved span is not a dead span.
- **Accretive:** Committed alongside code on GitHub, understanding accumulates globally through normal git workflow.
- **Durable:** The graph structure is the permanent asset. Ranking functions, search engines, and query interfaces are swappable layers on top.
- **Sustainability-relevant:** Eliminates the cold start tax — millions of redundant inference tokens per day — reducing AI's computational and energy waste.
- **Domain-agnostic:** Code, legal, research, business, personal knowledge — same primitives, same format, same CLI.

The filesystem holds the content. The damask holds the understanding. The spaces in between.
