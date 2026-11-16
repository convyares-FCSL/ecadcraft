# ecad-electrical

Layer: Core

## Purpose
The electrical rules of an ECAD project, decided from the document alone: what a device, wire,
cable or terminal strip is, what the checks say, what a report lists and how a generated page is
laid out. It reads a `Document` and returns plain data (positions in mm, texts, styles, ids) or ops.
The engine crate turns that into drawing entities.

## Contents
- `devices`, `kinds`, `symbols`, `reps`, `words`, `designations`: the shared vocabulary: kinds, symbols, representations, designations.
- `wires`, `cables`, `strips`, `connectors`, `accessories`, `plc/`: the electrical objects and their rules, including PLC racks, cards and channels.
- `xref`, `references`, `continuations`, `link_lines`, `slash`: cross-references, continuations and what a cable slash crosses.
- `checks`, `proposals`: project checks (`Check`, `Severity`, `Fix`) and proposed fixes.
- `rating/`: the rating dataset; `bom`, `device_list`, `outputs`: the reports.
- `front`, `frames`, `title_block/`, `generated`, `page_types`: the layout of generated pages, frames and title blocks.
- `locations`, `revisions`, `styles`, `defaults/`, `text_style/`: locations, revisions, styles and project defaults, and text styles (text is measured through a `TextMetrics` the caller supplies).

## Must not depend on
Core rules: no `cadcraft-*` crate, no UI, no OS access (no files, clock, environment or process:
callers pass the time in) and no font lookup. It uses only `ecad-model`, `ecad-drawing-style` and
`ecad-file` (for `definition_hash`), the Core crates before it. It is the last crate in Core, so no
other Core crate may use it.

## Key types
`checks::Check`, `checks::CheckSource`, `checks::Fix`, and the functions that start from a
`Document`: `checks::page_checks`, `net_checks`, `component_checks`.

## How to test
```bash
cargo test -p ecad-electrical
```
Its tests are unit tests inside the modules (there is no `tests/` folder); behaviour that needs a
project is also covered through `ecad-cadcraft`'s tests. Slow tests are marked
`#[ignore = "slow: …"]` and run only at the QA gate.
