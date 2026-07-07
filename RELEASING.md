# Releasing damask

## Publishing to crates.io

Internal dependencies carry `version` alongside `path`, so the workspace is
publishable as-is. Dependent crates will only package once their deps exist
on the registry — publish strictly in this order, waiting for each to appear
in the index (usually < 1 min):

```bash
cargo login            # once, with your crates.io token
cargo publish -p damask-core
cargo publish -p damask-resolve
cargo publish -p damask-store
cargo publish -p damask-tui
cargo publish -p damask       # the CLI — this is the `cargo install damask` name
```

Pre-flight (no token needed):

```bash
cargo package -p damask-core          # full verify
cargo test --workspace
```

After the first publish, the guarded SessionStart hook's fallback message
("Install the damask CLI to inherit it: cargo install damask") becomes true
for every teammate cloning a repo with a committed `.damask/` — the repo
recruits its own installs.

## Version bumps

`version` lives once in `[workspace.package]` and the internal dep versions
in `[workspace.dependencies]` — bump both together.

## Fast-follow (not yet set up)

- Prebuilt binaries + `cargo binstall` metadata via a release CI workflow.
- Homebrew tap once binaries exist.
