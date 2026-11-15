# Design 065: the crate map

Status: Draft (design only; no code moves in this lane).
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
   it. ecad-tokens is plain `const` data with no dependencies, and xtask already classes it as
   Core.
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
| **Core** | What an ECAD project *is* and what it *means*, without a screen or a drawing engine. Builds for wasm. | `ecad-model`, `ecad-store`, `ecad-file`, `ecad-electrical` (new), `ecad-inspect`, `ecad-tokens` |
| **Engine** | The link to CADCraft: draws pages from the model, turns edits into transactions, runs commands. | `ecad-cadcraft` |
| **UI kit** | ECAD's widgets: how a value, button, table or panel looks. Knows nothing about projects. | `ecad-ui` |
| **App** | The ECAD application: shell, workspaces, panels, dialogs, lists, tabs, on a CADCraft window. | `ecad-app` (new, from ecad-trial) |
| **Programs** | Things a person or agent starts: one `main` each, thin. | `ecad-desktop` (ecad-trial's binary), `ecad-cli`, `ecad-mcp` |
| **Dev** | Developer and test tools under `ecad/dev/`. Never shipped. | `ecad-bench`, `ecad-inspector`, `xtask` |

Inside Core, the order is: `ecad-tokens` and `ecad-model` at the top, then `ecad-store`,
`ecad-electrical`, `ecad-inspect`, then `ecad-file`.

- `ecad-model`: the document, ops, ledger, merge, numbering, derived nets and references.
- `ecad-store`: storage under the file: pages of bytes, the lock.
- `ecad-file`: the `.ecad` container, read and write.
- `ecad-electrical`: the electrical rules: devices, wires, cables, strips, checks, ratings,
  reports, cross-references, styles and defaults. It reads a `Document` and returns data or ops.
- `ecad-inspect`: read-only snapshots and the runtime event log.
- `ecad-tokens`: every colour, size and font as data.

`ecad-electrical` depends on `ecad-model` and `ecad-tokens` only, not on the store or the file.

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
| `defaults/` (14 files) | 2,837 | 0 | E | The defaults registry. | Clean, once ecad-tokens is Core (decision 1). `default_cmds` and `text_style` helpers move first (cut 4). |
| `designation_cmds` | 308 | 6 | C | Commands. | — |
| `designations` | 477 | 0 | E | The project's functions and locations list. | Helper functions now in `designation_cmds` and `location_cmds` move to E (cut 2). |
| `device_cmds` | 368 | 8 | C | Commands. | — |
| `device_list` | 441 | 1 | split | The Devices tab rows are E. "Go to" selects engine handles, C. | `Handle` stays C. |
| `devices` | 1,637 | 3 | split | Tags, prefixes, properties and starters are E. Text placement and colour index are C. | `text_style` split (cut 4). `project::Project` becomes `&Document`. `look::framed` moves to E. |
| `file` | 426 | 0 | F | Binding a project to its `.ecad` file, plus path helpers (`home`, `expand_home`, `normalize`, `same_path`). Uses only ecad-file and ecad-model. | Clean. Goes to `ecad_file::binding`; C re-exports it. |
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
   token font metrics), `parse_at`, `candidates`, `symbol_text_keys`, `rename_symbol_key` and the
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
8. **Clock.** `project::now_ms` (chrono, `clock` off wasm) is called by `revisions` and `views`.
   E takes a time argument instead. Only C and the App read the clock.
9. **`front`.** `check_rows` and the legend contents go to E. Drawing the cover stays in C.
10. **`title_block` and `frame`.** `bind`, `checks`, `house`, `frame::fit` and `frame::touches`
    go to E. `layout` and `dxf` stay in C.
11. **The boundary leak.** ecad-cadcraft gets `pub mod cad`, a short list of re-exports:
    `Vec`, `Bounds`, `Handle`, `Space`, `Session`, `View`, `Input`, `entity_bounds`,
    `extent_bounds` and `find_command` (what the apps and the MCP use today). ecad-app and
    ecad-mcp then drop `cadcraft-engine` from their `Cargo.toml`. A new engine type the App needs
    is added to `cad` on purpose, in review.
12. **The agent gate.** `ecad-mcp`'s `gate`, `policy` and `tools`, the shared limits and words
    in its `lib.rs`, `bridge::ALLOWED_METHODS`, and ecad-cadcraft's `agent_access` move together
    into `ecad_cadcraft::agent`. `ecad-mcp` keeps the server, the bridge client and launching.
    ecad-app stops depending on ecad-mcp.
13. **`actions`.** `ObjectKind` goes to E (`interact::kind` uses it). The action table goes to
    ecad-app.

---

## 4. Guard rules for `cargo xtask layers`

Today's `Class` enum (Core, Adapter, Ui, UiKit, Dev, Exempt) becomes the layer names. The table
in `xtask/src/main.rs` changes as follows:

| Layer | Crates | May depend on (normal deps) | Must not | Builds for wasm |
|---|---|---|---|---|
| Core | model, store, file, electrical, inspect, tokens | Core crates above it in the §1 order; crates.io crates allowed below | any `cadcraft-*`, any UI crate (`UI_CRATES`), `ecad-ui`, Engine, App, Programs, Dev | **yes**, all six |
| Engine | ecad-cadcraft | Core; `cadcraft-engine`, `cadcraft-io` | UI crates, `ecad-ui`, `cadcraft-ui-egui`, App, Programs, Dev | yes |
| UI kit | ecad-ui | `egui`, `epaint`, `egui_extras`, `ecad-tokens` (unchanged `UIKIT_ALLOWED`) | everything else | yes |
| App | ecad-app | Core, Engine, UI kit; `cadcraft-ui-egui`, `eframe`, `egui` | `cadcraft-engine` directly, Programs, Dev (except through its `dev` feature) | no |
| Programs | desktop, cli, mcp | Core, Engine, App, other Programs (owner ruling); `cadcraft-ui-egui` (desktop), `cadcraft-mcp` (mcp) | `cadcraft-engine` directly, Dev (except through `dev`) | no |
| Dev | under `dev/`: bench, inspector; `xtask` (Exempt) | anything | — | no |

Source checks, in addition to `Cargo.toml` (`layers` already scans sources for dev features and
raw egui):

- **No `cadcraft_engine::` path** in App or Programs sources. A Cargo rule is not enough, because
  `cadcraft-ui-egui` and `cadcraft-mcp` can hand engine types through.
- **No `cadcraft_` path** in `ecad-electrical` sources. This is redundant with Cargo, but it gives
  a clear message.
- **A crate is under `dev/` exactly when it is Dev.** This rule exists already; ecad-bench's move
  makes it hold.
- **wasm.** `WASM_CRATES` becomes "every Core crate, Engine and UI kit". Add `ecad-electrical`.
  `cargo xtask wasm` already builds the list with `-D warnings`.

Dependency rules from lane 061 (§6 there), one check each:

- **No `anyhow` in a library.** `anyhow` may appear only in Programs, `xtask` and `ecad-bench`'s
  binary. Libraries return their own `thiserror` enum.
- **No `tokio` anywhere** in an ECAD `Cargo.toml`.
- **`rayon` only off wasm.** It may appear only under `[target.'cfg(not(target_arch =
  "wasm32"))'.dependencies]`, and only in model, file, electrical or cadcraft.
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
| 2 | **Cuts 1, 2, 3, 8, 13** in ecad-cadcraft: helpers out of `interact`, `commands`, `*_cmds` and `look` into the modules that will go to E, with re-exports. | Pure moves inside one crate. |
| 3 | **Cut 4** (`text_style`), two commits. | Same. |
| 4 | **Cuts 5, 6, 7, 9, 10**: `draw/` modules in C. Domain functions take `&Document`. | Same. Engine imports leave the E modules. |
| 5 | **Closure check.** A temporary xtask check, or a script in the brief, lists the E set and fails on any `crate::<C module>` or `cadcraft_` in it. It must pass before step 6. | Read-only check. |
| 6 | **Create `ecad-electrical`** (Core, wasm) and `git mv` the E set in one commit. Fix `crate::` paths. ecad-cadcraft re-exports each moved module under its old name (`pub use ecad_electrical::strips;`), so apps and tests do not change. `file` moves to `ecad_file::binding` the same way. | The re-exports keep every path working. Run `cargo xtask wasm`. |
| 7 | **Cut 11**: `ecad_cadcraft::cad`. Apps and ecad-mcp switch paths and drop `cadcraft-engine`. | Path changes only. |
| 8 | **Cut 12**: `ecad_cadcraft::agent`. ecad-trial drops `ecad-mcp`. | MCP contract tests and the bridge tests unchanged. |
| 9 | **App split.** ecad-trial's library becomes `crates/ecad-app` (`Trial` → `App`), and `apps/ecad-desktop` keeps `main`, `window`, `startup` and `args`. Cut 13's action table moves in. Update `docs/quick-start.md` (`cargo run -p ecad-desktop`), the CI, `xtask demo`, `ecad-mcp`'s `--launch` binary name, `DEV_BY_DEFAULT` and the raw-egui allowlist paths. | The trial tests move with the library. |
| 10 | **Guards.** The §4 table, source checks and 061 rules in `layers`; the README check (§6) and the READMEs. `ecad/CLAUDE.md` gets the §1 vocabulary. | The new rules pass on the result of steps 1–9. |
| 11 | **Drop the re-exports** from step 6 that only tests and apps use. Callers say `ecad_electrical::…`. This step is optional, and can be left for the phase-end tidy-up. | Path changes only. |

Tester: yes. Steps 4 and 6 touch the projection and `translate.rs` neighbours. The tester runs
ecad-cadcraft's and ecad-trial's fast tests, `cargo xtask wasm` and the medium bench (open and
first page), and compares the result with `dev`.

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
`cargo test -p <crate>`; where its tests live (`tests/all/…`); slow tests; whether it is in
`cargo xtask wasm`. For Programs: link to `docs/quick-start.md` instead of repeating it.
```

The crate's `//!` doc stays the detailed reference. The README is the short card, and keeps the
"what it excludes" sentence that `ecad/CLAUDE.md` asks of every crate doc.

---

## 7. Owner decisions

### Going ahead (follows the map as given)

1. **Core** = model, store, file, the new **ecad-electrical**, plus inspect and tokens (see calls
   2 and 1). Every Core crate builds for wasm.
2. **ecad-electrical** gets the electrical rules from ecad-cadcraft (§2.1, "E"). 064's
   `ecad-domain` is renamed `ecad-electrical`.
3. **Engine** = ecad-cadcraft, keeping the CADCraft link, the projection, the commands and the
   interaction forms.
4. **UI kit** = ecad-ui. It stays a sibling of Engine; neither uses the other. This is today's
   rule.
5. **Programs** = ecad-desktop (ecad-trial's binary), ecad-cli, ecad-mcp; Program→Program
   dependencies are allowed.
6. **Dev** = ecad-bench (moved from `crates/` to `dev/`), ecad-inspector and xtask.
7. **The apps stop using `cadcraft-engine` directly**, through `ecad_cadcraft::cad` (code review
   finding 9). App may use `cadcraft-ui-egui`, and ecad-mcp may use `cadcraft-mcp`.
8. **The guard rules** in §4, with lane 061's dependency rules, and a README per crate checked by
   xtask (§6).
9. **Order:** after 060–063 merge, before the R9 worker. R9's `derived/` goes straight into
   ecad-electrical.

### Needs your call

1. **ecad-tokens goes in Core, not the UI kit.** The engine draws canvas colours, dashes and the
   sheet font from it, so the engine needs it, and tokens is plain data with no dependencies.
   *Recommend: Core.*
2. **ecad-inspect stays in the product (Core), not `dev/`, and does not merge with
   ecad-inspector.** `Project` keeps its event log, and the MCP `ecad_inspect` tool returns its
   snapshot. A merge would pull a WebSocket server into the product. *Recommend: keep the two
   apart; inspect in Core.*
3. **ecad-app comes out of ecad-trial, not ecad-cadcraft.** ecad-cadcraft has no UI. The split
   is ecad-trial's library → `crates/ecad-app`, and a thin `apps/ecad-desktop` binary. *Recommend:
   do it, in step 9 of the split.*
4. **Keep the name `ecad-cadcraft`; don't rename it `ecad-engine`.** Next to `cadcraft-engine`,
   `ecad-engine` reads as ECAD's own engine. `ecad-cadcraft` says exactly what it is: the link to
   CADCraft. The layer is still called Engine. *Recommend: keep the name.*
5. **The agent write gate moves from ecad-mcp into `ecad_cadcraft::agent`, and the `*_cmds`
   command modules stay in Engine for now.** The window and headless MCP share the gate. If it
   stays in ecad-mcp, the App library depends on a Program. Command rules move down to
   ecad-electrical later, with R6's typed parameters. *Recommend: both.*
