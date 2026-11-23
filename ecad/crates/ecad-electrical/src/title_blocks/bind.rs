//! What a field shows.
//!
//! | Source | Keys | Value |
//! | --- | --- | --- |
//! | `project` | `title`, `customer`, `drawing_number`, `status`, `company`, any key | The project property |
//! | `page` | `title`, `number`, `next`, `total`, `function`, `location` | As in the frame |
//! | `revision` | `rev` (with the pending marker), `date`, `description`, `drawn`, `checked`, `approved`, any revision column field | The newest issued revision |
//! | `print` | `sheet` | The page's sheet |
//!
//! Each source carries its own typed key, so every binding that reads can be
//! filled: an unknown source or key is a read error, not a field drawn empty.

use std::collections::{BTreeMap, BTreeSet};

use ecad_model::Document;
use ecad_model::document::project::RevField;
use ecad_model::objects::title_block::{Binding, IfEmpty, PageField, PrintField, RevisionColumn, TitleBlockLayout};

use crate::pages::frames::FrameInput;

/// The project keys the palette names.
pub const PROJECT_KEYS: [&str; 5] = ["title", "customer", "drawing_number", "status", "company"];
/// Longest free project key, in characters (`a-z 0-9 _`, 1–32).
pub const MAX_KEY_CHARS: usize = ecad_model::objects::title_block::MAX_PROJECT_KEY;
/// The missing mark (`?`).
pub const MISSING: &str = "?";

/// Whether `key` is a project key a field may name: 1–32 characters of `a-z 0-9 _ .`.
pub fn is_project_key(key: &str) -> bool {
    ecad_model::objects::title_block::ProjectKey::new(key).is_some()
}

/// The project keys and the revision fields `layout`'s fields read.
pub fn keys(layout: &TitleBlockLayout) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut project = BTreeSet::new();
    let mut revision = BTreeSet::new();
    for b in layout.bindings() {
        match b {
            Binding::Project(k) => {
                project.insert(k.as_str().to_string());
            }
            Binding::Revision(RevisionColumn::Field(RevField::Rev)) => {}
            Binding::Revision(c) => {
                revision.insert(c.word().to_string());
            }
            Binding::Page(_) | Binding::Print(_) => {}
        }
    }
    (project, revision)
}

/// The values of the project keys and newest-revision fields `layout` reads, for the frame's
/// input: every key read is present (empty when unset), so a change to it is seen.
pub fn read(doc: &Document, layout: &TitleBlockLayout) -> (BTreeMap<String, String>, BTreeMap<String, String>) {
    let (pk, rk) = keys(layout);
    // A function or location is an entry key of the project's list, shown by its code.
    let project = pk
        .into_iter()
        .map(|k| {
            let v = crate::pages::frames::project_text(doc, &k).map(|v| crate::locations::designations::display(doc, &k, &v)).unwrap_or_default();
            (k, v)
        })
        .collect();
    let newest = if rk.is_empty() { None } else { crate::releases::revisions::issued(doc).into_iter().next() };
    let revision = rk.into_iter().map(|k| (k.clone(), newest.as_ref().map(|r| r.get(&k).to_string()).unwrap_or_default())).collect();
    (project, revision)
}

/// The value `b` shows on the page `input` describes.
pub fn value(input: &FrameInput, b: &Binding) -> String {
    match b {
        Binding::Project(k) => input.project.get(k.as_str()).cloned().unwrap_or_default(),
        Binding::Page(f) => match f {
            PageField::Title => input.page_title.clone(),
            PageField::Number => input.page.clone(),
            PageField::Next => input.next.clone(),
            PageField::Total => input.total.clone(),
            PageField::Function => input.function.clone(),
            PageField::Location => input.location.clone(),
        },
        Binding::Revision(RevisionColumn::Field(RevField::Rev)) => input.rev.clone(),
        Binding::Revision(c) => input.newest.get(c.word()).cloned().unwrap_or_default(),
        Binding::Print(PrintField::Sheet) => crate::title_blocks::sheet_name(&input.sheet),
    }
}

/// What a field shows: its value, or the "if empty" choice when the value is empty.
pub fn shown(value: String, if_empty: &IfEmpty) -> String {
    match Some(value).filter(|v| !v.trim().is_empty()) {
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
    let n = match b {
        Binding::Project(k) => match k.as_str() {
            "title" => "Project name",
            "drawing_number" => "Drawing number",
            "customer" => "Customer",
            "status" => "Status",
            "company" => "Company",
            _ => return b.text(),
        },
        Binding::Page(f) => match f {
            PageField::Title => "Page title",
            PageField::Number => "Page number",
            PageField::Next => "Next page",
            PageField::Total => "Total pages",
            PageField::Function => "Function",
            PageField::Location => "Location",
        },
        Binding::Revision(RevisionColumn::Field(f)) => match f {
            RevField::Rev => "Revision",
            RevField::Date => "Revision date",
            RevField::Description => "Revision description",
            RevField::Drawn => "Drawn",
            RevField::Checked => "Checked",
            RevField::Approved => "Approved",
            RevField::Ecr => "ECR",
        },
        Binding::Revision(RevisionColumn::Custom(_)) => return b.text(),
        Binding::Print(PrintField::Sheet) => "Sheet size",
    };
    n.to_string()
}

/// The source chip the palette and the name tag show: `project`, `page`, `revision`, `people`
/// (drawn, checked, approved), `print`.
pub fn chip(b: &Binding) -> &'static str {
    match b {
        Binding::Revision(RevisionColumn::Field(RevField::Drawn | RevField::Checked | RevField::Approved)) => "people",
        _ => ecad_model::Word::word(b.source()),
    }
}
