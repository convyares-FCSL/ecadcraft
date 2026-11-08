//! Every integration test of this crate in one test executable (brief 017): each former
//! `tests/<name>.rs` is the module `<name>`, so its tests are named `<name>::<test>` and
//! `cargo test -p ecad-model <name>` still runs one file's tests. One executable links the
//! dependencies once instead of once per file.

mod connectivity_contracts;
mod device_contracts;
mod incremental_contracts;
mod ledger_contracts;
mod verify_mode_acceptance;
mod worked_example;
