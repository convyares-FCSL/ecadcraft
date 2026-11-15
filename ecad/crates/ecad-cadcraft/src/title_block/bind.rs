//! What a field shows (design 055 §7).
//!
//! | Source | Keys | Value |
//! | --- | --- | --- |
//! | `project` | `title`, `customer`, `drawing_number`, `status`, `company`, any key | The project property |
//! | `page` | `title`, `number`, `next`, `total`, `function`, `location` | As 019's frame |
//! | `revision` | `rev` (with the pending marker), `date`, `description`, `drawn`, `checked`, `approved`, any revision column field | The newest issued revision |
//! | `print` | `sheet` | The page's sheet |
//!
//! A key not in the list for its source, or an unknown source, is not filled ([`known`] is
//! false): it draws as empty and Checks counts it (`title-binding-unknown`).

use std::collections::{BTreeMap, BTreeSet};

use ecad_model::Document;
use ecad_model::title_block::{Binding, IfEmpty, Source, TitleBlockLayout};

use crate::frames::FrameInput;

/// The project keys the palette names (design 055 §4.3).
pub const PROJECT_KEYS: [&str; 5] = ["title", "customer", "drawing_number", "status", "company"];
/// The page keys.
pub const PAGE_KEYS: [&str; 6] = ["title", "number", "next", "total", "function", "location"];
/// The print keys.
pub const PRINT_KEYS: [&str; 1] = ["sheet"];
/// Longest free project key, in characters (design 055 §4.3: `a-z 0-9 _`, 1–32).
pub const MAX_KEY_CHARS: usize = 32;
/// The missing mark (033's `?`).
pub const MISSING: &str = "?";

/// Whether `key` is a project key a field may name: 1–32 characters of `a-z 0-9 _ .`.
pub fn is_project_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= MAX_KEY_CHARS && key.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'.')
}

/// Whether this build can fill `b`.
pub fn known(b: &Binding) -> bool {
    match &b.source {
        Source::Project => is_project_key(&b.key),
        Source::Page => PAGE_KEYS.contains(&b.key.as_str()),
        Source::Revision => crate::revisions::FIELDS.contains(&b.key.as_str()) || crate::revisions::is_custom_field(&b.key),
        Source::Print => PRINT_KEYS.contains(&b.key.as_str()),
        Source::Unknown(_) => false,
    }
}

/// The project keys and the revision fields `layout`'s fields read (known bindings only).
pub fn keys(layout: &TitleBlockLayout) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut project = BTreeSet::new();
    let mut revision = BTreeSet::new();
    for b in layout.bindings().filter(|b| known(b)) {
        match b.source {
            Source::Project => {
                project.insert(b.key.clone());
            }
            Source::Revision if b.key != "rev" => {
                revision.insert(b.key.clone());
            }
            _ => {}
        }
    }
    (project, revision)
}

/// The values of the project keys and newest-revision fields `layout` reads, for the frame's
/// input: every key read is present (empty when unset), so a change to it is seen.
pub fn read(doc: &Document, layout: &TitleBlockLayout) -> (BTreeMap<String, String>, BTreeMap<String, String>) {
    let (pk, rk) = keys(layout);
    // Brief 052: a function or location is an entry key of the project's list, shown by its code.
    let project = pk
        .into_iter()
        .map(|k| {
            let v = doc.properties.get(&k).map(|v| crate::designations::display(doc, &k, v)).unwrap_or_default();
            (k, v)
        })
        .collect();
    let newest = if rk.is_empty() { None } else { crate::revisions::issued(doc).into_iter().next() };
    let revision = rk.into_iter().map(|k| (k.clone(), newest.as_ref().map(|r| r.get(&k).to_string()).unwrap_or_default())).collect();
    (project, revision)
}

/// The value `b` shows on the page `input` describes; `None` when this build cannot fill it.
pub fn value(input: &FrameInput, b: &Binding) -> Option<String> {
    if !known(b) {
        return None;
    }
    let k = b.key.as_str();
    Some(match &b.source {
        Source::Project => input.project.get(k).cloned().unwrap_or_default(),
        Source::Page => match k {
            "title" => input.page_title.clone(),
            "number" => input.page.clone(),
            "next" => input.next.clone(),
            "total" => input.total.clone(),
            "function" => input.function.clone(),
            "location" => input.location.clone(),
            _ => return None,
        },
        Source::Revision if k == "rev" => input.rev.clone(),
        Source::Revision => input.newest.get(k).cloned().unwrap_or_default(),
        Source::Print => super::sheet_name(&input.sheet),
        Source::Unknown(_) => return None,
    })
}

/// What a field shows: its value, or the "if empty" choice when the value is empty.
pub fn shown(value: Option<String>, if_empty: &IfEmpty) -> String {
    match value.filter(|v| !v.trim().is_empty()) {
        Some(v) => v,
        None => match if_empty {
            IfEmpty::Blank => String::new(),
            IfEmpty::Dash => "-".to_string(),
            IfEmpty::Missing => MISSING.to_string(),
            IfEmpty::Text(t) => t.clone(),
        },
    }
}

/// The palette's name of a binding (`Drawing number`), or its text for a free key.
pub fn name(b: &Binding) -> String {
    let n = match (&b.source, b.key.as_str()) {
        (Source::Project, "title") => "Project name",
        (Source::Project, "drawing_number") => "Drawing number",
        (Source::Project, "customer") => "Customer",
        (Source::Project, "status") => "Status",
        (Source::Project, "company") => "Company",
        (Source::Page, "title") => "Page title",
        (Source::Page, "number") => "Page number",
        (Source::Page, "next") => "Next page",
        (Source::Page, "total") => "Total pages",
        (Source::Page, "function") => "Function",
        (Source::Page, "location") => "Location",
        (Source::Revision, "rev") => "Revision",
        (Source::Revision, "date") => "Revision date",
        (Source::Revision, "description") => "Revision description",
        (Source::Revision, "drawn") => "Drawn",
        (Source::Revision, "checked") => "Checked",
        (Source::Revision, "approved") => "Approved",
        (Source::Revision, "ecr") => "ECR",
        (Source::Print, "sheet") => "Sheet size",
        _ => return b.text(),
    };
    n.to_string()
}

/// The source chip the palette and the name tag show: `project`, `page`, `revision`, `people`
/// (drawn, checked, approved), `print`.
pub fn chip(b: &Binding) -> &str {
    match (&b.source, b.key.as_str()) {
        (Source::Revision, "drawn" | "checked" | "approved") => "people",
        (s, _) => s.word(),
    }
}
