# ECADCraft UI design spec

Nov 12, 2026 · @Carl von AYRES

ECADCraft becomes a app with four workspaces, seven tool groups, a fixed radial wheel, one table component and a paper-coloured sheet. The fourteen screens (a to n) are mocked up in [ECADCraft UI](https://claude.ai/artifact/GttUyoA7zQVVRe78xLMoRB), one tab per screen. This doc holds everything an implementer needs beside them. Questions for you are numbered Q1 to Q12 in the last section.

## What changed in round 2

- **Left is navigation, right is detail.** The left panel has three tabs in every workspace: Pages, Devices and History. The right panel shows details only. The right rail is gone.
- **Seven tool groups with flyouts** replace the seven single tools. Each remembers its last tool.
- **Lists holds the managers as tabs.** Any of them still opens beside the page.
- **Symbols at scale:** a library browser (screen i), New symbol (g), Make symbol from a selection (j), Edit symbol with a review of what changes (k).
- **Terminal manager reads like a terminal plan:** inside, the strip, outside. What you set looks like an input; what is read from the drawing is plain grey text (d).
- **Part sets:** a device carries several order codes, with kits and configured codes (a, l, h).
- **Print sizes are real:** tags 2.5 mm, labels 1.6 mm, terminal labels 1.5 mm. Screens zoom in; nothing is enlarged.
- **Two-colour wires and cores are drawn in both colours.**
- **PLC rack overview** with a card per slot that links to its page (m), and a **quick connector maker** (n).
- **Reports are sheets:** project frame, zones and title block, with a plain report as the alternative (h).
- **Confirmed:** paper sheet, branches as drafts, zones A to F by 1 to 8, references written 2/B5.

## 1. Design principles

1. **Ink prints; overlay never does.** Symbols, wires, text and frame are drawn in ink and are the document. Rings, dots, halos and markers are the app talking, use overlay colours, and never reach the PDF. The user can always tell what the customer will see.
2. **A quiet page is a finished page.** Healthy things carry no marker. Colour appears only where there is something to do: an open end, an error, a provisional number. A finished page looks almost exactly like its print.
3. **One device, many places, shown as one.** A contactor is a coil on one page, contacts on two others, a row in the parts list and later a footprint on the mounting plate. Selecting any of them marks all of them and lists the rest with a go-to.
4. **The click says what it will do, electrically, before you make it.** Wiring mistakes are cheap to prevent and expensive to find. Every tool previews the electrical result ("joins A1 to wire 24V-05"), not only the geometry.
5. **Tools follow the thing under the cursor.** Seven tool groups start new things. Everything you do to an existing thing is on the wheel for that kind of thing, in the same position every time.
6. **A table is another view of the drawing, and just as editable.** Terminals, cables and I/O are faster to edit as rows than as graphics. The row and the object on the page are the same thing, so there is nothing to synchronise.
7. **Standards fill in; the designer decides.** Every tag, colour and style arrives filled in from the standard, shows where it came from, and is overridden in the place it is displayed.
8. **Every change has an author and an undo.** A person's edit and an agent's call are the same command, recorded the same way. An agent session can be read, followed on the page and undone as a unit.

## 2. Workspaces and layout

The app has four workspaces. A workspace changes two things only: the main view and the tool strip. Everything else stays where it is, so nothing has to be relearned when you switch.

| Workspace | Main view | Tool strip | Right panel shows |
| --- | --- | --- | --- |
| Draw | A page | Seven groups: Select, Wire, Place, Cable, Draw, Box, Text. On a panel-layout page (M3): Select, Enclosure, Rail and duct, Footprint, Draw, Dimension, Text | Properties of the selection, or of the page when nothing is selected |
| Lists | The managers as tabs: Devices, Terminals, Cables, Wires, PLC | None. The tab header carries filter, import, export and "Open beside the page" | Properties of the selected row, with its part set |
| Library | The library browser (category tree, filters, preview grid) or the symbol builder | In the builder: Select, Draw, Text, Connection point, Text slot, Origin | Details of the selected symbol or part |
| Publish | Any page as it prints, with the release and export bar above it | None | Report settings |

### Tool groups

A group button shows the last tool used in it. A click uses that tool. Press-and-hold or hover opens the flyout. The group's key picks the last-used tool, and pressing it again steps to the next one.

| Group | Key | Tools | Notes |
| --- | --- | --- | --- |
| Select | V | Select, Lasso |  |
| Wire | W | Wire, Branch, Potential or bus line, Jump |  |
| Place | P | Symbol, Terminal, Connector, Continuation arrow | Opens a panel: the finder for Symbol (screen b), the quick maker for Connector (screen n) |
| Cable | C | Cable slash, Continue cable |  |
| Draw | D | Line, Polyline, Rectangle, Circle, Arc | Plain geometry for symbols and notes. It has no electrical meaning until "Make symbol" |
| Box | B | Location box, Function box, Device or PLC outline | A box carries meaning: a location box sets +CAB1 on what is inside it, and Properties then reads "from box" |
| Text | T | Text, Note, Revision cloud |  |

The right-click wheel still handles everything done to an existing object.

Switch with the segmented control in the top bar or Ctrl+1 to Ctrl+4. A go-to from any row, check or reference also switches for you, and Back (Alt+Left) returns you to where you were.

### Shell regions at 1920 x 1080

| Region | Size | Contents |
| --- | --- | --- |
| Top bar | 44 px high | Project menu, the four workspaces, search (Ctrl+K), Checks, revision badge, agent status. Nothing else |
| Agent banner | 32 px high, only while an agent is connected | Agent name, connected since, last write, Pause writes, Undo session, Disconnect |
| Left panel | 264 px, drags to 420 px, collapses to a 44 px rail | Navigation only, the same in every workspace. Tabs: Pages, Devices (with "Not on a page yet" at the top), History |
| Tool strip | 48 px | Tool groups for the workspace and the page kind |
| Main view | The rest | A page, the managers, the library or the print preview. Splits in two for a companion view |
| Drawer | 150 px open, 36 px collapsed, under the main view | Checks. The same slot holds the "Before saving" review of a symbol edit |
| Right panel | 340 px, 380 px in the builder | Details only: properties and part set, report settings, symbol details, the Standards detail. Never navigation |
| Status bar | 26 px | Save state, page, standard in use, zone under the cursor, grid, zoom, MCP state, Commands button |

### Companion view

The main view can split to show a second view beside the first: a table beside a page in Draw (screen d), or a page beside a table in Lists. It is the same table component and the same page view, so a manager is never a separate window with its own rules. On a second monitor the companion pops out into its own window.

### Why this shape

- **Left is navigation, right is detail.** Pages, Devices and History all answer "take me there", so they share the left panel. Everything about the thing you arrived at is on the right. You never have to wonder which side to look at.
- **Seven groups, not forty buttons.** Tools only start new things. Connect, Disconnect, Jumper, Fix wire number, Cable join and the rest act on an existing thing, so they live on that thing's wheel.
- **Managers are tabs of one view.** Lists gives a manager the whole window. The companion view gives you the same manager beside the page. Both are the one table component.
- **The Library's category tree sits inside the main view.** The left panel is the project's navigation in every workspace, so the library's own tree cannot take its place.
- **Release and export sit above the print preview.** That keeps the left panel as the book and the right panel as the report's settings.
- **Checks and the agent are global.** They matter in every workspace, so they sit in the shell and never move.
- **Settings is a place you visit, not a workspace.** It opens over the window from the project menu, the status bar's "Standard: IEC" or any origin badge, and closes back to where you were.
- **No page tabs, no model and layout tabs.** A page is one thing. The left panel and Back are enough for 10 to 50 pages.

## 3. Where everything lives

Every item from your sections 4 to 6 has one home below. Nothing needs a new region later; each feature fills a slot the shell already has.

### What exists today

| Today | New home |
| --- | --- |
| New, Open, Save, Save As, Close, project status | Project menu under the project name in the top bar. Save state in the status bar |
| Page list, folders, numbering, drag to reorder, preview, page menu | Pages panel (left, Draw mode). Preview appears on hover. Right-click a page for its wheel |
| Revision bar | Revision pill in the top bar; full detail in Publish |
| Developer fold: JSON command field, debugger | Text panel, "Commands" tab |
| Find Symbol, Place Symbol | Place group, Symbol. Its panel finds by search and stays while you place. "Browse the library" opens the Library |
| Convert to Component, Add Connection Point | Wheel on plain drawn graphics ("Make component"). Full editing in the symbol builder |
| Device Properties, Set Property | Inspector, always showing the selection. Or the cell in a list |
| Delete Device, Delete Electrical Object | Wheel, bottom slot. Del key |
| Place Terminals, Strip Accessory, Draw Bridge, Jumper | Terminal tool to place. Terminal wheel for Link to next, Add level, Change type. Terminals list for bulk |
| Move Mirror Table, Location Box | Drag the contact table directly. Box tool |
| Wire, Junction, Connect, Disconnect | Wire tool. Ending on a wire makes the junction. Drag a wire end onto or off a connection point |
| Fix Wire Number | Wire wheel, "Fix number" |
| Renumber, Renumber Wires, Finalise numbering, the Renumber dialog | Devices and Wires lists, filtered to "Provisional", with Accept per row or for all. No dialog |
| Cable Slash, Add to Cable, Cable Join, Continue Cable, Cable Reference, Core Label | Cable tool to start one. Wire wheel "Add to cable". Cable wheel for join, continue and reference. Core labels are generated text |
| Cable View | Cables list |
| Go to Reference, Preview Reference | Wheel, right slot. Hovering any reference previews its target. Back returns |
| CAD command line | Removed. Replaced by the intent line (output only), search and the Text panel |
| Hover tooltips, snap rings | Kept. Hover card after 400 ms. The connection ring grows when a wire end will attach |
| Devices panel | Devices tab in the left panel to navigate. Devices tab in Lists, or as a companion, to edit |
| Checks | Checks drawer and the Checks pill |
| History: transactions, versions, Restore, branches, merge, Merge dialog | History tab in the left panel. The selected change's details show on the right. Merge and compare show in the main view, not a dialog |
| Outputs: PDF book, BOM, wire report, cable report, terminal plan | Publish workspace. Reports are generated sheets of the book, with the project frame and title block, or plain |
| Release: pre-release check, named revision, release folder | Release and export bar above the page in Publish |
| Sheets and properties: size, frame, title block, revisions, Issue | Inspector with nothing selected shows the page. Defaults in Settings, Standards |
| Drafting and Modeling tabs, Hatch, Block, Dimension, Leader, Table, Parametric, Layers, Model and Layout tabs | Not in ECADCraft's shell. Free drawing tools appear only in the symbol builder and on panel-layout pages |

### Being built now (v0.15)

| Feature | Home |
| --- | --- |
| Text styles per kind of text | Settings, Standards, Text styles |
| Show or hide per text at symbol, page or project level | Inspector, "Shown on the page", with a badge for the level. Hidden text shows faintly while its object is selected |
| Tag placement, dragging generated text | On the page. A moved text reads "overridden" in the Inspector with a reset |
| Wire label rule | Standards, Wire categories, label rule column |
| Print quality, missing data visible in print | Publish shows the print. Missing data prints as an amber marker |
| Wires join end to end, or show an open end | Loose-end marker on the page and an information line in Checks |
| Tag display style, revision table columns, pending revision D\* | Standards for the styles. D\* in the revision pill and title block, in the provisional colour |
| About 70 built-in symbols | Symbol finder and Library mode |
| Defaults registry | It is the Standards screen |

### M1

| Feature | Home |
| --- | --- |
| Live MCP bridge, banner, authorship | Agent pill and banner in the shell. Authors in History. Permissions in Settings, Agent access |
| Symbol builder, promote a drawn component | Library workspace. New symbol starts on an empty grid (screen g). "Make symbol" on the wheel of selected lines and text opens the builder with that geometry (screen j). "Edit symbol" on a placed symbol opens its definition (screen k) |
| Multi-level terminals, fuse and disconnect terminals, relabelling | Terminals list: levels are rows, links are lanes in the Link column. Terminal wheel for Add level and Change type |
| PLC racks, cards, channels, addressing, I/O list | PLC tab in Lists: the rack overview with a card per slot, each linking to the page where it is drawn, and the channel table under it (screen m). The overview and the I/O list are also generated sheets |
| Wire categories | Standards, Wire categories. Chosen in the Wire tool's intent line and on the wire wheel |
| Graphical terminal-strip pages | Generated pages in the book |
| Continuation cross-references with captions | Generated text on the page, with hover preview and go-to |
| Connector devices | Place group, Connector: a quick maker for a plug, a socket or a mated pair (screen n). Each half is a device with its own location and part set |
| BOM completeness | Checks warnings and amber cells in the parts list |
| Shell, wheel, one table component | This design |
| Settings, Standards | Screen e |

### M2

| Feature | Home |
| --- | --- |
| .ecadlib libraries and the library viewer | Library workspace: category tree, filters and preview grid in the main view, details on the right (screen i). Every symbol names its source, and a library shows "update available" |
| Parts catalogue | Library, Parts, on the table component, with kits and configured codes. A device's part set is filled from it (screen l) |
| Device, Terminal, Cable and PLC managers | Tabs of the Lists workspace, and the companion view |
| List-driven design, import and re-import | Lists, Import. Imported rows arrive in the table as a proposal marked added, changed or removed. Accept per row or all |
| Report generator | Publish, report definition panel beside any generated page |

### M3 and after

| Feature | Home |
| --- | --- |
| 2D panel layout | A page kind in Draw. The tool strip changes. "Not in panel yet" uses the same tray as "Not on a page yet". Fill warnings go to Checks |
| Revision compare | History: pick two versions, Compare. Clouds on the pages, change list in the panel |
| Label export, XLSX everywhere | Publish, Export. Every table header also has Export |
| Onboarding | Empty states and a sample project. No tour overlay |
| SolidCraft 3D link | "Open in SolidCraft" on a panel-layout page |
| KiCad PCB as a device | A part kind in Library, Parts |
| AI adoption of old PDFs | Lists, Import, with the same proposal review |

## 4. Design tokens

The sheet is paper-coloured by default and the shell around it is dark graphite. Real wire colours (blue, brown, black, green-yellow) only read truthfully on a light ground, and the page then looks like its print. A dark sheet is a second token set, not a second design. Everything below is flat fills, 1 px strokes, rounded rectangles and one shadow style, which egui 0.37 draws natively.

### Shell colours (Graphite)

| Token | Value | Use |
| --- | --- | --- |
| bg | #131416 | Window ground, the desk behind the sheet |
| panel | #1C1D20 | Panels, top bar, status bar |
| raised | #25272B | Fields, buttons, group rows, chips |
| hover | #2D3035 | Hovered row or button |
| active | #353941 | Pressed button, active mode, active side panel |
| line | #32353B | 1 px hairlines between regions and rows |
| line-strong | #464A52 | Popover borders, key caps, badges |
| text | #E8E9EB | Primary text |
| text-2 | #A9ADB5 | Secondary text |
| text-3 | #8A8F98 | Field labels, placeholders (4.6:1 on raised) |
| accent | #2F6BE0 | Primary button, active tool (white text 4.9:1) |
| accent-text | #7FB0FF | Selected outline, focus ring, links |
| accent-soft | accent at 18 % | Selected row |
| ok | #55D68B | Saved, check passed |
| warning | #F2B84B | Warning |
| error | #FF6B5E | Error, delete |
| provisional | #FF7AB8 | Provisional and pending values |
| agent | #B9E34A | Agent presence and agent-made changes |
| cable | #B48CFF | Cable names in lists |
| connect | #45CFD3 | Links and connection points in lists |

### Canvas colours

Each meaning has a colour and a shape, so none relies on hue alone. Markers are drawn at a fixed screen size and do not scale with zoom.

| Meaning | Shape | Paper sheet | Dark sheet | Prints |
| --- | --- | --- | --- | --- |
| Sheet |  | #F6F4EF | #1B1D21 | White |
| Ink: symbols, text, default wire |  | #16191D | #E4E6E9 | Yes |
| Ink 2: zone labels, pin labels, references |  | #5A616B | #9299A3 | Yes |
| Grid | 0.25 mm dot every 5 mm | #D6D2C6 | #30333A | No |
| Connection point | Hollow ring, 8 px | #0A8F94 | #45CFD3 | No |
| Wire end attached | Solid dot, 5 px | #17A05B | #55D68B | No |
| Loose wire end | Hollow square, 7 px | #7C8591 | #8B94A1 | No |
| Cable and cable references | Oblique slash with a dot per core; tag in the same colour | #7A3FE0 | #B48CFF | Yes, in ink |
| Error, orphan reference, refused preview | Filled disc with "!". A refused preview is a red outline with hatching | #D7281E | #FF6B5E | No |
| Warning | Filled triangle with "!" | #A86400 | #F2B84B | Only the missing-data marker |
| Provisional tag or wire number, pending revision | Text in this colour with a dotted underline | #CF1F7A | #FF7AB8 | Yes, in ink |
| Terminal link (jumper) | Dashed bar, 0.6 mm, dash 1.8 / 1.4 | #0A8F94 | #45CFD3 | Yes, in ink |
| Selection | 12 % fill, 0.5 mm outline, four grips. Other placements of the same device get a dashed outline | #1F6FEB | #5B9BFF | No |
| Placement preview | 8 % fill and outline in the selection colour | #1F6FEB | #5B9BFF | No |
| Agent's last write | Dashed outline, 0.6 mm, with a name chip | #5F8F00 | #B9E34A | No |

The connection point and the terminal link share one hue on purpose: both mean "a connection that is not a wire", and a ring cannot be mistaken for a dashed bar. Say if you want them split (Q9).

Default wire category colours, all changeable in Standards: power in ink at 0.6 mm; 24 V DC and 0 V #23408F on paper and #8FB0FF on dark; PE #2F8F3A with #E3B800 dashes on paper, #6FD67A with #F2D34B on dark.

**Two-colour wires and cores are drawn in both colours.** The first colour is the solid line and the second is laid over it as dashes, 2 mm on and 2 mm off. That covers GNYE, and the two-colour cores of multicore cables such as RDBU or YEBN. The same swatch appears beside the colour code in the Terminals and Cables lists. A colour code the app does not know is drawn in ink and listed in Checks as information, with a button to define it in Standards.

### Type

UI font: IBM Plex Sans. Canvas and print font: Barlow Semi Condensed, a narrow sans in the spirit of technical lettering with a true bold. Both are under the SIL Open Font License, so they can be embedded.

| UI style | Size / line | Weight | Use |
| --- | --- | --- | --- |
| Caption | 11 / 14 px | 600 | Section labels, badges, key caps |
| Small | 12 / 16 px | 400 | Tables at density, status bar, hints |
| Body | 13 / 18 px | 400, 500 for emphasis | Everything else |
| Title | 15 / 20 px | 600 | Panel and card titles |
| Object | 20 / 26 px | 600 | The selected object's name, screen titles |

| Sheet text style | Cap height on paper | Font size | Weight |
| --- | --- | --- | --- |
| Tag | 2.5 mm | 3.57 mm | 600 |
| Label: pin label, wire label, reference, continuation | 1.6 mm | 2.29 mm | 400 |
| Terminal label, description | 1.5 mm | 2.15 mm | 400 |
| Title block label | 1.4 mm | 2.0 mm | 400 |
| Zone label | 2.0 mm | 2.86 mm | 400 |
| Page title | 3.5 mm | 5.0 mm | 600 |
| Page number | 4.2 mm | 6.0 mm | 600 |

These are the sizes measured from real packs, taken as cap heights. Barlow Semi Condensed has a cap height of 0.70 em, so the font size is the cap height divided by 0.70. Nothing is enlarged for the screen. At fit, a tag on an A3 page is about 7 px tall on a 1920 px monitor, so a page opens at the zoom where tags reach 13 px (about 135 %) and each page remembers its zoom. Reports use the same sizes.

### Spacing, sizes, radii

| Token | Value |
| --- | --- |
| Spacing steps | 2, 4, 6, 8, 12, 16, 24 px |
| Panel padding | 12 px |
| Control heights | 24 px small, 28 px default, 32 px search field |
| Row heights | 28 px list row, 30 px table row, 32 px group row |
| Bars | 44 px top bar, 36 px panel header, 32 px agent banner, 26 px status bar |
| Tool button | 36 px square in a 48 px strip, 20 px icon |
| Radii | 4 px chips and key caps, 5 px fields, 6 px buttons, 8 px tools and tiles, 10 px popovers and cards, full for pills |
| Wheel | 150 px outer radius, 54 px inner, eight 41-degree slots with 4-degree gaps |
| Shadow | Popovers and the sheet only: 0 8 24 px black at 45 % |

### Line weights

| Line | Weight |
| --- | --- |
| UI hairline | 1 px |
| UI icon stroke | 1.5 px on a 16 px grid, round caps and joins |
| Focus ring | 2 px, accent-text |
| Symbol outline | 0.35 mm |
| Control wire | 0.4 mm |
| Power wire, PE | 0.6 mm |
| Frame | 0.5 mm |
| Title block and table rules | 0.2 mm |
| Mechanical link | 0.25 mm, dash 1.2 / 1 |
| Cable slash | 0.5 mm |

Canvas lines are in millimetres and scale with zoom, with a floor of 1 px on screen.

Icons are drawn in code from lines, arcs and circles only. Tool icons echo the schematic: a wire is a line between two end rings, a terminal is a ring on a line, a cable is a slash across three lines.

## 5. Interaction rules

### Mouse

- **Hover** outlines the object in the selection colour. After 400 ms a small card names it, gives one or two key facts and says where else it is drawn. The card has no buttons.
- **Click** selects. Shift-click adds. Dragging on empty paper box-selects. Esc or a click on empty paper clears.
- **Selecting any placement of a device** marks its other placements with a dashed outline and lists them in the Inspector.
- **Drag** moves. Wires stay attached and re-route at right angles. Drag a wire end onto a connection point to connect it (the ring grows when it will attach), or off it to disconnect.
- **Drag from a connection point** starts a wire without choosing the Wire tool.
- **Double-click text** edits it in place: tag, description, wire number. Double-click a reference to go to its target.
- **Drag a row** from a list or from "Not on a page yet" onto a page to place that device.
- **Middle-drag** pans. The scroll wheel zooms about the cursor. Double middle-click fits the page.
- **Right-click** opens the wheel.

### The radial wheel

- It opens centred on the cursor. The object stays visible through the 54 px centre, and a chip above the wheel names it.
- Two ways to use it, with the same result: press, move and release in one gesture, or click to open and click a slot.
- Eight slots, fixed per kind of object: device, wire, wire end, terminal, cable, text, reference, empty page, page row, mixed selection.
- Positions mean the same thing on every wheel. Top and top right: the two things you most often do to that kind. Right: go to what it relates to. Bottom right: show its row. Bottom: delete. The three on the left: copy, turn or change it.
- A kind with fewer than eight actions leaves slots empty. Nothing shifts to fill a gap.
- A slot may open one short list of up to eight choices, such as wire categories. There is never a second wheel.
- A slot that cannot run stays in place, greyed. Hovering it says why and offers the next step.
- Releasing outside the wheel cancels.

### Keyboard that remains

| Key | Does |
| --- | --- |
| V, W, P, C, D, B, T | Select, Wire, Place, Cable, Draw, Box, Text. The key picks the group's last-used tool. Pressing it again steps to the next tool in the group |
| Esc | Back to Select. Also ends the current tool, closes the wheel or a flyout, and cancels an edit |
| R, X | Rotate, mirror the thing being placed or the selection. On a connector, X swaps the halves |
| Tab | Next variant while placing. Next cell in a table |
| F2, Enter | Edit the selected tag or cell. Enter accepts |
| Del | Delete |
| Ctrl+Z, Ctrl+Y | Undo, redo |
| Ctrl+C, Ctrl+V, Ctrl+D | Copy, paste, duplicate with the next free tag |
| Ctrl+K | Search pages, tags, symbols and actions |
| Ctrl+1 to Ctrl+4 | Draw, Lists, Library, Publish |
| Alt+Left, Alt+Right | Back and forward through where you have been |
| PgUp, PgDn | Previous and next page |
| F | Fit the page |
| Ctrl+/ | Open or close the Commands panel |

Single-letter keys work only while the pointer is over the main view and no text is being edited. Nothing in the app can be done only from the keyboard.

### The optional Text panel

It opens over the right panel from the Commands button in the status bar, or Ctrl+/, and is closed by default. It has two tabs. **Commands** takes the exact command names and JSON that MCP uses, with completion, and replaces today's Developer fold. **Ask** takes plain words and shows the command it resolved to before running it. Both run the same commands as the wheel and the tools, so nothing is reachable only from here.

### Refusals and errors

- A refusal is never a modal dialog. It appears where the attempt was made: at the cursor while a tool is active (the preview turns to a red hatched outline with a label), in the intent line, or as a message at the bottom of the main view for actions made elsewhere.
- Every refusal has three parts: what was refused, why in the designer's terms, and one or two buttons that move forward. Example: "Can't release D. Protective earth of -M01 is not connected." with **Show it** and **Release as draft**.
- If no fix can be offered, the button is **Show it**. A refusal with no button is a defect.
- Problems in the drawing are not refusals. They go to Checks, each with a fix button and a go-to, and stay until fixed. Checks never stop you drawing. Errors block only a release, and the release box says which.
- A programmatic call gets the same refusal as data: a code, the reason and the suggested next call. Nothing opens on screen except the entry in History.

### Provisional and pending

- One rule: **dotted means not final yet**. Dashed means a link or a relation.
- A provisional tag or wire number is drawn in the provisional colour with a dotted underline on the page, in lists and in the Inspector. Accept one from its wheel ("Fix number"), or filter a list to "Provisional" and accept them all.
- A pending revision shows as D\* in the same colour in the title block and the revision pill until it is released.
- A proposal (a list import, or an agent set to ask first) shows its rows marked added, changed or removed, and its objects ghosted on the page. Accept or reject per row or all at once.
- There is no unsaved state. Saving is continuous and the status bar shows the time of the last save.

### How "live" is shown without refresh buttons

- No refresh, update or retag control exists anywhere, so there is nothing to wonder about.
- When an edit changes something elsewhere, that place acknowledges it. The changed cell or text is tinted for 1.2 seconds, and counts in pills and tabs tick over.
- A page changed while you were elsewhere carries a dot in the Pages panel until you visit it.
- Generated pages sit under "Generated · always current". They have no stale state to show.
- In a companion view, selecting a row or its object draws a leader line between the two for one second (screen d).
- Work that takes real time, such as a 50-page PDF, shows a thin progress line on the item itself while everything else stays usable.

### Symbols: finding, new, made from a selection, edited

- **Finding.** The Place panel finds by search and pins recent and favourite symbols. The Library browser is for everything else: a collapsible category tree, filters for poles, contact type, standard, source and maker, and a large preview grid. Hovering a tile shows its connection points and text slots. Every tile names its source: built-in, project, or the .ecadlib it came from.
- **New symbol.** Library, New symbol opens an empty 2.5 mm grid at print size. A symbol cannot be placed until it has a name, at least one connection point and a tag slot. The right panel lists what is missing, with a button for each.
- **Make symbol from a selection.** Select plain lines, arcs and text on a page, then choose Make symbol on the wheel. The builder opens with that geometry moved onto the grid. Free line ends are offered as connection points and texts as slots. Offers are drawn dashed and nothing is applied until you accept it. On saving, you choose whether the drawing on the page is replaced by the new symbol.
- **Edit symbol.** Edit symbol on the wheel of a placed symbol opens its definition. Editing a built-in or library symbol makes a project copy that remembers its origin; the library is never changed silently.
- **Before an edit is saved**, the drawer lists every placed symbol that uses it, with a now and after preview and a line on what changes there. You choose: update all, update selected, or save as a new symbol.
- **A placed symbol that is not updated** keeps the old version and reads "update available" in Properties and in Devices, with Update and Compare.

### Part sets

- A device has a part set: an ordered list of lines, each with a maker, an order code and a quantity. The first line is the main part. It gives the device its ratings and the contacts it offers. Accessories can add contacts and coils.
- Lines come from the parts catalogue as single parts, as kits that insert several lines at once, or as a configured code, which is one line whose order code carries its options. Kit lines stay separate and editable, and carry the kit's name.
- Any device's set can be saved as a kit.
- A line with no order code raises a check with a "Choose from catalogue" fix. It is never left as a blank.
- The parts list report shows a device's lines under the device, or flattened so that each order code appears once with its total quantity and its tags. That is a report setting.

### What you set and what is read

- In every manager, a value you set there looks like an input. A value read from the drawing is plain grey text with no box, and clicking it goes to where it is set.
- The Terminal manager is laid out as a terminal plan: inside on the left, the strip drawn down the middle, outside on the right. You set the label, type and part, levels, links and accessories, and the strip's tag, description, part set and rail. Targets, wires, cables, cores and pages are read.
- The strip is drawn with its end clamps and end plates, stepped levels, PE marked in both its colours, and links as lanes beside the terminals.
- The PLC tab opens on the rack: one card per slot showing its type, address range and a square per channel (in use, free, provisional address, has a check). Each card links to the page where it is drawn, or offers "Make its page". The channel table of the selected card sits underneath.
- The connector maker in the Place group asks five things: which halves (plug, socket, both, socket in a panel), the type, how many pins, how they are named, and whether to draw all pins in a row or pin by pin. Each half is a device with its own tag, location and part set. The preview shows which wires the click will cut.

### Reports on the sheet

- Every report is a sheet of the book. It uses the project's frame, zones and title block, has a page number, continues onto further sheets when it is long, and keeps clear of the title block.
- "Plain report" shows the same table without the frame, for a standalone PDF. CSV and XLSX come from the same definition.
- This follows the pack you attached, where the parts lists, terminal plans, cable lists, contents and revisions are all sheets in one numbered book.

## 6. Migration path

Build the tokens, the shell and the table first, in that order. After those three, every later feature fills an existing slot and no panel is styled twice.

### What changes first

1. **Tokens and primitives.** One theme module holding the token table from section 4, the two fonts embedded, and nine primitives: button, icon button, pill, chip, badge, field, segmented control, key cap, row. Icons become painter functions. No layout changes yet. From here on, nothing is styled locally.
2. **The shell.** Top bar, mode switch, left panel slot, tool strip slot, main view, Checks drawer slot, side panel slot with its rail, status bar. Move today's panels into the slots with their contents unchanged: Pages, Devices and History to the left panel's tabs, Properties to the right panel. Hide the CAD menus, ribbons, layers, model and layout tabs and the command line behind a developer switch instead of deleting them.
3. **The one table component.** Wrap egui\_extras' table with sort, filter, group, multi-select, inline edit, go-to, the row-to-object link and the change tint. Convert Checks first because it is read-mostly, then Devices. Every later manager is a column set plus a data source.
4. **The canvas.** Paper sheet, the ink and overlay split, the markers, selection with related placements. Land the v0.15 text styles against the Standards registry here, so each style is defined once.
5. **Wheel, intent line and the seven tools.** One command registry with a slot table per kind of object. When the wheel covers a group of the old ECAD tool-set buttons, remove that group.
6. **Inspector with origin badges, then Settings and Standards.** Both read the same registry. Standards is the table component with a detail panel.
7. **Agent banner and authors in History**, with the M1 bridge.

### What each later feature slots into

| Feature | Slots into | New UI it needs |
| --- | --- | --- |
| Symbol builder | Library mode: main view, tool strip, Inspector | The builder's eight tools |
| Multi-level terminals | Terminals list | The link-lane cell |
| PLC racks and cards | PLC list, Symbol tool | An address cell editor |
| Wire categories | A Standards catalogue, the wire wheel's list | None |
| Terminal-strip pages, I/O list | "Generated" in Pages, Publish | Page templates |
| .ecadlib and parts | Library left panel, the table | A viewer grid |
| Device, Terminal, Cable, PLC managers | Lists mode | Column sets |
| List import | The table's proposal state | Added, changed and removed row states |
| Report generator | Publish side panel | The definition panel |
| 2D panel layout | A page kind in Draw | A tool set and footprint drawing |
| Revision compare | History and the main view | The cloud overlay |
| Label and XLSX export | The Export button on every table and in Publish | None |

### The rule that keeps it clean

A feature may add a column set, a page kind, a wheel table, a Standards catalogue or a tool. It may not add a panel, a dialog or a toolbar. If a feature seems to need one, the shell is missing a slot and that is the thing to fix.

## 7. Push-back

Eleven places where I would change the plan, most important first.

1. **Branches and merge are the biggest piece of bloat in today's UI.** A lone designer with 10 to 50 pages does not think in branch graphs. Keep the engine. Show versions, Restore and Compare. Call a branch a draft, and show the picker and merge only once a second draft exists (Q5).
2. **"Live" must not mean tags and page numbers move on their own.** If inserting a contactor renumbers the ones after it, or a longer terminal plan pushes every later page down, printed references go wrong silently. Rule: the app never changes a tag that was typed by hand or has been released, and automatic numbering only fills in new things. Generated pages number after the schematics, so a schematic page number never moves by itself. That is what "fixed" and "provisional" should mean, and it is why the Renumber dialog can go.
3. **"Nobody types" is right for commands and wrong for names.** In a 50-page project, typing -K07 is the fastest way to get there. Keep one search box (Ctrl+K) for pages, tags, symbols and actions. It is search, not a command line, and nothing depends on it.
4. **"One way to do each thing" should mean one command and one editor, not one door.** The wheel, a key, search and MCP all open the same command, and a table is the only editor of its data. A wheel-only app would shut out trackpads and anyone who cannot make a right-button gesture.
5. **Do not force eight slots.** Fixed and not configurable: yes. But positions that mean the same thing on every wheel matter more than filling all eight, so a kind with five actions shows five. Allow one short list from a slot, never a second wheel.
6. **Keep the modes to four and make them shallow.** The Blender comparison is about tools matching the task, not about the number of modes. Checks, History and Settings as modes would hide things the user needs while drawing.
7. **Give the agent a second setting.** Watching it draw live is the right default, with one-click "Undo session". Some users will want "ask first", where the agent's calls arrive as a proposal to accept. That is the same review UI as list import, so it costs little (Q6).
8. **Five levels of standards is one too many to show.** The chain is fine in the engine. On screen, the page level will be set rarely and forgotten easily. Show a level only when it holds a value, and consider limiting page-level overrides to function, location and sheet (Q7).
9. **Do not build a chat inside the app yet.** With the MCP bridge the conversation already lives in the agent's own app. Build the Commands tab, and make Ask a search over commands until a model ships with the product (Q3).
10. **Ship named reports, not just a generator.** Source, columns, filter, sort and group is the right engine, and no form designer is the right call. Most users should still never open the definition panel: parts list, terminal plan, cable list, wire list and I/O list should exist in every new project.
11. **Take the general CAD tools out of the shell completely.** Hatch, dimension, parametric constraints and layers one click away is what makes it feel like a CAD platform with an electrical plug-in. Plain geometry stays, as the Draw group you asked for in round 2, because symbols and notes need it. Layers stop being a user concept; visibility is set per kind of text.

Two things I would keep exactly as you wrote them: programmatic calls never open dialogs, and a table is the editor of its data. I would go further on dialogs for people too. The whole app needs only the system file picker and a confirmation before a release.

### Round 2: what I would still watch

1. **Undo, redo and Back are now keys only.** Your top bar list left them out, so they are gone from it. That is fine for regular users and a risk for new ones (Q8).
2. **History is cramped at 264 px.** It reads comfortably at about 400 px, so the left panel remembers a width per tab.
3. **The left panel costs the Library two tiles a row** at 1920 px. Collapsing it gives them back. The Library could open with it collapsed.
4. **Real print sizes make "fit" unreadable.** A tag is about 7 px tall at fit. Fit stays for layout; a page should open at a readable zoom instead.
5. **Stepping through a group with its key is easy to learn and slow for the fourth tool** (Q7).
6. **A title block made for drawings has fields a report does not use.** On a report sheet I show the report's kind and arrangement where function and location would be.
7. **Two devices for a mated connector doubles its tags and parts lines.** That is right when the halves sit in different locations. A plug on its own stays one device.

## 8. Questions for you

Round 2 settled four of the first ten: the paper sheet is the default, branches are shown as drafts, zones are rows A to F by columns 1 to 8 with references written 2/B5, and the sheet font is Barlow Semi Condensed. Twelve remain or are new. Each has the assumption I designed to, so nothing is blocked if you agree.

1. **Q1. Do wire and core colours print?** I assumed print is black with the colour code in the label, and colour on screen only. If they print, the two-colour dashes print too.
2. **Q2. Will a model ship inside the app, or is it agents over MCP only?** I assumed MCP only, so the Commands panel is not a chat.
3. **Q3. Does the agent write live or ask first by default?** You kept the banner and per-session undo, so I left it live, with ask-first as a setting.
4. **Q4. Is the page level needed for every default**, or only for function, location and sheet?
5. **Q5. Connection point and terminal link share one hue.** Keep, or give the link its own colour?
6. **Q6. Which contactor letter is your IEC default?** IEC 81346-2 gives Q. I show K as a project override.
7. **Q7. One key per tool group, stepping through it, or a key per tool?** I assumed one per group: V, W, P, C, D, B, T.
8. **Q8. Should undo, redo and Back have buttons anywhere?** I assumed keys only, with History for anything further back.
9. **Q9. Are 2.5, 1.6 and 1.5 mm cap heights or font sizes?** I took them as cap heights, which makes the font sizes 3.57, 2.29 and 2.15 mm.
10. **Q10. When a symbol is edited, which choice is pre-selected?** I pre-select "update all", and switch to "update selected" when any placed symbol would clash.
11. **Q11. How do you name connector halves?** I used -X10.S for the socket and -X10.P for the plug, as a Standards setting.
12. **Q12. What does a plain report carry, and does your article number belong on a part line?** I assumed a one-line header (project, revision, date), and an article number field that is off in the report until you add the column.

## What I have not checked

The mockups are drawn to the sizes in this doc and every file is checked for well-formed markup, but I have not seen them rendered and have not tried any of this in egui. The contrast figures quoted are calculated, not measured on screen. Parts, makers and order codes in the mockups are invented.
