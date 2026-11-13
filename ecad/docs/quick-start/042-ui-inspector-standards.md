# 042: the inspector, Settings and Standards (U4c)

What brief 042 changed, and how to try it by clicking. Folded into `docs/quick-start.md` at the
phase-end tidy-up.

## What changed

- **Settings is a place you visit.** It covers the whole window, with its sections on the left:
  **Standards**, **Project**, **Text styles**, **Display**, **Agent access** and **Developer**.
  Esc or the close button (top right) returns to where you were: the same workspace, page and
  selection. Open it from the project menu (**Settings**, **Project info**, **Project
  settings**), from the status bar's **Standard: IEC (built in)**, or from any origin badge in
  the right panel.
- **Project** has two tabs, **Info** and **Settings**: the old Project info and Project settings
  popouts, contents unchanged. **Text styles** is the old Text section of Project settings. OK
  writes the changes and closes Settings; Cancel or Esc writes nothing.
- **Agent access** (035) moved from the Commands panel's Developer section to Settings ▸ Agent
  access. The Commands panel no longer shows it.
- **Standards** lists every default: setting, group, value, where the value comes from (a badge:
  Built-in, Standard, Project, Page, Object, Symbol) and its key. Filter with the search field,
  group by Group or From, or show only **Overridden** rows. Click a row: on the right is its chain,
  built-in → standard → project → page → object, with the level in use marked **In use**. A level
  is listed only when it holds a value or the default allows it (text heights have no page level).
  Type a new project value: before anything is written, a line says what changes (for a tag
  prefix: "2 devices use -K; fixed and released tags keep their names; new devices get -Q").
  **Apply** writes it (one transaction, undoable), **Reset** removes the project's value. The
  standard is IEC, built in, and read only; named standards (IEC, US) come later. A second tab,
  **Text styles**, lists each kind of text with its height, weight, font and whether it is shown.
- **The inspector** (right panel). Under the properties, **DEFAULTS** lists the values from the
  registry that apply to what is selected: the page (nothing selected), a symbol or its tag text,
  or a wire. Each has its origin badge; click it to open Standards at that default. A value set at
  this level is a field or a list (for example whether this symbol's tag is shown), with **Reset**
  when it is set here; a value read from further out is grey and clicking it opens Standards.
  With a device placed several times, **Select all N placements** selects them.
- **`setdefault`** is the command behind all of this: `setdefault {"key": "symbol.coil.prefix",
  "value": "Q"}` (project level), `{"key": "text.tag.show", "level": "page", "value": "off"}`,
  `{"key": "text.tag.show", "level": "object", "representation": "<id>", "value": "first"}`, or
  `{"key": …, "reset": true}`. A level the default does not allow is refused with the levels it
  does. It writes the same stored values the registry reads (no new op, no new file field).

## Try it

1. Run the trial with a project, for example
   `cargo run --release -p ecad-trial -- C:\dev\ecad-projects\<project>.ecad`, or the demo project
   built in `docs/quick-start.md`.
2. **Owner check.** Click **Standard: IEC (built in)** in the status bar. Search `coil`, click
   **Tag prefix: Contactor / relay coil**. Type `Q` in **Project value** and read the line under
   it before clicking **Apply**. Apply: the row's badge says Project and the chain marks Project in
   use. Place a new coil: it gets `-Q…`; the old ones keep their tags. Click **Reset**: back to
   Built-in.
3. **Inspector.** Close Settings (Esc). Select a symbol's tag text on the page. Under the
   properties, **Tag height** shows 2.5 mm, grey, with a Built-in badge; click the badge: Standards
   opens at `text.tag.height`. Esc: the same symbol is still selected. Pick **Off** in **Tag
   shown**: the tag disappears from that symbol only, the badge says Symbol, and **Reset** brings it
   back.
4. **Live.** Open Settings ▸ Standards, then type `textstyle {"kind":"tag","height":3}` in the
   Commands panel (Ctrl+/): the Tag height row changes to 3 mm and is tinted for a moment.
5. **Agent access.** Settings ▸ Agent access: the mode, port and status as before.

## Checks run (worker, not owner-tried)

`cargo test -p ecad-cadcraft lane_042` (setdefault over the whole registry, refusals, chain,
users), `cargo test -p ecad-mcp lane_042` (MCP author and writes off), `cargo test -p ecad-trial
lane_042` (Settings opening and Esc, Standards, inspector, update map), and the fast tests of the
three crates.
