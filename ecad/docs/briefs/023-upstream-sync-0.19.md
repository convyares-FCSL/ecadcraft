# Brief 023: Upstream sync to CADCraft 0.19.0

Status: **Approved** (owner, 2026-11-09: "Yes, after #46")
Branch: `upstream-sync/2026-11-09` (worktree `C:\dev\ecad-features\upstream-sync`), from `dev` after
PR #46 (the rustfmt reformat).
Designer: no · Size: S–M (about a day, testing is the larger part) · Tester: yes (CADCraft seams)
Review: [`docs/discussions/upstream-review-2026-11.md`](../discussions/upstream-review-2026-11.md).

## Goal

ECAD runs on CADCraft **v0.19.0** (`227e3c38`, 2026-11-09) instead of 0.15.1 (`660e15c`), with every
ECAD-SEAM (X1–X10) intact and every ECAD test green.

## Scope

1. `git fetch https://github.com/storytold/cadcraft --tags`, then `git merge v0.19.0` on this branch.
   - `README.md`: keep ours.
   - Resolve the conflicts in `execute.rs`, `interactive.rs` and `file.rs` as in the review's
     `C:\dev\ecad-features\_tmp\sync-guide\upstream-review-2026-11-seam-resolution.diff`.
2. Apply the ECAD-side fixes from `_tmp\sync-guide\upstream-review-2026-11-ecad-fixes.diff`
   (`View.twist`, `Insert.clip`, `Services.pick_pgp` literals). That diff predates the reformat (#46),
   so apply it by hand where it doesn't apply cleanly, then run `cargo fmt`.
   - In `ecad/Cargo.toml`, add the `egui_extras` line and reword the patch comment (review §3).
   - Regenerate `ecad/Cargo.lock`.
3. Update `ecad/UPSTREAM.md` (commit `227e3c38…`, CADCraft 0.19.0, 2026-11-09) and the
   `UPSTREAM-PATCHES.md` wording, then run `cargo xtask patches`.
4. Fix the two failures the review found:
   - `refusals::replacing_or_unsupported_edits_are_refused`: BREAK, and probably TRIM, must be refused
     again on owned pages. This is an adapter fix.
   - The DEVICEPROPS prompt in `device_contracts`: bring ECAD's keyword prompts to upstream's
     "no 'or'" convention.
5. Overseer decisions (2026-11-09):
   - **OOPS, `-ARRAY` and `MINSERT` on owned pages:** record them properly where it's safe. Refuse
     them, with a reason, only if they would damage ECAD data; never a silent dead end. Add contract
     tests either way, plus a TRIM test that the handle is kept.
   - **Adopt upstream's DWG worker and window-size memory** in `ecad-trial` if they come free with
     the merge; anything more is a follow-up.
   - **Emergency-save hook (X5) for UI panics:** not in this brief. Note it in `UPSTREAM-PATCHES.md`
     as a candidate to offer upstream later.

## Checks

- Root workspace: `cargo test -p cadcraft-engine --test ext_owner --test ext_commands`, and
  `-p cadcraft-ui-egui --test owner_overlay --test toolsets_extra`.
- ECAD: fmt, clippy, and `cargo xtask test-changed`. Every crate counts as changed, since the lock
  file moved.
- Tester: undo and redo on owned pages after upstream's LISP-`U` change; a transparent command inside
  an owned command; the QSAVE/SAVEAS hand-over; the new contract tests.
- No full suite: that runs at the QA gate.

## Acceptance

1. `UPSTREAM.md` says 0.19.0, and `cargo xtask patches` passes.
2. All ECAD tests pass, and the seam tests in the root workspace pass.
3. Owner trial: the demo opens, and drawing, devices, wiring, save and reopen behave as before.
   Window size is remembered.
