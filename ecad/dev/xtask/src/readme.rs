//! Every workspace member has a README with the standard sections, whose `## Contents` names files and
//! folders that exist.

use super::*;

/// The sections of a crate's README, in order, after its `# <crate name>` title.
pub(crate) const README_SECTIONS: &[&str] = &["## Purpose", "## Contents", "## Must not depend on", "## Key types", "## How to test"];

/// Workspace members whose README is missing or lacks the standard sections. The list only
/// shrinks: a member not
/// listed whose README fails is a violation, and an entry whose README passes (or that is no
/// longer a member) fails `layers` until it is removed.
pub(crate) const README_ALLOWLIST: &[&str] = &[];

/// What is wrong with the README of crate `name` (in layer `class`), `text` being its content if
/// it exists: the first heading is `# <name>`, the [`README_SECTIONS`] follow in
/// order (other headings may come between), and a `Layer:` line names the crate's layer. Headings
/// inside fenced code blocks are not headings.
pub(crate) fn readme_problem(name: &str, class: Class, text: Option<&str>) -> Option<String> {
    let Some(text) = text else { return Some(format!("{name}: no README.md at the crate root")) };
    let mut headings = Vec::new();
    let mut fenced = false;
    for l in text.lines().map(str::trim_end) {
        if l.trim_start().starts_with("```") {
            fenced = !fenced;
        } else if !fenced && (l.starts_with("# ") || l.starts_with("## ")) {
            headings.push(l);
        }
    }
    let title = format!("# {name}");
    if headings.first() != Some(&title.as_str()) {
        return Some(format!("{name}: its README must start with the heading `{title}`"));
    }
    let mut rest = headings.iter().skip(1);
    if let Some(missing) = README_SECTIONS.iter().find(|s| !rest.any(|h| h == *s)) {
        return Some(format!("{name}: its README lacks the heading `{missing}`, or has it out of order"));
    }
    let want = layer_name(class);
    match text.lines().find_map(|l| l.strip_prefix("Layer:")).map(str::trim) {
        Some(layer) if layer == want => None,
        Some(layer) => Some(format!("{name}: its README says `Layer: {layer}`, but the crate is in {want}")),
        None => Some(format!("{name}: its README has no `Layer: {want}` line")),
    }
}

/// The README ratchet: `results` holds each workspace member's [`readme_problem`]; a problem
/// fails unless the member is in `allowlist`, and an allowlist entry whose member passes or is
/// not in `results` fails too, so the list only shrinks.
pub(crate) fn readme_ratchet(results: &[(&str, Option<String>)], allowlist: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = results.iter().filter(|(n, _)| !allowlist.contains(n)).filter_map(|(_, p)| p.clone()).collect();
    for entry in allowlist {
        match results.iter().find(|(n, _)| n == entry) {
            Some((_, Some(_))) => {}
            Some((_, None)) => out.push(format!("{entry}: its README passes; remove it from the README allowlist")),
            None => out.push(format!("{entry}: in the README allowlist but not a workspace member; remove the entry")),
        }
    }
    out
}

/// The files and folders a README's `## Contents` section names: each backticked `name/` or `name.rs`, a path
/// inside the crate (one that starts with `ecad/` names a place elsewhere and is skipped).
pub(crate) fn readme_contents_paths(text: &str) -> Vec<&str> {
    let Some(start) = text.find(
        "
## Contents
",
    ) else {
        return Vec::new();
    };
    let body = &text[start
        + "
## Contents
"
        .len()..];
    let body = body
        .find(
            "
## ",
        )
        .map_or(body, |end| &body[..end]);
    body.split('`')
        .skip(1)
        .step_by(2)
        .filter(|s| {
            (s.ends_with('/') || s.ends_with(".rs"))
                && !s.starts_with("ecad/")
                && s.len() > 1
                && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '/' | '-'))
        })
        .collect()
}

/// The README checks over the workspace members in `meta`: the shape, with [`README_ALLOWLIST`], and, a hard
/// rule, that every file or folder its Contents names exists under the crate's `src/` or the crate folder.
pub(crate) fn readme_violations(meta: &Value) -> Result<Vec<String>, String> {
    let mut missing = Vec::new();
    let results: Vec<(&str, Option<String>)> = classed_packages(meta)?
        .into_iter()
        .map(|(name, class, dir, _)| {
            let text = dir.as_ref().and_then(|d| std::fs::read_to_string(d.join("README.md")).ok());
            if let (Some(d), Some(t)) = (&dir, &text) {
                for p in readme_contents_paths(t) {
                    let rel = p.trim_end_matches('/');
                    if !d.join("src").join(rel).exists() && !d.join(rel).exists() {
                        missing.push(format!("{name}: its README's Contents names `{p}`, which does not exist"));
                    }
                }
            }
            (name, readme_problem(name, class, text.as_deref()))
        })
        .collect();
    let mut out = readme_ratchet(&results, README_ALLOWLIST);
    out.extend(missing);
    Ok(out)
}
