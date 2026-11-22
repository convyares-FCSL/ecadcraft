//! Every integration test of this crate in one test executable (brief 017): each former
//! `tests/<name>.rs` is the module `<name>`, so its tests are named `<name>::<test>` and
//! `cargo test -p ecad-app <name>` still runs one file's tests. One executable links the
//! dependencies once instead of once per file.

// Lanes in flight add their modules in `lanes/`, not to the list below (see `lanes/mod.rs`).
mod cables;
mod canvas;
mod checks;
mod commands;
mod common;
mod developer;
mod devices;
mod history;
mod inspector;
mod library;
mod pages;
mod plc;
mod references;
mod reports;
mod settings;
mod shell;
mod terminal_strips;
mod title_blocks;
mod wires;
mod workspace;
