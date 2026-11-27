# ecad-xtask

Layer: Dev

## Purpose
The workspace's own commands, run as `cargo xtask <command>`: the layer and README checks, the
patch guard, the full CI run, the benchmark and the scoped test run. It is a developer tool and is
never shipped.

## Contents
- `layers`: the crate-map rules, one module per family: `src/layers.rs` (layers, no OS in Core or the app, `anyhow`,
  `tokio` and `rayon`, dev tools apart), `src/ui_style.rs` (UI components and tokens), `src/drawn_text.rs`,
  `src/errors.rs` (typed errors), `src/readme.rs` (a README per crate whose Contents names real files), and
  the `src/*_guards.rs` modules (typed model, views, engine rules, starters, no history in the comments of
  `src/` or `defaults/*.toml`).
- `patches`: files changed outside `ecad/` must be listed in `UPSTREAM-PATCHES.md`.
- `lint`, `ci`: fmt, clippy, tests (slow set included), layers, patches (the QA gate's run).
- `test-changed`: fast tests of the crates not pushed yet (`--with-dependents` adds their dependents).
- `bench`, `demo`: the load benchmark and the demo session.
- `clean-builds` (`src/clean.rs`): deletes build folders and merged worktrees.

## Must not depend on
Dev may use any crate, but nothing may use it. It holds no product logic: it reads `cargo
metadata` and runs `cargo`. `anyhow` is allowed here.

## Key types
There are none to start from; the entry point is `main` in `src/main.rs`. Each rule family's tables
(lists with reasons, no counts) sit at the top of its module. To change a layer rule, edit `TABLE` in
`src/layers.rs` and the layer table in `docs/developer/architecture.md`.

## How to test
```bash
cargo test -p ecad-xtask
```
The tests are unit tests in `src/`. Run a command with `cargo xtask layers` and so on.
