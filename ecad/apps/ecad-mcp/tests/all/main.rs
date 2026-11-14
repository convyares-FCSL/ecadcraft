//! Every integration test of this crate in one test executable (brief 017): each former
//! `tests/<name>.rs` is the module `<name>`, so its tests are named `<name>::<test>` and
//! `cargo test -p ecad-mcp <name>` still runs one file's tests. One executable links the
//! dependencies once instead of once per file.

// Lanes in flight add their modules in `lanes/`, not to the list below (see `lanes/mod.rs`).
mod lanes;

mod accessories_contracts;
mod bom_contracts;
mod cable_command_contracts;
mod connect_contracts;
mod connector_contracts;
mod continuation_contracts;
mod defaults_contracts;
mod demo;
mod history_contracts;
mod levels_contracts;
mod library_contracts;
mod mcp_contracts;
mod mcp_tester_contracts;
mod merge_contracts;
mod merge_tester_contracts;
mod numbering_contracts;
mod outputs_contracts;
mod pack_rerun_fixes;
mod plc_contracts;
mod plc_visuals_contracts;
mod print_project_contracts;
mod print_project_q7_contracts;
mod release_contracts;
mod review_026_merge;
mod strip_page_contracts;
mod strips_contracts;
mod style_contracts;
mod symbol_builder_contracts;
mod tester_024_mcp;
mod tester_026_mcp;
mod tester_027_mcp;
mod tester_028_mcp;
mod tester_030_mcp;
mod tester_031_mcp;
mod tester_032_mcp;
mod tester_033_mcp;
mod tester_034_mcp;
mod tester_035_mcp;
mod tester_036_mcp;
mod tester_044_mcp;
mod tester_045_mcp;
mod tester_050_mcp;
mod text_contracts;
mod wires_nets_contracts;
mod xref_contracts;
