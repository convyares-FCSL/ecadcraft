# ecad-inspect

Layer: Core

## Purpose
Read-only inspection of a running or saved project: a log of what happened at run time, and a JSON
snapshot of a project at a given step. It is part of the product (a `Project` keeps the log, and
`ecad-mcp` returns snapshots), which is why it is in Core and not under `dev/`.

## Contents
Everything is in `src/lib.rs`:
- `EventLog`, `Event`, `EventKind`: runtime diagnostics (commands accepted or refused, saves, recoveries). Never saved in the project, never replayed.
- `snapshot`: a JSON snapshot of a `Ledger` at a transaction number, with readable labels and consistency checks.
- `references_json`, `over_referenced`: cable cross-reference results.

## Must not depend on
Core rules: no `cadcraft-*`, no UI. It uses only `ecad-model`. It has no way to change a project, no
network server (the inspector tool with its WebSocket server is `dev/ecad-inspector`) and no OS
access.

## Key types
`EventLog`, `snapshot`, `LiveFacts`, `SNAPSHOT_FORMAT`.

## How to test
```bash
cargo test -p ecad-inspect
```
The crate has no tests of its own. Its callers' tests cover it, for example `ecad-mcp`'s contract
tests (the `ecad_inspect` tool) and `ecad-cli`'s.
