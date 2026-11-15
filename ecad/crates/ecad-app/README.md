# ecad-app

Layer: App

> **How to build, run and try it:** [`docs/quick-start.md`](../../docs/quick-start.md), the single
> maintained guide. The program that runs this crate is `ecad-desktop`.

## Purpose
The ECAD application: the shell, workspaces, panels, dialogs, lists and tabs, on a CADCraft window.
Everything about a project goes through `ecad-cadcraft`; this crate is what a person sees and
clicks.

## Contents
- `lib.rs`: `App`, the application on CADCraft's window.
- `shell/`: the frame: top bar, left and right panels, status bar, tool strip, keys, layout.
- `actions`: the one table of actions (menus, buttons and shortcuts are built from it).
- `canvas/`, `preview`: the page canvas and the read-only preview of another page.
- Panels and dialogs: `project_panel`, `devices_panel`, `checks_panel`, `history_panel`, `settings/`, `lists/`, `library/`, `outputs_ui`, `publish`, `merge_dialog`, `renumber_dialog`, `text_ui`, `sheets_ui`, `release_ui`.
- `file_ops`, `folder`: New, Open, Save, Save As and Close, with the Save, Discard, Cancel question.
- `agent/`: live agent access (off by default; banner with Undo session, Pause and Disconnect).
- `debugger`, `developer`: the read-only debugger and the Developer section.

## Must not depend on
App rules: Core, Engine and UI kit crates, `cadcraft-ui-egui`, `eframe` and `egui`. Never
`cadcraft-engine` directly (go through `ecad-cadcraft`), and no Programs or Dev crate (except
through a `dev` feature). Panels and dialogs use only `ecad-ui` components for values.

## Key types
`App`, `actions::Action`, `actions::Context`.

## How to test
```bash
cargo test -p ecad-app
```
Tests are in `tests/all/` and drive the UI headlessly. Two of them follow the quick-start's steps
(`the_quick_start_runs_headless`, `the_quick_start_debugger_example_runs_headless`); keep
them in step with the guide. Slow tests are marked `#[ignore = "slow: …"]` and run only at the QA
gate.
