# Brief 051: PLC visuals and the rack overview

Status: Draft
Branch: `feature/plc-visuals`, from `origin/dev` `83300e3b` (045 merged).
Designer: **yes** (`docs/design/051.md`) · Size: L
Roadmap: follows M1 item **1.3** (045). Brings forward the visible part of 2.8 that 045 deferred:
screen m of the UI spec (the rack overview). Lanes D (`plc/`), S (`symbols.rs`, one block), T (two
text kinds), U (`generated.rs`, `ecad-ui` table, Lists).

## Goal

PLC cards and channels look like the owner's drawing pack and the UI mock-up: a card is one box
sized to its channels, wires end on its top edge, pins, addresses and functions are readable and
never overlap, and its name, maker and part number sit inside it. Lists ▸ PLC opens on the rack as
screen m shows it. The I/O list and a rack overview print as sheets in the pack's report look.

## Why

The owner on 045's screenshots: "a bit disappointing, does not look like the design file". 045
built the data and commands correctly, but the cell texts are tiny and overlap, wire numbers land
inside the cells, wires run through them, the card bodies are bare boxes away from their channels,
the I/O sheet has no ruling and starts a sheet per card, and screen m was deferred. Design §0.1 lists
each fault with its cause in the code.

## What exists today (`origin/dev` `83300e3b`)

- 045: racks, slots, cards, channel cells (`chdi` … `chiol`), the fence (`plcchannels`), live
  addresses, `plcracks`, `iolist`, the `iolist` page, Lists ▸ I/O (read-only column set).
- 031 text kinds (050 is turning them into built-in text styles, in flight); 039's table; 038's
  Lists workspace; 024/033 generated pages; 028 wire splitting.

## §0 Files touched

Design §0.3 has the full table. In short:

| File | Change |
| --- | --- |
| `ecad/crates/ecad-cadcraft/src/plc/look.rs` (**new**) | Runs, bands, frames, `parts_on_page` |
| `ecad/crates/ecad-cadcraft/src/plc/overview.rs` (**new**) | Tile model; `plcrack` sheet drawing |
| `ecad/crates/ecad-cadcraft/src/plc/{mod,channels,iolist}.rs` | Text placement from bands; fence spacing and wire trim; `plcpage`; I/O sheet groups |
| `ecad/crates/ecad-cadcraft/src/symbols.rs` (lane S, one block; 048 also edits) | Cell widths and graphics, card bodies as the power end, cell anchors |
| `ecad/crates/ecad-cadcraft/src/project.rs`, `verify.rs` | One loop each for the frame parts |
| `ecad/crates/ecad-cadcraft/src/devices.rs` | Two `kind_of` arms |
| `ecad/crates/ecad-cadcraft/src/text_style.rs`, `defaults/text.rs` (lane T; 050 in flight) | Text kinds `address`, `channel_text` |
| `ecad/crates/ecad-cadcraft/src/defaults/plc.rs` | `plc.frame.show`, `plc.page.title` |
| `ecad/crates/ecad-cadcraft/src/generated.rs` (lane U) | `GenKind::PlcRack`; ruled groups for `IoList`; no sheet break per card |
| `ecad/crates/ecad-cadcraft/src/checks.rs` | `wire-through-card`, `plc-text-wide` |
| `ecad/crates/ecad-ui/src/table/strip.rs` (**new**) | The table's group strip (generic) |
| `ecad/apps/ecad-trial/src/lists/plc.rs`, `shell/mod.rs` | Lists ▸ PLC; tab label I/O → PLC |
| Tests `tests/all/lanes/lane_051*` in ecad-cadcraft, ecad-mcp, ecad-trial; a module in ecad-ui; one `mod` line per `lanes/mod.rs` | Design §10 |
| `ecad/crates/ecad-bench` | Design §9.4 rows |
| `ecad/docs/quick-start/051-plc-visuals.md` (**new**) | By clicking |

Not touched: `ecad-model`, `ecad-store`, `ecad-file`, upstream, CI.

## Scope

1. **Lists ▸ PLC (screen m)** (design §3): the I/O tab becomes PLC; rack chips; a strip of tiles
   (rack, one per slot, a free one), each with type, address range, a square per channel (in use,
   free, reserved, has a check) and a link to its page or **Make its page** (`plcpage`, new); the
   selected card's channel table underneath, on 039's table, with Description, Address and Pins as
   inputs (Q11). No new panel: the strip is a generic part of the table (Q9).
2. **Cards on the page** (design §2): cells redrawn to the pack's pitch (Q1) as top edge and
   dividers; one derived frame per run of a card with the name block and type text (Q2); card
   bodies as the power end (Q3); break edges (Q4); text sizes from text styles, two new styles (Q6),
   bands so nothing overlaps (Q7); outputs at 180° (Q15); wires end at the point (Q5).
3. **Sheets** (design §4): the I/O sheet as a ruled table per card, packed on sheets (Q14); a new
   generated **PLC racks** sheet (`plcrack`) with the same tiles in ink (Q13).
4. **Screenshots side by side** with the mock-up and the pack pages (design §10.2). Composites that
   contain the pack stay outside the repository.

## Acceptance criteria

Design §10.1 rows A–S. In short:

1. A fence across wires 20 mm apart gives one framed card; wires end on its top edge; no wire piece
   or wire number inside the frame; no `Loose wire` from the fence; one transaction, `u` undoes.
   Closer wires are refused with the sentence. A wire that continues through the card is kept with a
   `wire-through-card` row.
2. The card body at a run's end joins the box as its power end; a run that does not hold the card's
   first or last channel has a slanted side; a run at 180° keeps texts upright with the name block
   top left.
3. At built-in styles no text on fixture P51's PLC pages overlaps another text, a frame line or a
   wire (projection test). Raising `channel_text` and `address` moves texts apart and raises
   `plc-text-wide` when they no longer fit.
4. Lists ▸ PLC shows the rack chips, tiles, squares in the four states, totals matching `iolist`,
   page links, Make its page (`plcpage`: one page, one transaction), and the channel table filtered
   to the selected card, with Description, Address and Pins editable through `plcchannel`.
5. The `iolist` sheet: ruled table per card, heading above, Slot and Card only in the heading,
   cards packed on sheets, a long card split with `(continued)`.
6. The `plcrack` sheet: per rack a heading and its tiles (9 per A3 row), squares, places; missing
   from the release with its sentence when there are no racks.
7. Update maps (design §9): one channel text edit redraws only the page on screen and patches one
   tile; a commit with no PLC data patches nothing (counters).
8. **MCP contract test (brief 011 rule):** `plcpage` and a fence with trims record one transaction
   with `Author::Agent(<client>)` and are refused with writes off; `plcracks` with writes off records
   nothing and returns the tile data.
9. Hostile inputs of design §10.1 R: refusal in words or a Checks row, never a panic.
10. Bench rows of design §9.4 within target.
11. **Screenshots side by side** (design §10.2): eight composites, ours next to the mock-up page or
    the pack page; pack composites kept in the notes folder or `plan/` only.

## FP items (overseer)

Design §11: FP1 two text kinds and their registry entries (and, with 050, two built-in styles);
FP2 page property `generated = plcrack`; FP3 built-in cell and body geometry changes (points
unchanged; 045-era DI/DQ cells 10 mm apart overlap until redrawn, pre-v1); FP4 a fence's transaction
also deletes the wire pieces it trims (existing ops); FP5 registry entries `plc.frame.show`,
`plc.page.title`. No op, no record field, nothing in `ecad-model`.

**Approved (overseer, 2026-11-13): FP1–FP5.** FP1: the two kinds become text styles on 050's model;
051 builds after 050a merges into `dev`, or merge-trains onto `feature/style-model`. FP3: changing
how existing drawings look is accepted (pre-v1).

## Tester

No (design §14, Q17): no format or op change. The reviewer checks the update maps and the trim
rule on the feature PR; the screenshots are the owner's evidence.

## Needs owner check

Design §13 questions 1–17, and the composites of design §10.2.
