Historical record (2026-11-15). Not current; see [developer/ui-components.md](../../developer/ui-components.md).

# Brief 063: UI kit, round 2

Status: Approved (overseer, 2026-11-15: lane R3 of the 2026-11 code review, after #113).
Branch: `chore/ui-kit-2`, from `origin/dev` `445787b8` (#113 and #116 merged).
Designer: no (the look follows `docs/design/components.md`) · Tester: no (UI light process) ·
Size: M. Source: finding 8 and row R3 of the 2026-11 code review.
Runs beside 060 (guards: widens the same `xtask` style ratchet), 061, 062 and 064.

## Goal

Lanes stop rebuilding widgets by hand. Round 1 (#113) gave panels their value components
(`control::*`, `prim::Segmented`, `panel::section_header`, `dialog::Dialog`) and the gallery. This
round adds the remaining shared pieces, gives the colours that are still written in place a
semantic token, and moves the files that never used `ecad_ui` onto it.

## Scope

1. **Components** in `ecad_ui::prim`, each one also drawn in the gallery
   (`showcase::components`). Skip what #113 already provides: `Dialog` is `dialog::Dialog` (this
   round gives it button kinds in the footer and a width token), the section header is
   `panel::section_header` (this round re-exports it as `prim::SectionHeader`, with an optional
   height and a fold state the caller keeps).
   - `SectionHeader` (fold chevron, title, right-aligned summary).
   - `PropertyRow` / `PropertyGrid`: a label in the label column and a value; a grid of them.
   - `Dialog` footer: primary, secondary and danger buttons; widths `size::DIALOG_S`/`DIALOG_M`.
   - `ListRow`: a selectable list row (a thin layer over `prim::row`).
   - `Caption` / `Heading`: text in the caption and title styles.
   - `Checkbox`, `Radio`, `SearchField`.
2. **Tokens.** `shell.warning` and `shell.error` exist: `error` is the one danger red, used for
   errors and delete buttons alike. Add `shell.on_accent` (ink on the accent and on any status
   fill) and `shell.ai_ink` (ink on the agent's lime chip). Add `size::CHEVRON`,
   `size::DIALOG_S` and `size::DIALOG_M`; the label column is `size::LABEL_COLUMN` (no second
   name). The four error reds (`#e06060`, `#ff0000`, `#ff4040`, and the orange of
   `renumber_dialog.rs`) and the pure yellow warnings all become `shell.error` / `shell.warning`.
3. **Convert** `sheets_ui`, `outputs_ui`, `merge_dialog`, `renumber_dialog`, `release_ui` and
   `debugger` to `ecad_ui`; the hand-made section headers to `SectionHeader`; the hand-made
   property rows to `PropertyRow`; the hand-made `egui::Window` dialogs to `dialog::Dialog`. The
   preview window (a viewer that is moved and resized) and the search palette stay windows.
   **As built:** the questions became `dialog::Dialog` (merge, undo merge, renumber, the six
   History dialogs, undo issue, issue Rev, convert, notice, close, file, page name). Windows
   that are not questions keep their window but use the kit inside: Release and Revisions (the
   person fixes things while they are open), Reports and Print Book (hosted in a workspace
   slot). The fold headers of History, Settings ▸ Pages and the connector panel use
   `SectionHeader`; the group rows of the Devices list and the Project tree (drag, rename, +)
   are tree rows, not section headers, and are left for the views lane (064).
4. **Ratchet.** Lower the `STYLE_ALLOWLIST` and `RAW_CONTROL_ALLOWLIST` counts in `xtask` to what
   remains. 060 widens the same list in parallel; on merging, keep both changes and recount.
5. **Split** `apps/ecad-trial/src/lib.rs` into `args`, `file_ops`, `actions` and `windows/*`.
   Moves only, no behaviour change.

## Out of scope

The `msg` module for refusals (R1), canvas geometry constants, any new behaviour.

## Acceptance

- `cargo xtask layers` passes with lower counts; the gallery test passes in both themes and its
  images are refreshed.
- The trial's existing tests pass for the converted screens (they find controls by label, so
  labels stay as they were).
- Side-by-side screenshots (before and after, half scale) of each converted screen, for the
  owner's look check.
- `cargo xtask wasm` once at the end (`ecad-ui` and `ecad-tokens` build for wasm).
