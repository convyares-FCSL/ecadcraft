# ecad-app

Layer: App

> **How to build, run and try it:** [`docs/user/quick-start.md`](../../docs/user/quick-start.md), the single
> maintained guide. The program that runs this crate is `ecad-desktop`.

## Purpose
The ECAD application: the shell, workspaces, panels, dialogs, lists and tabs, on a CADCraft window.
Everything about a project goes through `ecad-engine`; this crate is what a person sees and
clicks.

## Contents
`lib.rs` lists the areas, one line each. The desktop program uses `shell`, `canvas`, `library` and `title_blocks::draft`;
the other areas are `pub` only for this crate's tests and the agent-bridge tests in ecad-mcp.
- `shell/`: the frame and the `App` struct (`shell/app.rs`, its state in groups: files, command, hover, branch, seen, dialogs): top bar, panels, status bar, tool strip, keys,
  layout, file dialogs and the close flow. `shell/host.rs` is what the program supplies (default folder,
  login name, file manager, clock).
- `commands/`: the action table, what the app runs, the Tool Sets tab.
- `inspector/`: the right panel. `canvas/`: the page canvas. `pages/`: the Project panel's page list and previews.
- `workspace/`, `devices/`, `cables/`, `wires/`, `terminal_strips/`, `plc/`, `references/`, `checks/`, `reports/`,
  `publish/`: one screen area each.
- `title_blocks/`, `library/`: the title-block editor and the symbol library.
- `history/`, `settings/`, `agent/`, `developer/`: History, Settings, live agent access (off by default),
  the Developer section and the read-only debugger.

## Must not depend on
App rules: Core, Engine and UI kit crates, `cadcraft-ui-egui`, `eframe` and `egui`. Never
`cadcraft-engine` directly (go through `ecad-engine`), and no Programs or Dev crate (except
through a `dev` feature). Panels and dialogs use only `ecad-widgets` components for values.

## Key types
`shell::app::App`, `commands::actions::Action`, `commands::actions::Context`.

## How to test
```bash
cargo test -p ecad-app
```
Tests are in `tests/all/` and drive the UI headlessly. Two of them follow the quick-start's steps
(`the_quick_start_runs_headless`, `the_quick_start_debugger_example_runs_headless`); keep
them in step with the guide. Slow tests are marked `#[ignore = "slow: …"]` and run only at the QA
gate.
