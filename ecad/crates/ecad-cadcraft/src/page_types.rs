//! Brief 054a (design 054 §4, §8.3, §9): folder start numbers, Renumber pages, deleting a
//! generated page, and the Checks rows of page types and folder starts.
//!
//! # Semantics
//! - A folder's **start number** is the project property `folder.<folder id>.start` (FP7): 1 to 6
//!   digits, leading zeros setting the width. Setting it renumbers nothing. The first page added
//!   to a folder with one takes it (`sheet_cmds::new_page_ops_at`).
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

use cadcraft_engine::{CommandSpec, Session};
use ecad_model::book::{self, Book, FOLDER_KEY, MAX_FOLDER_DEPTH, NUMBER_KEY};
use ecad_model::style::StyleLayer;
use ecad_model::{Document, FolderId, ObjectRef, Op, PageId, StyleRef};
use serde_json::{Value, json};

use crate::checks::{Check, Kind, Object};
use crate::commands::{err, id, with_project};
use crate::generated::{self, GenKind};
use crate::outputs::Outputs;
use crate::styles::page as ptype;

/// The commands this module adds (registered in [`crate::lanes`]).
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("folderprops", "Folder Properties", run_folderprops)
            .params("{folder: id, start?: \"100\"|null} (a folder's start number: 1 to 6 digits, leading zeros set the width; null clears; no start: read only)")
            .noundo(),
        CommandSpec::new("renumberpages", "Renumber Pages", run_renumberpages)
            .params("{folder?: id} (renumbers each folder from its start number, the rest from 1; with folder, that folder and its sub-folders only; one transaction)")
            .noundo(),
        CommandSpec::new("deletepage", "Delete Page", run_deletepage)
            .params("{page: id} or {pages: [id]} (generated pages only: a cover, a contents, a report…; one transaction)")
            .noundo(),
    ]
}

/// Most characters of a start number.
pub const MAX_START_DIGITS: usize = 6;
/// The refusal for a start number that is not 1–6 digits.
pub const BAD_START: &str = "A start number is 1 to 6 digits.";
/// The refusal for deleting a drawn page (design 054 §8.3, Q9).
pub const DRAWN_PAGE: &str = "Deleting a drawn page comes later; move its content first.";
const KEY_PREFIX: &str = "folder.";
const KEY_SUFFIX: &str = ".start";

/// The project property holding `folder`'s start number (FP7).
pub fn start_key(folder: FolderId) -> String {
    format!("{KEY_PREFIX}{}{KEY_SUFFIX}", folder.hex())
}

/// Whether `t` is a start number: 1 to 6 ASCII digits.
pub fn is_start(t: &str) -> bool {
    (1..=MAX_START_DIGITS).contains(&t.len()) && t.bytes().all(|b| b.is_ascii_digit())
}

/// `folder`'s start number, when it has a valid one.
pub fn start_of(doc: &Document, folder: FolderId) -> Option<String> {
    doc.properties.get(&start_key(folder)).map(|s| s.trim()).filter(|s| is_start(s)).map(str::to_string)
}

/// The folder a `folder.<id>.start` key names (whether or not it exists), or `None` for any
/// other key.
fn folder_of_key(key: &str) -> Option<FolderId> {
    let hex = key.strip_prefix(KEY_PREFIX)?.strip_suffix(KEY_SUFFIX)?;
    (hex.len() == 32).then(|| u128::from_str_radix(hex, 16).ok().map(FolderId::from)).flatten()
}

/// Whether `key` is a folder start key.
pub fn is_start_key(key: &str) -> bool {
    key.starts_with(KEY_PREFIX) && key.ends_with(KEY_SUFFIX)
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
        out.entry(run_of(doc, *p)).or_default().push((*p, generated::sheets(doc, *p, outputs).max(1)));
    }
    out
}

/// The numbers each run would give if nothing else were in the way: its span, from its start by
/// the step, a multi-sheet page taking one number per sheet.
fn spans(doc: &Document, runs: &Runs) -> BTreeMap<Option<FolderId>, BTreeSet<u64>> {
    let step = u64::from(doc.settings.page_number_step.max(1));
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
    let step = u64::from(doc.settings.page_number_step.max(1));
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
                let fits = fmt(n.saturating_add(k64.saturating_sub(1)), width).len() <= crate::sheet_cmds::MAX_NUMBER_CHARS;
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

fn folder_param(doc: &Document, params: &Value) -> Result<Option<FolderId>, String> {
    let f: Option<FolderId> = crate::commands::opt_id(params, "folder")?;
    match f {
        Some(f) if !doc.folders.contains_key(&f) => Err("no such folder".into()),
        f => Ok(f),
    }
}

fn run_folderprops(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let folder: FolderId = id(params, "folder").map_err(err)?;
    let start = match params.get("start") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(Value::String(t)) if t.trim().is_empty() => Some(None),
        Some(Value::String(t)) if is_start(t.trim()) => Some(Some(t.trim().to_string())),
        Some(_) => return Err(err(BAD_START)),
    };
    with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let key = start_key(folder);
        // A start key whose folder is gone (a hostile or merged file) may only be cleared.
        let gone = !doc.folders.contains_key(&folder) && doc.properties.contains_key(&key) && start == Some(None);
        let name = match doc.folders.get(&folder) {
            Some(f) => f.name.clone(),
            None if gone => String::from("(gone)"),
            None => return Err("no such folder".into()),
        };
        let before = doc.properties.get(&key).cloned();
        let Some(after) = start else {
            return Ok(json!({"folder": folder.hex(), "name": name, "start": before}));
        };
        if before == after {
            return Ok(json!({"folder": folder.hex(), "name": name, "start": after, "changed": false}));
        }
        let ops = vec![Op::SetProperty { object: ObjectRef::Project, key, before, after: after.clone() }];
        p.apply_edit(pg, "folderprops", "Folder start number", ops)?;
        let line = match &after {
            Some(n) => format!("Folder \"{name}\" starts at {n}. Press Renumber pages to number its pages from it."),
            None => format!("Folder \"{name}\" has no start number now."),
        };
        pg.echo(line.clone());
        Ok(json!({"folder": folder.hex(), "name": name, "start": after, "changed": true, "message": line}))
    })
}

fn run_renumberpages(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    with_project(s, |p, pg, _| {
        let scope = folder_param(p.ledger.document(), params)?;
        if generated::has_bom_page(p.ledger.document()) {
            p.outputs();
        }
        let doc = p.ledger.document();
        let outputs = p.outputs.as_ref();
        let plan = renumber(doc, outputs, scope);
        let overlaps: Vec<String> = overlaps(doc, outputs).into_iter().map(|(a, b, n)| overlap_words(doc, a, b, n)).collect();
        if plan.is_empty() {
            let line = "Page numbers already in order.".to_string();
            pg.echo(line.clone());
            return Ok(json!({"renumbered": 0, "message": line, "overlaps": overlaps}));
        }
        let ops: Vec<Op> = plan
            .iter()
            .map(|(page, n)| Op::SetProperty {
                object: ObjectRef::Page(*page),
                key: NUMBER_KEY.into(),
                before: doc.pages.get(page).and_then(|x| x.properties.get(NUMBER_KEY)).cloned(),
                after: Some(n.clone()),
            })
            .collect();
        let count = plan.len();
        let pages: Vec<Value> = plan.iter().map(|(pg, n)| json!({"page": pg.hex(), "number": n})).collect();
        p.apply_edit(pg, "renumberpages", "Renumber pages", ops)?;
        let line = format!("Renumbered {count} page{}.", if count == 1 { "" } else { "s" });
        pg.echo(line.clone());
        for o in &overlaps {
            pg.echo(o.clone());
        }
        Ok(json!({"renumbered": count, "pages": pages, "message": line, "overlaps": overlaps}))
    })
}

fn run_deletepage(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let ids: Vec<PageId> = if params.get("pages").is_some() {
        crate::commands::id_list(params, "pages", 1_000).map_err(err)?
    } else {
        vec![id(params, "page").map_err(err)?]
    };
    if ids.is_empty() {
        return Err(err("Give the page to delete."));
    }
    with_project(s, |p, pg, active| {
        let doc = p.ledger.document();
        let set: BTreeSet<PageId> = ids.iter().copied().collect();
        for page in &set {
            let x = doc.pages.get(page).ok_or("no such page")?;
            if generated::kind_of(doc, *page).is_none() || doc.representations_on(*page).next().is_some() {
                let _ = x;
                return Err(DRAWN_PAGE.into());
            }
        }
        let remaining: Vec<PageId> = doc.page_order.iter().copied().filter(|p| !set.contains(p)).collect();
        if remaining.is_empty() {
            return Err("A project keeps at least one page.".into());
        }
        // Remove from the back so each index is the page's place when its op is applied (and undo,
        // reversed, puts them back in order).
        let mut placed: Vec<(usize, PageId)> = doc.page_order.iter().enumerate().filter(|(_, p)| set.contains(p)).map(|(i, p)| (i, *p)).collect();
        placed.sort_by_key(|p| std::cmp::Reverse(p.0));
        let mut ops = Vec::new();
        let mut words = Vec::new();
        let book = Book::of(doc);
        for (index, page) in &placed {
            let Some(x) = doc.pages.get(page) else { continue };
            words.push(format!("{} ({})", x.title, book.number(doc, *page)));
            ops.push(Op::RemovePage { id: *page, page: x.clone(), index: *index });
        }
        words.reverse();
        // The page shown moves to the page before it when it goes (design 016 §4.3).
        if set.contains(&active) {
            let at = doc.page_order.iter().position(|p| *p == active).unwrap_or(0);
            let earlier = doc.page_order.get(..at).unwrap_or_default().iter().rev().find(|p| !set.contains(p)).copied();
            if let Some(to) = earlier.or(remaining.first().copied()) {
                p.open_page(pg, to)?;
            }
        }
        p.apply_edit(pg, "deletepage", "Delete page", ops)?;
        let line = format!("Removed {}. Press u to bring it back.", words.join(", "));
        pg.echo(line.clone());
        Ok(json!({"deleted": ids.iter().map(|p| p.hex()).collect::<Vec<_>>(), "message": line}))
    })
}

/// The type `newpage` gives a page (design 054 §4.2): `type` read as a page type ID or name,
/// checked against `generated`. `Ok(None)`: no type and no generated kind (a page as before).
///
/// # Errors
/// In words: no such type, a generated-only type with no kind, a kind the type does not hold.
pub fn newpage_type(doc: &Document, ty: Option<&str>, kind: Option<GenKind>) -> Result<Option<StyleRef>, String> {
    let Some(t) = ty.map(str::trim).filter(|t| !t.is_empty()) else {
        return Ok(kind.map(|k| ptype::derived(Some(k))));
    };
    let id = crate::styles::find(doc, ptype::WORD, t)?;
    match kind {
        None if ptype::generated_only(&id) => Err(ptype::generated_refusal(doc, &id)),
        Some(k) if ptype::look_of(doc, &id).holds != ptype::Holds::of_kind(k) => Err(ptype::kind_refusal(k)),
        _ => Ok(Some(id)),
    }
}

/// A fix of a page type or folder start row (design 054 §9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageFix {
    /// **Use its usual type**: clear the page's own `page.type`, so it takes its derived type.
    UseDerived(PageId),
    /// **Reset**: clear field `field` (an index into the catalogue's fields) of a type's project
    /// record.
    Reset(TypeRef, u8),
    /// **Remove**: clear a folder start key (the folder may be gone).
    RemoveStart(FolderId),
}

/// A page type, as a fix can hold it: a built-in by its index, or an own type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeRef {
    Builtin(u8),
    Own(u128),
}

impl TypeRef {
    fn of(id: &StyleRef) -> Option<TypeRef> {
        match id {
            StyleRef::Own(n) => Some(TypeRef::Own(*n)),
            StyleRef::Builtin(w) => ptype::CATALOGUE.builtins.iter().position(|b| b.id == w).and_then(|i| u8::try_from(i).ok()).map(TypeRef::Builtin),
        }
    }

    fn id(self) -> String {
        match self {
            TypeRef::Own(n) => StyleRef::Own(n).text(),
            TypeRef::Builtin(i) => ptype::CATALOGUE.builtins.get(usize::from(i)).map_or_else(String::new, |b| b.id.to_string()),
        }
    }
}

impl PageFix {
    /// The button's words.
    pub fn label(self) -> &'static str {
        match self {
            PageFix::UseDerived(_) => "Use its usual type",
            PageFix::Reset(..) => "Reset",
            PageFix::RemoveStart(_) => "Remove",
        }
    }

    /// The command and parameters the fix runs (one transaction).
    pub fn command(self) -> (&'static str, Value) {
        match self {
            PageFix::UseDerived(p) => {
                ("setdefault", json!({"key": ecad_model::sheet::PAGE_TYPE_KEY, "level": "page", "page": p.hex(), "reset": true}))
            }
            PageFix::Reset(t, f) => {
                let key = ptype::CATALOGUE.fields.get(usize::from(f)).map_or("", |f| f.key);
                ("setstyle", json!({"catalogue": ptype::WORD, "id": t.id(), "fields": {key: null}}))
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

/// Whether `op` can change [`check_rows`].
pub fn touches_checks(op: &Op) -> bool {
    match op {
        Op::SetProperty { object: ObjectRef::Project, key, .. } => is_start_key(key),
        Op::SetProperty { object: ObjectRef::Page(_), key, .. } => {
            key == FOLDER_KEY || key == NUMBER_KEY || key == ecad_model::sheet::PAGE_TYPE_KEY || key == generated::KEY
        }
        Op::AddPage { .. } | Op::RemovePage { .. } | Op::AddFolder { .. } | Op::RemoveFolder { .. } | Op::ModifyFolder { .. } => true,
        Op::SetProjectSettings { before, after } => before.page_number_step != after.page_number_step,
        Op::SetStyle { catalogue, .. } => catalogue == ptype::WORD,
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
        let used = crate::styles::name(doc, ptype::WORD, &ptype::type_of(doc, p));
        let which = format!("Page {}: page type {value:?} not found; drawn as {used}", book::page_number(doc, p));
        let mut r = row(Kind::PageTypeUnknown, Some(p), which);
        r.fix = Some(crate::checks::Fix::Page(PageFix::UseDerived(p)));
        out.push(r);
    }
    // Type records holding values their fields do not take.
    for layer in [StyleLayer::Standard, StyleLayer::Project] {
        for (id, r) in crate::styles::records(doc, layer, ptype::WORD) {
            for (i, f) in ptype::CATALOGUE.fields.iter().enumerate() {
                let Some(v) = r.fields.get(f.key) else { continue };
                if crate::defaults::parse_value(f.kind, v).is_none() {
                    let name = crate::styles::name(doc, ptype::WORD, id);
                    let shown: String = v.chars().take(24).filter(|c| !c.is_control()).collect();
                    let mut c = row(Kind::PageTypeField, None, format!("Page type {name}: {} {shown:?} not read; its usual value is used", f.label));
                    // Reset writes the project's record; a standard's value is changed in Standards.
                    if layer == StyleLayer::Project {
                        c.fix = TypeRef::of(id).zip(u8::try_from(i).ok()).map(|(t, i)| crate::checks::Fix::Page(PageFix::Reset(t, i)));
                    }
                    out.push(c);
                }
            }
        }
    }
    // Folder start keys that name no folder or are not 1–6 digits.
    for (key, value) in doc.properties.range(KEY_PREFIX.to_string()..).take_while(|(k, _)| k.starts_with(KEY_PREFIX)) {
        if !key.ends_with(KEY_SUFFIX) {
            continue;
        }
        let named = folder_of_key(key);
        let folder = named.filter(|f| doc.folders.contains_key(f));
        let which = match folder.and_then(|f| doc.folders.get(&f)) {
            None => "A folder start number names a folder that is not in the project".to_string(),
            Some(f) if !is_start(value.trim()) => format!("Folder {}'s start number is not 1 to 6 digits", f.name),
            Some(_) => continue,
        };
        let mut r = row(Kind::FolderStartUnknown, None, which);
        r.fix = named.map(|f| crate::checks::Fix::Page(PageFix::RemoveStart(f)));
        out.push(r);
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
