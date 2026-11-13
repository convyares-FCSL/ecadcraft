//! Every integration test of this crate in one test executable (brief 017).

// Lanes in flight add their modules in `lanes/`, not to the list below (see `lanes/mod.rs`).
mod lanes;

mod common;

mod cli_contracts;
mod protocol_contracts;
mod serve_contracts;
mod tester_029_serve;
