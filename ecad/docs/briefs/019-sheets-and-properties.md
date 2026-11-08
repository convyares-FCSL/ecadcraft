# Brief 019: Sheets, title block, project and page properties, revisions

Status: Approved (owner, 2026-11-08: "a page border will have to be one of the next things and will
naturally lead to what project and page properties we want, including revision control")
Branch: `feature/sheets` (worktree `C:\dev\ecad-features\sheets`), from `feature/project-manager`
(016 + 018); rebased onto `dev` once that merges.
Designer: **yes** (`docs/design/019.md`) · Size: L
Roadmap: `docs/roadmap-v0.1.md`, the page-frame half of F6. Printing, generated pages and CSV
lists stay in F6.

## Goal

Every page is a real sheet: a drawn frame at true scale with a column and row grid and a title
block, so the designer keeps a sense of scale. The information a title block needs (project,
page, revision) lives in the project as properties that can be seen and edited in one place.

## What exists today

- `Page { title }`, `page_order`, and the ops `AddPage`, `RemovePage` and `SetProperty`. Check
  whether `SetProperty` can target a page and the project, and use it if it can.
- `NameVersion` and `UnnameVersion` (named points in the log); `SetProjectSettings`.
- The 016 Project panel (Pages | Devices) and the 018 device properties (description, maker, part
  number, `+` location, `=` function).
- `/page.column` references, whose column is computed today without a drawn frame.
- CADCraft's Properties palette, unused by ECAD so far.

## Scope

1. **The sheet frame.** Default A3 landscape (420 × 297 mm) at 1:1 in model units. Drawn as derived
   graphics, never stored as lines:
   - the border;
   - a column grid lettered A–H and rows numbered 1–6, by default;
   - margins.

   Frame size and grid are project settings, with a per-page override for odd pages. References
   (`/page.column`, and later `page-column-row` such as `110-B-5`) use the same grid, so what the
   reference says is what the frame shows.
2. **The title block.** A default layout, derived and never stored, filled from the properties
   below:
   - project title, drawing number and customer;
   - page title, page number, and "this / next / total";
   - the revision table: rev, date, description, drawn, checked, approved;
   - status;
   - function (`=`) and location (`+`).

   One built-in layout for now; custom title-block templates come later.
3. **Project properties.** A dialog from the Project panel header holding the title, drawing
   number, customer, status and default function and location, plus any free properties.
4. **Page properties.** A page has:
   - an internal ID (hidden);
   - a **page number** chosen by the designer (text such as `100` or `105a`, default the next
     number);
   - a **title or description**;
   - an optional `=`/`+` override.

   No page type (owner): folders say what kind of page it is.
5. **Folders.** Pages grouped in the panel, nested, with drag between folders and reordering
   inside them. A folder is a name and an order, not a page.
6. **Revisions.** A project revision list: rev (`A`, `B`… or `0`, `1`…), date, description, and
   drawn, checked and approved by. "Issue revision" names the current point in the log through
   `NameVersion`, so a released revision can always be reopened as it was. The title block shows
   the current revision and the table.
7. **The Properties panel.** CADCraft's Properties tab shows the ECAD properties of the selection:
   - device or symbol;
   - wire, cable or reference;
   - page (when nothing is selected);
   - project.

   Edits go through the same commands (`SetProperty` and the tag rules). This becomes the main
   place to edit; the separate Device Properties tool can stay as a shortcut.
8. **Live and cheap.** An **update map** for every derived thing: frame, title block, Properties
   panel, folder tree and reference texts (`ecad/CLAUDE.md` "Derived views", no lazy rebuilds).
   A page-number change refreshes the texts that show it, not the project.
9. **Quick start:** a "Sheets and properties" section with click-only steps.

## Format and ops

New ops and `.ecad` format additions are allowed **only when required**. The worker or designer
proposes them with reasons; the **overseer decides** and reports to the owner. Prefer existing
ops (`SetProperty`, `SetProjectSettings`, `NameVersion`). Keep additions optional, left out at
their defaults.

## Acceptance criteria

1. A new project's pages show an A3 frame at true scale with A–H and 1–6 and a filled title block.
   Zoom to fit shows the whole sheet.
2. The page number, title, project properties and revision table can be edited from the Properties
   panel or the dialogs. Each edit is one transaction and one `u`, and the title block follows.
3. A reference's column matches the frame's lettered column on both ends.
4. Folders nest. Dragging a page between folders is one transaction.
5. Issuing a revision records it and names the log point. Reopening that revision shows the project
   as it was.
6. Save and reopen keep everything. An old file opens with defaults (frame A3, page numbers as
   positions).
7. The large bench shows no whole-project work per edit for any of the above.
8. Reference screenshots, approved by the owner.

## Needs owner check

- Approve the design (`docs/design/019.md`).
- A click-only trial: frame, page numbers, folder, title block, issuing Rev A.
