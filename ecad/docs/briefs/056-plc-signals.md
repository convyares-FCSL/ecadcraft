# Brief 056: The PLC signal list and PLC sync

Status: Approved (overseer: FP1–FP5, 2026-11-14; owner: design §16 answered, 2026-11-14)
Branch: `feature/plc-signals`, from `origin/dev` `8b6c2ccd`.
Designer: **yes** (`docs/design/056.md`) · Size: L
Release: 0.2.2 (UI rev 4). Source: rev 4 build spec "Project workspace ▸ PLC: the signal list"
and board 2.03. Lanes D (`plc/`), U (`apps/ecad-trial/src/lists/`).

## Goal

The PLC tab becomes the project's signal list: every channel has a PLC tag, a safe state and what
it drives, and the list stays in step with the PLC programming project through a tag table that
ECADCraft exports and imports back. The rack overview is laid out as board 2.03 and is always in
the book.

## Why

Rev 4 makes the PLC list the backbone of the project: the Tests tab (0.2.3) is generated from it,
and the PLC programmer works from the same names. Today the tab has no tag, no safe state, no link
to the PLC project, and Import is a greyed placeholder.

## What exists today (`origin/dev` `8b6c2ccd`)

- 045: racks, slots, cards, channels, live addresses, `plcchannel`, `iolist` rows (targets are
  every tagged device on the channel's nets).
- 051: tiles as the table's group strip, `plcpage` ("Make its page"), the channel table with
  Description, Address and Pins as inputs, the `plcrack` and `iolist` sheets.
- Rev 3 Project workspace: tab row with greyed **Import an I/O list**, Export copies CSV, the right
  panel for a tile.
- No XLSX support in the workspace (none is added: CSV only, Q3).

## §0 Files touched

Design §0.3 has the full table. In short:

| File | Change |
| --- | --- |
| `ecad/crates/ecad-cadcraft/src/plc/signals.rs` (**new**) | Tag, safe state, Drives, `TestState` |
| `ecad/crates/ecad-cadcraft/src/plc/sync.rs` (**new**) | Snapshot, matching, states; `plcsync`, `plctake`, `plcimport` |
| `ecad/crates/ecad-cadcraft/src/plc/tagtable.rs` (**new**) | The format layer (one interface, one registry) and its one format, generic CSV; `plcexport` |
| `ecad/crates/ecad-cadcraft/src/plc/{mod,channels,iolist,overview,sheets,address}.rs` | `plcchannel {tag, safe}`, `plcmaketags`, row fields, tile words, sheet tiles, `percent` style |
| `ecad/crates/ecad-cadcraft/src/defaults/plc.rs`, `checks.rs`, `release.rs`, `lanes.rs` | Registry entries, one Checks block, CSV columns, one lane line |
| `ecad/apps/ecad-trial/src/lists/{plc,workspace,detail}.rs`, `lists/plc_compare.rs` (**new**) | Rack heading, card header, chip, columns, Compare, Import and Export, right panel |
| Tests `tests/all/lanes/lane_056*` in ecad-cadcraft, ecad-mcp, ecad-trial | Design §13 |
| `ecad/crates/ecad-bench` | Design §12.4 rows |
| `ecad/docs/quick-start/056-plc-signals.md` (**new**) | By clicking |
| `ecad/tests/fixtures/plc/` (Q15) | Synthetic generic CSV tag tables |

Not touched: `ecad-model`, `ecad-store`, `ecad-file`, upstream, CI.
Overlaps: `detail.rs` and `workspace.rs` with the other rev 4 Project briefs (each adds its own
arm); the `plcrack` page's place in the book with the page-types brief (054).

## Scope

1. **Rack overview** (design §3): rack heading line, legend, **Add a rack**; tiles widened to the
   board with slot and kind words, tag, description, squares, range, `n of m in use`, page link or
   **Make its page**; the free tile `Slot n is free / Add a card from the catalogue`.
2. **The selected card's channels** (design §4): a card header with the sync chip, **Compare**,
   **Export tags**, **→ Page**; columns Ch, Address, Tag, Description, Type, Safe, Drives, Page,
   PLC, Test; free channels folded.
3. **New channel properties** (design §4.3, §4.4): PLC tag (unique per PLC) and safe state
   (outputs: Off/On/Hold; analogue: Hold/min/max/value), with registry defaults; **Make tags**
   from descriptions.
4. **Drives** read from the drawing through terminals and plugs to the end device (design §4.5).
5. **PLC sync** (design §6): an imported snapshot kept in the project; per-channel states (in sync,
   differs, moved, not in PLC, only in PLC) and tags on no card; the chip; Compare side by side
   with **Use PLC's**; Export tags; Import with a preview and one transaction.
6. **Formats** (design §7): **generic CSV only** (owner, Q3): no dependency, works on wasm, any PLC
   tool's table can be mapped to it in a spreadsheet. Formats sit behind one interface and a
   registry, so the vendor formats of 0.5.0 (TIA Portal, TwinCAT and the like, with manufacturer
   data) are each one module, with no change to sync, Compare, the commands or the UI.
7. **Test column** designed and shown with `—`; the Tests brief (0.2.3) fills it (design §4.6).
8. **The rack overview in the book** (design §8): `plcrack` tiles in the new words, always present
   with a rack, tiles link to their pages; the PLC tab's footer links to it.
9. **Right panel** for a card and a channel (design §5).
10. **Update maps** throughout (design §12); Checks (design §9); commands and MCP (design §10).

Out of scope: tests themselves, a live link to a PLC tool, building cards from a PLC hardware
configuration, vendor tag table formats (0.5.0), provisional addresses, several PLC snapshots
(design §15).

## Acceptance criteria

Design §13.1 rows A–P. In short:

1. Project ▸ PLC on fixture P56 matches board 2.03's layout (rack heading, tiles, card header,
   columns, folded free channels, footer link); screenshots side by side (design §13.2).
2. Tag and Safe edit through `plcchannel` in one transaction each; a duplicate tag is refused with
   its sentence; an unset safe state shows the default with its badge and follows `setdefault`.
3. Drives shows the end device through a terminal, with every target in the tooltip.
4. Import of the generic CSV (comma, and `;` with a BOM as a spreadsheet saves it) gives the
   states of design §13.1 G; any other file is refused with words; the preview writes nothing;
   Import writes one `Import PLC tags` transaction that `u` undoes whole.
5. The chip counts the card's differences; Compare lists them side by side; **Use PLC's** and **for
   all** take names in one transaction each.
6. Export → reimport round-trips (same snapshot); untagged channels are left out
   and counted.
7. A card move updates addresses and sync states, nothing else (counter).
8. The `plcrack` page is in the book when a rack exists, with the new tile lines, and goes with the
   last rack.
9. Update maps (design §12): a tag edit patches one row and one chip; a commit with no PLC data
   patches nothing; an import rebuilds the sync index once.
10. **MCP contract tests (brief 011 rule):** `plcchannel {tag, safe}`, `plcmaketags`, `plcimport`,
    `plctake` record one transaction with `Author::Agent(<client>)` and are refused with writes
    off; `plcimport {dry_run}`, `plcexport`, `plcsync`, `iolist` work with writes off and record
    nothing.
11. Hostile files and values of design §13.1 O: refusal in words, skipped and counted, or a Checks
    row; never a panic.
12. Bench rows of design §12.4 within target.

## FP items (overseer)

Design §14:

- **FP1.** Channel keys `plc.ch.<i>.tag` and `plc.ch.<i>.safe` on cards (045 FP3's pattern).
- **FP2.** Project properties `plc.sync.table` (the snapshot as canonical CSV text, ≤ 20 000
  rows), `plc.sync.source`, `plc.sync.at`: the first table-sized project property; merge treats it
  as one value.
- **FP3.** Import is ops: `plcimport`, `plctake` and `plcmaketags` record `SetProperty` ops in one
  transaction each; undo restores the previous snapshot and names.
- **FP4.** Registry entries `plc.safe.dq`, `plc.safe.ao`, and the address style
  value `percent`.
- **FP5.** No dependencies (Q3: CSV only). The XLSX crates of the draft are dropped; what
  remains is the format layer, which is code.

No new op, no record field, nothing in `ecad-model`.

**Approved (overseer, 2026-11-14): FP1–FP5.** The FP5 conditions for XLSX crates (permissive
licences already on the house-rules allowlist, one crate for reading and writing preferred, wasm
green) lapse with Q3. They apply again in 0.5.0. The allowlist is not edited in this repository.

## Tester

**Yes** (design §17): import writes project data from hostile files through a new reader, adds a
table-sized property, and must undo whole. The tester runs design §13.1 with hostile files (O) and
the round trip (I) first, and a merge of two branches that both imported.

## Owner answers

**Owner (Carl, 2026-11-14, through the overseer):** design §16 answered.

- **Q1:** file only. No live link to the PLC tool or to any external Excel file.
- **Q3:** generic CSV only, for now. TIA Portal, TwinCAT and the like come in 0.5.0 with
  manufacturer data. No XLSX crates in 056. The format layer stays ready for more formats.
- **All other questions:** as recommended. Q5: add the `percent` style, `byte.bit` stays built in.
  Q6: `DQ` everywhere. Q12: provisional addresses get a separate brief. Q15: the fixtures folder,
  with synthetic CSV tag tables only. Q16: a tester.
