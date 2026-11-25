//! Every integration test of this crate in one test executable: each former
//! `tests/<name>.rs` is the module `<name>`, so its tests are named `<name>::<test>` and
//! `cargo test -p ecad-file <name>` still runs one file's tests. One executable links the
//! dependencies once instead of once per file.

mod common;

mod container;
mod format;
mod history;
mod verify;
