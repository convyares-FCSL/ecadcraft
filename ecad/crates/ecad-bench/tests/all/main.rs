//! Every integration test of this crate in one test executable (brief 017): each former
//! `tests/<name>.rs` is the module `<name>`, so its tests are named `<name>::<test>` and
//! `cargo test -p ecad-bench <name>` still runs one file's tests. One executable links the
//! dependencies once instead of once per file.

mod generator_contracts;
mod tester_014_move_probe;
mod tester_014_r3_commit_cost;
mod tester_contracts;
