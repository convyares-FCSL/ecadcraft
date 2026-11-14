# Brief 016: project tab

Part of the [quick start](../quick-start.md). A section number refers to a section of this file
when one is headed with it here, otherwise to the quick start's (1–10, 5A…).

## What this brief changed

**Brief 016 (design `ecad/docs/design/016.md`), on `feature/project-manager`, not yet in `dev`:**
the Project panel, one active page at a time, undo and redo bringing their page to the front, a
read-only preview of another page, and the top banner replaced by the panel's header and an
folded **Developer** section at the bottom of the panel (design 016, "Top bar removed"). Sections 4, 5E, 6, 7 and 8 describe it.
- *Automated checks:* `ecad/apps/ecad-trial/tests/all/project_panel.rs` (design 016 checks A to J by
  label in a 1280×800 headless window, and the six `016-*.png` screenshots), the one-window and
  undo checks in `ecad/crates/ecad-cadcraft/tests/all/project_pages.rs`, check K in
  `ecad/apps/ecad-mcp/tests/all/mcp_contracts.rs`, and `the_quick_start_trial_runs_headless`
  (sections 5 to 7 below).
- *Agent-observed:* the worker rendered the `016-*.png` reference screenshots headless (wgpu) and
  looked at them; no agent has clicked through the live window.
- *Owner-tried:* not yet.

This guide describes `dev` with stage 2b (the debugger panel, PR #11) and the ECAD tab in Tool
Sets (PR #12) merged on 2026-10-15, on top of stage 2a, now built on **CADCraft 0.19.0**
(upstream tag `v0.19.0`, commit `227e3c38`, brief 023, 2026-11-09; egui 0.37, Rust 1.95 or later;
before that CADCraft 0.15.1, brief 007). The steps below did not change with either sync; the
Block Editor, REFEDIT and LISP are refused on ECAD pages, and BREAK, TRIM and OOPS are refused
where they would split or bring back an object with an electrical meaning (section 9). Your trials
recorded below were on the earlier CADCraft 0.6.2 base, except your click-only trial of brief 008
on native Windows, which was on 0.15.1.
