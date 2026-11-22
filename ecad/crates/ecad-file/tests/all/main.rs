//! Every integration test of this crate in one test executable (brief 017): each former
//! `tests/<name>.rs` is the module `<name>`, so its tests are named `<name>::<test>` and
//! `cargo test -p ecad-file <name>` still runs one file's tests. One executable links the
//! dependencies once instead of once per file.

// Lanes in flight add their modules in `lanes/`, not to the list below (see `lanes/mod.rs`).
mod lanes;

mod common;

mod branches_contracts;
mod branches_tester_contracts;
mod canonical_contracts;
mod chunk_key;
mod container_crash;
mod container_recovery;
mod container_v2;
mod history_sessions_contracts;
mod merge_store_contracts;
mod merge_store_tester_contracts;
mod records_contracts;
mod style_contracts;
mod symbol_definition_contracts;
mod tag_rule_contracts;
mod tester_029_contracts;
mod verify_contracts;
