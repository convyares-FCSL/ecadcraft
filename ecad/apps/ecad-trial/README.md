# ecad-trial: ECAD desktop trial harness (experiment)

> **How to build, run and try it:** [`../../docs/quick-start.md`](../../docs/quick-start.md).
> That guide is the single maintained set of instructions. It also records which revision was
> tried, and how: automated checks, an agent's virtual-display run, or by the owner.

`ecad-trial` is CADCraft's own window with the ECAD commands installed and one ECAD project open
at a time. It is an experiment for manual trials, not an ECAD application.

**What it adds to CADCraft's window:**
- A banner naming the experiment, the project, its file and its unsaved changes, with New,
  Open, Save and Save As.
- Project files (`.ecad`, persistence stage 2a):
  - Save writes to the project's file. Save As, and the first Save, asks for a file name in a
    small dialog of its own, because the trial has no native file dialogs.
  - A project that another session is editing, or one found damaged, opens read-only and can
    be saved as another file.
- A Save/Discard/Cancel question when a project with unsaved changes is closed. It replaces
  CADCraft's prompt for ECAD pages, through CADCraft's public close API.
- Optional diagnostic snapshots (`--snapshots <folder>`). These are not saved projects.
- An **ECAD** tab in CADCraft's Tool Sets panel, beside Drafting and Modeling (`src/tools.rs`), for
  testing: one button per
  implemented ECAD command. It runs Convert to Component (on the selection) and Inspect Project
  directly, and fills the Command field with an editable template for the others. It goes
  through the ordinary command path and never runs a placeholder.

**Not included:**
- native file dialogs;
- CADCraft file formats (plain drawings cannot be written);
- autosave and the control channel;
- saved window or profile settings;
- the live debugger (stage 2b);
- ECAD menus and interactive ECAD prompts.

**Usage:** `ecad-trial [--snapshots <folder>] [<project.ecad>]`.

The code is in `src/lib.rs`. The headless tests in `tests/trial.rs` include the quick-start
guide's steps (`the_quick_start_trial_runs_headless`); keep the two in step.
