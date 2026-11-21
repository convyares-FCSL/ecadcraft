# ecad-engine

Layer: Engine

## Purpose
The link between the ECAD project and CADCraft's drawing engine. It draws each page from the model,
turns CADCraft edits into ECAD transactions, owns undo, redo and save, and runs the ECAD commands.
It is the only ECAD crate that uses `cadcraft-engine`, and it has no UI.

## Contents
- `project`, `access`: `Project` owns the document and the open page drawings; `ProjectHandle` is the only way to reach it from outside, under a lock.
- `translate`, `history`, `file`, `oops`, `merging`, `release`: turning engine commits into transactions, undo and redo, the `.ecad` file bound to a project, merging and releases.
- `draw/`: draws pages from the model (devices, wires, text, checks, PLC, title blocks, generated pages).
- `cmd/`: **the ECAD commands.** `cmd/mod.rs` is the registry (`specs`) and the shared helpers; each area has its own file or folder (`cmd/wire.rs`, `cmd/cable.rs`, `cmd/plc/`…).
- `lanes.rs`: commands of lanes still in flight register here, one line each, until the phase-end tidy-up moves them into `cmd::specs`.
- `interact/`: the click-based forms of the commands, marks drawn over pages, hover text.
- `agent/`: the write gate and bridge for agents (shared by the window and `ecad-mcp`).
- `debug`, `verify`, `printbook`, `views`, `look`, `preview`, `strip_page`: debugger copies and checks, the PDF book, cable views, preview.
- `test_support`: `ProjectBuilder`, for tests of this and later crates.

The electrical work inside commands is being moved into `ecad-electrical` in the typed-parameters
lane (R6). That leaves thin shims in `cmd/`: read the parameters, call the electrical rule, commit
the ops.

## Must not depend on
Engine rules: no UI crate, no `ecad-ui`, no `ecad-tokens`, no `cadcraft-ui-egui`, nothing in App,
Programs or Dev. It may use Core, `cadcraft-engine` and `cadcraft-io`. It has no file encoding of
its own (that is `ecad-file`) and puts no ECAD meaning into upstream crates.

## Key types
`Project`, `ProjectHandle`, `cmd::specs` (the command registry), `translate` (commits to
transactions).

## How to test
```bash
cargo test -p ecad-engine
```
Tests are in `tests/all/` (one executable). A new lane adds `tests/all/lanes/lane_<NNN>.rs`.
Slow tests are marked `#[ignore = "slow: …"]` and run only at the QA gate. Every command also has
an MCP contract test in `ecad-mcp`.
