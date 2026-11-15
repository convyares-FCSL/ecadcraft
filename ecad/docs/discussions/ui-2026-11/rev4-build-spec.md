# ECADCraft UI rev 4: build spec

Transcribed from the owner's rev 4 handoff (Part 1, pages 1–9; 14 November 2026), so later
designers and workers read text rather than the PDF. The PDF itself is not in the repository.
Part 2 of the handoff shows every board at 1920 × 1080, numbered 1.01 to 6.01 to match the board
table below.

Screens are rendered with system fonts; the design uses IBM Plex Sans for the app and Barlow Semi
Condensed on the sheet. The boards come from the owner's design canvas.

## The build spec

This is the spec to build from. Where earlier design rounds disagree with it, this wins. Rev 4
keeps the rev 3 shell and adds the v3 scope: the PLC list as the backbone, network topology,
calculations as checks, tests tied to revisions, a 2D panel layout, generated front pages and a
simple title block editor.

The screens are in ECADCraft UI (the design canvas), one board per state, at 1920 × 1080, numbered
as in the table near the end. Part 2 of the PDF shows every board.

## What changed since rev 3

- **Project** gains three tabs: **Network**, **Power** and **Tests**. PLC grows into the signal
  list, with sync to the PLC project.
- **Wires** and **Cables** carry a **Rating** section. Panel wires are rated from the part's
  datasheet; field cables from the IEC 60364 / BS 7671 tables, with volt drop.
- **Power** checks every source against its budget (current) and the enclosure against its heat
  limit (**Current | Heat**).
- **Tests**: one test per signal and device, made from the PLC list. A change after a pass asks for
  a retest.
- **Panel layout** is a page type in Draw with its own tool strip: rails, trunking, cut-outs,
  dimensions. Fill % and cut lengths; no routing, no 3D.
- **Front pages** are generated: cover with notes, wire colours and symbol legend; revision
  history; contents.
- **Library ▸ Title blocks**: a simple editor. Place fields, bind each to project info, the page or
  the revision, add a logo, one frame per sheet size.
- **Settings ▸ Pages and sheets**: each page type brings its sheet, frame, title block, numbering
  and its place in the book (front, body, back).
- Out of scope, on purpose: hazardous areas, functional safety calculations, 3D, automatic wire
  routing.

## Principles

- The drawing comes first. Nothing permanent sits over the sheet. Panels open beside it or float
  over it only until you start drawing.
- One bar at each edge. The top bar is for the project and the app. The bottom bar is for status,
  pages and the view.
- Draw is the only workspace with the tool strip. Project, Library and Publish use the full width.
- Every titled section in a side panel folds. A folded section shows a one-line summary of what is
  set.
- Grey text is read from the drawing. An input box is something you set here.
- One name for one thing. A tagged item (-K01, -X10, -M01) is a **device** everywhere.
- Little text. An on/off is a toggle with a short name; detail goes in a tooltip.
- Calculations are **checks**, not separate tools. A result shows where the item is, with the basis
  it used and a fix.
- Every feature carries its own weight. Small to medium teams, not every case EPLAN covers.

## Shell at 1920 × 1080

| Region | Size | Holds |
|---|---|---|
| Top bar | 40 px | Same on every screen, Settings included. Project name pill and menu at the far left. Workspace switch: Draw, Project, Library, Publish. Search centred (Ctrl K, also quick sums). At the right: agent icon or "Claude writing" pill, one light/dark switch, help (?), account avatar. |
| Tool strip | 56 px | Draw and Symbol builder only. Draw has two sets: schematic and panel layout. |
| Panel slot | 248 px (Book, Devices) or 280 px (History) | One panel at a time, beside the strip. |
| Canvas | the rest | The sheet fits with frame and title block visible, 24 px margin. |
| Right panel | 320 px (Draw), 340 to 360 px (other workspaces); 56 px collapsed | The selection. |
| Bottom bar | 32 px | Status at left, page well in the centre, view at right. |

## Tool strip

Schematic pages, top to bottom:

1. Panels, each with a › mark (‹ while open): **Book**, **Devices**, **History**.
2. Double divider.
3. **Select**, **Draw**, **Text**.
4. Divider. **Device** (symbol or connector), **Terminal**, **PLC**.
5. Divider. **Wire** (wire, bridge, continuation), **Cable**.
6. Divider. **Location** (location box, function box, device or PLC outline).
7. At the foot: **Snap**, **Grid**, **Ortho**, each a toggle with a popout.

Panel layout pages swap the middle for: **Select**, **Draw**, **Text** | **Device**, **Rail**,
**Trunking** | **Cut-out**, **Dimension**. The foot is the same.

Symbol builder: Select, Draw, Text | Point, Slot, Origin.

Every tool from Draw down is a group with a flyout (the corner mark). Buttons are 54 × 44 px, an
18 px icon over a 10 px label.

## Panels: pin or let it close

- Each panel header has a pin. Pinned: it docks and the drawing moves over. One pinned at a time.
  Unpinned: it floats with a shadow and closes when you start drawing.
- **Book:** pages in groups (Power and control, PLC, Panel layout, Reports, and the front pages),
  thumbnails or a list. The open page has a blue outline on its thumbnail; selected pages get a
  ticked checkbox and a grey background.
- **Devices:** search, then group by None, Location, Function, Type. "Not on a page yet" sits first.
- **History:** revision, agent, branches with merge, changes with a branch graph, named versions.

## Right panel in Draw

- Tabs: **Device**, **Layout**, **Checks**. Device holds identity, parts, where it is drawn,
  connections, notes. Layout holds placement, text on the sheet, the contact table and line and
  colour. Checks is this page or the project, each with a fix and a go-to.
- Collapses to a 56 px rail with three labelled buttons (Device, Layout, Checks), each with a ‹
  mark. Pressing one opens that tab. Checks carries a dot when there are problems.

## Bottom bar

- Left: Saved, Rev C D\* (opens History), checks counter. Each in its own bordered pill.
- Centre, the page well: arrows for group and page, current page number, "/ 10" and title, the
  open group's pages; other groups fold to a folder, a count and a ›. Hover previews a page. +
  adds a page.
- Right: IEC · A3 · Paper/Follow/Dark as one pill; Fit, zoom out, zoom %, zoom in as another. In
  Project, Library and Publish only the status shows.

## Project workspace

Tabs: **Devices**, **Terminals**, **PLC**, **Network** | **Wires**, **Cables**, **Power** |
**Reports**, **Tests**. Each tab shows its count. Each is a full-width table with Import, Export
and "Open beside the page". The right panel shows only the selected item: its properties, its
checks and their fixes.

### PLC: the signal list

- Rack overview, a card per slot with address range, channels in use and its page; "Make its page"
  for a card not drawn yet.
- The selected card's channels: Ch, Address, Tag, Description, Type, Safe state, Drives (the device
  and pin, from the drawing), Page, PLC, Test.
- **PLC** column: in sync, a differing name (e.g. Valve_In), or "not in PLC". A chip above the
  table counts the differences; **Compare** opens them side by side, **Export tags** writes the
  tag table for the PLC project, and **Import** reads it back.
- **Test** column: the revision it passed at, retest, failed, or —.
- The rack overview is also a generated page in the book, always current.

### Network

- A topology drawing above the table, built from ports and cables, not drawn by hand.
- Each network has a colour and a legend chip: PROFINET (VLAN 10), Plant LAN (VLAN 20), EtherCAT,
  IO-Link, Internet.
- Zones: inside the cabinet (+CAB1), the field (+FIELD), and outside / internet. Anything crossing
  the cabinet wall goes through a drawn bulkhead or service port (-X20 door RJ45, -X21 M12), so
  inside and outside are always clear.
- Switches show numbered ports with VLAN bands. Every link runs from a named port to a named port
  with its cable tag (-A01 X1 → -A10 port 1, cable -W20).
- Devices show their port tags (X1, X3, NIC1, NIC2, LAN, WAN, IN, OUT). A PC lists software that
  matters to the network (Node-RED).
- Firewall -A60: LAN inside, WAN out through a gland to the Internet cloud and on to the web
  server.
- Fieldbus lines draw in order: EtherCAT -A40 → -A41 → -X21 → -A42 IO-Link master, then its ports
  to the sensors.
- Toggles: **Traffic** (dotted overlay: Modbus TCP 502, MQTT TLS 8883, with a lock for encrypted),
  **Address**, **Cables**.
- Table: Device, Description, Network, Port, Connected to, Address, Name, Cable, Check. A duplicate
  IP is shown red and blocks its test.
- Right panel (switch selected): its ports with VLAN chips, the VLAN list, **Export switch
  config**.

### Wires, with conductor rating

- Every wire grouped by potential (or none, page, cable): number, from, to, colour, size, type,
  ferrules, length, page, status.
- Right panel **Rating**: basis Datasheet, Table or Manual. Default for panel wiring is
  **Datasheet**: the part's rating at its reference temperature × the panel temperature factor ×
  the bundling factor. Example: 27 A × 0.87 (40 °C) × 0.80 (in duct T2) = 18.8 A, against -Q01 at
  4 A ✓.
- Do not use the IEC field-cable tables as a stand-in for panel wire size: a tri-rated conductor in
  a panel carries more.

### Cables, with IEC rating

- Cable list, the selected cable drawn end to end (glands, jacket, each core in its colours, open
  cores red), and the core table.
- Right panel **Rating**: basis Datasheet, **IEC table** or Manual. IEC table uses the install
  method (e.g. E, perforated tray), the table rating for the size, then ambient and grouping
  factors. Example: 23 A × 0.94 (35 °C) × 0.89 (3 on tray) = 19.3 A against -Q01 at 4 A ✓.
  **Volt drop** from length and load: 0.3% against a 5% limit ✓.
- Identity, Ends and Where it is drawn fold.

### Power: current

- The load tree traced from the drawing: each source and what it feeds.
- Every source has a budget: MCCB, MCB, MPCB, ECB channel, PSU, PoE switch, IO-Link port, PLC rack,
  terminal. Columns: Type, Rating, Load, Peak, Used (bar with an 80% mark), Check.
- Toggles: **Peak** (inrush and start current, against the device's own short-time limit such as a
  PSU boost) and **Diversity** (set per group, e.g. 4 of 6 valves on).
- Right panel for a source: capacity from the part (derated for panel temperature, boost), load
  (running, peak, spare), diversity, warn level (80%, from the project).
- Footer: sources, how many above 80%, how many over.

### Power: heat

- **Current | Heat** switch in Power.
- Each enclosure as a card: heat in against what it can lose (186 W of 300 W with fan), and the
  inside temperature against its limit (41 °C of 45 °C).
- Heat sources table, from each part's dissipation and its load.
- Right panel: enclosure (size, material, mounting), conditions (ambient), cooling (fan -E10 and its
  airflow), heater toggle.

### Tests

- One test per signal and per device that needs one (drives, network, stops), generated from the
  PLC list.
- Coverage bar: passed, failed, retest, not tested. Filter: All, Open, Failed.
- Rows grouped by card, then devices: test, signal or device, do, expect, Pass/Fail, revision tested
  at, by, date.
- **Retest**: a test that passed is flagged when something it depends on changes later (e.g. wire
  Q-04 changed at 2/B5 after it passed at Rev C).
- **Blocked**: when a test cannot run yet (not in the PLC project; duplicate IP).
- Right panel: steps (from the signal), result with note and photo, history across revisions.
- **FAT sheet** makes printable pages; **Export** and **Import results** use a spreadsheet for the
  test bench.

### Reports

Each report with what it reads, grouping and sort, layout, where it sits in the book, when
generated, what it exports; a preview below.

## Draw: panel layout page

- A page type with its own scale (1:4 by default) and strip. Plate and door side by side.
- Rails, trunking and cut-outs drawn on the plate; devices placed from the schematic at their real
  size.
- **Not on the plate**: a floating list of devices in this location still to place, dragged on.
- Trunking selected: size and type (slotted, closed, tray), cut length as drawn, **Fill** against
  its limit (58% of 50%) with the wires that pass through it and a suggested size ("Use 60 × 60"),
  and its part.
- No routing and no 3D.

## Draw: generated front pages

- Front pages sit before the schematic in the book: Cover (01), Revision history (02), Contents
  (03).
- The cover holds notes, wire colours (from the wire categories), a symbol legend (only symbols
  used), revisions (from History) and the title block.
- Right panel Page tab: toggle each block, list of front pages, sheet. Everything updates itself;
  only the notes are typed.

## Library workspace

Tabs: Symbols, Snippets, Parts, Makers, **Title blocks**. Symbol builder, Make symbol and Edit
symbol as before.

**Title blocks** (simple editor):

- Left: the project's blocks, and a field palette to drag from.
- Toolbar: the Library tabs, then Select, Line, Text, Field, Logo; toggles **Sample** (show this
  project's values) and **Snap** (1 mm); **Frame…** (sheet size, margins, zones).
- The block zoomed, fields as dashed boxes.
- Right panel for a field: **Value** (filled from project info, the page or the revision), **Look**
  (label toggle, size, bold, align), **Box**.
- One frame per sheet size. No scripting.

## Settings

- From the project name menu. Same top and bottom bars. Back to Draw (Esc).
- **Project info:** project, people and approval, functions and locations, title block fields,
  revision letters, defaults.
- **Pages and sheets:** each page type (Front sheet, Schematic, Network, Single-line, Panel layout,
  Report) with its sheet, frame, title block, zones, numbering (01–09, 100s, 200s…) and position:
  Front, Body or Back. + Page asks for a type and puts the page where the type belongs. Frames and
  title blocks are made in Library ▸ Title blocks.
- **Standards:** as before.

## Publish workspace

Output sets on the left; every page in the centre to tick and drag; PDF, Printer or DXF settings on
the right. No history here.

## Calculations: where each one lives

| Check | Where | Basis |
|---|---|---|
| Panel wire rating | Wires ▸ Rating | Part datasheet × temperature × bundling |
| Field cable rating and volt drop | Cables ▸ Rating | IEC 60364 / BS 7671 table × ambient × grouping; volt drop from length |
| Load budget | Power ▸ Current | Each source's rating, running, peak and diversity |
| Enclosure heat | Power ▸ Heat | Part dissipation against the enclosure's loss and cooling |
| Quick sums | Ctrl K | Answers inline; results that matter become checks |

All results appear in the Checks tab and the bottom bar counter, with a go-to and a fix.

## Boards on the canvas

Numbered by workspace: 1 Draw, 2 Project, 3 Library, 4 Publish, 5 Settings, 6 Tokens and shared
parts.

| Board | Shows |
|---|---|
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
| 1.15 | Panel layout page, trunking selected |
| 1.16 | Generated cover page |
| 1.17 | Parts list report page |
| 1.18 | Compare revisions, from History |
| 1.19 | Light theme |
| 1.20 | Full dark: sheet follows the theme |
| 2.01 | Project ▸ Devices, with a part set |
| 2.02 | Project ▸ Terminals |
| 2.03 | Project ▸ PLC: signals, PLC sync, tests |
| 2.04 | Project ▸ Network |
| 2.05 | Project ▸ Wires, with conductor rating |
| 2.06 | Project ▸ Cables |
| 2.07 | Project ▸ Power: load budgets |
| 2.08 | Project ▸ Power: enclosure heat |
| 2.09 | Project ▸ Reports |
| 2.10 | Project ▸ Tests: FAT and coverage |
| 3.01 | Library ▸ Symbols |
| 3.02 | Library ▸ New symbol |
| 3.03 | Library ▸ Make symbol from a selection |
| 3.04 | Library ▸ Edit symbol, with what changes |
| 3.05 | Library ▸ Title blocks |
| 4.01 | Publish: pick, order, print settings |
| 5.01 | Settings ▸ Project info |
| 5.02 | Settings ▸ Pages and sheets |
| 5.03 | Settings ▸ Standards |
| 6.01 | Design tokens |

## Suggested build order

1. Shell: top bar, bottom bar with the page well, panel slot, right panel frame and rail. Theme and
   sheet colour tokens.
2. Tool strip with flyouts, both sets, and the Snap, Grid and Ortho toggles.
3. Book and Devices panels with pinning and grouping.
4. Right panel with folding sections, the three tabs; the checks engine and counter.
5. Device placement and the connector maker.
6. Project: one table component for every tab. PLC signal list with export, import and compare
   first.
7. Wires and Cables with Rating; Power current, then heat.
8. Tests generated from the PLC list, results tied to revision, retest flags, FAT sheet.
9. Network topology from ports and cables.
10. Settings ▸ Pages and sheets; Library ▸ Title blocks; generated front pages.
11. Panel layout page.
12. History with branches, then Compare; Publish; radial wheel.
