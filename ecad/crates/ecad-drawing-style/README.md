# ecad-drawing-style

Layer: Core

## Purpose
The look of an ECAD page, drawn on the definitions of `ecad-model` and the electrical rules of
`ecad-electrical`. It is the last crate in Core.

## Contents
- `src/lib.rs`: the crate doc and `REGISTRY`, every crate's defaults and style catalogues together.
- `src/text/`: text styles (a kind's resolved height, weight, colour and style name, own styles,
  which texts are shown), `src/text/fonts.rs` (the fonts a project is drawn with, their widths, `FontLook`)
  and `src/text/commands.rs` (`textstyle`, and the rules of `showtext`).
- `src/catalogues/`: the text, location box and frame style catalogues (fields, built-ins, how a
  field resolves, the frame's and box's resolved look).
- `src/title_blocks/`: the block a frame draws (its own, or the built-in house block computed for
  the sheet) and the title block commands (`titleblocks`, `titleblock`).
- `src/defaults/`: the look's registry entries (font, Shown settings, style choices) and its data
  set, built from `defaults/text.toml` and `defaults/styles.toml`.

## Must not depend on
No `cadcraft-*` crate, no UI toolkit and no UI kit. It holds neither the screen's own appearance
(shell colours, spacing, icons), which is `ecad-widgets`' tokens, nor the canvas palette, which is
`ecad_model::palette`.

## Key types
- `REGISTRY` (`ecad_model::registry::data::Registry`): what the `defaults` and `style` commands, the
  Standards screen and Checks list. The engine passes it to commands and views.
- `text::fonts::FontLook`: answers `ecad_model::look::Look` over the drawing engine's
  `TextMetrics`; the electrical rules lay out and measure their texts through it.
- `text::fonts::Fonts` and `DrawStyles`: the fonts found on this machine, per kind.
- `catalogues::frame::frame_draw` and `catalogues::location::box_draw`: a page's frame and a
  location box as drawn.

## How to test
```bash
cargo test -p ecad-drawing-style
```
