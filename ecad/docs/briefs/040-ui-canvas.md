# Brief 040: The canvas (U4a)

Status: Approved (Carl, 2026-11-13).
Branch: `feature/ui-canvas`, from `origin/dev` **after the redraw fix has merged** (below).
Designer: done (`docs/design/ui-architecture.md` §1.5, §4, §11; the look is spec §4 "Canvas
colours", "Type", "Line weights") · Size: L, in five steps
Roadmap: M1 **1.5** (UI shell, part 3: the page). Lane **U4a** of the U programme; runs beside
041, 042 and 043 (file ownership below).
Inputs: `docs/discussions/ui-2026-11/ui-design-spec.md` §1 (principles 1–3), §4, §6 step 4;
`owner-answers.md` (answers 1, 5, 9; the sheet font); U0 Q1 (fonts in `ecad/assets/fonts/`).

## Goal

The page looks like its print: a paper-coloured sheet with ink that prints and overlay colours
that never do, the spec's markers, selection that shows every placement of a device, and the
sheet text in Barlow Semi Condensed at its real cap heights.

## What exists today (`dev` after #74)

- The page is CADCraft's drawing area in the main view through US1 (`CadApp::ui_embedded`), on
  CADCraft's dark model background. CADCraft's ViewCube, UCS icon and viewport label still show
  (038 open point 3): `canvas.rs` gates the first two on `ui.show_viewcube` / `ui.show_ucs_icon`
  and draws `viewport_label` unconditionally.
- ECAD's marks (connection points, attached and loose ends, cable crossings, the go-to ring) are
  `ext::Mark`s from `ecad_cadcraft::interact::overlay_marks` with ACI-era colours (`TERMINAL_RGB`,
  `ATTACHED_RGB`, `LOOSE_RGB`, …).
- `ecad-tokens` holds the spec's sheet sets (`sheet::PAPER`, `sheet::DARK`, `prints()`), markers,
  line weights and `sheet_type` (U1); nothing on the canvas reads them yet.
- **Text heights are already cap heights.** CADCraft scales TrueType and stroke text so that the
  text height is the cap height (`crates/fonts/src/ttf.rs` header, `lib.rs`
  `stroke_fallback_metrics`). 031's `text.tag.height` 2.5 therefore already draws 2.5 mm
  capitals. Owner answer 9 ("cap heights, not font sizes; U4a converts them") is met by checking
  and relabelling, not by changing values: no stored value changes meaning (§ Scope step 4).
- The page font is 031's chain: `text.font` by family name through CADCraft's resolver, which
  finds only craft-fonts faces or system fonts (`ttf.rs`: "system fonts found at run time, never
  bundled"); Barlow is embedded by `ecad-ui` for egui only (U1), so the page cannot use it yet.
- `apply_tx` re-projects the active page after every commit (design §8.4). **A separate fix is
  being built now; this brief starts after it merges**, because its step 3 adds per-frame work on
  the page and its targets assume the fix.

## §0 Files touched

This lane **owns** the files marked ●; ○ are shared, with the hunk named.

| File | Change |
| --- | --- |
| ● `crates/fonts/src/ttf.rs` or `craft_fonts.rs` (**upstream seam US3**, `// ECAD-SEAM(owner): embedder fonts.`) | `pub fn register_faces(&'static [EmbeddedFace])`, called once before first use; registered faces are found by family name exactly like craft-fonts faces. Default: none registered, nothing changes |
| ● `crates/fonts/tests/` (one module) | US3: a registered face is found by family; unregistered behaviour unchanged; registering twice is refused, not a panic |
| ○ `crates/ui-egui/src/canvas.rs` (US1, one line) | `viewport_label` gated on `embedding.chrome` |
| ○ `ecad/UPSTREAM-PATCHES.md` (append a U4a block) | US3; the US1 line |
| ● `ecad/apps/ecad-trial/src/canvas/` (new: `mod.rs`, `sheet.rs`, `markers.rs`, `selection.rs`, `leader.rs`) | ECAD's painter layer over the page: dot grid, markers, related placements, the companion leader line; canvas preferences (paper, selection colour) and the ViewCube / UCS switches |
| ○ `ecad/apps/ecad-trial/src/shell/mod.rs` (`main_view`, one call after `self.cad.ui_embedded(ui)`) | `crate::canvas::paint(self, ui, page_rect)` |
| ○ `ecad/apps/ecad-trial/src/lib.rs` (one `pub mod canvas;` line; `main.rs` start-up: one `register_faces` call beside `ecad_cadcraft::install()`) | `fix/ui-followups` edits `main.rs`'s window options and timing; this lane's line is not in those hunks |
| ● `ecad/crates/ecad-cadcraft/src/interact/mod.rs` (`overlay_marks` body only) | Mark colours and sizes from `ecad-tokens` (`sheet`, `marker`) by the sheet in use |
| ● `ecad/crates/ecad-cadcraft/src/look.rs`, `wires.rs` (drawing of conductor and core colours) | Two-colour wires and cores (step 3) |
| ● `ecad/crates/ecad-cadcraft/src/text_style.rs`, `defaults/text.rs` (labels and the font default) | "cap height" in labels; `text.font` default Barlow Semi Condensed |
| ● `ecad/crates/ecad-cadcraft/src/defaults/print.rs` | `print.colour` `mono` \| `colour` (owner answer 1) |
| ○ `ecad/apps/ecad-trial/src/shell/state.rs` (one field) | `sheet: Sheet { Paper, Dark }`, a viewing preference kept with the shell state, not in the project; switched by the `ui.sheet` Text panel command (042's Settings adds a control for it later) |
| ○ `ecad/apps/ecad-trial/src/shell/text_panel.rs` (one arm beside `ui.cadchrome`) | `ui.sheet paper\|dark` |
| ● `ecad/crates/ecad-cadcraft/src/printbook.rs` / output path that plots (only where colour mode is read) | Mono prints ink black with the colour code in the label; colour prints wire colours and the two-colour dashes |
| ○ `ecad/crates/ecad-tokens/src/sheet.rs`, `marker.rs` and `docs/design/tokens.md`'s checked tables | Only corrections found while drawing; any new `ecad.*` token goes in this lane's design section (per-lane rule) |
| ● `ecad/crates/ecad-cadcraft/tests/all/lanes/lane_040.rs`, `ecad-trial/tests/all/lanes/lane_040.rs`, `crates/ecad-ui` none | Tests, one `mod lane_040;` line in each `lanes/mod.rs` |
| ● `ecad/docs/quick-start/040-ui-canvas.md` | How to try it |

Not touched: the shell's other slots, the tool strip and wheel (041), the right panel, Settings and
Standards (042), the banner, History and `agent/` (043), `project.rs` (the redraw fix is its own
brief), `translate.rs`.

## Scope, in order

1. **Sheet.** Paper by default, dark as the second token set (`ShellState::sheet`, per person, not stored in the project). The trial
   sets CADCraft's canvas preferences from the tokens (model background = `sheet`, selection =
   `selection`); colour 7 draws as ink on paper already (`render/src/display.rs`). CADCraft's grid
   off; ECAD paints the 0.25 mm dot grid every 5 mm inside the sheet frame only, non-printing.
   ViewCube and UCS icon off through the existing `UiState` flags; viewport label off through the
   US1 line. The sheet carries the spec's one shadow.
2. **Ink and overlay.** Every ECAD overlay mark takes its colour and shape from `ecad-tokens` by
   meaning (connection point ring 8 px, attached dot 5 px, loose square 7 px, error disc with "!",
   warning triangle with "!", provisional text colour with dotted underline, terminal link dashed
   bar, cable slash). Existing marks stay `ext::Mark`s; shapes `ext::MarkShape` lacks (disc with
   "!", triangle, dashed outlines, chips) are painted by `canvas/markers.rs` over the page from
   `cad.canvas.xf`, never as entities (design §1.5), so none can reach a plot. A test plots a page
   with every marker shown and finds none of their colours in the output.
3. **Two-colour wires and cores** in both colours: the first colour solid, the second as 2 mm on /
   2 mm off dashes laid over it (GNYE, RDBU, YEBN …), on screen always; in print per
   `print.colour` (owner answer 1: `mono` default, colour code in the label). An unknown colour
   code draws in ink and raises an information check with "Define in Standards" (the button opens
   042's Standards when present; until then it is **Show it**).
4. **Sheet text.** Register the embedded Barlow Semi Condensed faces with CADCraft's resolver
   (US3) at start; `text.font`'s built-in default becomes `Barlow Semi Condensed` (a registry
   default: projects with their own `text.font` keep it). Registry labels and the `textstyle`
   command's output say **cap height**; values do not change (see "What exists today"). The spec's
   "font size = cap / 0.70" applies only where text is set by em size (egui previews, PDF text kept
   as text): the step checks every such path and converts there, by the face's own cap height from
   its `OS/2` table, not by a constant.
5. **Selection and relations.** Hover outlines the object in the selection colour; selecting a
   placement of a device gives its other placements on the page a dashed outline and lists the rest
   (042 shows the list; this step provides `canvas::related(project, selection)`). Selecting a
   device's symbols on one page already exists as `Trial::goto_symbols` (from
   `fix/ui-followups`); this step draws the outlines and adds no second selection path. In a companion
   view, selecting a row or its object draws the one-second leader line between them (spec §5;
   038 open point 8). The page opens at the zoom where a tag is 13 px tall and remembers its zoom
   (038 already stores views; this step sets the first-open zoom).

## Out of scope

The wheel, intent line and tools (041); the inspector's list of related placements (042); the
agent's last-write outline (043 draws it with its own painter); proposal ghosts (M2); panel-layout
pages (M3); converting the floating windows.

## Update map

| View | Changed by | Does |
| --- | --- | --- |
| Marks (`overlay_marks`) | As today (the page's terminals, conductors, cables) | Unchanged; only colours and shapes change |
| Painted markers and related outlines | Selection change; `changes_since` containing the shown page | Recompute for the shown page only, cached per (page, change count, selection) |
| Dot grid, sheet | Page switch; `ShellState::sheet` change | Recompute the grid's visible dots from the view only (no project read) |
| Leader line | A companion selection | Draw for 1 s, repaint only while live |

Every other op: nothing. Nothing in the painter layer reads the project per frame when the change
count and selection are unchanged.

## Acceptance criteria

1. **Seams.** CADCraft's tests pass; US3's tests pass; with `Embedding::default()` the viewport
   label still draws; `cargo xtask patches` passes.
2. **Sheet.** On the demo project at 1920 × 1080 the page background is `#F6F4EF`, ink is
   `#16191D`, no ViewCube, UCS icon or viewport label (screenshot and a kittest checking the
   flags); `ui.sheet dark` switches to the dark set without a restart and is remembered.
3. **Overlay never prints.** A test exports the demo page (PDF and the raster export) with every
   marker kind present and finds no overlay colour in it; ink, cable and provisional text appear
   in ink (mono) as the spec's "Prints" column says.
4. **Two colours.** A GNYE conductor draws a green line with yellow 2 mm dashes on screen; in
   `print.colour mono` its print is black with `GNYE` in the label; in `colour` it prints both. An
   unknown code draws in ink and raises one information check.
5. **Cap height.** A tag at `text.tag.height 2.5` measures 2.5 mm from baseline to the top of
   `K` in the plotted output, in Barlow Semi Condensed SemiBold (test reads the plotted
   geometry); a project with `text.font` set keeps its font; a build without the faces registered
   falls back as 031 does.
6. **Relations.** Selecting `-K01`'s coil on page 2 outlines its contact on page 2 dashed; the
   function `related` returns its placements on other pages (test).
7. **Live, cheap.** With the selection and change count unchanged, 300 frames over the page do
   no project read from the painter layer (counter); the shell's share stays ≤ 2 ms and an idle
   frame ≤ 8 ms on the medium tier (design §10; release, by hand).
8. **Lessons from U2/U3** (reviews of #73 and #74): every painted overlay checks
   `Shell::over_shown_page` / `page_rect` and draws nothing when Lists, Library or Publish hides
   the page; no per-frame allocation proportional to the page's objects.

## Tester

None (no format, op or store change; `print.colour` is a registry default read like 031's, and
the sheet choice is shell state). The reviewer checks US3 and the "overlay never prints" test.

## Owner check

Open the owner's own project: does page 2 look like its print? Print one page mono and one colour.
