# UI architecture (U0)

Status: Owner questions answered (2026-11-13, §13); briefs 038 and 039 still Draft. Docs only.
Branch: `feature/ui-foundation`, from `origin/dev` `ee7bf71f`.
Inputs: [`ui-design-spec.md`](../discussions/ui-2026-11/ui-design-spec.md) (the design, decided),
[`owner-answers.md`](../discussions/ui-2026-11/owner-answers.md) (Carl's answers, decided), the
18-page mock-up deck (kept outside the repo, 3 MB), the code at `ee7bf71f`, and the in-flight
designs 035 (`origin/feature/mcp-bridge`) and 036 (`origin/feature/multi-level-terminals`).

This document says **how** the redesign is built. The spec says **what** it looks like; nothing
here re-opens it. Where this document names a size, colour or key, the spec is the source and wins.

The programme (owner, 2026-11-13): **U1** tokens, primitives and fonts (merges first) → **U2**
shell and **U3** table → **U4a** canvas, **U4b** wheel and commands, **U4c** inspector and
Standards, **U4d** agent banner and authors. Briefs: [037](../briefs/037-ui-tokens.md) (U1),
[038](../briefs/038-ui-shell.md) (U2), [039](../briefs/039-ui-table.md) (U3); U4 is outlined in §11.

## 0. Decisions at a glance

| # | Question | Decision |
| --- | --- | --- |
| 1 | What ECAD owns, what needs a seam | ECAD owns the whole window except the drawing area. Two new upstream seams, both generic: **US1 embedded drawing view** (CADCraft draws only its canvas into a rect the embedder gives it, with four switches), and **US2 font definitions builder** (CADCraft's UI font set as a value the embedder extends). The existing seams (owner overlay marks, owned documents, Tool Sets tabs, Properties section) stay; the last two fall out of use. §1 |
| 2 | Theme module and token table | Two new crates. **`ecad-tokens`** (Core: no dependencies) holds the one token table, used by the shell and by `ecad-cadcraft`'s canvas marks. **`ecad-ui`** (a new layer class, UiKit: egui, egui_extras, epaint and `ecad-tokens` only) holds the egui style, the nine primitives, the icon painters and, from U3, the table. §2, §3 |
| 3 | Fonts | IBM Plex Sans (UI) and Barlow Semi Condensed (sheet) are committed in `ecad/assets/fonts/` and embedded by `ecad-ui` with `include_bytes!` (owner, Q1: an exception to the craft-fonts rule; offering them to craft-fonts comes later). OFL-1.1, on the allowlist, with `ATTRIBUTION.md` rows. About 0.8 MB for five faces (estimate; U1 measures it). Always present, no fallback path. The page draws Barlow only from U4a, which needs CADCraft's text resolver to see ECAD's bytes (US3, §4). §4 |
| 4 | Shell layout | A pure layout function `layout(window, ShellState) -> Slots` (rects only, no egui context), drawn by `ecad-trial`'s `shell` module. State persists in eframe storage under its own key; per-page zoom is kept per project in the same storage, never in the `.ecad` file. Companion view is a split of the main view; pop-out to a second window is later. §5 |
| 5 | Table | One generic `Table<S: TableSource>` in `ecad-ui` on `egui_extras::TableBuilder` with virtualised rows. Sources are column sets in `ecad-trial` (Checks, Devices, then 036's Terminals). Rows are keyed by object ID, so selection and change tint survive sorting and updates. §6 |
| 6 | Command registry | Commands stay defined once, as CADCraft `CommandSpec`s installed by `ecad_cadcraft::install`; MCP already lists and runs exactly those (`list_commands`, `execute`). U4b adds an **action table** (UI-free, in `ecad-cadcraft`) that only says where a command appears: tool group, wheel slot per object kind, key, search words. A test fails if an action names a command that does not exist. §7 |
| 7 | Live updates | Every view pulls the project's change feed (`Project::change_count`, `changes_since` and siblings, which exist today) and applies its update map; a full rebuild only on open, reload, first show or when the feed has rolled over. The known redraw of the active page after every commit (`project.rs` `apply_tx` → `redraw_window`) is **not** fixed in U2: it is a narrow ECAD-core fix of its own (§8.4). |
| 8 | Tests | Headless and fast by default: layout maths, state and persistence, token contrast, doc-to-code sync and command tables as plain unit tests; widgets and shell flows through `egui_kittest` without a renderer, found by accessible label, as `ecad-trial` does today. Screenshots and the frame-time bench are slow and run only at the QA gate. §9 |
| 9 | Performance | Shell (everything but the canvas) ≤ 2 ms per frame; idle frame ≤ 8 ms, p95 ≤ 16 ms on the medium bench project (100 pages, larger than the owner's 43-page pack); a 10 000-row table scrolls with its body ≤ 3 ms per frame; a commit updates every open view in the same frame, with work proportional to what the commit touched. §10 |

FP items for the overseer: **none**. No op, no format field, no project property. §12.

## 1. Ownership and upstream seams

### 1.1 How the window is built today (`ee7bf71f`)

`ecad-trial` wraps CADCraft's whole window. `Trial::ui` (`apps/ecad-trial/src/lib.rs:1393`) draws
the Project panel as an `egui::Panel::left`, then calls `self.cad.ui(ui)`, which is CADCraft's
`CadApp::ui` (`crates/ui-egui/src/lib.rs:838`). That draws, in `CadApp::draw`
(`lib.rs`, after `ui_input`): the title bar and toolbar (`chrome::title_and_toolbar`), the
optional menu bar, status bar and file tabs, the Tool Sets strip, and a central panel with the
docking area (`docking::show`: the drawing, Tool Sets, Layers, Properties, External References),
then CADCraft's dialogs and prompts. The command line is an overlay inside the drawing
(`canvas.rs`, `cmdline::show` when `ui.show_command_line`), and typing anywhere reaches it
(`cmdline::keyboard`, called even when the overlay is hidden). Right-click on the drawing is
Enter while a command runs, otherwise CADCraft's shortcut, grip or snap menu (`canvas.rs`
around line 950). ECAD's own windows (sheets, reports, merge, renumber, preview, symbol search)
float over everything.

ECAD reaches CADCraft through four existing seams (`UPSTREAM-PATCHES.md`, `ECAD-SEAM(owner)`):
owner overlay marks (`ext::Mark`, brief 008), owned documents answering close and save through
their owner, Tool Sets tabs from the embedder (`extra_tool_tabs`), and a Properties palette
section from the embedder (`owner_properties`).

### 1.2 What ECAD owns from U2

Everything a person sees except the drawing area: top bar, agent banner, left panel, tool strip,
main view (including the page view's frame, the companion split, Lists, Library and Publish),
drawer, right panel, status bar, the Text panel, the wheel, flyouts, the hover card, search and
the refusal line. All of it is drawn by ECAD code with `ecad-ui` primitives.

CADCraft keeps: the drawing area (rendering, picking, snapping, grips, zoom and pan, the running
command's rubber band), the command engine and its interactive prompt machines, the
`Session`, the control channel and screenshots, its own modal prompts that protect data (close
and save-loss prompts, autosave notices), and the stroke font and text renderer on the sheet.

### 1.3 New seam US1: the embedded drawing view

One seam, in `crates/ui-egui`, marked `// ECAD-SEAM(owner): embedded drawing view.` It is
generic: any application that embeds CADCraft's drawing in a window of its own needs exactly
this, and nothing in it names ECAD.

```rust
/// How an embedding application uses the window. Default: CADCraft's own interface, unchanged.
pub struct Embedding {
    /// Draw CADCraft's title bar, toolbar, menu bar, status bar, file tabs, Tool Sets strip,
    /// docking area and start page. Off: `ui_embedded` draws only the drawing area.
    pub chrome: bool,
    /// Typing while no command runs starts commands through the hidden command line.
    /// Off: keys reach the command line only while a command is prompting (values, keywords,
    /// Enter, Esc); idle typing is left to the embedder.
    pub idle_typing: bool,
    /// Right-click on the drawing while no command runs opens CADCraft's shortcut, grip or snap
    /// menu. Off: the click is reported through `take_secondary_click` and no menu opens.
    /// Right-click while a command runs is unchanged (Enter).
    pub canvas_menus: bool,
    /// CADCraft applies its egui visuals and spacing (`theme::apply`) and installs its fonts.
    /// Off: the embedder owns the egui style and fonts; CADCraft still sets its canvas tokens.
    pub style: bool,
}

impl CadApp {
    pub embedding: Embedding,
    /// Everything `ui` does (frame start, input, the drawing area, CADCraft's protective prompts,
    /// capture) with the drawing filling `ui`'s rect; the chrome only when `embedding.chrome`.
    pub fn ui_embedded(&mut self, ui: &mut egui::Ui);
    /// The screen position of a right-click on the drawing this frame, when `canvas_menus` is off.
    pub fn take_secondary_click(&mut self) -> Option<egui::Pos2>;
}
```

Why each switch is needed (each was checked in the code, not assumed):

- **`chrome`**: `CadApp::draw` draws the title bar and toolbar unconditionally
  (`chrome::title_and_toolbar`), and the drawing is a dock pane, so there is no existing flag
  that leaves only the drawing. The existing `UiState` flags (`show_status_bar`,
  `show_file_tabs`, `show_toolsets`, `show_command_line`, `show_viewcube`, `show_ucs_icon`) are
  used too, but do not cover it.
- **`idle_typing`**: `canvas.rs` calls `cmdline::keyboard` "even when its overlay is hidden", so
  with the command line hidden a typed `L` + Enter would still start LINE. The spec's single
  letters (V W P C D B T, R, X, F) and Ctrl+K must belong to the shell.
- **`canvas_menus`**: egui computes the pointer state at the start of the pass, so the embedder
  cannot remove a right-click before CADCraft reads `secondary_clicked()`. The wheel needs it.
- **`style`**: `ui_frame_begin` re-applies CADCraft's visuals and fonts whenever the theme or
  language changes, which would overwrite ECAD's style for a frame at a time.

**The developer switch** is the same seam read the other way: with `chrome` on, `ui_embedded`
draws CADCraft's full interface inside the main view. ECAD keeps its shell around it. It is turned
on by `--cad-ui` on the trial's command line or the `ui.cadchrome` command in the Text panel
(Commands tab), is remembered with the shell state, and is never on by default. Nothing CADCraft
offers is deleted, so a developer can still reach layers, layouts and the command line.

What US1 does **not** do: it does not change any CADCraft default (`Embedding::default()` is
today's behaviour, and every existing CADCraft test runs with it), it adds no ECAD semantics, and
it does not touch the canvas renderer.

### 1.4 New seam US2: the font definitions builder

`theme::install_fonts(ctx, language)` (`crates/ui-egui/src/theme.rs:322`) builds egui's
`FontDefinitions` (system UI font, CJK, Arabic, Korean and craft-fonts fallbacks for the
interface language) and sets them. US2 splits it into a pure builder and the call that installs:

```rust
// ECAD-SEAM(owner): font definitions builder.
pub fn font_definitions(language: &str) -> egui::FontDefinitions; // what install_fonts sets today
pub fn install_fonts(ctx: &egui::Context, language: &str);         // unchanged behaviour
```

ECAD takes CADCraft's definitions for the language, puts IBM Plex Sans first in
`Proportional` and adds a named family for the sheet, and installs the result itself
(`Embedding::style` off). Without US2, ECAD would either lose CADCraft's script fallbacks or have
to copy them.

### 1.5 Seams that are not needed

- **Paper sheet colour**: CADCraft's drawing-area preferences already set the model background
  and selection colour (`theme::Canvas { model, paper, selection, .. }`, `theme::set_canvas`,
  applied each frame from `options.canvas()`), and the renderer already draws colour 7 black on a
  light background (`render/src/display.rs:76`). The paper colour is an option ECAD sets.
- **New markers** (disc with "!", triangle, related-placement outlines, agent's last write, the
  leader line to a companion row): drawn by ECAD with egui's painter over the drawing, from the
  public `cad.canvas.rect` and `cad.canvas.xf` that the trial already reads for hover. The
  existing `ext::Mark` seam keeps the marks it draws today. U4a decides per marker; neither path
  needs a new upstream variant.
- **Tool Sets tabs and the Properties section** (existing seams): with `chrome` off they are not
  shown. They stay for the developer switch and are not removed in this programme.

### 1.6 Upstream patch hygiene

Both seams are listed in `UPSTREAM-PATCHES.md` in the brief that adds them (US1 in 038, US2 in
037) and pass `cargo xtask patches`. Each comes with CADCraft-side tests (default unchanged;
embedded mode draws no chrome; idle typing off ignores letters but takes a prompt value; right
click reported and no menu). Both are offered upstream together once U4b has used every switch (owner, Q2).

## 2. Crates and layering

| Crate | Class (ecad `xtask layers`) | Depends on | Holds |
| --- | --- | --- | --- |
| `ecad/crates/ecad-tokens` (new, U1) | **Core** | nothing | The token table: shell colours, both sheet token sets, overlay meanings, type ramp, sheet text styles, spacing, sizes, radii, line weights, wheel geometry; `Icon` names. Plain data (`Rgb`, `f32`), `const`. |
| `ecad/crates/ecad-ui` (new, U1) | **UiKit** (new class) | `ecad-tokens`, `egui`, `epaint`, `egui_extras` | The egui style from the tokens, font registration, the nine primitives, icon painters (U1); the table (U3); the wheel widget (U4b). Knows nothing about projects. |
| `ecad/crates/ecad-cadcraft` | Adapter (unchanged) | + `ecad-tokens` | Canvas marks take their colours from `ecad-tokens` (U4a). The action table (U4b). |
| `ecad/apps/ecad-trial` | Ui (unchanged) | + `ecad-ui` | The shell (`src/shell/`), column sets (`src/lists/`), the page view's ECAD layer, moved panels. |

**UiKit** is added to `ecad/xtask` in U1: a UiKit crate may use egui, epaint and egui_extras but
no `cadcraft-*` crate and no ECAD crate other than `ecad-tokens`. That keeps the primitives and
the table swappable, testable in seconds and buildable for wasm, and it keeps CADCraft's
`cadcraft-ui-egui` out of anything that only draws ECAD widgets.

`ecad-tokens` and `ecad-ui` are added to the xtask's wasm list (`WASM_CRATES`) so the token
table and widgets cannot pick up native-only code. The ECAD app itself is native only today.

## 3. Tokens, style, primitives and icons (U1)

### 3.1 The token table

`ecad-tokens` holds spec §4 as Rust constants, one module per table:

- `shell`: `GRAPHITE: Shell { bg, panel, raised, hover, active, line, line_strong, text, text_2,
  text_3, accent, accent_text, accent_soft, ok, warning, error, provisional, agent, cable,
  connect }`.
- `sheet`: `PAPER: Sheet` and `DARK: Sheet`, each `{ sheet, ink, ink_2, grid, connection_point,
  wire_end_attached, loose_end, cable, error, warning, provisional, terminal_link, selection,
  placement_preview, agent_write }`, plus whether each meaning prints (`prints(meaning)`), so the
  rule "ink prints, overlay never does" is data, not a convention.
- `marker`: shape and screen size per meaning (8 px ring, 5 px dot, 7 px square …).
- `wire`: default category colours on each sheet; the two-colour dash (2 mm on, 2 mm off).
- `ui_type`: Caption, Small, Body, Title, Object (size, line height, weight).
- `sheet_type`: Tag, Label, Terminal label, Title block label, Zone label, Page title, Page
  number as **cap heights** in mm, with `font_size_mm = cap / 0.70` (owner answer 9). U4a
  points 031's registry defaults at these values (§11).
- `space`, `size`, `radius`, `line`, `wheel`, `shadow`.
- `icon`: the `Icon` enum (names only).

Each token's name in code is the spec's name (`text-2` → `text_2`). `ecad/docs/design/tokens.md`
is rewritten in U1 to the new names; a unit test reads its tables and compares every name and
value with the constants, so the doc and the code cannot drift. Old `ecad.*` names that the
canvas still uses keep a row marked "retired in U4a" until U4a moves them.

Every text token is tested against the background it is used on for 4.5:1 contrast (3:1 for
`text_3` labels on `raised`, as the spec states 4.6:1), so a later colour change cannot quietly
fail contrast.

### 3.2 Style

`ecad_ui::style::apply(ctx)` sets egui's `Visuals` and `Spacing` for both egui themes from
`GRAPHITE` (the shell has one appearance; it does not follow the OS light or dark setting), sets
the text styles from `ui_type`, and installs fonts (§4). It runs once at start and again only if
egui reports a changed `pixels_per_point`. With US1's `style` switch off, CADCraft never
overrides it.

**No local styling.** From U1 on, ECAD UI code takes colours, sizes and fonts only from
`ecad-tokens` through `ecad-ui`. U1 adds a source check to `ecad/xtask layers`: outside
`ecad-tokens` and `ecad-ui`, ECAD UI sources may not contain `Color32::from_rgb`,
`Color32::from_gray`, `FontId::new` or `egui::Rounding`/`CornerRadius` literals. Existing
offenders are listed in an allowlist that only shrinks, as the xtask already does for dev
features (`DEV_FEATURE_ALLOWLIST`); U2 and later remove entries as they restyle.

### 3.3 The nine primitives

Each is a function returning `egui::Response` (or a small enum of what happened), with an
accessible label so tests and the control channel find it. Sizes come from `size` and `radius`.

| Primitive | Signature (sketch) | States |
| --- | --- | --- |
| Button | `button(ui, label, Kind::{Primary, Secondary, Danger, Quiet})` | normal, hover, active, disabled (with a reason shown on hover) |
| Icon button | `icon_button(ui, Icon, label)`; 36 px square in the tool strip, 28 px elsewhere | as button, plus `selected` |
| Pill | `pill(ui, text, Tone)`; full radius | tones: neutral, ok, warning, error, provisional, agent |
| Chip | `chip(ui, text)` with optional close | normal, selected |
| Badge | `badge(ui, text_or_count, Tone)`; 4 px radius | tones as pill; the origin badge of U4c is a badge |
| Field | `field(ui, &mut String, Field::{Text, Search, Number})`; 28 px (32 px search) | normal, focus ring 2 px `accent_text`, invalid (error line under it), read-only |
| Segmented control | `segmented(ui, &mut usize, &[&str])` | the workspace switch is one |
| Key cap | `keycap(ui, "Ctrl+K")` | none |
| Row | `row(ui, RowSpec { height, selected, hovered, tint })`; 28 px list row, 30 px table row, 32 px group row | normal, hover, selected (`accent_soft`), change tint (§8.3) |

The **set-versus-read** rule (spec §5, "What you set and what is read") is part of the field and
row primitives: `field` is the only input look; `read_value(ui, text, go)` draws plain
`text_2` with no box and makes it a go-to. Provisional values use `provisional(ui, text)`: the
provisional colour with a dotted underline, everywhere (page, list, inspector).

### 3.4 Icons

`ecad_ui::icons::paint(painter, rect, Icon, colour)` draws each icon from lines, arcs and
circles on a 16 px grid, 1.5 px stroke, round caps and joins (spec §4). One function per icon, in
`ecad-ui/src/icons/`. Each icon is original, drawn in code, so it needs no `ATTRIBUTION.md` row;
none is traced from any product. CADCraft's `crates/ui-egui/src/icons.rs` is not reused, so the
two sets can evolve apart. U1 delivers the icons the shell needs (seven tool groups, workspaces,
search, checks, agent, history, pages, devices, collapse, close, chevrons); later briefs add
their own in the same module.

## 4. Fonts (U1)

| | UI | Sheet |
| --- | --- | --- |
| Family | IBM Plex Sans | Barlow Semi Condensed |
| Faces | Regular 400, Medium 500, SemiBold 600 | Regular 400, SemiBold 600 |
| Used for | All shell text (spec §4 type ramp) | Page text and reports (spec §4 sheet text styles), from U4a |
| Licence | SIL Open Font License 1.1 (`OFL-1.1`, on the house allowlist) | `OFL-1.1` |
| Size (estimate) | about 0.2 MB per face as TTF | about 0.1 MB per face |

**Where the files live (owner, 2026-11-13, Q1).** In ECAD's own repository, under
`ecad/assets/fonts/`: the five TTF files taken unmodified from the official sources (IBM Plex
releases, Google Fonts' `ofl/barlowsemicondensed`), each family's `OFL.txt`, and a `README.md`
with each file's source URL, version and SHA-256. Each family has a row in `ATTRIBUTION.md`.
Offering them to `storytold/craft-fonts` is a later step, approved separately.

This is an owner exception to the house rule (Rust block 3) and the root `AGENTS.md` ("font files
live in `storytold/craft-fonts`, never in this repo"). It is recorded here and in brief 037's
status line. When the fonts move to craft-fonts, `ecad/assets/fonts/` goes and the bytes come from
there.

**How they reach the binary.** `ecad-ui::fonts` embeds them with `include_bytes!` (native and
wasm), so they are always present: no build input, no `CRAFT_FONTS_DIR` dependency, and no
fallback path. It registers them with egui through US2's builder, on top of CADCraft's
definitions for the language (which keep their script fallbacks), and exposes two egui families,
`ui` (Plex, also first in `Proportional`) and `sheet` (Barlow). Nothing is read from disk at run
time. A test checks both families are registered, and that the `README.md` hashes match the
committed files.

**The sheet font on the page (U4a).** The page text is drawn by CADCraft's text renderer. 031
resolves the project font by family name through CADCraft's resolver
(`cadcraft_fonts::TextFont::resolve_style`), which finds only faces in its own static
`CRAFT_FONTS` table (built from `CRAFT_FONTS_DIR`), and has no way to register a face at run time
(checked: `crates/fonts/src/craft_fonts.rs`). So egui can show Barlow in the shell from U1, but the
**page** can draw Barlow only once U4a gives the resolver ECAD's bytes. U4a's designer chooses
between a small generic upstream seam (US3: faces an embedder registers before first use, found
by family name like craft-fonts faces) and waiting for the craft-fonts move; the seam is the
recommendation, since it keeps the bytes in one place. Until then, the page keeps 031's font chain
(BIZ UDPGothic when craft-fonts is embedded, otherwise the stroke font). Switching the default to
Barlow Semi Condensed is a default in 031's registry, so no stored project changes.

**Size.** About 0.8 MB (estimate; U1 records the measured size), in ECAD's binaries only, and in
the wasm build of `ecad-ui` too.

**Size on screen.** Body text is 13 px; egui renders at the monitor's `pixels_per_point`. Sheet
text sizes are millimetres on paper (cap heights), never enlarged for the screen; a page opens at
the zoom where a tag is 13 px tall (spec §4, about 135 %) and remembers its zoom (§5.4).

## 5. Shell layout model (U2)

### 5.1 Slots

```rust
pub enum Workspace { Draw, Lists, Library, Publish }
pub enum LeftTab { Pages, Devices, History }
pub struct ShellState {
    pub workspace: Workspace,
    pub left: LeftState { tab: LeftTab, collapsed: bool, width_per_tab: [f32; 3] },
    pub drawer_open: bool,
    pub right_collapsed: bool,
    pub companion: Option<Companion { view: CompanionView, ratio: f32 }>,
    pub text_panel: bool,
    pub cad_chrome: bool,       // the developer switch (§1.3)
}
pub struct Slots {
    pub top_bar: Rect, pub agent_banner: Option<Rect>, pub left: Rect, pub tool_strip: Option<Rect>,
    pub main: Rect, pub companion: Option<Rect>, pub drawer: Rect, pub right: Rect, pub status: Rect,
}
pub fn layout(window: Rect, state: &ShellState, agent: bool) -> Slots;
```

`layout` is pure arithmetic on `egui::Rect` (no `Context`), so every rule below is a table-driven
unit test. `shell::show` asks for the slots, then fills each with `egui::Panel` / child `Ui`s
bounded to the slot's rect (`allocate_new_ui` with the rect), in a fixed order: top bar, banner,
status bar, left, right, drawer, tool strip, main. The main view hosts the page through US1's
`ui_embedded`, or Lists, Library or Publish.

Workspaces change only the tool strip and the main view (spec §2). The tool strip has no slot in
Lists and Publish (`tool_strip: None`), so the main view widens; Library has the builder's strip
only in the builder (U4 and later).

### 5.2 Sizes and the 1920 × 1080 budget

From spec §2, at 100 % interface scale:

| Region | Width | Height |
| --- | --- | --- |
| Top bar | full | 44 |
| Agent banner (only while connected) | full | 32 |
| Left panel | 264 (drag 200–420; rail 44 when collapsed; width kept per tab, History defaults to 400) | rest |
| Tool strip (Draw, Library builder) | 48 | rest |
| Right panel | 340 (380 in the builder); rail 44 when collapsed | rest |
| Drawer | main view's width | 150 open, 36 collapsed |
| Status bar | full | 26 |

Draw at 1920 × 1080 with nothing collapsed: main view **1268 × 824** with the drawer open
(1080 − 44 − 26 − 150 − 36 for the drawer's header row is not counted twice: the 150 includes
it), **1268 × 974** with it collapsed to 36; with the agent banner, 32 less. Lists (no tool
strip): 1316 wide. With the left panel at 420 (History), Draw's main view is 1112.

**Smaller windows.** The minimum window stays 900 × 560 (`window.rs`). When the main view would
fall below 760 px wide, the shell collapses, in this order: the right panel to its rail, then the
left panel to its rail, then the drawer. Each collapse is a view state, not a saved preference:
widening the window restores what the user had. 1366 × 768 is usable, not specially designed
(owner, Q3).

### 5.3 Resizing

Left and right panels resize by dragging their inner edge (egui's panel resize, bounded by the
table above). The companion split has a 6 px drag handle; each half keeps at least 360 px, and
the split closes when the main view cannot hold two. Double-clicking a handle returns it to the
default. Nothing else resizes. The drawer has two heights only.

### 5.4 Persistence

- **Shell state** (`ShellState`): eframe storage, key `shell`, next to the existing window state
  (`window.rs`, key `window`, `APP_ID` `ecad-trial`). Saved on change (debounced to once a
  second) and on exit. An unreadable value falls back to the default and records one line in the
  command history; it never stops start-up.
- **Per page, per project** (zoom and centre of each page, the last page shown, the last
  workspace): the same storage, key `views`, a map from the project's stable ID to its pages'
  views, at most 32 projects (least recently opened dropped). Not in the `.ecad` file: a view is
  how one person looks at a project, not part of it, and it needs no format change.
- **Not persisted**: hover, selection, an open wheel or flyout, scroll positions inside tables,
  the companion's pop-out.

### 5.5 Companion view

`Companion { view: Table(ListTab) | Page(PageId), ratio }`. In Draw it is a table beside the
page; in Lists a page beside the table (spec §2). Both halves are the normal components (the page
view, the table), not copies. Selection is shared: one selection set in the shell, which both
halves read. The one-second leader line between a selected row and its object (spec §5) is drawn
by the shell over both halves. Pop-out to its own window (egui's deferred viewport) is out of U2
and U3; it comes when a second monitor is in the owner's trial (§11).

### 5.6 What moves where in U2 (contents unchanged)

| Today | U2 slot |
| --- | --- |
| Project panel ▸ Pages tab (`project_panel.rs`, virtualised tree) | Left ▸ Pages |
| Project panel ▸ Devices tab (`devices_panel.rs`) | Left ▸ Devices |
| Project panel ▸ History tab (`history_panel.rs`) | Left ▸ History |
| Project panel ▸ Checks tab (`checks_panel.rs`) | Drawer (the left panel keeps only Pages, Devices, History) |
| Project panel header: New, Open, Save, Save As, Close, status | Top bar ▸ project menu; save state in the status bar |
| Revision badge (`release_badge`) | Top bar ▸ revision pill |
| ECAD section of CADCraft's Properties palette (`sheets_ui`, `sync_props`) | Right panel |
| Developer fold: ECAD command field and debugger (`developer.rs`) | Text panel ▸ Commands, over the right panel (Ctrl+/) |
| Reports window (`outputs_ui.rs`) | Lists main view, as it is (U3 converts the tables) |
| Release and Print Book (`release_ui.rs`) | Publish main view, as it is |
| Symbol search (`search.rs`) | Library main view, as it is |
| ECAD Tool Sets tab (`tools.rs`) | Tool strip: the seven group buttons start today's tools that exist; flyouts list them (U4b fills the groups and removes the old buttons) |
| Sheets, merge, renumber, preview windows | Stay floating windows until their U4 home; listed as known exceptions to "no dialog" |
| CADCraft chrome, command line, layers, layout tabs | Hidden (US1); developer switch |

## 6. The table component (U3)

### 6.1 Shape

```rust
pub trait TableSource {
    type Key: Copy + Ord + Hash;               // the object's ID: row ↔ object link
    fn columns(&self) -> &[Column];
    fn keys(&self) -> &[Self::Key];             // all rows, the source's own order
    fn cell(&self, key: Self::Key, col: ColumnId) -> Cell;
    fn changed_since(&mut self, seen: u64) -> Changed<Self::Key>; // Some(keys) or All
}
pub struct Column { id, title, width: Width, kind: Set(Editor) | Read, align, sort: Option<SortKey>,
                    group: bool, filter: Option<FilterKind> }
pub enum Cell { Text(String), Number(f64, unit), Input(String), Read { text, go: Option<GoTo> },
                Provisional(String), Swatch(Vec<Rgb>), Badge(text, Tone), Lanes(LaneCell), Empty }
pub struct Table<S: TableSource> { view: ViewModel<S::Key>, selection: Selection<S::Key>,
                                   tint: TintMap<S::Key>, edit: Option<CellEdit<S::Key>> }
pub enum TableEvent<K> { Selected(Vec<K>), GoTo(GoTo), Commit { key: K, col: ColumnId, value: String },
                         Open(K), Accept(Vec<K>) }
```

`ecad-ui` owns `Table`, `ViewModel`, `Selection`, `TintMap` and drawing; it never runs a
command. The owner of a table (`ecad-trial`'s column set) turns `TableEvent`s into commands
(`app.run(id, params)`), so an inline edit is the same command, the same transaction and the same
refusal as on the page (spec principle 6). The table never holds its own copy of a value: after
a commit, the source reports the touched keys and the table re-reads those cells.

### 6.2 Features

| Feature | How |
| --- | --- |
| Virtualised rows | `egui_extras::TableBuilder` with `body.heterogeneous_rows`, which lays out only visible rows; heights are 30 px (rows) and 32 px (group headers), so the height list is two numbers per group and costs microseconds for 10 000 rows. Columns resizable, header sticky. |
| View model | `ViewModel` is the visible order: `Vec<Row>` where `Row = Group { key, count, open } \| Item(Key)`, built from `keys()` by filter → group → sort. Rebuilt only when the filter, grouping or sort changes or on `Changed::All`; touched keys are patched in place (§8.3). |
| Sort | Click a header: ascending, descending, source order. Stable; natural order for tags (`-K2` before `-K10`). One sort column (a second is Shift-click, later if asked). |
| Filter | A search field in the tab header matches any shown text; a filter chip per column kind (for example "Provisional", "Has a check", "Not on a page yet"). Filters compose with AND. |
| Group | By one column chosen in the header menu; groups collapse; counts in the group row. |
| Multi-select | Click, Shift-click range, Ctrl-click toggle, Ctrl+A in the table; keyboard up and down. Selection is a set of keys, so it survives sort and filter. |
| Inline edit | Only `Set` columns. F2 or Enter or double-click starts; Enter commits one command; Esc reverts; Tab goes to the next set cell. A refusal shows under the cell (spec §5 refusals) and the value reverts. |
| Go-to | A `Read` cell with a target, or the row's go-to button: switches workspace if needed, shows the object, records Back (Alt+Left). |
| Row ↔ object link | The key is the object's ID. Selecting rows selects their objects on the page (and in a companion page); selecting on the page scrolls the table to the first selected row and highlights it. |
| Change tint | Cells whose value changed in a commit are tinted for 1.2 s (spec §5), only for touched keys, by comparing old and new `cell` values. |
| Proposal state (later, M2) | Row states added, changed, removed with Accept per row or all. The `Row` enum has room for it; not built in U3. |
| Export | Header button, CSV, from the same rows and columns. U3 builds CSV only; XLSX is M3. |

### 6.3 Column sets in U3

1. **Checks** (read-mostly): the drawer's list and the Lists ▸ Checks tab (the drawer shows the
   current page's checks, the tab all of them). Source: `ecad_cadcraft::checks` kept list,
   updated by `update_checks` (design 022 §11).
2. **Devices**: Lists ▸ Devices. Source: `device_list` (with `device_changes_since`).
3. **Terminals** (036): Lists ▸ Terminals, laid out as a terminal plan from design 036 §9's row
   model (`stripplan` rows with `editable` field names): inside columns right-aligned, the strip
   in the middle, outside left-aligned; editable cells (Position, Label, Rating, links) as inputs.
   The link lanes are a `Lanes` cell drawn by `ecad-ui`. This step waits for 036 to be in `dev`.

## 7. One command registry (U4b, decided now)

### 7.1 What exists

- ECAD's commands are CADCraft `CommandSpec`s, installed once by `ecad_cadcraft::install()`
  (`cadcraft_engine::cmd::set_extension_commands(commands::specs)`). Each has an id, label,
  parameter docs, a JSON `run` and optionally an interactive prompt machine.
- MCP (`apps/ecad-mcp`) does not define commands: its `execute` tool runs any registered command
  by id with JSON params, and `list_commands` lists them. Its own tools (`ecad_project`,
  `ecad_history`, `ecad_inspect`, `save`) are reads and the save.
- The menus, ECAD's Tool Sets tab (`tools.rs`) and the Developer field start the same commands.

### 7.2 What U4b adds: the action table

A command says **what** happens; an action says **where a person finds it**. The action table is
data, in `ecad-cadcraft/src/actions.rs` (UI-free, so the MCP server and tests can read it):

```rust
pub struct Action {
    pub id: &'static str,             // "wire.fixnumber"
    pub command: &'static str,        // the CommandSpec id it runs, e.g. "fixwirenumber"
    pub params: Params,               // fixed params, or "from the selection / object under the cursor"
    pub label: &'static str,          // "Fix number"
    pub icon: Icon,                   // from ecad-tokens
    pub group: Option<(ToolGroup, u8)>,          // tool strip group and order in its flyout
    pub wheel: &'static [(ObjectKind, Slot)],    // spec §5: fixed positions per kind
    pub key: Option<Key>,             // V W P C D B T, R, X, F2, Del …
    pub search: &'static [&'static str],         // extra words for Ctrl+K
    pub enabled: fn(&Context) -> Enabled,        // Ok, or Greyed { why, next: Option<&'static str> }
}
```

- **Tool groups** (V W P C D B T) are the actions with a `group`, in order; the group's key picks
  the last used and steps (owner answer 7).
- **The wheel** for a kind is the actions with that kind in `wheel`, at their fixed slot; an
  empty slot stays empty (spec §5). Slot meanings are checked by a test: bottom is always a
  delete, right always a go-to, bottom-right always "show its row".
- **Keys** are the actions' `key`s; a test fails on any clash within one context.
- **Ctrl+K** searches pages, tags, symbols (today's `search.rs`) and actions (label + `search`).
- **The Text panel** (Commands tab) takes command ids and JSON, exactly what MCP takes.
- **MCP** keeps exposing commands, not actions. An agent and a person run the same command and
  get the same refusal; the action table adds nothing an agent needs. If agents later want "what
  can I do with this object", a read tool can list `enabled` actions from the same table.

**Nothing is defined twice**: a test checks every `Action::command` is a registered command id
and every user-visible ECAD command appears in at least one action or in an explicit
`PROGRAMMATIC_ONLY` list with a reason. Removing an old Tool Sets button (spec §6 step 5) is
safe when the action table covers its command.

## 8. Live updates without a full rebuild

### 8.1 The rule

Owner decision 2026-11-08 (`ecad/CLAUDE.md`, "Derived views scale with what changed" and "No
lazy default"): every view states its update map, is updated from the objects a commit touched,
and rebuilds fully only on open, reload or first show. No refresh buttons.

### 8.2 The mechanism (exists; extended, not replaced)

`Project` keeps a bounded change feed: `change_count()` (a counter callers compare every frame)
and `changes_since(n)` / `device_changes_since(n)` (`project.rs` around line 1705), built from
each commit's `Change { n, pages, devices, props, tree }` (`record_change`, line 1767;
`CHANGE_LOG` = 64). Checks and reports are kept models updated from each transaction's ops
(`update_checks`, `update_outputs`).

Every UI view follows one pattern:

```rust
let now = project.change_count();
if now != self.seen {
    match source.changed_since(self.seen) {          // update map: what this view shows
        Changed::Keys(k) => self.patch(k),            // only those rows, cells, pages
        Changed::All     => self.rebuild(),           // feed rolled over (> 64 changes unseen)
    }
    self.seen = now;
}
```

Each U brief lists its views' update maps in a table (op kind and target → what the view does;
every other op: nothing), as the designer's rules require. The views in U2 and U3:

| View | Shows | Changed by | Does |
| --- | --- | --- | --- |
| Pages tab | Tree, numbers, titles, changed-elsewhere dot | `Change.tree`; `Change.pages` for the dot | Re-read the tree rows (`tree_rows`) only when `tree`; set the dot on touched pages that are not shown |
| Devices tab and Devices table | Device rows | `device_changes_since` | Patch touched devices; add and remove rows |
| History tab | Transactions | A new head | Append the new transactions (and their author, 035) |
| Right panel | The selected object or page | `Change.props` containing the selection | Re-read the selected object's rows |
| Checks drawer and table | Checks | The kept checks model's revision | Patch rows the update changed |
| Status bar | Save state, counts | Save events; the checks count | Re-read the two values |
| Top bar pills | Checks count, revision, agent | As status bar; release events; banner state | Re-read |

### 8.3 Change tint

A view that patches rows compares each touched key's cells before and after and records
`(key, column) → changed_at` in a `TintMap`. The row primitive draws the tint for 1.2 s, fading;
the shell asks egui for a repaint only while a tint is live (`request_repaint_after`), so an
idle window costs nothing. Counts in pills tick over the same way.

### 8.4 The known issue: the active page redraws after every commit

`Project::apply_tx` (`crates/ecad-cadcraft/src/project.rs:2038`) ends with
`self.redraw_window(pg, Some(command))`, which re-projects the **active page** after every
commit (`redraw_window`, line 2004, calls `refresh_page` for the active page unconditionally).
The pages a commit touched are already known (`pages_touched`, the references' pages, the
numbered pages, merged into `Change.pages`), but `redraw_window` does not consult them. (`refresh_page` replaces the engine
drawing only when the projection differs, so the cost is the projection itself, paid on every
commit, including commits that touched other pages only.) The fix:
re-project the active page only when it is in the commit's touched pages, or when the frame is
stale (`frame_stale`), or when it is a **generated page** whose content changed; for that last
case `update_outputs` already works out `shown_changed` for the shown generated page and sets
`frame_stale`, so generated pages must be added to the touched set (the BOM, terminal-plan and
cable-schedule pages whose rows changed) rather than only to `frame_stale`.

**U2 does not fix it.** It is in ECAD's core commit path (`ecad-cadcraft`, `project.rs`), which
035 (author per commit) and 036 are editing now, and its tests belong with the projection tests,
not the shell. It does not block U2: the shell's own frame time does not depend on it. It should
be a narrow fix brief of its own (overseer item O1, §12), after 035 and 036 merge and before U4a,
because U4a's canvas work and its perf targets (§10) assume it.

## 9. Test strategy

Fast by default; nothing slow runs in an ordinary `cargo test`. The tests follow brief 017's
layout (one `tests/all` executable per crate, modules per area).

| What | Where | How | Speed |
| --- | --- | --- | --- |
| Token table: names and values equal `tokens.md`; contrast of text tokens; `prints` per meaning; cap-height to font-size conversion | `ecad-tokens` unit tests | Plain Rust | ms |
| Primitives: states, accessible labels, set-versus-read rendering, disabled reasons | `ecad-ui` `tests/all/primitives.rs` | `egui_kittest::Harness` without a renderer (no wgpu), queried by label and role | well under 1 s for the file |
| Icon painters: every `Icon` paints, within its rect, with the token stroke | `ecad-ui` | Collect shapes from a painter; check bounds and stroke widths | ms |
| Layering and no local styling | `ecad/xtask layers` | Manifest and source scan | s, already in CI |
| Shell layout: every row of §5.2, collapse order, minimums, companion split | `ecad-trial` unit tests on `layout` | Plain Rust on rects | ms |
| Shell state: save, load, corrupt value falls back, per-project views capped at 32 | `ecad-trial` | Plain Rust (serde) | ms |
| Shell flows: workspace switch (Ctrl+1–4), left tabs, drawer, Text panel (Ctrl+/), developer switch shows CADCraft's chrome, keys only over the main view | `ecad-trial` `tests/all/shell_ui.rs` | `egui_kittest` with the trial headless at 1920 × 1080, as `cable_references.rs` does today | under 2 s per test |
| US1 and US2 | `crates/ui-egui` tests | CADCraft's own harness: default unchanged; embedded draws no chrome; idle typing; right-click reported | CADCraft fast set |
| Table: view model (filter, group, sort, patch), selection across sort, tint lifetime, edit commit and revert, 10 000-row source builds the view model within budget | `ecad-ui` `tests/all/table.rs` | Plain Rust for the model; kittest for edit and keys | fast; the timing check uses a generous bound and is not a benchmark |
| Column sets: Checks and Devices rows equal today's panels' rows on the same project; go-to and edit run the expected command | `ecad-trial` | kittest on a small fixture project | under 2 s each |
| Update maps: a commit touching one device patches one row and rebuilds nothing; a commit touching nothing leaves every view's build counter unchanged | `ecad-trial` and `ecad-cadcraft` | Counters, as 031's update-map counter test does | fast |
| Screenshots against the mock-ups | `ecad-trial`, the existing `shot()` helper (`tests/all/mouse_tester.rs`) | egui-wgpu without a display, written only when `ECAD_TESTER_SHOTS` is set and an adapter exists, as today; never asserted in CI, never committed; compared by a person with the mock-ups | off by default; the tester and the QA gate turn it on |
| Frame time and table scroll | `ecad-bench` (a `ui` measure) | Release build, headless frames | QA gate only (`cargo xtask bench`) |

Harness facts this relies on (checked): `ecad-trial`'s tests build the trial in
`egui_kittest::Harness::builder().with_size(..).build_ui_state(|ui, t| { t.logic(ctx); t.ui(ui) })`
(`mouse_tester.rs` `harness_with`), click by AccessKit label and run command lines; they do not use
the control channel. Brief 017's whole ECAD run took 26 s, almost all of it linking egui and wgpu,
which is why `ecad-tokens` and `ecad-ui` tests must not link `ecad-trial` or wgpu.

The tester, when one is used (§11 says where), checks a brief's acceptance criteria from the
outside: the control channel (`docs/control-protocol.md`) and screenshots read by eye.

## 10. Performance targets

All on the owner's Windows machine class, release build, 1920 × 1080, 100 % scale. The bench
uses the **medium** tier (100 pages, 1 000 components; `ecad-bench` `TIERS`), which is larger
than the owner's 43-page pack; the owner confirms on that pack by hand. "Shell" means everything
but CADCraft's canvas.

| What a user would notice | Target | Measured by |
| --- | --- | --- |
| Moving the mouse over a page, idle | Frame ≤ 8 ms; p95 ≤ 16 ms (60 fps) | `CadApp::frame_ms` plus the shell's own timer, 300 frames |
| The shell's share of a frame | ≤ 2 ms, independent of project size (small vs medium within 20 %) | Shell timer |
| Switching workspace (Ctrl+1–4) or left tab | Next frame; ≤ 50 ms including any first show | Timer around the switch |
| An edit appears everywhere | Same frame as the commit, in every open view; view update work ≤ 1 ms for a commit touching ≤ 100 objects, independent of project size | Per-view counters and timers |
| Opening Lists ▸ Devices on the medium project (first show, full build) | ≤ 50 ms | Timer |
| Scrolling a 10 000-row table | Table body ≤ 3 ms per frame; no frame over 16 ms | Bench with a synthetic 10 000-row source |
| Sorting or grouping 10 000 rows | ≤ 20 ms (one frame may run long once) | Bench |
| Typing in a table filter, 10 000 rows | ≤ 10 ms per keystroke | Bench |
| Start-up | Shell and fonts add ≤ 50 ms to the first frame | 015's startup timing |
| Idle | No repaints while nothing changes and no tint is live (0 % CPU at rest) | Repaint counter over 2 s |

## 11. The U4 lanes (outline, not briefs yet)

| Lane | Scope | Depends on | Touches |
| --- | --- | --- | --- |
| **U4a Canvas** | Paper sheet (CADCraft's canvas preferences) and the dark sheet as a second token set; ink versus overlay split, so overlay never prints; the spec's markers and selection with related placements (§1.5); two-colour wires and cores drawn in both colours; Barlow Semi Condensed as the sheet font (registry default); 031's sizes converted to cap heights (owner answer 9: font size = cap / 0.70); text styles defined once against the Standards registry; page opens at the readable zoom and remembers it (§5.4); mono or colour print as a registry setting (owner answer 1). | U1; U2 (page view slot); the redraw fix (O1); US3 or the craft-fonts move for the page font (§4) | `ecad-cadcraft` (look, overlay, text styles, defaults), `ecad-trial` page view |
| **U4b Wheel, intent line, tools** | The action table (§7); the wheel widget (`ecad-ui`) with slot tables per object kind; the seven tool groups with flyouts and last-used memory; the intent line (output only) with the electrical preview text; the refusal line; Ctrl+K over actions; removing old Tool Sets buttons as their actions land. Right-click while a tool runs finishes the current step; the wheel opens only when idle (owner, Q4). | U1; U2 (tool strip, US1's `canvas_menus` and `idle_typing`) | `ecad-cadcraft/src/actions.rs`, `ecad-ui` wheel, `ecad-trial` shell and tools |
| **U4c Inspector and Standards** | Right panel as the inspector: origin badges from 031's registry (built-in → standard → project → page → object, page level only for function, location and sheet, owner answer 4), overridden and reset; part sets display. Settings over the window with the **Standards** screen (roadmap 1.13(b)): the U3 table listing every default with its origin, the detail chain on the right, consequences stated before a change. | U1; U2 (right panel, Settings overlay); U3 (table); roadmap 1.13(a) style model for catalogues beyond what 031's registry already holds | `ecad-trial` inspector and settings; read-only use of `defaults/` |
| **U4d Agent banner and authors** | Restyle 035's banner (its data contract, design 035 §7.1, element names kept) into the shell's banner slot and top-bar agent pill; History rows show author chips and group an agent session for Undo session; "agent's last write" outline on the page (with U4a's marker). | 035 merged; U1; U2 (banner slot) | `ecad-trial/src/agent/banner.rs` (from 035), History tab |

U4a, U4b, U4c and U4d can run at the same time once U2 is in `dev`, provided each keeps to its
files: U4a owns the page view and canvas code, U4b the tool strip, wheel and action table, U4c
the right panel and Settings, U4d the banner and History. Shared touch points (the shell's
`show` order, `ShellState`) are append-only, coordinated by the overseer.

### 11.1 U4 file ownership (briefs 040–043, 2026-11-13)

Each lane owns its new files and the files listed as owned in its brief's §0. The per-lane layout
(#72) covers tests (`tests/all/lanes/lane_04N.rs`), commands (`ecad-cadcraft/src/lanes.rs`),
quick start (`docs/quick-start/04N-*.md`) and new `ecad.*` tokens (the lane's design section).
These files are shared, and each lane touches only the hunk named:

| Shared file | 040 canvas | 041 wheel and tools | 042 inspector and Standards | 043 agent |
| --- | --- | --- | --- | --- |
| `ecad-trial/src/shell/mod.rs` | `main_view`: one `canvas::paint` call | `shell_ui`: the `show_command_line` line; `main_view`: the `take_secondary_click` line | `settings_overlay` removed, one `settings::show` call | `shell_ui`: banner block; `main_view`: one `last_write::paint` call |
| `ecad-trial/src/lib.rs` | `pub mod canvas;` | — | `pub mod settings;`; popout openers | — |
| `ecad-trial/src/shell/top_bar.rs` | — | search handler | — | `agent_pill` |
| `ecad-trial/src/shell/state.rs` | `sheet` field | — | — | — |
| `ecad-trial/src/shell/layout.rs` | — | `intent` row | — | — |
| `ecad-trial/src/shell/text_panel.rs` | `ui.sheet` arm | — | — | — |
| `ecad-cadcraft/src/interact/mod.rs` | `overlay_marks` body | two `mod` lines | — | — |
| `ecad-cadcraft/src/lib.rs` | — | `pub mod actions;` | `mod default_cmds;` | — |
| `ecad-trial/src/project_panel.rs` | — | one `Action` variant appended | — | — |
| `ecad-tokens/src/icon.rs` | — | appended block `// 041` | appended block `// 042` if needed | — |
| `ecad/UPSTREAM-PATCHES.md` | appended U4a block (US3, US1 line) | — | — | — |

Order: 041 and 043 start now; 042 starts when `fix/ui-followups` has merged (it moves the popout
code that branch fixed); 040 starts when the redraw fix (O1) has merged. Go-tos that select a
device's symbols use `Trial::goto_symbols` from `fix/ui-followups`. Merge order
does not matter for correctness; the merge train takes them as they are ready and merges `dev`
into each remaining branch locally (the union merge of the `lanes` files needs a local merge).

Later, not in this programme: the companion pop-out window; Library and Publish main views
beyond hosting today's content (symbol builder 1.1, report generator M2); the proposal row
state for list import (M2).

## 12. Items for the overseer

- **FP (format) items: none.** No op, no stored field, no project property. Shell state and
  per-page views live in eframe's app storage, not in the project.
- **O1. Redraw fix (§8.4).** A narrow fix brief in `ecad-cadcraft`: the active page re-projects
  only when touched (generated pages included). After 035 and 036 merge, before U4a. Needs a test
  that a commit touching another page does not project the active one (it fails today).
- **O2. 035's banner placement.** 035 places a 24 px strip under the page tabs "until the
  redesign" (design 035 §7.2). The shell's banner slot (32 px, under the top bar) takes it in U2
  if 035 is in `dev` by then; otherwise U4d. 035's data contract and element names
  (`agent.banner`, `agent.banner.undo`, …) are kept, so tests written for 035 still find them.
- **O3. 036's terminal manager.** U3's third column set (§6.3) depends on 036 in `dev`; if 036 is
  later than U3's first two column sets, U3 merges without it and Terminals follows as a step of
  its own.
- **O4. File ownership while U2 and U3 run together.** U2 owns `ecad-trial/src/shell/`,
  `lib.rs`'s `ui` and the moved panels; U3 owns `ecad-ui/src/table/`, `ecad-trial/src/lists/`
  and the Reports window's tables. U3 hands its views to U2's slots through one function per
  view (`lists::show(ui, tab, …)`), so the two touch `lib.rs` in one agreed place.
- **O6. `tokens.md` and 036.** 036 adds two rows to `tokens.md` (`ecad.level-box`,
  `ecad.field-editable`), and 035 adds `ecad.agent`, `ecad.agent-write`, `ecad.agent-bar`. U1
  rewrites the file to the new names, so U1 keeps every existing `ecad.*` row (marked "retired in
  U4a" or mapped to its new token) and merges whichever of 035 and 036 lands first. The doc-sync
  test covers only the new tables.
- **O7. 035's Agent access setting** sits in ECAD Dev "until the UI shell gives settings a home"
  (design 035 §3.2). Its home is Settings ▸ Agent access (spec §3, M1); U2 builds the Settings
  overlay as a slot only, and U4d moves the setting into it.
- **O5. CI.** Nothing in this programme changes CI. The UI fonts need no build input (§4); 031
  Q2's craft-common-actions issue for embedding craft-fonts in CI still matters for the page font
  until U4a.

## 13. Owner questions (answered)

Answered by the owner on 2026-11-13 (relayed by the overseer): Q2–Q4 as recommended; Q1 changed,
as below. The overseer accepted O1–O7 as written.

1. **Q1. Fonts.** **Decided:** the fonts are embedded in ECAD's own repository
   (`ecad/assets/fonts/`, `include_bytes!`), not added to `storytold/craft-fonts`; offering them
   upstream comes later (§4). This was the overseer's change to the recommendation, accepted by
   the owner.
2. **Q2. Seams upstream.** **Decided as recommended:** US1 and US2 stay ECAD patches during this
   programme and are offered to upstream CADCraft together once U4b has used every switch.
3. **Q3. Smaller screens.** **Decided as recommended:** 1920 × 1080 is the design target; at
   1366 × 768 the shell stays usable by collapsing the right panel, then the left, then the drawer
   (§5.2); no screen is designed specially for it.
4. **Q4. Right-click while a tool runs.** **Decided as recommended:** it finishes the current
   step (as Enter, as today); the wheel opens only when no tool is running; Esc ends the tool.

The questions as they were put:


Only what the spec and the answers leave open. Each has a recommendation; nothing is blocked if
the recommendation is accepted.

1. **Q1. Fonts in craft-fonts.** Adding IBM Plex Sans (3 faces) and Barlow Semi Condensed
   (2 faces) to `storytold/craft-fonts` is a pull request to another repository, and it adds
   about 0.8 MB to every craft app that embeds that manifest. **Recommend:** yes, open that pull
   request (the house rules require fonts to live there), from U1's worker, and say in it which
   app needs the fonts.
2. **Q2. Offer the two seams upstream.** US1 (embedded drawing view) and US2 (font definitions
   builder) are generic. **Recommend:** keep them as ECAD patches during this programme, then
   offer both to upstream CADCraft together once U4b has used every switch, so the seam is proven
   before it is proposed.
3. **Q3. Smaller screens.** The spec budgets 1920 × 1080. **Recommend:** 1920 × 1080 is the
   design target; at 1366 × 768 the shell stays usable by collapsing the right panel, then the
   left, then the drawer (§5.2), and no screen is designed specially for it.
4. **Q4. Right-click while a tool is running.** Today right-click acts as Enter during a command
   (CADCraft's default). The spec says right-click opens the wheel, which is for objects.
   **Recommend:** while a tool runs, right-click finishes the current step (as Enter, as today);
   the wheel opens only when no tool is running. Esc still ends the tool.
