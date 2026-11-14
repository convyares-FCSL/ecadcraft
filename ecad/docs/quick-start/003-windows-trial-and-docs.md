# Brief 003: windows trial and docs

Part of the [quick start](../quick-start.md). A section number refers to a section of this file
when one is headed with it here, otherwise to the quick start's (1–10, 5A…).

## What this brief changed

**Branch `ecad/windows-trial` (brief 003)**, cut from `dev` at `0afaf1d`, makes the trial behave
natively on Windows: `~` in a typed path is your home folder (on Windows your user profile, such
as `C:\Users\<you>`, whether or not `HOME` is set) and may be written `~/` or `~\`; the Save As
dialog suggests `C:\dev\ecad-projects` on Windows (`~/ecad-projects` on Linux) and creates that
folder the first time you save into it; a reserved device name such as `CON.ecad` is refused;
typing the project's own file in another letter case saves it without a "replace?" question;
release builds on Windows open no console window. Until it is merged, `dev` behaves as the
previous version of this guide said. For this branch there are automated checks only (below),
run on native Windows 11 Pro (10.0.26200) with Rust 1.99.1, 2026-11-01; nobody has tried it on a
display yet.
