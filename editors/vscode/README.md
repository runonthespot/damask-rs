# Damask for VS Code

A window onto the repo's knowledge graph. Findings (edges) are top-level,
ordered judgment-first (risk, gotcha, decision, …) then by confidence; the
nodes within each edge are its span anchors. Click anything — edge or
anchor — and land on the exact code region, at the graph's *effective*
location (anchors follow the code through renames and edits).

- Endorse/dispute marks (`×2✓`, `×1✗`) and closed-state shown per edge
- Live refresh when any `.damask/edges/*.jsonl` changes (append-only, so
  every change is a new fact landing)
- Toggle closed edges via the archive button; refresh via the refresh button
- Dead anchors warn and suggest `damask triage` instead of failing silently

The CLI is the API: the whole view is one `damask --format json log` call,
joined client-side. No SQLite access, no private formats — anything this
extension shows, an agent sees identically.

## Build & install

```bash
cd editors/vscode
npm install
npx tsc -p ./
npx @vscode/vsce package
code --install-extension damask-vscode-0.1.0.vsix
```

Requires the `damask` CLI on PATH (or set `damask.path` in settings) and a
workspace containing `.damask/config.json`.
