# Brief 038: the shell (U2)

Part of the [quick start](../quick-start.md). A section number refers to the quick start's own
sections.

## What this brief changed

**Branch `feature/ui-shell` (brief 038)**, cut from `dev` at `73c8b805`. The trial's window is
ECADCraft's shell now. CADCraft's menus, toolbar, Tool Sets, layers, layout tabs and idle command
line are hidden behind a developer switch; nothing was deleted.

The first launch (no saved window) opens at the largest size that fits the monitor, so on a
1920 × 1080 screen the left panel, the page and the open right panel all show; after that the
window comes back at the size and place you left it. A smaller window still collapses the right
panel to its rail first (follow-up to 038).

| Region | What is there |
| --- | --- |
| Top bar | The project menu under the project name: New project, Open project…, Save, Save As…, Close project, Project info, Project settings, Revisions, Settings. Then the workspaces Draw, Lists, Library and Publish; the search field (it types into the symbol search); and the Checks, revision (`Rev A`) and agent pills |
| Agent banner | Under the top bar while an agent is connected: 035's banner, unchanged |
| Left panel | Pages, Devices and History, as the old Project panel's tabs. The page tools (new page, kinds, move up and down, preview, Reports, Print Book, new folder) are at the top of Pages. A page changed by a commit while another page is shown gets a small blue dot until you show it. Drag the inner edge (200–420 px), double-click it for the default; the collapse button leaves a 44 px rail |
| Tool strip | In Draw: Select (V), Wire (W), Place (P), Cable (C), Draw (D), Box (B), Text (T). Hover a button for its flyout of today's tools; a click starts the group's last tool |
| Main view | Draw: the page. Lists: Reports. Library: the symbol search. Publish: Release or Print Book (the switch at its top) |
| Drawer | Checks, under the main view. Its chevron closes it to its header |
| Right panel | The ECAD properties (the section that was in CADCraft's Properties palette) |
| Status bar | Save state, the page shown, the standard, the zone under the pointer, zoom, the MCP state, and Commands |
| Text panel | Ctrl+/ or Commands: the old Developer section (ECAD command field, debugger, Inspect Project, Agent access, `ui.primitives`) |

Keys: Ctrl+1 to Ctrl+4 switch workspace; Ctrl+K opens the symbol search; Ctrl+/ the Text
panel; Alt+Left and Alt+Right go back and forward over pages and workspaces; PgUp and PgDn
step through the pages; Del, Ctrl+Z and Ctrl+Y as before. Over the page, with no text field
active: V W P C D B T start a group's tool (pressed again while that tool runs, the next one),
F fits the page, Esc clears the selection. Typing a command name over the page no longer starts
it: use the Text panel, or the developer switch.

The developer switch: Settings ▸ Developer, `--cad-ui` on the command line, or `ui.cadchrome`
(`on`, `off`) in the Text panel. It shows CADCraft's own interface inside the main view.

The shell remembers its layout and each page's zoom per project (at most 32 projects) in the
trial's own settings, not in the `.ecad` file. A setting that cannot be read starts with the
default and says so once in the command history.

Still floating windows, until their own brief: Sheets and properties popouts, Merge, Renumber,
the page preview.

## Try it

1. `cargo run --release -p ecad-trial` (from `ecad/`), maximised at 1920 × 1080.
2. Click through Draw, Lists, Library and Publish, then Ctrl+1 to Ctrl+4.
3. In Draw, hover each tool-strip button; start Wire from its flyout and draw a wire; press W
   twice over the page.
4. Open the project menu, then Settings; turn the developer switch on and off.
5. Ctrl+/: type `wire {"points":[[0,0],[40,0]]}` and Run.
6. Close and start the trial again: the workspace, panels and page zoom come back.

## Checks

Automated (agent, native Windows 11, 2026-11-13): `cargo test -p ecad-trial` (all tests, the
new `shell_ui` ones included) and `cargo test -p cadcraft-ui-egui --test embedded_view`
(upstream seam US1). Nobody has tried it by hand yet.
