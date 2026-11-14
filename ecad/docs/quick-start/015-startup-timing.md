# Brief 015: startup timing

Part of the [quick start](../quick-start.md). A section number refers to a section of this file
when one is headed with it here, otherwise to the quick start's (1–10, 5A…).

## What this brief changed

**Branch `ecad/startup-timing` (brief 015)**, cut from `dev` at `bb86e05`, adds start-up timing
to the trial, switched on by `ECAD_STARTUP_TIMING=1` (section 8, "Measuring start-up"). With the
switch off nothing changes. Evidence: `ecad/apps/ecad-trial/tests/all/startup_contracts.rs`, written
with the branch; three agent launches of the release trial on native Windows 11 Pro, 2026-11-08
(section 8). You have not run it yet.
