# Brief 045c: The I/O list

Status: Draft (designer, 2026-11-13)
Branch: `feature/plc`, after 045b.
Designer: **yes** (`docs/design/045.md`, §8, §11.2 c-rows) · Size: M
Roadmap: M1 item **1.3(c)**. Its row model is what the PLC manager (2.8) later edits.

## Goal

One live list of every channel of every card (rack, slot, card, channel, address, pins,
description, what it is wired to, wire number, place, and whether it is used, reserved or spare),
as a generated page, a CSV in the release, an MCP read and a read-only Lists ▸ I/O column set.

## What exists today

- 045a/b's `Plc` model; 022's nets and wire numbers; 024's generated pages and 033's continuation
  numbering; 027's release CSV slots; 039's table and its column sets in `apps/ecad-trial/src/lists/`.

## §0 Files touched

| File | Change |
| --- | --- |
| `ecad/crates/ecad-cadcraft/src/plc/iolist.rs` (**new**) | Rows (design §8.1), `iolist`, the CSV table |
| `ecad/crates/ecad-cadcraft/src/plc/mod.rs` | `by_target`, `channel_of_net` indexes if not already in b; `iolist` in `specs()` |
| `ecad/crates/ecad-cadcraft/src/generated.rs` (lane U; 046 and 047 also edit it: merge after them) | `GenKind::IoList` (`iolist`, "I/O list") and its arms, drawing through `plc::iolist` |
| `ecad/crates/ecad-cadcraft/src/release.rs` | One slot `io-list` (` I/O list.csv`) |
| `ecad/apps/ecad-trial/src/lists/plc.rs` (**new**), one line in `lists/mod.rs` | Lists ▸ I/O, read-only column set |
| `ecad/crates/ecad-cadcraft/tests/all/lanes/lane_045/iolist.rs` | Contract tests |
| `ecad/apps/ecad-mcp/tests/all/lanes/lane_045.rs` (append) | `iolist` with writes off |
| `ecad/apps/ecad-trial/tests/all/lanes/lane_045.rs` (**new**), one `mod` line | Column-set test |
| `ecad/crates/ecad-bench` | `iolist` row |
| `ecad/docs/quick-start/045-plc.md` | Section 3 |

Not touched: `ecad-model`, `ecad-store`, `ecad-file`, upstream, CI.

## Scope

1. The row model: one row per channel, drawn or not, ordered by rack, slot, channel; card
   summaries; `editable` fields named with their commands (for 2.8).
2. `iolist {rack?, card?, state?}`, read-only.
3. Generated page `iolist` with a heading per card, spare rows, continuation sheets.
4. Release CSV ` I/O list.csv`.
5. Lists ▸ I/O: read-only column set, go-to on every cell.
6. Update map (design §11.1): rows patched from touched cards, nets and targets.

## Acceptance criteria

Design §13 rows N–Q, R (`iolist`), S (`iolist`). In short:

1. `iolist` on fixture P gives one row per channel of every card with states, targets, wire
   numbers and places; with writes off it works and records nothing.
2. The `iolist` page: one heading per card, `Spare` rows, typed addresses with `*`, continuation
   sheets numbered as 033; a card is not split unless longer than a sheet.
3. The release contains ` I/O list.csv` with design §8.3's columns, or names why it is missing.
4. Lists ▸ I/O shows the same rows as `iolist`; a row goes to its cell; renaming a target device
   patches only its rows (counter); a commit touching no PLC data leaves the list untouched.
5. `iolist` bench row within target.

## FP items (overseer)

FP5: the page property `generated = iolist`. Nothing else.

## Tester

No: derived and read-only. The reviewer checks the update map. A tester only if the owner asks.

## Needs owner check

Design §15 questions 14 and 15. Reference screenshots `045-iolist-page.png`, `045-lists-io.png`.
