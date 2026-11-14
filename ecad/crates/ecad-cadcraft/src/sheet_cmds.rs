//! The commands of brief 019 (design 019 §5–§7): page and project properties, folders,
//! revisions. Each write is one transaction committed through [`Project::apply_edit`]; the
//! no-field forms of `pageprops` and `projectprops`, and `openrevision`/`closerevision`, only
//! read.
//!
//! Page number, `=`/`+` overrides, sheet and folder are page properties; title, drawing number,
//! customer, status, defaults and revisions are project properties (decision FP1, FP4); folders
//! are typed (FP3); the sheet and page-number step are project settings (FP2).

use std::collections::BTreeSet;
use std::sync::Arc;

use cadcraft_engine::{CommandSpec, Session};
use ecad_model::book::{self, Book, FOLDER_KEY, FUNCTION_KEY, LOCATION_KEY, MAX_FOLDER_DEPTH, NUMBER_KEY, Node};
use ecad_model::sheet::{SHEET_KEY, SheetSpec};
use ecad_model::{Document, Folder, FolderId, ObjectRef, Op, Page, PageId, ReferenceStyle};
use serde_json::{Map, Value, json};

use crate::commands::{err, id, opt_id, with_any_page, with_project};
use crate::frame::{CUSTOMER, DRAWING_NUMBER, STATUS, TITLE};
use crate::project::Project;
use crate::revisions::{self, FIELDS};

/// A revision opened read-only (design 019 §7.3): the project as it was when the revision was
/// issued, built once on request. Session only; dropped by `closerevision`.
#[derive(Debug, Clone)]
pub struct RevisionView {
    pub rev: String,
    pub date: String,
    /// The transaction it names.
    pub at: u64,
    pub doc: Arc<Document>,
    /// The point bar's text when this is a point of the history (brief 020, `showpoint`), not an
    /// issued revision opened from Revisions.
    pub bar: Option<String>,
    /// Distinct for every view opened in this process (the panel re-reads its pages when it moves).
    pub id: u64,
}

/// A fresh [`RevisionView::id`].
pub fn next_view_id() -> u64 {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// The question a plain undo across an issue stops with (owner answer Q7).
pub fn issue_undo_question(rev: &str, date: &str) -> String {
    let on = if date.is_empty() { String::new() } else { format!(" on {date}") };
    format!("Rev {rev} was issued{on}. Undo the issue? This removes the release mark.")
}

/// The commands this module adds.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("pageprops", "Page Properties", run_pageprops)
            .params(
                "{page?: id (default: the active page), number?, title?, function?, location?, sheet?} (each null \
clears; no fields: the current values, read only)",
            )
            .noundo(),
        CommandSpec::new("projectprops", "Project Properties", run_projectprops)
            .params(
                "{title?, drawing_number?, customer?, status?, function?, location?, sheet?: \"A3\"|\"A2 12x8\"|…, \
page_number_step?: n, reference_style?: \"page-zone\"|\"page-column-row\"|\"page-column\", contact_mirror?: \"auto\"|\"below\"|\"right\"|\"off\", tag_padding?: 1..6, \
tag_prefixes?: {symbol: prefix|null}, colour_style?: \"iec\"|\"three-letter\", label_height?: mm, \
tag_dash?: bool, tag_case?: \"upper\"|\"typed\", revision_columns?: \"rev, date, ecr:ECR@20, description\"|null, \
revision_pending?: \"star\"|\"draft\"|\"off\", props?: {key: value|null}} (one transaction; no fields: everything, read only)",
            )
            .noundo(),
        CommandSpec::new("newfolder", "New Folder", run_newfolder).params("{name, parent?: id|null}").noundo(),
        CommandSpec::new("renamefolder", "Rename Folder", run_renamefolder).params("{folder: id, name}").noundo(),
        CommandSpec::new("movefolder", "Move Folder", run_movefolder)
            .params("{folder: id, parent: id|null, index?: n} (1 = first among its new siblings; default last)")
            .noundo(),
        CommandSpec::new("deletefolder", "Delete Folder", run_deletefolder)
            .params("{folder: id} (its pages and sub-folders move to its parent)")
            .noundo(),
        CommandSpec::new("revision", "Revision", run_revision)
            .params(
                "{rev?, date?, description?, drawn?, checked?, approved?, fields?: {field: text|null}, n?} (the pending revision's \
fields, custom fields by name; an issued row is refused)",
            )
            .noundo(),
        CommandSpec::new("issuerevision", "Issue Revision", run_issuerevision)
            .params(
                "{rev?, date?, description?, drawn?, checked?, approved?, provisional?: \"finalise\"|\"issue\"} (issues the \
pending revision: needs Rev, Description and Drawn; names the log point \"Rev <rev>\"; with provisional tags or wire numbers \
present, `provisional` says whether to finalise them first or issue them as they are)",
            )
            .noundo(),
        CommandSpec::new("openrevision", "Reopen Revision", run_openrevision)
            .params("{rev} (the project as issued, read only; records nothing)")
            .noundo(),
        CommandSpec::new("closerevision", "Back to Current", run_closerevision).params("{}").noundo(),
        CommandSpec::new("undoissue", "Undo Issue", run_undoissue)
            .params("{} (undoes the last step when it issued a revision: removes its release mark)")
            .noundo(),
    ]
}

// ---------------------------------------------------------------- parameters

/// A text field: absent `None`; `null` `Some(None)`; a string `Some(Some(_))`.
fn text_field(p: &Value, key: &str) -> Result<Option<Option<String>>, String> {
    match p.get(key) {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(Value::String(s)) => Ok(Some(Some(s.clone()))),
        Some(_) => Err(format!("`{key}` must be a string, or null to clear it")),
    }
}

/// A value under the 018 §6.1 rules (trimmed, 200 characters, no control characters; `function`
/// starts with `=`, `location` with `+`; empty clears).
fn value(doc: &Document, key: &str, v: Option<&str>) -> Result<Option<String>, String> {
    crate::devices::property_value(doc, key, v)
}

fn set(object: ObjectRef, props: Option<&ecad_model::Properties>, key: &str, after: Option<String>) -> Option<Op> {
    let before = props.and_then(|p| p.get(key)).cloned();
    (before != after).then(|| Op::SetProperty { object, key: key.to_string(), before, after })
}

// ---------------------------------------------------------------- page numbers

/// Most characters of a page number.
pub const MAX_NUMBER_CHARS: usize = 12;

/// Check a page number (design 019 §6.1) against every other page's shown number.
///
/// # Errors
/// In words.
pub fn check_number(
    doc: &Document, book: &Book, page: Option<PageId>, number: &str, outputs: Option<&crate::outputs::Outputs>,
) -> Result<(), String> {
    let n = number.chars().count();
    if !(1..=MAX_NUMBER_CHARS).contains(&n) || !number.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err("A page number is 1 to 12 letters, digits or _ (no spaces, `.`, `-` or `/`).".into());
    }
    for p in &book.order {
        if Some(*p) != page && book.number(doc, *p).eq_ignore_ascii_case(number) {
            let title = doc.pages.get(p).map(|x| x.title.as_str()).unwrap_or_default();
            return Err(format!("Page {number} already exists (\"{title}\")."));
        }
    }
    // Brief 033 (design 033 §5.3): a generated page's continuation numbers are taken too.
    if let Some((owner, i)) = crate::generated::continuation_of(doc, book, number, outputs)
        && Some(owner) != page
    {
        return Err(crate::generated::continuation_taken(doc, number, owner, i));
    }
    Ok(())
}

/// The number a new page gets after the page shown as `before` (design 019 §6.1): its leading
/// digits plus the project's step, skipping numbers in use; with no digits or no page before, its
/// book position `position`.
pub fn default_number(doc: &Document, book: &Book, before: Option<PageId>, position: usize, outputs: Option<&crate::outputs::Outputs>) -> String {
    // Brief 033: every sheet's number, continuation sheets included, is in use.
    let used: BTreeSet<String> =
        book.order.iter().flat_map(|p| crate::generated::sheet_numbers(doc, book, *p, outputs)).map(|n| n.to_ascii_lowercase()).collect();
    let step = u64::from(doc.settings.page_number_step.max(1));
    let digits = before.map(|p| book.number(doc, p)).map(|n| n.chars().take_while(char::is_ascii_digit).collect::<String>());
    let start = digits.as_deref().and_then(|d| d.parse::<u64>().ok());
    let (mut n, step) = match start {
        Some(s) => (s.saturating_add(step), step),
        None => (position as u64, 1),
    };
    for _ in 0..100_000 {
        let t = n.to_string();
        if !used.contains(&t) && t.len() <= MAX_NUMBER_CHARS {
            return t;
        }
        n = n.saturating_add(step);
    }
    n.to_string()
}

/// Where a new page in `folder` goes in book order: the page before it, and its position.
fn new_page_place(doc: &Document, book: &Book, folder: Option<FolderId>) -> (Option<PageId>, usize) {
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
            Node::Folder { id, .. } if *id == f => break,
            Node::Page { id, .. } => before = Some(*id),
            Node::Folder { .. } => {}
        }
    }
    let pos = before.and_then(|p| book.position(p)).unwrap_or(0) + 1;
    (before, pos)
}

// ---------------------------------------------------------------- folders

/// Most characters of a folder name.
pub const MAX_FOLDER_NAME: usize = 100;

fn folder_name(p: &Value) -> Result<String, String> {
    const BAD: &str = "A folder name is 1 to 100 characters, with no control characters.";
    let t = p.get("name").and_then(Value::as_str).ok_or(BAD)?.trim();
    let n = t.chars().count();
    if !(1..=MAX_FOLDER_NAME).contains(&n) || t.chars().any(char::is_control) {
        return Err(BAD.into());
    }
    Ok(t.to_string())
}

/// The folders whose parent is `parent`, in folder order.
fn siblings(doc: &Document, parent: Option<FolderId>) -> Vec<FolderId> {
    doc.folder_order.iter().copied().filter(|f| doc.folders.get(f).is_some_and(|x| x.parent == parent)).collect()
}

fn check_sibling_name(doc: &Document, parent: Option<FolderId>, name: &str, except: Option<FolderId>) -> Result<(), String> {
    let clash = siblings(doc, parent)
        .into_iter()
        .filter(|f| Some(*f) != except)
        .any(|f| doc.folders.get(&f).is_some_and(|x| x.name.eq_ignore_ascii_case(name)));
    if clash {
        return Err(format!("There is already a folder \"{name}\" here."));
    }
    Ok(())
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

/// `Schematic layouts / Spare circuits` (empty at the top level).
pub fn folder_path(doc: &Document, folder: Option<FolderId>) -> String {
    let mut names = Vec::new();
    let mut at = folder;
    while let Some(f) = at {
        let Some(x) = doc.folders.get(&f) else { break };
        names.push(x.name.clone());
        if names.len() > MAX_FOLDER_DEPTH {
            break;
        }
        at = x.parent;
    }
    names.reverse();
    names.join(" / ")
}

fn opt_folder(p: &Value, key: &str) -> Result<Option<FolderId>, String> {
    opt_id(p, key)
}

fn existing(doc: &Document, f: Option<FolderId>) -> Result<(), String> {
    match f {
        Some(f) if !doc.folders.contains_key(&f) => Err("no such folder".into()),
        _ => Ok(()),
    }
}

fn run_newfolder(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let name = folder_name(params).map_err(err)?;
    let parent = opt_folder(params, "parent").map_err(err)?;
    with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        existing(doc, parent)?;
        check_sibling_name(doc, parent, &name, None)?;
        if depth_of(doc, parent) + 1 > MAX_FOLDER_DEPTH {
            return Err(format!("Folders nest at most {MAX_FOLDER_DEPTH} deep."));
        }
        let id: FolderId = p.ids.fresh();
        let index = doc.folder_order.len();
        let op = Op::AddFolder { id, folder: Folder { name: name.clone(), parent }, index };
        p.apply_edit(pg, "newfolder", "New folder", vec![op])?;
        Ok(json!({"folder": id.hex(), "name": name}))
    })
}

fn run_renamefolder(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let folder: FolderId = id(params, "folder").map_err(err)?;
    let name = folder_name(params).map_err(err)?;
    with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let old = doc.folders.get(&folder).cloned().ok_or("no such folder")?;
        if old.name == name {
            return Ok(json!({"folder": folder.hex(), "name": name, "changed": false}));
        }
        check_sibling_name(doc, old.parent, &name, Some(folder))?;
        let after = Folder { name: name.clone(), parent: old.parent };
        p.apply_edit(pg, "renamefolder", "Rename folder", vec![Op::ModifyFolder { id: folder, before: old, after }])?;
        Ok(json!({"folder": folder.hex(), "name": name, "changed": true}))
    })
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

fn run_movefolder(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let folder: FolderId = id(params, "folder").map_err(err)?;
    if params.get("parent").is_none() {
        return Err(err("`parent` (hex id, or null for the top level) is required"));
    }
    let parent = opt_folder(params, "parent").map_err(err)?;
    let index = params.get("index").and_then(Value::as_u64).and_then(|i| usize::try_from(i).ok());
    with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let old = doc.folders.get(&folder).cloned().ok_or("no such folder")?;
        existing(doc, parent)?;
        // Onto itself or one of its own sub-folders.
        let mut at = parent;
        let mut steps = 0;
        while let Some(f) = at {
            if f == folder {
                return Err("A folder cannot go inside itself.".into());
            }
            at = doc.folders.get(&f).and_then(|x| x.parent);
            steps += 1;
            if steps > MAX_FOLDER_DEPTH + 1 {
                break;
            }
        }
        if parent != old.parent {
            check_sibling_name(doc, parent, &old.name, Some(folder))?;
        }
        if depth_of(doc, parent) + height(doc, folder) > MAX_FOLDER_DEPTH {
            return Err(format!("Folders nest at most {MAX_FOLDER_DEPTH} deep."));
        }
        let from = doc.folder_order.iter().position(|f| *f == folder).ok_or("no such folder")?;
        let to = folder_slot(doc, folder, parent, index);
        let after = Folder { name: old.name.clone(), parent };
        if to == from && parent == old.parent {
            return Ok(json!({"folder": folder.hex(), "changed": false}));
        }
        let ops = vec![Op::RemoveFolder { id: folder, folder: old, index: from }, Op::AddFolder { id: folder, folder: after, index: to }];
        p.apply_edit(pg, "movefolder", "Move folder", ops)?;
        Ok(json!({"folder": folder.hex(), "changed": true}))
    })
}

fn run_deletefolder(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let folder: FolderId = id(params, "folder").map_err(err)?;
    with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let old = doc.folders.get(&folder).cloned().ok_or("no such folder")?;
        let up = old.parent.map(FolderId::hex);
        let mut ops = Vec::new();
        let pages = doc.pages_in_folder(folder);
        for page in &pages {
            let props = doc.pages.get(page).map(|x| &x.properties);
            ops.extend(set(ObjectRef::Page(*page), props, FOLDER_KEY, up.clone()));
        }
        for sub in doc.subfolders(folder) {
            let before = doc.folders.get(&sub).cloned().ok_or("no such folder")?;
            if check_sibling_name(doc, old.parent, &before.name, Some(folder)).is_err() {
                return Err(format!("Rename sub-folder \"{}\" first: its new place already has a folder of that name.", before.name));
            }
            let after = Folder { name: before.name.clone(), parent: old.parent };
            ops.push(Op::ModifyFolder { id: sub, before, after });
        }
        let index = doc.folder_order.iter().position(|f| *f == folder).ok_or("no such folder")?;
        ops.push(Op::RemoveFolder { id: folder, folder: old.clone(), index });
        let to = match old.parent.and_then(|f| doc.folders.get(&f)) {
            Some(f) => format!("\"{}\"", f.name),
            None => "the top level".into(),
        };
        p.apply_edit(pg, "deletefolder", "Delete folder", ops)?;
        let n = pages.len();
        let line = format!("Folder \"{}\" deleted; its {n} page{} moved to {to}.", old.name, if n == 1 { "" } else { "s" });
        pg.echo(line.clone());
        Ok(json!({"folder": folder.hex(), "pages_moved": n, "message": line}))
    })
}

// ---------------------------------------------------------------- pages

/// The ops that move `page` into `folder` (None: the top level) at 1-based `index` among that
/// folder's pages (default last): its `folder` property, then a RemovePage + AddPage pair when its
/// place in the page order changes.
pub(crate) fn move_page_ops(doc: &Document, page: PageId, folder: Option<FolderId>, index: Option<usize>) -> Result<Vec<Op>, String> {
    existing(doc, folder)?;
    let old = doc.pages.get(&page).cloned().ok_or("no such page")?;
    let mut ops = Vec::new();
    let mut now = old.clone();
    let want = folder.map(FolderId::hex);
    if now.properties.get(FOLDER_KEY) != want.as_ref() {
        ops.extend(set(ObjectRef::Page(page), Some(&old.properties), FOLDER_KEY, want.clone()));
        match &want {
            Some(w) => now.properties.insert(FOLDER_KEY.into(), w.clone()),
            None => now.properties.remove(FOLDER_KEY),
        };
    }
    let from = doc.page_order.iter().position(|p| *p == page).ok_or("no such page")?;
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
        ops.push(Op::RemovePage { id: page, page: now.clone(), index: from });
        ops.push(Op::AddPage { id: page, page: now, index: to });
    }
    Ok(ops)
}

/// `movepage {page, folder, index?}` (design 019 §5.3).
pub(crate) fn run_movepage_folder(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let page: PageId = id(params, "page").map_err(err)?;
    let folder = opt_folder(params, "folder").map_err(err)?;
    let index = params.get("index").and_then(Value::as_u64).and_then(|i| usize::try_from(i).ok());
    with_project(s, |p, pg, _| {
        let ops = move_page_ops(p.ledger.document(), page, folder, index)?;
        if ops.is_empty() {
            return Ok(json!({"page": page.hex(), "changed": false}));
        }
        p.apply_edit(pg, "movepage", "Move page", ops)?;
        Ok(json!({"page": page.hex(), "folder": folder.map(FolderId::hex), "changed": true}))
    })
}

/// The ops of `newpage {title, number?, folder?}`, the new page's ID and number.
pub(crate) fn new_page_ops(
    p: &mut Project, title: String, number: Option<String>, folder: Option<FolderId>,
) -> Result<(PageId, String, Vec<Op>), String> {
    if crate::generated::has_bom_page(p.ledger.document()) {
        p.outputs();
    }
    let doc = p.ledger.document();
    let outputs = p.outputs.as_ref();
    existing(doc, folder)?;
    let book = Book::of(doc);
    let number = match number.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()) {
        Some(n) => {
            check_number(doc, &book, None, &n, outputs)?;
            n
        }
        None => {
            let (before, position) = new_page_place(doc, &book, folder);
            default_number(doc, &book, before, position, outputs)
        }
    };
    let mut page = Page::new(title);
    page.properties.insert(NUMBER_KEY.into(), number.clone());
    if let Some(f) = folder {
        page.properties.insert(FOLDER_KEY.into(), f.hex());
    }
    let index = doc.page_order.len();
    let id: PageId = p.ids.fresh();
    Ok((id, number, vec![Op::AddPage { id, page, index }]))
}

/// The page's properties as `pageprops` reports them.
pub fn page_json(doc: &Document, book: &Book, page: PageId, outputs: Option<&crate::outputs::Outputs>) -> Value {
    let pg = doc.pages.get(&page);
    let prop = |k: &str| pg.and_then(|x| x.properties.get(k)).cloned();
    let project = |k: &str| doc.properties.get(k).cloned();
    let folder = book::folder_of(doc, page).filter(|f| doc.folders.contains_key(f));
    json!({
        "page": page.hex(),
        "number": book.number(doc, page),
        "number_stored": prop(NUMBER_KEY),
        "position": book.position(page),
        "title": pg.map(|x| x.title.clone()),
        "function": prop(FUNCTION_KEY),
        "location": prop(LOCATION_KEY),
        "function_shown": prop(FUNCTION_KEY).or_else(|| project(FUNCTION_KEY)),
        "location_shown": prop(LOCATION_KEY).or_else(|| project(LOCATION_KEY)),
        "sheet": prop(SHEET_KEY),
        "sheet_shown": ecad_model::sheet::sheet_of(doc, page).summary(),
        "folder": folder.map(FolderId::hex),
        "folder_path": folder_path(doc, folder),
        "generated": prop(crate::generated::KEY),
        "sheets": crate::generated::sheets(doc, page, outputs),
        "sheet_numbers": crate::generated::sheet_numbers(doc, book, page, outputs),
    })
}

fn run_pageprops(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    if params.get(crate::generated::KEY).is_some() {
        return Err(err(crate::generated::KIND_FIXED));
    }
    // Brief 031 (§10.6): a page's `text.` keys are written by `showtext` only, also when asked for
    // as `props` or as a field of their own.
    let text_key = |k: &String| crate::text_style::is_reserved_key(k);
    if params.as_object().is_some_and(|o| o.keys().any(text_key))
        || params.get("props").and_then(Value::as_object).is_some_and(|o| o.keys().any(text_key))
    {
        return Err(err(crate::text_style::RESERVED_KEY));
    }
    let page = opt_id::<PageId>(params, "page").map_err(err)?;
    let number = text_field(params, "number").map_err(err)?;
    let title = match params.get("title") {
        None => None,
        Some(_) => Some(crate::commands::page_title(params).map_err(err)?),
    };
    let function = text_field(params, FUNCTION_KEY).map_err(err)?;
    let location = text_field(params, LOCATION_KEY).map_err(err)?;
    let sheet = text_field(params, SHEET_KEY).map_err(err)?;
    let write = number.is_some() || title.is_some() || function.is_some() || location.is_some() || sheet.is_some();
    if !write {
        return with_any_page(s, |p, _| {
            if crate::generated::has_bom_page(p.ledger.document()) {
                p.outputs();
            }
            let doc = p.ledger.document();
            let page = page.or(p.active_page()).ok_or("`page` (hex id) is required: no page is shown")?;
            if !doc.pages.contains_key(&page) {
                return Err("no such page".into());
            }
            Ok(page_json(doc, &Book::of(doc), page, p.outputs.as_ref()))
        });
    }
    with_project(s, |p, pg, active| {
        let page = page.unwrap_or(active);
        if crate::generated::has_bom_page(p.ledger.document()) {
            p.outputs();
        }
        let doc = p.ledger.document();
        let outputs = p.outputs.as_ref();
        let old = doc.pages.get(&page).cloned().ok_or("no such page")?;
        let book = Book::of(doc);
        let object = ObjectRef::Page(page);
        let mut ops = Vec::new();
        let mut now = old.clone();
        let mut put = |key: &str, after: Option<String>, ops: &mut Vec<Op>| {
            if let Some(op) = set(object, Some(&now.properties), key, after.clone()) {
                ops.push(op);
                match after {
                    Some(v) => now.properties.insert(key.to_string(), v),
                    None => now.properties.remove(key),
                };
            }
        };
        if let Some(n) = number {
            let n = n.map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
            if let Some(t) = &n {
                check_number(doc, &book, Some(page), t, outputs)?;
            }
            put(NUMBER_KEY, n, &mut ops);
        }
        for (key, v) in [(FUNCTION_KEY, function), (LOCATION_KEY, location)] {
            if let Some(v) = v {
                put(key, value(doc, key, v.as_deref())?, &mut ops);
            }
        }
        if let Some(v) = sheet {
            let v = v.map(|t| t.trim().to_string()).filter(|t| !t.is_empty() && !t.eq_ignore_ascii_case("project"));
            let spec = match v {
                Some(t) => Some(SheetSpec::parse(&t)?.text()),
                None => None,
            };
            put(SHEET_KEY, spec, &mut ops);
        }
        if let Some(t) = title
            && t != now.title
        {
            let index = doc.page_order.iter().position(|x| *x == page).ok_or("no such page")?;
            let mut renamed = now.clone();
            renamed.title = t;
            ops.push(Op::RemovePage { id: page, page: now.clone(), index });
            ops.push(Op::AddPage { id: page, page: renamed, index });
        }
        if ops.is_empty() {
            let mut v = page_json(doc, &book, page, outputs);
            v["changed"] = json!(false);
            return Ok(v);
        }
        p.apply_edit(pg, "pageprops", "Page properties", ops)?;
        let doc = p.ledger.document();
        let mut v = page_json(doc, &Book::of(doc), page, p.outputs.as_ref());
        v["changed"] = json!(true);
        Ok(v)
    })
}

// ---------------------------------------------------------------- project

/// The project property keys the dialog and the title block use, in order.
pub const PROJECT_KEYS: [&str; 6] = [TITLE, DRAWING_NUMBER, CUSTOMER, STATUS, FUNCTION_KEY, LOCATION_KEY];
/// Status suggestions (owner answer Q9: any text is allowed).
pub const STATUS_SUGGESTIONS: [&str; 4] = ["Draft", "For review", "For construction", "As built"];
/// Most characters of a free property key.
pub const MAX_KEY_CHARS: usize = 40;
/// The page-number step's range.
pub const MAX_STEP: u32 = 1000;

/// Whether `key` is a free property (not one of [`PROJECT_KEYS`], a revision or a folder key).
fn check_free_key(doc: &Document, key: &str) -> Result<(), String> {
    // Brief 031 (§10.6): `text.` keys are written by the text commands only.
    if crate::text_style::is_reserved_key(key) {
        return Err(crate::text_style::RESERVED_KEY.into());
    }
    if let Some(rest) = key.strip_prefix(revisions::PREFIX) {
        let n = rest.split('.').next().and_then(|n| n.parse::<u32>().ok());
        if let Some(r) = n.and_then(|n| revisions::list(doc).into_iter().find(|r| r.n == n)).filter(|r| r.issued.is_some()) {
            return Err(format!("Rev {} is issued and cannot change.", r.rev()));
        }
        return Err("Revision fields are changed with `revision` (Revisions…).".into());
    }
    if PROJECT_KEYS.contains(&key) {
        return Err(format!("\"{key}\" is set above."));
    }
    if key.starts_with("folder") {
        return Err(format!("\"{key}\" is reserved."));
    }
    // Brief 026: the numbering keys are written by `numbering`, Renumber and Finalise.
    if key.starts_with("numbering.") {
        return Err(format!("\"{key}\" is reserved (Project settings → Numbering)."));
    }
    // Brief 036: the registry's terminal defaults are project properties of their entry's key.
    if key.starts_with("terminal.") && crate::defaults::find(key).is_some() {
        return Ok(());
    }
    let n = key.chars().count();
    if !(1..=MAX_KEY_CHARS).contains(&n) || !key.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
        return Err("A property name is 1 to 40 characters a-z, 0-9 or _.".into());
    }
    Ok(())
}

fn style_word(s: ReferenceStyle) -> &'static str {
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
        fields.insert(k.into(), doc.properties.get(k).map_or(Value::Null, |v| Value::String(v.clone())));
    }
    let free: Map<String, Value> = doc
        .properties
        .iter()
        .filter(|(k, _)| {
            !PROJECT_KEYS.contains(&k.as_str())
                && !k.starts_with(revisions::PREFIX)
                && !k.starts_with("numbering.")
                && !k.starts_with("tags.")
                && !k.starts_with("revisions.")
        })
        .map(|(k, v)| (k.clone(), Value::String(v.clone())))
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
    v["sheet"] = json!(doc.settings.sheet.text());
    v["sheet_shown"] = json!(doc.settings.sheet.summary());
    v["page_number_step"] = json!(doc.settings.page_number_step);
    v["reference_style"] = json!(style_word(doc.settings.reference_style));
    // Brief 025 (FP3): where contact tables go by default.
    v["contact_mirror"] = json!(doc.settings.contact_mirror.word());
    v["tag_padding"] = json!(crate::devices::padding(doc));
    // Brief 033: the tag style, the revision columns and the pending marker.
    let style = crate::defaults::print::tag_style(doc);
    v["tag_dash"] = json!(style.dash);
    v["tag_case"] = json!(if style.case == ecad_model::tag::Case::Typed { "typed" } else { "upper" });
    let cols = revisions::columns(doc);
    v["revision_columns"] = json!(cols.iter().map(|c| json!({"field": c.field, "header": c.header, "width": c.width})).collect::<Vec<_>>());
    v["revision_columns_stored"] = json!(doc.properties.get(revisions::COLUMNS_KEY));
    v["revision_pending"] = json!(match revisions::PendingStyle::of(doc) {
        revisions::PendingStyle::Star => "star",
        revisions::PendingStyle::Draft => "draft",
        revisions::PendingStyle::Off => "off",
    });
    v["tag_prefixes"] = Value::Object(crate::device_cmds::prefixes_json(doc));
    // Brief 022: the wiring settings the Project settings popout edits.
    v["colour_style"] = json!(doc.settings.wiring.colour_style.name());
    v["label_height"] = json!(doc.settings.wiring.label_height());
    // Brief 026: Project settings → Numbering.
    v["provisional"] = json!(ecad_model::numbering::provisional_on(doc));
    v["revisions"] = Value::Array(revs);
    v["pending_rev"] = json!(revisions::pending_rev(doc));
    v
}

/// The ops of `tag_dash` and `tag_case` in `projectprops` (design 033 §8.4): the settings, and
/// one `SetProperty(tag)` per component or cable tag whose stored text changes (a new dash, or
/// back to upper case). The echo in words. Tags are identities by `tag::key`, which ignores the
/// dash and the case, so no index changes key; each rewrite is an ordinary tag op that every
/// tag-showing view already maps. Merging two branches with different dash settings: the merged
/// `tags.dash` is the branch that changed it, its rewrites come with it, and tags the other branch
/// added keep their own form until the style is set again.
///
/// # Errors
/// A bad value, or (never expected: the key ignores what changes) two different tags rewritten to
/// the same text.
fn tag_style_ops(doc: &Document, params: &Value) -> Result<(Vec<Op>, Option<String>), String> {
    use ecad_model::tag::{self, CASE_KEY, Case, DASH_KEY, TAG_KEY};
    let dash = match params.get("tag_dash").filter(|v| !v.is_null()) {
        Some(v) => Some(v.as_bool().ok_or("`tag_dash` must be true or false")?),
        None => None,
    };
    let case = match params.get("tag_case").filter(|v| !v.is_null()) {
        Some(v) => Some(match v.as_str().map(str::trim) {
            Some("upper") => Case::Upper,
            Some("typed") => Case::Typed,
            _ => return Err("`tag_case` must be \"upper\" or \"typed\"".into()),
        }),
        None => None,
    };
    let before = crate::defaults::print::tag_style(doc);
    let after = tag::Style { dash: dash.unwrap_or(before.dash), case: case.unwrap_or(before.case), ..before };
    let mut ops = Vec::new();
    if let Some(d) = dash {
        ops.extend(set(ObjectRef::Project, Some(&doc.properties), DASH_KEY, Some(if d { "yes" } else { "no" }.to_string())));
    }
    if let Some(c) = case {
        ops.extend(set(ObjectRef::Project, Some(&doc.properties), CASE_KEY, Some(if c == Case::Typed { "typed" } else { "upper" }.to_string())));
    }
    let rewrite = after.dash != before.dash || (after.case == Case::Upper && before.case == Case::Typed);
    let mut n = 0usize;
    if rewrite {
        let mut seen: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        let objects = doc
            .components
            .iter()
            .filter_map(|(id, c)| c.properties.get(TAG_KEY).map(|t| (ObjectRef::Component(*id), t)))
            .chain(doc.cables.iter().filter_map(|(id, c)| c.properties.get(TAG_KEY).map(|t| (ObjectRef::Cable(*id), t))));
        for (object, old) in objects {
            let new = tag::restyle(old, after);
            if let Some(k) = seen.get(&new)
                && *k != tag::key(old)
            {
                return Err(format!("Changing the tag style would make two tags {new}; nothing was changed."));
            }
            seen.insert(new.clone(), tag::key(old));
            if new != *old {
                ops.push(Op::SetProperty { object, key: TAG_KEY.into(), before: Some(old.clone()), after: Some(new) });
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

fn run_projectprops(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let o = params.as_object().cloned().unwrap_or_default();
    if o.is_empty() {
        return with_any_page(s, |p, _| Ok(project_json(p.ledger.document())));
    }
    with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let mut ops = Vec::new();
        for k in PROJECT_KEYS {
            if let Some(v) = text_field(params, k)? {
                ops.extend(set(ObjectRef::Project, Some(&doc.properties), k, value(doc, k, v.as_deref())?));
            }
        }
        if let Some(props) = params.get("props") {
            let m = props.as_object().ok_or("`props` must be an object of key: value|null")?;
            for (k, v) in m {
                // Brief 050a (FP4 ruling): an old 031 text key may be cleared, which removes its
                // `ignored-old-setting` row; it is never written.
                if v.is_null() && crate::styles::checks::is_old_key(k) {
                    ops.extend(set(ObjectRef::Project, Some(&doc.properties), k, None));
                    continue;
                }
                check_free_key(doc, k)?;
                let v = match v {
                    Value::Null => None,
                    // Brief 030 (FP3, FP6): strip accessory defaults are checked like the strip's.
                    Value::String(t) => match crate::accessories::project_rule(doc, k, Some(t)) {
                        Some(r) => r?,
                        // Brief 036: the registry's terminal defaults (`""` on the separator is none).
                        None => match crate::defaults::terminals::setprop_value(k, Some(t)) {
                            Some(r) => r?,
                            None => value(doc, k, Some(t))?,
                        },
                    },
                    _ => return Err(format!("`props.{k}` must be a string, or null to clear it")),
                };
                ops.extend(set(ObjectRef::Project, Some(&doc.properties), k, v));
            }
        }
        // Brief 026 (design §5.1): Provisional tags until Finalise, as `numbering` writes it.
        if let Some(v) = params.get("provisional").filter(|v| !v.is_null()) {
            let on = v.as_bool().ok_or("`provisional` must be true or false")?;
            ops.extend(set(ObjectRef::Project, Some(&doc.properties), ecad_model::tag::PROVISIONAL_SETTING, on.then(|| "yes".to_string())));
        }
        let mut settings = doc.settings.clone();
        if let Some(v) = params.get("sheet").filter(|v| !v.is_null()) {
            let t = v.as_str().ok_or("`sheet` must be text such as \"A3\" or \"A2 12x8\"")?;
            settings.sheet = SheetSpec::parse(t)?;
        }
        if let Some(v) = params.get("page_number_step").filter(|v| !v.is_null()) {
            let n = v.as_u64().and_then(|n| u32::try_from(n).ok()).filter(|n| (1..=MAX_STEP).contains(n));
            settings.page_number_step = n.ok_or(format!("`page_number_step` must be a whole number from 1 to {MAX_STEP}"))?;
        }
        if let Some(v) = params.get("reference_style").filter(|v| !v.is_null()) {
            settings.reference_style = match v.as_str() {
                Some("page-zone") => ReferenceStyle::PageZone,
                Some("page-column-row") => ReferenceStyle::PageColumnRow,
                Some("page-column") => ReferenceStyle::PageColumn,
                _ => return Err("`reference_style` must be \"page-zone\", \"page-column-row\" or \"page-column\"".into()),
            };
        }
        if let Some(v) = params.get("contact_mirror").filter(|v| !v.is_null()) {
            settings.contact_mirror =
                v.as_str().and_then(ecad_model::ContactMirror::parse).ok_or("`contact_mirror` must be \"auto\", \"below\", \"right\" or \"off\"")?;
        }
        let (padding, prefixes) = crate::device_cmds::tag_settings_params(params, "tag_padding", "tag_prefixes")?;
        crate::device_cmds::apply_tag_settings(&mut settings, padding, prefixes);
        // Brief 022 fix round: how label colours are written, and the label height.
        if let Some(v) = params.get("colour_style").filter(|v| !v.is_null()) {
            settings.wiring.colour_style = v
                .as_str()
                .and_then(ecad_model::wiring::ColourStyle::parse)
                .ok_or("`colour_style` must be \"iec\" (BN, BU, GNYE) or \"three-letter\" (BRN, BLU, GRN/YEL)")?;
        }
        if let Some(v) = params.get("label_height").filter(|v| !v.is_null()) {
            settings.wiring.label_height =
                v.as_f64().filter(|x| ecad_model::wiring::LABEL_HEIGHTS.contains(x)).ok_or("`label_height` must be 0.5 to 10 (millimetres)")?;
        }
        if settings != doc.settings {
            ops.push(Op::SetProjectSettings { before: doc.settings.clone(), after: settings });
        }
        // Brief 033: the revision table's columns and the pending marker.
        if let Some(v) = params.get("revision_columns") {
            let t = match v {
                Value::Null => None,
                Value::String(t) if t.trim().is_empty() => None,
                Value::String(t) => {
                    revisions::parse_columns(t)?;
                    Some(t.trim().to_string())
                }
                _ => return Err("`revision_columns` must be text such as \"rev, date, ecr:ECR, description\", or null".into()),
            };
            ops.extend(set(ObjectRef::Project, Some(&doc.properties), revisions::COLUMNS_KEY, t));
        }
        if let Some(v) = params.get("revision_pending").filter(|v| !v.is_null()) {
            let w = v.as_str().map(str::trim).filter(|w| ["star", "draft", "off"].contains(w));
            let w = w.ok_or("`revision_pending` must be \"star\", \"draft\" or \"off\"")?;
            ops.extend(set(ObjectRef::Project, Some(&doc.properties), revisions::PENDING_KEY, Some(w.to_string())));
        }
        // Brief 033 (design 033 §8.4): the tag style, and its rewrite of the stored tags.
        let mut message = None;
        let (tag_ops, words) = tag_style_ops(doc, params)?;
        ops.extend(tag_ops);
        if let Some(w) = words {
            message = Some(w);
        }
        if ops.is_empty() {
            let mut v = project_json(doc);
            v["changed"] = json!(false);
            return Ok(v);
        }
        p.apply_edit(pg, "projectprops", "Project properties", ops)?;
        let doc = p.ledger.document();
        let mut v = project_json(doc);
        v["changed"] = json!(true);
        if revisions::widths(&revisions::columns(doc), revisions::A3_TABLE_MM).1 {
            v["narrowed"] = json!(revisions::NARROWED);
        }
        if let Some(m) = message {
            v["message"] = json!(m);
        }
        Ok(v)
    })
}

// ---------------------------------------------------------------- revisions

/// The ops writing the given revision fields (`rev`, `date`, …) to the pending row.
fn revision_ops(doc: &Document, params: &Value) -> Result<Vec<Op>, String> {
    let n = revisions::pending_n(doc);
    let mut ops = Vec::new();
    let mut given = false;
    for f in FIELDS {
        let Some(v) = text_field(params, f)? else { continue };
        given = true;
        let v = match v.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
            Some(t) if f == "rev" => {
                revisions::check_rev(doc, t)?;
                Some(t.to_string())
            }
            other => value(doc, f, other)?,
        };
        ops.extend(set(ObjectRef::Project, Some(&doc.properties), &revisions::key(n, f), v));
    }
    // Brief 033 (design 033 §7.2): custom fields, `fields: {ecr: "ECR-114"}`, on the pending row
    // only; an issued row never changes.
    if let Some(n_given) = params.get("n").filter(|v| !v.is_null()) {
        let m = n_given.as_u64().and_then(|m| u32::try_from(m).ok()).ok_or("`n` must be a revision row number")?;
        if m != n {
            return Err(match revisions::list(doc).into_iter().find(|r| r.n == m) {
                Some(r) if r.issued.is_some() => format!("Rev {} is issued and cannot change.", r.rev()),
                _ => format!("There is no pending revision row {m}."),
            });
        }
    }
    if let Some(f) = params.get("fields").filter(|v| !v.is_null()) {
        let m = f.as_object().ok_or("`fields` must be an object of field: text|null")?;
        for (field, v) in m {
            if !FIELDS.contains(&field.as_str()) && !revisions::is_custom_field(field) {
                return Err(if field == revisions::ISSUED || field == revisions::RELEASED {
                    format!("\"{field}\" is set by issuing.")
                } else {
                    format!("\"{field}\" is not a field name. Use 1 to 20 letters, digits or _.")
                });
            }
            let t = match v {
                Value::Null => None,
                Value::String(t) => Some(t.trim().to_string()).filter(|t| !t.is_empty()),
                _ => return Err(format!("`fields.{field}` must be text, or null to clear it")),
            };
            let t = match t {
                Some(t) if field == "rev" => {
                    revisions::check_rev(doc, &t)?;
                    Some(t)
                }
                other => value(doc, field, other.as_deref())?,
            };
            given = true;
            ops.extend(set(ObjectRef::Project, Some(&doc.properties), &revisions::key(n, field), t));
        }
    }
    // A new pending row gets the suggested rev with its first field.
    let row_exists = revisions::pending(doc).is_some();
    let rev_given = params.get("rev").is_some_and(|v| !v.is_null());
    if given && !row_exists && !rev_given {
        ops.push(Op::SetProperty {
            object: ObjectRef::Project,
            key: revisions::key(n, "rev"),
            before: None,
            after: Some(revisions::pending_rev(doc)),
        });
    }
    Ok(ops)
}

/// Revisions are issued, and the pending one edited, on a release line only (design 020 §2.5.2,
/// owner answer 10): on another branch, the refusal names the way forward.
pub fn release_line_refusal(p: &Project, issuing: bool) -> Option<String> {
    let v = p.file.v2()?;
    if v.branch_record().release_line {
        return None;
    }
    let name = &v.branch_record().name;
    Some(if issuing {
        format!("Revisions are issued on main. Merge \"{name}\" into main first.")
    } else {
        format!("The pending revision is edited on main; on branch \"{name}\" its fields are read-only.")
    })
}

fn run_revision(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    with_project(s, |p, pg, _| {
        if let Some(r) = release_line_refusal(p, false) {
            return Err(r);
        }
        let ops = revision_ops(p.ledger.document(), params)?;
        if ops.is_empty() {
            return Ok(json!({"changed": false, "pending_rev": revisions::pending_rev(p.ledger.document())}));
        }
        p.apply_edit(pg, "revision", "Revision", ops)?;
        Ok(json!({"changed": true, "pending_rev": revisions::pending_rev(p.ledger.document())}))
    })
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

/// Plan issuing the pending revision with the field writes in `params` (as `issuerevision`).
///
/// # Errors
/// Off a release line, nothing done yet, Rev, Description or Drawn empty, or a rev used before:
/// in words, each with its way forward.
pub fn plan_issue(p: &Project, params: &Value) -> Result<IssuePlan, String> {
    if let Some(r) = release_line_refusal(p, true) {
        return Err(r);
    }
    let head = p.ledger.head();
    if head == 0 {
        return Err("Nothing to issue yet.".into());
    }
    let mut ops = revision_ops(p.ledger.document(), params)?;
    // The pending row as it will be after those writes.
    let doc = p.ledger.preview(&ops).map_err(|e| e.to_string())?;
    let n = revisions::pending_n(&doc);
    let row = revisions::pending(&doc).unwrap_or_default();
    if revisions::NEEDED.iter().any(|f| row.get(f).trim().is_empty()) {
        return Err(revisions::FILL_FIRST.into());
    }
    let rev = row.rev().to_string();
    let name = revisions::version_name(&rev);
    if doc.versions.contains_key(&name) {
        return Err(format!("Rev {rev} was issued before; choose another."));
    }
    let mut date = row.get("date").to_string();
    if date.is_empty() {
        date = revisions::today().unwrap_or_default();
        if !date.is_empty() {
            ops.push(Op::SetProperty { object: ObjectRef::Project, key: revisions::key(n, "date"), before: None, after: Some(date.clone()) });
        }
    }
    ops.push(Op::SetProperty { object: ObjectRef::Project, key: revisions::key(n, revisions::ISSUED), before: None, after: Some(name.clone()) });
    ops.push(Op::NameVersion { name, at: head, description: None });
    Ok(IssuePlan { ops, rev, n, date, head })
}

fn run_issuerevision(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    with_project(s, |p, pg, _| {
        // Checked before anything is written.
        plan_issue(p, params)?;
        // Brief 026 (design §5.5): provisional tags and wire numbers are finalised first (their own
        // transaction), or issued as they are, as asked; without an answer the ways forward are
        // named.
        let note = crate::numbering_cmds::before_issue(p, pg, params)?;
        let IssuePlan { ops, rev, date, head, .. } = plan_issue(p, params)?;
        p.apply_edit(pg, "issuerevision", &format!("Issue Rev {rev}"), ops)?;
        let tx = p.ledger.head();
        let line = match &note {
            Some(n) => format!("Rev {rev} issued (transaction {tx}) {n}. Save to keep it."),
            None => format!("Rev {rev} issued (transaction {tx}). Save to keep it."),
        };
        pg.echo(line.clone());
        Ok(json!({"rev": rev, "date": date, "at": head, "transaction": tx, "message": line}))
    })
}

fn run_openrevision(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let rev = params.get("rev").and_then(Value::as_str).map(str::trim).ok_or_else(|| err("`rev` is required"))?.to_string();
    with_any_page(s, |p, _| {
        let doc = p.ledger.document();
        let row = revisions::issued(doc)
            .into_iter()
            .find(|r| r.rev().eq_ignore_ascii_case(&rev))
            .ok_or_else(|| format!("Rev {rev} has not been issued."))?;
        // The project as issued (brief 027 §16): the state after the issue transaction, so the
        // title block, the revision table and the generated Revision list read this revision.
        let (named, _, then) = crate::history::issued_state(p, row.rev())?;
        let at = named.seq;
        let book = Book::of(&then);
        let pages: Vec<Value> = book
            .order
            .iter()
            .map(|pg| {
                json!({"page": pg.hex(), "number": book.number(&then, *pg),
                       "title": then.pages.get(pg).map(|x| x.title.clone())})
            })
            .collect();
        let date = row.get("date").to_string();
        p.revision_view =
            Some(RevisionView { rev: row.rev().to_string(), date: date.clone(), at, doc: Arc::new(then), bar: None, id: next_view_id() });
        Ok(json!({"rev": row.rev(), "date": date, "at": at, "pages": pages}))
    })
}

fn run_closerevision(s: &mut Session, _params: &Value) -> cadcraft_engine::Result<Value> {
    with_any_page(s, |p, _| {
        let was = p.revision_view.take().map(|v| v.rev);
        Ok(json!({"closed": was}))
    })
}

fn run_undoissue(s: &mut Session, _params: &Value) -> cadcraft_engine::Result<Value> {
    with_project(s, |p, pg, _| {
        let ops = p.ledger.undo_ops().unwrap_or_default();
        let Some((rev, _)) = revisions::issue_in(&revisions::inverse(&ops)) else {
            return Err("The last step did not issue a revision; use Undo.".into());
        };
        p.confirm_issue_undo = true;
        let r = p.undo_step(pg);
        p.confirm_issue_undo = false;
        r?;
        if p.revision_view.as_ref().is_some_and(|v| v.rev == rev) {
            p.revision_view = None;
        }
        Ok(json!({"rev": rev, "undone": true}))
    })
}
