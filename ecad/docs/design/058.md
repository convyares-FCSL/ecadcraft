# Design 058: Paged reports

Brief: `docs/briefs/058-paged-reports.md`. Builds on 024 (generated pages, report models, update
maps), 033 (continuation numbers, print ranges), 046 (graphic strip sheets), 047 (BOM entries and
view), 051 (I/O and rack sheets), 053 (BOM draw cost), 052 (device-first model), 054 (page types,
folder start numbers, Renumber pages) and 055 (frames carry their title block).

## 0. What exists today, and the files this brief touches

### 0.1 Today (checked on `origin/dev` `637c58a4`)

- `generated::GenKind` has eight kinds: `cover`, `contents`, `revisions`, `bom-location`,
  `terminals` (table or graphic, 046), `cables`, `plcrack` and `iolist`. A report is **one stored
  page**, made by `newpage {generated}`.
- `generated::sheets(doc, page, outputs)` gives a report's sheet count. It comes from kept counts
  (`Outputs::bom_sheets`, `strips.sheets_total`, `plc.iolist_sheets`, `plc::sheets::rack_sheets`,
  `strip_page::sheets`) or from the row count divided by `ROWS_PER_SHEET` (44).
  `paginate_with` splits the rows: a sheet that starts inside a group repeats its heading as
  `(continued)`, and in the terminal plan each strip starts a new sheet.
- `page_parts` draws **every sheet side by side** in one drawing, sheet *i* at x = *i* × (paper +
  50 mm). The canvas shows them all, and `sheet_windows` gives the windows to the canvas and to
  Print Book.
- `Project::frame` keeps one `Arc<Vec<Part>>` for the active page. A report page rebuilds it
  whole whenever `update_outputs` says the page changed. For the BOM that is any change of a BOM
  row (`o.revision != revision`).
- `sheet_number(n, i)` adds *i* to the trailing digits (`400` → `401`). `continuation_of`,
  `continuation_checks` and `book_sheets` (Total) count the sheets.
- The BOM table is built by `bom_with(group, nested)` from `by_location`, `nested`,
  `strips.bom_rows` and `half_loc`. 053 made it once per draw. On large it takes about 11 ms; drawing
  117 sheets takes about 40 ms and projecting them about 44 ms.

### 0.2 Words

| Word | Meaning |
| --- | --- |
| **report** | A generated page: the one stored page with a `generated` kind. |
| **sheet** | One page of a report in the Book. Sheet 1 is the stored page itself; the others are worked out from the rows (§2). People see sheets as pages. Code and this design say *sheet* when the difference matters. |
| **unit** | The smallest thing a page break never splits: a row, a group heading, a nested device with its part lines, an I/O card's table, a rack tile. |
| **group** | A heading and the units under it: a BOM location, a strip, a card, a rack, a folder in Contents. |
| **break** | Where a sheet starts: the unit it starts with and whether it repeats its group heading. |
| **layout** | A report's units in order, with their heights and its breaks. It is kept, and it is only metadata: no cells and no parts. |
| **sheet block** | One sheet's kept parts (frame and content), built and updated on its own. |

### 0.3 Files touched (per-lane layout)

| File | Change | Part |
| --- | --- | --- |
| `crates/ecad-cadcraft/src/paging.rs` (**new**) | The page-break engine: `Unit`, `Layout`, `Break`, `layout_from`, `relayout_from`, and verify against a fresh layout (§3, §7.2) | a |
| `crates/ecad-cadcraft/src/sheet_blocks.rs` (**new**) | `SheetRef`, the kept sheet blocks, and the per-sheet frame input (§6) | a |
| `crates/ecad-cadcraft/src/generated.rs` | `sheets`, `page_parts`, `sheet_windows` and `content` take one sheet. `paginate_with` is replaced by `paging`. Side-by-side drawing goes | a, b |
| `crates/ecad-cadcraft/src/outputs.rs` | The BOM's kept unit index and its delta per update (§7.3). `bom_sheets` reads the layout | a |
| `crates/ecad-cadcraft/src/project.rs` | `frame` becomes the shown sheet's block, and `active_sheet` is added. `update_outputs` returns sheet deltas, not whole pages (§7) | a |
| `crates/ecad-cadcraft/src/commands.rs`, `sheet_cmds.rs` | `openpage {sheet}`, sheet rows in `pages`/`page` reads, and `default_number` and `renumberpages` count sheets (§4) | a |
| `crates/ecad-cadcraft/src/printbook.rs` | One print sheet per sheet, and a sheet number in a range prints that sheet (§4.6) | a |
| `crates/ecad-cadcraft/src/strips.rs`, `strip_page.rs`, `plc/sheets.rs`, `plc/mod.rs` | The units and deltas of their kinds (§3.4, §7.4) | b |
| `apps/ecad-trial/src/project_panel.rs`, `shell/keys.rs`, `canvas/mod.rs` | Sheet rows in the Book, `step_page` through sheets, and one sheet window (§4) | a |
| `crates/ecad-bench/src/bom.rs`, `report.rs` | The rows of §8 | a, b |
| Tests: `crates/ecad-cadcraft/tests/all/lanes/lane_058.rs` (+ `lane_058/{paging,update,book,print}.rs`), `apps/ecad-mcp/tests/all/lanes/lane_058.rs`, `apps/ecad-trial/tests/all/lanes/lane_058.rs` | §12, §13 | a, b |

**Overlaps.** `outputs.rs` and `generated.rs` with 053 and 052b, so this brief starts after 053 and
052c merge (brief). `sheet_cmds.rs` and `generated.rs` with 054, and `frame.rs`/`sheets.rs` with
055, at the two meeting points in §10.2. `plc/sheets.rs` with 056, which adds columns only.

## 1. Goal

A person reads, prints and numbers a report sheet by sheet, exactly as any other page. A change
redraws only the sheets it reaches, so a 117-sheet BOM costs no more to edit than a 3-sheet one.

## 2. Model: a sheet is a derived page

**One stored page per report, as today. Its sheets are derived.** Sheet 1 is the stored page.
Sheet *i* (2 ≤ *i* ≤ *n*) exists while the report's rows need it. It has no ID of its own; the UI
names it `SheetRef { page, sheet }`, which is temporary UI state (Part A §3.1).

Why the sheets are not stored pages (owner Q1):

- **P5.** The sheet count is a derived result: it comes from the rows. Storing a page per sheet
  would write a derived result back as stored intent.
- **P3 and P4.** A description edit that moves a row onto a new sheet would have to append an
  `AddPage` the user never asked for, and undoing the description would have to know about it.
  Two branches that both grow the BOM would each add a sheet page, and merging them would give
  duplicate sheets.
- **Old projects** open with nothing to migrate, and the file does not change size with the
  BOM.

Everything a person sees treats a sheet as a page: the Book row, the number, the title block,
Next and Total, the Contents, opening it, Page Up and Page Down, Print Book ranges, and Checks.
What a sheet cannot do alone: move apart from its report, take a title or number of its own, or be
deleted (its report moves, renames, renumbers and deletes as one, §4.5).

## 3. Sheets: where the page breaks fall

### 3.1 Capacity

A report's sheets all share the report page's sheet and frame (§5). The drawing area is 024's
`area(input)`, with 055's block height in place of `TITLE_BLOCK_MM`
(`sheets::keep_out`). Height is counted in **row units** of `ROW_MM` (5 mm):

`capacity = floor((top − bottom − HEADER_MM) / ROW_MM)`

On A3 with the built-in block that is 44, as today (a test fixes this). On A4 portrait with the
same block it is 44 too, because only the width changes; an own frame with a 60 mm block on A3
gives 40. A capacity below 6 is raised to 6, and Checks reports the frame (§9). This guard stops
a hostile or very small frame from making thousands of sheets.

### 3.2 Units and groups

Each unit has a height in row units (≥ 1) and, if it belongs to one, its group's key. A heading
unit opens its group. A unit's key orders it and stays the same while its content changes (§7.1).

### 3.3 Rules (all kinds)

Rules are applied in this order. Each sheet starts with the column headings (1 header row of
`HEADER_MM`, drawn as today), and the area below holds `capacity` row units.

1. **Column headings on every sheet.** They are not counted in the capacity (as today).
2. **Continued heading.** A sheet that starts inside a group first repeats the group heading as
   `<heading> (continued)`, 1 row unit, drawn in `TableHeading` like the heading.
3. **Keep a group together.** A group that fits on one sheet (its heading and units ≤ capacity)
   and does not fit in the space left starts a new sheet. A group bigger than a sheet starts where
   it is, if rule 4 allows, and is split.
4. **No orphan heading.** A heading is never the last thing on a sheet: a heading with fewer than
   `MIN_ROWS` (3) of its units after it on the sheet moves to the next sheet. For a group with
   fewer than 3 units, all of them have to fit.
5. **No widow rows.** A split group leaves at least `MIN_ROWS` units on each side of the break.
   If the last part would be shorter, units move from the previous sheet to the next until it has
   3. Rule 4 still holds, and the previous sheet keeps at least 3 units of the group or the
   heading moves too.
6. **A unit is never split.** A unit taller than the capacity (an I/O card with more channels
   than a sheet holds) gets a sheet of its own, is cut at the bottom of the area, and the sheet
   draws `… <k> rows continue on the next sheet` and carries on with the remaining rows of that
   unit on the next sheet. Checks reports it as Info (§9). This is the only place a unit is cut.
7. **Kind breaks.** Some kinds start a new sheet at every group: each strip in the terminal plan
   (as today, 028) and each strip in the graphic style (046).

When rules 3–5 cannot all hold (a sheet already has a continued heading and one unit), the
sheet takes as many units as fit and the break goes after them. It never loops; §3.6 bounds it.

`MIN_ROWS` is a constant (owner Q6 confirms 3). It is not a setting.

### 3.4 Units and groups per kind

| Kind | Unit | Group (heading) | Kind breaks | Notes |
| --- | --- | --- | --- | --- |
| `bom-location`, flat | A BOM row: one `(LocKey, PartKey)`, a strip row, a connector half group | A location (`+CP1`, or its description per 052 §8.2) | — | Order as `bom_with` |
| `bom-location`, nested (`bom.lines`) | A device's main row **with its part lines** (h = 1 + lines), or an item | A location | — | A device's lines never split from it |
| `cables` | A cable row | — (no groups) | — | 056 and 057 columns do not change units |
| `terminals`, table | A terminal (h = its levels, 036) | A strip | each strip | A cable bracket may cross a break; the bracket is drawn open-ended on both sheets (as today) |
| `terminals`, graphic | A strip sheet as 046 lays it out | A strip | each strip | Units are 046's kept sheets; the layout only numbers and maps them |
| `iolist` | A card's table (h = 2 heading + channel rows + 1 gap, as `pack`) | — | — | Rule 6 for cards taller than a sheet |
| `plcrack` | A rack tile row (`rack_pack` rows, h in mm ÷ `ROW_MM`, rounded up) | — | — | 2-D packing stays inside a tile row |
| `contents` | A page row; a report's row (owner Q5) | A top-level folder | — | |
| `revisions` | An issued revision row | — | — | |
| `cover` | — | — | — | Always one sheet |

### 3.5 An empty report

A report with no units has one sheet: the column headings and one row in `shell.text_2`, `No
rows yet.`, except the revision list, which keeps 033's `No revisions issued yet.`.

### 3.6 Bounds (house rule 12: each says what it prevents)

- `MAX_SHEETS = 9_999` per report: no input-sized allocation of sheet blocks. The last sheet then
  ends with `… <k> more rows are not shown; split the report by folder or strip.`, and Checks
  warns. 5,000 BOM lines on large is 117 sheets.
- Unit heights are `u32`, added with `checked_add`. A height that overflows counts as the capacity
  (rule 6).
- `relayout_from` walks at most the units after its start (§7.2), and every step moves forward by
  at least one unit, so it ends.

## 4. Pages in the Book

### 4.1 Rows

The Book shows a report as its page row with a fold arrow and a grey count at the right: `▸ 900
BOM by location   117 sheets` (element `ecad.book.report.<page>`, the arrow
`ecad.book.report.<page>.fold`). Unfolded, one row per sheet sits under it, indented one level:
`900  BOM by location (1/117)`, `901  BOM by location (2/117)`… (`ecad.book.sheet.<page>.<n>`,
*n* from 1). A report with one sheet has no arrow and no sheet rows. A report is folded by
default when it has more than 5 sheets (owner Q4). Fold state is UI state, kept per session.

Clicking a sheet row opens that sheet. The report row opens sheet 1. Right-click on a sheet row
offers the report's menu (it acts on the report). Drag of a sheet row drags its report.
Selection, rename and delete act on the report.

### 4.2 Numbers

- Sheet 1 has the report page's stored number. Sheet *i* is `sheet_number(number, i − 1)`, by
  **the project's page number step** (owner Q2). With the default step of 1 this is 033's rule
  (`400`, `401`); with a step of 10 it is `400`, `410`. A number with no trailing digits keeps
  033's `BOM`, `BOM2`.
- **Renumber pages** (054 §4.4) counts every sheet as a page of its run: the report takes the
  run's next number, and the page after it takes the number after the report's last sheet. It
  needs the report models, so `renumberpages` builds them when a report needs them (first show
  cost, once). The transaction is unchanged in kind: one `SetProperty(Page, number)` per stored
  page whose number changes.
- `default_number` (a new page) skips every sheet number in use, as it skips continuation numbers
  today.
- **Growth into the next number.** A report that grows can reach the next page's stored number.
  Nothing is written (§2). The two sheets share the number, the book prints, and Checks keeps 033's
  `continuation-clash` row with the words `Sheet 903 of BOM by location (900–903) has the number
  of page 903 (Cable schedule). Renumber pages.` and the Fix **Renumber pages** for that folder
  (owner Q3).

### 4.3 Title, Next, Total

The sheet title is `<report title> (i/n)` when *n* > 1, as today. The graphic strip plan keeps
046's `<title>: <strip>`. Next is the next sheet's number, and the last sheet's Next is the next
page's number. Total counts sheets (`book_sheets`), as today.

### 4.4 Opening and moving between sheets

- The canvas shows **one sheet**. There is no side-by-side view of a report (owner Q7).
- Page Down and Page Up (`step_page`) step through sheets, then go on to the next page. The page
  well and the status bar show `Sheet 2 of 117` after the page number
  (`ecad.status.sheet`).
- `openpage {page, sheet?}`: `sheet` counts from 1, and is 1 when absent. Out of range is
  refused with words and nothing changes: `BOM by location has 117 sheets.` It is a view command:
  it records no transaction and runs with writes off, as `openpage` today.
- When a report shrinks below the shown sheet, the canvas shows its last sheet and the status line
  says `BOM by location now has 116 sheets.`
- The `pages` and `page` readings list a report's sheets as `sheets: [{sheet, number, title}]`,
  where `sheet_numbers` was before (FD2). `sheets` (the count) stays.

### 4.5 What acts on the report

Move, folder, rename, delete (054's `deletepage`), page type and page properties act on the stored
page, so on all of its sheets. Drawing on any sheet is refused with 024's words.

### 4.6 Print Book

Each sheet is one print sheet (`printbook.rs` takes the sheet windows of §6.2, one per sheet, in
book order). In a range, a sheet number prints that sheet only (owner Q8), which replaces 033's
note `… the whole page is printed.` A range of sheet numbers (`901-905`) prints those sheets.
Print stays on demand and is the one place every sheet of a report is built in one go. It
does not keep the blocks it builds.

## 5. Title blocks

Each sheet draws its report page's frame. 055 resolves the frame page → page type → project
(`print.frame.style`, 054 §3.2), and the frame carries its block. All sheets of a report share it,
because a sheet has no page properties of its own. The bound fields read the sheet's values:

| Binding (055 §7) | On sheet *i* of *n* |
| --- | --- |
| `page:title` | `<title> (i/n)` (§4.3) |
| `page:number` | Sheet *i*'s number (§4.2) |
| `page:next` | §4.3 |
| `page:total` | `book_sheets` |
| `page:function`, `page:location` | The report page's |
| everything else | As on any page |

There is no new binding. If the owner wants `Sheet 2 of 117` in a field of its own, that is a
later 055 addition, `page:sheet`, and this design leaves room for it.

## 6. Sheet blocks

### 6.1 What is kept

```text
SheetRef   { page: PageId, sheet: usize }          // UI state; sheet from 0 in code
Break      { first: UnitKey, continued: bool }     // where a sheet starts
Layout     { units: BTreeMap<UnitKey, Unit>,       // order and heights (and group keys)
             breaks: Vec<Break>,                   // sorted; breaks[0] = first unit
             capacity: u32, rev: u64 }
SheetBlock { at: SheetRef, start: Break, end: Option<UnitKey>,
             input: FrameInput, view_rev: u64, parts: Arc<Vec<Part>> }
```

- The **layout** of each report drawn from the report models is kept in `Outputs`, next to the
  rows (BOM: §7.3). The layouts of contents and revisions are kept in `Project`, beside their
  update map (`generated::touches`).
- **Sheet blocks** are kept for the shown sheet and the last 4 shown (a small LRU, so Page Up
  and Page Down back are instant). A block is valid while its `start`, `end`, `input` and
  `view_rev` equal what the layout and frame give now, and no unit in its range is dirty (§7.1).
  An invalid block is rebuilt when shown and never before. Blocks of sheets that are not shown
  are only marked stale.
- A sheet block is built from its own range of units. Cells are made only for those units
  (BOM: `bom_row(key)` reads one row from `by_location`, §7.3). Building a sheet never builds the
  whole table.

### 6.2 Drawing

`page_parts` draws one sheet at the origin: frame and content, no x offset. `sheet_windows`
gives one window, `(0,0)–(w,h)`, so the canvas and its fit-to-sheet work as on any page.
Projection replaces the shown sheet's parts on `ECAD-GENERATED` and the frame layer as today
(`project_page_kept`). About 500 parts for a BOM sheet: about 0.4 ms to build and 0.4 ms to
project on large, from 053's per-part costs.

## 7. Update maps

No view rebuilds a whole report on an ordinary commit. A whole layout is built only on open,
reload, first show of a report and branch switch, and when a view setting changes the units
themselves (§7.5). Everything below runs inside `update_outputs` (report-model kinds) or with
`generated::touches` (contents, revisions), after the models' own map (024 §9, 047, 028, 045, 051,
052 §8). Every other op does nothing to any layout or block.

### 7.1 Row level: what the models report

Each kind's model update returns a **unit delta** in place of today's single "page changed" bit:

```text
UnitDelta { changed: BTreeSet<UnitKey>,          // same key and height, new cells
            inserted: BTreeSet<UnitKey>,          // new units (and new group headings)
            removed: BTreeSet<UnitKey>,
            resized: BTreeSet<UnitKey>,           // same key, new height
            view: bool }                          // §7.5: every sheet's look changed, no unit moved
```

A unit whose sort key changes (a BOM row moves to another location) is `removed` under its old
key and `inserted` under its new one.

### 7.2 Layout level: which sheets

Given a delta:

1. **Changed units.** For each key, binary-search `breaks` for the sheet that holds it, and mark
   that sheet's block dirty. This costs O(changed × log sheets), and no break moves.
2. **Inserted, removed and resized units.** Apply them to `units`. Then take `s`, the sheet that
   holds the first such key (the sheet it falls on, by its key, for an insert), and run
   `relayout_from(s)`: start at `breaks[s]`, fill sheets by §3.3, and after each new break
   compare it with the old break of the same unit key. **Stop** at the first new break that
   equals an old one (same `first` and `continued`) at or after the last structural key of the
   delta. From there the old breaks hold, and are shifted to their new sheet indexes. Every sheet
   whose `start` or `end` changed is marked dirty. Work is O(units walked), and the walk never
   goes past the end of the report.
3. **Sheet count.** If the number of breaks changed, the report has grown or shrunk:
   - the Book's sheet rows for that report (§4.1) are worked out again, O(sheets of the report);
   - Total changed, so the shown frame is stale (today's `sheets(&o) != before` rule, kept);
   - the `continuation-clash` rows are worked out again (as today, on a sheet-count change);
   - Contents is marked stale if it is in the book (its row for this report shows the range);
   - blocks of sheets beyond the new count are dropped.
4. **Numbers.** If no sheet count changed, no sheet number changed, so Next, Total, Contents and
   Checks are left alone.

Then, if the shown sheet's block is dirty, it is rebuilt and projected. No other block is built.

**Example.** A device is added in location `+CP1` on sheet 3 of 117. Its row is inserted, and
`relayout_from(3)` fills sheet 3. If sheet 3 had a free row, sheet 4's break is the same and the
walk stops: one sheet is dirty. If sheet 3 was full, a unit moves to sheet 4, and so on. The walk
stops at the first sheet that starts a kept-together group at the same unit (§3.3, rule 3, makes
these frequent) or at the end. If it reached the end and the last sheet overflowed, sheet 118 is
added.

### 7.3 The BOM (058a)

The BOM's rows are kept in order as **units**, by the report models, with the same scope that
updates them today:

| Op (kind, target) | Unit delta |
| --- | --- |
| Entry ops of 047 that change a row's quantity, tags, description, maker, part or article number but not its key | `changed` (that row) |
| An entry joins a `(LocKey, PartKey)` that had none, or a device gains a location group that had none | `inserted` (the row, and the location heading if new) |
| The last entry leaves a row or location | `removed` (row, and the heading if it is now empty) |
| An entry changes location or part key (`SetProperty` location, `part.*`, `article_number`; 052b's stored location) | `removed` old row (if now empty) or `changed`; `inserted` new row or `changed` |
| A nested device gains or loses a part line (`partline`) | `resized` (its unit, h = 1 + lines) |
| A strip's BOM row or a connector half group (028, 044) | as the rows above, keyed `(LocKey, strip or half key)` |
| `SetProperty(Project, designation.<a>.<key>.code)` (052 §8.2 rename, the one wide case) | `removed` + `inserted` for the rows of that location (re-keyed), O(rows of the location) |
| `SetProperty(Project, designation.<a>.<key>.description)` | `changed` (that location's heading) |
| `SetProperty(Project, bom.column.article)` | `view` (§7.5) |
| `SetProperty(Project, bom.lines)` | the units themselves change (§7.5) |
| `SetProjectSettings` reference or colour style | `view` |
| A sheet or frame change of the BOM page (§7.6) | capacity (§7.6) |
| anything else | nothing |

`bom_with` stays for CSV, MCP readings and the Reports window. The kept units equal its order,
and AC6 checks this. `bom_row(key)` makes the cells of one unit. `bom_sheets` becomes the
layout's break count.

### 7.4 The other kinds (058b)

| Kind | Delta comes from | Notes |
| --- | --- | --- |
| `cables` | The cable rows changed by the models (`last_cables_changed` becomes the set of changed `CableId`s); created and deleted cables insert and remove | Sort key as the schedule's order |
| `terminals`, table | `strips.last_changed` per strip → that strip's terminal units (028's per-strip rows); a strip's own sheets only, since each strip starts a sheet (rule 7), so the walk stops at the next strip | `strip` property pages: only their strip |
| `terminals`, graphic | `strip_pages.last_changed` per strip → its 046 sheets | `layout_changed` and the project style (when the page has none): `view` |
| `iolist` | `plc.rows_changed` per card → that card's unit (`resized` when its channel count changed) | 056's Tag and Safe columns: `changed` |
| `plcrack` | `plc.last_changed`, `rows_changed` and `rack_places_touched` → that rack's tile row (`resized` when it changes height) | `view` on the rack style |
| `contents` | `AddPage`, `RemovePage`, folder ops, page `number` and `folder` properties (024's `touches`), and a report's sheet count (§7.2, step 3) → those page rows | Its own layout, kept in `Project` |
| `revisions` | Issue and unissue ops and revision columns (033) → those rows; columns: `view` | |

### 7.5 View changes (no unit moves)

`view` (a BOM column on or off, a reference or colour style, a strip or rack look): no break
moves, because units keep their heights and the column headings keep their height. **Every sheet
of that report is marked stale, and only the shown sheet is rebuilt.** This is the BOM column
toggle, and it costs one sheet, O(1) in the report's size.

A change that changes the units themselves (`bom.lines` flat ↔ nested): the report's layout is
built again from the kept rows. That is O(units) of metadata with no cells, under 3 ms for 5,000
lines, plus the shown sheet. This is the only case where a layout is rebuilt in a commit, and it
happens on an explicit report setting.

### 7.6 Frame and capacity

- The shown sheet's frame follows 055 §10.1 (`frame::touches`) for its sheet's inputs (§5): a
  rename of the report or a number change rebuilds the shown sheet's frame. The other sheets of
  that report are marked stale, because they carry the title and their numbers. Nothing else is
  rebuilt.
- A change of the report page's sheet, page type or frame style that changes its **capacity**
  builds that report's layout again (O(units)), marks all its sheets stale, and runs §7.2 step 3.
  The same frame change that does not change the capacity (a field moved in the block) only marks
  the sheets stale.

### 7.7 Undo, redo, merge, branch switch

Undo and redo are transactions, so they go through the same maps with their inverted ops. A merge
is one transaction, and goes through the maps as any commit (020b). A branch switch and a reload
rebuild the report models and every layout (first show, as today).

### 7.8 Verify mode

With `ecad_model::ledger::verify_mode()`, after every update each kept layout is compared with a
fresh layout of the same units (`paging::verify`). A difference records an `IndexRebuilt` event, as
`verify_strips` does. The bench runs in verify mode for its check step (§8).

## 8. Targets and bench

Measured with `ecad-bench --part paged` on **medium** while building, and on **large** once at the
end. All times are in ms on release builds, with the report on screen unless the row says
otherwise.

| Row | What | Target (large) | Counters checked |
| --- | --- | --- | --- |
| `bomcols` (047, existing) | Toggle `bom.column.article` | **< 50** (owner) | 1 sheet built, 0 units walked |
| `bomrow` (new) | `setprop` description of one device whose row is on the shown sheet | **< 50** (owner) | 1 sheet built, 0 units walked |
| `bomrowoff` (new) | The same with the row on another sheet | < 16 | 0 sheets built, 1 marked |
| `bomgrow` (new) | Add a device in the first location, sheet 1 shown | < 50 | sheets marked = sheets walked; 1 built |
| `bomshrink` (new) | Undo of `bomgrow` | < 50 | as `bomgrow` |
| `bomnested` (new) | Switch `bom.lines` | < 50 | layout built once, 1 sheet built |
| `bomsheet` (new) | `openpage {sheet: 60}` (a block not kept) | < 30 | 1 sheet built |
| `cablerow` (new, 058b) | A cable property, cable schedule shown | < 16 | 1 sheet built |
| `termrow` (new, 058b) | A terminal's wire, terminal plan of all strips shown | < 16 | 1 sheet built |
| `iolistupd`, `plcrack`, `plcframe upd` (existing) | As 051 and 053 | As today | — |

Other rows are unchanged. The bench's check step asserts that the kept layouts equal fresh layouts
and that the BOM tables equal a rebuild, as 047's does. Print benches stay on a project of at most
25 pages, and print a 25-sheet BOM range.

Expected cost from 053's numbers: a BOM sheet is about 500 of 58,000 parts, so the column toggle
on large drops from about 105 ms to the update itself plus about 1 ms of drawing. The 11 ms
grouping goes, because the units are kept.

## 9. Checks

| Kind | Level | When | Words | Fix |
| --- | --- | --- | --- | --- |
| `continuation-clash` (033, kept) | Warning | §4.2 | `Sheet 903 of BOM by location (900–903) has the number of page 903 (Cable schedule). Renumber pages.` | Renumber pages (that folder) |
| `report-unit-split` (new) | Info | Rule 6 cut a unit | `I/O card -A12 is taller than a sheet of page 950; it runs onto the next sheet.` | Go to the sheet |
| `report-too-long` (new) | Warning | `MAX_SHEETS` reached | `BOM by location needs more than 9,999 sheets; the rest is not printed.` | — |
| `report-frame-small` (new) | Warning | Capacity raised to 6 (§3.1) | `The frame of page 900 leaves room for fewer than 6 rows.` | Go to the page type |

The Checks rows are kept with the layouts and worked out only when the layout they read changes.

## 10. Dependencies

### 10.1 On 052 (device-first model)

- **052b: a BOM row's location is the device's stored location** (052 §8.3). The scope rows "devices
  whose main symbol is on page P", and box and page-location ops, are gone. So a page or box edit
  makes **no** BOM unit delta, and §7.3 has no page rows. 058 relies on this to keep the BOM's
  delta to the devices an op names.
- **052 §8.2, a designation rename and merge are the one wide case.** It re-keys the rows of that
  location. §7.3 maps it to `removed`/`inserted` for those rows, and their location's sheets walk.
- **052 §8.2, the designation description** may print in the location heading. A description
  change is `changed` on the heading unit only.
- **`DeviceRow.goto`** (052b) replaces `main`. Sheets do not use it. A row's go-to from the Reports
  window is unchanged.
- **052c** (Settings list) touches nothing here. It is named only because the worker starts after it
  (the brief).

### 10.2 On 054 and 055 (the meeting points)

- **054: `renumberpages` and `default_number`** must count each sheet as a page (§4.2). If 054 is in
  `dev`, 058a changes its run walk in one place, where a page's numbers are taken: they become
  `generated::sheet_numbers` for a report. If 054 lands after 058a, 054's worker uses that
  function.
- **054: the position (Front/Body/Back) is dropped** (owner, 2026-11-14). Nothing here reads it.
  A report goes in any folder and is numbered by that folder's run.
- **055: `sheets::keep_out(doc, page)`** is the block height for the capacity (§3.1). Until it
  lands, the capacity reads `TITLE_BLOCK_MM`, and the one call is changed when 055 lands.

## 11. Format: decisions for the overseer

No `.ecad` change is proposed.

| # | Decision | Recommended |
| --- | --- | --- |
| **FD1** | Sheets are derived, not stored: no op, record, field or page property is added. A report is one stored page, as today (§2). Existing projects open unchanged. | Yes |
| **FD2** | Command surface only, not format: `openpage` gains `sheet`, and the `pages`/`page` readings gain `sheets: [{sheet, number, title}]` (§4.4). MCP contract tests cover them. | Yes |
| **FD3** | Only if the owner answers Q1 "stored": each sheet would be a stored page with the properties `report.of = <page id>` and `report.sheet = <i>`. Every commit that changes a sheet count would add or remove those pages in the same transaction (computed before commit). A merge rule would be needed for two branches that both add sheets, and old projects would need a one-off write on first open, which the house rules forbid ("opening never writes"). | Not recommended |

## 12. Acceptance checks (the tester runs these from outside)

**058a**

1. **AC1** On the medium tier project, the BOM report shows in the Book as one folded row with
   `<n> sheets`. Unfolded, it shows *n* sheet rows numbered in sequence from the report's number.
2. **AC2** Opening sheet 3 (Book row, or `openpage {page, sheet: 3}`) shows one sheet with column
   headings, the title `BOM by location (3/n)`, its own number in the title block, and Next =
   sheet 4's number.
3. **AC3** Page Down from the last sheet opens the next page. Page Up from sheet 1 opens the page
   before.
4. **AC4** Rules: a location of 10 rows that would split is moved whole to the next sheet. A
   heading is never last on a sheet. A split location has ≥ 3 rows on each side. A sheet starting
   inside a location repeats it `(continued)`. Checked on a synthetic project built for each rule.
5. **AC5** Nested BOM: a device and its part lines are never split across sheets.
6. **AC6** Seeded random edits (500 steps: add, delete, relocate, re-part, add and remove part
   lines, undo, redo) with the BOM shown: after each step the kept layout equals a fresh layout,
   and the units equal `bom_with`'s order.
7. **AC7** An op unrelated to the BOM (a wire drawn on a schematic page) walks 0 units, marks 0
   sheets and builds 0 blocks.
8. **AC8** `bom.column.article` toggled builds exactly 1 block (the shown sheet), walks 0 units and
   marks every sheet of the report stale. Opening another sheet then builds it with the column.
9. **AC9** A description change of a row on sheet *k* that is not shown builds 0 blocks and marks
   only sheet *k*.
10. **AC10** A row inserted on sheet *k* walks from sheet *k* and stops at the first realigned
    break, and its count is reported. Sheets after it are not marked.
11. **AC11** Growth: adding rows until a sheet is added grows the Book's rows by one and updates
    Total on the shown frame. If the new sheet's number is another page's number, the
    `continuation-clash` row appears, and its Fix renumbers that folder with room for every sheet.
    Undo removes the sheet.
12. **AC12** Shrink while the last sheet is shown: the canvas shows the new last sheet, and the
    status line says so.
13. **AC13** Print Book: range `901` prints one sheet. `900-905` prints six. The whole book's
    PDF page count equals Total.
14. **AC14** Old project (a `.ecad` saved by `dev` before 058 with a BOM page): it opens, page
    numbers are unchanged, and the BOM's sheet numbers follow the report's number.
15. **AC15** MCP: `openpage {page, sheet}` records no transaction, works with writes off, and an out
    of range sheet is refused with §4.4's words. `renumberpages` through `execute` records one
    transaction with author `Author::Agent(<client>)`, leaves room for sheets, and is refused with
    writes off. `printbook {pages: "901"}` prints one sheet.
16. **AC16** Hostile: a report with no rows draws `No rows yet.`. A `sheet` of 2^63 is refused.
    A frame whose capacity is 1 is raised to 6, with the Check. `MAX_SHEETS` is reached on a
    synthetic report with the Check. No panic anywhere.
17. **AC17** Bench rows of §8 meet their targets on large, and the counters match.

**058b**

18. **AC18** For each kind in §7.4: a row change on the shown sheet builds 1 block, one on another
    sheet builds 0, a seeded edit run keeps layout = fresh layout, and the kind's rules hold
    (each strip starts a sheet, an I/O card taller than a sheet is split with the Check, and the
    graphic strip sheets keep 046's titles).

Screenshots: a BOM sheet in the app (light and dark), the same printed, and the Book with a folded
and an unfolded report.

## 13. Tests

- **`lane_058/paging.rs`**: the §3.3 rules, table-driven (units, capacity → breaks), including
  rule conflicts, rule 6 and the bounds of §3.6. Pure, with no project.
- **`lane_058/update.rs`**: AC6–AC10 on generated projects through `ProjectHandle`, with counters
  `sheet_builds`, `sheets_marked`, `units_walked` and `layouts_built` exposed like `sorts` and
  `continuation_builds` today. The seeded edit run uses a fixed-seed LCG, so no new crate is
  added.
- **`lane_058/book.rs`**: numbers by step, Renumber with sheets, `default_number` skipping
  sheets, the clash Check and its Fix, Next and Total, fold rows, shrink clamp.
- **`lane_058/print.rs`**: AC13 on a project of at most 25 pages.
- **`ecad-mcp` `lanes/lane_058.rs`**: AC15.
- **`ecad-trial` `lanes/lane_058.rs`**: Book fold and sheet rows by element name, Page Down
  across a report, the status `Sheet 2 of n`.
- **Old project**: AC14 from a fixture written by today's code in the test (no binary fixture
  committed).
- Existing tests that read side-by-side windows (`sheet_windows` with *n* > 1, x offsets) are
  changed to one window per sheet. A test that only checked the side-by-side layout is removed and
  named in the PR.
- **Tester: recommended for 058a**. Page numbering and Renumber change, and an incremental
  page-break bug shows only as a wrong or stale sheet. 058b: the reviewer only.

## 14. Parts

| Part | Contents | Size | After |
| --- | --- | --- | --- |
| **058a** | `paging.rs`, sheet blocks, one sheet per window, Book rows, numbers, Renumber, Print Book, `openpage {sheet}`, the BOM's units and map, bench rows `bom*`, Checks | L | 053 and 052c merged; 054/055 meeting points §10.2 |
| **058b** | Cables, terminal plan (both styles), I/O list, racks, contents, revisions on the engine; their deltas; `cablerow`, `termrow` | M | 058a |

Until 058b lands, the other kinds keep today's side-by-side drawing: 058a keeps
`page_parts`' old path for kinds without a layout, behind one `match`, and 058b removes it. Both
parts land on `feature/paged-reports`, and the feature reaches `dev` in one PR.

## 15. Questions for the owner

1. **Q1. Are sheets stored pages, or derived pages?** Recommended: **derived** (§2). Each sheet is
   a page everywhere a person looks (Book, number, title block, print, Contents), but only the
   report is written to the file. Stored sheets would write a derived count into history, add pages
   to edits that never asked for them, and conflict on merges (FD3).
2. **Q2. Do sheet numbers count by the project's page step?** Recommended: **yes**. A sheet is
   numbered like any page in its run. At the default step of 1 this is today's `400`, `401`.
3. **Q3. When a report grows into the next page's number, what happens?** Recommended: **a Checks
   warning with a one-click Renumber pages for that folder**. Nothing renumbers by itself, because
   a derived change never writes. Folder start numbers (054) with room between them make this rare.
4. **Q4. How does the Book show a report's sheets?** Recommended: **one report row with a fold
   arrow and a sheet count**, and a sheet row each when unfolded. It is folded by default above 5
   sheets, so a 117-sheet BOM does not push the drawings off the panel.
5. **Q5. How does Contents list a report?** Recommended: **one row per report with its number
   range** (`900–1016  BOM by location`), as today, rather than one row per sheet.
6. **Q6. Do the keep-together and widow rules hold as written?** Recommended: **yes**: a group that
   fits on a sheet is never split, and a split leaves at least 3 rows on each side. This uses more
   sheets than today on reports with many small groups (estimate: up to about 10 % more BOM sheets).
7. **Q7. Is the side-by-side view of all a report's sheets dropped?** Recommended: **yes**. The canvas
   shows one sheet, and Page Up and Page Down step through sheets. The Book is the overview.
8. **Q8. Does a sheet number in a Print Book range print just that sheet?** Recommended: **yes**,
   replacing 033's "the whole page is printed".
9. **Q9. Which release?** Recommended: **0.2.3, first item**. It needs 053 and 052 (0.2.1) and meets
   054 and 055 (0.2.2). If the owner wants the 0.2.1 BOM target closed in 0.2.1, 058a can go
   first with today's numbering and block height, and wire the two meeting points (§10.2) when
   054 and 055 land.

## 16. Out of scope

Stored sheets (FD3); moving, retitling or deleting one sheet; a report split across folders, or
per-location BOM reports (a later brief could add `generated` filters like the terminal plan's
`strip`); wrapped multi-line cells; a `page:sheet` title block field (055 later); new report
kinds; CADCraft block definitions or INSERT entities, which are not needed because a sheet is its
own page window (§6.2).
