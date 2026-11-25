//! Library crates return typed errors: no `Result<_, String>` and no `Err(format!(` outside the crates
//! listed in [`STRING_ERROR_EXEMPT`], and none in the engine's command files.

use super::*;

/// The engine's session command files and folders: they report errors as `CmdError`, never as text.
pub(crate) const ENGINE_COMMAND_FILES: &[&str] = &[
    "crates/ecad-engine/src/cables/commands.rs",
    "crates/ecad-engine/src/commands",
    "crates/ecad-engine/src/connectors/commands.rs",
    "crates/ecad-engine/src/devices/commands.rs",
    "crates/ecad-engine/src/pages/commands",
    "crates/ecad-engine/src/plc/commands",
    "crates/ecad-engine/src/references/commands",
    "crates/ecad-engine/src/releases/commands.rs",
    "crates/ecad-engine/src/reports/commands.rs",
    "crates/ecad-engine/src/styles/commands.rs",
    "crates/ecad-engine/src/symbols/commands.rs",
    "crates/ecad-engine/src/terminal_strips/commands.rs",
    "crates/ecad-engine/src/title_blocks/commands.rs",
    "crates/ecad-engine/src/wires/commands.rs",
    "crates/ecad-engine/src/generic.rs",
];

/// No library crate reports an error as text: no `Result<_, String>` and no `Err(format!(`. A hard
/// zero, with two kinds of exception, each with its reason and no count: Programs and Dev crates (a
/// program or tool may return text from `main`, or use `anyhow`), and the crates listed here.
pub(crate) const STRING_ERROR_EXEMPT: &[(&str, &str)] = &[
    ("ecad-engine", "its errors move to its error type next; its command files already hold none (ENGINE_COMMAND_FILES)"),
    ("ecad-app", "its errors move to typed errors next"),
];

/// The lines of `text` (outside `//` comments) with a `Result<_, String>`, and with an
/// `Err(format!(`.
pub(crate) fn string_error_counts(text: &str) -> (usize, usize) {
    let code = strip_line_comments(text);
    let results = code.lines().filter(|l| l.contains("Result<") && (l.contains(", String>") || l.contains(",String>"))).count();
    let formats = code.lines().filter(|l| l.contains("Err(format!(")).count();
    (results, formats)
}

/// The text errors of every crate that neither [`STRING_ERROR_EXEMPT`] nor the Programs and Dev layers
/// exempt, of the engine's command files, and of the `CmdError` bridges from text: all must be none.
pub(crate) fn string_error_violations(meta: &Value) -> Result<Vec<String>, String> {
    const FIX: &str = "return a typed error: the crate's thiserror enum";
    let mut out = Vec::new();
    let packages = classed_packages(meta)?;
    for (name, class, dir, _) in &packages {
        let Some(dir) = dir else { continue };
        if matches!(class, Class::Programs | Class::Dev | Class::Exempt) || STRING_ERROR_EXEMPT.iter().any(|(n, _)| n == name) {
            continue;
        }
        let mut files = Vec::new();
        rust_files(&dir.join("src"), &mut files);
        let (mut results, mut formats) = (0, 0);
        for f in files {
            let text = std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
            let (r, e) = string_error_counts(&text);
            (results, formats) = (results + r, formats + e);
        }
        if results + formats > 0 {
            out.push(format!("{name}: {results} lines with `Result<_, String>` and {formats} with `Err(format!(`; {FIX}"));
        }
    }
    for (n, _) in STRING_ERROR_EXEMPT {
        if !packages.iter().any(|(name, ..)| name == n) {
            out.push(format!("{n}: in STRING_ERROR_EXEMPT but not a workspace member; remove the entry"));
        }
    }
    let root = PathBuf::from(meta["workspace_root"].as_str().ok_or("cargo metadata: no workspace root")?);
    let count_in = |dir: &Path, f: &dyn Fn(&str) -> (usize, usize)| -> Result<(usize, usize), String> {
        let mut files = Vec::new();
        rust_files(dir, &mut files);
        let mut sum = (0, 0);
        for p in files {
            let text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
            let (a, b) = f(&text);
            sum = (sum.0 + a, sum.1 + b);
        }
        Ok(sum)
    };
    let cmd = "ecad-engine command files";
    let (mut r, mut e) = (0, 0);
    for p in ENGINE_COMMAND_FILES {
        let path = root.join(p);
        let (a, b) = if path.is_file() {
            let text = std::fs::read_to_string(&path).map_err(|err| format!("{}: {err}", path.display()))?;
            string_error_counts(&text)
        } else {
            count_in(&path, &string_error_counts)?
        };
        r += a;
        e += b;
    }
    if r + e > 0 {
        out.push(format!("{cmd}: {r} lines with `Result<_, String>` and {e} with `Err(format!(`; return CmdError"));
    }
    let bridges = |t: &str| {
        let code = strip_line_comments(t);
        (code.lines().filter(|l| l.contains("impl From<String> for CmdError") || l.contains("impl From<&'static str> for CmdError")).count(), 0)
    };
    let (b, _) = count_in(&root.join("crates/ecad-electrical/src"), &bridges)?;
    if b > 0 {
        out.push(format!("ecad-electrical: {b} CmdError bridges from text; return a CmdError variant"));
    }
    Ok(out)
}
