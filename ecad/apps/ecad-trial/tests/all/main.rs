//! Every integration test of this crate in one test executable (brief 017): each former
//! `tests/<name>.rs` is the module `<name>`, so its tests are named `<name>::<test>` and
//! `cargo test -p ecad-trial <name>` still runs one file's tests. One executable links the
//! dependencies once instead of once per file.

// Lanes in flight add their modules in `lanes/`, not to the list below (see `lanes/mod.rs`).
mod lanes;

mod agent_banner_ui;
mod agent_ui;
mod args_contracts;
mod bom_ui;
mod bridge_contracts;
mod cable_look_tester;
mod cable_references;
mod cable_references_tester;
mod cable_slash;
mod cable_slash_tester;
mod cable_view;
mod cable_view_tester;
mod canvas_ui;
mod checks_ui;
mod continuation_ui;
mod debugger;
mod devices_ui;
mod history_ui;
mod iolist_ui;
mod layouts_and_hover;
mod merge_tester_ui;
mod merge_ui;
mod mouse;
mod mouse_tester;
mod numbering_ui;
mod outputs_ui;
mod paths_contracts;
mod plc_visuals_ui;
mod project_panel;
mod release_ui;
mod sheets_ui;
mod shell_ui;
mod standards_ui;
mod startup_contracts;
mod startup_tester;
mod strip_page_ui;
mod strips_ui;
mod style_tester_ui;
mod style_ui;
mod symbol_builder_ui;
mod table_ui;
mod tester_035_bridge;
mod text_ui;
mod tools;
mod trial;
mod ui_primitives;
mod uiv3_canvas;
mod uiv3_frame;
mod uiv3_inspector;
mod uiv3_library;
mod uiv3_panels;
mod uiv3_project;
mod uiv3_tools;
mod wheel_ui;
mod windows_trial_contracts;
mod xref_ui;
