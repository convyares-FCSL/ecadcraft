//! The `unreachable_pub` ratchet (end-gate finding #68): in a folder listed in [`NARROWED`], a
//! `pub` item that no other crate names is `pub(crate)`. rustc's own lint cannot say this, because
//! these folders are reachable modules; so the check is textual and conservative: an item counts as
//! used outside when its name appears as a word in any other crate's `src/` or `tests/`, or in its own
//! crate's `tests/` (integration tests are another crate).
//!
//! A feature slice adds its folders to [`NARROWED`] when it has narrowed them. Types that another
//! crate uses without naming (inferred, or in a public signature) are listed in [`EXEMPT`] with the
//! reason.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// `(crate folder, folder inside it)` pairs already narrowed.
const NARROWED: &[(&str, &str)] = &[
    ("crates/ecad-electrical", "src/references"),
    ("crates/ecad-electrical", "src/cables"),
    ("crates/ecad-engine", "src/references"),
    ("crates/ecad-engine", "src/cables"),
    ("crates/ecad-app", "src/cables"),
];

/// Items that stay `pub` though no other crate names them, with why.
const EXEMPT: &[(&str, &str)] = &[
    ("SlashAssign", "inferred by ecad-engine from a public command function"),
    ("CoreSet", "inferred by ecad-engine from a public command function"),
    ("OtherEnd", "inferred by ecad-engine from a public command function"),
    ("Stub", "inferred by ecad-engine from a public function"),
    ("OffPlan", "inferred by ecad-app from a public function"),
    ("DrawnEnd", "inferred by ecad-app from a public function"),
    ("CableInput", "in the signature of a public function"),
    ("EndInput", "a field type of CableInput"),
    ("Referenced", "in the signature of a public function"),
    ("StubRoute", "a field type of a public struct"),
    ("ColouredRoute", "in the signature of a public function"),
    ("CablesView", "inferred by ecad-app tests and a field of Reports"),
    ("CoresSource", "inferred by ecad-app tests"),
    ("CablesSource", "a field type of CablesView"),
    ("RowInfo", "a field type of CablesSource"),
    ("OfferedPole", "in the signature of public functions the engine calls"),
    ("DrawnPole", "a field type of a public struct"),
    ("TableShown", "a field type of a public struct"),
    ("ReferencesScope", "re-exported by views with the other scopes"),
];

/// The words (identifiers) of one source text.
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

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name();
        if p.is_dir() {
            if name != "target" {
                rs_files(&p, out);
            }
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// The name a line declares when it is a `pub` item (not `pub(…)`), if it is one.
pub fn pub_item(line: &str) -> Option<&str> {
    let mut rest = line.trim_start().strip_prefix("pub ")?;
    while let Some(r) = rest.strip_prefix("const ").filter(|r| r.starts_with("fn ")).or_else(|| rest.strip_prefix("async ")) {
        rest = r;
    }
    let rest = ["fn ", "struct ", "enum ", "const ", "static ", "type ", "trait "].iter().find_map(|k| rest.strip_prefix(k))?;
    let end = rest.find(|c: char| !(c.is_alphanumeric() || c == '_')).unwrap_or(rest.len());
    rest.get(..end).filter(|n| !n.is_empty())
}

/// The listed folders' `pub` items that no other crate names and that are not exempt.
pub fn violations(_meta: &Value) -> Result<Vec<String>, String> {
    let root = super::ecad_root();
    let mut crates: Vec<PathBuf> = Vec::new();
    for base in ["crates", "apps", "dev"] {
        let Ok(rd) = std::fs::read_dir(root.join(base)) else { continue };
        crates.extend(rd.flatten().map(|e| e.path()).filter(|p| p.join("Cargo.toml").is_file()));
    }
    // (crate, words in src, words in tests)
    let mut sets: Vec<(PathBuf, BTreeSet<String>, BTreeSet<String>)> = Vec::new();
    for c in &crates {
        let (mut src, mut tests) = (BTreeSet::new(), BTreeSet::new());
        let mut files = Vec::new();
        rs_files(c, &mut files);
        for f in files {
            let text = std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
            let in_src = f.strip_prefix(c).is_ok_and(|r| r.starts_with("src"));
            words(&text, if in_src { &mut src } else { &mut tests });
        }
        sets.push((c.clone(), src, tests));
    }
    let mut out = Vec::new();
    for (krate, folder) in NARROWED {
        let own = root.join(krate);
        let mut files = Vec::new();
        rs_files(&own.join(folder), &mut files);
        for f in files {
            let text = std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
            for (i, line) in text.lines().enumerate() {
                let Some(name) = pub_item(line) else { continue };
                if EXEMPT.iter().any(|(n, _)| *n == name) {
                    continue;
                }
                let used =
                    sets.iter().any(|(c, src, tests)| if *c == own { tests.contains(name) } else { src.contains(name) || tests.contains(name) });
                if !used {
                    let rel = f.strip_prefix(&root).unwrap_or(&f).display().to_string().replace('\\', "/");
                    out.push(format!("{rel}:{}: `pub {name}` is named by no other crate; make it pub(crate) (end-gate #68)", i + 1));
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pub_item_line_gives_its_name_and_a_narrowed_one_gives_none() {
        let rows: &[(&str, Option<&str>)] = &[
            ("pub fn a(x: u8) {", Some("a")),
            ("    pub struct B {", Some("B")),
            ("pub const C: u8 = 1;", Some("C")),
            ("pub const fn d() {}", Some("d")),
            ("pub(crate) fn e() {}", None),
            ("pub mod f;", None),
            ("    pub x: u8,", None),
        ];
        for (line, want) in rows {
            assert_eq!(pub_item(line), *want, "{line}");
        }
    }
}
