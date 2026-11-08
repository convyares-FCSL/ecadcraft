//! Every integration test of this crate in one test executable (brief 017): each former
//! `tests/<name>.rs` is the module `<name>`, so its tests are named `<name>::<test>` and
//! `cargo test -p ecad-trial <name>` still runs one file's tests. One executable links the
//! dependencies once instead of once per file.

mod args_contracts;
mod cable_look_tester;
mod cable_references;
mod cable_references_tester;
mod cable_slash;
mod cable_slash_tester;
mod cable_view;
mod cable_view_tester;
mod debugger;
mod layouts_and_hover;
mod mouse;
mod mouse_tester;
mod paths_contracts;
mod startup_contracts;
mod startup_tester;
mod tools;
mod trial;
mod windows_trial_contracts;
