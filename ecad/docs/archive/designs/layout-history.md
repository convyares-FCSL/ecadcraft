Historical record (2026-11-22). Not current; see [design/layout.md](../../design/layout.md).

# Source layout: walkthroughs and order of the moves (extract)

Taken unchanged from `design/layout.md` for the archive.

## Walkthroughs: the files you open today and after

| Feature | Today | After |
| --- | --- | --- |
| **Cables** | 13 files in 3 crates: electrical `cables.rs`, `slash.rs`, `cable_look.rs`, `words.rs`, `cmd/cable.rs`, `cmd/cable_draw.rs`; engine `cmd/cable.rs`, `cmd/cable_draw.rs`, `look.rs`, `views.rs`; app `lists/cable_*.rs` (4) | electrical `cables/` + engine `cables/` + app `cables/` |
| **PLC** | 27 files: electrical `plc/` (14), `cmd/plc.rs`, `defaults/plc.rs`; engine `cmd/plc/` (8), `draw/plc/` (3); app `lists/plc.rs`, `lists/card_editor.rs` | electrical `plc/` + engine `plc/` + app `plc/` (defaults stay in `defaults/plc.rs`) |
| **Terminal strips** | 17 files: electrical `strips.rs`, `strips/pages.rs`, `accessories.rs`, `terminal_types.rs`, `link_lines.rs`, `cmd/terminal.rs`, `cmd/accessory.rs`, `cmd/level.rs`; engine `strip_page.rs`, `cmd/terminal.rs`, `cmd/accessory.rs`, `cmd/level.rs`, `interact/terminals.rs`, `interact/accessories.rs`; app `lists/terminals.rs`; model `device/terminal_strip.rs` | electrical `terminal_strips/` + engine `terminal_strips/` + app `terminal_strips/`; the TerminalStrip and TerminalBlock types stay in model `objects/devices/` |
| **References** | 12 files, 4 names: electrical `xref.rs`, `references.rs`, `cmd/xref.rs`, `cmd/reference.rs`, `derived/refs.rs`; engine `draw/xref.rs`, `draw/references.rs`, `interact/refs.rs`, `interact/xrefs.rs`, `cmd/xref.rs`, `cmd/reference.rs` | electrical `references/` + engine `references/` + app `references/`; one word |
| **Title blocks** | 12 files: electrical `title_block/` (4), `frames.rs`, `cmd/title_block.rs`, `styles/frame.rs`; engine `draw/title_block/` (3), `frame.rs`, `cmd/title_block.rs`; app `library/title_blocks/` (6); model `title_block.rs`, `style/frame.rs` | electrical `title_blocks/` + engine `title_blocks.rs` + app `title_blocks/`; the frame (border, columns) belongs to `pages/` in both crates, its style to `styles/` |
| **Circuits / devices** (ratings split) | electrical `rating/` (mod, circuit, dataset, load, parse, protection, trace, reader.rs + reader/ 6 files), `cmd/rating.rs`; engine `cmd/rating.rs`; app `lists/rating.rs` | the readers in electrical `devices/ratings/`; the sizing and circuit maths in electrical `circuits/`; no engine file (the rating commands are generic) |

## Order of the moves (after approval)

One feature per slice, two workers at most, with `git mv`. Each slice does, for its feature: the folder
(#25, #65), its big-file splits (#26, #43), repointing away from the engine facade (#35, #49, #50), the
names (#42, #66), narrow `pub` (#68), its tests by area (#11, #20, #46, #54, #60) and its rule tests into
`ecad-electrical/tests/all` (#69). Order: references, cables, terminal_strips, plc, title_blocks,
circuits/devices, then the rest by size; ecad-model's grouping (#67); then the app; then the crate merges
and deletes (ecad-widgets, ecad-inspect). Small lanes (#63, #48, #57, #39, #32 + approved device gaps) run
between slices.
