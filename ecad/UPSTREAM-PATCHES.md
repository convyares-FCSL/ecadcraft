# Files changed outside `ecad/`

Every file in the upstream tree that differs from the incorporated CADCraft commit
([`UPSTREAM.md`](UPSTREAM.md)) is listed here. `cargo xtask patches` fails if a changed file is
missing. Code patches are marked in the source with `// ECAD-SEAM(owner):`.

## ECAD-owned files outside `ecad/`

These are ours. On upstream merges, conflicts are resolved by keeping ours.

| File | Why |
| --- | --- |
| `README.md` | ECAD repository front page |
| `.git-blame-ignore-revs` | Commits `git blame` skips: the ECAD workspace reformat (PR #46) |
| `.github/workflows/ci.yml` | CI: quick ECAD check (fmt, clippy, fast tests of changed crates and their dependents) on PRs into `dev`; full CADCraft and ECAD suite on PRs into `main`, pushes to `main`, by hand and nightly on `dev` (ECAD tests through cargo-nextest, sharded; large bench nightly); actions pinned to commit hashes |
| `.github/dependabot.yml` | Only GitHub Actions pins, weekly, into `dev`; upstream's daily root Cargo updates removed (root dependencies arrive with the upstream sync) |
| `docs/concept-spec.md` | ECAD concept spec (moves to `ecad/docs/` once housekeeping R11 is approved) |
| `docs/decisions.md` | ECAD decision log |
| `docs/cadcraft-assessment.md` | Phase 0 assessment |
| `docs/document-architecture-proposal.md` | architecture proposal (PR #1) |
| `docs/ecad-architecture-foundations.md` | owner's foundations note |
| `ATTRIBUTION.md` | Shared with upstream: ECAD's rows (brief 037) for the embedded IBM Plex Sans and Barlow Semi Condensed faces under `ecad/assets/fonts/` (owner answer Q1, 2026-11-13: ECAD keeps these fonts in its own repository); on upstream merges keep upstream's rows and ours |

## Upstream fixes taken ahead of the next sync

Already merged upstream; each entry goes away at the next upstream sync, which brings the same change.

| File | Upstream change | Why now |
| --- | --- | --- |
| `crates/fonts/src/mtext.rs` | CADCraft #2954 (`6f0f1fe`): `strips_formatting` expects a short `\U+` escape to keep its digits, as `unicode_escapes_are_decoded_in_every_parser` already did (test only) | After CADCraft 0.19.0 the two tests contradicted each other, so the root workspace's tests failed at the v0.1 QA gate |

## Code patches to upstream crates: the generic extension interface

Proposal §14, capabilities X1-X7, X8 (Tool Sets tabs supplied by the embedder, approved
2026-10-15) and X9 (overlay marks drawn by a document's owner, approved 2026-11-01 for brief 008,
owner answer O2) and X10 (a section of the Properties palette drawn by the embedder, approved
2026-11-08 for brief 019, owner answer Q8), and US2 (CADCraft's interface font set as a value
the embedder extends, approved 2026-11-13 with brief 037, design `ui-architecture.md` §1.4), and US1 (CADCraft's
drawing embedded in a window the embedder draws, approved 2026-11-13 with brief 038, design `ui-architecture.md` §1.3). One shape, one marker (`ECAD-SEAM(owner)`), nothing
ECAD-specific: the interface passes drawings, handles, command ids and provenance. Documents
without an owner behave exactly as before.

| File | Capability | What changes |
| --- | --- | --- |
| `crates/engine/src/ext.rs` | X2-X7, X9 | **new file**: `DocumentOwner` trait, `Extensions` registry, start check (`refuse_start`, `Start`: the owner may refuse a command or a LISP evaluation before it starts, added with the 0.15.1 sync), commit gate, history/save/close/drag hand-over, provenance records, owner-aware `Session::doc_unsaved`, `close_needs_prompt`, `has_unsaved_work`; X9: `Mark`, `MarkShape`, `DocumentOwner::overlay` (default none) and `Session::owner_overlay` (drawable marks only, at most `MAX_MARKS`; a busy or panicking owner draws none) |
| `crates/engine/src/lib.rs` | X2-X4 | `pub mod ext`; `Session::ext` field |
| `crates/engine/src/session/execute.rs` | X2-X4 | start check before the command runs; gate at the `execute` commit point, before LISP undo grouping, with no engine undo step for owned documents; `undo`/`redo` hand-over |
| `crates/engine/src/session/interactive.rs` | X2, X3, X7 | start check before an interactive command begins; gate at the `finish` commit point (after the transparent-command return, before LISP undo grouping); a new top-level interactive command starts with nothing recorded; drag preview contribution |
| `crates/engine/src/lisp.rs` | X2 | start check in `lisp_start`, the one entry of every LISP evaluation (command-line expressions, loaded files, `c:` commands), before the drawing changes |
| `crates/engine/src/cmd.rs` | X1 | `set_extension_commands`: embedder commands appended to the registry |
| `crates/engine/src/cmd/file.rs` | X5 | QSAVE/SAVEAS hand-over; CLOSE/CLOSEALL tell the owner, and stop with the document still open when the owner fails |
| `crates/engine/src/cmd/edit.rs` | X4 | UNDO options other than a count (Mark, Back, BEgin, End, Auto, Control) are reported as unsupported on owned documents |
| `crates/engine/src/autosave.rs` | X5 | owned documents are not autosaved as drawings |
| `crates/engine/src/cmd/modify/transform.rs` | X6 | provenance for COPY and the copy forms of MOVE-family transforms (`transform_entities`) |
| `crates/engine/src/cmd/clipboard.rs` | X6 | COPYCLIP reported with its serial; PASTECLIP provenance |
| `crates/ui-egui/src/closing.rs` | X5 | close and quit prompts ask the session (owner-aware) |
| `crates/ui-egui/src/chrome.rs` | X5 | file tab unsaved marker asks the session (owner-aware) |
| `crates/ui-egui/src/canvas.rs` | X9, US1 | US1: without `Embedding::idle_typing`, keys reach the command line only while a command prompts (or its options menu is open); without `Embedding::canvas_menus`, a right-click on the drawing while no command runs opens no shortcut, grip or snap menu and is kept for `CadApp::take_secondary_click`; right-click during a command is unchanged. X9: the drawing area paints the active document's overlay marks (`owner_mark_shapes`: a ring, dot or square centred on a drawing position, at a constant size on screen, after the drawing and before grips and previews); a plain drawing has none, so it is drawn as before |
| `crates/engine/tests/ext_owner.rs` | tests | **new file**: owner interface tests, including the start check (commands and LISP refused before the drawing changes; plain drawings unaffected; a panicking check refuses and recovers) and overlay marks (`the_owner_draws_overlay_marks`: plain drawings have none; undrawable marks dropped; capped; busy or panicking owner draws none; asking changes nothing) |
| `crates/engine/tests/ext_commands.rs` | tests | **new file**: extension command registration test |
| `crates/ui-egui/src/palettes.rs` | X8, X10 | Tool Sets tabs supplied by the embedder (`ExtraToolTab` of `ExtraToolGroup`s of `ExtraTool`s): shown after Drafting and Modeling, each with collapsible groups of text buttons; a click is queued, never started; without extra tabs the strip and palettes are unchanged; X10 (approved 2026-11-08 for brief 019, owner answer Q8): `properties_section` calls `CadApp::owner_properties` under its header, before CADCraft's own rows; without it the palette is unchanged |
| `crates/ui-egui/src/lib.rs` | X8, X10, US1 | US1 (brief 038, design `ui-architecture.md` §1.3): `Embedding { chrome, idle_typing, canvas_menus, style }` (default all on: CADCraft's interface unchanged), `CadApp::embedding`, `CadApp::ui_embedded` (everything `ui` does, the drawing filling the given `Ui`), `CadApp::take_secondary_click`; without `chrome`, `draw` paints only the drawing area and CADCraft's dialogs and protective prompts (`draw_overlays`, split out of `draw` unchanged); without `style`, `ui_frame_begin` installs no fonts and applies no visuals (canvas colours still set). X8: `CadApp::extra_tool_tabs` (empty by default) and the click queue, `CadApp::take_tool_requests` (each click returned once); X10: `CadApp::owner_properties`, a section of the Properties palette drawn by the embedder (`None` by default) |
| `crates/ui-egui/tests/owner_overlay.rs` | tests | **new file** (X9): marks keep their screen size at every zoom and sit on their drawing position; marks off the drawing area or off any screen are left out; an owned drawing asks its owner every frame and drawing the marks changes nothing |
| `crates/ui-egui/tests/toolsets_extra.rs` | tests | **new file**: without extra tabs Drafting and Modeling are unchanged; an extra tab shows its groups, queues a click once without starting it, and keeps the selection; a tab named like a built-in one is ignored |
| `crates/ui-egui/src/theme.rs` | US2 | font definitions builder (brief 037, design `ui-architecture.md` §1.4): `font_definitions(language) -> egui::FontDefinitions` is the font set `install_fonts` installed (system UI font, CJK, Arabic, Korean and craft-fonts fallbacks for the language); `install_fonts` now sets exactly that, so behaviour is unchanged. An embedder extends the set (ECAD puts IBM Plex Sans first) and installs it itself |
| `crates/ui-egui/tests/embedded_view.rs` | tests | **new file** (US1): `Embedding::default()` draws CADCraft's window and idle typing starts a command; without chrome only the drawing is drawn, in the rect given; without idle typing `L` + Enter starts nothing while a prompt still takes `10,10`; without canvas menus an idle right-click opens no menu and is reported once, and right-click during a command is unchanged |
| `crates/ui-egui/tests/font_definitions.rs` | tests | **new file** (US2): after `install_fonts` the fonts in use equal `font_definitions` for English and Japanese; the builder's result is an independent value |

Not yet covered by provenance (copies through them are refused by the adapter until added):
ARRAY, grip copy (`gripcmds.rs`), MIRROR/ROTATE/SCALE through other code paths, PASTEBLOCK,
and any other command that creates entities from existing ones (see the census in the experiment
report). COPYBASE, CUTBASE and PASTEORIG go through the same clipboard copy and paste as
COPYCLIP and PASTECLIP, so they are covered.

Paths CADCraft 0.15.1 added that change a drawing outside the commit gate or keep engine undo
steps of their own are refused on owned documents by the start check instead (the ECAD adapter's
`REFUSED_COMMANDS` and `LISP_REFUSED`, owner decision 2026-10-31): the Block Editor (BEDIT, BSAVE,
BSAVEAS, BCLOSE), in-place reference editing (REFEDIT, REFSET, REFCLOSE), and LISP (expressions
typed at the command line, APPLOAD, the LISP command, `c:` commands and `(command …)` routines).

The sync to CADCraft 0.19.0 (brief 023, 2026-11-09) needed no new seam points; the three
conflicting files (`execute.rs`, `interactive.rs`, `file.rs`) keep the same hand-overs, ordered
before upstream's LISP `U` handling and save options. Paths CADCraft 0.16.0 to 0.19.0 added that
change a page all reach the commit gate, and the ECAD adapter records or refuses them (overseer
decisions, 2026-11-09):
- BREAK and TRIM keep the object's handle for one piece (#2545). The adapter refuses a commit
  that changes an object with an electrical meaning in place while adding objects without
  provenance, as it refuses a replacement; on plain drawing they are recorded.
- OOPS brings back erased objects with their old handles. The adapter records it when it brings
  back only plain drawing removed by the last erase on the same page, and refuses it, with a
  reason, otherwise (`ecad-cadcraft/src/oops.rs`).
- `-ARRAY` copies carry provenance (the transform copy path) and are recorded as copies; MINSERT
  of an ECAD symbol is refused by the gate, and BLOCK stays refused, so MINSERT has nothing else
  to insert on an ECAD page.
- The Find and Replace and TEXTEDIT windows run commands, so they pass the gate.

**Candidate to offer upstream later (not implemented, brief 023):** CADCraft 0.18.0's UI panic
guard autosaves open drawings through `autosave::plan_with`, which skips owned documents (X5), so
an ECAD page's unsaved work is not saved when the UI panics (the app keeps running, so the
project can still be saved). A generic `DocumentOwner::emergency_save` hook would close that gap.

## Code patches to upstream crates: PDF window culling (brief 033, owner answer Q1, 2026-11-12)

Generic and not ECAD-specific: a page of a `pages` plot writes only what lies in its window. To
be offered upstream to CADCraft (design 033 FU2).

| File | What changes |
| --- | --- |
| `crates/io/src/pdf.rs` | `marks` skips a polyline, fill, mask or point primitive whose page-point bounds miss the plot area grown by its stroke width and `CULL_MARGIN_PT` (`outside_plot_area`); infinite lines and images are unchanged. Each page of a `pages` plot builds the whole model, so before this every page's content stream held every page's geometry and the file grew with pages squared. The page clips to the same area, so what shows is unchanged |
| `crates/io/src/tests.rs` | tests: a page of a `pages` plot holds only its window's paths; 20 pages are at most 2.2 times 10; a culled page's content stream equals that of its sheet plotted alone; a wide stroke whose edge reaches the window and text running into it are kept, text wholly outside is culled; a plot without a window draws everything |

## Code patches to upstream crates: the canvas (U4a, brief 040, 2026-11-13)

Generic, nothing ECAD-specific; offered upstream with US1 and US2 (design `ui-architecture.md`
§1.6). Defaults are unchanged: without `register_faces` the font table is as before, and
`Embedding::default()` still draws the viewport label.

| File | Seam | What changes |
| --- | --- | --- |
| `crates/fonts/src/ttf.rs` | US3 (embedder fonts, design `ui-architecture.md` §4) | `EmbeddedFace { family, style, bytes }` and `register_faces(&'static [EmbeddedFace]) -> Result<(), FacesAlreadyRegistered>`, called once at start-up: the faces are listed like the embedded craft-fonts fonts (in place of an installed file of the same stem, never of a font in the user's own folders, kept by a rescan) and found by family name, the regular face by the family alone and the others by family and style (`Db::embedder_face`); a second call is refused and the first faces stay; registering bumps the font generation. `embedder_faces()` lists them |
| `crates/fonts/tests/embedder_faces.rs` | tests | **new file** (US3): unregistered, nothing is listed or found; a registered face is found by family and by family and style; a second call is refused, not a panic, and the first faces stay; a rescan keeps them |
| `crates/ui-egui/src/canvas.rs` | US1 (one line) | the viewport label (`+ \| Top \| 2D Wireframe`) is drawn only with `Embedding::chrome`: an embedder without CADCraft's chrome labels its own view |
| `crates/ui-egui/tests/embedded_view.rs` | tests | `the_viewport_label_is_drawn_only_with_cadcrafts_chrome`: drawn with the default and with chrome on, not without |
