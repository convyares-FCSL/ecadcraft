//! Every integration test of this crate in one test executable (brief 017): each former
//! `tests/<name>.rs` is the module `<name>`, so its tests are named `<name>::<test>` and
//! `cargo test -p ecad-trial <name>` still runs one file's tests. One executable links the
//! dependencies once instead of once per file.

mod args_contracts;
mod cable_look_tester;
mod cable_references;
mod cable_references_tester;
mod cable_slash;
mod cable_slash_tester;
mod cable_view;
mod cable_view_tester;
mod checks_ui;
mod debugger;
mod devices_ui;
mod history_ui;
mod layouts_and_hover;
mod merge_tester_ui;
mod merge_ui;
mod mouse;
mod mouse_tester;
mod outputs_ui;
mod paths_contracts;
mod project_panel;
mod sheets_ui;
mod startup_contracts;
mod startup_tester;
mod tools;
mod trial;
mod windows_trial_contracts;
mod xref_ui;
