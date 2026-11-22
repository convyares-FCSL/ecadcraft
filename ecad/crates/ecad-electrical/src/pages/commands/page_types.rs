//! The rules of the page type and folder commands (brief 054, design 054): a folder's start number
//! (`folderprops`), the page numbers `renumberpages` writes, and which generated pages
//! `deletepage` removes. The numbering rules themselves are [`crate::pages::page_types`]; the engine
//! crate's shims bring the reports up to date, open the page shown, commit the ops and answer in
//! JSON.

use std::collections::BTreeSet;

use ecad_model::document::book::Book;
use ecad_model::{Document, Folder, FolderId, Op, PageId, PageNo};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::commands::input::nullable;
use crate::commands::run::{Cx, Generic, Outcome};
use crate::error::{CmdError, msg};
use crate::pages::page_types::*;
use crate::reports::kept::ReportModels;

/// Most pages one `deletepage` names.
pub const MAX_PAGES: usize = 1_000;

// ---------------------------------------------------------------------------------------------
// folderprops

/// A start number as written: blank clears it (`None`), else 1 to 6 digits (trimmed).
///
/// # Errors
/// Anything else.
pub(crate) fn start_value(text: &str) -> Result<Option<String>, CmdError> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(None);
    }
    if is_start(t) { Ok(Some(t.to_string())) } else { Err(CmdError::refused(BAD_START)) }
}

/// What `folderprops` reads or writes.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FolderStart {
    /// The folder's name (`(gone)` for a start key whose folder is gone).
    pub name: String,
    /// The start number before.
    pub before: Option<String>,
    /// With a write: the start number after, and the op (none when it is the same).
    pub write: Option<(Option<String>, Vec<Op>)>,
}

/// `folderprops {folder, start?}`: folder `folder`'s start number, read (`start` absent) or set
/// (`Some(None)` clears it).
///
/// # Errors
/// No such folder (a start key whose folder is gone may only be cleared).
pub(crate) fn plan_folderprops(doc: &Document, folder: FolderId, start: Option<Option<String>>) -> Result<FolderStart, CmdError> {
    let old = doc.folders().get(&folder).ok_or(CmdError::refused(msg::NO_SUCH_FOLDER))?;
    let before = old.start.as_ref().map(|s| s.as_str().to_string());
    let write = match start {
        None => None,
        Some(after) => {
            let stored = match after.as_deref() {
                Some(t) => Some(PageNo::parse(t).ok_or(CmdError::refused(BAD_START))?),
                None => None,
            };
            let now = Folder { start: stored, ..old.clone() };
            let ops = if now == *old { Vec::new() } else { vec![Op::ModifyFolder { id: folder, before: old.clone(), after: now }] };
            Some((after, ops))
        }
    };
    Ok(FolderStart { name: old.name.clone(), before, write })
}

/// The echo of a folder's new start number.
pub(crate) fn start_words(name: &str, after: Option<&str>) -> String {
    match after {
        Some(n) => format!("Folder \"{name}\" starts at {n}. Press Renumber pages to number its pages from it."),
        None => format!("Folder \"{name}\" has no start number now."),
    }
}

// ---------------------------------------------------------------------------------------------
// renumberpages

/// The folder `renumberpages` is limited to, checked.
///
/// # Errors
/// No such folder.
pub(crate) fn folder_scope(doc: &Document, folder: Option<FolderId>) -> Result<Option<FolderId>, CmdError> {
    match folder {
        Some(f) if !doc.folders().contains_key(&f) => Err(CmdError::refused(msg::NO_SUCH_FOLDER)),
        f => Ok(f),
    }
}

/// What `renumberpages` records and says.
#[derive(Debug, Clone, PartialEq)]
pub struct Renumbered {
    /// One `SetProperty` of each page's number (empty when already in order).
    pub ops: Vec<Op>,
    /// Each page renumbered and its new number.
    pub pages: Vec<(PageId, String)>,
    /// The folders whose numbers overlap, in words.
    pub overlaps: Vec<String>,
}

/// `renumberpages {folder?}`: each folder from its start number, the rest from 1; with `scope`,
/// that folder and its sub-folders only. `reports` are the generated pages kept.
pub(crate) fn plan_renumber(doc: &Document, reports: Option<&ReportModels>, scope: Option<FolderId>) -> Renumbered {
    let pages = renumber(doc, reports, scope);
    let overlaps = overlaps(doc, reports).into_iter().map(|(a, b, n)| overlap_words(doc, a, b, n)).collect();
    let ops = pages
        .iter()
        .filter_map(|(page, n)| {
            let before = doc.pages().get(page)?;
            let after = ecad_model::Page { number: ecad_model::PageNo::parse(n), ..before.clone() };
            (after != *before).then(|| Op::ModifyPage { id: *page, before: before.clone(), after })
        })
        .collect();
    Renumbered { ops, pages, overlaps }
}

// ---------------------------------------------------------------------------------------------
// deletepage

/// What `deletepage` records and says.
#[derive(Debug, Clone, PartialEq)]
pub struct Deleted {
    /// One `RemovePage` per page, from the back.
    pub ops: Vec<Op>,
    /// The page to show instead, when the page shown goes.
    pub show: Option<PageId>,
    pub message: String,
}

/// `deletepage {page | pages}`: the generated pages `pages` removed (one transaction), with
/// `active` the page shown.
///
/// # Errors
/// None named, no such page, a drawn page, or every page.
pub fn plan_deletepage(doc: &Document, pages: &[PageId], active: PageId) -> Result<Deleted, CmdError> {
    if pages.is_empty() {
        return Err(CmdError::input("Give the page to delete."));
    }
    let set: BTreeSet<PageId> = pages.iter().copied().collect();
    for page in &set {
        if !doc.pages().contains_key(page) {
            return Err(CmdError::refused(msg::NO_SUCH_PAGE));
        }
        if crate::pages::generated::kind_of(doc, *page).is_none() || doc.representations_on(*page).next().is_some() {
            return Err(CmdError::refused(DRAWN_PAGE));
        }
    }
    let remaining: Vec<PageId> = doc.page_order().iter().copied().filter(|p| !set.contains(p)).collect();
    if remaining.is_empty() {
        return Err(CmdError::refused("A project keeps at least one page."));
    }
    // Remove from the back so each index is the page's place when its op is applied (and undo,
    // reversed, puts them back in order).
    let mut placed: Vec<(usize, PageId)> = doc.page_order().iter().enumerate().filter(|(_, p)| set.contains(p)).map(|(i, p)| (i, *p)).collect();
    placed.sort_by_key(|p| std::cmp::Reverse(p.0));
    let mut ops = Vec::new();
    let mut words = Vec::new();
    let book = Book::of(doc);
    for (index, page) in &placed {
        let Some(x) = doc.pages().get(page) else { continue };
        words.push(format!("{} ({})", x.title, book.number(doc, *page)));
        ops.push(Op::RemovePage { id: *page, page: x.clone(), index: *index });
    }
    words.reverse();
    // The page shown moves to the page before it when it goes (design 016 §4.3).
    let show = if set.contains(&active) {
        let at = doc.page_order().iter().position(|p| *p == active).unwrap_or(0);
        let earlier = doc.page_order().get(..at).unwrap_or_default().iter().rev().find(|p| !set.contains(p)).copied();
        earlier.or(remaining.first().copied())
    } else {
        None
    };
    Ok(Deleted { ops, show, message: format!("Removed {}. Press u to bring it back.", words.join(", ")) })
}

// ---------------------------------------------------------------------------------------------
// The commands (generic: crate::commands::run). `deletepage` closes the page's view and stays the
// engine's.

/// `folderprops {folder, start?}`: a folder's start number (1 to 6 digits, leading zeros set the
/// width); null or blank clears it; no `start` reads it.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FolderpropsParams {
    /// The folder.
    folder: FolderId,
    /// The start number ("100"), or null to clear it.
    #[serde(default, deserialize_with = "nullable")]
    start: Option<Option<String>>,
}

/// `renumberpages {folder?}`: renumbers each folder from its start number, the rest from 1; with
/// `folder`, that folder and its sub-folders only (one transaction).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RenumberpagesParams {
    /// Only this folder and its sub-folders.
    #[serde(default)]
    folder: Option<FolderId>,
}

impl Generic for FolderpropsParams {
    const ID: &'static str = "folderprops";
    const TITLE: &'static str = "Folder Properties";
    const HELP: &'static str =
        "{folder: id, start?: \"100\"|null} (a folder's start number: 1 to 6 digits, leading zeros set the width; null clears; no start: read only)";

    fn check(&self) -> Result<(), CmdError> {
        match &self.start {
            Some(Some(t)) => start_value(t).map(|_| ()),
            _ => Ok(()),
        }
    }

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let start = match self.start {
            None => None,
            Some(None) => Some(None),
            Some(Some(t)) => Some(start_value(&t)?),
        };
        let folder = self.folder;
        let planned = plan_folderprops(cx.doc, folder, start)?;
        let name = planned.name;
        let Some((after, ops)) = planned.write else {
            return Ok(Outcome::answer(json!({"folder": folder.hex(), "name": name, "start": planned.before})));
        };
        if ops.is_empty() {
            return Ok(Outcome::answer(json!({"folder": folder.hex(), "name": name, "start": after, "changed": false})));
        }
        let line = start_words(&name, after.as_deref());
        let answer = json!({"folder": folder.hex(), "name": name, "start": after, "changed": true, "message": line});
        Ok(Outcome::edit("Folder start number", ops, answer).echo(Some(line)))
    }
}

impl Generic for RenumberpagesParams {
    const ID: &'static str = "renumberpages";
    const TITLE: &'static str = "Renumber Pages";
    const HELP: &'static str = "{folder?: id} (renumbers each folder from its start number, the rest from 1; with folder, that folder and its sub-folders only; one transaction)";

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let doc = cx.doc;
        let scope = folder_scope(doc, self.folder)?;
        if crate::pages::generated::has_bom_page(doc) {
            cx.derived.reports(doc);
        }
        let plan = plan_renumber(doc, cx.derived.kept_reports(), scope);
        if plan.ops.is_empty() {
            let line = "Page numbers already in order.".to_string();
            let answer = json!({"renumbered": 0, "message": line, "overlaps": plan.overlaps});
            return Ok(Outcome::answer(answer).echo(Some(line)));
        }
        let count = plan.pages.len();
        let pages: Vec<Value> = plan.pages.iter().map(|(pg, n)| json!({"page": pg.hex(), "number": n})).collect();
        let line = format!("Renumbered {count} page{}.", if count == 1 { "" } else { "s" });
        let answer = json!({"renumbered": count, "pages": pages, "message": line, "overlaps": plan.overlaps});
        let mut lines = vec![line];
        lines.extend(plan.overlaps);
        Ok(Outcome::edit("Renumber pages", plan.ops, answer).echo_lines(lines))
    }
}
