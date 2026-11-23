# ecad-electrical

Layer: Core

## Purpose
The electrical rules of an ECAD project, decided from the document alone: what a device, wire,
cable or terminal strip is, what the checks say, what a report lists and how a generated page is
laid out. It reads a `Document` and returns plain data (positions in mm, texts, styles, ids) or ops.
The engine crate turns that into drawing entities.

## Contents
One folder per feature area under `src/` (the crate doc in `src/lib.rs` gives one line each):
- `devices/`, `symbols/`, `locations/`, `numbering/`: devices and tags, device kinds and ratings, starter symbols, locations and designations, renumbering.
- `wires/`, `cables/`, `terminal_strips/`, `connectors/`, `plc/`: the electrical objects and their rules: wires and routes, cables and cable slash, terminal strips with terminal blocks and jumper bars, connectors, PLC racks and channels.
- `references/`, `continuations/`: device and cable references, continuation arrows.
- `checks/`, `circuits/`: the Checks list, and wire and cable sizing (each result a Checks row).
- `reports/`, `releases/`: the BOM, wire and cable reports; revisions, issue and release.
- `pages/`, `title_blocks/`, `styles/`, `defaults/`, `properties/`: page types, frames and generated pages, title blocks, the electrical style catalogues and text styles, the electrical registry entries and their built-in data set (the mechanism is `ecad_model::registry`), the Properties palette rows.
- `commands/`, `input.rs`, `error.rs`, `merge.rs`, `views/`: the generic command runner, typed input grammars, the crate's errors, merge value rules, and the update pipeline for derived views.

## Must not depend on
Core rules: no `cadcraft-*` crate, no UI, no OS access (no files, clock, environment or process:
callers pass the time in) and no font lookup. It uses only `ecad-model`, `ecad-store` and
`ecad-file` (for `definition_hash`), the Core crates before it. Only `ecad-drawing-style` comes
after it in Core.

## Key types
`checks::Check` and `checks::Fix` (a Checks row and its fix), `commands::run::Generic` (a
generic command's parameters), `views::Views` (the derived views of a project), and
`checks::nets::page_checks`, which starts from a `Document`.

## How to test
```bash
cargo test -p ecad-electrical
```
Unit tests sit inside the modules; the integration tests are one executable, `tests/all/main.rs`,
with one folder per feature area (`tests/all/<area>/`), so `cargo test -p ecad-electrical <area>`
runs one area. Behaviour that needs the engine is covered by `ecad-engine`'s tests. Slow tests are
marked `#[ignore = "slow: …"]` and run only at the QA gate.
