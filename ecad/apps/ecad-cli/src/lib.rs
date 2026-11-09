//! ecad-cli
//!
//! ECAD's command-line tools (brief 021):
//!
//! - `ecad upgrade <file> [--out <file>]`: convert a version 1 project file to a new version 2
//!   file (EEP-0003 §16.2 option a), verified by replaying both; the source is never changed.
//! - `ecadinspect <file> <view>` (also `ecad inspect <file> <view>`): read-only views of a
//!   version 2 file: `tables`, `branches`, `log <branch> [--last N]`, `versions`, `part <hash>`,
//!   `verify`, and `export-jsonl [--with-raw] [--out <file>]`.
//!
//! Excluded: anything that changes a version 2 file.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use std::io::Write;
use std::path::Path;

use ecad_file::v2::{Store, inspect};

/// How `ecad` is used.
pub const USAGE: &str = "usage:
  ecad upgrade <file.ecad> [--out <new.ecad>]
  ecad inspect <file.ecad> <view>     (the same as ecadinspect)

ecadinspect <file.ecad> <view>, where <view> is one of:
  tables                     row counts and sizes of every table, and meta
  branches                   branches, fork points and heads
  log <branch> [--last N]    the last N transactions (default 20), decompressed
  versions                   named versions, revisions and automatic snapshots
  part <hash>                one snapshot part, decompressed
  verify                     replay from the root and check every snapshot (reads everything)
  export-jsonl [--with-raw] [--out <file>]
                             the whole history as a version 1 JSON Lines file (stdout by default);
                             --with-raw writes every stored row with its raw bytes instead";

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

/// Whether `flag` is in `args` (removed).
fn take_flag(args: &mut Vec<String>, flag: &str) -> bool {
    let before = args.len();
    args.retain(|a| a != flag);
    args.len() != before
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
        Some("inspect") => ecadinspect(args.get(1..).unwrap_or_default(), out),
        _ => Err(USAGE.into()),
    }
}

/// This build, as recorded in files it writes.
pub fn writer() -> String {
    format!("ecad-cli {}", env!("CARGO_PKG_VERSION"))
}

/// Run `ecadinspect <args>`.
///
/// # Errors
/// Bad usage, a version 1 or unreadable file, or the view failed.
pub fn ecadinspect(args: &[String], out: &mut dyn Write) -> Result<(), String> {
    let mut args = args.to_vec();
    let last = take_opt(&mut args, "--last")?;
    let to = take_opt(&mut args, "--out")?;
    let with_raw = take_flag(&mut args, "--with-raw");
    let (Some(file), Some(view)) = (args.first().cloned(), args.get(1).cloned()) else { return Err(USAGE.into()) };
    let path = Path::new(&file);
    match ecad_file::v2::convert::sniff(path).map_err(|e| format!("{file}: {e}"))? {
        ecad_file::v2::convert::Major::V2 => {}
        ecad_file::v2::convert::Major::V1 => {
            return Err(format!("{file} is a version 1 file; convert it with `ecad upgrade {file}`"));
        }
        ecad_file::v2::convert::Major::Unknown => return Err(format!("{file} is not an ECAD project file")),
    }
    let store = Store::open(path, false).map_err(|e| format!("{file}: {e}"))?;
    let p = |r: Result<String, ecad_file::v2::StoreError>| r.map_err(|e| e.to_string());
    let text = match (view.as_str(), args.get(2)) {
        ("tables", None) => {
            let len = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            format!("{file}: {len} bytes\n\n{}", p(inspect::tables(&store))?)
        }
        ("branches", None) => p(inspect::branches(&store))?,
        ("log", Some(branch)) => {
            let n = match last {
                Some(n) => n.parse::<u64>().map_err(|_| format!("--last {n} is not a number"))?,
                None => 20,
            };
            p(inspect::log(&store, branch, n))?
        }
        ("versions", None) => p(inspect::versions(&store))?,
        ("part", Some(hash)) => p(inspect::part(&store, hash))?,
        ("verify", None) => {
            let v = ecad_file::v2::verify::verify(&store);
            format!("{}\n{}", v.lines.join("\n"), if v.ok { "OK" } else { "FAILED" })
        }
        ("export-jsonl", None) => {
            let n = match &to {
                Some(dest) => {
                    if Path::new(dest).exists() {
                        return Err(format!("{dest} already exists; it is never overwritten"));
                    }
                    let mut f =
                        std::io::BufWriter::new(std::fs::File::create_new(dest).map_err(|e| format!("{dest}: {e}"))?);
                    let n = inspect::export_jsonl(&store, with_raw, &mut f)?;
                    f.flush().map_err(|e| e.to_string())?;
                    n
                }
                None => inspect::export_jsonl(&store, with_raw, out)?,
            };
            match to {
                Some(dest) => format!("wrote {n} lines to {dest}"),
                None => return Ok(()),
            }
        }
        _ => return Err(USAGE.into()),
    };
    writeln!(out, "{}", text.trim_end()).map_err(|e| e.to_string())
}
