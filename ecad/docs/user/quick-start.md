# Quick start

ECAD is an electrical design program: schematic pages, devices with tags, wires, cables, terminal
strips and the reports that come from them. This page gets you from a fresh checkout to a saved
drawing. The other guides in this folder go into each part; [Where next](#where-next) lists them.

## Install and run

ECAD runs on Windows 11 and on Linux (including WSL with WSLg). You build it from source.

1. Install Git, Rust 1.95 or later from <https://rustup.rs>, and a C toolchain. On Windows that
   is the Visual Studio Build Tools with the "Desktop development with C++" workload; open a new
   PowerShell window afterwards so that `cargo` is found. On Linux it is `build-essential`,
   `pkg-config` and `curl`.
2. Clone the repository and build and start the desktop program from its `ecad` folder:

   ```sh
   git clone https://github.com/convyares-FCSL/ecadcraft.git
   cd ecadcraft/ecad
   cargo run --release -p ecad-desktop
   ```

   The first build takes several minutes and several GB of disk. A Windows release build opens
   no console window and prints nothing; use a debug build (`cargo run -p ecad-desktop -- --help`)
   to see `--help`.

Build options, environments and graphics fall-backs are in [the build guide](../developer/build.md).

## Open a project

- `cargo run --release -p ecad-desktop` starts with a new, unsaved project.
- `cargo run --release -p ecad-desktop -- C:\dev\ecad-projects\demo.ecad` opens a saved one.
- **Project menu ▸ Open project…** (click the project name at the far left of the top bar) opens
  one when no project is open.

To try a filled project first, `cargo xtask demo` writes a small motor control panel to
`C:\dev\ecad-projects\demo.ecad` (`~/ecad-projects/demo.ecad` on Linux). An existing file is
replaced only with `cargo xtask demo --force`.

## The window

- **Top bar:** the project name (its menu: New, Open, Save, Save As, Close, Project info,
  Project settings, Revisions, Settings, Standards), the workspaces **Draw / Project / Library /
  Publish** (Ctrl+1 to Ctrl+4), and search (Ctrl+K) for pages, tags, symbols and actions.
- **Tool strip** (left, in Draw): the **Book**, **Devices** and **History** panels, then the
  tool groups, then **Snap**, **Grid** and **Ortho**.
- **Inspector** (right): **Device**, **Layout** and **Checks**. `]` folds it.
- **Bottom bar:** the save state, the page well (page buttons, ‹ ›, **+**), and the sheet view
  (sheet size, sheet colour, Fit and zoom).

[Workspace](workspace.md) describes each part; [Drawing](drawing.md) the tools, keys and the wheel.

## First drawing

1. **Place a coil.** Hover **Device** on the tool strip and choose **Symbol**. The picker opens at
   the top left of the page. Type `coil` and click **Contactor / relay coil**, then **New device
   (-K01)**. Click on the page to place it; press Esc when done. Press **R** with the coil
   selected to turn it 90°.
2. **Place a contact of the same device.** Start **Symbol** again, type `no` and click **NO
   contact**. The picker offers a new device first, then the devices it can join: click `-K01`.
   Click beside the coil. Both symbols show `-K01`.
3. **Place a second device**, for example a push button (`push`) as a new device `-S01`.
4. **Wire them.** Press **W**. Move onto a connection point of the push button: a ring and its
   name show before you click. Click, click where the wire should bend, then click a connection
   point of the coil. The wire is finished and shows its wire number.
5. **Look.** With no tool running, rest the pointer on the wire or a connection point: a tooltip
   names it and what it is connected to. Hovering changes nothing.
6. **Move and undo.** Move the coil (see [Drawing](drawing.md#move-turn-copy-delete)): the wire
   end follows. Ctrl+Z undoes, Ctrl+Y redoes.
7. **Add a page.** Click **+** in the bottom bar's page well. Click a page button to switch back.

The same steps in typed form, and drawing a device from plain lines instead of a library symbol,
are in [Devices](devices.md#make-a-device-from-a-drawing).

## Save and reopen

1. Press Ctrl+S (or **Project menu ▸ Save**). A new project asks for a file name: type
   `C:\dev\ecad-projects\demo` (`~/ecad-projects/demo` on Linux) and press Enter. `.ecad` is
   added, and that default folder is made by the first save into it.
2. The bottom bar says **Saved**. After an edit it says **Unsaved changes** with a count.
3. **Project menu ▸ Close project.** With unsaved changes it asks: **Save**, **Discard** or
   **Cancel**.
4. Start the program again with the file. The pages, devices, wires and undo history are as you
   left them.

Locks, read-only files and project settings are in [Projects](projects.md).

## Where next

- [Drawing](drawing.md): tools, the wheel, the canvas, keys, the command window.
- [Projects](projects.md) and [Workspace](workspace.md).
- [Devices](devices.md), [Library](library.md), [Wires](wires.md), [Cables](cables.md).
- [References](references.md), [Connectors](connectors.md), [Terminal strips](terminal-strips.md),
  [PLC](plc.md), [Circuits](circuits.md), [Locations](locations.md).
- [Pages](pages.md), [Styles](styles.md), [Properties](properties.md), [Checks](checks.md),
  [Reports](reports.md), [History](history.md), [Releases](releases.md), [Publish](publish.md),
  [Agent access](agent-access.md).

## Reporting a problem

Send what you did and what you saw (a screenshot of the whole window helps), the output of
`git log --oneline -1`, and your operating system and Rust version (`rustc --version`).
