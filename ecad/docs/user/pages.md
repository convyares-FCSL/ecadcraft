# Pages

A project is a book of pages. Each page has a number, a title, a type and a place in the book (in a
folder or not). Exactly one page is open in the drawing at a time. Some pages are **generated**: ECAD
draws their content from the project, and nothing on them is stored.

Words used here are in the [glossary](../reference/glossary.md).

## Find and open pages

- **The Book panel** (first button of the tool strip) lists the pages, by thumbnail or as a list (the
  header switches). **Find a page** filters by number or title. Folders show their name, page count and
  a **+** for a new page in that folder; click a folder to fold it. The open page has a blue bar and a
  blue number. The panel can be pinned beside the drawing or float over it.
- **The page well** in the bottom bar shows the open page between `« ‹` and `› »`, the pages of its
  folder, the other folders folded, and **+** (**Add a page after this one**). Hovering a page button
  shows a preview with its number, title and folder.
- Keys in the Book: Up and Down move, Enter opens, F2 renames, Space previews. Alt+Up and Alt+Down move a
  page; dragging a row moves it too.
- A page row's wheel (right-click) has **Open**, **Rename**, **Preview**, **New page**, **New folder** and
  **Page properties**. **Preview** shows another page read-only, without leaving the one you are on.

Undo and redo bring the page they change to the front.

## Add, rename, number and move pages

**+** (in the page well, the Book header or a folder row) opens the list of page types, with the number
the page will get. The page goes right after the open page (or last in the folder) and opens with its
title ready to type. The Book footer's **New page ▾** also lists the generated kinds.

- **Rename**: F2 or double-click. A title is 1 to 200 characters.
- **Number**: double-click a page's number, or set **Number** in its page properties. A number is 1 to
  12 letters, digits or `_` (`105`, `105a`). A number another page has is refused. A page keeps its
  number when it moves, so references do not change.
- **Folders**: **New folder** in the Book footer or a page's wheel. Drag pages onto a folder. Folders
  nest up to 8 deep; inside a folder, pages come before sub-folders. Deleting a folder moves its pages up
  a level.
- **Start number and Renumber pages**: a folder row's menu has **Start number…** (`100`; empty clears)
  and **Renumber pages**. The Book header's number button renumbers every folder from its start number
  and the pages in no started folder from 1, in one step. Numbers are never rewritten until you press it.
- **Tick** pages (checkbox or Ctrl+click) for **Move to group** and **Delete**.

Each of these is one step of undo.

## Page types

Every page has a type: **Cover sheet**, **Schematic**, **Network**, **Single-line**, **Panel layout**,
**Report**, **Plain**, or one the project adds. A type brings the page's sheet, frame, title block and
zones. A page's own sheet still wins.

A page's type is its own, else its generated kind's (a cover, revision history or contents is a Cover
sheet page; a BOM, terminal plan, cable schedule, PLC racks or I/O list page is a Report page), else
Schematic.

**Project menu ▸ Settings ▸ Pages and sheets** lists the types with their sheet, frame, title block,
zones and page count. The right panel edits the selected type. **+ Page type** copies it. **Checks and
references** off means the type's pages get no electrical checks, no cross-references and no device or
wire numbering; the built-in **Plain** type, for imported drawings and notes, has it off. The project's
own sheet and numbering settings are folded under the table.

## Sheets, frames and zones

Each page sits on a sheet: the paper's bottom-left corner is the drawing's origin. The built-in sheet is
A3 landscape, with a frame whose columns are numbered `1`–`8` and rows lettered `A`–`F`, labelled on all
four sides. The zones run over the whole frame, title block included. References name these zones (see
[References](references.md)).

Sheets: A3, A2, A1 and A0 (each with its own grid: A2 is 12 × 8) or a custom size in millimetres, each
with columns and rows numbered or lettered. Set the project's sheet in **Project menu ▸ Project settings
▸ Sheet**, a type's in Pages and sheets, or one page's in its page properties. Changing the grid changes
reference texts on every page.

The frame and title block are derived: a click on a frame line selects nothing and says how to change it.

## Title blocks

The built-in frame's title block shows the project's and the page's facts: a revision table (newest at
the top), a logo box, Project, Customer, Status, Title, Drawing no., Rev, Function, Location, Size, and
the Page, Next and Of counters. The values come from **Project menu ▸ Project info** and the page's
properties; nothing is drawn by hand.

**Library ▸ Title blocks** is the editor:

- The built-in block is read only; **+ Title block** makes your own, from the built-in or blank, for A3
  to A0. **Import DXF** imports a block from a DXF at 1:1.
- Drag fields from the palette onto the block (project, page and revision fields, **Project field…** for
  a project property of your own, **Logo**). Tools: Select, Line, Text, Field, Logo, with **Sample** and
  **Snap** switches. **Frame…** sets the sheet the frame is drawn for.
- Edits go to a draft; **Save** writes one step and says how many pages it changes.
- A row's menu: **Use for this project**, **Duplicate for…**, **Promote to standard**.
- Logos are vector drawings from a DXF, fitted to their box.

Checks: **A value is cut in the title block**, **Title block fields this build cannot fill**, **Page
sheet differs from its frame's; the frame wins** (with **Clear the page's sheet**).

## Generated pages

Add one from **+** (Cover sheet and Report list their kinds) or **New page ▾**:

| Kind | Shows |
| --- | --- |
| Cover | The project title, customer and site, Notes, Wire colours, Symbol legend, Revisions, Structure codes |
| Contents | Every page in book order |
| Revision history | The issued revisions |
| BOM by location | The BOM, one block per location (see [Reports](reports.md)) |
| Terminal plan, Terminal strips | See [Terminal strips](terminal-strips.md#strip-pages) |
| Cable schedule | See [Cables](cables.md) |
| PLC racks, I/O list | See [PLC](plc.md#plc-pages-and-lists) |

Drawing on a generated page is refused. A generated page that does not fit on one sheet continues on
more sheets of the same page; the Book lists them under the page while it is open
(`Sheet 2 of 3 · 401`) and shows one sheet at a time. Go-to from a list or Checks opens the right sheet.
Print Book prints every sheet. Page numbers and the book's total count every sheet.

**The cover**: with nothing selected on it, the inspector's page tab shows **On this page** (a row per
cover block with its summary, **Edit…** and an on/off switch), **Generated pages** (Revision history and
Contents on or off: off removes the page, undo brings it back) and **Sheet** (the type and title block).
**Edit…** opens the table editor: Notes are typed (add, remove, reorder); Wire colours and Symbol legend
are generated rows you can hide, reorder or relabel; Revisions are read only here. The symbol legend lists
only the symbols placed. On screen each derived block has a dashed outline that does not print.

Checks: **The cover's symbol legend does not show every symbol**, **The cover's notes are past 40 notes of
400 characters**, **Continuation sheet has another page's number**, **Page type not found**, **Folder start
number not read**, **A folder's page numbers run into another folder's**.

## Command window

```
newpage {"title": "Motor control"}
newpage {"type": "network", "after": "<page id>"}
newpage {"generated": "contents"}
openpage {"page": "<page id>", "sheet": 2}
renamepage {"page": "<page id>", "title": "Supply"}
pageprops {"number": "105", "title": "Supply", "sheet": "A2"}
projectprops {"title": "Pump station", "drawing_number": "D-100", "sheet": "A3", "page_number_step": 1}
projectprops {"props": {"cover.notes": "..."}}
setdefault {"key": "front.cover.structure_codes", "level": "project", "value": "yes"}
newfolder {"name": "Schematic"}
folderprops {"folder": "<folder id>", "start": "100"}
renumberpages {}
deletepage {"pages": ["<page id>", ...]}
titleblocks {}
```

`openpage` records nothing and works with writes off. `deletepage` takes generated pages only.

## Limits

- A drawn page cannot be deleted yet (`Deleting a drawn page comes later; move its content first.`).
- **Duplicate** in the Book footer is not available yet.
- **Panel layout** is greyed in the page type list until the panel layout page exists.
- The thumbnails and the page well's preview are sketches of a page, not renders of it.
- The preview shows no connection point or wire-end marks and cannot be edited.
- Title-block texts are fitted by an estimate of their width, so a long value may be squeezed or cut a
  little early.
- Logos from PNG or JPEG files cannot be used; DXF only.
- The editor's DXF prompts take a typed path.
