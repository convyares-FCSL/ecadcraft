//! Every integration test of this crate in one test executable (brief 017): each former
//! `tests/<name>.rs` is the module `<name>`, so its tests are named `<name>::<test>` and
//! `cargo test -p ecad-cadcraft <name>` still runs one file's tests. One executable links the
//! dependencies once instead of once per file.

mod common;

mod accessories_contracts;
mod cable_look_contracts;
mod cable_look_tester_contracts;
mod cable_reference_contracts;
mod cable_reference_tester_contracts;
mod cable_slash_acceptance_contracts;
mod cable_slash_contracts;
mod cable_view_contracts;
mod cable_view_tester_contracts;
mod census;
mod debugger;
mod device_contracts;
mod experiment;
mod files;
mod history_tester_contracts;
mod interactive_contracts;
mod mouse_tools_contracts;
mod numbering_contracts;
mod outputs_contracts;
mod project_pages;
mod recovery;
mod refusals;
mod release_contracts;
mod sheets_contracts;
mod strips_contracts;
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
mod text_style_contracts;
mod upstream_sync_019_contracts;
mod upstream_sync_contracts;
mod wires_nets_contracts;
mod wiring_contracts;
mod xref_contracts;
