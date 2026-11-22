//! Review finding #68 (0.2.3): a `pub` item no other crate uses is `pub(crate)` (the
//! `unreachable_pub` idea, as a textual ratchet because the lint cannot see across a workspace
//! and the crates re-export nothing).
//!
//! Per library crate (Core, Engine, UI kit, App) it counts the top-level `pub fn`, `struct`, `enum`,
//! `const`, `static`, `type` and `trait` items in `src/` (outside comments and a file's trailing
//! `#[cfg(test)]` module) whose name appears in no other workspace crate (source, tests and
//! benches, as a whole word). The count of each crate is kept in [`ALLOWED`]: a ratchet that only
//! falls. Lower the number when items are narrowed; a crate at 0 is removed from the table.
//! `cargo xtask pub-unreachable` lists the items.
//!
//! Textual and so approximate: a name another crate happens to use for something else keeps its
//! item out of the count, which only makes the ratchet easier to satisfy, never wrongly strict.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde_json::Value;

/// The crates that still hold unreachable `pub` items, and how many. The count only falls.
const ALLOWED: &[(&str, usize)] = &[
    ("ecad-app", 495),
    ("ecad-drawing-style", 14),
    ("ecad-electrical", 633),
    ("ecad-engine", 133),
    ("ecad-file", 44),
    ("ecad-model", 116),
    ("ecad-store", 6),
    ("ecad-widgets", 33),
];

const KINDS: &[&str] = &["const fn ", "fn ", "struct ", "enum ", "const ", "static ", "type ", "trait "];

/// The words (identifiers) of `text`.
fn words(text: &str, into: &mut BTreeSet<String>) {
    let mut cur = String::new();
    for c in text.chars().chain(std::iter::once(' ')) {
        if c.is_alphanumeric() || c == '_' {
            cur.push(c);
        } else if !cur.is_empty() {
            into.insert(std::mem::take(&mut cur));
        }
    }
}

/// The name of a column-0 `pub` item on `line`.
fn item_name(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("pub ")?;
    let rest = KINDS.iter().find_map(|k| rest.strip_prefix(k))?;
    let end = rest.find(|c: char| !(c.is_alphanumeric() || c == '_')).unwrap_or(rest.len());
    rest.get(..end).filter(|n| !n.is_empty())
}

/// For each library crate: `(file relative to the workspace, line, name)` of its unreachable items.
pub fn unreachable(meta: &Value) -> Result<BTreeMap<String, Vec<(String, usize, String)>>, String> {
    let root = PathBuf::from(meta["workspace_root"].as_str().ok_or("cargo metadata: no workspace root")?);
    let mut all_words: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut items: BTreeMap<String, Vec<(String, usize, String)>> = BTreeMap::new();
    for p in meta["packages"].as_array().ok_or("cargo metadata: no packages")? {
        let name = p["name"].as_str().unwrap_or_default().to_string();
        let Some(dir) = p["manifest_path"].as_str().map(PathBuf::from).and_then(|m| m.parent().map(std::path::Path::to_path_buf)) else {
            continue;
        };
        if !dir.starts_with(&root) {
            continue;
        }
        let library = matches!(super::class_of(&name), Some(super::Class::Core | super::Class::Engine | super::Class::UiKit | super::Class::App));
        let mut paths = Vec::new();
        super::rust_files(&dir, &mut paths);
        let mut seen = BTreeSet::new();
        let mut tests = BTreeSet::new();
        for f in paths {
            if f.components().any(|c| c.as_os_str() == "target") {
                continue;
            }
            let text = std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
            let code = super::strip_line_comments(&text);
            // A crate's own integration tests and benches are another crate to its `src/`.
            let in_src = f.starts_with(dir.join("src"));
            words(&code, if in_src { &mut seen } else { &mut tests });
            if library && f.starts_with(dir.join("src")) {
                let rel = f.strip_prefix(&root).unwrap_or(&f).to_string_lossy().replace('\\', "/");
                for (i, l) in super::engine_guards::product_code(&text).lines().enumerate() {
                    if let Some(n) = item_name(l) {
                        items.entry(name.clone()).or_default().push((rel.clone(), i + 1, n.to_string()));
                    }
                }
            }
        }
        all_words.insert(name.clone(), seen);
        all_words.insert(format!("{name}#tests"), tests);
    }
    // A name counts as used outside when another crate's text holds it.
    let mut out = BTreeMap::new();
    for (krate, list) in items {
        let used: BTreeSet<&String> = all_words.iter().filter(|(k, _)| **k != krate).flat_map(|(_, w)| w.iter()).collect();
        let unused: Vec<_> = list.into_iter().filter(|(_, _, n)| !used.contains(n)).collect();
        if !unused.is_empty() {
            out.insert(krate, unused);
        }
    }
    Ok(out)
}

/// The guard's problems, if any.
pub fn violations(meta: &Value) -> Result<Vec<String>, String> {
    let found = unreachable(meta)?;
    let what = "`pub` items no other crate uses (finding #68)";
    let fix = "make them `pub(crate)` (`cargo xtask pub-unreachable` lists them)";
    let mut out = Vec::new();
    let names: BTreeSet<&str> = found.keys().map(String::as_str).chain(ALLOWED.iter().map(|(n, _)| *n)).collect();
    for n in names {
        let have = found.get(n).map_or(0, Vec::len);
        let allowed = ALLOWED.iter().find(|(k, _)| *k == n).map_or(0, |(_, c)| *c);
        if let Some(p) = super::ratchet_problem(n, have, allowed, what, fix) {
            out.push(p);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_item_name_is_read_from_a_column_zero_pub_item() {
        assert_eq!(item_name("pub fn run(a: u8)"), Some("run"));
        assert_eq!(item_name("pub const fn st(n: u128)"), Some("st"));
        assert_eq!(item_name("pub struct Tag<'a> {"), Some("Tag"));
        assert_eq!(item_name("pub(crate) fn hidden()"), None);
        assert_eq!(item_name("    pub fn method()"), None);
        assert_eq!(item_name("pub mod commands;"), None);
    }

    #[test]
    fn words_are_whole_identifiers() {
        let mut w = BTreeSet::new();
        words("a::run_it(b2) + x", &mut w);
        assert!(w.contains("run_it") && w.contains("b2") && !w.contains("run"));
    }
}
