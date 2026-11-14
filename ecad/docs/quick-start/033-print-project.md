# Brief 033: print project

Part of the [quick start](../quick-start.md). A section number refers to a section of this file
when one is headed with it here, otherwise to the quick start's (1–10, 5A…).

## What this brief changed

**Brief 033 (design `ecad/docs/design/033.md`), on `feature/print-project`, not yet in `dev`:**
Print Book's PDF holds on each page only what lies on that page (an approved upstream patch to
CADCraft's PDF writer), so a book grows in step with its sheets: the 25-page bench book went from
7,897 KB to 335 KB. A missing value in a generated table is `?` in a thin box, yellow on screen and
black when printed in either colour mode. An automatic contact table that would run into the title
block goes right of its coil, else is lifted above the title block; a table placed by hand stays
and Checks lists `Contact table over the title block` with **Move table…**. `table` on an ECAD
page is refused, nothing written: `Tables can't be placed on ECAD pages yet. Draw it with lines
and text, or add a generated page for lists (New Page ▸ Generated).` Generated pages number their
continuation sheets in sequence (`400, 401`). Project settings gain **Tags** (leading `-`, case
as typed; changing the dash rewrites every tag in one undoable step) and **Revisions** (pending
changes marker `D*` / `E (draft)` / off, and the revision table's columns with custom fields such
as ECR). The Rev box shows `D*` when the drawing changed after the newest issue. Section 5-033
describes it.
- *Automated checks:* `crates/io/src/tests.rs` (upstream: a page holds only its window's paths,
  20 pages ≤ 2.2 × 10, a culled page equals its sheet plotted alone),
  `ecad/crates/ecad-cadcraft/tests/all/print_project_contracts.rs` (the boxed `?`, its print
  colour), `ecad/apps/ecad-mcp/tests/all/print_project_contracts.rs` (the `contacts` reply's
  `table`, the Checks row, `table` refused, writes off), `print_project_q7_contracts.rs` (pending
  marker with undo and save/reopen, revision columns and custom fields, tag style and its
  rewrite, renumber under `typed`, writes off), unit tests in `revisions.rs` (column grammar,
  hostile settings) and `ecad-model/src/tag.rs` (the tag style table).
- *Agent-observed:* the worker ran the 25-page print bench before and after (release build).
- *Owner-tried:* not yet.

## 5-033. Print and project (by clicking)

Brief 033, on `feature/print-project`. Start from the demo project of section 5K (it has a BOM
page and devices without part numbers).

1. **Book size.** Click **Print Book**, keep **All pages**, check **File**, **Print**. Expected:
   `Printed n sheets (… generated) to … (… KB).`: a few hundred KB, not megabytes. Each PDF page
   looks as before.
2. **Missing values.** Show the BOM page: a missing part number is `?` in a thin yellow box. Print
   Book with **Colour: Screen**: in the PDF the `?` and its box are black.
3. **Contact table and the title block.** On a drawing page place a coil low on the sheet (just
   above the title block) with two contacts on another page. Expected: its contact table sits to
   the right of the coil, or is lifted above the title block; never over it. Move the table into
   the title block with **Move Mirror Table**: it stays there, and Checks lists `Contact table over
   the title block` with **Move table…**.
4. **Continuation numbers.** Number the BOM page `400` (Pages list, double-click its number). With
   enough devices for two sheets, the second sheet's Page box reads `401` and the first sheet's
   Next reads `401`; Contents lists `400–401`. Typing `401` as another page's number is refused:
   `Number 401 is sheet 2 of page 400 (BOM by location).` Print Book **Pages** `401` prints the
   whole BOM page and says why.
5. **`table`.** Type `table` on a drawing page and place it: refused, `Tables can't be placed on
   ECAD pages yet. Draw it with lines and text, or add a generated page for lists (New Page ▸
   Generated).` Nothing is added.

6. **Tag style.** Project settings ▸ Tags: untick **Leading - on tags** (the line under it reads
   `k5 is written K05`), **OK**. Every device and cable tag loses its `-` in one step (`Tag style:
   no leading -. n tags rewritten.`); one **u** puts them all back. **Case ▸ As typed**: tags
   typed from then on keep their letters (`M02a`); existing tags stay upper case.
7. **Revision columns.** Project settings ▸ Revisions ▸ **Columns**: type
   `rev, date, ecr:ECR, description, drawn`, **OK**. The title block's revision table now has an
   ECR column and no Checked or Approved. In the Revisions window the pending row has an ECR
   input. A wrong list says why under the field and is not written.
8. **Pending changes.** Issue Rev A, then draw a line: the Rev box reads `A*`, the cover
   `Revision A, … (changed since)`, and the Print Book dialog (and its result) notes `The drawing
   has changed since Rev A; the title block shows A*.` Undo the line: `A` again. Filling in the next revision's fields
   does not count. **Pending changes ▸ E (draft)** shows `B (draft)`; **Off** shows `A`.

Typed instead: `projectprops {"tag_dash":false}`, `projectprops {"tag_case":"typed"}`,
`projectprops {"revision_columns":"rev, date, ecr:ECR, description, drawn"}`,
`revision {"fields":{"ecr":"ECR-114"}}`, `projectprops {"revision_pending":"draft"}`,
`contacts {"component":"<id>"}` (its `table` says where the table went and why),
`pageprops {"page":"<id>"}` (`sheet_numbers`), `printbook {"scope":{"pages":"401"}}` (`notes`).
