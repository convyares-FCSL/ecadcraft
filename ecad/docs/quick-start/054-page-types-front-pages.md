# Brief 054: page types, Settings ▸ Pages and sheets, and generated front pages

Part of the [quick start](../quick-start.md). Brief: `docs/briefs/054-page-types-front-pages.md`;
design: `docs/design/054.md` (boards 1.16 and 5.02 of UI rev 4).

## What this brief changed

**Branch `feature/page-types`**, cut from `dev` at `8b6c2ccd`.

| Where | What is there now |
| --- | --- |
| Page types | Every page has a type: **Front sheet**, **Schematic**, **Network**, **Single-line**, **Panel layout**, **Report**, or a project's own. A type brings the page's sheet, frame style, title block and zones. A page's type is its own `page.type`, else its generated kind's (a cover, revision history or contents is a Front sheet page; a BOM, terminal plan, cable schedule, PLC racks or I/O list is a Report page), else Schematic. A page's own sheet still wins |
| Settings ▸ Pages and sheets | The table of page types (board 5.02): Sheet, Frame (the zone grid, or Plain), Title block, Zones, Position (Front · Body · Back) and how many pages have the type. The right panel shows the selected type's sheet, with Size, Title block, Frame style and Zones, and its Position. **+ Page type** copies the selected type. The project's own sheet and numbering rows are folded under the table (**Project sheet and numbering**) |
| + Page | Every + that adds a page (the page well, the Book header, a Book group row) opens a list of page types. Front sheet and Report open their generated kinds. The page goes right after the open page (or last in the group), with the next free number. Panel layout is shown but disabled until the panel layout page exists |
| Book | A folder row's menu has **Start number…** (the folder's first page number, `100`; empty clears) and **Renumber pages**. The Book header's number button (**Renumber pages**) renumbers every folder from its start number and the pages in no started folder from 1, in one step. **Delete** takes a selection of generated pages |
| Commands | `newpage {type, after}`, `folderprops {folder, start}`, `renumberpages {folder?}`, `deletepage {page \| pages}` (generated pages only) |
| Checks | `page-type-unknown`, `page-type-field`, `folder-start-unknown`, `folder-runs-overlap` (a folder's numbers run into another folder's) |

Two intended look changes on old projects: existing front pages (cover, revision list,
contents) lose their zone ticks and labels (the Front sheet type has zones off), and the cover
draws its new blocks. Page numbers and folders are never rewritten until you press **Renumber
pages**.

## Try it (by clicking)

1. Open the demo. Project menu ▸ **Settings ▸ Pages and sheets**. Expected: six types; Panel layout
   is selected; Front sheet, Panel layout and Report read **Plain**, the others `A–F × 1–8`.
2. On the **Schematic** row, open **Sheet** and choose **A2 landscape**. Expected: one step in
   History; every schematic page without a sheet of its own is now A2 (its frame and zone
   references follow). Press **u**: back to A3.
3. Esc back to the drawing. Click **+** at the right of the page well. Expected: the list of page
   types, Schematic highlighted, the number the page will get at the right; Panel layout greyed.
   Choose **Network**. Expected: a new page right after the open one, titled `Network`, with the
   rename field on its title; the status line says `Added page … (Network) after …`.
4. In the Book, make a folder `Schematic`, right-click it ▸ **Start number…**, type `100`, Enter.
   Expected: `Schematic  100` on the row. Drag a few pages into it, then press the Book header's
   number button (**Renumber pages**). Expected: its pages read 100, 101, …; the others 1, 2, …;
   one **u** puts every number back.
