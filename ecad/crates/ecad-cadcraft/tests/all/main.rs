//! Every integration test of this crate in one test executable (brief 017): each former
//! `tests/<name>.rs` is the module `<name>`, so its tests are named `<name>::<test>` and
//! `cargo test -p ecad-cadcraft <name>` still runs one file's tests. One executable links the
//! dependencies once instead of once per file.

// Lanes in flight add their modules in `lanes/`, not to the list below (see `lanes/mod.rs`).
mod lanes;

mod common;

mod accessories_contracts;
mod actions_contracts;
mod active_page_reproject;
mod bom_contracts;
mod cable_look_contracts;
mod cable_look_tester_contracts;
mod cable_reference_contracts;
mod cable_reference_tester_contracts;
mod cable_slash_acceptance_contracts;
mod cable_slash_contracts;
mod cable_view_contracts;
mod cable_view_tester_contracts;
mod canvas_marks_contracts;
mod census;
mod connector_contracts;
mod continuation_contracts;
mod debugger;
mod defaults_contracts;
mod device_contracts;
mod experiment;
mod files;
mod history_sessions_contracts;
mod history_tester_contracts;
mod interactive_contracts;
mod levels_contracts;
mod library_contracts;
mod mouse_tools_contracts;
mod numbering_contracts;
mod outputs_contracts;
mod pack_rerun_fixes;
mod plc_contracts;
mod plc_visuals_contracts;
mod print_project_contracts;
mod project_pages;
mod recovery;
mod refusals;
mod release_contracts;
mod sheets_contracts;
mod strip_page_contracts;
mod strips_contracts;
mod style_contracts;
mod symbol_builder_contracts;
mod tag_rule_contracts;
mod terminal_name_contracts;
mod tester_014_r3_scope_probe;
mod tester_023_contracts;
mod tester_024_probe;
mod tester_025_xref_probe;
mod tester_026_numbering;
mod tester_027_release;
mod tester_028_strips;
mod tester_029_tag_rule;
mod tester_030_accessories;
mod tester_031_api;
mod tester_031_text;
mod tester_032_wires_nets;
mod tester_033_print;
mod tester_034_api;
mod tester_034_library;
mod tester_036_levels;
mod tester_044_connectors;
mod tester_045_plc;
mod tester_050a_style;
mod tester_050b_wire_categories;
mod text_place_contracts;
mod text_show_contracts;
mod text_style_contracts;
mod upstream_sync_019_contracts;
mod upstream_sync_contracts;
mod wires_nets_contracts;
mod wiring_contracts;
mod xref_contracts;
