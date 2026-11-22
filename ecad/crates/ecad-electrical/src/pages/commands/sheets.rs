//! The page, project, folder and revision commands' rules (brief 019, design 019 §5–§7; briefs
//! 026, 033, 052, 054): folders, page numbers and page properties, project properties and
//! settings, the tag style, and the pending revision and its issue. Each function reads the
//! document and returns the ops to commit and what to report.

use serde_json::{Map, Value, json};
use std::collections::{BTreeSet, HashMap};

use ecad_model::document::book::{self, Book, FUNCTION_KEY, LOCATION_KEY, MAX_FOLDER_DEPTH, PageNo};
use ecad_model::document::page::{PageKind, PageTitle};
use ecad_model::document::sheet::SheetSpec;
use ecad_model::naming::tag::{self, CASE_KEY, Case, DASH_KEY, TAG_KEY};
use ecad_model::{
    DeviceId, Document, Folder, FolderId, IdAllocator, Ledger, ObjectRef, Op, Page, PageId, ProjectSettings, Properties, ReferenceStyle, StyleRef,
};

use crate::defaults::strip_pages;
use crate::error::{CmdError, msg};
use crate::locations::designations::{self, Aspect, Pending};
use crate::pages::frames::{CUSTOMER, DRAWING_NUMBER, STATUS, TITLE};
use crate::pages::page_types::MAX_NUMBER_CHARS;
use crate::releases::revisions::{self, FIELDS, Revision};
use crate::reports::kept::ReportModels;
use crate::symbols::proposals::{self, Mode, Planned};
use ecad_model::GenKind;

/// Most characters of a folder name.
pub const MAX_FOLDER_NAME: usize = 100;
/// Most characters in a page title.
pub const MAX_PAGE_TITLE: usize = ecad_model::document::page::MAX_PAGE_TITLE;
/// The project property keys the dialog and the title block use, in order.
pub const PROJECT_KEYS: [&str; 6] = [TITLE, DRAWING_NUMBER, CUSTOMER, STATUS, FUNCTION_KEY, LOCATION_KEY];
/// Most characters of a free property key.
pub const MAX_KEY_CHARS: usize = 40;
/// The page-number step's range.
pub const MAX_STEP: u32 = ecad_model::naming::builtins::MAX_PAGE_NUMBER_STEP;

/// A `SetProperty` of `key` on `object` (whose properties are `props`) to `after`, when it changes.
fn set(object: ObjectRef, props: Option<&Properties>, key: &str, after: Option<String>) -> Option<Op> {
    let before = props.and_then(|p| p.get(key)).cloned();
    (before != after).then(|| Op::SetProperty { object, key: key.to_string(), before, after })
}

/// A value under the 018 §6.1 rules (trimmed, 200 characters, no control characters; `function`
/// starts with `=`, `location` with `+`; empty clears).
fn value(doc: &Document, key: &str, v: Option<&str>) -> Result<Option<String>, CmdError> {
    crate::devices::json::property_value(doc, key, v)
}

/// The `proposal` word of a page's or the project's function or location (brief 052d):
/// "propose" (the default), "move" or "keep", in any case.
///
/// # Errors
/// Any other word.
pub fn proposal_mode(word: Option<&str>) -> Result<Mode, CmdError> {
    match word.map(|w| w.trim().to_ascii_lowercase()).as_deref() {
        None | Some("propose") => Ok(Mode::Propose),
        Some("move") => Ok(Mode::Move),
        Some("keep") => Ok(Mode::Keep),
        Some(_) => Err(CmdError::refused(msg::PROPOSAL_WORDS)),
    }
}

/// A page title: 1 to [`MAX_PAGE_TITLE`] characters after trimming, one line, no hidden
/// characters ([`PageTitle`]).
///
/// # Errors
/// [`msg::PAGE_TITLE`].
pub fn page_title(text: &str) -> Result<PageTitle, CmdError> {
    PageTitle::new(text).map_err(|_| CmdError::refused(msg::PAGE_TITLE))
}

// ---------------------------------------------------------------- folders

/// A folder name: 1 to [`MAX_FOLDER_NAME`] characters after trimming, no control characters.
///
/// # Errors
/// In words.
pub fn folder_name(text: &str) -> Result<String, CmdError> {
    let t = text.trim();
    let n = t.chars().count();
    if !(1..=MAX_FOLDER_NAME).contains(&n) || t.chars().any(char::is_control) {
        return Err(CmdError::refused(msg::FOLDER_NAME));
    }
    Ok(t.to_string())
}

/// That folder `f`, when given, exists.
///
/// # Errors
/// [`msg::NO_SUCH_FOLDER`].
pub fn check_folder(doc: &Document, f: Option<FolderId>) -> Result<(), CmdError> {
    match f {
        Some(f) if !doc.folders.contains_key(&f) => Err(CmdError::refused(msg::NO_SUCH_FOLDER)),
        _ => Ok(()),
    }
}

/// The folder `f`.
fn folder(doc: &Document, f: FolderId) -> Result<Folder, CmdError> {
    doc.folders.get(&f).cloned().ok_or(CmdError::refused(msg::NO_SUCH_FOLDER))
}

/// The folders whose parent is `parent`, in folder order.
fn siblings(doc: &Document, parent: Option<FolderId>) -> Vec<FolderId> {
    doc.folder_order.iter().copied().filter(|f| doc.folders.get(f).is_some_and(|x| x.parent == parent)).collect()
}

fn check_sibling_name(doc: &Document, parent: Option<FolderId>, name: &str, except: Option<FolderId>) -> Result<(), CmdError> {
    let clash = siblings(doc, parent)
        .into_iter()
        .filter(|f| Some(*f) != except)
        .any(|f| doc.folders.get(&f).is_some_and(|x| x.name.eq_ignore_ascii_case(name)));
    if clash {
        return Err(CmdError::refused(format!("There is already a folder \"{name}\" here.")));
    }
    Ok(())
}

fn too_deep() -> CmdError {
    CmdError::refused(format!("Folders nest at most {MAX_FOLDER_DEPTH} deep."))
}

fn depth_of(doc: &Document, parent: Option<FolderId>) -> usize {
    parent.and_then(|p| book::folder_depth(doc, p)).unwrap_or(0)
}

/// How many levels `folder` and its sub-folders take (1 for a folder with none).
fn height(doc: &Document, folder: FolderId) -> usize {
    let mut best = 1;
    let mut todo = vec![(folder, 1usize)];
    let mut seen = BTreeSet::new();
    while let Some((f, h)) = todo.pop() {
        if !seen.insert(f) || h > MAX_FOLDER_DEPTH + 1 {
            continue;
        }
        best = best.max(h);
        todo.extend(doc.subfolders(f).into_iter().map(|c| (c, h + 1)));
    }
    best
}

/// `newfolder`: a folder `name` (as [`folder_name`] gives it) in `parent`, last in folder order.
/// Its id and the op.
///
/// # Errors
/// No such parent, a sibling of that name, or too deep.
pub fn new_folder(doc: &Document, ids: &mut IdAllocator, name: &str, parent: Option<FolderId>) -> Result<(FolderId, Op), CmdError> {
    check_folder(doc, parent)?;
    check_sibling_name(doc, parent, name, None)?;
    if depth_of(doc, parent) + 1 > MAX_FOLDER_DEPTH {
        return Err(too_deep());
    }
    let id: FolderId = ids.fresh();
    let index = doc.folder_order.len();
    Ok((id, Op::AddFolder { id, folder: Folder::new(name, parent), index }))
}

/// `renamefolder`: the op renaming `folder` to `name`; `None` when it already has that name.
///
/// # Errors
/// No such folder, or a sibling of that name.
pub fn rename_folder(doc: &Document, f: FolderId, name: &str) -> Result<Option<Op>, CmdError> {
    let old = folder(doc, f)?;
    if old.name == name {
        return Ok(None);
    }
    check_sibling_name(doc, old.parent, name, Some(f))?;
    let after = Folder { name: name.to_string(), ..old.clone() };
    Ok(Some(Op::ModifyFolder { id: f, before: old, after }))
}

/// The index in the folder order that puts `folder` at 1-based `index` among `parent`'s other
/// sub-folders (default last), in the order with `folder` removed.
fn folder_slot(doc: &Document, folder: FolderId, parent: Option<FolderId>, index: Option<usize>) -> usize {
    let order: Vec<FolderId> = doc.folder_order.iter().copied().filter(|f| *f != folder).collect();
    let sibs: Vec<FolderId> = order.iter().copied().filter(|f| doc.folders.get(f).is_some_and(|x| x.parent == parent)).collect();
    let pos = |f: &FolderId| order.iter().position(|x| x == f).unwrap_or(order.len());
    match index.and_then(|i| i.checked_sub(1)).and_then(|i| sibs.get(i)) {
        Some(f) => pos(f),
        None => sibs.last().map_or(order.len(), |f| pos(f) + 1),
    }
}

/// `movefolder`: the ops moving `f` into `parent` (None: the top level) at 1-based `index` among
/// its new siblings (default last); none when nothing changes.
///
/// # Errors
/// No such folder or parent, into itself, a sibling of that name, or too deep.
pub fn move_folder(doc: &Document, f: FolderId, parent: Option<FolderId>, index: Option<usize>) -> Result<Vec<Op>, CmdError> {
    let old = folder(doc, f)?;
    check_folder(doc, parent)?;
    // Onto itself or one of its own sub-folders.
    let mut at = parent;
    let mut steps = 0;
    while let Some(x) = at {
        if x == f {
            return Err(CmdError::refused("A folder cannot go inside itself."));
        }
        at = doc.folders.get(&x).and_then(|x| x.parent);
        steps += 1;
        if steps > MAX_FOLDER_DEPTH + 1 {
            break;
        }
    }
    if parent != old.parent {
        check_sibling_name(doc, parent, &old.name, Some(f))?;
    }
    if depth_of(doc, parent) + height(doc, f) > MAX_FOLDER_DEPTH {
        return Err(too_deep());
    }
    let from = doc.folder_order.iter().position(|x| *x == f).ok_or(CmdError::refused(msg::NO_SUCH_FOLDER))?;
    let to = folder_slot(doc, f, parent, index);
    if to == from && parent == old.parent {
        return Ok(Vec::new());
    }
    let after = Folder { parent, ..old.clone() };
    Ok(vec![Op::RemoveFolder { id: f, folder: old, index: from }, Op::AddFolder { id: f, folder: after, index: to }])
}

/// What `deletefolder` does.
#[derive(Debug, Clone, PartialEq)]
pub struct DeletedFolder {
    pub ops: Vec<Op>,
    /// How many pages moved to its parent.
    pub pages: usize,
    /// The echo, in words.
    pub message: String,
}

/// `deletefolder`: its pages and sub-folders move to its parent, its start number goes with it.
///
/// # Errors
/// No such folder, or a sub-folder whose name its new place already has.
pub fn delete_folder(doc: &Document, f: FolderId) -> Result<DeletedFolder, CmdError> {
    let old = folder(doc, f)?;
    let mut ops = Vec::new();
    let pages = doc.pages_in_folder(f);
    for page in &pages {
        if let Some(before) = doc.pages.get(page) {
            let after = Page { folder: old.parent, ..before.clone() };
            ops.push(Op::ModifyPage { id: *page, before: before.clone(), after });
        }
    }
    for sub in doc.subfolders(f) {
        let before = folder(doc, sub)?;
        if check_sibling_name(doc, old.parent, &before.name, Some(f)).is_err() {
            return Err(CmdError::refused(format!("Rename sub-folder \"{}\" first: its new place already has a folder of that name.", before.name)));
        }
        let after = Folder { parent: old.parent, ..before.clone() };
        ops.push(Op::ModifyFolder { id: sub, before, after });
    }
    let index = doc.folder_order.iter().position(|x| *x == f).ok_or(CmdError::refused(msg::NO_SUCH_FOLDER))?;
    ops.push(Op::RemoveFolder { id: f, folder: old.clone(), index });
    let to = match old.parent.and_then(|x| doc.folders.get(&x)) {
        Some(x) => format!("\"{}\"", x.name),
        None => "the top level".into(),
    };
    let n = pages.len();
    let message = format!("Folder \"{}\" deleted; its {n} page{} moved to {to}.", old.name, if n == 1 { "" } else { "s" });
    Ok(DeletedFolder { ops, pages: n, message })
}

// ---------------------------------------------------------------- pages

/// The refusal of a page number that does not read.
pub const NUMBER_WORDS: &str = "A page number is 1 to 12 letters, digits or _ (no spaces, `.`, `-` or `/`).";

/// Check a page number (design 019 §6.1) against every other page's shown number, continuation
/// sheets of generated pages included (brief 033, design 033 §5.3).
///
/// # Errors
/// In words.
pub fn check_number(doc: &Document, book: &Book, page: Option<PageId>, number: &str, reports: Option<&ReportModels>) -> Result<(), CmdError> {
    let n = number.chars().count();
    if !(1..=MAX_NUMBER_CHARS).contains(&n) || !number.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(CmdError::refused(NUMBER_WORDS));
    }
    for p in &book.order {
        if Some(*p) != page && book.number(doc, *p).eq_ignore_ascii_case(number) {
            let title = doc.pages.get(p).map(|x| x.title.as_str()).unwrap_or_default();
            return Err(CmdError::refused(format!("Page {number} already exists (\"{title}\").")));
        }
    }
    if let Some((owner, i)) = crate::pages::generated::continuation_of(doc, book, number, reports)
        && Some(owner) != page
    {
        return Err(CmdError::refused(crate::pages::generated::continuation_taken(doc, number, owner, i)));
    }
    Ok(())
}

/// The ops that move `page` into `folder` (None: the top level) at 1-based `index` among that
/// folder's pages (default last): a RemovePage + AddPage pair when its place in the page order
/// changes, else a `ModifyPage` of its folder.
///
/// # Errors
/// No such folder or page.
pub fn move_page_ops(doc: &Document, page: PageId, folder: Option<FolderId>, index: Option<usize>) -> Result<Vec<Op>, CmdError> {
    check_folder(doc, folder)?;
    let old = doc.pages.get(&page).cloned().ok_or(CmdError::refused(msg::NO_SUCH_PAGE))?;
    let mut ops = Vec::new();
    let now = Page { folder, ..old.clone() };
    let from = doc.page_order.iter().position(|p| *p == page).ok_or(CmdError::refused(msg::NO_SUCH_PAGE))?;
    let order: Vec<PageId> = doc.page_order.iter().copied().filter(|p| *p != page).collect();
    let in_folder: Vec<PageId> =
        order.iter().copied().filter(|p| book::folder_of(doc, *p).filter(|f| doc.folders.contains_key(f)) == folder).collect();
    let pos = |p: &PageId| order.iter().position(|x| x == p).unwrap_or(order.len());
    let to = match index.and_then(|i| i.checked_sub(1)).and_then(|i| in_folder.get(i)) {
        Some(p) => pos(p),
        None => match in_folder.last() {
            Some(p) => pos(p) + 1,
            None => from.min(order.len()),
        },
    };
    if to != from {
        ops.push(Op::RemovePage { id: page, page: old, index: from });
        ops.push(Op::AddPage { id: page, page: now, index: to });
    } else if now != old {
        ops.push(Op::ModifyPage { id: page, before: old, after: now });
    }
    Ok(ops)
}

/// The kind of page `newpage` makes (briefs 024, 028, 046): a generated page's kind, a terminal
/// plan's style and strip.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NewPageKind {
    pub generated: Option<GenKind>,
    /// The terminal plan's style word (`table` or `graphic`).
    pub style: Option<String>,
    /// The one strip a terminal plan shows.
    pub strip: Option<DeviceId>,
}

/// `newpage`'s `generated`, `style` and `strip` read together.
///
/// # Errors
/// A kind or style word that does not read; a style or strip for a page that is not a terminal
/// plan.
pub fn new_page_kind(generated: Option<&str>, style: Option<&str>, strip: Option<DeviceId>) -> Result<NewPageKind, CmdError> {
    // Brief 024 (FP1): a generated page's kind, set only here.
    let generated = match generated {
        Some(t) => Some(
            <GenKind as ecad_model::Word>::parse(t)
                .ok_or(CmdError::input("`generated` is \"cover\", \"contents\", \"revisions\", \"bom-location\", \"terminals\" or \"cables\""))?,
        ),
        None => None,
    };
    // Brief 046 (design 046 §8.1): a terminal plan page's style, stored as its page-level
    // `terminal_block.plan.style` in the AddPage, so the page is one transaction.
    let style = match style {
        Some(t) => Some(
            strip_pages::STYLE
                .parse(t)
                .and_then(|v| if let crate::defaults::Value::Choice(w) = v { Some(w.to_string()) } else { None })
                .ok_or(CmdError::input("`style` is \"table\" or \"graphic\""))?,
        ),
        None => None,
    };
    if style.is_some() && generated != Some(GenKind::TerminalBlocks) {
        return Err(CmdError::input("`style` is for a terminal plan page (`generated: \"terminals\"`)"));
    }
    // Brief 028 (FP3): a terminal plan page may show one strip.
    if strip.is_some() && generated != Some(GenKind::TerminalBlocks) {
        return Err(CmdError::input("`strip` is for a terminal plan page (`generated: \"terminals\"`)"));
    }
    Ok(NewPageKind { generated, style, strip })
}

/// The type a new page of `kind` gets (design 054 §4.2: `ty`, a page type's ID or name), and its
/// title when none is given and it has no generated kind: the type's name.
///
/// # Errors
/// `strip` is not a terminal strip; no page `after`; the type's refusals.
pub fn new_page_type(
    doc: &Document, kind: &NewPageKind, after: Option<PageId>, ty: Option<&str>,
) -> Result<(Option<StyleRef>, Option<String>), CmdError> {
    if let Some(c) = kind.strip
        && crate::terminal_strips::strip_of(doc, c).is_none()
    {
        return Err(CmdError::refused("That device is not a terminal strip; leave `strip` out to show every strip."));
    }
    if after.is_some_and(|a| !doc.pages.contains_key(&a)) {
        return Err(CmdError::refused("No such page to add after."));
    }
    let page_type = crate::pages::page_types::newpage_type(doc, ty, kind.generated)?;
    // A page with no kind is titled after its type (design 054 §4.2).
    let title = match (&page_type, kind.generated) {
        (Some(t), None) => Some(crate::styles::name(doc, ecad_model::StyleCatalogue::Page, t)),
        _ => None,
    };
    Ok((page_type, title))
}

/// Stamp the new page in `ops` (its `AddPage`) with its type (only when it differs from what the
/// kind gives, FP2) and its kind, strip and style.
pub fn stamp_new_page(ops: &mut [Op], kind: &NewPageKind, page_type: Option<&StyleRef>) {
    let derived = crate::styles::page::derived(kind.generated);
    for op in ops {
        if let Op::AddPage { page, .. } = op {
            if let Some(t) = page_type.filter(|t| **t != derived) {
                page.page_type = Some(t.clone());
            }
            if let Some(what) = kind.generated {
                page.kind = match kind.strip {
                    Some(strip) => PageKind::TerminalStrip { strip },
                    None => PageKind::Generated(what),
                };
                if let Some(v) = &kind.style {
                    page.display.insert(strip_pages::STYLE.key.into(), v.clone());
                }
            }
        }
    }
}

/// A page renamed or moved: its title, its 1-based position, and the ops (empty when nothing
/// changes).
#[derive(Debug, Clone, PartialEq)]
pub struct PageMoved {
    pub title: String,
    pub index: usize,
    pub ops: Vec<Op>,
}

/// `renamepage` (design 010 §2.3, §7.5; design 072 §5.1): a `ModifyPage` with only the title
/// changed. The same title changes nothing.
///
/// # Errors
/// A title that does not read ([`page_title`]); no such page.
pub fn rename_page(doc: &Document, page: PageId, title: &str) -> Result<PageMoved, CmdError> {
    let title = page_title(title)?;
    let old = doc.pages.get(&page).cloned().ok_or(CmdError::refused(msg::NO_SUCH_PAGE))?;
    let index = doc.page_order.iter().position(|x| *x == page).ok_or(CmdError::refused(msg::NO_SUCH_PAGE))?;
    if old.title == title {
        return Ok(PageMoved { title: title.to_string(), index: index + 1, ops: Vec::new() });
    }
    // The page as it is with only its title changed, so any other field of a page is kept.
    let renamed = Page { title: title.clone(), ..old.clone() };
    let ops = vec![Op::ModifyPage { id: page, before: old, after: renamed }];
    Ok(PageMoved { title: title.to_string(), index: index + 1, ops })
}

/// `movepage {page, index}` (design 010 §2.3, §7.5): `RemovePage` at the old position + `AddPage`
/// at `index` (1 = first). The same position changes nothing.
///
/// # Errors
/// `index` not from 1 to the number of pages; no such page.
pub fn move_page_to(doc: &Document, page: PageId, index: Option<usize>) -> Result<PageMoved, CmdError> {
    let n = doc.page_order.len();
    let to = index.filter(|i| (1..=n).contains(i)).ok_or_else(|| CmdError::input(format!("`index` must be a whole number from 1 to {n}")))?;
    let old = doc.pages.get(&page).cloned().ok_or(CmdError::refused(msg::NO_SUCH_PAGE))?;
    let from = doc.page_order.iter().position(|x| *x == page).ok_or(CmdError::refused(msg::NO_SUCH_PAGE))?;
    let title = old.title.to_string();
    if from + 1 == to {
        return Ok(PageMoved { title, index: to, ops: Vec::new() });
    }
    let ops = vec![Op::RemovePage { id: page, page: old.clone(), index: from }, Op::AddPage { id: page, page: old, index: to - 1 }];
    Ok(PageMoved { title, index: to, ops })
}

/// What `pageprops` writes: each `Some` is given (`Some(None)` clears).
#[derive(Debug, Clone, Default)]
pub struct PageChange {
    pub number: Option<Option<String>>,
    /// The new title, as [`page_title`] gives it.
    pub title: Option<PageTitle>,
    pub function: Option<Option<String>>,
    pub location: Option<Option<String>>,
    pub sheet: Option<Option<String>>,
    /// What a new function or location proposes for the page's device symbols.
    pub mode: Mode,
}

impl PageChange {
    /// Whether anything is written (else `pageprops` only reads).
    pub fn writes(&self) -> bool {
        self.number.is_some() || self.title.is_some() || self.function.is_some() || self.location.is_some() || self.sheet.is_some()
    }
}

/// The ops of a property change, and what a new function or location proposed for device symbols
/// (design 052 §5.6).
#[derive(Debug, Default)]
pub struct PropsPlan {
    /// Every op, the proposals' included.
    pub ops: Vec<Op>,
    pub proposed: Planned,
    /// The echo of a tag-style change (`projectprops`).
    pub message: Option<String>,
}

/// Merge `x` into `into`.
fn absorb(into: &mut Planned, x: Planned) {
    into.ops.extend(x.ops);
    into.set.extend(x.set);
    into.conflicts.extend(x.conflicts);
    into.moved += x.moved;
}

/// The code of entry `key` of aspect `a`, also when `pending` adds it in this transaction.
fn code_with(doc: &Document, pending: &Pending, a: Aspect, key: &str) -> String {
    designations::code_of(doc, a, key).or_else(|| pending.new_codes.iter().rev().find(|(x, _)| *x == a).map(|(_, c)| c.clone())).unwrap_or_default()
}

/// `pageprops` (design 019 §6): the ops writing `change` to `page`.
///
/// # Errors
/// No such page, a page number in use, a bad function, location or sheet.
pub fn page_props(
    doc: &Document, ids: &mut IdAllocator, page: PageId, change: &PageChange, reports: Option<&ReportModels>,
) -> Result<PropsPlan, CmdError> {
    let old = doc.pages.get(&page).cloned().ok_or(CmdError::refused(msg::NO_SUCH_PAGE))?;
    let book = Book::of(doc);
    let mut ops = Vec::new();
    // Every change goes into one `ModifyPage`, written after the list entries it names.
    let mut now = old.clone();
    if let Some(n) = &change.number {
        let n = n.as_ref().map(|t| t.trim()).filter(|t| !t.is_empty());
        now.number = match n {
            Some(t) => {
                check_number(doc, &book, Some(page), t, reports)?;
                Some(PageNo::parse(t).ok_or(CmdError::refused(NUMBER_WORDS))?)
            }
            None => None,
        };
    }
    // Brief 052 (design 052 §5.9): a typed code is stored as its entry's key; an unknown code
    // adds its entry in the same transaction.
    let mut pending = Pending::default();
    // Brief 052d (design 052 §5.6): a new page value proposes itself for the device symbols on
    // the page outside every box carrying that aspect (`proposal`, as `locationbox`).
    let mut proposed = Planned::default();
    for (key, v) in [(FUNCTION_KEY, &change.function), (LOCATION_KEY, &change.location)] {
        let Some(v) = v else { continue };
        let a = Aspect::of_property(key).ok_or(CmdError::refused("not an aspect"))?;
        let stored = match v.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
            Some(t) => Some(pending.store(doc, ids, a, t, &mut ops)?),
            None => None,
        };
        let old = designations::page_key(&old, a);
        if let Some(k) = stored.as_ref().filter(|k| old.as_ref() != Some(*k)) {
            let code = code_with(doc, &pending, a, k);
            let spots = proposals::spots_on_page(doc, a, page);
            let x = proposals::plan_change(doc, &proposed.ops, a, &spots, old.as_deref(), k, &code, &change.mode);
            absorb(&mut proposed, x);
        }
        now = designations::with_page_key(&now, a, stored.as_deref());
    }
    if let Some(v) = &change.sheet {
        let v = v.as_ref().map(|t| t.trim()).filter(|t| !t.is_empty() && !t.eq_ignore_ascii_case("project"));
        now.sheet = match v {
            Some(t) => Some(SheetSpec::parse(t)?),
            None => None,
        };
    }
    if let Some(t) = &change.title {
        now.title = t.clone();
    }
    if now != old {
        ops.push(Op::ModifyPage { id: page, before: old, after: now });
    }
    ops.extend(proposed.ops.iter().cloned());
    Ok(PropsPlan { ops, proposed, message: None })
}

// ---------------------------------------------------------------- project

/// A free project property's key (not one of [`PROJECT_KEYS`], a revision or a folder key).
///
/// # Errors
/// A key written by another command, a reserved key, or not 1 to 40 characters a-z, 0-9 or _.
pub fn check_free_key(doc: &Document, key: &str) -> Result<(), CmdError> {
    // Brief 052: the project's list of functions and locations changes through its commands.
    if ecad_model::naming::keys::is_reserved_on(ecad_model::naming::keys::Holder::Project, key) && key.starts_with("designation.") {
        return Err(CmdError::refused(
            "Functions and locations are changed with `designationadd`, `designationedit`, `designationmerge` and `designationremove`.",
        ));
    }
    // Brief 031 (§10.6): `text.` keys are written by the text commands only.
    if crate::styles::text_style::is_reserved_key(key) {
        return Err(CmdError::refused(crate::styles::text_style::RESERVED_KEY));
    }
    if let Some(rest) = key.strip_prefix(revisions::PREFIX) {
        let n = rest.split('.').next().and_then(|n| n.parse::<u32>().ok());
        if let Some(r) = n.and_then(|n| revisions::list(doc).into_iter().find(|r| r.n == n)).filter(|r| r.issued.is_some()) {
            return Err(issued_row(&r));
        }
        return Err(CmdError::refused("Revision fields are changed with `revision` (Revisions…)."));
    }
    if PROJECT_KEYS.contains(&key) {
        return Err(CmdError::refused(format!("\"{key}\" is set above.")));
    }
    if key.starts_with("folder") {
        return Err(CmdError::refused(format!("\"{key}\" is reserved.")));
    }
    // Brief 026: the numbering keys are written by `numbering`, Renumber and Finalise.
    if key.starts_with("numbering.") {
        return Err(CmdError::refused(format!("\"{key}\" is reserved (Project settings → Numbering).")));
    }
    // Brief 036: the registry's terminal defaults are project properties of their entry's key.
    if key.starts_with("terminal_block.") && crate::defaults::find(key).is_some() {
        return Ok(());
    }
    // Brief 054 (FP4, design 054 §8.1a): the cover's notes and its table editor rows.
    if key == crate::pages::front::NOTES_KEY || key == crate::pages::front::WIRE_ROWS_KEY || key == crate::pages::front::LEGEND_ROWS_KEY {
        return Ok(());
    }
    let n = key.chars().count();
    if !(1..=MAX_KEY_CHARS).contains(&n) || !key.chars().all(|c| matches!(c, 'a'..='z' | '0'..='9' | '_')) {
        return Err(CmdError::refused("A property name is 1 to 40 characters a-z, 0-9 or _."));
    }
    Ok(())
}

/// The value free property `key` stores for `v` (`None` clears it).
fn free_value(doc: &Document, key: &str, v: Option<&str>) -> Result<Option<String>, CmdError> {
    let Some(t) = v else { return Ok(None) };
    // Brief 054 (FP4, design 054 §6.4): the cover's notes, one per line, bounded.
    if key == crate::pages::front::NOTES_KEY {
        let lines: Vec<&str> = t.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
        if lines.len() > crate::pages::front::MAX_NOTES || lines.iter().any(|l| l.chars().count() > crate::pages::front::MAX_NOTE_CHARS) {
            return Err(CmdError::refused(crate::pages::front::NOTES_TOO_LONG));
        }
        return Ok((!lines.is_empty()).then(|| lines.join("\n")));
    }
    // Brief 054 (design 054 §8.1a): the cover's table editor rows.
    if key == crate::pages::front::WIRE_ROWS_KEY || key == crate::pages::front::LEGEND_ROWS_KEY {
        crate::pages::front::check_overrides(t)?;
        return Ok((!t.trim().is_empty()).then(|| t.trim_end().to_string()));
    }
    // Brief 030 (FP3, FP6): strip accessory defaults are checked like the strip's.
    Ok(match crate::terminal_strips::accessories::project_rule(doc, key, Some(t)) {
        Some(r) => r?,
        // Brief 036: the registry's terminal defaults (`""` on the separator is none).
        None => match crate::defaults::terminal_blocks::setprop_value(key, Some(t)) {
            Some(r) => r?,
            None => value(doc, key, Some(t))?,
        },
    })
}

/// What `projectprops` writes. Each `Some` is given; `Some(None)` clears where that means
/// something.
#[derive(Debug, Clone, Default)]
pub struct ProjectChange {
    /// [`PROJECT_KEYS`] given, in that order.
    pub fields: Vec<(&'static str, Option<String>)>,
    /// Free properties, in order.
    pub props: Vec<(String, Option<String>)>,
    pub provisional: Option<bool>,
    pub sheet: Option<String>,
    pub page_number_step: Option<u64>,
    pub reference_style: Option<ReferenceStyle>,
    pub contact_mirror: Option<String>,
    pub tag_padding: Option<u64>,
    pub tag_prefixes: Vec<(String, Option<String>)>,
    pub colour_style: Option<String>,
    pub label_height: Option<f64>,
    pub revision_columns: Option<Option<String>>,
    pub revision_pending: Option<String>,
    pub tag_dash: Option<bool>,
    pub tag_case: Option<Case>,
    /// What a new function or location proposes for the device symbols.
    pub mode: Mode,
}

/// Tag padding and prefixes (`projectprops`' `tag_padding` and `tag_prefixes`, named here by
/// `padding_key`) written to `settings`: a prefix equal to its symbol's built-in one, or `None`,
/// goes back to the built-in one.
///
/// # Errors
/// A padding out of 1 to [`tag::MAX_PADDING`], no such symbol, or a bad prefix; nothing is written.
pub fn tag_settings(
    settings: &mut ProjectSettings, padding_key: &str, padding: Option<u64>, prefixes: &[(String, Option<String>)],
) -> Result<(), CmdError> {
    let padding = match padding {
        None => None,
        Some(n) => Some(
            Some(n)
                .filter(|n| (1..=u64::from(tag::MAX_PADDING)).contains(n))
                .and_then(|n| u8::try_from(n).ok())
                .ok_or_else(|| CmdError::refused(format!("`{padding_key}` must be a whole number from 1 to {}", tag::MAX_PADDING)))?,
        ),
    };
    let mut changes: Vec<(&'static str, Option<String>)> = Vec::new();
    for (k, v) in prefixes {
        let symbol = crate::devices::json::starter(k)?;
        let prefix = match v {
            None => None,
            Some(t) => Some(crate::devices::valid_prefix(t).ok_or_else(|| {
                CmdError::refused(format!("A prefix is 1 to {} letters, for example K or SR.", crate::devices::MAX_PREFIX_LETTERS))
            })?),
        };
        changes.push((ecad_model::Word::word(symbol.key), prefix));
    }
    if let Some(n) = padding {
        settings.tag_padding = Some(n);
    }
    for (k, v) in changes {
        crate::devices::set_symbol_prefix(settings, ecad_model::naming::prefix::PrefixOf::Starter(k.into()), v.as_deref());
    }
    Ok(())
}

/// `projectprops`' `tag_case` word: `upper` or `typed`.
///
/// # Errors
/// Any other word.
pub fn tag_case(word: &str) -> Result<Case, CmdError> {
    <Case as ecad_model::Word>::parse(word).ok_or(CmdError::refused("`tag_case` must be \"upper\" or \"typed\""))
}

/// The ops of `tag_dash` and `tag_case` in `projectprops` (design 033 §8.4): the settings, and
/// one `ModifyDevice` or `ModifyCable` per component or cable tag whose stored text changes (a new dash, or
/// back to upper case). The echo in words. Tags are identities by `tag::key`, which ignores the
/// dash and the case, so no index changes key; each rewrite is an ordinary tag op that every
/// tag-showing view already maps. Merging two branches with different dash settings: the merged
/// `tags.dash` is the branch that changed it, its rewrites come with it, and tags the other branch
/// added keep their own form until the style is set again.
///
/// # Errors
/// A bad case word, or (never expected: the key ignores what changes) two different tags
/// rewritten to the same text.
pub fn tag_style_ops(doc: &Document, dash: Option<bool>, case: Option<Case>) -> Result<(Vec<Op>, Option<String>), CmdError> {
    let before = crate::defaults::print::tag_style(doc);
    let after = tag::Style { dash: dash.unwrap_or(before.dash), case: case.unwrap_or(before.case), ..before };
    let mut ops = Vec::new();
    if let Some(d) = dash {
        ops.extend(set(ObjectRef::Project, Some(&doc.display), DASH_KEY, Some(ecad_model::naming::words::flag_word(d).to_string())));
    }
    if let Some(c) = case {
        ops.extend(set(ObjectRef::Project, Some(&doc.display), CASE_KEY, Some(ecad_model::Word::word(c).to_string())));
    }
    let rewrite = after.dash != before.dash || (after.case == Case::Upper && before.case == Case::Typed);
    let mut n = 0usize;
    if rewrite {
        let mut seen: HashMap<String, String> = HashMap::new();
        let objects = doc
            .devices
            .iter()
            .filter_map(|(id, c)| c.tag().map(|t| (ObjectRef::Device(*id), t)))
            .chain(doc.cables.iter().filter_map(|(id, c)| c.properties.get(TAG_KEY).map(|t| (ObjectRef::Cable(*id), t.as_str()))));
        for (object, old) in objects {
            let new = tag::restyle(old, after);
            if let Some(k) = seen.get(&new)
                && *k != tag::key(old)
            {
                return Err(CmdError::refused(format!("Changing the tag style would make two tags {new}; nothing was changed.")));
            }
            seen.insert(new.clone(), tag::key(old));
            if new != old {
                match object {
                    ObjectRef::Device(c) => {
                        if let Some(d) = doc.devices.get(&c) {
                            ops.push(Op::ModifyDevice { id: c, before: d.clone(), after: d.with_tag(Some(&new)) });
                        }
                    }
                    ObjectRef::Cable(k) => Op::set_cable_property(doc, &mut ops, k, TAG_KEY, Some(new)),
                    _ => ops.push(Op::SetProperty { object, key: TAG_KEY.into(), before: Some(old.to_string()), after: Some(new) }),
                }
                n += 1;
            }
        }
    }
    let rewritten = if n == 1 { "1 tag rewritten".to_string() } else { format!("{n} tags rewritten") };
    let words = if after.dash != before.dash {
        Some(format!("Tag style: {}. {rewritten}.", if after.dash { "leading -" } else { "no leading -" }))
    } else if after.case != before.case {
        Some(match after.case {
            Case::Typed => "Tags typed from now on keep their case; existing tags are unchanged.".to_string(),
            Case::Upper => format!("Tag style: upper case. {rewritten}."),
        })
    } else {
        None
    };
    Ok((ops, words))
}

/// `projectprops` (design 019 §6): the ops writing `change`, in one transaction.
///
/// # Errors
/// The first value refused, in words.
pub fn project_props(doc: &Document, ids: &mut IdAllocator, change: &ProjectChange) -> Result<PropsPlan, CmdError> {
    let mut ops = Vec::new();
    let mut pending = Pending::default();
    // Brief 052d (design 052 §5.6): a new project value proposes itself for the device symbols
    // whose context falls back to the project (the one project-wide case).
    let mut proposed = Planned::default();
    for (k, v) in &change.fields {
        // Brief 052 (design 052 §5.9): the project's function and location are entry keys.
        let aspect = Aspect::of_property(k);
        let after = match (aspect, v.as_deref().map(str::trim).filter(|t| !t.is_empty())) {
            (Some(a), Some(t)) => Some(pending.store(doc, ids, a, t, &mut ops)?),
            (Some(_), None) => None,
            (None, _) => value(doc, k, v.as_deref())?,
        };
        let Some(a) = aspect else {
            // Design 072 §5.3: the title, drawing number, customer and status are the project's info.
            let f =
                <ecad_model::InfoField as ecad_model::Word>::parse(k).ok_or_else(|| CmdError::refused(format!("\"{k}\" is not a project field.")))?;
            let before = pending.project(doc).clone();
            let now = crate::pages::frames::with_info(&before, f, after.as_deref())?;
            if now != before {
                ops.push(Op::ModifyProject { before, after: now.clone() });
                pending.set_project(now);
            }
            continue;
        };
        // Design 072 §5.3: the project's own function and location are fields of the project.
        let old = designations::default_key(&doc.project, a);
        if let Some(key) = after.as_ref().filter(|key| old.as_ref() != Some(*key)) {
            let code = code_with(doc, &pending, a, key);
            let spots = proposals::spots_of_project(doc, a);
            let x = proposals::plan_change(doc, &proposed.ops, a, &spots, old.as_deref(), key, &code, &change.mode);
            absorb(&mut proposed, x);
        }
        let before = pending.project(doc).clone();
        let now = designations::with_default_key(&before, a, after.as_deref());
        if now != before {
            ops.push(Op::ModifyProject { before, after: now.clone() });
            pending.set_project(now);
        }
    }
    ops.extend(proposed.ops.iter().cloned());
    for (k, v) in &change.props {
        check_free_key(doc, k)?;
        let v = free_value(doc, k, v.as_deref())?;
        // Design 072 §5.3: the site and the cover's notes are the project's info.
        if let Some(f) = <ecad_model::InfoField as ecad_model::Word>::parse(k) {
            let before = pending.project(doc).clone();
            let now = crate::pages::frames::with_info(&before, f, v.as_deref())?;
            if now != before {
                ops.push(Op::ModifyProject { before, after: now.clone() });
                pending.set_project(now);
            }
            continue;
        }
        ops.extend(set(ObjectRef::Project, Some(&doc.display), k, v));
    }
    // Brief 026 (design §5.1): Provisional tags until Finalise, as `numbering` writes it.
    if let Some(on) = change.provisional {
        ops.extend(set(
            ObjectRef::Project,
            Some(&doc.display),
            tag::PROVISIONAL_SETTING,
            on.then(|| ecad_model::naming::words::flag_word(true).to_string()),
        ));
    }
    let mut settings = doc.settings.clone();
    if let Some(t) = &change.sheet {
        settings.sheet = Some(SheetSpec::parse(t)?);
    }
    if let Some(n) = change.page_number_step {
        let n = u32::try_from(n).ok().filter(|n| (1..=MAX_STEP).contains(n));
        settings.page_number_step =
            Some(n.ok_or_else(|| CmdError::refused(format!("`page_number_step` must be a whole number from 1 to {MAX_STEP}")))?);
    }
    if let Some(s) = change.reference_style {
        settings.reference_style = Some(s);
    }
    if let Some(w) = &change.contact_mirror {
        settings.contact_mirror =
            ecad_model::ContactMirror::parse(w).ok_or(CmdError::refused("`contact_mirror` must be \"auto\", \"below\", \"right\" or \"off\""))?;
    }
    tag_settings(&mut settings, "tag_padding", change.tag_padding, &change.tag_prefixes)?;
    // Brief 022 fix round: how label colours are written, and the label height.
    if let Some(w) = &change.colour_style {
        settings.wiring.colour_style =
            Some(<ecad_model::geometry::colour::ColourWords as ecad_model::naming::words::Word>::parse(w).ok_or(CmdError::refused(
                "`colour_style` must be \"code2\" (BN, BU, GN/YE), \"code3\" (BRN, BLU, GRN/YEL) or \"name\" (Brown, Blue, Green/Yellow)",
            ))?);
    }
    if let Some(h) = change.label_height {
        settings.wiring.label_height = Some(
            Some(h)
                .filter(|x| ecad_model::connectivity::wiring::LABEL_HEIGHTS.contains(x))
                .ok_or(CmdError::refused("`label_height` must be 0.5 to 10 (millimetres)"))?,
        );
    }
    if settings != doc.settings {
        ops.push(Op::SetProjectSettings { before: doc.settings.clone(), after: settings });
    }
    // Brief 033: the revision table's columns and the pending marker.
    if let Some(v) = &change.revision_columns {
        let t = match v.as_deref() {
            None => None,
            Some(t) if t.trim().is_empty() => None,
            Some(t) => {
                revisions::parse_columns(t)?;
                Some(t.trim().to_string())
            }
        };
        ops.extend(set(ObjectRef::Project, Some(&doc.display), revisions::COLUMNS_KEY, t));
    }
    if let Some(w) = &change.revision_pending {
        let w = Some(w.trim()).filter(|w| ["star", "draft", "off"].contains(w));
        let w = w.ok_or(CmdError::refused("`revision_pending` must be \"star\", \"draft\" or \"off\""))?;
        ops.extend(set(ObjectRef::Project, Some(&doc.display), revisions::PENDING_KEY, Some(w.to_string())));
    }
    // Brief 033 (design 033 §8.4): the tag style, and its rewrite of the stored tags.
    let (tag_ops, message) = tag_style_ops(doc, change.tag_dash, change.tag_case)?;
    ops.extend(tag_ops);
    Ok(PropsPlan { ops, proposed, message })
}

// ---------------------------------------------------------------- revisions

/// The refusal of a change to issued revision `r`.
fn issued_row(r: &Revision) -> CmdError {
    CmdError::refused(format!("Rev {} is issued and cannot change.", r.rev()))
}

/// The revision fields `revision` and `issuerevision` write to the pending row. Each `Some` is
/// given; `None` inside clears the field.
#[derive(Debug, Clone, Default)]
pub struct RevisionFields {
    /// Fields of [`FIELDS`] given, in that order, as typed.
    pub standard: Vec<(&'static str, Option<String>)>,
    /// The row the caller means; refused unless it is the pending one.
    pub n: Option<u64>,
    /// Custom fields by name (brief 033, design 033 §7.2), in order.
    pub custom: Vec<(String, Option<String>)>,
}

/// The ops writing `f` to the pending row. A new pending row gets the suggested rev with its
/// first field.
///
/// # Errors
/// A bad or used rev, a value refused, an issued or unknown row, or a field name issuing sets or
/// that is not a field name.
pub fn revision_ops(doc: &Document, f: &RevisionFields) -> Result<Vec<Op>, CmdError> {
    let n = revisions::pending_n(doc);
    let mut project = doc.project.clone();
    let mut given = false;
    for (field, v) in &f.standard {
        given = true;
        let v = match v.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
            Some(t) if *field == "rev" => {
                revisions::check_rev(doc, t)?;
                Some(t.to_string())
            }
            other => value(doc, field, other)?,
        };
        project = revisions::with_field(&project, n, field, v.as_deref())?;
    }
    // Brief 033 (design 033 §7.2): custom fields, `fields: {ecr: "ECR-114"}`, on the pending row
    // only; an issued row never changes.
    if let Some(m) = f.n {
        let m = u32::try_from(m).map_err(|_| CmdError::refused("`n` must be a revision row number"))?;
        if m != n {
            return Err(match revisions::list(doc).into_iter().find(|r| r.n == m) {
                Some(r) if r.issued.is_some() => issued_row(&r),
                _ => CmdError::refused(format!("There is no pending revision row {m}.")),
            });
        }
    }
    for (field, v) in &f.custom {
        if !FIELDS.contains(&field.as_str()) && !revisions::is_custom_field(field) {
            return Err(CmdError::refused(if field == revisions::ISSUED || field == revisions::RELEASED {
                format!("\"{field}\" is set by issuing.")
            } else {
                format!("\"{field}\" is not a field name. Use 1 to 20 letters, digits or _.")
            }));
        }
        let t = v.as_ref().map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
        let t = match t {
            Some(t) if field == "rev" => {
                revisions::check_rev(doc, &t)?;
                Some(t)
            }
            other => value(doc, field, other.as_deref())?,
        };
        given = true;
        project = revisions::with_field(&project, n, field, t.as_deref())?;
    }
    let row_exists = revisions::pending(doc).is_some();
    let rev_given = f.standard.iter().any(|(k, v)| *k == "rev" && v.is_some());
    if given && !row_exists && !rev_given && revisions::rows(&project).len() >= n as usize {
        project = revisions::with_field(&project, n, "rev", Some(&revisions::pending_rev(doc)))?;
    }
    Ok(revisions::modify(&doc.project, project).into_iter().collect())
}

/// What issuing the pending revision records (design 019 §7.2): the remaining field writes,
/// `rev.<n>.issued` and `NameVersion { "Rev <rev>", head }`, worked out before anything is written.
#[derive(Debug, Clone, PartialEq)]
pub struct IssuePlan {
    pub ops: Vec<Op>,
    pub rev: String,
    /// The pending row's number.
    pub n: u32,
    pub date: String,
    /// The transaction the version names: the one before the issue.
    pub head: u64,
}

/// Plan issuing the pending revision of `ledger` with the field writes `f` (as `issuerevision`).
/// `refusal` is the release line's ([`IssuePlan`] is made on a release line only); `today` gives
/// the date when the row has none.
///
/// # Errors
/// Off a release line, nothing done yet, Rev, Description or Drawn empty, or a rev used before:
/// in words, each with its way forward.
pub fn plan_issue(
    ledger: &Ledger, refusal: Option<String>, f: &RevisionFields, today: impl FnOnce() -> Option<String>,
) -> Result<IssuePlan, CmdError> {
    if let Some(r) = refusal {
        return Err(CmdError::refused(r));
    }
    let head = ledger.head();
    if head == 0 {
        return Err(CmdError::refused("Nothing to issue yet."));
    }
    let mut ops = revision_ops(ledger.document(), f)?;
    // The pending row as it will be after those writes.
    let doc = ledger.preview(&ops)?;
    let n = revisions::pending_n(&doc);
    let row = revisions::pending(&doc).unwrap_or_default();
    if revisions::NEEDED.iter().any(|f| row.get(f).trim().is_empty()) {
        return Err(CmdError::refused(revisions::FILL_FIRST));
    }
    let rev = row.rev().to_string();
    let name = revisions::version_name(&rev);
    if doc.versions.contains_key(&name) {
        return Err(CmdError::refused(format!("Rev {rev} was issued before; choose another.")));
    }
    let mut date = row.get("date").to_string();
    let mut project = doc.project.clone();
    if date.is_empty() {
        date = today().unwrap_or_default();
        if !date.is_empty() {
            project = revisions::with_field(&project, n, "date", Some(&date))?;
        }
    }
    project = revisions::with_field(&project, n, revisions::ISSUED, Some(&name))?;
    ops.extend(revisions::modify(&doc.project, project));
    ops.push(Op::NameVersion { name, at: head, description: None });
    Ok(IssuePlan { ops, rev, n, date, head })
}

/// The issued revision `rev` (any case), as `openrevision` opens it.
///
/// # Errors
/// Not issued.
pub fn issued(doc: &Document, rev: &str) -> Result<Revision, CmdError> {
    revisions::issued(doc)
        .into_iter()
        .find(|r| r.rev().eq_ignore_ascii_case(rev))
        .ok_or_else(|| CmdError::refused(format!("Rev {rev} has not been issued.")))
}

/// The rev whose issue the ops `undo` would undo (`undoissue`).
///
/// # Errors
/// The last step did not issue a revision.
pub fn undone_issue(undo: &[Op]) -> Result<String, CmdError> {
    revisions::issue_in(&revisions::inverse(undo)).map(|(rev, _)| rev).ok_or(CmdError::refused("The last step did not issue a revision; use Undo."))
}

// ---------------------------------------------------------------- page numbers (brief 074: from the engine)

/// The number a new page gets after the page shown as `before` (design 019 §6.1): its leading
/// digits plus the project's step, skipping numbers in use; with no digits or no page before, its
/// book position `position`.
pub fn default_number(doc: &Document, book: &Book, before: Option<PageId>, position: usize, reports: Option<&ReportModels>) -> String {
    default_number_from(doc, book, before, position, reports, None)
}

/// [`default_number`], or for the first page of a folder with a start number (brief 054, design
/// 054 §4.3) that start when it is free, else the next free after it. Leading zeros keep their
/// width (`01` gives `01`, `02`), from the start or from the page before.
pub fn default_number_from(
    doc: &Document, book: &Book, before: Option<PageId>, position: usize, reports: Option<&ReportModels>, start: Option<&str>,
) -> String {
    // Brief 033: every sheet's number, continuation sheets included, is in use.
    let used: BTreeSet<String> =
        book.order.iter().flat_map(|p| crate::pages::generated::sheet_numbers(doc, book, *p, reports)).map(|n| n.to_ascii_lowercase()).collect();
    let step = u64::from(doc.settings.page_number_step().max(1));
    let digits = before.map(|p| book.number(doc, p)).map(|n| n.chars().take_while(char::is_ascii_digit).collect::<String>());
    let from_start = start.and_then(|t| t.parse::<u64>().ok().map(|n| (n, t.len())));
    let (mut n, step, width) = match (from_start, digits.as_deref().and_then(|d| d.parse::<u64>().ok().map(|n| (n, d)))) {
        (Some((s, w)), _) => (s, step, w),
        (None, Some((s, d))) => (s.saturating_add(step), step, if d.len() > 1 && d.starts_with('0') { d.len() } else { 0 }),
        (None, None) => (position as u64, 1, 0),
    };
    for _ in 0..100_000 {
        let t = format!("{n:0width$}");
        if !used.contains(&t) && t.len() <= MAX_NUMBER_CHARS {
            return t;
        }
        n = n.saturating_add(step);
    }
    n.to_string()
}

/// Where a new page in `folder` goes in book order: the page before it, and its position.
pub fn new_page_place(doc: &Document, book: &Book, folder: Option<FolderId>) -> (Option<PageId>, usize) {
    let in_folder: Vec<PageId> =
        doc.page_order.iter().copied().filter(|p| book::folder_of(doc, *p).filter(|f| doc.folders.contains_key(f)) == folder).collect();
    if let Some(last) = in_folder.last() {
        let pos = book.position(*last).unwrap_or(0);
        return (Some(*last), pos + 1);
    }
    let Some(f) = folder else { return (None, 1) };
    // An empty folder: the page before the folder in book order.
    let mut before = None;
    for node in &book.nodes {
        match node {
            book::Node::Folder { id, .. } if *id == f => break,
            book::Node::Page { id, .. } => before = Some(*id),
            book::Node::Folder { .. } => {}
        }
    }
    let pos = before.and_then(|p| book.position(p)).unwrap_or(0) + 1;
    (before, pos)
}

/// Where a new page goes (brief 054, design 054 §4.2): right after `after` in its folder, else
/// last in `folder`. Its folder, its index in the page order, and (the page before it, its book
/// position).
///
/// # Errors
/// No such page to add after.
pub fn new_page_at(
    doc: &Document, book: &Book, folder: Option<FolderId>, after: Option<PageId>,
) -> Result<(Option<FolderId>, usize, (Option<PageId>, usize)), CmdError> {
    Ok(match after {
        Some(a) => {
            let i = doc.page_order.iter().position(|x| *x == a).ok_or(CmdError::refused("No such page to add after."))?;
            let f = book::folder_of(doc, a).filter(|f| doc.folders.contains_key(f));
            (f, i + 1, (Some(a), book.position(a).unwrap_or(0) + 1))
        }
        None => (folder, doc.page_order.len(), new_page_place(doc, book, folder)),
    })
}

/// The number a new page placed by [`new_page_at`] gets (design 054 §4.3): the first page of a
/// folder with a start number takes it; every other page the next after the page before it.
pub fn next_number(doc: &Document, book: &Book, folder: Option<FolderId>, place: (Option<PageId>, usize), reports: Option<&ReportModels>) -> String {
    let (before, position) = place;
    let first = before.is_none_or(|b| book::folder_of(doc, b).filter(|f| doc.folders.contains_key(f)) != folder);
    let start = folder.filter(|_| first).and_then(|f| crate::pages::page_types::start_of(doc, f));
    default_number_from(doc, book, before, position, reports, start.as_deref())
}

/// The page's properties as `pageprops` reports them.
pub fn page_json(doc: &Document, book: &Book, page: PageId, reports: Option<&ReportModels>) -> Value {
    let pg = doc.pages.get(&page);
    use crate::locations::designations::{Aspect, default_key, page_key};
    let prop = |k: &str| Aspect::of_property(k).and_then(|a| pg.and_then(|x| page_key(x, a)));
    let project = |k: &str| Aspect::of_property(k).and_then(|a| default_key(&doc.project, a));
    let folder = book::folder_of(doc, page).filter(|f| doc.folders.contains_key(f));
    json!({
        "page": page.hex(),
        "number": book.number(doc, page),
        "number_stored": pg.and_then(|x| x.number.as_ref()).map(ecad_model::PageNo::as_str),
        "position": book.position(page),
        "title": pg.map(|x| x.title.as_str()),
        // Brief 052: stored as entry keys of the project's list, shown as codes.
        "function": prop(FUNCTION_KEY).map(|v| crate::locations::designations::display(doc, FUNCTION_KEY, &v)),
        "location": prop(LOCATION_KEY).map(|v| crate::locations::designations::display(doc, LOCATION_KEY, &v)),
        "function_key": prop(FUNCTION_KEY),
        "location_key": prop(LOCATION_KEY),
        "function_shown": prop(FUNCTION_KEY).or_else(|| project(FUNCTION_KEY)).map(|v| crate::locations::designations::display(doc, FUNCTION_KEY, &v)),
        "location_shown": prop(LOCATION_KEY).or_else(|| project(LOCATION_KEY)).map(|v| crate::locations::designations::display(doc, LOCATION_KEY, &v)),
        "sheet": pg.and_then(|x| x.sheet).map(|s| s.text()),
        "sheet_shown": ecad_model::document::sheet::sheet_of(doc, page).summary(),
        "folder": folder.map(FolderId::hex),
        "folder_path": crate::releases::revisions::folder_path(doc, folder),
        "generated": pg.and_then(|x| x.kind.generated_kind()).map(ecad_model::Word::word),
        // Brief 054: the page's type, resolved (design 054 §2.3).
        "type": crate::styles::page::type_of(doc, page).text(),
        "sheets": crate::pages::generated::sheets_of(doc, page, reports),
        "sheet_numbers": crate::pages::generated::sheet_numbers(doc, book, page, reports),
    })
}

pub fn style_word(s: ReferenceStyle) -> &'static str {
    match s {
        ReferenceStyle::PageColumnRow => "page-column-row",
        ReferenceStyle::PageColumn => "page-column",
        ReferenceStyle::PageZone => "page-zone",
    }
}

/// Everything `projectprops` reports.
pub fn project_json(doc: &Document) -> Value {
    let mut fields = Map::new();
    for k in PROJECT_KEYS {
        // Brief 052: the function and location show their codes (design 072 §5.3: fields of the
        // project).
        let v = match crate::locations::designations::Aspect::of_property(k) {
            Some(a) => crate::locations::designations::default_key(&doc.project, a).map(|v| crate::locations::designations::display(doc, k, &v)),
            None => crate::pages::frames::project_text(doc, k),
        };
        fields.insert(k.into(), v.map_or(Value::Null, Value::String));
    }
    // The site and the cover's notes are written as free properties and stored as the project's
    // info (design 072 §5.3): reported with the free ones.
    let written_free = [ecad_model::InfoField::Site, ecad_model::InfoField::Notes];
    let info = written_free
        .into_iter()
        .filter_map(|f| doc.project.info.get(f).map(|v| (ecad_model::Word::word(f).to_string(), Value::String(v.to_string()))));
    let free: Map<String, Value> = doc
        .display
        .iter()
        .filter(|(k, _)| {
            !PROJECT_KEYS.contains(&k.as_str())
                && !k.starts_with(revisions::PREFIX)
                && !k.starts_with("numbering.")
                && !k.starts_with("tags.")
                && !k.starts_with("revisions.")
        })
        .map(|(k, v)| (k.clone(), Value::String(v.clone())))
        .chain(info)
        .collect();
    let revs: Vec<Value> = revisions::list(doc)
        .iter()
        .map(|r| {
            let mut m = Map::new();
            for (f, v) in FIELDS.iter().zip(r.fields.iter()) {
                m.insert((*f).into(), Value::String(v.clone()));
            }
            // Brief 033: custom fields.
            for (f, v) in &r.extra {
                m.insert(f.clone(), Value::String(v.clone()));
            }
            m.insert("issued".into(), r.issued.clone().map_or(Value::Null, Value::String));
            // Brief 027 (FP1): where it was released, when it was.
            if let Some(f) = &r.released {
                m.insert("released".into(), Value::String(f.clone()));
            }
            Value::Object(m)
        })
        .collect();
    let mut v = Value::Object(fields);
    v["props"] = Value::Object(free);
    v["sheet"] = json!(doc.settings.sheet().text());
    v["sheet_shown"] = json!(doc.settings.sheet().summary());
    v["page_number_step"] = json!(doc.settings.page_number_step());
    v["reference_style"] = json!(style_word(doc.settings.reference_style()));
    // Brief 025 (FP3): where contact tables go by default.
    v["contact_mirror"] = json!(doc.settings.contact_mirror.word());
    v["tag_padding"] = json!(crate::devices::padding(doc));
    // Brief 033: the tag style, the revision columns and the pending marker.
    let style = crate::defaults::print::tag_style(doc);
    v["tag_dash"] = json!(style.dash);
    v["tag_case"] = json!(ecad_model::Word::word(style.case));
    let cols = revisions::columns(doc);
    v["revision_columns"] = json!(cols.iter().map(|c| json!({"field": c.field, "header": c.header, "width": c.width})).collect::<Vec<_>>());
    v["revision_columns_stored"] = json!(doc.display.get(revisions::COLUMNS_KEY));
    v["revision_pending"] = json!(match revisions::PendingStyle::of(doc) {
        revisions::PendingStyle::Star => "star",
        revisions::PendingStyle::Draft => "draft",
        revisions::PendingStyle::Off => "off",
    });
    v["tag_prefixes"] = Value::Object(crate::devices::commands::devices::prefixes_json(doc));
    // Brief 022: the wiring settings the Project settings popout edits.
    v["colour_style"] = json!(ecad_model::naming::words::Word::word(doc.settings.wiring.colour_style()));
    v["label_height"] = json!(wire_label_height(doc));
    // Brief 026: Project settings → Numbering.
    v["provisional"] = json!(ecad_model::naming::numbering::provisional_on(doc));
    v["revisions"] = Value::Array(revs);
    v["pending_rev"] = json!(revisions::pending_rev(doc));
    v
}

/// The wire label cap height the project resolves to, millimetres (its chosen one, its standard's
/// or the built-in; brief 066).
pub fn wire_label_height(doc: &ecad_model::Document) -> f64 {
    crate::defaults::resolve(doc, &crate::defaults::text::WIRE_LABEL_HEIGHT, crate::defaults::Scope::project()).number()
}
