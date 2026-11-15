# ecad-electrical

Layer: Core

## Purpose
The electrical rules of an ECAD project: what its devices, wires, cables, terminals, references,
PLC channels and reports mean, and how their generated pages are laid out, decided from the
document alone, without a drawing engine or a screen.

## Contents
- `devices`, `wires`, `cables`, `strips`, `connectors`, `accessories`, `terminal_types`: the objects and their rules.
- `references`, `continuations`, `xref`, `link_lines`, `slash`: cross-references and what a page shows of them.
- `checks`, `proposals`, `rating`: project checks, proposed fixes and the rating dataset.
- `bom`, `device_list`, `outputs`, `plc`, `generated`, `front`: reports and generated pages, as layouts.
- `title_block`, `frames`, `page_types`, `locations`, `revisions`, `styles`, `defaults`: page furniture and project settings.
- `text_style` (with `text_style::fonts`), `symbols`, `reps`, `kinds`, `words`, `designations`: shared vocabulary.

## Must not depend on
Any `cadcraft-*` crate, any UI crate, `ecad-ui`, `ecad-tokens`, the Engine, App, Programs or Dev
layers. No OS access: no files, no clock, no environment (callers pass the time). No font lookup:
text is measured through `text_style::fonts::TextMetrics`, which the caller supplies.

## Key types
`text_style::fonts::TextMetrics` and `FixedWidth`, `frames::FrameInput`, `generated::GenKind`, `checks::Check`.

## How to test
```bash
cargo test -p ecad-electrical
```
The unit tests live next to the code. Most behaviour is tested through commands in
`ecad-cadcraft`'s `tests/all/`.
