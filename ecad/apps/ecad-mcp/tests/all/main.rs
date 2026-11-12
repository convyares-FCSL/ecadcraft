//! Every integration test of this crate in one test executable (brief 017): each former
//! `tests/<name>.rs` is the module `<name>`, so its tests are named `<name>::<test>` and
//! `cargo test -p ecad-mcp <name>` still runs one file's tests. One executable links the
//! dependencies once instead of once per file.

mod cable_command_contracts;
mod demo;
mod history_contracts;
mod mcp_contracts;
mod mcp_tester_contracts;
mod merge_contracts;
mod merge_tester_contracts;
mod outputs_contracts;
mod release_contracts;
mod tester_024_mcp;
mod xref_contracts;
