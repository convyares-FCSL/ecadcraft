//! ecad-cli
//!
//! ECAD's command-line tool `ecad`. It has no commands in this version: `ecad upgrade` (the
//! version 1 converter) went with the version 1 format (brief 069), and `ecad inspect` says where
//! the text views went.
//!
//! Excluded: anything that changes a project file, and looking inside one: that is the developer
//! tool `ecadinspect` (`dev/ecad-inspector`, brief 029), which never ships.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::io::Write;

/// How `ecad` is used.
pub const USAGE: &str = "usage: ecad <command>
  this version has no commands; `ecad inspect` says where the developer tool for looking inside a project file is";

/// What `ecad inspect` answers (brief 029: the text views moved to the developer tool).
pub const INSPECT_MOVED: &str = "`inspect` moved to the developer tool `ecadinspect`; run `cargo run -p ecad-inspector --bin ecadinspect -- <file> <view>` from ecad/ (or `ecadinspect serve <file>` for the inspector page)";

/// Run `ecad <args>`; what to print, or an error message.
///
/// # Errors
/// Bad usage, or the command failed.
pub fn ecad(args: &[String], out: &mut dyn Write) -> Result<(), String> {
    let _ = out;
    match args.first().map(String::as_str) {
        Some("inspect") => Err(INSPECT_MOVED.into()),
        _ => Err(USAGE.into()),
    }
}
