<h1 align="center">Damask</h1>

<p align="center">
  <b>A knowledge fabric for AI agents.</b><br>
  Verified, code-anchored findings that outlive the session that found them —
  so your agents inherit what the last one learned instead of re-deriving it.
</p>

<p align="center">
  <em>for the greater good of agents</em>
</p>

---

## The problem

Every agent session against a codebase starts from zero. It reads the same
files, traces the same dependencies, rediscovers the same landmine —
conservatively 100–300k tokens of exploration, paid again every session,
by every agent, forever. And when the last agent worked something out — *why*
a decision was made, what broke last time, which "obvious fix" is actually
wrong — that understanding evaporates the moment the context window closes.

Worse: without a shared memory, agents don't just forget — they re-derive the
same conclusions **wrong**, over and over, with no way to accumulate trust.

## What Damask does

Damask lets agents pin **typed, confidence-scored observations** to exact
regions of your code, stored as append-only JSONL in `.damask/` right beside
the repo. The next session — another agent, a cheaper model, a teammate —
starts already knowing.

```bash
# one agent records what it learned, anchored to the code
damask record src/auth.rs 42 67 risk \
  -m "login has no rate limiter — brute-forceable" -c 0.9 --severity high

# every future session, touching that file, gets it back for ~100 tokens
damask at src/auth.rs
#   ⚠ risk (0.90) [high] src/auth.rs:42-67 — login has no rate limiter…
```

It's not "memory" (vector recall of what was *said*) and not docs (prose that
rots unread). It stores **what was understood**, and it's the only tool in its
space that does all three of these at once:

- **The knowledge follows the code.** Anchors track through edits and renames
  automatically — `git mv` a file and `at` still finds the finding, at the new
  line numbers. When code changes, findings are re-checked, not silently stale.
- **It self-prunes.** Fixed findings get closed and vanish; refuted ones sink;
  `damask triage` proposes bulk cleanup of anything anchored to deleted code.
- **It grows more trustworthy as it's used.** Independent sessions endorse or
  dispute findings; a claim confirmed by three separate agents outranks a lone
  guess, and every read surface leads with what's *still true* — honestly
  marked (✅ fresh · ↪ moved · ⚠ changed · ❌ gone) when it might not be.

## Quick start

```bash
# install from source (crates.io publish coming — see Releasing)
git clone https://github.com/runonthespot/damask-rs
cd damask-rs && cargo install --path crates/damask-cli

# in your repo
cd ~/my-project
damask init            # sets up .damask/, auto-detects your agent (see below)
damask bootstrap       # optional: seed from manifests, TODOs, co-change history
damask orient          # what does this repo already know?
```

That's the whole ceremony. `record` as you work; `at` before you edit; signal
(`endorse` / `dispute` / `close`) as you confirm or refute. The graph is just
files — commit `.damask/` and your whole team's agents share one memory.

## The loop (it runs itself)

`damask init` in a **Claude Code** repo installs hooks so the loop is
automatic and invisible:

- **SessionStart** → a graph briefing is injected — no cold start.
- **On file touch** → relevant findings for that file appear as context (`peek`).
- **Session end** → a gentle nudge if you edited but recorded nothing.

For **OpenAI Codex**, `damask init --codex` writes the same loop into
`AGENTS.md` (Codex's instruction file) as explicit steps, since Codex has no
hooks. Either way, a fresh agent arrives briefed.

## See it: the VS Code extension

`editors/vscode/` is a companion panel — the graph as a browsable, clickable
tree: findings grouped by namespace and type, a freshness ribbon, click-through
to the exact code, right-click → "copy remediation prompt" to hand a finding
straight to Claude Code.

```bash
cd editors/vscode && npm install && npx tsc -p ./ && npx @vscode/vsce package
code --install-extension damask-vscode-*.vsix
```

## A tour of the commands

| | |
|---|---|
| `orient` / `briefing` | what's known here, ranked, freshness-marked |
| `at <file>[:line]` / `at <dir>/` | findings at a location (dir → per-file rollup) |
| `where "rel=risk" "severity=critical"` | filter by any payload field |
| `record … -m "…" -c 0.9` | pin a finding to code, in one shot |
| `endorse` / `dispute` / `close` | signal — and *see the edge's history as you do* |
| `confirm <id>` | re-anchor a finding that drifted but still holds |
| `triage` / `sweep` | find rot; propose closes; heal drifted anchors |
| `why <id>` / `blame <id>` | provenance: who claimed, confirmed, disputed |
| `search` / `follow` / `tui` | full-text search, graph traversal, interactive UI |

Namespaces isolate work (per-audit, per-agent), and can **assert their own
payload schema** — `damask` over legal contracts declares `jurisdiction`
values and their ranking weights; nothing about the format is code-specific.
Run `damask help <topic>` for depth.

## Design principles

- **Protocol, not product.** Spans + edges + JSONL are the substrate. Code is
  the first domain, not the only one.
- **Record judgment, not description.** A future agent can re-read the code; it
  can't re-learn what the code *cost* you.
- **Honest by construction.** Anything the index knows — freshness, rank,
  provenance, contested state — is shown at the moment it matters. A knowledge
  layer that hides its own uncertainty is worse than none.
- **Make the right thing the automatic thing.** The loop, the history-on-signal,
  the guardrails — the correct move is always the path of least resistance.

## Building & contributing

```bash
cargo build --workspace
cargo test --workspace      # the suite CI runs
cargo fmt --all
```

Rust 1.75+. The workspace is five crates: `damask-core` (types),
`damask-resolve` (span resolution + freshness), `damask-store` (JSONL + SQLite
index + ranking), `damask-tui`, and `damask-cli`. CI runs format + build +
test on every push; see `.github/workflows/`.

## Releasing

Tag `vX.Y.Z` and push — the release workflow publishes the five crates to
crates.io in dependency order and attaches prebuilt binaries to the GitHub
release. Requires a `CARGO_REGISTRY_TOKEN` repo secret. See
[`RELEASING.md`](RELEASING.md).

## License

MIT.
