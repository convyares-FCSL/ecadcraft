# ecad-tokens

Layer: UI kit

## Purpose
How the ECAD screen looks: shell colours, spacing, sizes, radii, shadows, icon names, UI type
sizes and screen strokes, as `const` data. How a drawn page looks is not here; that is
`ecad-drawing-style`.

## Contents
- `shell`: the Graphite and light shell colours (`GRAPHITE`, `LIGHT`), with contrast checks.
- `ui_type`, `space`, `size`, `radius`, `shadow`, `wheel`: the type ramp and geometry.
- `line`: screen strokes in pixels.
- `icon`: the names of the shell's icons (they are painted by `ecad-ui`).

## Must not depend on
UI kit rule: only `ecad-drawing-style` (for the shared `Color`). No `cadcraft-*`, no other Core
crate, no Engine. No drawing code and no toolkit type.

## Key types
`Shell`, `Color` (from `ecad-drawing-style`), `Icon`.

## How to test
```bash
cargo test -p ecad-tokens
```
Tests are in `tests/all/`. The doc-sync test compares every row of the token tables in
`docs/design/tokens.md` with these constants, so edit the doc and the code together.
