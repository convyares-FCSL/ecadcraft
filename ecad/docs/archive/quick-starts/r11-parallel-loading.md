Historical record (2026-11-21). Not current; see [developer/debugger.md](../../developer/debugger.md).

# R11: parallel work and the UI thread

Lane R11 of the 0.2.3 refactor (design 061, rayon). What changed, and how to try it.

## What changed

- **Opening a file.** The chunks of a snapshot and the ranges of a long history are read in
  parallel with rayon (one task per chunk or range, results in their order), replacing ECAD's own
  thread code. The project's index builds its large maps side by side the same way. Nothing a
  person sees changes; the opened project is the same.
- **Debugger checks run in the background.** **Verify replay**, **Verify projection**, **Verify
  file** and **Check index** copy what they need (the project is locked only for the copy), then
  run on a small background pool that leaves one core for the window. The panel shows
  `Running n checks…` until the results arrive on a later frame. If the project changes while a
  check runs (a commit moves its head), that result is dropped, and the panel says how many were
  dropped; press the button again to check the new state.

## Try it

1. Open a project, open the debugger (ECAD Dev tool set, **Debugger**), go to **Checks**.
2. Press **Verify replay**, **Verify projection** and **Verify file**: `Running 3 checks…` shows
   briefly, then each shows `PASS … at seq n` and the time. The window stays responsive meanwhile.
3. Press **Verify file** on a large project and, while it runs, place a symbol: no result is shown
   for the old state, and the panel says `1 result dropped: the project changed while the check
   ran.`

## Checked

- Automated: `lanes::lane_r11` in ecad-app (a commit while checks run drops their results; started
  again at the new head they are applied) and in ecad-engine (the background jobs: stale results
  dropped, a panicking job is a failure, not a crash). The file reader's own tests, including the
  hostile-input tests, are unchanged.
- Not yet tried by the owner.
