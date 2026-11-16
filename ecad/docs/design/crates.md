# Design 065: the crate map

Status: Approved (owner, 2026-11-15). Design only; no code moves in this lane. The owner's
answers to the five calls are in §7; §1.1 (drawing style and UI tokens) follows from call 1.
Branch: `docs/crates`, from `origin/dev` at `445787b8`.
Runs: the split lane comes after lanes 060–063 merge and before the R9 views worker starts.

The goal: every ECAD crate has one fixed meaning, the words for the layers become fixed
vocabulary, and `cargo xtask layers` keeps them true.

All facts below come from `cargo metadata`, `grep` and reading the module headers at `445787b8`.
No builds were run. Line counts include each file's unit tests.

---

## 0. What the code says (read this first)

Five facts change the proposed map. Each one is a decision in §7.

1. **ecad-cadcraft has no UI.** It depends on no egui crate and no `ecad-ui`; `grep egui` finds
   nothing in its `src/`. The shell, panels, dialogs, lists and tabs all live in
   **`apps/ecad-trial`** (44,500 lines, about 100 files). So `ecad-app` comes out of ecad-trial, not out
   of ecad-cadcraft.
2. **ecad-cadcraft is one tangle.** 71 of its 75 top-level modules form a single dependency cycle.
   Only `file`, `slash`, `lib` and `preview` are outside it. Domain modules call engine-side
   helpers: `strips` calls `interact::terminal_name`, `commands::core_label` and
   `terminal_cmds::terminal_words`, and `checks` calls `look::colour_code` and
   `cable_cmds::planning`. So ecad-electrical cannot be split off one module at a time. First
   the cuts in §3 are made inside ecad-cadcraft, then the electrical set moves in one step.
3. **The engine draws with ecad-tokens.** `look`, `text_style`, `wires`, `continuations`, `styles`
   and `defaults::text` read canvas colours, dash lengths and the embedded sheet font from
   `ecad-tokens`. If ecad-tokens sits in the UI kit layer, below the engine, the engine cannot use
   it. Owner, 2026-11-15: split it. The drawing style goes in Core and the screen tokens stay with
   the UI kit (§1.1).
4. **ecad-inspect is part of the product.** `Project` keeps an `ecad_inspect::EventLog` (runtime
   diagnostics), and `ecad-mcp`'s `ecad_inspect` tool returns `ecad_inspect::snapshot`. A product
   crate may not depend on a crate under `dev/`. So ecad-inspect cannot move to `dev/`, and it
   cannot merge with ecad-inspector, which pulls in a WebSocket server (`tungstenite`).
5. **The window depends on the MCP program.** `ecad-trial/src/agent` uses `ecad_mcp::{gate,
   policy, tools, bridge::ALLOWED_METHODS, not_offered}`, the write gate shared by the window
   and headless agents. App→app dependencies are allowed (owner ruling), but once ecad-app is a
   library in the App layer, this edge points from App down to Programs.

Also: `ecad-bench` is in `crates/`, not `dev/`. The apps and the bench depend on
`cadcraft-engine` directly. ecad-trial has 50 references in 19 files, mostly `geom::Vec`,
`doc::Handle`, `Bounds`, `Session`, `View` and `Input`. ecad-bench has 28 (`Session`) and
ecad-mcp has 5 (`Session`, `find_command`). This is finding 9 of the code review, the boundary
leak.

---

## 1. The map

Six layers, top to bottom. **A crate uses only crates in layers above it** (and crates in its own
layer where §4 says so). Engine and UI kit are **siblings**: neither uses the other.

| Layer | One-line meaning (fixed vocabulary) | Crates |
|---|---|---|
| **Core** | What an ECAD project *is* and what it *means*, including how its drawing is defined, without a screen or a drawing engine. | `ecad-model`, `ecad-store`, `ecad-file`, `ecad-electrical` (new), `ecad-inspect`, `ecad-drawing-style` (new, from ecad-tokens) |
| **Engine** | The link to CADCraft: draws pages from the model, turns edits into transactions, runs commands. | `ecad-cadcraft` |
| **UI kit** | ECAD's widgets and screen look: how a value, button, table or panel looks. Knows nothing about projects. | `ecad-ui`, `ecad-tokens` (screen tokens only) |
| **App** | The ECAD application: shell, workspaces, panels, dialogs, lists, tabs, on a CADCraft window. | `ecad-app` (new, from ecad-trial) |
| **Programs** | Things a person or agent starts: one `main` each, thin. | `ecad-desktop` (ecad-trial's binary), `ecad-cli`, `ecad-mcp` |
| **Dev** | Developer and test tools under `ecad/dev/`. Never shipped. | `ecad-bench`, `ecad-inspector`, `xtask` |

**ECAD is desktop-only (owner ruling, 2026-11-15).** There is no web or browser product, so no
layer has to build for wasm. The `cargo xtask wasm` check of the ECAD crates existed only because
of CADCraft's "never break wasm" rule. It goes in the split lane (§5, step 10). CADCraft's own
wasm build, and any `ECAD-SEAM` in CADCraft files, are not affected.

Inside Core, the order is: `ecad-model`, `ecad-drawing-style`, `ecad-store`, `ecad-file`,
`ecad-inspect`, then `ecad-electrical` (owner, 2026-11-15, split lane step 4). A Core crate depends
only on Core crates before it in this order; `cargo xtask layers` refuses the rest, so a cycle
such as `ecad-file` → `ecad-electrical` cannot be written.

- `ecad-model`: the document, ops, ledger, merge, numbering, derived nets and references.
- `ecad-store`: storage under the file: pages of bytes, the lock.
- `ecad-file`: the `.ecad` container, read and write.
- `ecad-electrical`: the electrical rules: devices, wires, cables, strips, checks, ratings,
  reports, cross-references, styles and defaults. It reads a `Document` and returns data or ops.
- `ecad-inspect`: read-only snapshots and the runtime event log.
- `ecad-drawing-style`: how a page is drawn and printed: sheet colours, line weights, dashes,
  sheet text sizes, the sheet font, canvas markers.
- `ecad-tokens` (UI kit): how the screen looks: shell colours, spacing, radii, shadows, icons,
  UI type sizes.

`ecad-electrical` depends on `ecad-model`, `ecad-drawing-style` and `ecad-file`, and makes no OS
calls itself. The file edge exists because a starter symbol's definition carries its content hash
(`ecad_file::definition_hash`, the canonical JSON of the file format; owner, 2026-11-15, option A).
Moving that hash into `ecad-model` is listed in `docs/deferred-work.md`.

### 1.1 Drawing style and UI tokens (owner call 1)

Today `ecad-tokens` holds both kinds of token: the screen's look and the drawing's look. The owner
split them on 2026-11-15: the drawing style is part of the drawing definition, and the screen
tokens belong with the UI kit.

| Module today | Goes to | What it holds |
|---|---|---|
| `sheet` (`Sheet`, `Meaning`, `PAPER`, `DARK`, `Color`) | drawing style | Print and canvas colours, including the selection and agent-write marks. |
| `sheet_font` | drawing style | The embedded sheet font (family and bytes). |
| `sheet_type` | drawing style | Sheet text cap heights and weights (mm). |
| `wire` | drawing style | Wire category colours, the two-colour dash. |
| `marker` | drawing style | Canvas marker shapes and screen sizes. |
| `line`, `MM` half (`SYMBOL_MM`, `CONTROL_WIRE_MM`, `POWER_WIRE_MM`, `FRAME_MM`, `RULE_MM`, link, slash, selection and agent-write weights and dashes) | drawing style | Line weights on the page. |
| `line`, `PX` half (`HAIRLINE_PX`, `ICON_STROKE_PX`, `FOCUS_RING_PX`, `SCREEN_FLOOR_PX`) | `ecad-tokens` | Screen strokes. |
| `shell`, `space`, `size`, `radius`, `shadow`, `icon`, `ui_type`, `wheel` | `ecad-tokens` | The screen's look. |

**Core, not Engine.** The drawing style is not used by the drawing engine alone:
- ecad-electrical names the sheet font (`defaults::text::SHEET_FONT`) and measures text widths
  for its rules (cut 4).
- ecad-ui embeds the sheet font and shows sheet colours in its gallery (`fonts.rs`,
  `showcase.rs`).
- ecad-app paints the canvas and the agent-write marks with `sheet::PAPER` and `sheet::DARK`
  (`canvas/mod.rs`, `agent/last_write.rs`).

The UI kit is a sibling of the Engine and may not use it. Only a Core crate can be reached from
all four.

**A new crate, not a module.** `ecad-drawing-style` is plain `const` data plus the font bytes,
with no dependencies. A module in `ecad-model` would put the font bytes into every crate that
links the model (`ecad-cli`, `ecad-file`'s tools). The shared `Color` type moves with it, and
`ecad-tokens` uses it from there.

**Cost: small and mechanical, one step of the split lane (§5, step 1b).**
- ecad-cadcraft has 13 files naming `ecad_tokens`, about 21 drawing-token paths in 8 files. Its
  one UI token, `Icon` in `actions`, goes to ecad-app with that module.
- ecad-ui uses the drawing tokens in 2 files, and ecad-app in a few (through `ecad_ui::tokens::sheet`).
- The doc-sync test (`ecad-tokens/tests/all/doc_sync.rs`) reads `docs/design/tokens.md`. Its
  sheet, sheet text and line-weight tables move to a test in the new crate, and `tokens.md`
  keeps one file with the two sections apart.

**Overlap with lane 066 (defaults as data, `docs/design/066-defaults-data.md` on
`docs/defaults-data`).** Some of these constants are user-facing drawing defaults: line weights,
wire colours and sheet text heights. They may become TOML data in 066's defaults sets, not
tokens. 066 already defers line weights to plot styles (§2.3 there). When a value moves to data,
it leaves `ecad-drawing-style` for the defaults set in ecad-electrical. The crate then keeps only
what no user sets: the font bytes, canvas marker sizes and the selection and agent-write colours.
The split lane moves the constants as they are; whichever lane lands second rebases its paths.

---

## 2. Every module and where it goes

Key: **E** = `ecad-electrical`, **C** = stays in `ecad-cadcraft`, **A** = `ecad-app`,
**F** = `ecad-file`, **split** = the module is cut in two (see §3). The column "engine" counts
lines naming `cadcraft_engine` or `cadcraft_io` outside tests. No ecad-cadcraft module uses egui.
"Cut" names what has to leave or be passed in before the module can move. "Clean" means its only
crate edges already point at other modules going to E.

### 2.1 ecad-cadcraft (85,400 lines)

| Module | Lines | Engine | Target | Why | Cut to move |
|---|---:|---:|---|---|---|
| `access` | 331 | 7 | C | The lock between the project and the CADCraft session. | — |
| `accessories` | 1,063 | 0 | E | Strip accessories and links: rules. | `interact::position_label` moves to E (`terminal_types`). |
| `accessory_cmds` | 561 | 3 | C | Commands. | — |
| `actions` | 1,115 | 0 (a doc link) | A | Where a person finds each command: menus, icons, search. That is App content. | `ObjectKind` (used by `interact::kind`) moves to E; the table moves to ecad-app. |
| `agent_access` | 250 | 2 | C (`agent`) | Agent-access preference and port. Used by the window and by `ecad-mcp`. | Joins the agent gate (§3, cut 12). |
| `bom` | 1,239 | 4 | split | Part lines and article rules are E. `partline`, `bomitem` and `article` are commands, in C. | Commands go to `bom_cmds`; `interact::bomitem` stays C. |
| `cable_cmds` | 1,251 | 4 | split | Commands, but they also hold the cable planning model (`planning`, `is_drawn`, `Side`, `CableField`, `value_checks`, `gland`, `shield`, `is_hidden`) that `checks`, `outputs` and `styles` read. | The planning model moves to E (`cables`). The commands stay in C. |
| `checks` | 2,299 | 1 | E | The Checks list: pure rules over the document. | `handles_of` (engine handles) moves to C. Pure helpers from `look` move to E (cut 3). `cable_cmds` items move to E (cut 2). `front::check_rows` moves to E (cut 9). |
| `commands` | 1,661 | 30 | C | The command registry and the JSON forms. | `core_label` and `plain_style` move to E (cut 1). |
| `connector_cmds` | 765 | 4 | C | Commands. | — |
| `connectors` | 1,413 | 0 | E | Connector devices: mating, pins, halves. | `terminal_cmds::{plan_land, Land}` move to E (cut 2). |
| `continuation_cmds` | 78 | 2 | C | Command. | — |
| `continuations` | 728 | 3 | split | Groups and names are E. The arrow entities and ink colour are drawing, C. | `text_style` split (cut 4), `look::rgb` and `projection_sheet` stay C. |
| `convert` | 263 | 7 | C | ECAD items ↔ CADCraft entities. | — |
| `debug` | 1,384 | 1 | C (`dev`) | Debugger copies under one lock. Belongs in the crate's `dev` module (design 029). | — |
| `default_cmds` | 290 | 2 | C | Command `setdefault`. | — |
| `defaults/` (14 files) | 2,837 | 0 | E | The defaults registry. | Clean, once the drawing style is in Core (§1.1). `default_cmds` and `text_style` helpers move first (cut 4). |
| `designation_cmds` | 308 | 6 | C | Commands. | — |
| `designations` | 477 | 0 | E | The project's functions and locations list. | Helper functions now in `designation_cmds` and `location_cmds` move to E (cut 2). |
| `device_cmds` | 368 | 8 | C | Commands. | — |
| `device_list` | 441 | 1 | split | The Devices tab rows are E. "Go to" selects engine handles, C. | `Handle` stays C. |
| `devices` | 1,637 | 3 | split | Tags, prefixes, properties and starters are E. Text placement and colour index are C. | `text_style` split (cut 4). `project::Project` becomes `&Document`. `look::framed` moves to E. |
| `file` | 426 | 0 | split | Binding a project to its `.ecad` file, plus path helpers. It uses only ecad-file and ecad-model, but it reads the home folder and canonicalizes paths (§3.1). | `FileStatus`, `Binding`, `normalize_with` (pure, given the home folder) and the messages go to F (`ecad_file::binding`). `same_path` goes to ecad-store. `home` stays C. C re-exports all of them. |
| `frame` | 435 | 6 | C | Draws the sheet frame. | `fit` and `touches` (geometry over the model) move to E for `title_block::checks`. |
| `front` | 987 | 3 | split | The cover's content (legends, notes, check rows) is E. Drawing it is C. | Cut 9. |
| `generated` | 1,055 | 2 | split | Generated-page kinds, sheets and continuation checks are E. Drawing is C. | `convert::kind_for_primitive` and `frame::touches` stay C. |
| `history` | 1,766 | 1 | C | The History tab's graph over `Project` and branches. | Later: the graph model could move to Core; not in this lane. |
| `interact/` (15 files) | 6,684 | 49 | C | Click forms, marks, hover and capture over the engine session. | `terminal_name` and `position_label` move to E (cut 1). |
| `lanes` | 23 | 1 | C | Lane command registrations. | — |
| `level_cmds` | 369 | 3 | C | Commands. | `is_fuse_device` moves to E (cut 2). |
| `link_lines` | 171 | 0 | E | Which terminals a link line goes through. | `project::pages_touched` goes when R9 lands (064 §9). Until then it is passed in. `interact::position_label` moves to E. |
| `location_cmds` | 313 | 7 | C | Commands. | — |
| `locations` | 968 | 14 | split | Location and function rules and boxes are E. Label entities and line weights are C. | `text_style` split (cut 4), `look::projection_sheet` stays C. |
| `look` | 1,546 | 12 | split | The cable fan drawing is C. The words and codes (`colour_code`, `conductor_colour`, `shorten`, `dash_budget`, `MAX_CODE_CHARS`, `MAX_CORE_CHARS`, `is_frame_rep`, `FRAME_DERIVED`) are E. | Cut 3. |
| `merging` | 824 | 3 | C | `merge` and `undomerge` glue over `ecad_model::merge`. | — |
| `numbering_cmds` | 815 | 7 | C | Commands. The numbering rules are already in `ecad_model::numbering`. | — |
| `oops` | 114 | 1 | C | OOPS on pages. | — |
| `output_cmds` | 443 | 9 | C | Commands. | — |
| `outputs` | 3,250 | 0 | E | Report models: BOM, wire report and cable report, as tables. | `cable_cmds` planning model (cut 2) and `interact::terminal_name` (cut 1). |
| `page_types` | 567 | 4 | split | Folder starts and checks rows are E. The commands are C. | Commands go to `page_type_cmds`. |
| `plc/` (12 files) | 7,327 | 41 | split | The model (`mod`'s `Plc`, `channels`, `rack`, `address`, `iolist`, `signals`, `tagtable`, `overview`) is E. Commands (inside `mod`, `channels`, `rack`, `exchange`) are C. Drawing (`look`, `page`, `sheets`) is C. | Commands go to `plc_cmds/`. `commands::with_project` and `project::Project` become `&Document`. |
| `preview` | 162 | 5 | C | Renders a page into an image. | — |
| `printbook` | 475 | 10 | C | Print Book on CADCraft's plot path. | — |
| `project` | 3,289 | 28 | C | The CADCraft document owner. R9 splits it into files (064 §8). | `now_ms` moves to a clock passed in. `HandleMap` and `Mapped` stay C. |
| `proposals` | 579 | 0 | E | Proposed context when placing or moving. | Reads through `&Document` instead of `access` and `project` (cut 6). `interact` helpers move (cut 1). |
| `props` | 1,145 | 1 | C | The Properties palette for an engine selection. | Later: the row building could move to E. |
| `rating/` (5 files) | 2,754 | 0 | E | Ib ≤ In ≤ Iz and volt drop. | Clean. |
| `rating_cmds` | 431 | 5 | C | Commands. | — |
| `references` | 1,111 | 0 | split | Cable-reference derivation (`references_of`, `core_chains`, `page_number`, `page_words`) is E. Markers on the page are C. | `look::marker_box`, `interact::{OUT_OF_RANGE, MAX_COORD}`, `commands::plain_style` and `access::Pages` stay C (cut 7). |
| `release` | 1,510 | 3 | C | Drives the check, the print book and the stamped folder. | — |
| `reps` | 317 | 0 | E | Representation types and relations. | Clean. |
| `revisions` | 532 | 0 | E | Revision fields. | `project::now_ms` becomes a time argument. `sheet_cmds::folder_path` moves to E. |
| `sheet_cmds` | 1,297 | 13 | C | Commands. | `folder_path` moves to E. |
| `slash` | 466 | 0 | E | What a cable slash crosses, and cable words. | Clean (a leaf today). |
| `strip_page` | 1,040 | 2 | C | Draws the graphical strip pages. | — |
| `strips` | 1,827 | 0 | E | The terminal plan and core counts. | Cut 1 and cut 2 (`terminal_words`, `is_fuse_device`, `core_label`, `terminal_name`, `MAX_CORE_CHARS`). |
| `style_cmds` | 641 | 6 | C | Commands. | — |
| `styles/` (9 files) | 2,225 | 2 | E | Style catalogues. | `colour_rgb` (returns an engine `Rgb`) moves to C. `cable_cmds::is_hidden` moves to E. |
| `symbol_cmds` | 1,391 | 9 | C | Commands. | — |
| `symbols` | 1,977 | 0 | E | Starter symbols and definitions. | Clean. |
| `terminal_cmds` | 1,162 | 6 | C | Commands. | `plan_land`, `Land` and `terminal_words` move to E. |
| `terminal_types` | 869 | 0 | E | Multi-level terminal catalogue. | Clean. |
| `text_cmds` | 745 | 9 | C | Commands. | `kind_words` moves to E. |
| `text_style` | 1,385 | 54 | split | Text kinds, resolved heights, own styles, run rules and `parse_at` are E. CADCraft `TextStyle`, entities, fonts and `register_sheet_font` are C. | Cut 4, the largest. |
| `title_block/` (6 files) | 1,233 | 9 | split | `bind`, `checks` and `house` are E. `layout` and `dxf` (`cadcraft_io::read_dxf`) are C. | `title_block_cmds` and `style_cmds` edges are cut. `frame::fit` moves to E. |
| `title_block_cmds` | 276 | 4 | C | Commands. | — |
| `translate`, `translate_oracle` | 1,629 | 4 | C | CADCraft commit → transaction. | — |
| `verify` | 406 | 1 | C | Does a rebuilt page show the document? | — |
| `views` | 391 | 0 | C | Per-user fan or traditional setting, read by the projection. | `cable_title` (read by `checks`, `rating`, `props` and `styles`) moves to E. |
| `wire_cmds` | 912 | 15 | C | Commands. | — |
| `wires` | 1,727 | 11 | split | Conductor labels, net words and route vertices are E. Label entities, dashes and junction drawing are C. | `translate::ShownOnce`, `verify` and `project::HandleMap` stay C. `text_style` split (cut 4). |
| `xref` | 2,321 | 1 | split | Parse, classify, places and table are E. Drawing (`HAlign`, `VAlign`) is C. | `project::Mapped` stays C. `text_style` split (cut 4). |
| `xref_cmds` | 211 | 7 | C | Commands. | — |

**Since step 11:** `commands` is `cmd/mod.rs` (the registry and shared helpers), each `<area>_cmds` is
`cmd/<area>` (`wire_cmds` → `cmd::wire`) and `plc_cmds/` is `cmd/plc/`. The old paths that other crates
use stay as re-exports in `lib.rs`. `release`'s two commands are `cmd/release`; the `run_*` commands in
`history` and `merging` stay there for now (`docs/deferred-work.md`).

**Size.** E receives about 35,000–40,000 lines, C keeps about 45,000, and about 1,100 go to
ecad-app (`actions`).

**Commands stay in C** for this split (decision 5). The rule of each command (which ops it makes)
moves down to E later, command by command, when R6 types its parameters. Then `*_cmds` keep only
the JSON form, the `CommandSpec` and the `Session` glue.

### 2.2 ecad-trial → ecad-app and ecad-desktop

ecad-trial already has a library (`lib.rs` and about 100 modules) and a 189-line `main.rs`.

| Modules | Target | Why | Cut |
|---|---|---|---|
| `main.rs`, `window.rs` (window size and place), `startup.rs` (start-up timing), argument parsing (now in `lib.rs`, code review finding 9) | `ecad-desktop` | The program: parse arguments, open the window, run. | Code review finding 9 splits `Trial`'s `args` out of `lib.rs`. That happens as part of this move. |
| `shell/`, `canvas/`, `lists/`, `library/`, `settings/`, the panels (`project_panel`, `devices_panel`, `checks_panel`, `history_panel`), the dialogs (`merge_dialog`, `renumber_dialog`), `sheets_ui`, `text_ui`, `outputs_ui`, `release_ui`, `publish`, `search`, `tools`, `preview`, the rest of `lib.rs` (`Trial`, renamed `App`) | `ecad-app` | Everything a person sees and clicks. | `cadcraft_engine::…` paths become `ecad_cadcraft::cad::…` (cut 11). |
| `debugger.rs`, `developer.rs` | `ecad-app`, `dev` module | Developer panels, behind the existing `dev` feature (design 029). | — |
| `agent/` (banner, undo session, last write) | `ecad-app` | The window's side of agent access. | `ecad_mcp::{gate, policy, tools}` become `ecad_cadcraft::agent::…` (cut 12). |
| `tests/all` (harness tests) | `ecad-app` | They drive the library, not the binary. | — |

Nothing in ecad-trial is electrical domain logic that belongs in E. The closest are the row
models in `lists/devices.rs`, `lists/checks.rs` and `shell/right/details.rs`, and the matching
in `search.rs`. They are view models over `ecad_cadcraft` data, which is App's job. If a second
program ever needs the same rows, they move then.

### 2.3 ecad-mcp, ecad-bench, ecad-inspect, ecad-cli

| Crate / module | Target | Why | Cut |
|---|---|---|---|
| `ecad-mcp`: `gate`, `policy`, `tools` (and `agent_name`, `not_offered`, the limits in `lib.rs`, `bridge::ALLOWED_METHODS`) | `ecad_cadcraft::agent` | The write gate is shared by the window and headless agents. It is headless and uses only `Session` and `ProjectHandle`. | Cut 12. ecad-mcp and ecad-app both use it from the engine. |
| `ecad-mcp`: `lib.rs` (server), `bridge` (TCP client), `launch`, `args`, `demo`, `main` | Programs | The MCP program. | Uses `cadcraft-mcp` (the protocol host) and `ecad_cadcraft::cad::Session`. |
| `ecad-bench` (6,237 lines, in `crates/`) | Dev, `dev/ecad-bench` | A benchmark, never shipped. | A folder move. Dev may use `cadcraft-engine` directly. |
| `ecad-inspect` (324 lines) | Core (no move) | The product uses it: `Project`'s event log and the MCP `ecad_inspect` tool. | — |
| `ecad-cli` (70 lines) | Programs (no move) | `ecad` on the command line. | — |

---

## 3. Doesn't fit: modules that straddle layers, and the cuts

Each cut is a small change *inside ecad-cadcraft*. Behaviour stays the same, and each cut is one
commit that builds green. After all of them, the E set in §2.1 has no `crate::` edge to a C
module and no `cadcraft_*` import. That is the condition for moving it.

1. **Helpers living in the wrong module.** `interact::terminal_name`, `interact::position_label`,
   `commands::core_label` and `commands::plain_style` are pure text over the document. Move them to
   E (`devices` or `strips`, `terminal_types`) and keep a re-export in C, so callers don't change.
2. **Rules living in command modules.** The cable planning model in `cable_cmds`,
   `terminal_cmds::{plan_land, Land, terminal_words}`, `level_cmds::is_fuse_device`,
   `sheet_cmds::folder_path`, `text_cmds::kind_words`, and the helpers in `designation_cmds` and
   `location_cmds`. Move each to its E module (`cables`, `connectors`, `strips`, `revisions`,
   `text`, `designations`). The `*_cmds` module calls it.
3. **`look` is half words, half drawing.** Colour codes, `shorten`, `dash_budget`, the max-length
   constants and `is_frame_rep` go to an E module `words` (or `cables::look_words`). The fan
   drawing stays in C.
4. **`text_style` is half rules, half CADCraft text.** This is the largest cut. The E side is
   `TextKind`, `Look` without colour, `OwnStyle`, `RunRule`, `text_box`, `width_in` (via the
   sheet font metrics in `ecad-drawing-style`), `parse_at`, `candidates`, `symbol_text_keys`, `rename_symbol_key` and the
   style resolution. The C side is building `doc::TextStyle` and text entities, `Fonts`, and
   `register_sheet_font`. `devices`, `locations`, `wires`, `xref` and `continuations` depend on
   this cut. Do it first, in two commits: move the types, then move the functions.
5. **Drawing inside domain modules.** `devices`, `locations`, `wires`, `xref`, `continuations`,
   `generated`, `front` and `references` build entities next to their rules. The rules go to E,
   and the entity builders stay in C in a `draw/<name>.rs` module, one per domain module. The
   projection (`project`, `verify`, `translate`) calls `draw/*`.
6. **Domain code reading through `Project`.** `devices`, `plc`, `proposals` and `xref` take
   `&Project` or `ProjectHandle` and read only `doc` and the ledger. Change the signature to
   `&Document` (and `&Ledger` where needed). The C caller passes `&p.doc`. `HandleMap` and
   `Mapped` stay C. Where a domain function also needs handles, it returns model IDs, and C maps
   them to handles.
7. **`references`.** Derivation goes to E. `markers_on_page` and the flag drawing go to
   `draw/references.rs`.
8. **Clock.** `project::now_ms` (chrono) is called by `revisions` and `views`. E takes a time
   argument instead, so its rules stay deterministic and testable. Only C and the App read the
   clock.
9. **`front`.** `check_rows` and the legend contents go to E. Drawing the cover stays in C.
10. **`title_block` and `frame`.** `bind`, `checks`, `house`, `frame::fit` and `frame::touches`
    go to E. `layout` and `dxf` stay in C.
    **Fonts (owner, 2026-11-15, split lane step 4, option F2).** `FrameInput::of` builds the
    frame's text looks, whose bold lineweight exists only when CADCraft does not find the font on
    this machine, and `title_block::checks` and `checks` (E) build frame inputs. So font knowledge
    reaches E through one trait, `TextMetrics` in ecad-electrical (is a family found; the width of
    a text), implemented by ecad-cadcraft over CADCraft's fonts. `Fonts`, `Look`, `width_in` and
    `FrameText` move to E and take `&dyn TextMetrics`; so do the rules that lay out or check text
    (frame, title block checks, device text placement, Checks). ecad-cadcraft keeps wrappers with
    today's signatures, so callers and tests do not change. E does no font lookup and no OS call
    (the Core OS ratchet stays at 0), and E's own tests use a fixed-width test `TextMetrics`.
11. **The boundary leak.** ecad-cadcraft gets `pub mod cad`, a short list of re-exports:
    `Vec`, `Bounds`, `Handle`, `Space`, `Session`, `View`, `Input`, `entity_bounds`,
    `extent_bounds` and `find_command` (what the apps and the MCP use today). ecad-app and
    ecad-mcp then drop `cadcraft-engine` from their `Cargo.toml`. A new engine type the App needs
    is added to `cad` on purpose, in review.
12. **The agent gate.** `ecad-mcp`'s `gate`, `policy` and `tools`, the shared limits and words
    in its `lib.rs`, `bridge::ALLOWED_METHODS`, and ecad-cadcraft's `agent_access` move together
    into `ecad_cadcraft::agent`, beside the commands (owner, 2026-11-15). `ecad-mcp` stays its own
    program and keeps the server, the bridge client and launching. ecad-app stops depending on
    ecad-mcp. The permission check belongs with the commands it guards, so when the command rules
    move down to ecad-electrical (with R6), it moves with them. Only the `Session` glue stays in
    C.
13. **`actions`.** `ObjectKind` goes to E (`interact::kind` uses it). The action table goes to
    ecad-app.

---

### 3.1 OS and file-system access in Core (owner rule, 2026-11-15)

Wasm is gone, but the discipline it gave stays. **Core crates do no file-system or OS access
directly. Only `ecad-store` touches the disk.** The clock, randomness, the process id,
environment variables and config switches are passed in by the caller, from Engine, App or a
Program. Threads are allowed (rayon, or `std::thread::scope` until 061 replaces it), because
they compute and touch nothing outside the process.

What breaks the rule today. The counts are lines outside `#[cfg(test)]`; `ecad-store`,
`ecad-inspect` and `ecad-tokens` are not counted.

**ecad-model: 1.**

| Where | What | Cut |
|---|---|---|
| `dev.rs:22` | Reads `ECAD_VERIFY_INVARIANTS` from the environment. | A switch passed in: `ecad_model::dev::set_verify_invariants(bool)`, an atomic set once by the program at start-up, which reads the variable itself (App, Programs, test harness). The feature gate stays as it is. |

**ecad-file: 10.** The choice: **ecad-file works on handles and bytes that ecad-store supplies**.
The v2 container's file handling does not move into ecad-store. ecad-store must not learn the
`.ecad` format, and it already has almost every call needed: `DiskFile::open` and `create_new`,
`replace`, `remove_file`, `sync_dir`, the lock, `FileIdentity` and `temp_path`. ecad-file keeps
the format (records, chunks, redb schema, verify, convert) and calls ecad-store for every disk
operation.

| Where | What | Cut |
|---|---|---|
| `disk.rs:169-170` (`holder_record`) | `SystemTime::now`, `process::id` | The lock holder record is ecad-store's business. It moves to `ecad_store::disk::holder_record(writer, now_secs)`, the process id is read there, and the time is passed in. |
| `disk.rs:228`, `v2/disk.rs:52` (`same_file`) | `fs::canonicalize` | `ecad_store::disk::same_path(a, b)`, built on `FileIdentity`. |
| `v2/convert.rs:96` (`sniff`) | `File::open` | `ecad_store::disk::read_head(path, n)`. |
| `v2/convert.rs:141` | `fs::read` | `ecad_store::disk::read_all(path, cap)`. |
| `v2/convert.rs:205` | `fs::metadata` | `ecad_store::disk::len(path)`. |
| `v2/schema.rs:94` (`BranchId::random`) | `SystemTime::now` (and `RandomState`, which reads OS entropy) | `BranchId::from_seed(u128)`. The caller passes a seed made from its clock and entropy. C already has `session_seed`. |
| `v2/store.rs:126` (`Store::create`) | `OpenOptions::create_new` | `ecad_store::disk::DiskFile::create_new(path)` plus a new `into_file()`, then redb's `FileBackend` on that handle. |
| `v2/store.rs:143` | `fs::remove_file` | `ecad_store::disk::remove_file`. |

The guard does not count redb's own path-based `Builder::open` and `open_read_only` in
`v2/store.rs:153, 165`, but they are OS access too. The worker moves them onto a store-supplied
handle (`create_with_backend(FileBackend::new(file))`) where redb 4.5 allows it. Any call that
must stay path-based is listed by name as an exception in the guard, with the reason.

**ecad-cadcraft.** A broad grep finds 68 lines, but 20 are false matches (`refs::` matching
`fs::`, and `ProjectFile::` calls). The 48 real lines are all in modules that **stay in Engine**:
the engine and the commands may touch the disk.

| Module (lines) | What | Stays in C / cut before E |
|---|---|---|
| `release` (17) | Writes the stamped release folder: staging, manifest, rename. | Stays C. |
| `views` (7) | The per-user settings file and its folder (environment). | Stays C. `cable_title` moves to E (pure; already in §2.1). |
| `agent_access` (4) | The agent preference file. | Stays C (`agent`). |
| `debug` (4), `translate_oracle` (4) | Debugger file reads; the oracle's log path from the environment, and abort. | Stay C (dev features). |
| `rating_cmds` (3) | Reads and writes the rating dataset CSV. | Stays C. The parse and `to_csv` are already in `rating/dataset` (E, no I/O). |
| `plc/exchange` (2) | `read_file` and `write_file` for tag import and export. | Cut: `export_rows`, `read_tags`, `plan_fill` and the formats go to E (bytes in, bytes out). `read_file`, `write_file`, `run_plcimport` and `run_plcexport` go to C (`plc_cmds/exchange`). |
| `title_block/dxf` (2) | Reads the DXF file. | Stays C (already C in §2.1). |
| `project` (2) | `now_ms` (clock) and the process id for `session_seed`. | Stays C. It is the clock and seed source that E receives (cut 8). |
| `file` (2) | `env::home_dir`, `fs::canonicalize`. | Split (§2.1): `home` stays C; `same_path` goes to ecad-store. |
| `history` (1) | The local time-zone offset (`chrono::Local`). | Stays C. |

**ecad-electrical starts at zero.** No module in the E set of §2.1 makes a direct OS call today.
Their only indirect reach is the clock through `project::now_ms` (cut 8) and the `plc/exchange`
file helpers (cut above). Step 5's closure check also refuses these patterns in the E set.

**Ratchet.** Lane 060 adds a count-can-only-fall guard with today's numbers: ecad-model 1 and
ecad-file 10. The split lane brings both to **0** (step 1c). When it creates ecad-electrical
(step 6), it registers the crate at **0**, and `ecad-drawing-style` and `ecad-inspect` at 0 too.
At the end of the lane (step 10) the ratchet becomes the §4 rule: zero for every Core crate except
ecad-store, with no allowlist beyond named redb exceptions, if any.

### 3.2 v1 removal (step 12, owner 2026-11-15)

Pre-v1 there is no old-format compatibility: the redb container (`ecad-file/src/v2/`) is the only
`.ecad` format kept. Survey at `df5c1e72`, before anything is deleted. **Status: stopped at the
survey**, because removing v1 reaches past the agreed knock-ons (marked **beyond** below). **Done
by lane 069** (`docs/briefs/069-one-format.md`), with the owner's calls on the items beyond. The
golden v1 file was also used by `tester_025_xref_probe`, `tester_026_numbering` and
`tester_031_text`; they moved to the golden redb project with `tester_024_probe` and
`lane_057_tester`.

**Goes (ecad-file):** `codec` (JSON Lines framing), `read` (`open_project`, `OpenReport`,
`Outcome`, `Check`, `RecordSpan`…), `write` (`ProjectFile`, `SaveOutcome`, `AttemptSummary`…), the
v1 `disk` (`DiskProject`), `record::Header` and the v1 container constants (`CODEC "jsonl/1"`,
`CONTAINER_VERSION 1`, `MAX_LINE`, `MAX_RECORDS`), `v2/convert.rs` (`convert_file`, `sniff`,
`Major`), `binding`'s `needs_conversion` / `convert`, the golden file
`tests/golden/project-v1.ecad`, and the v1-only tests (`disk_contracts`, `disk_edge_contracts`,
`incremental_acceptance`, `recovery_contracts`, `save_contracts`, the v1 parts of
`records_contracts`, `canonical_contracts`, `symbol_definition_contracts`, and the conversion tests
in `container_v2` and `container_recovery`). **ecad-cli:** `ecad upgrade` and its test.

**Stays, renamed neutrally:** `v1.rs` (the record types `Op`, `Primitive`, `Definition`, `TxKind`…:
the redb format stores every transaction as these records) → `records`; `record.rs`'s
`tx_value`, `decode_tx`, `check_limits`, `seq_and_prev`, `SCHEMA_VERSION`, `definition_hash`
(pinned by a known-hash test); `canonical`, `chain`, `crc`. From the v1 `disk`, `ReadOnly`,
`JUST_ENDED`, `holder_record` (used by `v2/disk.rs`) and `utc` (used by ecad-app's debugger and
ecad-bench's report) move to a neutral module. `v2::schema::LinkV1` stays: it is part of the
version 2 format (a converted file's link), read and verified, just never written again.

**Agreed knock-ons:** ecad-bench `generate.rs` writes v1 (to the redb format); its
`generator_contracts`, `tester_contracts` and `lane_benchv2` read v1 or convert; its open, edit and
save timings are measured on the v1 file. ecad-cadcraft `debug.rs`'s saved-record view
(`read_record`, line ~864). The app's conversion offer (`binding::needs_conversion` / `convert`).

**Beyond the knock-ons (why step 12 stopped):**
1. **ecad-cadcraft runtime: `Persistence::TestStore`.** `Project::create` defaults to it;
   `MemoryStore` saves through v1's `ProjectFile` over a `MemoryMedium`, `Project::open_from` /
   `ProjectHandle::open_from` reopen v1 bytes, `Project::open_report` keeps v1's `OpenReport`.
   Used by ecad-cadcraft tests (`experiment`, `refusals` (a CRC-damaged v1 file opens read-only),
   `cable_reference_contracts`, `cable_slash_acceptance_contracts`, `sheets_contracts`,
   `tester_033_print`) and by every test that saves a project made with `Project::create`.
   Needs a decision: an in-memory or temporary redb store in its place, or no test store.
2. **ecad-cadcraft `debug.rs` beyond the saved-record view:** the file snapshot carries v1's
   `Header`, `AttemptSummary` / `AttemptStage` and `RecordSpan`s and a v1 `open_report`, shown by
   ecad-app's debugger (Header and "Opening found" rows) and serialised as JSON.
3. **dev/ecad-inspector:** `export-jsonl` (CLI, the `export_jsonl` protocol call and the HTML
   page's Export JSONL button) writes a v1 file with `codec` and `Header`; `cli.rs` and
   `source.rs` detect a v1 file with `convert::sniff` and offer `ecad upgrade`; its tests build
   v1 files and convert them as fixtures (`common.rs`), and read the export back with
   `open_project` (`cli_contracts`).
4. **A v1 fixture used by redb tests:** ecad-cadcraft's `tester_024_probe`
   (`the_golden_v1_project_opens_and_reports_build`) and `lanes/lane_057_tester` convert
   `ecad-file/tests/golden/project-v1.ecad` and test the redb project made from it.
5. **Docs:** `docs/quick-start.md` §7B (open a version 1 project, `ecad upgrade`), the
   `export-jsonl` and inspector drop steps, the bench's "version 1" timings and the limitation
   "the `.ecad` format is version 1"; `docs/spec/eep-0002.md` (the v1 spec, marked current) and
   `eep-0003.md` §11 and §16 (v2 hashes defined over v1 canonical JSON; migration from v1).

Not affected: ecad-mcp (its tools and tests use `Persistence::File` and `v2` only), ecad-model,
ecad-store, ecad-electrical (only `definition_hash`).

---

## 4. Guard rules for `cargo xtask layers`

Today's `Class` enum (Core, Adapter, Ui, UiKit, Dev, Exempt) becomes the layer names. The table
in `xtask/src/main.rs` changes as follows:

| Layer | Crates | May depend on (normal deps) | Must not |
|---|---|---|---|
| Core | model, drawing-style, store, file, inspect, electrical | Core crates before it in the §1 order (model → drawing-style → store → file → inspect → electrical); crates.io crates allowed below | any `cadcraft-*`, any UI crate (`UI_CRATES`), `ecad-ui`, `ecad-tokens`, Engine, App, Programs, Dev |
| Engine | ecad-cadcraft | Core; `cadcraft-engine`, `cadcraft-io` | UI crates, `ecad-ui`, `ecad-tokens`, `cadcraft-ui-egui`, App, Programs, Dev |
| UI kit | ecad-ui, ecad-tokens | `ecad-tokens`: `ecad-drawing-style` only. `ecad-ui`: `egui`, `epaint`, `egui_extras`, `ecad-tokens`, `ecad-drawing-style` (`UIKIT_ALLOWED` gains `ecad-drawing-style`) | everything else |
| App | ecad-app | Core, Engine, UI kit; `cadcraft-ui-egui`, `eframe`, `egui` | `cadcraft-engine` directly, Programs, Dev (except through its `dev` feature) |
| Programs | desktop, cli, mcp | Core, Engine, App, other Programs (owner ruling); `cadcraft-ui-egui` (desktop), `cadcraft-mcp` (mcp) | `cadcraft-engine` directly, Dev (except through `dev`) |
| Dev | under `dev/`: bench, inspector; `xtask` (Exempt) | anything | — |

No layer has to build for wasm: ECAD is desktop-only (owner ruling, 2026-11-15).

Source checks, in addition to `Cargo.toml` (`layers` already scans sources for dev features and
raw egui):

- **No `cadcraft_engine::` path** in App or Programs sources. A Cargo rule is not enough, because
  `cadcraft-ui-egui` and `cadcraft-mcp` can hand engine types through.
- **No `cadcraft_` path** in `ecad-electrical` sources. This is redundant with Cargo, but it gives
  a clear message.
- **A crate is under `dev/` exactly when it is Dev.** This rule exists already; ecad-bench's move
  makes it hold.
- **No wasm check.** `WASM_CRATES` and `cargo xtask wasm` are removed for the ECAD crates (§5,
  step 10). CADCraft's own wasm build is not affected.

- **No OS access in Core** (owner, 2026-11-15; §3.1). In Core sources other than `ecad-store`,
  outside `#[cfg(test)]` and `tests/`, `layers` refuses `std::fs`, `std::env`, `std::process`,
  `std::net`, `SystemTime`, `Instant::now`, `RandomState`, `chrono::Local`, `Utc::now`,
  `home_dir`, `canonicalize`, `File::`, `OpenOptions` and any `dirs`/`directories` crate.
  Threads and rayon are allowed. Until step 10 this is lane 060's count-can-only-fall ratchet,
  and from step 10 it is zero.

Dependency rules from lane 061 (§6 there), one check each:

- **No `anyhow` in a library.** `anyhow` may appear only in Programs, `xtask` and `ecad-bench`'s
  binary. Libraries return their own `thiserror` enum.
- **No `tokio` anywhere** in an ECAD `Cargo.toml`.
- **`rayon` only where the work is.** It may appear only in model, file, electrical or cadcraft,
  as a plain dependency. It needs no wasm `cfg`, because there is no wasm build (061's
  "native only" rule and its sequential twin fall away).
- **`schemars` only in Engine,** plus `ecad-model`'s optional `schema` feature.

Lane 060 (R0 guards) owns the ratchets in `layers`. The split lane adds its rules after 060 has
merged, so the two lanes never edit `layers` at the same time.

---

## 5. The split lane: step order

Preconditions: lanes 060–063 have merged into `dev`. 058 (paged reports) and 064 (R9 views) have
not started building. One worker, on `feature/crates`. Every step builds, passes clippy and the
tests of the crates it touched, and keeps behaviour the same. No test changes except paths.
ecad-cadcraft's `tests/all` (about 1,000 tests, all going through commands) does not move; it
tests E through C.

| Step | What | Green because |
|---|---|---|
| 1 | **Folder moves.** `crates/ecad-bench` → `dev/ecad-bench` (class Dev). xtask table, CI and docs paths. | No code change. |
| 1b | **Token split (§1.1).** New `crates/ecad-drawing-style` (Core) takes `sheet`, `sheet_font`, `sheet_type`, `wire`, `marker`, the mm half of `line`, and `Color`. `ecad-tokens` keeps the screen tokens and is reclassed UI kit. Paths change in ecad-cadcraft (8 files), ecad-ui (2), ecad-trial (a few). The doc-sync test splits with the tables. | Path changes only. Both doc-sync tests pass. |
| 1c | **Core OS cuts** (§3.1): ecad-model's switch; ecad-file's ten calls onto ecad-store (`holder_record`, `same_path`, `read_head`, `read_all`, `len`, `into_file`); `BranchId::from_seed`; the redb opens. Lane 060's ratchet goes to 0 for both crates. Independent of the cadcraft cuts. File-format code: the tester covers it. | Calls move, behaviour doesn't. The v2 file tests pass. |
| 2 | **Cuts 1, 2, 3, 8, 13** in ecad-cadcraft: helpers out of `interact`, `commands`, `*_cmds` and `look` into the modules that will go to E, with re-exports. | Pure moves inside one crate. |
| 3 | **Cut 4** (`text_style`), two commits. | Same. |
| 4 | **Cuts 5, 6, 7, 9, 10**: `draw/` modules in C. Domain functions take `&Document`. | Same. Engine imports leave the E modules. |
| 5 | **Closure check.** A temporary xtask check, or a script in the brief, lists the E set and fails on any `crate::<C module>` or `cadcraft_` in it. It must pass before step 6. | Read-only check. |
| 6 | **Create `ecad-electrical`** (Core, OS-access count 0) and `git mv` the E set in one commit. Fix `crate::` paths. ecad-cadcraft re-exports each moved module under its old name (`pub use ecad_electrical::strips;`), so apps and tests do not change. `file`'s F half moves to `ecad_file::binding` the same way, and `same_path` to ecad-store. | The re-exports keep every path working. |
| 7 | **Cut 11**: `ecad_cadcraft::cad`. Apps and ecad-mcp switch paths and drop `cadcraft-engine`. | Path changes only. |
| 8 | **Cut 12**: `ecad_cadcraft::agent`. ecad-trial drops `ecad-mcp`. | MCP contract tests and the bridge tests unchanged. |
| 9 | **App split.** ecad-trial's library becomes `crates/ecad-app` (`Trial` → `App`), and `apps/ecad-desktop` keeps `main`, `window`, `startup` and `args`. Cut 13's action table moves in. Update `docs/quick-start.md` (`cargo run -p ecad-desktop`), the CI, `xtask demo`, `ecad-mcp`'s `--launch` binary name, `DEV_BY_DEFAULT` and the raw-egui allowlist paths. | The trial tests move with the library. |
| 10 | **Guards and wasm.** The §4 table, source checks and 061 rules in `layers`; the README check (§6) and the READMEs. **Remove the ECAD wasm check:** `WASM_CRATES`, `cargo xtask wasm` and its step in `cargo xtask ci`, and the ECAD wasm job in CI. `ecad/CLAUDE.md`: add the §1 vocabulary and the desktop-only ruling. Remove its wasm lines: the "Run wasm locally (`cargo xtask wasm` …) only when a change could affect it …" sentence, and "with the slow set and wasm" in the CI paragraph. 061's design and lane rules lose "rayon native only". Existing `cfg(target_arch = "wasm32")` branches (`ruzstd` decoding in `ecad-file`, `chrono` clock, `agent_access`, `same-file`) stay working. Removing them is a later clean-up, listed in `docs/deferred-work.md`, not part of this lane. | The new rules pass on the result of steps 1–9. |
| 11 | **Drop the re-exports** from step 6 that only tests and apps use. Callers say `ecad_electrical::…`. This step is optional, and can be left for the phase-end tidy-up. | Path changes only. |

Tester: yes. Step 1c changes the file reader and writer; steps 4 and 6 touch the projection and `translate.rs` neighbours. The tester runs
ecad-cadcraft's and ecad-trial's fast tests and the medium bench (open and first page), and
compares the result with `dev`.

**Where R9 lands** (design 064 on `feature/views`, §2). 064 was written assuming the split comes
after R9 and calls the domain crate `ecad-domain`. With the split first:

| R9 piece | 064 said | Now |
|---|---|---|
| `Effect`, `Effects::of`, `EffectKind`, `Area`, `Key` | `ecad-model/src/effect.rs` | Same (Core). |
| `DerivedView`, `Phase`, `NoteCx`, `Outcome`, `ViewScope` | `ecad-model/src/view.rs` | Same (Core). |
| `Views` and one file per view (`derived/<view>.rs`) | `ecad-cadcraft/src/derived/`, moving later to `ecad-domain` | **`ecad-electrical/src/derived/`** from the start. No move later, and the "no `cadcraft_*` in `derived/`" rule is implied by the crate. |
| `Changes` (change log, feeds) | `ecad-cadcraft/src/project/changes.rs` | The engine-free `Changes` type goes in `ecad-electrical` (`derived/changes.rs`). Calling redraw stays in `project/changes.rs` (C). |
| `DocumentOwner` (commit, undo, redo) | `ecad-cadcraft/src/project/owner.rs` | Same (Engine). |
| `project.rs` split (064 §8, R9 step 1) | R9 | Stays in R9. It is inside the Engine and does not change the map. |
| Trial consumers (064 §6) | ecad-trial | ecad-app. |

The overseer amends 064 §2 and §9 step 2 to say `ecad-electrical`, and to say that the split has
landed.

---

## 6. A README in every crate

Every workspace member gets `README.md` at its root. xtask checks it as part of `layers`, using
`cargo metadata`'s package list: the file exists, it has the six headings below in order, and the
`Layer:` line names the crate's layer from the xtask table. A crate whose README is missing or
whose layer is wrong fails with a one-line message.

Template:

```markdown
# <crate name>

Layer: <Core | Engine | UI kit | App | Programs | Dev>

## Purpose
One or two sentences: what this crate is for, in the words of the crate map.

## Contents
The main modules, one line each.

## Must not depend on
What `cargo xtask layers` forbids for this layer, plus anything this crate keeps out on purpose
(for example "no clock: callers pass the time").

## Key types
The few types a caller starts from, with a link to their docs.

## How to test
`cargo test -p <crate>`; where its tests live (`tests/all/…`); slow tests. For Programs: link
to `docs/quick-start.md` instead of repeating it.
```

The crate's `//!` doc stays the detailed reference. The README is the short card, and keeps the
"what it excludes" sentence that `ecad/CLAUDE.md` asks of every crate doc.

---

## 7. Owner decisions (approved 2026-11-15)

All decisions below are approved. The owner answered the five calls on 2026-11-15.

### Going ahead (follows the map as given)

1. **Core** = model, store, file, the new **ecad-electrical**, ecad-inspect and the new
   **ecad-drawing-style**.
2. **ecad-electrical** gets the electrical rules from ecad-cadcraft (§2.1, "E"). 064's
   `ecad-domain` is renamed `ecad-electrical`.
3. **Engine** = ecad-cadcraft, keeping the CADCraft link, the projection, the commands and the
   interaction forms.
4. **UI kit** = ecad-ui and ecad-tokens (screen tokens). It is a sibling of Engine; neither uses
   the other. This is today's rule.
5. **Programs** = ecad-desktop, ecad-cli, ecad-mcp; Program→Program dependencies are allowed.
6. **Dev** = ecad-bench (moved from `crates/` to `dev/`), ecad-inspector and xtask.
7. **The apps stop using `cadcraft-engine` directly**, through `ecad_cadcraft::cad` (code review
   finding 9). App may use `cadcraft-ui-egui`, and ecad-mcp may use `cadcraft-mcp`.
8. **The guard rules** in §4, with lane 061's dependency rules, and a README per crate checked by
   xtask (§6).
9. **Order:** after 060–063 merge, before the R9 worker. R9's `derived/` goes straight into
   ecad-electrical.
10. **No OS access in Core** (owner rule, 2026-11-15; §3.1, §4). Only ecad-store touches the disk.
    The clock, entropy, environment and switches are passed in, and rayon is fine. ecad-file works
    on handles and bytes from ecad-store. Lane 060's ratchet reaches zero within the split lane.

### The owner's answers to the calls

1. **Tokens: split.** The drawing style (sheet and canvas colours, sheet font, sheet text sizes,
   wire colours and dashes, markers, mm line weights) goes to a new Core crate,
   **`ecad-drawing-style`**. It goes in Core rather than Engine because ecad-electrical, ecad-ui
   and ecad-app use it too. The screen tokens stay in `ecad-tokens`, in the UI kit. Core is the
   data structure, not look-and-feel. The cost is one mechanical step (§1.1, §5 step 1b).
   User-facing drawing defaults may later become TOML data through lane 066.
2. **ecad-inspect stays in Core.** The dev inspector, with its web server, stays in `dev/` and
   never ships; the two do not merge. **ECAD is desktop-only:** no web or browser product. The
   ECAD wasm requirement is dropped from the map, the guard rules and `ecad/CLAUDE.md` (§5 step
   10). Rayon needs no wasm `cfg`. CADCraft's own wasm build is not affected.
3. **Yes:** ecad-trial's library becomes `crates/ecad-app`, with a thin `apps/ecad-desktop` on
   top (§5 step 9).
4. **Yes:** keep the name `ecad-cadcraft`. The layer is still called Engine.
5. **Yes:** MCP stays its own program. The agent permission check moves out of ecad-mcp and sits
   beside the commands, in `ecad_cadcraft::agent` (cut 12). It moves to ecad-electrical with the
   command rules later. The `*_cmds` modules stay in Engine for this split.
