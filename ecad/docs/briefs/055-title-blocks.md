# Brief 055: Library ▸ Title blocks, a simple title block and frame editor

Status: Draft
Branch: `feature/title-blocks`, from `origin/dev` `8b6c2ccd` (050a merged).
Designer: **yes** (`docs/design/055.md`) · Size: L, in three parts (below)
Release: 0.2.2 (UI rev 4). Spec: UI rev 4 build spec, "Library workspace ▸ Title blocks"
(`docs/discussions/ui-2026-11/rev4-build-spec.md`), board 3.05, build order step 10.

## Goal

A project's sheets look like the company's drawings: a border with zones, and a title block with
the company's fields and logo. The title block is made once in Library ▸ Title blocks and every page
fills itself in. A generic built-in border and title block, drawn to the house style, is what every
project gets until it makes its own, and it becomes the public examples' border.

## Why

The drawing-pack re-run found the sheet look, title block included, to be the biggest remaining
gap: it affects all 43 pages. The review of the owner's best drawings set what a good title block
holds: a revision grid with an ECR column, a red status field, the function and location, sheet,
next and total counters, the drawing title and the drawing number. Today's block (019) has some of
these, in one fixed layout. 019 left a `FrameLayout` seam for user layouts, and 050a gave frames a
catalogue. Nothing lets a user change the block.

## What exists today (`origin/dev` `8b6c2ccd`)

- 019/033: `frame.rs` draws the frame and a fixed title block as derived parts, with an update map
  (`touches`). Revision columns per project, with custom fields such as `ecr`, and the pending
  marker.
- 050a: the `frame` catalogue (`print.frame.style`, levels Standard, Project, Page), with weights,
  colour, grid labels and a one-word `title_block` choice. Standards ▸ Frames lists it.
- 019: the sheet (`SheetSpec`, landscape, at least 420 × 250 mm) per page or project. Margins and the
  40 mm title block strip are constants.
- 048: the symbol builder's draft, undo and Save pattern. 038: the Library tab `Title blocks` is a
  placeholder.
- CADCraft's PDF writer does not write raster images, so a PNG logo would not print (design §0.1).

## §0 Files touched

Design §0.3 has the full table. In short:

| File | Change |
| --- | --- |
| `ecad-model/src/style.rs`, `merge.rs`, `diff.rs`, `invariant.rs` (hunks) | The style record's optional body (FP1) and its bounds |
| `ecad-file` `v1.rs` and v2's record encoding (hunks) | The body in the file |
| `ecad-cadcraft/src/title_block/` (**new**), `sheets.rs` (**new**), `title_block_cmds.rs` (**new**) | Layout to parts, bindings, the built-in house block, DXF import, sheet resolution, commands |
| `ecad-cadcraft/src/frame.rs`, `styles/frame.rs`, `checks.rs`, the revision-column default (hunks) | Draw the body, four frame fields, three checks, `ecr` column (FP2–FP5) |
| Ten call sites of `Grid::of`, `sheet_of`, `TITLE_BLOCK_MM` (one line each) | Through `sheets::*` (FP4) |
| `apps/ecad-trial/src/library/title_blocks/` (**new**), `library/{mod,browser}.rs`, `settings/standards.rs` (hunks) | The editor; the tab; an Edit in Library link |
| `lane_055*` test modules, `ecad-bench` rows, `docs/quick-start/055-title-blocks.md` (**new**) | Design §10.3, §12 |

Not touched: CADCraft (no upstream seam), `ecad-store`, CI.

## Scope

1. **The built-in house block** (design §3): the built-in `sheet` frame is redrawn to the house
   style, 40 mm high as today, so nothing on a page moves. Thin frame, zones on all four sides, and
   centre marks.
2. **The model** (design §1, §2): a frame style carries its title block as a typed body (lines,
   texts, fields, a revision table, a logo). An own frame is drawn for one sheet size and sets its
   pages' sheet, margins and zones. No second model: Standards ▸ Frames, `print.frame.style`,
   delete-with-replacement and promote are 050a's.
3. **Bindings** (design §7): a field shows a project property (including any key the user names), a
   page value, a field of the newest issued revision, or the sheet size. If empty: blank, dash, the
   missing mark or a short text.
4. **The editor** (design §4, §5): left, the project's blocks and the field palette; toolbar
   Select, Line, Text, Field, Logo, Sample, Snap (1 mm), Frame… (sheet, margins, zones). Fields are
   drawn as dashed boxes. The right panel holds Value, Look (label, size, bold, align, colour) and
   Box. Draft and Save, with the consequence line first.
5. **Logos and Import DXF** (design §4.6, §6): vector logos from DXF, fitted to their box; a whole
   block imported from DXF with texts turned into fields one by one.
6. **Commands** `titleblocks`, `titleblock`, `titleblockimport`; checks `title-value-cut`,
   `title-binding-unknown`, `frame-sheet-differs`; update maps with counters; bench rows.

## Out of scope

Scripting (spec). Raster logos (Q3). Portrait and A4 sheets, and choosing a frame per page type
(054). A print-date field. DRAFT overlays and redline layers. Several blocks per frame.

## Acceptance criteria

Design §12.1 rows A–N. In short:

1. The sample project shows the house block on every page, and no device, reference, wire or
   generated table moves.
2. A frame made from the built-in round-trips through save and open. A field moved and saved moves
   on every page that uses it, and `u` undoes it.
3. A field bound to a user's project key redraws only when that key changes. A property no field
   reads rebuilds no frame (counter).
4. An own A2 frame gives its pages A2 paper and zones. A page whose own sheet differs gets a Checks
   row with a fix.
5. DXF logos and Import DXF work on synthetic files, and too-large or unreadable files are refused
   in words.
6. The editor works as design §4 says (kittest). Built-ins are read-only. Writes off disables Save.
7. Hostile bodies open, draw, report and never panic, with a regression test per bound.
8. Merge treats a frame's record as one unit.
9. **MCP contract tests** for every writing form: one transaction, `Author::Agent(<client>)`,
   refused with writes off, and undo.
10. Bench rows within target. Screenshots side by side with board 3.05 and with `dev`.

## FP items (overseer)

Design §14: **FP1** `StyleRecord.body` (typed title block layout; additive; no new op); **FP2** four
`frame` fields (`sheet`, `margins`, `centre_marks`, `block`); **FP3** the built-in `sheet` frame
redrawn to the house block and `title_block` retired (every drawing changes look, nothing moves;
pre-v1); **FP4** a page's sheet, margins and keep-out strip resolved through its frame, at ten call
sites; **FP5** `ecr` in the built-in revision columns. No raster assets, no new op, no new table.

**Approved (overseer, 2026-11-14): FP1–FP5.** FP3: redrawing the built-in frame to the house block
is accepted (pre-v1). Owner answers to design §15 are still to come.

## Parts

- **055a**: model, house block, sheet resolution, commands, checks and bench. It closes the
  drawing-pack gap by itself, so it goes first. Size M.
- **055b**: the editor. Size M–L.
- **055c**: logos and Import DXF. Size S–M.

## Tester

**Yes.** FP1 changes the `.ecad` record through the file layer, merge and undo, and FP4 changes
where every page gets its sheet. The tester runs design §12.1 A–M (J in full) and the fast tests of
the crates touched.

## Coordination

- **054 (page types, in design)** picks frames per page type and owns portrait and A4. `sheets::sheet_of`
  is the one place that answers "which sheet" (design §2, §11).
- Settings ▸ Project info lists the project keys bound blocks read (`titleblocks` returns `keys`).

## Needs owner check

Design §15 questions 1–10. The house block on the sample project beside `dev`, and the editor beside
board 3.05.
