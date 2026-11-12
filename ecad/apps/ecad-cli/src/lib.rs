//! ecad-cli
//!
//! ECAD's command-line tool `ecad` (brief 021):
//!
//! - `ecad upgrade <file> [--out <file>]`: convert a version 1 project file to a new version 2
//!   file (EEP-0003 §16.2 option a), verified by replaying both; the source is never changed.
//!
//! Excluded: anything that changes a version 2 file, and looking inside one: that is the
//! developer tool `ecadinspect` (`dev/ecad-inspector`, brief 029), which never ships. `ecad
//! inspect` says where it went.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::io::Write;
use std::path::Path;

/// How `ecad` is used.
pub const USAGE: &str = "usage:
  ecad upgrade <file.ecad> [--out <new.ecad>]";

/// What `ecad inspect` answers (brief 029: the text views moved to the developer tool).
pub const INSPECT_MOVED: &str = "`inspect` moved to the developer tool `ecadinspect`; run `cargo run -p ecad-inspector --bin ecadinspect -- <file> <view>` from ecad/ (or `ecadinspect serve <file>` for the inspector page)";

/// The value after `flag` in `args`, and `args` without both.
fn take_opt(args: &mut Vec<String>, flag: &str) -> Result<Option<String>, String> {
    match args.iter().position(|a| a == flag) {
        Some(i) => {
            let v = args.get(i + 1).cloned().ok_or_else(|| format!("{flag} needs a value"))?;
            args.drain(i..=i + 1);
            Ok(Some(v))
        }
        None => Ok(None),
    }
}

/// Run `ecad <args>`; what to print, or an error message.
///
/// # Errors
/// Bad usage, or the command failed.
pub fn ecad(args: &[String], out: &mut dyn Write) -> Result<(), String> {
    let mut args = args.to_vec();
    match args.first().map(String::as_str) {
        Some("upgrade") => {
            args.remove(0);
            let output = take_opt(&mut args, "--out")?;
            let [file] = args.as_slice() else { return Err(USAGE.into()) };
            let r = ecad_file::v2::convert::convert_file(Path::new(file), output.as_deref().map(Path::new), &writer())?;
            writeln!(out, "{}", r.describe()).map_err(|e| e.to_string())
        }
        Some("inspect") => Err(INSPECT_MOVED.into()),
        _ => Err(USAGE.into()),
    }
}

/// This build, as recorded in files it writes.
pub fn writer() -> String {
    format!("ecad-cli {}", env!("CARGO_PKG_VERSION"))
}
