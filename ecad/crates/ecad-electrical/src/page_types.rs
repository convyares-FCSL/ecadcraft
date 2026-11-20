//! Brief 054a (design 054 §4, §8.3, §9): folder start numbers, Renumber pages, deleting a
//! generated page, and the Checks rows of page types and folder starts.
//!
//! # Semantics
//! - A folder's **start number** is the project property `folder.<folder id>.start` (FP7): 1 to 6
//!   digits, leading zeros setting the width. Setting it renumbers nothing. The first page added
//!   to a folder with one takes it (`ecad_cadcraft::cmd::sheet::new_page_ops_at`).
//! - **Renumber pages** walks the book order. Each page belongs to the **run** of the nearest
//!   folder above it with a start number, else the book's run from `1`; each run numbers its pages
//!   in book order from its start by the project's step. A number given in this renumber, a
//!   continuation sheet's number (033), or a number in another run's span is skipped, never
//!   refused. One transaction; nothing written when nothing changes.
//! - `deletepage` removes generated pages only: they store nothing but their properties, so undo
//!   restores them exactly. A drawn page is refused before anything is written.
//!
//! # Update map (design 054 §10.2)
//! The Checks rows here ([`check_rows`]) are worked out again only after an op [`touches_checks`]
//! names: a `folder.*.start` property, a page's folder or type, a page or folder added or removed,
//! a page type record. Renumber runs on request (O(pages)).

use std::collections::{BTreeMap, BTreeSet};

use crate::CmdError;
use ecad_model::book::{self, Book, MAX_FOLDER_DEPTH};
use ecad_model::{Document, FolderId, ObjectRef, Op, PageId, StyleRef};
use serde_json::{Value, json};

use crate::checks::{Check, Kind, Object};
use crate::generated::{self, GenKind};
use crate::outputs::kept::Outputs;
use crate::styles::page as ptype;

/// Most characters of a page number.
pub const MAX_NUMBER_CHARS: usize = 12;

/// Most characters of a start number.
pub const MAX_START_DIGITS: usize = 6;
/// The refusal for a start number that is not 1–6 digits.
pub const BAD_START: &str = "A start number is 1 to 6 digits.";
/// The refusal for deleting a drawn page (design 054 §8.3, Q9).
pub const DRAWN_PAGE: &str = "Deleting a drawn page comes later; move its content first.";
/// Whether `t` is a start number: 1 to 6 ASCII digits.
pub fn is_start(t: &str) -> bool {
    t.len() <= MAX_START_DIGITS && ecad_model::words::is_digits(t)
}

/// `folder`'s start number (design 072 §5.2: `Folder.start`), when it has a valid one.
pub fn start_of(doc: &Document, folder: FolderId) -> Option<String> {
    doc.folders.get(&folder)?.start.as_ref().map(|s| s.as_str().trim()).filter(|s| is_start(s)).map(str::to_string)
}

/// The run a page belongs to: the nearest folder above it (its own first) with a start number,
/// else `None` (the book's run from 1).
fn run_of(doc: &Document, page: PageId) -> Option<FolderId> {
    let mut at = book::folder_of(doc, page).filter(|f| doc.folders.contains_key(f));
    for _ in 0..=MAX_FOLDER_DEPTH {
        let f = at?;
        if start_of(doc, f).is_some() {
            return Some(f);
        }
        at = doc.folders.get(&f).and_then(|x| x.parent);
    }
    None
}

/// Whether `page` is in `folder` or one of its sub-folders.
fn within(doc: &Document, page: PageId, folder: FolderId) -> bool {
    let mut at = book::folder_of(doc, page).filter(|f| doc.folders.contains_key(f));
    for _ in 0..=MAX_FOLDER_DEPTH {
        match at {
            Some(f) if f == folder => return true,
            Some(f) => at = doc.folders.get(&f).and_then(|x| x.parent),
            None => return false,
        }
    }
    false
}

/// A run's start: its value and width (`01` → 1, width 2).
fn start_value(doc: &Document, run: Option<FolderId>) -> (u64, usize) {
    run.and_then(|f| start_of(doc, f)).and_then(|s| s.parse::<u64>().ok().map(|n| (n, s.len()))).unwrap_or((1, 1))
}

fn fmt(n: u64, width: usize) -> String {
    format!("{n:0width$}")
}

/// The name of a run for words: the folder's name, or `The book`.
fn run_name(doc: &Document, run: Option<FolderId>) -> String {
    run.and_then(|f| doc.folders.get(&f)).map_or_else(|| "the book".to_string(), |f| format!("folder {}", f.name))
}

/// One run's pages in book order, with their sheet counts.
type Runs = BTreeMap<Option<FolderId>, Vec<(PageId, usize)>>;

fn runs(doc: &Document, book: &Book, outputs: Option<&Outputs>) -> Runs {
    let mut out: Runs = BTreeMap::new();
    for p in &book.order {
        out.entry(run_of(doc, *p)).or_default().push((*p, generated::sheets_of(doc, *p, outputs).max(1)));
    }
    out
}

/// The numbers each run would give if nothing else were in the way: its span, from its start by
/// the step, a multi-sheet page taking one number per sheet.
fn spans(doc: &Document, runs: &Runs) -> BTreeMap<Option<FolderId>, BTreeSet<u64>> {
    let step = u64::from(doc.settings.page_number_step().max(1));
    runs.iter()
        .map(|(run, pages)| {
            let (mut n, _) = start_value(doc, *run);
            let mut set = BTreeSet::new();
            for (_, k) in pages {
                for i in 0..*k as u64 {
                    set.insert(n.saturating_add(i));
                }
                n = n.saturating_add(*k as u64 - 1).saturating_add(step);
            }
            (*run, set)
        })
        .collect()
}

/// Where two runs' spans meet: the run that reaches into the other, the other, and the first
/// number they share. Each pair once, the earlier start first.
pub fn overlaps(doc: &Document, outputs: Option<&Outputs>) -> Vec<(Option<FolderId>, Option<FolderId>, u64)> {
    let book = Book::of(doc);
    let rs = runs(doc, &book, outputs);
    let sp = spans(doc, &rs);
    let mut out = Vec::new();
    let keys: Vec<Option<FolderId>> = sp.keys().copied().collect();
    for (i, a) in keys.iter().enumerate() {
        for b in keys.iter().skip(i + 1) {
            let (Some(sa), Some(sb)) = (sp.get(a), sp.get(b)) else { continue };
            if let Some(n) = sa.intersection(sb).next() {
                let (lo, hi) = if start_value(doc, *a).0 <= start_value(doc, *b).0 { (*a, *b) } else { (*b, *a) };
                out.push((lo, hi, *n));
            }
        }
    }
    out
}

/// The overlap row's words: `Folder Schematic's numbers run into Terminal's (300).`
pub fn overlap_words(doc: &Document, from: Option<FolderId>, into: Option<FolderId>, n: u64) -> String {
    let mut a = run_name(doc, from);
    if let Some(c) = a.get(..1) {
        a = format!("{}{}", c.to_uppercase(), a.get(1..).unwrap_or_default());
    }
    let b = into.and_then(|f| doc.folders.get(&f)).map_or_else(|| "the book's".to_string(), |f| format!("{}'s", f.name));
    let width = start_value(doc, into).1;
    format!("{a}'s numbers run into {b} ({}).", fmt(n, width))
}

/// What [`renumber`] would write: each page whose number changes, with its new number.
pub fn renumber(doc: &Document, outputs: Option<&Outputs>, scope: Option<FolderId>) -> Vec<(PageId, String)> {
    let book = Book::of(doc);
    let step = u64::from(doc.settings.page_number_step().max(1));
    let rs = runs(doc, &book, outputs);
    let sp = spans(doc, &rs);
    let in_scope = |p: PageId| scope.is_none_or(|f| within(doc, p, f));
    // Numbers in use that stay: every sheet of the pages outside the scope.
    let mut used: BTreeSet<String> = book
        .order
        .iter()
        .filter(|p| !in_scope(**p))
        .flat_map(|p| generated::sheet_numbers(doc, &book, *p, outputs))
        .map(|n| n.to_ascii_lowercase())
        .collect();
    let mut out = Vec::new();
    // Lowest start first; a run skips past the spans of the runs that start after it (Schematic
    // past 299 skips Terminal's 300s), never the other way round.
    let mut order: Vec<(&Option<FolderId>, &Vec<(PageId, usize)>)> = rs.iter().collect();
    order.sort_by_key(|(run, _)| start_value(doc, **run).0);
    for (run, pages) in order {
        let (mut n, width) = start_value(doc, *run);
        let mine = start_value(doc, *run).0;
        let others: BTreeSet<u64> =
            sp.iter().filter(|(r, _)| *r != run && start_value(doc, **r).0 > mine).flat_map(|(_, s)| s.iter().copied()).collect();
        for (p, k) in pages {
            if !in_scope(*p) {
                continue;
            }
            let k64 = *k as u64;
            // Bounded: each step passes at least one blocked number, and there are at most
            // (pages + their sheets) of them.
            let mut tries = 0usize;
            loop {
                let free = (0..k64).all(|i| {
                    let m = n.saturating_add(i);
                    !others.contains(&m) && !used.contains(&fmt(m, width).to_ascii_lowercase())
                });
                let fits = fmt(n.saturating_add(k64.saturating_sub(1)), width).len() <= MAX_NUMBER_CHARS;
                if (free && fits) || tries > 1_000_000 {
                    break;
                }
                n = n.saturating_add(1);
                tries += 1;
            }
            let number = fmt(n, width);
            for i in 0..k64 {
                used.insert(fmt(n.saturating_add(i), width).to_ascii_lowercase());
            }
            let stored = book::stored_number(doc, *p);
            if stored.is_none_or(|s| s != number) {
                out.push((*p, number));
            }
            n = n.saturating_add(k64 - 1).saturating_add(step);
        }
    }
    // Book order, so the transaction reads front to back.
    let pos: BTreeMap<PageId, usize> = book.order.iter().enumerate().map(|(i, p)| (*p, i)).collect();
    out.sort_by_key(|(p, _)| pos.get(p).copied().unwrap_or(usize::MAX));
    out
}

/// The type `newpage` gives a page (design 054 §4.2): `type` read as a page type ID or name,
/// checked against `generated`. `Ok(None)`: no type and no generated kind (a page as before).
///
/// # Errors
/// In words: no such type, a generated-only type with no kind, a kind the type does not hold.
pub fn newpage_type(doc: &Document, ty: Option<&str>, kind: Option<GenKind>) -> Result<Option<StyleRef>, CmdError> {
    let Some(t) = ty.map(str::trim).filter(|t| !t.is_empty()) else {
        return Ok(kind.map(|k| ptype::derived(Some(k))));
    };
    let id = crate::styles::find(doc, ecad_model::StyleCatalogue::Page, t)?;
    match kind {
        None if ptype::generated_only(&id) => Err(CmdError::refused(ptype::generated_refusal(doc, &id))),
        Some(k) if ptype::look_of(doc, &id).holds != ptype::holds_of_kind(k) => Err(CmdError::refused(ptype::kind_refusal(k))),
        _ => Ok(Some(id)),
    }
}

/// A fix of a page type or folder start row (design 054 §9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageFix {
    /// **Use its usual type**: clear the page's own `page.type`, so it takes its derived type.
    UseDerived(PageId),
    /// **Remove**: clear a folder start key (the folder may be gone).
    RemoveStart(FolderId),
}

impl PageFix {
    /// The button's words.
    pub fn label(self) -> &'static str {
        match self {
            PageFix::UseDerived(_) => "Use its usual type",
            PageFix::RemoveStart(_) => "Remove",
        }
    }

    /// The command and parameters the fix runs (one transaction).
    pub fn command(self) -> (&'static str, Value) {
        match self {
            PageFix::UseDerived(p) => {
                ("setdefault", json!({"key": ecad_model::sheet::PAGE_TYPE_KEY, "level": "page", "page": p.hex(), "reset": true}))
            }
            PageFix::RemoveStart(f) => ("folderprops", json!({"folder": f.hex(), "start": null})),
        }
    }

    /// The command as one line (the Checks panel runs it through the command path).
    pub fn line(self) -> String {
        let (c, p) = self.command();
        format!("{c} {p}")
    }
}

/// Whether `ops` change which pages have checks and references off (design 054 §3.4): a page's
/// type moved to or from such a type, such a page added or removed, a type record whose `checks`
/// changed. Looked up in `doc` (call it on the state before and after).
pub fn checks_changed(doc: &Document, ops: &[Op]) -> bool {
    ecad_model::effect::Effects::of(ops).iter().any(|e| checks_changed_by(doc, e))
}

/// One effect of [`checks_changed`], looked up in `doc`.
pub fn checks_changed_by(doc: &Document, effect: &ecad_model::effect::Effect<'_>) -> bool {
    use ecad_model::effect::Effect;
    let off = |page: &ecad_model::Page| !ecad_model::sheet::type_checks(doc, &ecad_model::sheet::page_type_of(doc, page));
    match effect {
        Effect::Page { before: Some(b), after: Some(a), index_before: None, index_after: None, .. } => {
            b.page_type != a.page_type && (off(b) || off(a))
        }
        // Added, removed, or both (a move).
        Effect::Page { before, after, .. } => before.is_some_and(off) || after.is_some_and(off),
        Effect::Style { catalogue: ecad_model::StyleCatalogue::Page, before, after, .. } => {
            let c = |r: Option<&ecad_model::StyleRecord>| r.and_then(|r| r.body.page()).and_then(|p| p.checks);
            c(*before) != c(*after)
        }
        _ => false,
    }
}

/// Whether `op` can change [`check_rows`].
pub fn touches_checks(op: &Op) -> bool {
    match op {
        Op::ModifyPage { before, after, .. } => before.folder != after.folder || before.number != after.number || before.page_type != after.page_type,
        Op::AddPage { .. } | Op::RemovePage { .. } | Op::AddFolder { .. } | Op::RemoveFolder { .. } | Op::ModifyFolder { .. } => true,
        Op::SetProjectSettings { before, after } => before.page_number_step != after.page_number_step,
        Op::SetStyle { catalogue, .. } => *catalogue == ecad_model::StyleCatalogue::Page,
        _ => false,
    }
}

/// The Checks rows of design 054 §9 that 054a adds: a page type that is not found, a type field
/// that is not read, a folder start key that is not read, runs that overlap.
pub fn check_rows(doc: &Document) -> Vec<Check> {
    let mut out = Vec::new();
    let row = |kind, page, which: String| Check { kind, page, at: None, object: Object::None, which, fix: None };
    // A `page.type` naming no type, through the style-choice index (explicit choices only).
    for (value, key, o) in doc.style_choices() {
        if key != ecad_model::sheet::PAGE_TYPE_KEY {
            continue;
        }
        let ObjectRef::Page(p) = o else { continue };
        if StyleRef::parse(value).is_some_and(|id| ecad_model::sheet::page_type_exists(doc, &id)) {
            continue;
        }
        let used = crate::styles::name(doc, ecad_model::StyleCatalogue::Page, &ptype::type_of(doc, p));
        let which = format!("Page {}: page type {value:?} not found; drawn as {used}", book::page_number(doc, p));
        let mut r = row(Kind::PageTypeUnknown, Some(p), which);
        r.fix = Some(crate::checks::Fix::Page(PageFix::UseDerived(p)));
        out.push(r);
    }
    // Design 072 §8: a type record holds typed values only; a value its field does not take
    // does not read, so there is nothing to report about type records here.
    // Folder start numbers that are not 1–6 digits (design 072 §5.2: a page number is stored).
    for (id, f) in &doc.folders {
        if f.start.as_ref().is_some_and(|s| !is_start(s.as_str().trim())) {
            let mut r = row(Kind::FolderStartUnknown, None, format!("Folder {}'s start number is not 1 to 6 digits", f.name));
            r.fix = Some(crate::checks::Fix::Page(PageFix::RemoveStart(*id)));
            out.push(r);
        }
    }
    // Runs that reach each other.
    for (a, b, n) in overlaps(doc, None) {
        let page = runs_first_page(doc, b);
        out.push(row(Kind::FolderRunsOverlap, page, overlap_words(doc, a, b, n)));
    }
    out
}

fn runs_first_page(doc: &Document, run: Option<FolderId>) -> Option<PageId> {
    Book::of(doc).order.iter().copied().find(|p| run_of(doc, *p) == run)
}

/// Every folder start number: folder → start, for the Book.
pub fn starts(doc: &Document) -> BTreeMap<FolderId, String> {
    doc.folder_order.iter().filter_map(|f| start_of(doc, *f).map(|s| (*f, s))).collect()
}
