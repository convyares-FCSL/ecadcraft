# Upstream CADCraft review, 2026-11-09

Read-only review of `storytold/cadcraft` since the commit ECAD incorporates. Nothing in
`C:\dev\ecadcraft` was changed or pushed. All trial work was done in a scratch clone,
`C:\dev\ecad-features\_tmp\cadcraft-upstream`.

- **Our base:** `660e15c` (v0.15.1, 2026-10-30), from `ecad/UPSTREAM.md` on `origin/dev` (`7a38d54`, PR #44).
  The local `dev` checkout is 112 commits behind `origin/dev`, so `origin/dev` was used throughout.
- **Upstream now:** **v0.19.0** (`227e3c38`, released 2026-11-09 06:29 -0500). The brief expected 0.18.x;
  0.19.0 came out this morning. `main` (`a3d9348f`) is the 0.19.0 release commit minus its version
  bump, so **nothing is on `main` after 0.19.0**.
- **Size:** 525 commits from our base to v0.19.0 (361 without merges). Over `crates/` and `apps/`:
  456 files, +45,055/−2,559 lines. Over `engine` and `ui-egui` alone: 247 files, +24,161/−1,523.
- **Release shape:** 0.16.0 and 0.17.0 are on `main`. 0.17.1 (4 commits, release workflow only, no
  notes) and 0.18.0 (8 commits, release merges) are on a release branch. Their content reached `main`
  and is all inside 0.19.0. Commit counts: 0.15.1→0.16.0 31, →0.17.0 84, →0.18.0 (main part) 303,
  →0.19.0 107.

**Method.** I read the release notes. I diffed every seam file. I ran a **trial merge of `origin/dev`
with each of v0.16.0, v0.17.0, v0.18.0 and v0.19.0** in the scratch clone. I resolved the v0.19.0
merge by hand. Then I ran `cargo check --workspace --all-targets` of the ECAD workspace on the result,
with its own target dir under `_tmp`, and fixed what failed until it compiled. Results are below.
Artifacts kept in `_tmp`:
- `upstream-review-2026-11-seam-resolution.diff`: our seams on top of v0.19.0 in the three files that conflict.
- `upstream-review-2026-11-ecad-fixes.diff`: the ECAD-side edits that make the workspace compile.
- `upstream-review-check*.log` and `upstream-review-test.log`: the check and test logs.

---

## 1. What changed (per release)

### 0.16.0 (2026-11-01, 31 commits)
- **Fixed: picking after an edit no longer ends the app on an `rstar` panic** (#2539). Confirmed.
- Zooming far in on a highlight no longer freezes: only the visible part is dashed (#2454).
- Backups of drawings that share a name no longer overwrite each other (#2537).
- The window opens centred (#2510).
- Hardening against NaN and infinite values: SETVAR, CAL min/max, `atof`, OSMODE range, Bounds2 overflow, mirror-line EPS.
- AutoCAD-parity commands: `-UNITS`, `-ARRAY` (the classic command-line array), `CHANGE`, `-INSERT` attribute prompts, RECTANG/POLYGON vertex order, SCALE scales heights.
- WBLOCK writes `$INSUNITS`. LEADER vertex Z is kept in DXF.
- Vietnamese UI language.

### 0.17.0 (2026-11-02, 84 commits)
- **The main window remembers its size, position and maximized state** (#2604). Confirmed, but it
  lives in `apps/cadcraft/src/main.rs` (eframe `app.ron` plus `ron`), **not** in `ui-egui`.
  ECAD's own `ecad-trial` main does not get it from a sync. It is a separate small ECAD change.
- Performance: hit tests no longer lay out dashes, so hovering over blocks stays smooth (#2603).
  Drawings with many groups are read in linear time (#2645).
- `U` inside a LISP routine takes back the routine's last command (#2610). This is what conflicts
  with our undo/redo seam, see §2.
- `OOPS` brings back the last erase (#2605, #2625). `-WBLOCK`, `-ATTDEF` and `LEADER` at the command line.
- macOS: Cmd+Q and Dock Quit ask to save (#2620).
- Saves into a missing folder create it (#2668, #2669). Plot style and plotter files save atomically.
- SAVEAS `portableFonts`, and a note at save time for fonts AutoCAD on Windows lacks (`report_fonts`).
- Many DXF/DWG fidelity fixes: viewport frozen layers, caret escaping, dimension associations,
  polyface meshes, OLE2FRAME kept on DWG save, layout limits, plot style ByBlock, MTEXT whitespace.
- Thai text and MTEXT word wrap: new dependency `icu_segmenter`, see §4.
- QuickCalc history persists.

### 0.18.0 (2026-11-06, 303 commits on main)
- **Raster images drawn on screen and in PNG output** (#2749/#2505): `IMAGE`, `IMAGEATTACH`. Confirmed.
- **SHX fonts, big fonts and unifonts. Linetype shapes** (#2751/#1944, #2803/#2316). Hebrew SHX,
  user font folders, Latin accents in the stroke font, a Georgian font and UI language. Confirmed.
- **"build: drop three egui patches nothing uses"** (#2835): `egui_system_font_provider`,
  `egui_demo_lib`, `egui_inspection`. Confirmed, see §3.
- **Performance work** (confirmed): constraints, region renesting, extents per block, multilines,
  dynamic blocks, merging drawings in linear time, indexed name lookups (AUDIT, layer states, CAL,
  DXF import), and render charges per block entry with linear dash and outline walks.
- **Robustness:**
  - The DWG reader and writer run **in a child process** (E7, #2673), opt-in by the app.
  - **A panic in the UI autosaves and reports** instead of ending the app (E4, #2671, `ui-egui/src/fault.rs`).
  - Autosave failures are shown, and a crash is recovered at the next launch (#2670, #2676).
  - Autosave files are owner-only (#2717).
  - The DXF group-code limit is raised and bounded (#2683). Hostile DXF input is hardened (#1026).
- New commands and windows: `DVIEW` and view twist (model, tiles, layout viewports), `XCLIP`/`XCLIPFRAME`,
  `MINSERT`, a Find and Replace window, a TEXTEDIT window, new icons drawn in code.
- Fixes: EXPLODE of non-uniform blocks gives ellipses. EXTEND on polyline arcs.
  **BREAK and TRIM keep the object's handle** (#2545), see §4. Properties edits skip locked layers.
  A drawing opens on its saved view. Recent-file cleanup.
- An open → save → reopen round-trip harness (`cadcraft-cli roundtrip`, `cargo xtask corpus`).
- Web: page host mode. MCP: `render` takes a background and a plot style.

### 0.19.0 (2026-11-09, 107 commits)
- The MTEXT in-place editor from TEXTEDIT. Multileader text can be edited.
- **The Properties palette shows type properties for several objects of one type** (#2403, `palettes.rs` +706 lines).
- Alias Editor imports `.pgp`, and aliases persist (`Services::pick_pgp`, see §4). The keyboard shortcuts
  dialog can bind any command. Right-click customisation.
- DWG/DXF: other applications' objects are kept as proxies. Hard-owned objects are kept.
- More performance work (NURBS derivatives, spline fit, region build). MTEXT bidi and format-code fixes.
- `window.cadcraft` web page API. The updater reports its states (`craft-update` rev bump).

**Worth having for ECAD:** the rstar pick panic fix, the UI panic guard (with the caveat in §4),
hover and render performance, the hostile-DXF and NaN hardening, BREAK/TRIM/EXPLODE fixes, and the
Properties multi-select (useful for ECAD symbol edits). The rest is mostly AutoCAD parity and file
fidelity, which ECAD does not use directly (see §4, file behaviour).

---

## 2. Our seams (X1–X10)

The trial merge of `origin/dev` with each release found conflicts in only **3 code files, plus our
`README.md`** (ECAD-owned: keep ours). The set is the same from 0.17.0 to 0.19.0, and 0.16.0 merges
with only the README conflict. After the merge, every one of the 20 files with `ECAD-SEAM` markers
keeps the same marker count as on `dev`.

| Seam | File | Upstream touched? | Re-apply |
| --- | --- | --- | --- |
| X2–X7, X9 | `crates/engine/src/ext.rs` (ours, new) | no | none |
| X2–X4 | `crates/engine/src/lib.rs` | yes (+12) | none (clean) |
| X2–X4 | `crates/engine/src/session/execute.rs` | yes, 0.17.0 (#2610 LISP `U`; `swap_in`, `lisp_rebase`) | **conflict**, easy: `undo()` ~l.133 and `redo()` ~l.153 both gained `lisp_take_back` where our `ext::history` sits. Keep ours first, then upstream's. The commit gate at ~l.80 merged clean. |
| X2, X3, X7 | `crates/engine/src/session/interactive.rs` | yes, 0.17.0 (#2610) | **conflict**, easy but needs care: ~l.396 in `finish`. Upstream merged the transparent and LISP branches into one `if` and records `lisp_record_step`. Keep our transparent early return and owner gate, then take upstream's LISP block (`if self.lisp_groups_undo()` with its body). |
| X2 | `crates/engine/src/lisp.rs` | yes (+85) | none (clean, start check at l.261) |
| X1 | `crates/engine/src/cmd.rs` | yes (+4) | none |
| X5 | `crates/engine/src/cmd/file.rs` | yes, 0.17.0 (`report_fonts`, `save_options`) | **conflict**, easy: ~l.990, `run_qsave_reporting` and `run_saveas_reporting`. `save_reporting` takes a 4th argument (`None`, or the `asked` version from `save_options("saveas", p)?`). Keep our `ext::save` hand-over first. |
| X4 | `crates/engine/src/cmd/edit.rs` | yes (−8/+1) | none |
| X5 | `crates/engine/src/autosave.rs` | yes, 0.18.0 (+222: failure display, crash recovery, `save_now`, `emergency_save`) | none (clean, l.237). All new paths go through `plan_with`, so owned documents are still skipped. See §4 for the panic-save gap. |
| X6 | `crates/engine/src/cmd/modify/transform.rs` | yes (+81, OLE frame handling) | none (clean, l.63) |
| X6 | `crates/engine/src/cmd/clipboard.rs` | yes (−40: `merge`/`block_names` moved to `cmd::merge`) | none (clean, l.45 and l.89) |
| X5 | `crates/ui-egui/src/closing.rs`, `chrome.rs` | no | none |
| X9 | `crates/ui-egui/src/canvas.rs` | yes (+115: images, view twist) | none (clean, l.994 and l.1101). `Xf::to_screen` now applies the twist, so marks follow a twisted view. |
| X8, X10 | `crates/ui-egui/src/palettes.rs` | yes, 0.19.0 (+706, Properties multi-select) | none (clean, l.21/181/221/310/874). `properties_section` still calls `owner_properties` under the header, before CADCraft's rows. |
| X8, X10 | `crates/ui-egui/src/lib.rs` | yes (+54, `Services::pick_pgp`, fault state) | none (clean, l.335/342/409) |
| tests | `engine/tests/ext_*.rs`, `ui-egui/tests/owner_overlay.rs`, `toolsets_extra.rs` | no | none |

The resolution I used is in `_tmp/upstream-review-2026-11-seam-resolution.diff`. It is 3 hunks with
no new seam points. `UPSTREAM-PATCHES.md` needs only its wording changed (the commit, and
"added with the 0.15.1 sync"), not new rows.

**New upstream paths that change a drawing, checked against the gate:**
- **Find and Replace window, TEXTEDIT window:** both run `app.run("find"/"textedit")`, so they
  pass the commit gate.
- **New commands** (`-array`, `-units`, `-wblock`, `change`, `dview`, `image`, `imageattach`,
  `minsert`, `oops`, `xclip`, `xclipframe`) all run through `execute` or `interactive`, so they are gated.
- `-ARRAY` copies entities without provenance. The adapter refuses copies without provenance at
  commit, as it does for ARRAY, but nothing tests that yet. **`OOPS`** brings back erased entities
  with their old handles. How the adapter classifies that needs a test.

---

## 3. Patch list (`[patch.crates-io]`)

Upstream's root list at v0.19.0 compared with 0.15.1:
- **Dropped** (#2835): `egui_system_font_provider`, `egui_demo_lib`, `egui_inspection`. **No change for
  us**: `ecad/Cargo.toml` already leaves them out.
- **Unchanged:** every rev. egui family `6d518fc…`, winit `a517b38…`, krilla/hayro `92d392c…`.
- **New use:** `egui_extras` is now a real dependency of `cadcraft-ui-egui` (resizable About-credits
  columns), so it **enters the ECAD graph**. Our comment lists it as unused.
- Not patches, but they arrive with the sync: `craft-update` moves to rev `1b34eba4…` (root
  workspace dependency, not in the ECAD graph), and `acadrust` goes 0.6.5→0.6.6 (`cadcraft-dwg`).

**Exact edits to `ecad/Cargo.toml`** (tested in the trial, the patch is used per the check log):

```toml
# Mirrors CADCraft's root [patch.crates-io] (CADCraft 0.19.0, ../Cargo.toml): ...
# Re-copy on every upstream sync. Entries the ECAD graph does not use (hayro-syntax and hayro-jbig2)
# are left out; upstream itself dropped egui_demo_lib, egui_inspection and
# egui_system_font_provider in 0.18.0. egui_extras is used since CADCraft 0.18.0 (cadcraft-ui-egui).
...
egui_glow = { git = "https://github.com/storytold/egui.git", rev = "6d518fc799681571963842f95b55a6b68431b15a" }
egui_extras = { git = "https://github.com/storytold/egui.git", rev = "6d518fc799681571963842f95b55a6b68431b15a" }   # add
egui_kittest = ...
```

That is one added line and a reworded comment. Without the line, `egui_extras` 0.37 would come from
crates.io on top of the patched egui. It would probably still build, but the egui family would be
mixed, which the comment's own rule forbids.

---

## 4. Risks

**Toolchain and MSRV.** `rust-version = "1.95"` is unchanged, and there is no `rust-toolchain` file.
The local toolchain (1.99.1) built everything.

**Dependencies** (new in `ecad/Cargo.lock` after the trial):
- `icu_segmenter` and `icu_segmenter_data`: Unicode-3.0. The ICU4X 2.3 family (`icu_collections`,
  `icu_locale_core`, …) is **already in our lock** through `url`/`idna`, so this adds no new licence.
  It does add compiled-in LSTM word data to `cadcraft-fonts`, so expect a bigger binary.
- `core_maths` (MIT), `enum-map` and `enum-map-derive` (MIT/Apache-2.0), `egui_extras` (MIT/Apache-2.0).
- Removed: `zopfli`.

Neither repo has a `deny.toml`, so a person checks licences by hand. **wasm:** ECAD's
wasm-relevant crates (`ecad-model`, `ecad-store`, `ecad-file`) don't depend on cadcraft, so they are
not affected. Upstream keeps its own wasm build green.

**API changes `ecad-cadcraft` and `ecad-trial` use.** Found by compiling, not by grep. No public item
ECAD uses was removed. Three struct-literal breaks, 7 sites, all fixed in
`_tmp/upstream-review-2026-11-ecad-fixes.diff`:

| Change | Our sites | Fix |
| --- | --- | --- |
| `cadcraft_engine::View` gained `twist: f64` (0.18.0, DVIEW) | `ecad-cadcraft/src/commands.rs:1435`; `ecad-trial/src/lib.rs:1829, 1852, 2298`; tests `checks_ui.rs:68`, `project_pages.rs:85` | add `twist: 0.0` (or a `View::new` helper) |
| `cadcraft_doc::Insert` gained `clip: Option<ClipBoundary>` (0.18.0, XCLIP) | `ecad-cadcraft/src/convert.rs:105` | `clip: None` |
| `cadcraft_ui_egui::Services` gained `pick_pgp` (0.19.0) | `ecad-trial/src/lib.rs:366`; tests `refusals.rs:206`, `upstream_sync_contracts.rs:430` | `pick_pgp: None` |

With these, `cargo check --workspace --all-targets` of the ECAD workspace passes with **0 errors and
0 warnings**. A grep over all removed `pub` items found nothing ECAD uses.

**Behaviour changes that touch ECAD's model of a page:**
1. **BREAK and TRIM keep the object's handle** (#2545). Before, the adapter's commit saw a delete
   plus new objects. Now it sees one modified handle plus new objects with no provenance.
   **Confirmed by the smoke test:** BREAK on a page is no longer refused (see below). TRIM still
   needs checking.
2. **OOPS** (new) and **`-ARRAY`** (new): see §2. They need adapter contract tests: refuse them, or
   classify them as restore and copy.
3. **UI panic guard (E4):** the emergency autosave goes through `plan_with`, so **owned (ECAD)
   pages are skipped**. That is correct for our X5 rule, but an ECAD page's unsaved work is *not*
   saved when the UI panics. Upstream's guard keeps the app running, so the user can still save the
   project. A generic hook (`DocumentOwner::emergency_save`) would be a small, upstreamable X5
   addition. It is a follow-up, not a sync blocker.
4. **DWG in a child process:** this is opt-in, through `cadcraft_dwg::serve_if_asked()` and
   `use_own_worker()` in the app's `main`. `ecad-trial` doesn't call them, so DWG stays in-process,
   as now. ECAD only opens and refuses `.dwg` as plain drawings. It is optional to adopt.
5. **Window size memory:** this lives in `apps/cadcraft/src/main.rs` only. ECAD would copy it into
   `ecad-trial`'s main if wanted.

**File, DXF and DWG behaviour.** ECAD pages are not stored as DXF: `.ecad` holds ECAD's own model,
and `ecad-cadcraft` uses `cadcraft-io` only through `cadcraft-mcp`'s PDF export. The many DXF/DWG
fixes therefore only affect exporting or opening plain drawings. The one conversion detail ECAD
uses, `Lineweight::from_dxf`/`to_dxf` in `convert.rs`, is unchanged.

**Smoke test.** I ran `cargo test -p ecad-cadcraft` (fast set) on the trial merge. **307 passed, 2
failed, 9 ignored.** Both failures come from upstream behaviour changes, and the sync must fix both.
I did not re-run them on `dev`; they are taken as passing there, because `dev` merges green.
1. `refusals::replacing_or_unsupported_edits_are_refused`, at `refusals.rs:51`:
   `break: nothing appended`, left 7, right 6. **BREAK on an ECAD page is no longer refused: a
   transaction is recorded.** This is risk 1 below (#2545) in action. Because BREAK keeps the handle,
   the gate now sees a modified object plus a new one instead of a replacement, and the adapter accepts
   it. The fix is in the adapter: refuse a commit that keeps a handle and adds an object without
   provenance, or put `break` and `trim` in `REFUSED_COMMANDS`. Check TRIM as well.
   **This is the sync's one real piece of work.**
2. `device_contracts::g_device_properties_renames_and_asks_before_merging`, at `device_contracts.rs:427`:
   the DEVICEPROPS prompt now reads `Enter property to change [Tag/…/Function]::`. Upstream rewrites
   keyword prompts without "or" (#2540/#2563) and adds its own colon. ECAD's prompt text must follow
   the new convention: drop "or" and the trailing colon. This is cosmetic, but every ECAD keyword
   prompt and its test expectation need checking.

**Not run:** CADCraft's own engine tests in the root workspace (`ext_owner`, `ext_commands`,
`owner_overlay`, `toolsets_extra`), clippy, the root workspace build, wasm, and other ECAD crates' tests.

---

## 5. Recommendation

**Sync now, in one step to v0.19.0.** Stepped syncs (0.16 → 0.17 → 0.18 → 0.19) buy nothing:
- The trial merges show the **same three conflicts from 0.17.0 to 0.19.0**, and 0.16.0 has none besides README.
- 0.17.1 and 0.18.0 sit on a release branch whose content is all in 0.19.0, so stepping to them means
  merging release-branch commits we then merge again.
- Each step would repeat the patch-list, lock and test work.

The weekly-review policy matters more from here. Upstream ships about two releases a week, and
`palettes.rs` (X8/X10) and `autosave.rs` (X5) are now being reworked actively.

**Size estimate:** medium. About one working day for a worker, plus a tester pass (the brief changes
the gate and refusals, which is on the tester's list):
- the merge with 3 small conflict resolutions (done in the trial, about 30 lines);
- 7 one-line ECAD fixes and 1 `Cargo.toml` line;
- `Cargo.lock` regeneration;
- `UPSTREAM.md` and `UPSTREAM-PATCHES.md` wording;
- **an adapter fix so BREAK (and probably TRIM) is refused again** (a failing test today), plus
  ECAD keyword prompt texts brought to upstream's "no 'or'" convention (a second failing test);
- new contract tests for TRIM, OOPS and `-ARRAY` on owned pages;
- the root CADCraft build and tests for the seam crates.

The test effort is the larger part, not the merge.

**Outline brief, `upstream-sync/2026-11-xx`:**
1. On a branch from `dev`, `git merge v0.19.0` (tag `227e3c38`). Keep ours for `README.md`. Resolve
   `execute.rs`, `interactive.rs` and `file.rs` as in `_tmp/upstream-review-2026-11-seam-resolution.diff`.
2. Apply `_tmp/upstream-review-2026-11-ecad-fixes.diff` (the `View.twist`, `Insert.clip` and
   `Services.pick_pgp` literals). Add the `egui_extras` line and reword the `ecad/Cargo.toml` patch
   comment (§3). Regenerate `ecad/Cargo.lock`.
3. Update `ecad/UPSTREAM.md` (commit `227e3c38…`, CADCraft 0.19.0, 2026-11-09) and the
   `UPSTREAM-PATCHES.md` wording. Run `cargo xtask patches`.
4. Tests:
   - root workspace: `cargo test -p cadcraft-engine --test ext_owner --test ext_commands`, and
     `-p cadcraft-ui-egui --test owner_overlay --test toolsets_extra`;
   - ECAD: `cargo xtask test-changed` (every crate counts as changed, since the lock moved);
   - the tester covers the gate: undo and redo on owned pages after the LISP-`U` change, the
     transparent command inside an owned command, QSAVE/SAVEAS hand-over;
   - fix the two `ecad-cadcraft` failures from the trial (`refusals::replacing_or_unsupported_edits_are_refused`
     for BREAK; the DEVICEPROPS prompt in `device_contracts`). Then add contract tests: TRIM keeps
     the handle, and OOPS and `-ARRAY` on a page are refused or recorded.
5. Owner decisions:
   - whether to add `oops`, `-array` (and `minsert`?) to `REFUSED_COMMANDS`;
   - whether to adopt the DWG worker and window-size memory in `ecad-trial`;
   - whether to propose an `emergency_save` owner hook (X5 extension) for UI panics.
6. Full suite at the QA gate as usual (wasm, Windows and Linux).

**Clone:** I deleted the scratch clone and the `_tmp/upstream-review-target` build folder. The sync
should start from the ECAD repo itself (`git fetch https://github.com/storytold/cadcraft --tags`),
with the two `.diff` files here as the guide.
