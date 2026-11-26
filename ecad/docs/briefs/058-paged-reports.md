# Brief 058: Paged reports

Status: Draft, not started; planned for 0.2.6 (designer, 2026-11-14; written against the pre-0.2.3 tree,
so re-check paths and the design against the current code before building; owner questions in design §15, format decisions for the
overseer in design §11)
Branch: to be cut from `dev` when the work starts
Designer: **yes** (`docs/briefs/058-paged-reports-design.md`) · Size: L (two parts, design §14)
Roadmap: the BOM column target of 0.2.1 ("the BOM column change … drop under their targets"),
which 053 could not close on the large tier. Release slot: the overseer's call (design §15 Q9).

## Goal

Every generated report works with page breaks. Each sheet of a report is a **page** of its own in
the Book: it has its own number in its folder's run, its own row, its own title block, opens on
its own and prints on its own. Each sheet is built and updated on its own, from an exact update map,
so a change rebuilds only the sheets it reaches. This covers the BOM, the cable schedule, the
terminal plans (table and graphic), the PLC racks, the I/O list, the contents and the revision
list.

## Why

053 got the BOM column toggle on the large bench project from 1.5 s to about 0.1 s and stopped:
the BOM is one drawing of 117 sheets side by side, about 58,000 parts, and a column change moves
every cell on every sheet. Drawing those parts takes about 40 ms and turning them into drawing
entities another 44 ms, so no further fix to the old shape gets under 50 ms (053, "Remaining
misses"). The owner asked on 2026-11-14 for reports that work with page breaks, each sheet its
own block.

## What exists today (`origin/dev` `637c58a4`)

- A generated report is one stored page with the page property `generated` (024 FP1). Its rows
  are split into sheets of 44 rows (`ROWS_PER_SHEET`), and all its sheets are drawn **side by
  side in one drawing** (`generated::page_parts`, `SHEET_GAP`).
- Sheets 2 onwards are numbered after the page (`400`, `401`…; 033 §5.1). A sheet number that
  another page also has is a Checks row (`continuation-clash`). The Book shows the report as
  **one** row. Print Book prints the whole report when a range names one of its sheets.
- The page's parts are kept whole (`Project::frame`) and rebuilt whole when anything on any
  sheet changes (`update_outputs`, `frame_stale`).
- The report models (`outputs.rs`) are kept and updated per transaction from their update map.
  The BOM's rows are grouped from scratch on each draw (`bom_with`, about 11 ms on large).

## Open question: report sheets as pages of their own

To settle before building, against the current tree:

- **Question:** should each sheet of a report page (BOM, terminal strip pages, cable schedule, I/O list) be
  a page in the book of its own, rather than a derived view of one stored page? The engine would then write
  pages nobody made, against "derived, never stored".
- **Since this brief was written (0.2.3):** a report page is one stored page and its sheets are derived. The
  page window shows and draws one sheet at a time (the Pages tree lists them under the page), and Print Book
  prints them all. On the large project that brought a BOM column toggle from 95 ms to 16 ms and a strip
  edit with a 252-sheet graphic strip page shown from 436 ms to 6 ms, so keeping parts per sheet between
  edits was not needed. The performance case in "Why" is met; what remains is whether each sheet should be
  a page in the book (its own number, title block and Book row).

## Scope

**058a: paged reports, the frame of it, and the BOM**

1. A report's sheets are **derived pages**: one stored page per report, as today, and its sheets
   worked out from its rows (design §2). Nothing new is stored.
2. **Sheets.** One page-break engine for every report kind. Group headings and column headings
   are repeated on each sheet. A group that fits on one sheet is kept together. A heading never
   ends a sheet, and a split never leaves fewer than 3 rows of a group on a sheet (design §3).
   The rows that fit on a sheet come from the sheet and its frame's block height (055), not a
   fixed 44.
3. **Pages in the Book** (design §4). Each sheet has a row under its report in the Book and a
   number in its folder's run. Renumber pages (054) counts each sheet as a page. The canvas shows
   one sheet at a time, and Page Up and Page Down step through the sheets. Print Book prints a
   single sheet when a range names it. Next and Total count sheets. `openpage` gains `sheet`.
4. **Title blocks** (design §5). Each sheet has its report page's frame and title block, resolved
   page → page type → project (054, 055), with its own number, Next and `(i/n)` title.
5. **Sheet blocks and update maps** (design §6, §7): each sheet's parts are kept and rebuilt on
   their own. There is an exact map from each op to the sheets it reaches. A row change rebuilds
   only the sheet that holds the row. When rows cross a page break, the page breaks are worked out
   again from that sheet until they line up with the old ones. Sheets are added and removed as the
   report grows and shrinks. A whole-report rebuild happens only on open, reload, first show and
   branch switch.
6. The **BOM by location** on this engine, with its rows kept in order by the report models, so no
   draw groups the BOM from scratch.

**058b: the other report kinds**

7. The cable schedule, the terminal plan (table and graphic styles), the I/O list, the PLC racks,
   the contents and the revision list, each with its units, groups and update map (design §3.4,
   §7.4).

Out of scope: storing sheets in the file; moving, retitling or deleting one sheet apart from its
report; a report spread over several folders; rows taller than one line (wrapped cells); new
report kinds; the 056 and 057 column additions, which arrive as columns of the kinds above;
changing `ROW_MM` or the table look.

## Format and ops

**No `.ecad` change**: no new op, record, field or property. Design §11 lists the decisions for
the overseer: FD1, sheets are derived and not stored; FD2, `openpage {sheet}` and the sheet rows
of the `pages` reading; FD3, what stored sheets would need, in case the owner answers Q1 the other
way. Existing projects open unchanged. Their sheets may break differently because of the
keep-together rules, which is intended.

## Acceptance criteria

Design §12, AC1–AC18, including:

- An incremental layout that differs from a fresh one fails: in verify mode, and in a seeded
  test that runs random edits and compares the two after each one (AC6).
- Counters prove the maps. An unrelated op walks no unit and marks no sheet. The column toggle
  builds one sheet. A row change on a sheet that is not shown builds none. A row inserted on
  sheet k works out page breaks only from sheet k until they line up (AC7–AC10).
- Bench, measured on the medium tier while building and on the large tier once at the end, all
  under 50 ms: `bomcols` and `bomrow`; `bomgrow` and `bomnested` (design §8).
- **MCP contract tests** through `ecad-mcp`'s `execute`: `openpage {page, sheet}`, which records
  no transaction, works with writes off and refuses a sheet out of range in words;
  `renumberpages`, whose transaction and author `Author::Agent(<client>)` are checked, which
  leaves room for sheets and is refused with writes off; `printbook` with a sheet number.
- Hostile reports never panic: no rows, a unit taller than a sheet, a huge sheet index, and the
  sheet cap (AC16).
- Screenshots: a BOM sheet in the app and printed, the Book with a folded and an unfolded report.
- `docs/quick-start/058-paged-reports.md`.

## Testing

Fast scoped tests (`cargo xtask test-changed`) before each commit and clippy before each push. No
`--include-ignored`. Tester: **recommended for 058a** (design §13). The Book's page numbers and
Renumber change, and an incremental page-break bug would be silent: a row on the wrong sheet, or
a sheet that is not redrawn. 058b: the reviewer only.

## Coordination

- **Start after 053 (PR #109) and 052c merge**: 053, 052b and this brief all edit `outputs.rs`
  and `generated.rs`. Design §10 lists what this design depends on in 052.
- **054 (page types, PR #111)** and **055 (title blocks)**: they provide the folder runs and
  Renumber pages, and the frame's block height (`sheets::keep_out`). If they are not in `dev` when
  058a starts, the two meeting points are named in design §10.2, and whichever lands second wires
  them.
- 056 and 057 add columns to the I/O list and the cable schedule. Columns go through each kind's
  column list, so they do not conflict with this design.

## Needs owner check

- The owner's questions, design §15 (Q1 decides the model).
- A click-only trial (the quick start file).
