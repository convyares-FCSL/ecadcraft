//! Design 064 §5.2: every derived view has one way in, the project's `Views`. This guard counts
//! direct full builds of a view type (`ReportModels::build(`, `RefIndex::build(`, …) outside `Views`
//! and the view's own module; the count only falls and reaches 0 at the end of lane R9 (test C13).
//!
//! Counted over `src/` of every product and Dev crate (the bench included), outside `//` comments
//! and outside a file's trailing `#[cfg(test)]` module. A view's own module may build the view and
//! its parts ([`OWN_MODULES`]: an update that rebuilds, the report models building their strips),
//! and so may `Views` itself (ecad-electrical's `derived/`); each such build outside `derived/` is
//! listed with its reason ([`OWN_BUILDS`]).
//!
//! In ecad-electrical, no match over `Effect`/`EffectKind` ends in a wildcard arm, so a new effect
//! kind needs a decision in every view's note; and view notes route property keys by prefix only
//! where [`KEY_PREFIXES`] says why (design 064 §9 step 10, overseer ruling 2026-11-21).

use std::path::PathBuf;

use serde_json::Value;

/// The view types whose `::build(` is counted (design 064 §5.2's inventory).
const VIEW_TYPES: &[&str] = &["ReportModels", "Checks", "RefIndex", "connectors::Index", "Uses", "Plc", "Strips", "StripPages"];

/// The modules where a view builds itself or its parts (design 064 §5.2: "inside the view: fine"),
/// and `Views`. A path ending in `/` is a folder.
const OWN_MODULES: &[&str] = &[
    "crates/ecad-electrical/src/views/",
    "crates/ecad-electrical/src/checks/view.rs",
    "crates/ecad-electrical/src/connectors/view.rs",
    "crates/ecad-electrical/src/devices/view.rs",
    "crates/ecad-electrical/src/pages/frame_view.rs",
    "crates/ecad-electrical/src/pages/generated_view.rs",
    "crates/ecad-electrical/src/pages/tree_view.rs",
    "crates/ecad-electrical/src/pages/view.rs",
    "crates/ecad-electrical/src/properties/view.rs",
    "crates/ecad-electrical/src/references/view.rs",
    "crates/ecad-electrical/src/reports/view.rs",
    "crates/ecad-electrical/src/reports/kept/",
    "crates/ecad-electrical/src/reports/mod.rs",
    "crates/ecad-electrical/src/terminal_strips/pages.rs",
    "crates/ecad-electrical/src/terminal_strips/mod.rs",
    "crates/ecad-electrical/src/plc/",
    "crates/ecad-electrical/src/checks/",
    "crates/ecad-electrical/src/checks/mod.rs",
    "crates/ecad-electrical/src/references/devices/index.rs",
    "crates/ecad-electrical/src/connectors/mod.rs",
    "crates/ecad-electrical/src/pages/front.rs",
];

/// Probe files whose direct builds are not counted, each with its reason: they time a full rebuild
/// on purpose (the reference point for deciding which views keep an update map). Listed by file;
/// a file not listed is counted.
const PROBES: &[(&str, &str)] = &[("dev/ecad-bench/src/views.rs", "bench probe timing each view's full rebuild on purpose (D1, 2026-11-22)")];

/// The count on `refactor/0.2.3` at 3e54b06a; lower it as the view steps land.
const ALLOWED: usize = 3;

/// The calls that work out views of a state from scratch, outside `Views` (`.detached(`,
/// `.report_models(`).
const FRESH_CALLS: &[&str] = &[".detached(", ".report_models("];

/// Every file allowed to work out views from scratch, how many times, and why: a state the
/// project does not keep, or a check that must not trust the kept views (design 064 §5.2). A
/// file not listed may not; a listed count is exact, so it only falls.
const FRESH_BUILDS: &[(&str, usize, &str)] = &[
    ("crates/ecad-engine/src/dev/debug.rs", 1, "debugger projection check, on a copy, must not trust the kept models"),
    ("crates/ecad-engine/src/checks.rs", 1, "the `checks` command's from-scratch pass, on request"),
    ("crates/ecad-engine/src/pages/preview.rs", 2, "previews: a page outside its window, and an issued revision's state"),
    ("crates/ecad-engine/src/project/changes.rs", 1, "recovery check of a rebuilt page, must not trust the kept models"),
    ("crates/ecad-engine/src/project/mod.rs", 1, "live facts: an open page against a fresh projection"),
    ("crates/ecad-engine/src/releases/release.rs", 3, "release: the issued and the finalised states, not kept"),
    ("dev/ecad-bench/src/bom.rs", 1, "bench verify mode: the kept BOM model against a rebuild"),
];

/// Every direct build inside a view's own module (not `Views`), how many, and why: each must be a
/// view rebuilding itself or its parts, or a verify pass against a rebuild (design 064 §9 step 10c,
/// checked by hand 2026-11-21, re-listed after decision 7's map deletions 2026-11-22). A count is
/// exact, so a new build needs a reason here.
const OWN_BUILDS: &[(&str, usize, &str)] = &[
    ("crates/ecad-electrical/src/pages/front.rs", 1, "Uses rebuilds itself on a project-wide change (wire style, potentials)"),
    ("crates/ecad-electrical/src/reports/kept/mod.rs", 2, "The report models build their strips and strip pages on first show"),
    ("crates/ecad-electrical/src/reports/kept/update.rs", 2, "The verify pass compares the strips with a rebuild and the strip pages follow"),
    (
        "crates/ecad-electrical/src/terminal_strips/pages.rs",
        1,
        "StripPages builds itself again after every commit (no update map, decision 7: 0.5 ms on large)",
    ),
    ("crates/ecad-electrical/src/references/devices/index.rs", 1, "RefIndex::verify compares the kept index with a rebuild"),
];

/// `Views` and every feature's view module: all of them count as view notes for [`KEY_PREFIXES`].
const DERIVED: &[&str] = &[
    "crates/ecad-electrical/src/views/",
    "crates/ecad-electrical/src/checks/view.rs",
    "crates/ecad-electrical/src/connectors/view.rs",
    "crates/ecad-electrical/src/devices/view.rs",
    "crates/ecad-electrical/src/pages/frame_view.rs",
    "crates/ecad-electrical/src/pages/generated_view.rs",
    "crates/ecad-electrical/src/pages/tree_view.rs",
    "crates/ecad-electrical/src/pages/view.rs",
    "crates/ecad-electrical/src/properties/view.rs",
    "crates/ecad-electrical/src/references/view.rs",
    "crates/ecad-electrical/src/reports/view.rs",
];

/// Whether `rel` is `Views` or a feature's view module.
fn is_view_module(rel: &str) -> bool {
    DERIVED.iter().any(|m| if m.ends_with('/') { rel.starts_with(m) } else { rel == *m })
}

/// The crate whose matches over effects and view notes are checked.
const EFFECT_CRATE: &str = "crates/ecad-electrical/src/";

/// Files whose view notes (all of `derived/`, and every fn taking an `Effect` elsewhere) route a
/// property key by its prefix (`.starts_with("`), how many, and why (overseer ruling 2026-11-21).
/// A key's meaning belongs in data (its definition), not its spelling; the count only falls.
const KEY_PREFIXES: &[(&str, usize, &str)] = &[
    ("crates/ecad-electrical/src/connectors/view.rs", 1, "connector naming keys: the connector defaults have no key table"),
    ("crates/ecad-electrical/src/pages/view.rs", 1, "project connector keys reach the connector pages; no key table"),
    ("crates/ecad-electrical/src/pages/front.rs", 1, "the cover's `front.cover.*` settings; no key table"),
    ("crates/ecad-electrical/src/reports/mod.rs", 3, "strip, terminal, connector and BOM settings pick the lists to redo; no key table"),
    ("crates/ecad-electrical/src/circuits/kept.rs", 2, "`link.*`, `rating.*` and `wire.*` keys; no key table"),
];

/// Every place a view widens a change to "everything" (an unclassified or project-wide change
/// rebuilds all it keeps), per file, how many, and why (lane D2, owner ruling 2026-11-22: no
/// "unknown means everything" path; each wide case is a named kind of change with its reason, and
/// the count only falls). The cable references' `Touch::All` arms each carry
/// their reason in the code.
const WIDE: &[(&str, usize, &str)] = &[
    (
        "crates/ecad-electrical/src/references/cables.rs",
        8,
        "Touch::All arms: frame or standard, a wire made again, other style catalogues, a standard, settings, the project record, an aid; each carries its reason",
    ),
    (
        "crates/ecad-electrical/src/references/devices/watch.rs",
        11,
        "project-wide table, style, sheet and frame settings reach every device or arrow page; the cable scope's all_cables",
    ),
    ("crates/ecad-electrical/src/pages/front.rs", 5, "the cover's uses: wire style, potentials and standards reach every wire"),
    ("crates/ecad-electrical/src/title_blocks/mod.rs", 3, "a frame or standard change moves every page's grid"),
    ("crates/ecad-electrical/src/devices/view.rs", 1, "a wide frame change marks the pages list"),
    ("crates/ecad-electrical/src/checks/mod.rs", 1, "wiring scheme, digits or potentials renumber every wire's check"),
];

/// Whether `line` widens a view's work to everything (see [`WIDE`]).
fn is_wide(line: &str) -> bool {
    ["Touch::All(\"", ".wide = true", "arrows_wide = true", "all_cables = true", "self.all = true", "Reach::Wide"].iter().any(|p| line.contains(p))
}

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

/// `text`'s code lines up to its trailing `#[cfg(test)]` module, with `//` comments, the insides
/// of string literals and brace character literals blanked, so braces can be counted.
fn code_lines(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        if line.trim_start().starts_with("#[cfg(test)]") {
            break;
        }
        let line = line.replace("'{'", "' '").replace("'}'", "' '");
        let mut code = String::new();
        let (mut in_str, mut esc) = (false, false);
        let mut chars = line.chars().peekable();
        while let Some(c) = chars.next() {
            if in_str {
                match (esc, c) {
                    (true, _) => esc = false,
                    (false, '\\') => esc = true,
                    (false, '"') => {
                        in_str = false;
                        code.push('"');
                    }
                    _ => {}
                }
                continue;
            }
            if c == '/' && chars.peek() == Some(&'/') {
                break;
            }
            if c == '"' {
                in_str = true;
            }
            code.push(c);
        }
        out.push(code);
    }
    out
}

/// Whether an arm's pattern names an effect variant (`Effect::Wire { .. }`, `EffectKind::Prop`).
fn is_effect_arm(arm: &str) -> bool {
    let head = arm.trim_start_matches('|').trim_start();
    let path = head.split(|c: char| c.is_whitespace() || c == '(' || c == '{').next().unwrap_or_default();
    path.contains("Effect::") || path.contains("EffectKind::")
}

/// The change in brace depth over one code line.
fn braces(code: &str) -> i64 {
    code.matches('{').count() as i64 - code.matches('}').count() as i64
}

/// In one file: the wildcard arms of matches over effects, and the `.starts_with("` calls inside
/// fns that take an `Effect` (anywhere, with `whole`), each as `rel:line: text`.
fn effect_routing(rel: &str, text: &str, whole: bool) -> (Vec<String>, Vec<String>) {
    let raw: Vec<&str> = text.lines().collect();
    let site = |i: usize| format!("{rel}:{}: {}", i + 1, raw.get(i).map_or("", |l| l.trim()));
    let (mut wild, mut prefix) = (Vec::new(), Vec::new());
    // Open matches: the depth of their arms, whether an arm names an effect, wildcard arm lines.
    let mut matches: Vec<(i64, bool, Vec<usize>)> = Vec::new();
    // The depth an effect-taking fn's body closes at, and a signature still being read.
    let (mut in_fn, mut sig): (Option<i64>, Option<String>) = (None, None);
    let mut depth = 0i64;
    for (i, code) in code_lines(text).iter().enumerate() {
        let t = code.trim();
        if let Some((arm_depth, over, wilds)) = matches.last_mut()
            && depth == *arm_depth
            && !t.is_empty()
            && !t.starts_with('}')
        {
            *over |= is_effect_arm(t);
            if t.starts_with("_ =>") || t.starts_with("_ if ") {
                wilds.push(i);
            }
        }
        if in_fn.is_none() {
            match sig.as_mut() {
                Some(s) => s.push_str(t),
                None if t.contains("fn ") => sig = Some(t.to_string()),
                None => {}
            }
            if let Some(s) = &sig {
                if s.contains('{') {
                    in_fn = s.contains("Effect<").then_some(depth);
                    sig = None;
                } else if s.ends_with(';') {
                    sig = None;
                }
            }
        }
        if (whole || in_fn.is_some()) && t.contains(".starts_with(\"") {
            prefix.push(site(i));
        }
        let opens_match = (t.starts_with("match ") || t.contains(" match ")) && t.ends_with('{');
        depth += braces(code);
        if opens_match {
            matches.push((depth, false, Vec::new()));
        }
        while matches.last().is_some_and(|(d, _, _)| depth < *d) {
            if let Some((_, true, wilds)) = matches.pop() {
                wild.extend(wilds.into_iter().map(site));
            }
        }
        if in_fn.is_some_and(|d| depth <= d) {
            in_fn = None;
        }
    }
    (wild, prefix)
}

/// The problems of one exact-count table: every file found or listed, against its count.
fn ratchet(table: &[(&str, usize, &str)], mut found: Vec<(String, Vec<String>)>, what: &str, fix: &str) -> Vec<String> {
    for (p, _, _) in table {
        if !found.iter().any(|(f, _)| f == p) {
            found.push(((*p).to_string(), Vec::new()));
        }
    }
    let mut out = Vec::new();
    for (rel, sites) in found {
        let allowed = table.iter().find(|(p, _, _)| *p == rel).map_or(0, |(_, n, _)| *n);
        if let Some(p) = super::ratchet_problem(&rel, sites.len(), allowed, what, fix) {
            out.push(p);
            out.extend(sites.into_iter().map(|f| format!("    {f}")));
        }
    }
    out
}

/// The guard's problem, if any.
pub fn violations(meta: &Value) -> Result<Vec<String>, String> {
    let root = PathBuf::from(meta["workspace_root"].as_str().ok_or("cargo metadata: no workspace root")?);
    let mut found = Vec::new();
    let mut fresh_found: Vec<(String, Vec<String>)> = Vec::new();
    let (mut own_found, mut wild_found, mut prefix_found) = (Vec::new(), Vec::new(), Vec::new());
    let mut wide_found: Vec<(String, Vec<String>)> = Vec::new();
    for (_, class, dir, _) in super::classed_packages(meta)? {
        let (false, Some(dir)) = (matches!(class, super::Class::Exempt), dir) else {
            continue;
        };
        let mut paths = Vec::new();
        super::rust_files(&dir.join("src"), &mut paths);
        for f in paths {
            let rel = f.strip_prefix(&root).unwrap_or(&f).to_string_lossy().replace('\\', "/");
            let text = std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
            if rel.starts_with(EFFECT_CRATE) {
                let (wild, prefix) = effect_routing(&rel, &text, is_view_module(&rel));
                if !wild.is_empty() {
                    wild_found.push((rel.clone(), wild));
                }
                if !prefix.is_empty() {
                    prefix_found.push((rel.clone(), prefix));
                }
                let wide = lines_where(&rel, &text, is_wide);
                if !wide.is_empty() {
                    wide_found.push((rel.clone(), wide));
                }
            }
            if is_own_module(&rel) {
                let own = places(&rel, &text);
                if !is_view_module(&rel) && !own.is_empty() {
                    own_found.push((rel, own));
                }
                continue;
            }
            if PROBES.iter().any(|(p, _)| *p == rel) {
                continue;
            }
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
    let fix = "rebuild only the view itself, or verify; else add it to OWN_BUILDS with its reason";
    out.extend(ratchet(OWN_BUILDS, own_found, "view builds inside a view's own module", fix));
    let fix = "name every effect the match passes over";
    out.extend(ratchet(&[], wild_found, "wildcard arms in matches over effects", fix));
    let fix = "say what the key is in its definition and route on that";
    out.extend(ratchet(KEY_PREFIXES, prefix_found, "property keys routed by prefix in a view's note", fix));
    let fix = "decide what the change reaches and name it; a project-wide case needs its reason in WIDE";
    out.extend(ratchet(WIDE, wide_found, "places a view widens a change to everything", fix));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_builds_are_found() {
        let rows: &[(&str, usize)] = &[
            ("let o = ReportModels::build(&doc);", 1),
            ("let i = crate::connectors::Index::build(doc);", 1),
            ("let i = ecad_model::index::Index::build(doc);", 0),
            ("let o = MyReportModels::build(&doc);", 0),
            ("// ReportModels::build(&doc) in a comment", 0),
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
        for (path, n, why) in FRESH_BUILDS.iter().chain(OWN_BUILDS).chain(KEY_PREFIXES).chain(WIDE) {
            assert!(*n > 0 && !why.trim().is_empty(), "{path}");
        }
    }

    #[test]
    fn effect_wildcards_and_key_prefixes_are_found() {
        // (source, whole file, wildcard arms over effects, prefix routes)
        let rows: &[(&str, bool, usize, usize)] = &[
            ("fn n(e: &Effect<'_>) -> bool {\n    match e {\n        Effect::Wire { .. } => true,\n        _ => false,\n    }\n}", false, 1, 0),
            (
                "fn n(e: &Effect<'_>) -> bool {\n    match e {\n        Effect::Wire { .. } => true,\n        Effect::Page { .. } => false,\n    }\n}",
                false,
                0,
                0,
            ),
            // A wildcard over something else, nested in an effect arm.
            (
                "fn n(e: &Effect<'_>) {\n    match e {\n        Effect::Item { before, .. } => match before {\n            Some(_) => {}\n            _ => {}\n        },\n        Effect::Page { .. } => {}\n    }\n}",
                false,
                0,
                0,
            ),
            ("fn n(e: &Effect<'_>) {\n    let x = match e {\n        EffectKind::Prop => 1,\n        _ if true => 2,\n    };\n}", false, 1, 0),
            ("fn n(e: &Effect<'_>, key: &str) -> bool {\n    key.starts_with(\"plc.\")\n}", false, 0, 1),
            ("fn other(key: &str) -> bool {\n    key.starts_with(\"plc.\")\n}", false, 0, 0),
            ("fn other(key: &str) -> bool {\n    key.starts_with(\"plc.\")\n}", true, 0, 1),
            ("fn other(key: &str) -> bool {\n    // key.starts_with(\"plc.\")\n    let s = \"{ _ => }\";\n    true\n}", true, 0, 0),
        ];
        for (text, whole, wild, prefix) in rows {
            let (w, p) = effect_routing("f.rs", text, *whole);
            assert_eq!((w.len(), p.len()), (*wild, *prefix), "{text}");
        }
    }

    #[test]
    fn wide_fallbacks_are_found() {
        let rows: &[(&str, bool)] = &[
            ("Effect::Aid { .. } => return Touch::All(\"an aid\"),", true),
            ("self.wide = true;", true),
            ("self.all_cables = true;", true),
            ("matches!(touched, Touch::All(_))", false),
            ("let wide = false;", false),
        ];
        for (text, want) in rows {
            assert_eq!(is_wide(text), *want, "{text}");
        }
    }

    #[test]
    fn a_views_own_module_may_build_it() {
        let rows: &[(&str, bool)] = &[
            ("crates/ecad-electrical/src/reports/kept/mod.rs", true),
            ("crates/ecad-electrical/src/references/devices/index.rs", true),
            ("crates/ecad-electrical/src/views/mod.rs", true),
            ("crates/ecad-electrical/src/pages/generated.rs", false),
            ("crates/ecad-electrical/src/xref_extra.rs", false),
            ("crates/ecad-engine/src/releases/release.rs", false),
        ];
        for (rel, want) in rows {
            assert_eq!(is_own_module(rel), *want, "{rel}");
        }
    }
}
