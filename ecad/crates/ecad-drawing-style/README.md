# ecad-drawing-style

Layer: Core

## Purpose
How an ECAD page is drawn and printed: sheet colours, line weights, dashes, sheet text sizes, the
sheet font and canvas markers. It is part of the drawing's definition, so it sits in Core, where the
engine, the rules and the UI kit can all reach it.

## Contents
- `sheet`: the `PAPER` and `DARK` colour sets (`Sheet`), the `Meaning` of each colour and which meanings print.
- `wire`: default wire category colours and the two-colour dash.
- `line`: line weights on the page, in mm.
- `sheet_type`: sheet text styles (cap heights, mm).
- `sheet_font`: the embedded sheet font (Barlow Semi Condensed) as bytes.
- `marker`: the shape and screen size of each canvas marker.

## Must not depend on
Nothing at all: it has no dependencies. It is plain `const` data plus the font bytes. It holds no
drawing code, no toolkit type, and not the screen's own look (shell colours, spacing, icons), which
is `ecad-tokens`.

## Key types
`Color`, `Sheet`, `Meaning`, and the `sheet::PAPER` and `sheet::DARK` sets.

## How to test
```bash
cargo test -p ecad-drawing-style
```
Tests are in `tests/all/`. One test compares every row of the "Drawing style" section of
`docs/design/tokens.md` with these constants, so edit the doc and the code together.
