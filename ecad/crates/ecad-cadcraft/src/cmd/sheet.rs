//! The commands of brief 019 (design 019 §5–§7): page and project properties, folders,
//! revisions. Each write is one transaction committed through [`Project::apply_edit`]; the
//! no-field forms of `pageprops` and `projectprops`, and `openrevision`/`closerevision`, only
//! read.
//!
//! Page number, `=`/`+` overrides, sheet and folder are page properties; title, drawing number,
//! customer, status, defaults and revisions are project properties (decision FP1, FP4); folders
//! are typed (FP3); the sheet and page-number step are project settings (FP2).
//!
//! Each command is a thin shim (brief 070): its params struct reads the JSON, the rules in
//! [`ecad_electrical::cmd::sheet`] decide, and the shim commits the ops and answers in JSON.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use cadcraft_engine::{CommandSpec, Session};
use ecad_electrical::CmdError;
use ecad_electrical::cmd::sheet::{self as rules, PageChange, ProjectChange, PropsPlan, RevisionFields};
use ecad_model::book::{self, Book, FUNCTION_KEY, LOCATION_KEY, Node};
use ecad_model::{Document, FolderId, Op, Page, PageId, ReferenceStyle};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::cmd::input::{Command, Typed, nullable};
use crate::cmd::{err, on_any_page, on_project};
use crate::project::Project;
pub use crate::revisions::folder_path;
use crate::revisions::{self, FIELDS};
pub use ecad_electrical::cmd::sheet::{IssuePlan, MAX_FOLDER_NAME, MAX_KEY_CHARS, MAX_STEP, PROJECT_KEYS};

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

/// The commands of this module with a typed input.
pub const TYPED: &[Typed] = &[
    Typed::of::<PagepropsParams>("pageprops"),
    Typed::of::<ProjectpropsParams>("projectprops"),
    Typed::of::<NewfolderParams>("newfolder"),
    Typed::of::<RenamefolderParams>("renamefolder"),
    Typed::of::<MovefolderParams>("movefolder"),
    Typed::of::<DeletefolderParams>("deletefolder"),
    Typed::of::<RevisionParams>("revision"),
    Typed::of::<IssuerevisionParams>("issuerevision"),
    Typed::of::<OpenrevisionParams>("openrevision"),
    Typed::of::<CloserevisionParams>("closerevision"),
    Typed::of::<UndoissueParams>("undoissue"),
];

/// The commands this module adds.
pub fn specs() -> Vec<CommandSpec> {
    use crate::cmd::input::typed;
    vec![
        CommandSpec::new("pageprops", "Page Properties", typed::<PagepropsParams>)
            .params(
                "{page?: id (default: the active page), number?, title?, function?, location?, sheet?} (each null \
clears; no fields: the current values, read only)",
            )
            .noundo(),
        CommandSpec::new("projectprops", "Project Properties", typed::<ProjectpropsParams>)
            .params(
                "{title?, drawing_number?, customer?, status?, function?, location?, sheet?: \"A3\"|\"A2 12x8\"|…, \
page_number_step?: n, reference_style?: \"page-zone\"|\"page-column-row\"|\"page-column\", contact_mirror?: \"auto\"|\"below\"|\"right\"|\"off\", tag_padding?: 1..6, \
tag_prefixes?: {symbol: prefix|null}, colour_style?: \"iec\"|\"three-letter\", label_height?: mm, \
tag_dash?: bool, tag_case?: \"upper\"|\"typed\", revision_columns?: \"rev, date, ecr:ECR@20, description\"|null, \
revision_pending?: \"star\"|\"draft\"|\"off\", props?: {key: value|null}} (one transaction; no fields: everything, read only)",
            )
            .noundo(),
        CommandSpec::new("newfolder", "New Folder", typed::<NewfolderParams>).params("{name, parent?: id|null}").noundo(),
        CommandSpec::new("renamefolder", "Rename Folder", typed::<RenamefolderParams>).params("{folder: id, name}").noundo(),
        CommandSpec::new("movefolder", "Move Folder", typed::<MovefolderParams>)
            .params("{folder: id, parent: id|null, index?: n} (1 = first among its new siblings; default last)")
            .noundo(),
        CommandSpec::new("deletefolder", "Delete Folder", typed::<DeletefolderParams>)
            .params("{folder: id} (its pages and sub-folders move to its parent)")
            .noundo(),
        CommandSpec::new("revision", "Revision", typed::<RevisionParams>)
            .params(
                "{rev?, date?, description?, drawn?, checked?, approved?, fields?: {field: text|null}, n?} (the pending revision's \
fields, custom fields by name; an issued row is refused)",
            )
            .noundo(),
        CommandSpec::new("issuerevision", "Issue Revision", typed::<IssuerevisionParams>)
            .params(
                "{rev?, date?, description?, drawn?, checked?, approved?, provisional?: \"finalise\"|\"issue\"} (issues the \
pending revision: needs Rev, Description and Drawn; names the log point \"Rev <rev>\"; with provisional tags or wire numbers \
present, `provisional` says whether to finalise them first or issue them as they are)",
            )
            .noundo(),
        CommandSpec::new("openrevision", "Reopen Revision", typed::<OpenrevisionParams>)
            .params("{rev} (the project as issued, read only; records nothing)")
            .noundo(),
        CommandSpec::new("closerevision", "Back to Current", typed::<CloserevisionParams>).params("{}").noundo(),
        CommandSpec::new("undoissue", "Undo Issue", typed::<UndoissueParams>)
            .params("{} (undoes the last step when it issued a revision: removes its release mark)")
            .noundo(),
    ]
}

// ---------------------------------------------------------------- page numbers

pub use crate::page_types::MAX_NUMBER_CHARS;

/// The number a new page gets after the page shown as `before` (design 019 §6.1): its leading
/// digits plus the project's step, skipping numbers in use; with no digits or no page before, its
/// book position `position`.
pub fn default_number(doc: &Document, book: &Book, before: Option<PageId>, position: usize, outputs: Option<&crate::outputs::Outputs>) -> String {
    default_number_from(doc, book, before, position, outputs, None)
}

/// [`default_number`], or for the first page of a folder with a start number (brief 054, design
/// 054 §4.3) that start when it is free, else the next free after it. Leading zeros keep their
/// width (`01` gives `01`, `02`), from the start or from the page before.
pub fn default_number_from(
    doc: &Document, book: &Book, before: Option<PageId>, position: usize, outputs: Option<&crate::outputs::Outputs>, start: Option<&str>,
) -> String {
    // Brief 033: every sheet's number, continuation sheets included, is in use.
    let used: BTreeSet<String> = book
        .order
        .iter()
        .flat_map(|p| crate::generated::sheet_numbers(doc, book, *p, crate::draw::outputs::kept_of(outputs)))
        .map(|n| n.to_ascii_lowercase())
        .collect();
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

/// `newfolder {name, parent?}`: a folder, last among its siblings.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct NewfolderParams {
    /// The folder's name: 1 to 100 characters.
    name: String,
    /// The folder it goes in (absent or null: the top level).
    #[serde(default)]
    parent: Option<FolderId>,
}

impl Command for NewfolderParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let name = rules::folder_name(&self.name).map_err(|e| err(e.to_string()))?;
        on_project(s, |p, pg, _| {
            let (id, op) = rules::new_folder(p.ledger.document(), &mut p.ids, &name, self.parent)?;
            p.apply_edit(pg, "newfolder", "New folder", vec![op])?;
            Ok(json!({"folder": id.hex(), "name": name}))
        })
    }
}

/// `renamefolder {folder, name}`: the same name changes nothing.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RenamefolderParams {
    /// The folder.
    folder: FolderId,
    /// Its new name: 1 to 100 characters.
    name: String,
}

impl Command for RenamefolderParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let folder = self.folder;
        let name = rules::folder_name(&self.name).map_err(|e| err(e.to_string()))?;
        on_project(s, |p, pg, _| {
            let Some(op) = rules::rename_folder(p.ledger.document(), folder, &name)? else {
                return Ok(json!({"folder": folder.hex(), "name": name, "changed": false}));
            };
            p.apply_edit(pg, "renamefolder", "Rename folder", vec![op])?;
            Ok(json!({"folder": folder.hex(), "name": name, "changed": true}))
        })
    }
}

/// `movefolder {folder, parent, index?}`: into another folder or the top level.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct MovefolderParams {
    /// The folder.
    folder: FolderId,
    /// The folder it goes in, or null for the top level.
    #[serde(deserialize_with = "nullable")]
    #[schemars(required, with = "crate::cmd::input::OrNull<FolderId>")]
    parent: Option<Option<FolderId>>,
    /// Its place among its new siblings, 1 = first (default last).
    #[serde(default)]
    index: Option<usize>,
}

impl Command for MovefolderParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let (folder, parent, index) = (self.folder, self.parent.flatten(), self.index);
        on_project(s, |p, pg, _| {
            let ops = rules::move_folder(p.ledger.document(), folder, parent, index)?;
            if ops.is_empty() {
                return Ok(json!({"folder": folder.hex(), "changed": false}));
            }
            p.apply_edit(pg, "movefolder", "Move folder", ops)?;
            Ok(json!({"folder": folder.hex(), "changed": true}))
        })
    }
}

/// `deletefolder {folder}`: its pages and sub-folders move to its parent.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct DeletefolderParams {
    /// The folder.
    folder: FolderId,
}

impl Command for DeletefolderParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let folder = self.folder;
        on_project(s, |p, pg, _| {
            let done = rules::delete_folder(p.ledger.document(), folder)?;
            p.apply_edit(pg, "deletefolder", "Delete folder", done.ops)?;
            pg.echo(done.message.clone());
            Ok(json!({"folder": folder.hex(), "pages_moved": done.pages, "message": done.message}))
        })
    }
}

// ---------------------------------------------------------------- pages

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
pub fn next_number(
    doc: &Document, book: &Book, folder: Option<FolderId>, place: (Option<PageId>, usize), outputs: Option<&crate::outputs::Outputs>,
) -> String {
    let (before, position) = place;
    let first = before.is_none_or(|b| book::folder_of(doc, b).filter(|f| doc.folders.contains_key(f)) != folder);
    let start = folder.filter(|_| first).and_then(|f| crate::page_types::start_of(doc, f));
    default_number_from(doc, book, before, position, outputs, start.as_deref())
}

/// The ops of `newpage {title, number?, folder?}`, the new page's ID and number.
/// The page is placed right after page `after` in its folder when given (brief 054, design
/// 054 §4.2: the page well's +), else last in `folder`. The first page of a folder with a start
/// number takes it (design 054 §4.3).
pub(crate) fn new_page_ops_at(
    p: &mut Project, title: String, number: Option<String>, folder: Option<FolderId>, after: Option<PageId>,
) -> Result<(PageId, String, Vec<Op>), CmdError> {
    if crate::generated::has_bom_page(p.ledger.document()) {
        p.outputs();
    }
    let doc = p.ledger.document();
    let outputs = p.outputs.as_ref();
    rules::check_folder(doc, folder)?;
    let book = Book::of(doc);
    let (folder, index, place) = new_page_at(doc, &book, folder, after)?;
    let number = match number.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()) {
        Some(n) => {
            rules::check_number(doc, &book, None, &n, crate::draw::outputs::kept_of(outputs))?;
            n
        }
        None => next_number(doc, &book, folder, place, outputs),
    };
    let mut page = Page::new(rules::page_title(&title)?);
    page.number = Some(ecad_model::PageNo::parse(&number).ok_or(CmdError::refused(rules::NUMBER_WORDS))?);
    page.folder = folder;
    let id: PageId = p.ids.fresh();
    Ok((id, number, vec![Op::AddPage { id, page, index }]))
}

/// The page's properties as `pageprops` reports them.
pub fn page_json(doc: &Document, book: &Book, page: PageId, outputs: Option<&crate::outputs::Outputs>) -> Value {
    let pg = doc.pages.get(&page);
    use crate::designations::{Aspect, default_key, page_key};
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
        "function": prop(FUNCTION_KEY).map(|v| crate::designations::display(doc, FUNCTION_KEY, &v)),
        "location": prop(LOCATION_KEY).map(|v| crate::designations::display(doc, LOCATION_KEY, &v)),
        "function_key": prop(FUNCTION_KEY),
        "location_key": prop(LOCATION_KEY),
        "function_shown": prop(FUNCTION_KEY).or_else(|| project(FUNCTION_KEY)).map(|v| crate::designations::display(doc, FUNCTION_KEY, &v)),
        "location_shown": prop(LOCATION_KEY).or_else(|| project(LOCATION_KEY)).map(|v| crate::designations::display(doc, LOCATION_KEY, &v)),
        "sheet": pg.and_then(|x| x.sheet).map(|s| s.text()),
        "sheet_shown": ecad_model::sheet::sheet_of(doc, page).summary(),
        "folder": folder.map(FolderId::hex),
        "folder_path": folder_path(doc, folder),
        "generated": pg.and_then(|x| x.kind.generated_kind()).map(ecad_model::Word::word),
        // Brief 054: the page's type, resolved (design 054 §2.3).
        "type": crate::styles::page::type_of(doc, page).text(),
        "sheets": crate::generated::sheets(doc, page, outputs),
        "sheet_numbers": crate::generated::sheet_numbers(doc, book, page, crate::draw::outputs::kept_of(outputs)),
    })
}

/// Commit `plan` (with `label`), keep its conflicts when it moved nothing, echo them, and answer
/// with `json` of the document after it.
fn commit_props(p: &mut Project, pg: &mut crate::access::Pages<'_>, id: &str, label: &str, plan: PropsPlan) -> Result<(), CmdError> {
    p.apply_edit(pg, id, label, plan.ops)?;
    if plan.proposed.moved == 0 {
        p.conflicts = plan.proposed.conflicts.clone();
    }
    for l in crate::proposals::conflict_words(p.ledger.document(), &plan.proposed.conflicts) {
        pg.echo(l);
    }
    Ok(())
}

/// `pageprops {page?, number?, title?, function?, location?, sheet?, proposal?}`: a page's
/// properties (each null clears; no fields: the current values, read only).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PagepropsParams {
    /// The page (default: the active page).
    #[serde(default)]
    page: Option<PageId>,
    /// The page number: 1 to 12 letters, digits or _ (null: back to the book position).
    #[serde(default, deserialize_with = "nullable")]
    number: Option<Option<String>>,
    /// The page title: 1 to 200 characters.
    #[serde(default, deserialize_with = "nullable")]
    title: Option<Option<String>>,
    /// The page's function (=…), or null for the project's.
    #[serde(default, deserialize_with = "nullable")]
    function: Option<Option<String>>,
    /// The page's location (+…), or null for the project's.
    #[serde(default, deserialize_with = "nullable")]
    location: Option<Option<String>>,
    /// The page's sheet ("A3", "A2 12x8"…), or null (or "project") for the project's.
    #[serde(default, deserialize_with = "nullable")]
    sheet: Option<Option<String>>,
    /// What a new function or location does to the page's device symbols: "propose" (default),
    /// "move" or "keep".
    #[serde(default)]
    proposal: Option<String>,
    /// Refused: a page's kind is set when it is made.
    #[serde(default, deserialize_with = "nullable")]
    generated: Option<Option<Value>>,
    /// Refused for `text.` keys (Text styles); page properties are fields of their own.
    #[serde(default)]
    props: Option<Map<String, Value>>,
}

impl Command for PagepropsParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        if self.generated.is_some() {
            return Err(err(crate::generated::KIND_FIXED));
        }
        // Brief 031 (§10.6): a page's `text.` keys are written by `showtext` only, also when asked
        // for as `props`.
        if self.props.as_ref().is_some_and(|o| o.keys().any(|k| crate::text_style::is_reserved_key(k))) {
            return Err(err(crate::text_style::RESERVED_KEY));
        }
        let page = self.page;
        let title = match self.title {
            None => None,
            Some(t) => Some(rules::page_title(t.as_deref().unwrap_or_default()).map_err(|e| err(e.to_string()))?),
        };
        let mut change =
            PageChange { number: self.number, title, function: self.function, location: self.location, sheet: self.sheet, mode: Default::default() };
        if !change.writes() {
            return on_any_page(s, |p, _| {
                if crate::generated::has_bom_page(p.ledger.document()) {
                    p.outputs();
                }
                let doc = p.ledger.document();
                let page = page.or(p.active_page()).ok_or(CmdError::input("`page` (hex id) is required: no page is shown"))?;
                if !doc.pages.contains_key(&page) {
                    return Err(CmdError::refused(ecad_electrical::msg::NO_SUCH_PAGE));
                }
                Ok(page_json(doc, &Book::of(doc), page, p.outputs.as_ref()))
            });
        }
        let proposal = self.proposal;
        on_project(s, |p, pg, active| {
            let page = page.unwrap_or(active);
            if crate::generated::has_bom_page(p.ledger.document()) {
                p.outputs();
            }
            if !p.ledger.document().pages.contains_key(&page) {
                return Err(CmdError::refused(ecad_electrical::msg::NO_SUCH_PAGE));
            }
            change.mode = rules::proposal_mode(proposal.as_deref())?;
            let kept = crate::draw::outputs::kept_of(p.outputs.as_ref());
            let plan = rules::page_props(p.ledger.document(), &mut p.ids, page, &change, kept)?;
            if plan.ops.is_empty() {
                let doc = p.ledger.document();
                let mut v = page_json(doc, &Book::of(doc), page, p.outputs.as_ref());
                v["changed"] = json!(false);
                return Ok(v);
            }
            commit_props(p, pg, "pageprops", "Page properties", plan)?;
            let doc = p.ledger.document();
            let mut v = page_json(doc, &Book::of(doc), page, p.outputs.as_ref());
            v["changed"] = json!(true);
            Ok(v)
        })
    }
}

// ---------------------------------------------------------------- project

/// Status suggestions (owner answer Q9: any text is allowed).
pub const STATUS_SUGGESTIONS: [&str; 4] = ["Draft", "For review", "For construction", "As built"];

/// Brief 054 (FP4): the cover's notes, a project property of newline-separated notes, at most
/// [`MAX_COVER_NOTES`] of [`MAX_COVER_NOTE_CHARS`] characters (design 054 §6.4).
pub use crate::front::{
    MAX_NOTE_CHARS as MAX_COVER_NOTE_CHARS, MAX_NOTES as MAX_COVER_NOTES, NOTES_KEY as COVER_NOTES, NOTES_TOO_LONG as COVER_NOTES_TOO_LONG,
};

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
        // Brief 052: the function and location show their codes (design 072 §5.3: fields of the
        // project).
        let v = match crate::designations::Aspect::of_property(k) {
            Some(a) => crate::designations::default_key(&doc.project, a).map(|v| crate::designations::display(doc, k, &v)),
            None => doc.properties.get(k).cloned(),
        };
        fields.insert(k.into(), v.map_or(Value::Null, Value::String));
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
    v["tag_case"] = json!(if style.case == ecad_model::tag::Case::Typed { "typed" } else { "upper" });
    let cols = revisions::columns(doc);
    v["revision_columns"] = json!(cols.iter().map(|c| json!({"field": c.field, "header": c.header, "width": c.width})).collect::<Vec<_>>());
    v["revision_columns_stored"] = json!(doc.properties.get(revisions::COLUMNS_KEY));
    v["revision_pending"] = json!(match revisions::PendingStyle::of(doc) {
        revisions::PendingStyle::Star => "star",
        revisions::PendingStyle::Draft => "draft",
        revisions::PendingStyle::Off => "off",
    });
    v["tag_prefixes"] = Value::Object(crate::cmd::device::prefixes_json(doc));
    // Brief 022: the wiring settings the Project settings popout edits.
    v["colour_style"] = json!(doc.settings.wiring.colour_style().name());
    v["label_height"] = json!(wire_label_height(doc));
    // Brief 026: Project settings → Numbering.
    v["provisional"] = json!(ecad_model::numbering::provisional_on(doc));
    v["revisions"] = Value::Array(revs);
    v["pending_rev"] = json!(revisions::pending_rev(doc));
    v
}

/// How cross-references are written.
#[derive(Deserialize, JsonSchema, Clone, Copy)]
enum ReferenceStyleParam {
    /// Page and zone (`12.C4`).
    #[serde(rename = "page-zone")]
    Zone,
    /// Page, column and row.
    #[serde(rename = "page-column-row")]
    ColumnRow,
    /// Page and column.
    #[serde(rename = "page-column")]
    Column,
}

/// `projectprops {title?, drawing_number?, customer?, status?, function?, location?, sheet?,
/// page_number_step?, reference_style?, contact_mirror?, tag_padding?, tag_prefixes?,
/// colour_style?, label_height?, tag_dash?, tag_case?, revision_columns?, revision_pending?,
/// provisional?, props?, proposal?}`: the project's properties and settings, in one transaction
/// (no fields: everything, read only).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProjectpropsParams {
    /// The project title (null clears).
    #[serde(default, deserialize_with = "nullable")]
    title: Option<Option<String>>,
    /// The drawing number (null clears).
    #[serde(default, deserialize_with = "nullable")]
    drawing_number: Option<Option<String>>,
    /// The customer (null clears).
    #[serde(default, deserialize_with = "nullable")]
    customer: Option<Option<String>>,
    /// The status, any text (null clears).
    #[serde(default, deserialize_with = "nullable")]
    status: Option<Option<String>>,
    /// The project's function (=…; null clears).
    #[serde(default, deserialize_with = "nullable")]
    function: Option<Option<String>>,
    /// The project's location (+…; null clears).
    #[serde(default, deserialize_with = "nullable")]
    location: Option<Option<String>>,
    /// The sheet: "A3", "A2 12x8"…
    #[serde(default, deserialize_with = "nullable")]
    sheet: Option<Option<String>>,
    /// The page-number step, 1 to 1000.
    #[serde(default, deserialize_with = "nullable")]
    page_number_step: Option<Option<u64>>,
    /// How cross-references are written.
    #[serde(default, deserialize_with = "nullable")]
    reference_style: Option<Option<ReferenceStyleParam>>,
    /// Where contact tables go by default: "auto", "below", "right" or "off".
    #[serde(default, deserialize_with = "nullable")]
    contact_mirror: Option<Option<String>>,
    /// Digits of a tag's number, 1 to 6.
    #[serde(default, deserialize_with = "nullable")]
    tag_padding: Option<Option<u64>>,
    /// Tag prefixes by symbol: a prefix, or null for the built-in one.
    #[serde(default, deserialize_with = "nullable")]
    tag_prefixes: Option<Option<BTreeMap<String, Option<String>>>>,
    /// How label colours are written: "iec" (BN, BU, GNYE) or "three-letter" (BRN, BLU, GRN/YEL).
    #[serde(default, deserialize_with = "nullable")]
    colour_style: Option<Option<String>>,
    /// Wire label height in mm, 0.5 to 10.
    #[serde(default, deserialize_with = "nullable")]
    label_height: Option<Option<f64>>,
    /// Whether tags have a leading `-`.
    #[serde(default, deserialize_with = "nullable")]
    tag_dash: Option<Option<bool>>,
    /// "upper" or "typed": whether tags keep the case they are typed in.
    #[serde(default, deserialize_with = "nullable")]
    tag_case: Option<Option<String>>,
    /// The revision table's columns ("rev, date, ecr:ECR@20, description"), or null for the
    /// built-in ones.
    #[serde(default, deserialize_with = "nullable")]
    revision_columns: Option<Option<String>>,
    /// How the pending revision shows: "star", "draft" or "off".
    #[serde(default, deserialize_with = "nullable")]
    revision_pending: Option<Option<String>>,
    /// Provisional tags until Finalise.
    #[serde(default, deserialize_with = "nullable")]
    provisional: Option<Option<bool>>,
    /// Free properties: key: value, or null to clear.
    #[serde(default)]
    props: Option<BTreeMap<String, Option<String>>>,
    /// What a new function or location does to the device symbols: "propose" (default), "move" or
    /// "keep".
    #[serde(default, deserialize_with = "nullable")]
    proposal: Option<Option<String>>,
}

impl ProjectpropsParams {
    /// Whether no field is given (`projectprops` then only reads).
    fn is_empty(&self) -> bool {
        let fields = [&self.title, &self.drawing_number, &self.customer, &self.status, &self.function, &self.location];
        fields.iter().all(|f| f.is_none())
            && self.sheet.is_none()
            && self.page_number_step.is_none()
            && self.reference_style.is_none()
            && self.contact_mirror.is_none()
            && self.tag_padding.is_none()
            && self.tag_prefixes.is_none()
            && self.colour_style.is_none()
            && self.label_height.is_none()
            && self.tag_dash.is_none()
            && self.tag_case.is_none()
            && self.revision_columns.is_none()
            && self.revision_pending.is_none()
            && self.provisional.is_none()
            && self.props.is_none()
            && self.proposal.is_none()
    }
}

impl Command for ProjectpropsParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        if self.is_empty() {
            return on_any_page(s, |p, _| Ok(project_json(p.ledger.document())));
        }
        let given = [self.title, self.drawing_number, self.customer, self.status, self.function, self.location];
        let fields = PROJECT_KEYS.into_iter().zip(given).filter_map(|(k, v)| v.map(|v| (k, v))).collect();
        let proposal = self.proposal.flatten();
        let mut change = ProjectChange {
            fields,
            props: self.props.map(|m| m.into_iter().collect()).unwrap_or_default(),
            provisional: self.provisional.flatten(),
            sheet: self.sheet.flatten(),
            page_number_step: self.page_number_step.flatten(),
            reference_style: self.reference_style.flatten().map(|r| match r {
                ReferenceStyleParam::Zone => ReferenceStyle::PageZone,
                ReferenceStyleParam::ColumnRow => ReferenceStyle::PageColumnRow,
                ReferenceStyleParam::Column => ReferenceStyle::PageColumn,
            }),
            contact_mirror: self.contact_mirror.flatten(),
            tag_padding: self.tag_padding.flatten(),
            tag_prefixes: self.tag_prefixes.flatten().map(|m| m.into_iter().collect()).unwrap_or_default(),
            colour_style: self.colour_style.flatten(),
            label_height: self.label_height.flatten(),
            revision_columns: self.revision_columns,
            revision_pending: self.revision_pending.flatten(),
            tag_dash: self.tag_dash.flatten(),
            tag_case: self.tag_case.flatten(),
            mode: Default::default(),
        };
        on_project(s, |p, pg, _| {
            change.mode = rules::proposal_mode(proposal.as_deref())?;
            let plan = rules::project_props(p.ledger.document(), &mut p.ids, &change)?;
            if plan.ops.is_empty() {
                let mut v = project_json(p.ledger.document());
                v["changed"] = json!(false);
                return Ok(v);
            }
            let message = plan.message.clone();
            commit_props(p, pg, "projectprops", "Project properties", plan)?;
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
}

// ---------------------------------------------------------------- revisions

/// The revision fields given, as the rules take them.
fn revision_fields(standard: [Option<Option<String>>; 6], n: Option<u64>, custom: Option<BTreeMap<String, Option<String>>>) -> RevisionFields {
    RevisionFields {
        standard: FIELDS.into_iter().zip(standard).filter_map(|(k, v)| v.map(|v| (k, v))).collect(),
        n,
        custom: custom.map(|m| m.into_iter().collect()).unwrap_or_default(),
    }
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

/// `revision {rev?, date?, description?, drawn?, checked?, approved?, fields?, n?}`: the pending
/// revision's fields, custom fields by name; an issued row is refused.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RevisionParams {
    /// The rev (null clears).
    #[serde(default, deserialize_with = "nullable")]
    rev: Option<Option<String>>,
    /// The date (null clears).
    #[serde(default, deserialize_with = "nullable")]
    date: Option<Option<String>>,
    /// The description (null clears).
    #[serde(default, deserialize_with = "nullable")]
    description: Option<Option<String>>,
    /// Who drew it (null clears).
    #[serde(default, deserialize_with = "nullable")]
    drawn: Option<Option<String>>,
    /// Who checked it (null clears).
    #[serde(default, deserialize_with = "nullable")]
    checked: Option<Option<String>>,
    /// Who approved it (null clears).
    #[serde(default, deserialize_with = "nullable")]
    approved: Option<Option<String>>,
    /// Custom fields by name: text, or null to clear.
    #[serde(default)]
    fields: Option<BTreeMap<String, Option<String>>>,
    /// The revision row meant; refused unless it is the pending one.
    #[serde(default)]
    n: Option<u64>,
}

impl RevisionParams {
    fn fields(self) -> RevisionFields {
        revision_fields([self.rev, self.date, self.description, self.drawn, self.checked, self.approved], self.n, self.fields)
    }
}

impl Command for RevisionParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let f = self.fields();
        on_project(s, |p, pg, _| {
            if let Some(r) = release_line_refusal(p, false) {
                return Err(CmdError::refused(r));
            }
            let ops = rules::revision_ops(p.ledger.document(), &f)?;
            if ops.is_empty() {
                return Ok(json!({"changed": false, "pending_rev": revisions::pending_rev(p.ledger.document())}));
            }
            p.apply_edit(pg, "revision", "Revision", ops)?;
            Ok(json!({"changed": true, "pending_rev": revisions::pending_rev(p.ledger.document())}))
        })
    }
}

/// Plan issuing the pending revision with the field writes `f` (as `issuerevision`).
///
/// # Errors
/// Off a release line, nothing done yet, Rev, Description or Drawn empty, or a rev used before:
/// in words, each with its way forward.
fn plan_issue_of(p: &Project, f: &RevisionFields) -> Result<IssuePlan, CmdError> {
    rules::plan_issue(&p.ledger, release_line_refusal(p, true), f, today)
}

/// Plan issuing the pending revision with no field writes (Release, brief 027).
///
/// # Errors
/// As `issuerevision`, in words.
pub fn plan_issue(p: &Project) -> Result<IssuePlan, CmdError> {
    plan_issue_of(p, &revision_fields([None, None, None, None, None, None], None, None))
}

/// `issuerevision {rev?, date?, description?, drawn?, checked?, approved?, provisional?}`: issues
/// the pending revision (needs Rev, Description and Drawn; names the log point "Rev <rev>").
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct IssuerevisionParams {
    /// The rev (null clears).
    #[serde(default, deserialize_with = "nullable")]
    rev: Option<Option<String>>,
    /// The date (null clears; empty: today).
    #[serde(default, deserialize_with = "nullable")]
    date: Option<Option<String>>,
    /// The description (null clears).
    #[serde(default, deserialize_with = "nullable")]
    description: Option<Option<String>>,
    /// Who drew it (null clears).
    #[serde(default, deserialize_with = "nullable")]
    drawn: Option<Option<String>>,
    /// Who checked it (null clears).
    #[serde(default, deserialize_with = "nullable")]
    checked: Option<Option<String>>,
    /// Who approved it (null clears).
    #[serde(default, deserialize_with = "nullable")]
    approved: Option<Option<String>>,
    /// With provisional tags or wire numbers present: "finalise" them first, or "issue" them as
    /// they are.
    #[serde(default)]
    provisional: Option<String>,
}

impl Command for IssuerevisionParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let f = revision_fields([self.rev, self.date, self.description, self.drawn, self.checked, self.approved], None, None);
        on_project(s, |p, pg, _| {
            // Checked before anything is written.
            plan_issue_of(p, &f)?;
            // Brief 026 (design §5.5): provisional tags and wire numbers are finalised first (their
            // own transaction), or issued as they are, as asked; without an answer the ways forward
            // are named.
            let note = crate::cmd::numbering::before_issue(p, pg, self.provisional.as_deref())?;
            let IssuePlan { ops, rev, date, head, .. } = plan_issue_of(p, &f)?;
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
}

/// `openrevision {rev}`: the project as issued, read only; records nothing.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OpenrevisionParams {
    /// The issued rev (any case).
    rev: String,
}

impl Command for OpenrevisionParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let rev = self.rev.trim().to_string();
        on_any_page(s, |p, _| {
            let row = rules::issued(p.ledger.document(), &rev)?;
            // The project as issued (brief 027 §16): the state after the issue transaction, so the
            // title block, the revision table and the generated Revision list read this revision.
            let (named, _, then) = crate::history::issued_state(p, row.rev()).map_err(CmdError::refused)?;
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
}

/// `closerevision {}`: back to the current project from an opened revision.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(extend("properties" = {}))]
pub(crate) struct CloserevisionParams {}

impl Command for CloserevisionParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        on_any_page(s, |p, _| {
            let was = p.revision_view.take().map(|v| v.rev);
            Ok(json!({"closed": was}))
        })
    }
}

/// `undoissue {}`: undoes the last step when it issued a revision (removes its release mark).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(extend("properties" = {}))]
pub(crate) struct UndoissueParams {}

impl Command for UndoissueParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        on_project(s, |p, pg, _| {
            let rev = rules::undone_issue(&p.ledger.undo_ops().unwrap_or_default())?;
            p.confirm_issue_undo = true;
            let r = p.undo_step(pg);
            p.confirm_issue_undo = false;
            r.map_err(CmdError::refused)?;
            if p.revision_view.as_ref().is_some_and(|v| v.rev == rev) {
                p.revision_view = None;
            }
            Ok(json!({"rev": rev, "undone": true}))
        })
    }
}

/// Today's date `YYYY-MM-DD` (UTC; the desktop app fills the local date in the dialog), or `None`
/// where there is no clock.
pub fn today() -> Option<String> {
    revisions::date_of_ms(crate::project::now_ms()?)
}

/// The wire label cap height the project resolves to, millimetres (its chosen one, its standard's
/// or the built-in; brief 066).
pub(crate) fn wire_label_height(doc: &ecad_model::Document) -> f64 {
    crate::defaults::resolve(doc, &crate::defaults::text::WIRE_LABEL_HEIGHT, crate::defaults::Scope::project()).number()
}
