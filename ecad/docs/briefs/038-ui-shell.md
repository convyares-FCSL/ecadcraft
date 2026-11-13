# Brief 038: The shell (U2)

Status: Approved (owner, 2026-11-13).
Branch: `feature/ui-shell`, from `origin/dev` (037, 035, 036 and the per-lane layout, PR #72, have merged).
Designer: done (`docs/design/ui-architecture.md` §1, §5, §8, §9, §10; the look is spec §2) ·
Size: L
Roadmap: M1 **1.5** (UI shell), second part. Runs beside U3 (039), which owns the table.
Inputs: `docs/discussions/ui-2026-11/ui-design-spec.md` §2, §3 and §6 step 2; `owner-answers.md`.

## Goal

The window becomes ECADCraft's: top bar, workspaces, left navigation, tool strip, main view,
drawer, right detail panel and status bar, with today's panels moved into their slots unchanged.
CADCraft's menus, toolbar, Tool Sets, layers, model and layout tabs and command line disappear
behind a developer switch; nothing is deleted.

## §0 Files touched

| File | Change |
| --- | --- |
| `crates/ui-egui/src/lib.rs`, `canvas.rs` (**upstream seam US1**, `// ECAD-SEAM(owner): embedded drawing view.`) | `Embedding { chrome, idle_typing, canvas_menus, style }`, `CadApp::ui_embedded`, `take_secondary_click` (design §1.3). Default: unchanged |
| `crates/ui-egui/tests/` (one module, `embedded_view.rs`) | US1 tests (design §9) |
| `ecad/UPSTREAM-PATCHES.md` | Seam US1 |
| `ecad/apps/ecad-trial/src/shell/` (new: `mod.rs`, `layout.rs`, `state.rs`, `top_bar.rs`, `status_bar.rs`, `left.rs`, `drawer.rs`, `right.rs`, `tool_strip.rs`, `text_panel.rs`, `keys.rs`) | The shell |
| `ecad/apps/ecad-trial/src/lib.rs` | `Trial::ui` draws `shell::show`; `cad.ui(ui)` becomes `cad.ui_embedded` in the main view; the Project panel host goes |
| `ecad/apps/ecad-trial/src/project_panel.rs`, `devices_panel.rs`, `history_panel.rs`, `checks_panel.rs`, `developer.rs`, `sheets_ui.rs` (section only), `outputs_ui.rs`, `release_ui.rs`, `search.rs`, `tools.rs` | Hosted in slots (design §5.6); contents unchanged except framing; allowlist entries removed where restyled |
| `ecad/apps/ecad-trial/src/window.rs` | Shell state and per-project views in eframe storage (design §5.4) |
| `ecad/apps/ecad-trial/src/main.rs` | `--cad-ui` flag |
| `ecad/apps/ecad-trial/tests/all/lanes/lane_038.rs` (new lane module, one line in `lanes/mod.rs`) | Shell tests |
| `ecad/docs/quick-start/038-ui-shell.md` (per-lane file) | New layout, keys, the developer switch |
| `ecad/crates/ecad-cadcraft/src/lanes.rs` | One line, `("038", …)`, only if the shell adds ECAD commands (per-lane registration) |
| `ecad/apps/ecad-trial/src/agent/banner.rs` (placement only) | 035 has merged: the banner (`agent::AgentBanner`, `Trial::agent_banner()`, `Trial::agent_action`) moves to the shell's agent banner slot; its data contract and element names stay (design §12 O2) |

Not touched: `ecad-cadcraft` (including `project.rs`; the redraw issue is overseer item O1), the
canvas look (U4a), new tools or the wheel (U4b), the table (U3; this brief hosts U3's views
through `lists::show` once it exists, design §12 O4).

## Since this brief was drafted

- 035 (live MCP bridge) and 036 (multi-level terminals) have merged into `dev`. 035's banner
  (`agent::AgentBanner`, `Trial::agent_banner()`, `Trial::agent_action`) goes into the shell's
  agent banner slot with its data contract and element names unchanged (design §12 O2); the top
  bar's agent pill reads the same banner state.
- 035's Agent access setting stays in the Developer section until the shell gives settings a
  home; U2 builds the Settings overlay as a slot, and the setting moves out of the Developer fold
  into Settings ▸ Agent access in U4d (design §12 O2 and O7).
- Per-lane files (PR #72): quick-start text in `docs/quick-start/038-ui-shell.md`, tests in
  `tests/all/lanes/lane_038.rs`, any new command module registered in `ecad-cadcraft/src/lanes.rs`.

## Scope

1. **US1** (design §1.3), with its tests.
2. **Layout** (design §5.1–§5.3): `layout(window, state, agent) -> Slots`, pure; resizing,
   collapsing to rails, the collapse order below 760 px main width.
3. **Regions** (spec §2 table):
   - top bar: project menu (New, Open, Save, Save As, Close), workspace segmented control
     (Ctrl+1–4), search field (Ctrl+K opens today's symbol search for now), Checks pill (count),
     revision pill, agent pill (off when 035 is absent);
   - left panel: tabs Pages, Devices, History; width per tab; rail; Pages shows a dot on pages
     changed while elsewhere;
   - tool strip: seven group buttons (V W P C D B T) whose flyouts list today's ECAD tools that
     fit each group; a group key picks the last used and steps;
   - main view: Draw shows the page (`ui_embedded`); Lists, Library and Publish host today's
     Reports, symbol search and Release / Print Book bodies (design §5.6);
   - drawer: Checks, 150 / 36 px;
   - right panel: the ECAD properties section (today in CADCraft's Properties palette), 340 px,
     rail;
   - status bar: save state and time, page, standard name, zone under the cursor, zoom, MCP
     state, Commands button;
   - Text panel over the right panel (Ctrl+/), Commands tab = today's Developer fold;
   - Settings: an overlay slot opened from the project menu, empty but for a Developer section
     with the developer switch.
4. **Companion split** (design §5.5) with a page and today's Reports body as the second view;
   shared selection. No pop-out.
5. **Developer switch** (`--cad-ui`, `ui.cadchrome`, Settings ▸ Developer).
6. **Keys** (spec §5 table) that exist today: groups, Esc, Del, Ctrl+Z/Y, Ctrl+1–4, Ctrl+K,
   Ctrl+/, PgUp/PgDn, F, Alt+Left/Right (Back over page and workspace visits). Single letters only
   over the main view and not while editing text.
7. **Persistence** (design §5.4).
8. **Update maps** for every hosted view (design §8.2 table), with build counters.

## Out of scope

The table component and converting Checks and Devices to it (U3); the wheel and the action table
(U4b; right-click on the page does nothing in U2 except while a command runs); paper sheet and
markers (U4a); inspector origin badges and Standards (U4c); restyled banner and History authors
(U4d); converting the remaining floating windows (sheets, merge, renumber, preview), which stay as
they are and are listed in the PR; pop-out companion.

## Update map

As design §8.2's table. Every other op: nothing. Full rebuild only on open, reload, first show of a
view, or a change feed that rolled over (more than 64 changes unseen).

## Acceptance criteria

1. **Seam.** CADCraft's tests pass unchanged; `embedded_view.rs` shows: `Embedding::default()`
   draws as today; `chrome: false` draws no title bar, toolbar, menu, status bar, file tabs, Tool
   Sets or docking; `idle_typing: false` ignores `L` + Enter while idle but takes `10,10` at a
   prompt; `canvas_menus: false` opens no menu and `take_secondary_click` returns the position.
   `cargo xtask patches` passes.
2. **Layout.** Unit tests reproduce every size in design §5.2 at 1920 × 1080 (main view 1268 ×
   824 with the drawer open, 1268 × 974 collapsed, 32 px less with the banner; 1316 wide in Lists),
   the collapse order, and the companion minimum of 360 px per half.
3. **Shell flows** (kittest, 1920 × 1080, by label): Ctrl+1–4 switch workspace and the tool strip
   shows only in Draw; each left tab shows today's content; Ctrl+/ opens the Commands tab and a
   command typed there runs; `--cad-ui` shows CADCraft's toolbar inside the main view; `L` + Enter
   over the page does not start LINE; V, W, P, C, D, B, T are ignored while a text field has focus.
4. **Nothing lost.** Every action reachable from today's Project panel header, tabs, Developer
   fold, ECAD Tool Sets tab and Properties section is reachable in the shell (a test lists them);
   the existing `ecad-trial` tests pass, updated only for where elements moved, each named in the
   PR.
5. **Persistence.** Shell state and per-page zoom survive a restart; a corrupt stored value starts
   with defaults and one history line; more than 32 projects drops the oldest.
6. **Live.** A commit on page 3 while page 1 is shown sets the dot on page 3 in Pages and leaves
   the Pages rows' build counter unchanged; a commit changing nothing a view shows leaves its
   counter unchanged.
7. **Performance** (release, medium tier, by hand or with the bench's `ui` measure if 039 adds
   it): shell share of a frame ≤ 2 ms, idle frame ≤ 8 ms, workspace switch within the next frame,
   0 repaints over 2 s at rest (design §10).
8. **Screenshots** (`ECAD_TESTER_SHOTS`) of Draw, Lists, Library and Publish at 1920 × 1080, in
   the PR for the owner to compare with mock-up screens a and the shell slide.

## Tester

Yes, for criteria 3–6 from the outside (control channel), because the shell changes every
surface the owner tries.

## Owner check

Click through the four workspaces on the owner's own project, and the developer switch.
