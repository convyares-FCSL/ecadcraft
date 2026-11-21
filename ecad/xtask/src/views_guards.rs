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
const ALLOWED: usize = 3;

/// The calls that work out views of a state from scratch, outside `Views` (`.detached(`,
/// `.report_models(`).
const FRESH_CALLS: &[&str] = &[".detached(", ".report_models("];

/// Every file allowed to work out views from scratch, how many times, and why: a state the
/// project does not keep, or a check that must not trust the kept views (design 064 §5.2). A
/// file not listed may not; a listed count is exact, so it only falls.
const FRESH_BUILDS: &[(&str, usize, &str)] = &[
    ("crates/ecad-cadcraft/src/debug.rs", 1, "debugger projection check, on a copy, must not trust the kept models"),
    ("crates/ecad-cadcraft/src/draw/checks.rs", 1, "the `checks` command's from-scratch pass, on request"),
    ("crates/ecad-cadcraft/src/preview.rs", 2, "previews: a page outside its window, and an issued revision's state"),
    ("crates/ecad-cadcraft/src/project/changes.rs", 1, "recovery check of a rebuilt page, must not trust the kept models"),
    ("crates/ecad-cadcraft/src/project/mod.rs", 1, "live facts: an open page against a fresh projection"),
    ("crates/ecad-cadcraft/src/release.rs", 3, "release: the issued and the finalised states, not kept"),
    ("dev/ecad-bench/src/bom.rs", 1, "bench verify mode: the kept BOM model against a rebuild"),
];

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

/// The lines of one file (outside `//` comments and its trailing `#[cfg(test)]` module) for which
/// `hit` holds, as `rel:line: text`.
fn lines_where(rel: &str, text: &str, hit: impl Fn(&str) -> bool) -> Vec<String> {
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
        if hit(code) {
            out.push(format!("{rel}:{}: {}", i + 1, line.trim()));
        }
    }
    out
}

/// The direct view builds in one file.
fn places(rel: &str, text: &str) -> Vec<String> {
    lines_where(rel, text, is_place)
}

/// The from-scratch view calls in one file.
fn fresh(rel: &str, text: &str) -> Vec<String> {
    lines_where(rel, text, |code| FRESH_CALLS.iter().any(|c| code.contains(c)))
}

/// The guard's problem, if any.
pub fn violations(meta: &Value) -> Result<Vec<String>, String> {
    let root = PathBuf::from(meta["workspace_root"].as_str().ok_or("cargo metadata: no workspace root")?);
    let mut found = Vec::new();
    let mut fresh_found: Vec<(String, Vec<String>)> = Vec::new();
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
            let f = fresh(&rel, &text);
            if !f.is_empty() || FRESH_BUILDS.iter().any(|(p, _, _)| *p == rel) {
                fresh_found.push((rel, f));
            }
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
    let what = "from-scratch view calls (`.detached(`, `.report_models(`)";
    for (rel, sites) in fresh_found {
        let allowed = FRESH_BUILDS.iter().find(|(p, _, _)| *p == rel).map_or(0, |(_, n, _)| *n);
        let fix = "read the kept view, or add the file to FRESH_BUILDS with its reason";
        if let Some(p) = super::ratchet_problem(&rel, sites.len(), allowed, what, fix) {
            out.push(p);
            out.extend(sites.into_iter().map(|f| format!("    {f}")));
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
    fn from_scratch_calls_are_found() {
        let rows: &[(&str, usize)] = &[
            ("let d = p.derived().detached(&then);", 1),
            ("let o = engine_views().report_models(doc);", 1),
            ("pub fn detached(&self, doc: &Document) -> Detached {", 0),
            ("// views.detached(doc) in a comment", 0),
        ];
        for (text, want) in rows {
            assert_eq!(fresh("f.rs", text).len(), *want, "{text}");
        }
    }

    #[test]
    fn every_fresh_build_has_a_reason() {
        for (path, n, why) in FRESH_BUILDS {
            assert!(*n > 0 && !why.trim().is_empty(), "{path}");
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
