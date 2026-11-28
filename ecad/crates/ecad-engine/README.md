# ecad-engine

Layer: Engine

## Purpose
The link between the ECAD project and CADCraft's drawing engine. It draws each page from the model,
turns CADCraft edits into ECAD transactions, owns undo, redo and save, and runs the ECAD commands.
It is the only ECAD crate that uses `cadcraft-engine`, and it has no UI.

## Contents
- `project/`, `access.rs`: `Project` owns the document and the open page drawings, with its session state in groups (`project/parts.rs`: storage, display, counts, frame, indexes, defaults, asks, caller); `ProjectHandle` is the only way to reach it from outside, under a lock.
- `translate/`, `history/`, `file.rs`: turning CADCraft commits into transactions, undo and redo, versions, branches and merging, and the `.ecad` file bound to a project.
- `commands/`, `generic.rs`: the command registry and shared helpers, and the one runner of the generic commands declared in `ecad-electrical`.
- `pointer/`: what is under the cursor and the running tool's intent, marks drawn over pages, hover text, and what the click-based forms share (picking, capture, previews). Each form is in its feature area's `interact` module.
- `convert.rs`, `cad.rs`: building CADCraft entities from what the rules decide (with each feature area's `draw` module), and the CADCraft names the app and programs may use.
- Feature areas, each the engine half of the same area in `ecad-electrical` (its session commands, drawing and click forms): `cables/`, `checks.rs`, `connectors/`, `continuations/`, `devices/`, `locations/`, `pages/`, `plc/`, `properties/`, `references/`, `releases/`, `reports/`, `styles/`, `symbols/`, `terminal_strips/`, `title_blocks/`, `wires/`.
- `agent/`: the write gate and bridge for agents (shared by the window and `ecad-mcp`).
- `background.rs`, `warm.rs`: work off the UI thread and the kept views' first builds.
- `export/`: Print Book (PDF).
- `dev/`: the debugger's copies and checks, verify, the snapshot, and `test_support` (`ProjectBuilder`, for tests of this and later crates).

## Must not depend on
Engine rules: no UI crate (no `ecad-widgets`, no `cadcraft-ui-egui`), nothing in App,
Programs or Dev. It may use Core, `cadcraft-engine` and `cadcraft-io`. It has no file encoding of
its own (that is `ecad-file`) and puts no ECAD meaning into upstream crates.

## Key types
`Project`, `ProjectHandle`, `commands::specs` (the command registry), `cad` (the engine
boundary for the app and the programs), `ProjectFileError` (why opening, saving or switching a branch
failed, as the choice it leaves the person: damaged, other format, locked, not found, access refused,
read-only, replaced, other I/O).

## How to test
```bash
cargo test -p ecad-engine
```
Tests are in `tests/all/` (one executable), one folder per feature area (`tests/all/<area>/`).
Slow tests are marked `#[ignore = "slow: …"]` and run only at the QA gate. Every command also has
an MCP contract test in `ecad-mcp`.
