//! `ecad-desktop`: the ECAD desktop program. It parses the command line
//! ([`args`]), sizes and places the window ([`window`]), times start-up ([`startup`]) and runs
//! ecad-app's [`ecad_app::shell::app::App`] in an eframe window (`main.rs`).
//!
//! This library holds the program's parts so that they can be tested; the binary is `main.rs`.
//! It excludes everything a person sees and clicks inside the window: that is ecad-app.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod args;
pub mod folder;
pub mod host;
pub mod startup;
pub mod window;

pub use args::{Invocation, USAGE, parse_args};
