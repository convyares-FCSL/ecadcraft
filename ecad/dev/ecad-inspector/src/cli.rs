//! `ecadinspect`: the text views (moved from `ecad-cli` by brief 029) and `serve`.

use std::io::Write;
use std::path::Path;

use ecad_file::v2::Store;

use crate::text;

/// How `ecadinspect` is used.
pub const USAGE: &str = "usage:
  ecadinspect serve [<file.ecad>] [--port N]
                             serve the inspector page and its protocol on 127.0.0.1 (read-only;
                             without a file, drop one on the page)
  ecadinspect <file.ecad> <view>, where <view> is one of:
  tables                     row counts and sizes of every table, and meta
  branches                   branches, fork points and heads
  log <branch> [--last N]    the last N transactions (default 20), decompressed
  versions                   named versions, revisions and automatic snapshots
  chunk <hash>               one snapshot chunk, decompressed
  verify                     the chain, the replay with the commit check, and the full check at
                             every snapshot and branch end (reads everything)
  <branch> is a name or a branch ID (32 hex digits); a name several branches share is refused";

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

/// Run `ecadinspect <args>`, `serve` included (which runs until the process is stopped).
///
/// # Errors
/// Bad usage, a file that is not a project or cannot be read, or the view failed.
pub fn ecadinspect(args: &[String], out: &mut dyn Write) -> Result<(), String> {
    if args.first().map(String::as_str) == Some("serve") {
        let mut rest = args.get(1..).unwrap_or_default().to_vec();
        let port = match take_opt(&mut rest, "--port")? {
            Some(p) => p.parse::<u16>().map_err(|_| format!("--port {p} is not a port number"))?,
            None => 0,
        };
        let file = match rest.as_slice() {
            [] => None,
            [f] => Some(std::path::PathBuf::from(f)),
            _ => return Err(USAGE.into()),
        };
        let server = crate::serve::Server::bind(file.as_deref(), port).map_err(|e| e.to_string())?;
        writeln!(out, "{}", server.banner()).map_err(|e| e.to_string())?;
        out.flush().map_err(|e| e.to_string())?;
        server.run();
        return Ok(());
    }
    views(args, out)
}

/// The text views of `ecadinspect` (everything but `serve`).
///
/// # Errors
/// As [`ecadinspect`].
pub fn views(args: &[String], out: &mut dyn Write) -> Result<(), String> {
    let mut args = args.to_vec();
    let last = take_opt(&mut args, "--last")?;
    let (Some(file), Some(view)) = (args.first().cloned(), args.get(1).cloned()) else { return Err(USAGE.into()) };
    let path = Path::new(&file);
    if !ecad_file::v2::disk::starts_like_project(path).map_err(|e| format!("{file}: {e}"))? {
        return Err(format!("{file} is not an ECAD project file"));
    }
    let store = Store::open(path, false).map_err(|e| format!("{file}: {e}"))?;
    let p = |r: Result<String, ecad_file::v2::StoreError>| r.map_err(|e| e.to_string());
    let text = match (view.as_str(), args.get(2)) {
        ("tables", None) => {
            let len = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            format!("{file}: {len} bytes\n\n{}", p(text::tables(&store))?)
        }
        ("branches", None) => p(text::branches(&store))?,
        ("log", Some(branch)) => {
            let n = match last {
                Some(n) => n.parse::<u64>().map_err(|_| format!("--last {n} is not a number"))?,
                None => 20,
            };
            p(text::log(&store, branch, n))?
        }
        ("versions", None) => p(text::versions(&store))?,
        ("chunk", Some(hash)) => p(text::chunk(&store, hash))?,
        ("verify", None) => {
            let v = ecad_file::v2::verify::verify(&store);
            format!("{}\n{}", v.lines.join("\n"), if v.ok { "OK" } else { "FAILED" })
        }
        _ => return Err(USAGE.into()),
    };
    writeln!(out, "{}", text.trim_end()).map_err(|e| e.to_string())
}
