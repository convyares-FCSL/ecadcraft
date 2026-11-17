//! Design 064 §5.2: every derived view has one way in, the project's `Views`. This guard counts
//! direct full builds of a view type (`Outputs::build(`, `RefIndex::build(`, …) outside the Core
//! crates that hold the views; the count only falls and reaches 0 at the end of lane R9 (test C13).
//!
//! Counted over `src/` of the Engine, UI kit, App, Programs and Dev crates (the bench included),
//! outside `//` comments and outside a file's trailing `#[cfg(test)]` module. The Core crates hold
//! the views themselves, where a view builds its own parts.

use std::path::PathBuf;

use serde_json::Value;

/// The view types whose `::build(` is counted (design 064 §5.2's inventory).
const VIEW_TYPES: &[&str] = &["Outputs", "Checks", "RefIndex", "connectors::Index", "Uses", "Plc", "Strips", "StripPages"];

/// The count on `refactor/0.2.3` at 3e54b06a; lower it as the view steps land.
const ALLOWED: usize = 26;

/// Whether `line` builds a view type directly.
fn is_place(line: &str) -> bool {
    VIEW_TYPES.iter().any(|t| {
        let pat = format!("{t}::build(");
        line.match_indices(&pat).any(|(i, _)| {
            // A whole type name: not the tail of a longer one (`MyOutputs::build(`).
            line[..i].chars().next_back().is_none_or(|c| !(c.is_alphanumeric() || c == '_'))
        })
    })
}

/// The places in one file, as `rel:line: text`.
fn places(rel: &str, text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let code = line.trim_start();
        if code.starts_with("#[cfg(test)]") {
            break;
        }
        if code.starts_with("//") {
            continue;
        }
        let code = code.split("//").next().unwrap_or_default();
        if is_place(code) {
            out.push(format!("{rel}:{}: {}", i + 1, line.trim()));
        }
    }
    out
}

/// The guard's problem, if any.
pub fn violations(meta: &Value) -> Result<Vec<String>, String> {
    let root = PathBuf::from(meta["workspace_root"].as_str().ok_or("cargo metadata: no workspace root")?);
    let mut found = Vec::new();
    for (_, class, dir, _) in super::classed_packages(meta)? {
        let (true, Some(dir)) =
            (matches!(class, super::Class::Engine | super::Class::UiKit | super::Class::App | super::Class::Programs | super::Class::Dev), dir)
        else {
            continue;
        };
        let mut paths = Vec::new();
        super::rust_files(&dir.join("src"), &mut paths);
        for f in paths {
            let text = std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
            let rel = f.strip_prefix(&root).unwrap_or(&f).to_string_lossy().replace('\\', "/");
            found.extend(places(&rel, &text));
        }
    }
    let mut out = Vec::new();
    let what = "direct builds of a derived view outside Views (design 064 §5.2)";
    if let Some(p) = super::ratchet_problem("views", found.len(), ALLOWED, what, "read the view through the project's Views") {
        out.push(p);
        if found.len() > ALLOWED {
            out.extend(found.into_iter().map(|f| format!("    {f}")));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_builds_are_found() {
        let rows: &[(&str, usize)] = &[
            ("let o = Outputs::build(&doc);", 1),
            ("let i = crate::connectors::Index::build(doc);", 1),
            ("let i = ecad_model::index::Index::build(doc);", 0),
            ("let o = MyOutputs::build(&doc);", 0),
            ("// Outputs::build(&doc) in a comment", 0),
            ("let s = Strips::build(doc, &o, &cx); let p = StripPages::build(doc, &s);", 1),
        ];
        for (text, want) in rows {
            assert_eq!(places("f.rs", text).len(), *want, "{text}");
        }
    }
}
