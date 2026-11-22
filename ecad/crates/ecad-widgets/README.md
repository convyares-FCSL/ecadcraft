# ecad-ui

Layer: UI kit

## Purpose
ECAD's widgets: how a value, button, table, panel or dialog looks and behaves. Panels and dialogs
in the app build only from these, so every value looks the same everywhere. It knows nothing about
projects, documents or commands.

## Contents
- `style`: sets egui's look from `ecad-tokens` (`set_theme`).
- `control`: components for values: `TextEntry`, `Select`, `Toggle`, `ValueBox`, `TextArea`, `read_text`, `link`.
- `prim`: primitives such as `Button`, `IconButton`, `Segmented`, `Field`, `ListRow`, `PropertyGrid`.
- `panel`: section headers, value rows, `status_pill`.
- `table`: the one table every manager uses (sort, filter, group, inline edit, CSV).
- `dialog`, `splitter`, `wheel`, `icons`, `fonts`: dialogs, panes, the wheel, icons painted in code, embedded fonts.
- `showcase`: a gallery page of every component in every state, used for screenshots.

## Must not depend on
UI kit rule: only `egui`, `egui_extras`, `ecad-tokens` and `ecad-drawing-style`. No `cadcraft-*`, no
Engine, no other Core crate. Colours, sizes and fonts come from here and never from local values
(`cargo xtask layers` checks this, and refuses new raw `egui::TextEdit` or `ComboBox`).

## Key types
`control::TextEntry`, `control::Select`, `prim::Button`, `prim::Segmented`, `table::Table`.
A missing component is added here and to `showcase` (see `docs/design/components.md`).

## How to test
```bash
cargo test -p ecad-ui
```
Tests are in `tests/all/` and run egui headlessly. Slow tests are marked `#[ignore = "slow: …"]`.
