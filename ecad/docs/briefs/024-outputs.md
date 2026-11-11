# Brief 024: Outputs: locations, the PDF book, generated pages and reports

Status: Approved (owner, 2026-11-11: design §14 Q1–Q11 accepted as recommended; FP1–FP4 and the Q7 PDF seam approved by the overseer)
Branch: `feature/outputs` (worktree `C:\dev\ecad-features\sheets`, historical name), from `dev`
`af2d41eb`.
Designer: **yes** (`docs/design/024.md`) · Size: L
Roadmap: `docs/roadmap-v0.1.md`, the rest of F6 (019 did the frame). Lane A; 027 (release) follows
it. 025 (cross-references), 026 (numbering) and 028 (terminal strips) run beside it.

## Goal

The project leaves ECAD as a printed book and its lists: every page in a PDF with its frame and
title block, generated cover, contents, revision list and BOM-by-location pages, and three separate
reports (BOM, wires, cables) that are shown in the app and exported to CSV. Devices know where they
physically are (`+` location), so the BOM can be grouped by it and the reports show it.

## What exists today

- Component property `location` (018, stored with a leading `+`), editable through `setprop`
  (several devices at once with `components`) and the Properties panel; not drawn, not inherited.
- Page and project properties `location` and `function` (019 FP1), shown in the title block.
- The 019 frame and title block, drawn as derived parts on `ECAD-FRAME`; book order (`Book`), page
  numbers, folders, revisions `rev.<n>.*`.
- Nets, wire numbers, `potential`, `cross_section`, `colour` on conductors (022); the Checks tab.
- Cables with `tag`, `designation`, `type` properties and core members (004, 009).
- CADCraft's `plot` with `pages: [{window, paper, landscape}]`, as used by `pdfbook`
  (`crates/engine/src/cmd/exchange.rs`, `crates/io/src/pdf.rs`, at most 256 sheets per file).

## Scope (owner decisions, 2026-11-11)

1. **Device location is a device property.** Set from the Properties panel (one or several
   devices) and through MCP, with `setprop`. Precedence: the device's own value, then the location
   box its main symbol is in, then the page's `+`, then the project's `+`; the panel says where an
   inherited value comes from (`from box on 110`). The design says how a device resolves when its
   symbols sit in different locations (main-symbol rule).
   **Location boxes** (owner correction, 2026-11-11): a dashed rectangle **or any closed
   polyline**, labelled `+…`, drawn with the Location Box tool (rectangle or polyline mode) or made
   from an existing closed polyline; grips edit the outline; the `+` is set when drawn and edited
   in Properties. Every device symbol whose insertion point is inside takes the box's location;
   nested boxes: the innermost wins. Containment is **live and derived** (writes nothing on
   devices): moving a symbol in or out, or reshaping, moving or deleting the box, updates at once,
   and only the devices in the old and new outlines are recomputed. A device whose own value
   differs keeps it, with a Checks warning, go-to and a one-click **Use the box's location**.
2. **PDF book** of all pages, a folder or a list of pages, in book order, with frame and title
   block, through CADCraft's plot path (no second PDF writer).
3. **Generated pages**, derived and never stored as lines, each with an update map: cover,
   contents (page number, title, folder), revision list, and BOM by location.
4. **Three reports**, each in the app and exported to CSV:
   - **BOM:** devices and parts only; wires never appear.
   - **Wire report:** one row per conductor: wire number, from and to (device:connection point and
     location), cross-section, colour.
   - **Cable report:** tag, type, cores, from and to location, with room for used and spare cores
     (F5, brief 028).
5. **Never a dead end:** a missing value shows `?` and is flagged; nothing is refused for it.
6. **MCP:** read the reports and set locations through the same commands.
7. **Quick start:** a click-only "Outputs" section on the demo.

Out of scope: the release folder, pre-release check, terminal and cable-type CSVs (027); terminal
strips and core usage (028); wire types; custom title-block templates; XLSX.

## Format and ops

**No new op and no format change.** The overseer approved design FP1–FP4 on 2026-11-11: the
page property `generated`; a location box as a tagless drawn component (a closed polyline item),
as 022's junctions are; device `location` stays a component property, inheritance derived; no
report settings in the file.

## Acceptance criteria

1. A device's location can be set for one or several devices (one transaction, one `u`), cleared
   back to inherited, and its source (`box`, `page`, `project`) is shown. Location boxes, drawn as
   rectangles or closed polylines or made from an existing closed polyline, draw dashed with their
   `+` label and give their location, live, to the device symbols inside (innermost wins); moving
   a symbol or editing the box updates it at once and `u` undoes it. A differing own value is kept
   and flagged with a one-click fix.
2. A device whose symbols lie in different locations resolves by design §3.3, and a symbol drawn
   away from its device's location shows the `+` prefix on its tag.
3. **Print Book** writes one PDF of all pages, a folder or a page list, in book order, each sheet at
   its paper size with frame and title block; the generated pages print like the others.
4. Cover, contents, revision list and BOM-by-location pages are added from the Project panel,
   follow edits live, overflow onto continuation sheets, and refuse drawing with a way on.
5. The Reports window shows BOM, wires and cables; each exports to CSV with the documented columns.
   No wire is on the BOM. `?` cells are flagged and counted, never refused.
6. MCP: `bom`, `wirelist`, `cablelist` and `printbook` work; `setprop` location and `locationbox`
   record `Author::Agent(<client>)` and are refused with writes off (contract tests).
7. Old files open unchanged; save and reopen keep locations, boxes and generated pages.
8. The large bench shows no whole-project work per ordinary edit for any view in design §9.
9. Reference screenshots and a sample PDF, approved by the owner.

## Needs owner check

- Approve the design (`docs/design/024.md`) and its questions (§14).
- A click-only trial on the demo (quick start "5K. Outputs").
