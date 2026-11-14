# Brief 054: Page types, Settings ▸ Pages and sheets, and generated front pages

Status: Draft
Branch: `feature/page-types`, from `origin/dev` `8b6c2ccd`
Designer: **yes** (`docs/design/054.md`) · Size: L (split in two, design §15)
Roadmap: UI rev 4, release **0.2.2** (with 055 title block editor, the PLC signal list and sync,
and wire and cable ratings). Rev 4 build spec: `docs/discussions/ui-2026-11/rev4-build-spec.md`,
"Draw: generated front pages", "Settings" (Pages and sheets) and "Panels" (Book); boards 1.16
and 5.02. Build order item 10.

## Goal

Every page has a **type**: Front sheet, Schematic, Network, Single-line, Panel layout or Report.
The type brings the page's sheet, frame, title block, zones, numbering and its place in the book
(front, body or back), and **+ Page** asks for a type and puts the new page where it belongs.
The front of the book is generated: a cover with typed notes, a wire-colour legend, a symbol
legend of only the symbols used, the revisions and the title block; then the revision history and
the contents. Everything on them updates itself except the notes.

## Why

The drawing-pack test's biggest remaining gap is the sheet look: the pack opens with a cover that
explains the drawing (notes, wire colours, symbols, revisions), and each kind of page is numbered
in its own range. Today every page is numbered in one run, every page takes the project's sheet,
the cover is six centred lines, and the Project panel's **+** offers page kinds with no idea of
where they go.

## What exists today (`origin/dev` `8b6c2ccd`)

- Pages with `number`, `folder`, `sheet` (page override of `settings.sheet`), `function`,
  `location` (019); folders give book order (`ecad_model::book`).
- Generated pages by page property `generated` (024 FP1): `cover`, `contents`, `revisions`,
  `bom-location`, `terminals` (with style, 046), `cables`, `plcrack`, `iolist`; continuation
  sheets numbered in sequence (033); `newpage {generated}`; the kind is fixed once made.
- The style model and its catalogues `text`, `location`, `frame`, `wire` (050a/050b), with the
  registry's choice entries, `setstyle`, `deletestyle`, `setdefault`, `styles`, and the users index.
  Wire categories carry colour, cross-section and line look.
- The frame style's `grid_labels` and `title_block` (`standard`, the only layout).
- Settings ▸ Pages and sheets exists (rev 3): today's sheet, numbering and text-style settings.
- Issued revisions and per-project revision columns (019, 033).

## Scope

**054a: page types and Settings ▸ Pages and sheets**

1. A new style catalogue **`page`**: six built-in types, each with sheet, frame style, title
   block, zones, first and last number, position (front, body, back) and group (design §2).
   Projects may override a built-in or add their own, through 050a's commands.
2. Every page resolves a type: its own choice, else from its generated kind, else Schematic. Old
   projects need no migration and keep their numbers and folders (design §2.4).
3. The page's sheet, frame, title block and zones follow its type; a page's own `sheet` still
   wins (design §3).
4. **Settings ▸ Pages and sheets**: the table of board 5.02 and its right panel (design §5).
5. **+ Page** (page well, Book header, Book group row) asks for a type (and, for Front sheet and
   Report, the generated kind), then places the page in the type's group and numbers it in the
   type's range (design §4). `newpage {type}` does the same over JSON and MCP.

**054b: generated front pages**

6. The **cover** (Front sheet, 01) draws blocks: heading, customer and site, notes, wire colours,
   symbol legend, revisions, structure codes, and the title block (design §6).
7. **Revision history** (02) and **Contents** (03) are 024's revision list and contents pages,
   placed as front pages (design §7).
8. The right panel's **Page** tab on a generated page: the block toggles, the front pages with
   their toggles, and the sheet read from the type (design §8). Turning a front page off deletes
   it with a new `deletepage`, for generated pages only (design §8.3).
9. **Update maps** for every new or changed view; no rescan on an ordinary commit (design §10).

Out of scope: the title block editor and new title block layouts (055); the panel layout page,
its strip and scale (later, 0.2.3); generated network and single-line drawings (later); a
drawing-number pattern per page; renumbering existing pages by type (design §14 Q11); references
that omit the zone on a zoneless page.

## Format and ops

No new op. Design §11, FP1–FP6, for the overseer:

- FP1 a style catalogue word `page` (records through `SetStyle`);
- FP2 page property `page.type` (a registry choice at page level);
- FP3 `ecad_model::sheet::sheet_of` reads the page type's stored sheet;
- FP4 project property `cover.notes`;
- FP5 registry entries `front.cover.*` and `page.type`;
- FP6 the frame drawing honours the type's zones.

## Acceptance criteria

Design §12, AC1–AC16, including:

- **MCP contract tests** (through `ecad-mcp`'s `execute`, as `mcp_contracts.rs` does), each
  checking the transaction recorded, its author `Author::Agent(<client>)`, and the refusal with
  writes off: `newpage {type}` (with and without `generated`), `setstyle {catalogue: "page"}`,
  `setdefault {key: "page.type", level: "page"}`, `setdefault {key: "front.cover.notes"}`,
  `deletepage`, and the `cover.notes` write.
- Old projects open with the same page numbers, folders, sheets and references (AC2).
- Hostile type records and notes are refused or read as absent, never a panic (AC15).
- Screenshots side by side with boards 1.16 and 5.02 (051's rule).
- `docs/quick-start/054-page-types-front-pages.md`.

## Testing

Fast scoped tests (`cargo xtask test-changed`) before each commit, clippy before each push, no
`--include-ignored`. Tester: **yes, for 054a only** (FP3 changes how `ecad-model` resolves a
page's sheet, which references and the grid read; design §13). 054b: the reviewer only.

## Coordination

- **055 (title block editor)** owns title block layouts. 054's page type names a title block by
  ID; until 055 lands the only one is `standard`. Design §14 Q10 proposes moving the title block
  choice from the frame style onto the page type; 055's designer should agree it.
- `generated.rs` (cover content, `touches`, default title), `frame.rs` (zones) and
  `ecad-model/src/sheet.rs` (FP3) are shared files: one block each (design §0).

## Needs owner check

- Approve the design (`docs/design/054.md`) and its questions (§14).
- A click-only trial (the quick start file).
