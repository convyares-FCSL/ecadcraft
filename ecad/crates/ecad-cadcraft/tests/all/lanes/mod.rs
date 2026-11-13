//! The test modules of lanes in flight (owner, 2026-11-13). A lane (a brief being built on its
//! feature branch) puts its tests in `lanes/lane_<NNN>.rs` (submodules in `lanes/lane_<NNN>/`) and
//! adds one line, `mod lane_<NNN>;`, at the end of this file, instead of adding modules to the list
//! in `main.rs`; its tests are named `lanes::lane_<NNN>::...`. Git merges this file with
//! `merge=union` (`ecad/.gitattributes`), so two lanes adding their line keep both instead of
//! conflicting; `cargo fmt` puts them back in order. At the phase-end tidy-up the lanes' modules
//! move into `main.rs`'s list and this file holds only this comment again.
mod lane_043;
