//! Review finding #27 (0.2.3): what **kind of device** something is comes from `Device.device_type`
//! (typed data), never from the starter symbol that draws it. This guard counts the places that
//! look a starter symbol up from stored data: `of_definition(` (a definition's starter) and
//! `rep_starter(` (the starter a representation draws).
//!
//! A starter lookup is right when the question is about the *drawing*: which symbol this
//! representation is (a PLC cell, a PE terminal, a connector pin), what its labels or texts are.
//! The count is kept per file: a file on [`ALLOWED`] may have exactly that many (a ratchet: lower
//! the count when one goes, remove the entry at 0), every other file none. Each entry says why its
//! lookups are drawing questions and not device kinds. A new lookup in another file fails: ask the
//! device's `device_type` instead, or add the file here with the reason.
//!
//! Textual, like the other guards: over the product crates' `src/`, outside `//` comments and
//! outside a file's trailing `#[cfg(test)]` module; a function's own declaration is not a lookup.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::Value;

/// The names of the lookups.
const NAMES: &[&str] = &["of_definition", "rep_starter"];

/// The files that still look a starter symbol up, how many times, and why each is a drawing
/// question. The count only falls.
#[rustfmt::skip]
const ALLOWED: &[(&str, usize, &str)] = &[
    ("crates/ecad-app/src/inspector/mod.rs", 1, "the placed symbol's prefix entry in the defaults scope (the symbol's, not the device's)"),
    ("crates/ecad-electrical/src/checks/mod.rs", 1, "a symbol marks some of its own connection points quiet (a drawing property)"),
    ("crates/ecad-electrical/src/connectors/mod.rs", 1, "the side a connector half or pin representation draws (gap: a stored side on the representation)"),
    ("crates/ecad-electrical/src/devices/hover.rs", 1, "the symbol's short name in a hover line (drawing question)"),
    ("crates/ecad-electrical/src/devices/json.rs", 1, "the symbol key a representation draws in the device JSON (drawing question)"),
    ("crates/ecad-electrical/src/devices/merge.rs", 3, "which symbol a representation is: erase and merge notes (drawing questions)"),
    ("crates/ecad-electrical/src/devices/tags.rs", 2, "the head noun and symbol names in tag words (drawing questions)"),
    ("crates/ecad-electrical/src/devices/texts.rs", 1, "the run rule of the symbol a representation draws (drawing question)"),
    ("crates/ecad-electrical/src/plc/channels.rs", 1, "whether a definition is a channel symbol (a symbol of a card, not a device)"),
    ("crates/ecad-electrical/src/plc/commands.rs", 3, "which representations of a card are its body or cells (drawn pieces)"),
    ("crates/ecad-electrical/src/plc/draw.rs", 3, "the cell or body piece a representation draws"),
    ("crates/ecad-electrical/src/plc/mod.rs", 5, "which representation is a channel cell or card caption (drawn pieces)"),
    ("crates/ecad-electrical/src/references/devices.rs", 2, "the poles a drawn contact symbol shows (gap: a `poles` table on the definition)"),
    ("crates/ecad-electrical/src/styles/text_style.rs", 1, "like symbols run together in a text style (a symbol key)"),
    ("crates/ecad-electrical/src/symbols/catalogue.rs", 1, "the symbols catalogue itself: the starter a definition was made from"),
    ("crates/ecad-electrical/src/symbols/project.rs", 3, "the symbols catalogue itself: registry key, mating half, bridge keys"),
    ("crates/ecad-electrical/src/symbols/rules.rs", 1, "fuse bank ways (gap: `FuseBank.ways` is not stored)"),
    ("crates/ecad-electrical/src/terminal_strips/commands/terminal_strips.rs", 1, "an embedded definition made from a given starter"),
    ("crates/ecad-electrical/src/terminal_strips/mod.rs", 1, "whether a representation draws a PE terminal block (gap: a terminal-block type on the representation)"),
    ("crates/ecad-electrical/src/terminal_strips/terminal_blocks.rs", 3, "the terminal block symbol (type and levels) a representation draws"),
    ("crates/ecad-engine/src/plc/commands/page.rs", 1, "whether the definition about to be placed is a rack card (the device does not exist yet)"),
];

/// Whether `line` uses one of the lookups as a whole word (called or passed as a function): not
/// its declaration or an import, and not a longer name such as `kind_of_definition`.
fn looks_up(line: &str) -> bool {
    let t = line.trim_start();
    if t.contains("fn ") || t.starts_with("use ") || t.starts_with("pub use ") {
        return false;
    }
    NAMES.iter().any(|n| {
        line.match_indices(n).any(|(i, _)| {
            let before = line.get(..i).and_then(|b| b.chars().last());
            let after = line.get(i + n.len()..).and_then(|a| a.chars().next());
            before.is_none_or(|c| !(c.is_alphanumeric() || c == '_')) && after.is_none_or(|c| !(c.is_alphanumeric() || c == '_'))
        })
    })
}

/// The lines of `code` that look a starter up.
fn places(code: &str) -> Vec<usize> {
    code.lines().enumerate().filter(|(_, l)| looks_up(l)).map(|(i, _)| i + 1).collect()
}

/// The guard's problems, if any.
pub fn violations(meta: &Value) -> Result<Vec<String>, String> {
    let root = PathBuf::from(meta["workspace_root"].as_str().ok_or("cargo metadata: no workspace root")?);
    let mut by_file: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (_, class, dir, _) in super::classed_packages(meta)? {
        let (true, Some(dir)) =
            (matches!(class, super::Class::Core | super::Class::Engine | super::Class::UiKit | super::Class::App | super::Class::Programs), dir)
        else {
            continue;
        };
        let mut paths = Vec::new();
        super::rust_files(&dir.join("src"), &mut paths);
        for f in paths {
            let text = std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
            let found = places(&super::engine_guards::product_code(&text));
            if !found.is_empty() {
                by_file.insert(f.strip_prefix(&root).unwrap_or(&f).to_string_lossy().replace('\\', "/"), found);
            }
        }
    }
    let what = "starter-symbol lookups (`of_definition(`, `rep_starter(`: finding #27)";
    let fix = "read the device's `device_type`; a drawing question goes on starter_guards::ALLOWED with its reason";
    let mut out = Vec::new();
    for (rel, found) in &by_file {
        let allowed = ALLOWED.iter().find(|(p, _, _)| p == rel).map_or(0, |(_, n, _)| *n);
        if let Some(p) = super::ratchet_problem(rel, found.len(), allowed, what, fix) {
            out.push(p);
            if found.len() > allowed {
                out.extend(found.iter().map(|l| format!("    {rel}:{l}")));
            }
        }
    }
    for (rel, allowed, _) in ALLOWED {
        if !by_file.contains_key(*rel)
            && let Some(p) = super::ratchet_problem(rel, 0, *allowed, what, fix)
        {
            out.push(p);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lookup_is_found_but_not_a_declaration() {
        assert_eq!(places("let s = rep_starter(doc, r);\npub fn rep_starter(doc: &D) {}\nlet k = of_definition(d);"), vec![1, 3]);
        assert!(
            places(
                "let x = starter_of(d);
let k = kind_of_definition(d);
use a::of_definition;"
            )
            .is_empty()
        );
        assert_eq!(places(".and_then(ecad_electrical::symbols::catalogue::of_definition)"), vec![1]);
    }
}
