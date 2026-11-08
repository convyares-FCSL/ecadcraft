# Brief 014: Edits made in the drawing scale with what changed

Status: Approved (owner, 2026-11-07: "Separate 014 now")
Branch: `ecad/drawing-edit-speed`
Designer: no (no visible change) · Size: M
Depends on: 013 (PR #31) and 012 (PR #30). This branch starts from 013's branch with 012 merged in,
and is rebased as they merge.

## Goal

A person's edit in the drawing (move a component, draw a wire, erase, copy) costs in proportion
to what it touches, not to the size of the project, with **exactly the same results** as today.
Brief 013 did this for the model (`Ledger::commit`). This brief does it for the step before:
turning a CADCraft commit into ECAD ops (`ecad-cadcraft/src/translate.rs`).

## What exists today

`translate::translate` compares the engine's drawing with the last projection, classifies each
difference, applies the movement policy (§10: M1 reroute, M2 connected equipment, M3 reconnection)
and returns ops. Per edit, regardless of how much changed, it:
- **clones the whole document twice:** `preview` (`translate.rs:87`) is called from `translate`
  (`:244`) and from `movement` (`:421`), and each call clones the `Document` and runs the full
  `Document::check`;
- **goes through every conductor in the project** in `movement` (`:426`, over
  `proposed.conductors`), not only those whose ends could have moved;
- **builds the positions of every terminal on the page twice** (`positions`, `:102`).

The engine-side drawing comparison (`entities`, `table_changes`) is per page and inside CADCraft's
data. It is out of scope here.

## Scope

1. **Measure first.** Add a `draw edit ms` column to `cargo xtask bench`: on the loaded project,
   in a headless CADCraft session with page 1 open, time one edit made the way a person makes it,
   through the engine and translate. Time two edits, each a median: (a) move a component that has
   wires attached; (b) draw one wire between two terminals. Record the result on the branch's
   base **before** changing `translate.rs`, and report both tables.
2. **Preview without cloning.** Add a public, minimal API in `ecad-model` that applies ops in
   place, runs a read-only closure on the result, and always rolls back using 013's journal. Use
   the drop guard, so it also rolls back on a panic. For example,
   `Document::with_applied(&ops, |d| …) -> Result<R>`, which returns the scoped-check result. Both
   `preview` calls use it, with no clone and no full check. The full check still runs in verify
   mode, through 013's machinery.
3. **Movement only where something moved.** Decide which conductors can be affected:
   - conductors attached (through 013's terminal→conductor index) to terminals whose shown
     position on the page changed;
   - plus `fresh_ends`;
   - plus conductors whose own route items the edit changed.

   Go only through those. Compute terminal positions only for the terminals involved, or once per
   edit, not twice for the whole page.
4. **Exactly the same results.** For every edit, the new code produces the same ops in the same
   order as the old code. Prove it with a differential test:
   - keep the old functions under `#[cfg(test)]` (or a test-only module);
   - run them and the new ones on every interactive, mouse, cable and reference test scenario
     already in the repo, plus a randomised set of moves, wires, erases and copies on the small
     benchmark project;
   - require byte-identical ops.

   Remove the old code only when the differential test has served its purpose; keeping it as a
   test oracle is fine.
5. Nothing else changes: no format, op or behaviour change, no change to refusals or their
   wording, and no change outside `ecad/`.

## Acceptance criteria

1. The `draw edit ms` column exists, with before and after tables (small and medium tiers) in the
   PR. On medium, both edits are no longer dominated by project size: a move on medium costs about
   the same as on small, give or take the size of the page.
2. The differential test passes on all existing scenarios and at least 1,000 randomised edits,
   with identical ops.
3. All existing `ecad-cadcraft` and `ecad-trial` tests pass unchanged, including mouse, tools,
   cable slash, cable views and the trial's headless quick-start test.
4. A panic inside the preview closure leaves the document, indexes and log unchanged (test with
   `catch_unwind`).
5. Verify mode (013's feature) still cross-checks every preview with the full check.

## Needs owner check

- Try the medium benchmark project (`--keep`) in the trial and say whether moving a component
  feels instant.
