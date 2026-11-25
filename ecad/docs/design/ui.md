# The user interface: why it is shaped this way

ECAD's window is built around one idea: the drawing comes first, and everything else is a view of the same
project that never gets in its way. This note gives the rules the interface follows and the reasons for them.
How each screen is used is in the user guides ([`../user/workspace.md`](../user/workspace.md),
[`../user/drawing.md`](../user/drawing.md)); the value controls are in
[`../developer/ui-components.md`](../developer/ui-components.md); sizes and colours are in
[`../reference/tokens.md`](../reference/tokens.md).

## Principles

- **The drawing comes first.** Nothing permanent sits over the sheet. Panels open beside it, or float over it
  only until you start drawing.
- **One bar at each edge.** The top bar is for the project and the app; the bottom bar is for status, pages and
  the view. Nothing is stacked into two bars.
- **Draw is the only workspace with the tool strip.** Project, Library and Publish are project-wide or
  external, so they use the full width.
- **Every titled section in a side panel folds**, and a folded section shows a one-line summary of what is set.
- **Grey text is read from the drawing; an input box is something you set here.** A read value never looks
  like an input, and clicking it goes to where it is set.
- **One name for one thing.** A tagged item (`-K01`, `-X10`, `-M01`) is a **device** everywhere: the Devices
  panel, the Device tool, the Device tab, Project ▸ Devices. Words come from the glossary
  ([`../reference/glossary.md`](../reference/glossary.md)).
- **Little text.** An on/off is a toggle with a short name; detail goes in a tooltip.
- **Calculations are checks, not separate tools.** A result shows where the item is, with the basis it used and
  a fix, in the Checks tab and the bottom bar's counter.
- **Ink prints; overlay never does.** Symbols, wires, text and the frame are the document. Rings, dots,
  outlines and markers are the app talking: they use overlay colours and never reach a PDF or a print.
- **A quiet page is a finished page.** Healthy things carry no marker; colour appears only where there is
  something to do (an open end, an error, a provisional number).
- **Standards fill in; the designer decides.** A value arrives filled in from the standard, shows where it came
  from, and is overridden where it is shown.
- **Every change has an author and an undo.** A person's edit and an agent's call are the same command,
  recorded the same way (see [`agent.md`](agent.md)).

## No new panels, screens or dialogs

A feature may add a column set, a page type, a wheel table, a Standards catalogue or a tool. It may not add a
panel, a dialog or a toolbar. If a feature seems to need one, the shell is missing a slot, and that is what to
fix.

Why: every feature that brought its own window would bring its own rules, its own way of selecting and its own
way of being out of date. The shell's slots, the one table and the wheel are where every feature lives, so a
person learns them once.

## The shell

The layout is pure arithmetic from the window's size and the shell state to a rectangle per slot
(`ecad-app` `shell/layout.rs`), so every size rule is a plain unit test. At 1920 × 1080:

| Slot | Size | Holds |
| --- | --- | --- |
| Top bar | 40 px | Project name and menu, the workspace switch (Draw, Project, Library, Publish), search (Ctrl+K), the agent pill, the theme switch, help |
| Agent banner | 32 px, only while an agent is connected | See [`agent.md`](agent.md) |
| Tool strip | 56 px, Draw and the symbol builder only | Panels, tool groups, Snap / Grid / Ortho |
| Panel slot | 248 px (Book, Devices), 280 px (History) | One panel at a time, beside the strip |
| Canvas | the rest | The sheet, fitted with frame and title block visible |
| Right panel | 320 px in Draw, wider in the other workspaces; a 56 px rail when closed | The selection |
| Bottom bar | 32 px | Status at the left, the page well in the centre, the view at the right |

- **Pin or let it close.** Each panel header has a pin. A pinned panel docks and the drawing moves over for it;
  one panel is pinned at a time. An unpinned panel floats with a shadow and closes when you start drawing.
- **The right panel is details only, never navigation.** Its rail has one labelled button per tab (Device,
  Layout, Checks); Checks carries a dot when there are problems. While a symbol is being placed, the panel
  describes the device about to be placed and what the click changes.
- **The sheet colour** (Paper, Follow, Dark) changes the screen only. PDFs and prints always use paper colours.
- **Settings is a place you visit, not a workspace.** It opens from the project menu over the window, keeps the
  same top and bottom bars, and Esc goes back.
- **Workspaces change only the main view and the tool strip.** Everything else stays where it is, so nothing is
  relearned on switching (Ctrl+1 to Ctrl+4). A go-to from a row, a check or a reference switches for you, and
  Back (Alt+Left) returns.
- **The window's own state is not the project's.** Which workspace and panels are open, and each page's zoom,
  are kept in the app's own storage per user, never in the `.ecad` file: a view is how one person looks at a
  project, not part of it.

ECAD draws the whole window except the drawing area itself. CADCraft draws the canvas (rendering, picking,
snapping, grips, zoom and pan, a running command's rubber band) into the rectangle ECAD gives it, and keeps its
own prompts that protect data. This takes one generic upstream seam (the embedded drawing view, with switches
for CADCraft's chrome, idle typing, canvas menus and style) and a font definitions builder; both are listed in
`ecad/UPSTREAM-PATCHES.md`. With CADCraft's chrome switched on (the developer switch) its full interface
appears inside the main view; it is never on by default.

## Tools start new things; the wheel acts on existing things

The tool strip, top to bottom: the panels (Book, Devices, History); then the tool groups Select, Draw, Text |
Device, Terminal, PLC | Wire, Cable | Location; at the foot, Snap, Grid and Ortho. A group starts its
last-used tool on a click and opens its flyout on hover. A group with a key (V, D, T, W, C) picks its
last-used tool, and pressing the key again steps to the next. A tool that has no command yet stays in place,
greyed, with its hover saying why.

Everything done to an existing object is on its **wheel**: a right-click on the page while no tool runs opens
the wheel for the kind of thing under the cursor. Slots mean the same on every wheel: bottom deletes, right goes
to what the object relates to, bottom-right shows its row, the left side copies, turns or changes it. A slot
that cannot run stays in place, greyed; an empty slot stays empty, so nothing shifts. While a tool runs, a
right-click is Enter.

Why: a small number of groups that only start things, and one wheel per kind of thing, replace forty buttons.
Fixed positions let the hand learn the wheel.

## One command registry, many ways in

A command (in the engine's registry) says **what** happens; an **action** says where a person finds it: in a
tool group's flyout, in a wheel slot, on a key, and under which words Ctrl+K finds it. The action table is data
(`ecad-app` `commands/actions.rs`). A test checks that every action names a registered command, and that every
command a person can reach appears in an action or in an explicit list of commands that are for programs only.

The tool strip, the wheel, keys, Ctrl+K, the command line, MCP and the control channel all run the same
commands, so a person and an agent get the same result and the same refusal. MCP exposes commands, not
actions. See [`interaction.md`](interaction.md) for how an interactive tool commits.

## One table for every list

Every manager (Devices, Terminals, PLC, Wires, Cables, Reports, the Checks list) is a column set and a data
source on one table component (`ecad-widgets` `table/`), never a panel of its own. The same table appears full
width in the Project workspace and, with **Open beside the page**, beside the page in Draw.

- **A row and its object are the same thing.** Rows are keyed by the object's ID, so selection, sorting and the
  change tint survive updates, and selecting a row selects the object on the page and back. There is nothing to
  synchronise.
- **The table never runs a command.** It reports what happened (selected, go-to, an edited cell), and its owner
  runs the same command the page would run. An inline edit is the same transaction, and gets the same refusal,
  as on the page.
- **It never holds its own copy of a value.** After a commit the source names the keys that changed, and the
  table re-reads those cells.
- **Rows are virtualised**, so a frame lays out only the visible rows; sort, filter, group, multi-select,
  inline edit, go-to and CSV export are the table's, not each manager's.

Why: tables are faster than graphics for terminal strips, cables and I/O, but only if they are another view of
the drawing and just as editable. One component means one way to select, sort, edit and export.

## Live, without refresh buttons

No refresh, update or retag control exists anywhere. Every view keeps an update map: it is built whole only on
first show, on reload, or when the change feed has rolled over, and otherwise patches only what a commit
touched. Where an edit changes something elsewhere, that place acknowledges it: the changed cell is tinted for
1.2 seconds and fades, and counts in pills and tabs tick over. An idle window repaints nothing.

Why: a refresh button is a confession that the view can be wrong. If every view follows the change feed, there
is never anything to wonder about. When a view keeps an update map at all is decided by its cost (see
[`views.md`](views.md) and [`decisions.md`](decisions.md)).

## Refusals and checks

- A refusal is never a modal dialog. It appears where the attempt was made: in the intent line at the top of
  the canvas while a tool runs, or beside the field or row that was edited.
- A refusal says what was refused, why in the designer's terms, and offers a way forward where one exists.
- Problems in the drawing are not refusals. They go to Checks, each with a go-to and a fix where one exists,
  and stay until fixed. Checks never stop you drawing; errors block only a release.
- A program gets the same refusal as data.

## Fonts

The interface font is IBM Plex Sans; the sheet font is Barlow Semi Condensed. Both are embedded in the
binary, so they are always present and nothing is read from disk at run time. The files live in
`ecad/assets/fonts/`, unmodified, with their OFL-1.1 licences, their sources and checksums in the folder's
README and rows in `ATTRIBUTION.md`; a test checks every file against its checksum. `ecad-widgets` embeds the
interface faces and `ecad-model`'s palette the sheet faces. Text sizes on the sheet are cap heights, not font
sizes.

## Performance targets

Measured on a release build at 1920 × 1080 on the medium bench project:

- an idle frame over a page within 8 ms (p95 within 16 ms), the shell's own share within 2 ms whatever the
  project's size;
- an edit shows in every open view in the same frame as its commit, with view work in proportion to what the
  commit touched;
- scrolling a 10 000-row table within 3 ms per frame for the table body;
- no repaints while nothing changes.

## Where the interface is going

The target layout also has Network, Power and Tests tabs in the Project workspace, a panel layout page type
with its own tool strip, and the Library's Snippets and Parts tabs filled in; where these are not built yet
the shell shows them disabled with the reason, or not at all. The order and the releases are in
[`../roadmap.md`](../roadmap.md).
