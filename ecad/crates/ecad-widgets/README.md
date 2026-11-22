# ecad-widgets

Layer: UI kit

## Purpose
ECAD's widgets and screen tokens: how a value, button, table, panel or dialog looks and behaves,
and the shell colours, spacing, sizes, radii, shadows, icon names, UI type sizes and screen strokes
they are built from. Panels and dialogs in the app build only from these, so every value looks the
same everywhere. It knows nothing about projects, documents or commands. How a drawn page looks
is not here; that is `ecad-drawing-style`.

## Contents
- `tokens`: the screen tokens as `const` data: `shell` (the Graphite and light shell colours, with
  contrast checks), `ui_type`, `space`, `size`, `radius`, `shadow`, `wheel`, `line` (screen
  strokes in pixels) and `icon` (the names of the shell's icons).
- `style`: sets egui's look from the tokens (`set_theme`).
- `control`: components for values: `TextEntry`, `Select`, `Toggle`, `ValueBox`, `TextArea`, `read_text`, `link`.
- `prim`: primitives such as `Button`, `IconButton`, `Segmented`, `Field`, `ListRow`, `PropertyGrid`.
- `panel`: section headers, value rows, `status_pill`.
- `table`: the one table every manager uses (sort, filter, group, inline edit, CSV).
- `dialog`, `splitter`, `wheel`, `icons`, `fonts`: dialogs, panes, the wheel, icons painted in code, embedded fonts.
- `showcase`: a gallery page of every component in every state, used for screenshots.

## Must not depend on
UI kit rule: only `egui`, `egui_extras` and `ecad-drawing-style` (for the shared `Color`). No
`cadcraft-*`, no Engine, no other Core crate. Colours, sizes and fonts come from here and never
from local values (`cargo xtask layers` checks this, and refuses new raw `egui::TextEdit` or
`ComboBox`).

## Key types
`control::TextEntry`, `control::Select`, `prim::Button`, `prim::Segmented`, `table::Table`,
`tokens::Shell`, `tokens::Icon`. A missing component is added here and to `showcase` (see
`docs/design/components.md`).

## How to test
```bash
cargo test -p ecad-widgets
```
Tests are in `tests/all/` and run egui headlessly. Slow tests are marked `#[ignore = "slow: …"]`.
The doc-sync test (`tokens::doc_sync`) compares every row of the token tables in
`docs/design/tokens.md` with the token constants, so edit the doc and the code together.
