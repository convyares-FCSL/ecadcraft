//! Every integration test of this crate in one test executable (brief 017): each former
//! `tests/<name>.rs` is the module `<name>`, so its tests are named `<name>::<test>` and
//! `cargo test -p ecad-file <name>` still runs one file's tests. One executable links the
//! dependencies once instead of once per file.

mod common;

mod branches_contracts;
mod branches_tester_contracts;
mod canonical_contracts;
mod container_crash;
mod container_recovery;
mod container_v2;
mod disk_contracts;
mod disk_edge_contracts;
mod incremental_acceptance;
mod merge_store_contracts;
mod records_contracts;
mod recovery_contracts;
mod save_contracts;
mod merge_store_tester_contracts;
