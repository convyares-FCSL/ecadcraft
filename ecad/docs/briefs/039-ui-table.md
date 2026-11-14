# Brief 039: The one table component (U3)

Status: Approved (owner, 2026-11-13).
Branch: `feature/ui-table`, from `origin/dev` after 037 has merged. Runs beside U2 (038).
Designer: done (`docs/design/ui-architecture.md` §6, §8, §9, §10; the look is spec §2 and §5) ·
Size: L, in three steps
Roadmap: M1 **1.7** (table component). 036's terminal manager lands here (owner, 2026-11-13).
Inputs: `docs/discussions/ui-2026-11/ui-design-spec.md` §5 ("What you set and what is read",
"How live is shown") and §6 step 3; design 036 §9 (the terminal row model).

## Goal

One editable, virtualised, live table that every manager uses: a manager becomes a column set
plus a data source. Checks first because it is read-mostly, then Devices, then Terminals.

## §0 Files touched

| File | Change |
| --- | --- |
| `ecad/crates/ecad-ui/src/table/` (new: `mod.rs`, `source.rs`, `view_model.rs`, `selection.rs`, `tint.rs`, `edit.rs`, `cells.rs`, `export.rs`) | The table (design §6.1–§6.2) |
| `ecad/crates/ecad-ui/tests/all/lanes/lane_039.rs` (new lane module) | Model and widget tests, and the `ui` measure's generous timing check |
| `ecad/crates/ecad-ui/benches/` or an ignored measure in `lane_039.rs` | The `ui` measure: a synthetic 10 000-row source, scroll, sort, filter (design §10) |
| `ecad/apps/ecad-trial/src/lists/` (new: `mod.rs`, `checks.rs`, `devices.rs`, `terminals.rs`) | Column sets and sources; `lists::show(ui, tab, …)` for the shell (design §12 O4) |
| `ecad/apps/ecad-trial/src/checks_panel.rs`, `devices_panel.rs` | Their row drawing replaced by the column sets; kept as thin hosts until U2 lands |
| `ecad/apps/ecad-trial/src/outputs_ui.rs` | Reports ▸ Terminals uses the Terminals column set (step 3) |
| `ecad/apps/ecad-trial/src/lib.rs` | One call site per converted view |
| `ecad/apps/ecad-trial/tests/all/lanes/lane_039.rs` (new lane module) | Column-set tests |
| `ecad/docs/quick-start/039-ui-table.md` (new) | What the owner runs and sees (per-lane quick start) |
| `ecad/crates/ecad-cadcraft/src/lanes.rs` | Only if a new command module is added (one line, `("039", …)`); none is planned |

Not touched: `ecad-cadcraft` (sources read the existing models: `checks`, `device_list`,
`stripplan` rows); the shell (038); upstream.

**Changes at approval (2026-11-13):**
- File paths follow the per-lane layout (PR #72): tests in each crate's
  `tests/all/lanes/lane_039.rs`, quick-start text in `docs/quick-start/039-ui-table.md`, any new
  command module registered in `ecad-cadcraft/src/lanes.rs`.
- The `ui` measure moves out of `ecad-bench`. `ecad-bench` is an Adapter crate and U1's layer rule
  now refuses UiKit dependencies there, so the measure lives with `ecad-ui` itself (its own bench
  or an ignored measure in its lane tests, run in release), against a synthetic source.
- 036 has merged into `dev`, so step 4 (Terminals) runs now, in this PR, on 036's row model
  (design 036 §9).

## Scope, in order

1. **Table** in `ecad-ui`: `TableSource`, `Column`, `Cell`, `Table`, `TableEvent` (design §6.1);
   virtualised rows on `egui_extras::TableBuilder`; sort, filter, group, multi-select, inline
   edit, go-to, row ↔ object link through the key, change tint, CSV export (design §6.2).
   Set-versus-read: `Set` cells look like fields, `Read` cells are grey text and go to their
   source; provisional values in the provisional style.
2. **Checks** column set: severity, message, object, page, fix and go-to. The drawer shows the
   current page's checks; Lists ▸ Checks shows all.
3. **Devices** column set: tag (set), description (set), function and location (read, "from
   box" where it comes from a box), symbol, pages placed, part (read until part sets exist),
   provisional state; filter chips Provisional and Not on a page yet.
4. **Terminals** column set (036 is in `dev`; this step runs now): design 036 §9's row model laid out as a
   terminal plan (inside right-aligned | strip | outside left-aligned); editable cells Position,
   Label, Rating, links and accessories as inputs; links as a `Lanes` cell. If 036 is not in
   `dev` when steps 1–3 are done, this PR merges without step 4 and step 4 follows on the same
   branch (design §12 O3).

## Out of scope

Proposal row state (list import, M2), XLSX (M3), Cables, Wires and PLC column sets (later
managers), part sets, pop-out window, placing the views in the shell's slots (038 does that; this
brief exposes `lists::show`).

## Update map

| Source | Changed by | Does |
| --- | --- | --- |
| Checks | The kept checks model's revision (`update_checks`) | Patch the rows that changed, add and remove; tint changed cells |
| Devices | `device_changes_since` | Patch touched devices; add and remove rows |
| Terminals | 036's strip rows `last_changed` (kept outputs model) | Patch touched strips' rows |

Every other op: nothing. Full rebuild only on first show, reload or a feed that rolled over.

## Acceptance criteria

1. **Model** (plain unit tests): filter, group and sort produce the expected order on a table of
   cases; natural tag order (`-K2` before `-K10`); selection survives sort and filter; patching a
   touched key moves it to its sorted place without rebuilding (build counter unchanged); tint
   lasts 1.2 s and is only on changed cells.
2. **Widget** (kittest, no wgpu): Enter on a set cell starts edit, Enter commits one
   `TableEvent::Commit`, Esc reverts, Tab goes to the next set cell; read cells cannot be edited
   and emit `GoTo`; Shift-click selects a range; a refusal from the owner shows under the cell
   and the value reverts.
3. **Checks and Devices** show the same rows as today's panels on the demo project (test compares
   them); selecting a device row selects its symbols on the page, and selecting a symbol scrolls
   to and highlights its row; editing a tag runs the same command as on the page and appears as
   one transaction in History.
4. **Live**: renaming one device patches one row (counter shows one patch, no rebuild) and tints
   the tag cell; a commit that touches no device leaves the Devices table untouched.
5. **Terminals** (step 4): rows equal `stripplan` rows; only 036's editable fields are inputs;
   editing a label writes `termlabel` as one transaction; inside columns are right-aligned and
   outside left-aligned.
6. **Performance** (`cargo xtask bench --tier medium` plus `ecad-ui`'s `ui` measure, release): 10 000-row
   scroll with the table body ≤ 3 ms per frame and no frame over 16 ms; sort or group ≤ 20 ms;
   filter keystroke ≤ 10 ms; first show of Devices on the medium project ≤ 50 ms (design §10).
   The fast test suite contains no timing assertion tighter than 10× these.
7. `cargo xtask layers` and `wasm` pass (the table is in `ecad-ui`).

## Tester

Yes, for criteria 3–5 (edits that write transactions from a new surface).

## Owner check

Edit a tag and a terminal label in the table and watch the page change.
