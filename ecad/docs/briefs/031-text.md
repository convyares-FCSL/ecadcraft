# Brief 031: Text styles, show and hide, placement

Status: Draft (designer, 2026-11-12; waiting for the owner's answers in design 031 §15. Format:
FP1–FP3, FP5 and FP6 approved by the overseer, FP4 approved on condition, design 031 §14)
Branch: `feature/text-styles`, from `origin/dev` `6729c8f1`.
Designer: **yes** (`docs/design/031.md`) · Size: L (Q1 M, Q3 M, Q2 L)
Roadmap: `docs/discussions/roadmap-review-2026-11.md` (branch `docs/roadmap-2026-11`), v0.15 lane
**T: text**, items **Q1 → Q3 → Q2**. Lanes W (032), P (033) and S (034) run at the same time and
do not edit this lane's files (design §0).

## Goal

A page that reads like a good printed schematic: a sans font with bold tags, sizes chosen per kind
of text and per project, no repeated or colliding tags, and texts the designer can hide or move
without losing them as data.

The owner's words: "Biggest one is font, text size, position. Page 100 is recognisable but is
UGLY." and "Ability to hide/show text, e.g. terminals: you do not want to show TB01 ten times."

## What exists today (`origin/dev` `6729c8f1`)

- Every ECAD text uses the text style `Standard`, which CADCraft draws in its thin single-stroke
  font. No project font, no bold.
- Sizes are constants: `TAG_HEIGHT` 3.5 and `LABEL_HEIGHT` 2.0 (`devices.rs`), `LABEL_HEIGHT` 3.5
  (`locations.rs`), `LABEL_HEIGHT` 2.5 (`look.rs`), `TEXT_HEIGHT_MM` 2.5 (`references.rs`), and
  literals in `frame.rs` and `generated.rs`. Only the wire label height is a project setting
  (`wiring.label_height`, 1.8 mm).
- Tags are derived on every draw (`devices::texts_on_page`): left of a vertical symbol or above a
  horizontal one, with no collision avoidance. Every strip terminal prints the strip tag; every
  PLC channel prints its device's tag; continuations print their name.
- Derived texts cannot be moved (`TAG_DERIVED` refuses any edit). `text` with `justify` is refused,
  because the stored text item (`Primitive::Text`) has no justification.

## Scope, in order

1. **Q1 Text styles.** A project font (regular and bold) from `storytold/craft-fonts`, with
   CADCraft's stroke font as the fallback. A height, a weight and a default visibility for each
   kind of text, set per project in Project settings ▸ Text and by a new `textstyle` command.
   Defaults suit dense schematics: tags 2.5 mm bold, labels 1.6 mm, connection point labels
   1.5 mm. **One getter**, `text_style::text_style(doc, kind)`, that lanes W and P read for sizes
   and style names (design §3.3).
2. **Q3 Show and hide per text.** Tag, connection point labels, description, part number and
   continuation name switch on or off per symbol, per device, per page and per project, with a
   new `showtext` command, a Show/Hide Text tool and Properties panel rows. Defaults: a strip's
   tag once per run of adjacent terminals, a PLC device's tag once per run of its channels,
   continuation names hidden. Hidden text is still data: search, reports and lists are unchanged.
   A display-only **Show Hidden Text** view draws hidden texts dimmed and never prints them.
3. **Q2 Placement.** Tags (with any shown description and part number stacked under them) are
   placed by the symbol's orientation and avoid wires, symbols and other texts. A derived text
   can be moved with a Move Text tool and a new `movetext` command; the move is stored and
   survives regeneration, renumbering, save and reopen. `text` with `justify` is recorded, so
   justified and centred text works.

## Later (not designed now)

- Wire labels avoiding tags and the one-label-per-net rule: lane W's Q4 (it reads this lane's
  getter and visibility function).
- Title block, frame, table and generated-page sizes: lane P reads the getter; its defaults equal
  today's sizes, so nothing changes there until lane P chooses.
- Definition text slots (proposal §4.2), per-text rotation, grip-dragging derived texts, a Checks
  row for texts that still overlap.

## Format and ops

Design §14. All additions are optional properties, left out when unused, except **FP4**: an
optional justification on a stored text item (needed for `justify`). No new op.

## Acceptance criteria

Each step lands with its tests; the full list is design §13.

1. **Q1.** A project with no `text.*` keys draws tags 2.5 mm bold, connection point labels 1.5
   mm and location labels 1.6 mm in the project font when craft-fonts is built in, and in the
   stroke font when it is not; the getter returns the design §3.2 defaults for every kind and the
   default for any hostile stored value (NaN, text, out of range). `textstyle` changes a height,
   a weight and the font in one transaction each, and the open page redraws.
2. **Q3.** On a strip of ten adjacent terminals the tag is drawn once; hiding a device's tag on
   one symbol, on the device, on a page and on the project works and the narrower scope wins;
   `devices`, the BOM and search still find the device; Show Hidden Text draws hidden texts in
   `ecad.hidden-text` and a printed PDF has none of them.
3. **Q2.** A tag whose preferred place is on a wire moves to the next free candidate; two
   neighbouring tags never overlap when a free candidate exists; a moved tag keeps its place after
   moving or rotating its symbol, after renumbering, and after save and reopen; `text` with
   `justify: "MC"` is recorded and reopens centred; an old file opens and saves byte-identical.
4. **MCP contract tests** through `ecad-mcp`'s `execute`, for every added or changed command:
   `textstyle`, `showtext`, `movetext` and `text` with `justify` each record one transaction with
   `Author::Agent(<client>)` and are refused with writes off; `texts` and `textstyle {}` read with
   writes off and record nothing; `projectprops`, `pageprops` and `setprop` refuse `text.` keys
   with the sentence of design §10.
5. **Update maps** (design §11) hold: a `text.*` change on a device redraws only the pages of its
   symbols; the verify-mode comparison of the page projection against a rebuild passes.
6. **Bench** (design §12): page draw on the medium project's densest page is no more than 15 %
   slower than `dev`; the large tier is run once for `textplace` and `textstyle`.

## Needs owner check

Design §15 Q1–Q9, in particular the font (Q1), whether ECAD builds embed craft-fonts (Q2) and the
default sizes (Q3).
