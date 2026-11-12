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
  export-jsonl [--branch <name|id>] [--with-raw] [--out <file>]
                             a branch's history (default main) as a version 1 JSON Lines file
                             (stdout by default; the branches left out are named on stderr);
                             --with-raw writes every stored row with its raw bytes instead
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

/// Whether `flag` is in `args` (removed).
fn take_flag(args: &mut Vec<String>, flag: &str) -> bool {
    let before = args.len();
    args.retain(|a| a != flag);
    args.len() != before
}

/// Run `ecadinspect <args>`, `serve` included (which runs until the process is stopped).
///
/// # Errors
/// Bad usage, a version 1 or unreadable file, or the view failed.
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
    ecadinspect_with(args, out, &mut std::io::stderr())
}

/// The text views of `ecadinspect`, with notes (the branches `export-jsonl` left out) written to
/// `err`.
///
/// # Errors
/// As [`ecadinspect`].
pub fn ecadinspect_with(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> Result<(), String> {
    let mut args = args.to_vec();
    let branch = take_opt(&mut args, "--branch")?;
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
        ("export-jsonl", None) => {
            let n = match &to {
                Some(dest) => {
                    if Path::new(dest).exists() {
                        return Err(format!("{dest} already exists; it is never overwritten"));
                    }
                    let mut f = std::io::BufWriter::new(std::fs::File::create_new(dest).map_err(|e| format!("{dest}: {e}"))?);
                    let n = text::export_branch_jsonl(&store, with_raw, branch.as_deref().unwrap_or("main"), &mut f)?;
                    f.flush().map_err(|e| e.to_string())?;
                    n
                }
                None => text::export_branch_jsonl(&store, with_raw, branch.as_deref().unwrap_or("main"), out)?,
            };
            let (n, left) = n;
            if !left.is_empty() {
                writeln!(err, "left out: {} other branch(es): {}", left.len(), left.join(", ")).map_err(|e| e.to_string())?;
            }
            match to {
                Some(dest) => format!("wrote {n} lines to {dest}"),
                None => return Ok(()),
            }
        }
        _ => return Err(USAGE.into()),
    };
    writeln!(out, "{}", text.trim_end()).map_err(|e| e.to_string())
}
