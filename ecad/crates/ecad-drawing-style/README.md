# ecad-drawing-style

Layer: Core

## Purpose
The look of an ECAD page, drawn on the definitions of `ecad-model` and the electrical rules of
`ecad-electrical`. It is the last crate in Core.

## Contents
- `src/lib.rs`: the crate doc and `REGISTRY`, every crate's defaults and style catalogues together.

## Must not depend on
No `cadcraft-*` crate, no UI toolkit and no UI kit. It holds neither the screen's own appearance
(shell colours, spacing, icons), which is `ecad-widgets`' tokens, nor the canvas palette, which is
`ecad_model::palette`.

## Key types
- `REGISTRY` (`ecad_model::registry::data::Registry`): what the `defaults` and `style` commands, the
  Standards screen and Checks list. The engine passes it to commands and views.

## How to test
```bash
cargo test -p ecad-drawing-style
```
