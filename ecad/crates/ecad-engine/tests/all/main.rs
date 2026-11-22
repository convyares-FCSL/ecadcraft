//! Every integration test of this crate in one test executable (brief 017): each former
//! `tests/<name>.rs` is the module `<name>`, so its tests are named `<name>::<test>` and
//! `cargo test -p ecad-engine <name>` still runs one file's tests. One executable links the
//! dependencies once instead of once per file.

// Lanes in flight add their modules in `lanes/`, not to the list below (see `lanes/mod.rs`).
mod background;
mod cables;
mod circuits;
mod commands;
mod common;
mod connectors;
mod continuations;
mod defaults;
mod dev;
mod devices;
mod export;
mod history;
mod interact;
mod locations;
mod numbering;
mod pages;
mod plc;
mod pointer;
mod project;
mod properties;
mod references;
mod releases;
mod reports;
mod styles;
mod symbols;
mod terminal_strips;
mod title_blocks;
mod translate;
mod upstream;
mod views;
mod wires;
