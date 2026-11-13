# Brief 050a: the style model (standards and style catalogues)

Status: Approved (owner, 2026-11-13: design 050 §14 Q1–Q15 answered as recommended, including
roadmap Q25–29; overseer FP1–FP7 rulings of design §13: FP1–FP3 with full arms, round-trip and
merge tests; FP4 with one `ignored-old-setting` Checks row per kind of old key, no conversion; FP5's
`location` area one line in the union-merged `AREAS` list).
Branch: `feature/style-model`, from `origin/dev` after 049 (1.9 continuation references) has
merged, because lane D is sequential.
Designer: **yes** (`docs/design/050.md`) · Size: L
Roadmap: `docs/discussions/roadmap-review-2026-11.md` M1 item **1.13(a)**, lane D, fourth (after
1.2, 1.3 and 1.9). 050b (wire categories, 1.4) follows on the same branch.

## Goal

A project uses a named, shareable house style: a **standard** that holds values for every registry
entry at the Standard level, plus **named style catalogues** (text styles, location and box styles,
title block and frame styles). Objects refer to a style by stable ID, and changing a style updates
every user live, through an update map and never through a full rebuild. Everything shows and
changes on the existing Settings ▸ Standards screen and in the inspector, with no new panel.

## What exists today (`origin/dev` after #81)

- **Registry** (031, 042): `defaults::{Entry, Kind, Value, Level, Source, Scope, resolve, chain,
  users, all, AREAS}`. `Level::Standard` is reserved, with no source. Entries are in
  `defaults/{text, wires, print, symbols, terminals, connectors, strip_pages, bom}.rs`.
- **Text styles** (031, 040): `text_style::text_style(doc, kind)` reads `text.<kind>.height`,
  `.weight` and `.font` project properties. These are the "named styles" the roadmap said 1.13
  would adopt.
- **Standards screen** (042): `apps/ecad-trial/src/settings/standards.rs`, with a Defaults table
  and a Text styles table (one row per `TextKind`), the chain, `setdefault`, and the consequence
  line. The standard is shown as "IEC (built in)", read-only (042 Q1).
- **Location boxes** (024): `locations.rs`, `DASHED` at `look::WIRE_WEIGHT`. **Frame** (019):
  `frame.rs`, `FRAME_WEIGHT` 0.5 and `THIN_WEIGHT` 0.25.
- **Ops**: `SetProperty` on `ObjectRef::{Component, Conductor, Cable, Page, Project}`, and
  `SetProjectSettings`.

## §0 Files touched

Design 050 §0.1 is the full table. In short:

- **Owned:** `ecad-model/src/style.rs`, `ecad-cadcraft/src/styles/{mod, text, location, frame}.rs`,
  `style_cmds.rs`, `defaults/styles.rs`, `apps/ecad-trial/src/lists/style_catalogues.rs`,
  `lane_050.rs` test modules, and `docs/quick-start/050a-style-model.md`.
- **Shared, by hunk:**
  - `ecad-model` `doc.rs` (two fields), `op.rs` (two ops, one `ObjectRef`), `id.rs`, and the arms
    in `diff.rs`, `merge.rs` and `invariant.rs`;
  - `ecad-file` `record.rs` and `v1.rs` arms;
  - `defaults/mod.rs` (Standard source, `Kind::Style`, one line each in `AREAS` and `all()`);
  - `defaults/text.rs` (height, weight and font entries leave);
  - `text_style.rs`, `text_cmds.rs`, `locations.rs` and `frame.rs` (their call sites only);
  - `default_cmds.rs` (`level: standard`), `project.rs` (`pages_touched` arms) and `lanes.rs`
    (one line);
  - the trial's `settings/standards.rs` (header, `CATALOGUES`), `lists/standards.rs`
    (`StylesSource`), `lists/mod.rs` (one line), `shell/status_bar.rs` and `shell/right.rs`
    (style rows).
- **Lane U** owns the trial files above. Lane D edits only the hunks named, and the overseer checks
  for overlap with any lane U brief running at the same time.

## Scope, in order

1. **Model and format** (FP1–FP3): `Standard`, `StyleRecord`, `StyleRef`, `Document.standard` and
   `.styles`, `SetStyle`, `SetStandard` and `ObjectRef::Standard`, with apply, inverse, diff,
   merge (whole-record units), invariants, the file arms and the bounds (design §3).
2. **Resolution:** the `Level::Standard` source in `resolve` and `chain`, `Kind::Style`, and
   `styles::field` with the five steps of design §2.4. The users index is kept from the touched
   set (§2.5).
3. **Catalogues:** text (20 built-in styles carrying today's sizes, plus the new `colour` field),
   location (`box`) and frame (`sheet`), with their choice entries (design §4). `text_style`,
   `locations.rs` and `frame.rs` read them. **Nothing on a page moves** when this lands.
4. **Commands:** `standard`, `style`, `deletestyle`, `promote` and `styles`; `setdefault` at
   `standard`; `textstyle` writing records (design §6). MCP contract tests for each.
5. **Standards screen and inspector:** the standard header, the catalogue tables and the shared
   style detail, with the consequence line first, and inspector style rows with Reset (design §7).
   No new panel, screen or dialog.
6. **Checks:** `style-unknown`; `ignored-old-setting` for 031's old text properties (overseer ruling on FP4, design §3.4); bound breaches from hostile files.
7. **Bench** rows of design §11 (medium while building; large once at the end).

## Out of scope

Wire categories (050b). Cable types (2.7) and part sets (2.6), which are catalogue words only.
`.ecadlib`, origins and "update available" (2.1). A US base standard. Exporting a standard as its
own file. New panels or dialogs. Converting old `text.<kind>.*` properties (FP4: pre-v1, no
compatibility work).

## Format and ops

FP1 `Document.standard`, FP2 `Document.styles`, FP3 `SetStyle` / `SetStandard` /
`ObjectRef::Standard`, FP4 031's text height, weight and font properties no longer read, and FP5
style choices as properties with the new area `location` (design 050 §13). All are additive in the
file, and none needs migration (pre-v1). **Approved by the overseer (2026-11-13), with the old-key check (design §13).**

## Update map

Design 050 §10. In short:

- `SetStyle` marks stale only the pages whose resolved choice is that style, and patches one
  catalogue row.
- A style-choice `SetProperty` touches its object's page, or all pages for a project or standard
  choice of `text` or `frame`.
- `ObjectRef::Standard` acts like a project-level write of that key, and does nothing on the page
  when the project overrides it.
- `SetStandard` changes only the header.
- Every other op does nothing.
- Counters prove there are no rebuilds.

## Acceptance criteria

1. Design 050 §12 checks 1–10 pass.
2. On the sample project, after the brief, every page renders the same as on `dev` (screenshot
   compare). This shows the built-in styles carry today's values exactly.
3. Registry tests: every choice entry's built-in names a built-in style of its catalogue; every
   built-in style's values parse as its fields' kinds; and a hostile stored value at the Standard
   level resolves to the built-in (031's table, one row more).
4. `text_style(doc, kind)` keeps its signature, and returns each built-in style's values on an
   empty project (031's drift test, kept).
5. Hostile file (design §12 check 8): opens, draws, reports, and does not panic. A regression test
   for each bound.
6. MCP contract tests (design §6.8) for every new or changed writing command: the transaction,
   `Author::Agent(<client>)`, refusal with writes off, and undo.
7. Element names of design §9 are pressed in kittest tests. Standards tables have no per-frame
   O(styles) copies (042's lesson 7).
8. `docs/quick-start/050a-style-model.md`: make a standard, change the tag style's colour, add and
   delete a style with a replacement, copy a standard from another project, and the FP4 note
   ("text sizes set before 050 go back to the built-ins; set them again in Standards ▸ Text
   styles").

## Tester

**Yes.** It is a `.ecad` format and op change (FP1–FP3): new document fields, new ops and a new
`ObjectRef` through the store, replay, undo and redo, merge, and the file layer. The tester runs
design §12 checks 1–10, with check 8 in full, plus the fast tests of `ecad-model`, `ecad-file`,
`ecad-cadcraft` and `ecad-trial`.

## Owner questions

Design 050 §14: questions 1, 6, 7, 8, 9, 10 and 11–15 (roadmap 25–29) bear on this brief.

## Needs owner check

Make a standard, change the tag colour in Standards ▸ Text styles, and watch the page change, with
the consequence line read before Apply. Then print one page mono and one in colour.
