# Damask

Damask is a knowledge fabric for AI agents — structured memory layered over codebases. This repo is the Rust implementation.

## Architecture

Four crates, layered with no circular dependencies:

```
damask-cli          Binary + clap commands
damask-store        JSONL I/O, SQLite index, ranking, predicates
damask-resolve      Content hashing + freshness detection + git rename tracking
damask-core         Pure types (Span, Edge, Fact, IDs) — no I/O
```

Visual browsing lives in the VS Code extension (`editors/vscode/`).
`damask-tui` (terminal UI) is mothballed — kept in-tree but `exclude`d from
the workspace build; revive by dropping the exclude and restoring the `tui`
subcommand.

## Conventions

- JSONL source logs are **append-only and immutable**. Never modify them in place. `compact` writes to `.views/`.
- The SQLite index is ephemeral — always rebuildable from JSONL. Never treat it as source of truth.
- Meta-edges (endorsed, disputed, supersedes, invalidates) use `from=target_edge, to=null`.
- All commands must support `--format json` and `--ns <name>`.
- `cargo test` must pass before committing. Currently 157+ tests across all crates.

## Damask Skill

This project has a `/damask` skill for working with damask annotations. Use it when exploring, recording findings, or checking known risks.
