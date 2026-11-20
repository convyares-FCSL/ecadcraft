//! Design 064 §5.2: every derived view has one way in, the project's `Views`. This guard counts
//! direct full builds of a view type (`Outputs::build(`, `RefIndex::build(`, …) outside `Views`
//! and the view's own module; the count only falls and reaches 0 at the end of lane R9 (test C13).
//!
//! Counted over `src/` of every product and Dev crate (the bench included), outside `//` comments
//! and outside a file's trailing `#[cfg(test)]` module. A view's own module may build the view and
//! its parts ([`OWN_MODULES`]: an update that rebuilds, the report models building their strips),
//! and so may `Views` itself (ecad-electrical's `derived/`).

use std::path::PathBuf;

use serde_json::Value;

/// The view types whose `::build(` is counted (design 064 §5.2's inventory).
const VIEW_TYPES: &[&str] = &["Outputs", "Checks", "RefIndex", "connectors::Index", "Uses", "Plc", "Strips", "StripPages"];

/// The modules where a view builds itself or its parts (design 064 §5.2: "inside the view: fine"),
/// and `Views`. A path ending in `/` is a folder.
const OWN_MODULES: &[&str] = &[
    "crates/ecad-electrical/src/derived/",
    "crates/ecad-electrical/src/outputs/",
    "crates/ecad-electrical/src/outputs.rs",
    "crates/ecad-electrical/src/strips/",
    "crates/ecad-electrical/src/strips.rs",
    "crates/ecad-electrical/src/plc/",
    "crates/ecad-electrical/src/checks/",
    "crates/ecad-electrical/src/checks.rs",
    "crates/ecad-electrical/src/xref.rs",
    "crates/ecad-electrical/src/connectors.rs",
    "crates/ecad-electrical/src/front.rs",
];

/// The count on `refactor/0.2.3` at 3e54b06a; lower it as the view steps land.
const ALLOWED: usize = 19;

/// Whether `rel` is a view's own module, or `Views`.
fn is_own_module(rel: &str) -> bool {
    OWN_MODULES.iter().any(|m| if m.ends_with('/') { rel.starts_with(m) } else { rel == *m })
}

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
        let (false, Some(dir)) = (matches!(class, super::Class::Exempt), dir) else {
            continue;
        };
        let mut paths = Vec::new();
        super::rust_files(&dir.join("src"), &mut paths);
        for f in paths {
            let rel = f.strip_prefix(&root).unwrap_or(&f).to_string_lossy().replace('\\', "/");
            if is_own_module(&rel) {
                continue;
            }
            let text = std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
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

    #[test]
    fn a_views_own_module_may_build_it() {
        let rows: &[(&str, bool)] = &[
            ("crates/ecad-electrical/src/outputs/kept.rs", true),
            ("crates/ecad-electrical/src/xref.rs", true),
            ("crates/ecad-electrical/src/derived/mod.rs", true),
            ("crates/ecad-electrical/src/generated.rs", false),
            ("crates/ecad-electrical/src/xref_extra.rs", false),
            ("crates/ecad-cadcraft/src/release.rs", false),
        ];
        for (rel, want) in rows {
            assert_eq!(is_own_module(rel), *want, "{rel}");
        }
    }
}
