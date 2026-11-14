# ECADCraft UI rev 3: build spec

Transcribed from the owner's rev 3 handoff (Part 1, pages 1–8; 13 November 2026), so later
workers read text rather than the PDF. The PDF itself is not in the repository. Part 2 of the
handoff shows every board at 1920 × 1080, numbered 1.01 to 6.01 to match the table below.

Screens are rendered with system fonts; the design uses IBM Plex Sans for the app and Barlow Semi
Condensed on the sheet. Design canvas: claude.ai/artifact/GttUyoA7zQVVRe78xLMoRB.

## The build spec

This is the spec to build from. Where earlier design rounds disagree with it, this wins. The full
spec doc also keeps the reasons behind each decision.

The screens are in ECADCraft UI (the design canvas), one board per state, at 1920 × 1080,
numbered as in the table near the end of this section. Part 2 of the PDF shows every board.

## Principles that decided rev 3

- The drawing comes first. Nothing permanent sits over the sheet. Panels open beside it or float
  over it only until you start drawing.
- One bar at each edge. The top bar is for the project and the app. The bottom bar is for status,
  pages and the view. Nothing is stacked into two bars.
- Draw is the only workspace with the tool strip. Project, Library and Publish are project-wide or
  external, so they use the full width.
- Every titled section in a side panel folds. A folded section shows a one-line summary of what is
  set.
- Grey text is read from the drawing. An input box is something you set here.
- One name for one thing. A tagged item (-K01, -X10, -M01) is a **device** everywhere: the Devices
  panel, the Device tool, the Device tab, Project ▸ Devices.

## Shell at 1920 × 1080

| Region | Size | Holds |
| --- | --- | --- |
| Top bar | 40 px high | The same on every screen, Settings included. Project name and menu (open, save, settings and standards) at the far left. Workspace switch: Draw, Project, Library, Publish. Search centred on the window (Ctrl K). At the right: agent icon or "Claude writing" pill, one light/dark switch, help (?), account avatar. No branding, no revision pill. |
| Tool strip | 56 px wide | Draw workspace and Symbol builder only. See below. |
| Panel slot | 248 px (Book, Devices) or 280 px (History) | One panel at a time, beside the strip. |
| Canvas | the rest | The sheet fits with frame and title block visible, 24 px margin. |
| Inspector | 320 px (Draw), 340 to 360 px (other workspaces); 56 px collapsed | Details of the selection. |
| Bottom bar | 32 px high | Status at left, page well in the centre, view at right. |

## Tool strip (Draw)

Top to bottom:

1. Panels, each with a › mark to show it opens a panel (‹ while open): **Book**, **Devices**,
   **History**.
2. Double divider.
3. **Select**, **Draw**, **Text**.
4. Divider.
5. **Device** (place a symbol or a connector), **Terminal**, **PLC**.
6. Divider.
7. **Wire** (wire, bridge, continuation), **Cable**.
8. Divider.
9. **Location** (location box, function box, device or PLC outline).
10. At the foot: **Snap**, **Grid**, **Ortho**. Each is a toggle and each has a popout:
    - Snap: snap spacing, snapping to connection points.
    - Grid: shown or hidden (separate from snap), spacing, style, major lines.
    - Ortho: 90° only, 45° steps, or free.

Every tool from Draw to Location is a group with a flyout (the corner mark). Terminal's flyout
holds: Terminal, Terminals across wires, Draw a link (bridge), Swap top and bottom, Reorder the
strip, Renumber, and a link to the Terminal manager. PLC's holds rack overview, detail page, card
and channel.

Buttons are 54 × 44 px, an 18 px icon over a 10 px label.

The Symbol builder strip is Select, Draw, Text, then Point, Slot, Origin, with the same foot.

## Panels: pin or let it close

- Each panel header has a pin.
- **Pinned:** the panel docks and the drawing moves over for it. Only one panel is pinned at a
  time; pinning another unpins the first.
- **Unpinned:** the panel floats over the drawing with a shadow and closes when you start drawing.
  The drawing does not move.
- **Book:** pages in groups, thumbnails or a list. The open page has a 3 px blue bar, a blue
  outline on its thumbnail and a blue page number. Selected pages (for moving) get a ticked
  checkbox and a plain grey background, so the two never look alike. Multi-select footer: Move to
  group, Duplicate, Delete.
- **Devices:** search, then a grouping control: None, Location, Function, Type. "Not on a page
  yet" sits first, and you drag from it onto the sheet.
- **History:** see the History section below.

## Inspector (right panel in Draw)

- Header: tag, description, where it is, a Row button (opens the device's row in Project) and the
  collapse button.
- Tabs: **Device**, **Layout**, **Checks**.
- Device: a one-line status of this device's own checks, then Identity, Parts, Where it is drawn,
  Connections, Notes and attributes. All fold.
- Layout: how this symbol is drawn. Placement (position in mm, rotation 0/90/180/270, mirror,
  variant, lock). Text on the sheet: for each slot (tag, description, pin labels, part number,
  location) whether it shows, where, and its size from the standard; Reset to standard; labels can
  also be dragged on the sheet. Contact table under a coil: below, right or hidden; used contacts
  only or all. Line and colour (folded, from the symbol).
- Checks: This page or Project. Each check has its fix button and a go-to arrow, then "Elsewhere
  in the project".
- The panel collapses to a 56 px rail, the same width as the tool strip and its mirror image (the
  collapse button in the panel header, or `]`). The rail holds one labelled button per tab:
  Device, Layout, Checks, each with a ‹ mark like the strip's panel buttons. Pressing one expands
  the panel on that tab. Checks carries a dot when there are problems.
- Device checks stay with the device. Page and project checks have their own tab, which the bottom
  bar counter also opens. A check about another device never appears in a device's details.

## Bottom bar

- **Left, status:** Saved time; Rev C D\* (opens History); the checks counter (opens the Checks
  tab).
- **Centre, the page well:** a darker inset area, 880 px wide, centred. Previous group and
  previous page arrows at its left end, next page and next group at its right end. Current page
  number, "/ 09" and title, then the page buttons. Only the open group shows its pages; other
  groups fold to a folder icon, a count and a ›, with the group name on hover. A › opens one more
  group. Hovering a page shows a preview above the bar. + adds a page after this one.
- **Right, the view:** standard (IEC), sheet size (A3), sheet colour as one group, then Fit, zoom
  out, zoom %, zoom in as another.
- Every group sits in its own bordered pill: Saved, Rev, Checks, the page well, IEC · A3 · sheet
  colour, and Fit with zoom. The project name in the top bar uses the same pill as the workspace
  switch beside it. In Project, Library and Publish the well and the view groups are hidden; the
  status stays.

## Sheet colour and full dark

- The sheet colour menu offers **Paper** (always light), **Follow** (the app theme) and **Dark**
  (always dark).
- It changes the screen only. PDFs and prints always use paper colours.
- The default for new projects is set in Settings.
- The canvas colour tokens have a dark set (`.dark-sheet`) alongside the paper set.

## Placing a device

- The Device panel has Symbol and Connector. Continuations live in the Wire group, since they
  continue a wire.
- After a symbol is chosen the picker folds to a small chip at the top left: the symbol, "as
  -K02", Change (P), and search. P or the chip reopens it.
- The intent line sits at the top of the canvas and says what the click will do; it never covers
  the title block.
- The inspector shows the device about to be placed and what the click changes.

## Connector maker

Sections, each folding with a summary:

- **Which halves:** plug only, socket only, plug and socket, socket in a panel.
- **Type:** by family, a chip per variant.
  - M12: A-coded, B-coded, D-coded, X-coded, T and L power.
  - M8: 3, 4 and 8 pin.
  - D-sub: 9 W, 15 W, 25 W, 37 W, HD 15, HD 26.
  - Heavy-duty: Han 3A, 4A, 6B, 10B, 16B, 24B, modular.
  - Other: RJ45, USB-C, 7/8", custom. Or search the library.
- **Pins:** the count is set by the type. Each pin has a label and a description. Shield contact on
  the housing.
- **Hood and housing:** plug hood (straight, angled, none), cable entry (M20, M25, M32), locking
  (single or double lever), socket housing.
- **Draw it:** all pins in a row, or pin by pin where needed.
- Then tag, description and the two halves (-X10.S socket, -X10.P plug). Each half is its own
  device with its own location and parts.

## Radial wheel

- About 190 px across, eight slots, icons only.
- The slot under the cursor names itself in a tooltip and says what it will do.
- Fixed positions: the two most frequent at top and top right, "go to" at right, its row at bottom
  right, delete at the bottom.
- A slot may open one short list. A slot that cannot run stays in place, greyed, and its tooltip
  says why with a way forward.

## History: revisions, branches and the agent

The History panel (280 px), top to bottom:

- **Revision:** "Rev D\* pending", what changed, Release D… and Compare with C.
- **Agent:** shown while an agent is connected. It always writes on its own branch, so it reads
  "Claude is writing on branch fan-circuit", with Pause, Review and merge, and disconnect. Its
  changes show outlined in lime on the sheet until merged.
- **Branches:** main (you are here), other branches with how far ahead they are and a Merge
  button, merged branches greyed. New branch.
- **Changes:** a branch graph down the left edge (main in blue, a live branch in lime, merged
  branches grey), each change with who made it, the time, the branch and where it is. Filter: All,
  main, This page. Selecting a change marks it on the page.
- **Versions:** named versions with Compare. Name this one.

Compare opens the full view: the sheet with numbered revision clouds, the list of changes on this
page, and the selected change with the command that made it. A person's edit and an agent's call
are the same command, recorded the same way.

## Project workspace

Was "Lists". Tabs, in the same order as the tool strip: Devices, Terminals, PLC | Wires, Cables |
Reports.

- Each tab is a full-width table with Import, Export and "Open beside the page".
- The right panel shows only the selected item: its properties and its own checks. Detail belongs
  in the table, not the panel.
- **Terminals:** laid out like a terminal plan. Inside (read from the drawing), the strip (what you
  set), outside (read from the drawing).
- **PLC:** the rack overview with a card per slot, then the selected card's channels.
- **Wires:** every wire, grouped by potential (or none, page, cable). Number, from, to, colour,
  size, type, ferrules, length, page, status; provisional numbers and wires on a branch are
  marked. The right panel shows the selected wire: identity, how it is made (category, colour and
  size from the category, type) and its ends (ferrules, marker, length). Renumber and Export.
- **Cables:** every cable with type, cores, length, ends, glands, cores used and checks. Below, the
  selected cable drawn end to end: the cabinet end and its terminals, glands, the jacket, each core
  in its own colours, the field end and its terminals; an open core is marked red. Under the
  drawing, the core table: core, colour, size, both ends and their wire numbers, status. The right
  panel: identity, part (which sets the cores), length with spare, both ends and glands, shield,
  and the cable's checks with their fix.
- **Reports:** each report with what it reads, grouping and sort, layout, where it sits in the
  book, when it was generated and what it exports. A preview below, as a table or on the sheet.
  The right panel sets contents, columns (drag to reorder), output, and automatic or on-demand
  updates.

## Library workspace

Tabs: Symbols, Snippets (small circuits you reuse), Parts (manufacturer data), Makers, Title
blocks. Category tree at the left, filters across the top, tiles in the centre, the selected item
at the right. Symbol builder, Make symbol and Edit symbol are as in rounds 1 and 2.

## Settings

- Opened from the project name menu, which shows as active while Settings is open. Same top and
  bottom bars as every other screen.
- Left: Back to Draw (Esc), then Settings for this project (Project info, Standards, Pages and
  sheets, Libraries, Agent access, Versions and backups) and for this computer (Appearance,
  Shortcuts).
- **Project info:** project (name, number, description, customer, site, customer reference);
  people and approval (drawn, checked, approved, what a release needs); functions and locations
  with descriptions and how many devices use each, renaming one updates every tag; the title block
  with a live preview and where each field is filled from (drawing number pattern, page title,
  function and location, drawn, customer logo); revision letters and the pending mark; defaults
  for the project (sheet, sheet colour, language, units).
- **Standards:** as before, now inside the shared bars.

## Publish workspace

- **Left:** output sets (Full booklet, Panel builder pack, Customer approval, Site wiring), each
  remembering its pages, order and settings. Last published.
- **Centre:** every page at a readable size, grouped as in the book. Tick to include, drag to
  reorder; a blue line shows where a dragged page will land. Each page shows its position in the
  output and "changed" since the last release.
- **Right:** PDF, Printer or DXF. Paper (size, scale, colour), On every page (revision letter,
  clouds, watermark, bookmarks, numbering), and the selected page as it will print. Export PDF and
  Print at the foot.
- Publish has no history; that lives in the Draw History panel.

## Boards on the canvas

Numbered by workspace: 1 Draw, 2 Project, 3 Library, 4 Publish, 5 Settings, 6 Tokens. The Shared
parts page holds the pieces every screen uses: top bar, bottom bar, tool strip, Book, Devices and
History panels, and the sheet.

| Board | Shows |
| --- | --- |
| 1.01 | Draw, both sides closed |
| 1.02 | Book open and pinned, two pages selected |
| 1.03 | Devices panel unpinned, floating |
| 1.04 | History panel, agent writing on its branch |
| 1.05 | Right panel open, Device tab |
| 1.06 | Right panel, Layout tab |
| 1.07 | Right panel, Checks tab |
| 1.08 | Page well: hover preview, another group opened |
| 1.09 | Location group flyout |
| 1.10 | Terminal group flyout |
| 1.11 | Placing a symbol, picker folded |
| 1.12 | Placing a symbol, picker open |
| 1.13 | Connector maker |
| 1.14 | Radial wheel |
| 1.15 | Parts list report page |
| 1.16 | Compare revisions, from History |
| 1.17 | Light theme |
| 1.18 | Full dark: sheet follows the theme |
| 2.01 | Project ▸ Devices, with a part set |
| 2.02 | Project ▸ Terminals |
| 2.03 | Project ▸ PLC rack |
| 2.04 | Project ▸ Wires |
| 2.05 | Project ▸ Cables |
| 2.06 | Project ▸ Reports |
| 3.01 | Library ▸ Symbols |
| 3.02 | Library ▸ New symbol |
| 3.03 | Library ▸ Make symbol from a selection |
| 3.04 | Library ▸ Edit symbol, with what changes |
| 4.01 | Publish: pick, order, print settings |
| 5.01 | Settings ▸ Project info |
| 5.02 | Settings ▸ Standards |
| 6.01 | Design tokens |

## Suggested build order

1. Shell: top bar, bottom bar with the page well, the empty panel slot and the inspector frame.
   Theme and sheet colour tokens, including the dark sheet set.
2. Tool strip with flyouts and the Snap, Grid and Ortho toggles.
3. Book panel with pinning, then Devices with grouping.
4. Inspector with folding sections, the three tabs and the collapsed rail; the checks counter.
5. Device placement with the folding picker and the intent line; then the connector maker.
6. Project workspace: one table component reused for every tab; the cable drawing; then Reports.
7. History panel: revision and changes first, then branches and merge, then Compare.
8. Publish.
9. Library tabs beyond Symbols; Settings ▸ Project info.
10. Radial wheel.
