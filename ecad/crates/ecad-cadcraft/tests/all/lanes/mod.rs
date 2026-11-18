//! The test modules of lanes in flight (owner, 2026-11-13). A lane (a brief being built on its
//! feature branch) puts its tests in `lanes/lane_<NNN>.rs` (submodules in `lanes/lane_<NNN>/`) and
//! adds one line, `mod lane_<NNN>;`, at the end of this file, instead of adding modules to the list
//! in `main.rs`; its tests are named `lanes::lane_<NNN>::...`. Git merges this file with
//! `merge=union` (`ecad/.gitattributes`), so two lanes adding their line keep both instead of
//! conflicting; `cargo fmt` puts them back in order. At the phase-end tidy-up the lanes' modules
//! move into `main.rs`'s list and this file holds only this comment again.
mod lane_052;
mod lane_053;
mod lane_054;
mod lane_054_tester;
mod lane_055;
mod lane_056;
mod lane_057;
mod lane_057_tester;
mod lane_059;
mod lane_059_tester;
mod lane_060;
mod lane_062;
mod lane_064;
mod lane_066;
mod lane_070;
mod lane_070_strict;
mod lane_072a;
mod lane_072b;
mod lane_072c;
mod lane_072d;
mod lane_072d_cable;
mod lane_072d_cores;
mod lane_072d_junction;
mod lane_072d_terminals;
mod lane_072d_tester;
mod lane_072e;
