# Workspace

The window has one bar at each edge, and the drawing comes first. This guide names each part and
what it is for. Tools and keys are in [Drawing](drawing.md).

## Top bar

From the left:

- **The project name.** Its menu: New project, Open project…, Save, Save As…, Close project;
  Project info, Project settings, Revisions; Settings, Standards ([Projects](projects.md)).
- **The workspaces**, **Draw**, **Project**, **Library** and **Publish** (Ctrl+1 to Ctrl+4).
- **Search** (Ctrl+K), centred: pages, tags, symbols and actions. Enter or a click on an action
  runs it on the selection or starts its tool.
- At the right: the agent icon ([Agent access](agent-access.md)), **Share** (does nothing yet),
  the light and dark switch, help and the account (both come later).

While an agent is connected, its banner shows under the top bar.

## The four workspaces

| Workspace | Shows |
| --- | --- |
| **Draw** | The page, with the tool strip, the panels and the inspector |
| **Project** | Lists of the whole project, one tab each ([below](#the-project-workspace)) |
| **Library** | The symbol library and the symbol builder ([Library](library.md)) |
| **Publish** | Output sets, the pages to print, and PDF export ([Publish](publish.md)) |

Library and Publish have no inspector. In Project, Library and Publish the bottom bar keeps only
its status.

## Panels: Book, Devices, History

The three buttons at the top of the tool strip open a panel beside it; pressing the button again,
or the panel's close button, closes it.

- **Pinned** (the pin in the panel's header): the panel docks and the drawing moves over. One
  panel is pinned at a time.
- **Unpinned:** the panel floats over the drawing; starting to draw closes it.

Book starts pinned, Devices and History unpinned; the pin is remembered.

- **Book** lists the pages in folders, as thumbnails or a list, with **Find a page**, ticks for
  several pages, **New page** and **New folder** ([Pages](pages.md)).
- **Devices** lists the devices with a search and a grouping (**None**, **Location**,
  **Function**, **Type**; Location by default). **Not on a page yet** comes first. A group shows
  eight devices, then "n more". A click goes to the device's first symbol and selects its symbols
  on that page. **Project →** opens Project ▸ Devices ([Devices](devices.md)).
- **History** shows the pending revision, branches, changes and named versions
  ([History](history.md)).

## The inspector

The rail at the right edge has one button each for **Device**, **Layout** and **Checks**; Checks
carries a red dot when there are problems. Pressing one opens the inspector on that tab; pressing
it again closes it. `]` or the close button folds it too.

- **Device** names what is selected in its header (tag, type, where it is), with a **Row** button
  that opens its row in the Project workspace. Then a status line of its own checks and folding
  sections. For a device: **Identity** (tag, description, function, location…), **Parts**,
  **Where it is drawn** (a click goes to that symbol), **Connections**, and **Notes and
  attributes**. A folded section shows a one-line summary. Wires and cables show their own
  sections ([Wires](wires.md), [Cables](cables.md)); every editable field is described in
  [Properties](properties.md).
- **Layout**: **Placement** (position, rotation, mirror, read from the drawing), **Text on the
  sheet** (each text's On box, where, and its text style), the **Contact table** (Below, Right,
  Hidden) and **Line and colour**.
- **Checks**: **This page** or **Project**, each check as a card with its fix and a go-to, then
  "Elsewhere in the project" ([Checks](checks.md)).

With nothing selected, Device shows the page's properties and Layout its defaults.

## The bottom bar

- **Status** at the left: the save state and time, the revision (**Rev C D\***, opens History) and
  the checks counter (opens the Checks tab).
- **The page well** in the centre: the page shown (number, "/ total", title) between `« ‹` and
  `› »`, the open group's page buttons, the other groups folded, and **+** to add a page after
  this one. Hover a page button for a preview; right-click it for the page's wheel.
- **The view** at the right: the standard (opens Settings ▸ Standards), the sheet size, the sheet
  colour menu, **Fit**, zoom out, the zoom % and zoom in.

## The Project workspace

**Project** (Ctrl+2) shows the whole project as lists. Its tabs, each with a count: **Devices**,
**Terminals**, **PLC** | **Wires**, **Cables** | **Reports**.

- The head has the filter, **Import**, **Export** (copies the rows shown as CSV) and **Open beside
  the page** (the page opens beside the table; press again to close it).
- The foot says what the colours mean and gives the tab's counts.
- Click a row: the inspector shows that item only, with where it is drawn (a go-to), its own
  checks and its sections. Devices keeps the property editor (Tag, Description, Maker, Part
  number…), which runs the same commands as editing on the page.

The tables sort (click a header), filter (the search field and chips), group, select several rows
(Shift-click, Ctrl-click, Ctrl+A, ↑ ↓) and edit in place: select a row, then Enter or F2 (or
double-click a field); Enter writes it as one undo step, Esc puts it back, Tab writes it and moves
on. A refused edit shows its reason under the cell. A value you set looks like a field; a value
read from the drawing is grey and, when it has a target, a click goes there. A change made anywhere
tints the changed cells for a moment.

What each tab holds: [Devices](devices.md), [Terminal strips](terminal-strips.md), [PLC](plc.md),
[Wires](wires.md), [Cables](cables.md#the-cable-manager), [Reports](reports.md). The check list is
the inspector's Checks tab; a row's "1 check" opens it, filtered to that item.

## Settings

Settings is a place you visit, opened from the project menu. It is described in
[Projects](projects.md#project-info-and-project-settings).

## What is remembered

The workspace, panels, pins, the inspector and each page's zoom are kept per project in the
program's own settings on this computer, not in the `.ecad` file. A setting that cannot be read
starts with the default.

## Limits

- **Import** in the Project workspace is greyed: there is no importer yet.
- **Share**, help, accounts and floating panels are not built yet.
- Book's **Duplicate** does nothing yet, and its **Delete** removes generated pages only.
- Dropping a "Not on a page yet" device onto the sheet is not available yet.
- The page thumbnails and the well's preview are sketches of a page, not renders of it.
- The saved time is the time the project became saved in this session; a project opened already
  saved shows **Saved** with no time until it is saved again.
