//! The commands of brief 019 (design 019 §5–§7) that need the session: `issuerevision` (it may
//! Finalise numbering as a transaction of its own first), `openrevision`, `closerevision` and
//! `undoissue`. Page and project properties, folders and the pending revision are generic
//! ([`ecad_electrical::pages::commands`]).
//!
//! Page number, `=`/`+` overrides, sheet and folder are page properties; title, drawing number,
//! customer, status, defaults and revisions are project properties (decision FP1, FP4); folders
//! are typed (FP3); the sheet and page-number step are project settings (FP2).
//!
//! Each command is a thin shim (brief 070): its params struct reads the JSON, the rules in
//! [`ecad_electrical::pages::commands::sheets`] decide, and the shim commits the ops and answers in JSON.

use std::sync::Arc;

use cadcraft_engine::{CommandSpec, Session};
use ecad_electrical::CmdError;
use ecad_electrical::pages::commands::sheets::{self as rules, RevisionFields, revision_fields};
use ecad_model::document::book::Book;
use ecad_model::{Document, FolderId, Op, Page, PageId};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::commands::input::{Command, Typed};
use crate::commands::{on_any_page, on_project};
use crate::project::Project;
use ecad_electrical::commands::input::nullable;
pub use ecad_electrical::pages::commands::sheets::{IssuePlan, MAX_FOLDER_NAME, MAX_KEY_CHARS, MAX_STEP, PROJECT_KEYS};
use ecad_electrical::releases::revisions;
pub use ecad_electrical::releases::revisions::folder_path;

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
pub(crate) fn next_view_id() -> u64 {
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
    Typed::of::<IssuerevisionParams>("issuerevision"),
    Typed::of::<OpenrevisionParams>("openrevision"),
    Typed::of::<CloserevisionParams>("closerevision"),
    Typed::of::<UndoissueParams>("undoissue"),
];

/// The commands this module adds.
pub fn specs() -> Vec<CommandSpec> {
    use crate::commands::input::typed;
    vec![
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

pub use ecad_electrical::pages::page_types::MAX_NUMBER_CHARS;

// ---------------------------------------------------------------- folders

// ---------------------------------------------------------------- pages

/// The ops of `newpage {title, number?, folder?}`, the new page's ID and number.
/// The page is placed right after page `after` in its folder when given (brief 054, design
/// 054 §4.2: the page well's +), else last in `folder`. The first page of a folder with a start
/// number takes it (design 054 §4.3).
pub(crate) fn new_page_ops_at(
    p: &mut Project, title: String, number: Option<String>, folder: Option<FolderId>, after: Option<PageId>,
) -> Result<(PageId, String, Vec<Op>), CmdError> {
    if ecad_electrical::pages::generated::has_bom_page(p.ledger.document()) {
        p.reports();
    }
    let doc = p.ledger.document();
    let reports = p.kept_reports();
    rules::check_folder(doc, folder)?;
    let book = Book::of(doc);
    let (folder, index, place) = rules::new_page_at(doc, &book, folder, after)?;
    let number = match number.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()) {
        Some(n) => {
            rules::check_number(doc, &book, None, &n, reports)?;
            n
        }
        None => rules::next_number(doc, &book, folder, place, reports),
    };
    let mut page = Page::new(rules::page_title(&title)?);
    page.number = Some(ecad_model::PageNo::parse(&number).ok_or(CmdError::refused(rules::NUMBER_WORDS))?);
    page.folder = folder;
    let id: PageId = p.ids.fresh();
    Ok((id, number, vec![Op::AddPage { id, page, index }]))
}

// ---------------------------------------------------------------- project

/// Status suggestions (owner answer Q9: any text is allowed).
pub const STATUS_SUGGESTIONS: [&str; 4] = ["Draft", "For review", "For construction", "As built"];

/// Brief 054 (FP4): the cover's notes, a project property of newline-separated notes, at most
/// [`MAX_COVER_NOTES`] of [`MAX_COVER_NOTE_CHARS`] characters (design 054 §6.4).
pub use {
    ecad_electrical::pages::front::MAX_NOTE_CHARS as MAX_COVER_NOTE_CHARS, ecad_electrical::pages::front::MAX_NOTES as MAX_COVER_NOTES,
    ecad_electrical::pages::front::NOTES_KEY as COVER_NOTES, ecad_electrical::pages::front::NOTES_TOO_LONG as COVER_NOTES_TOO_LONG,
};

// ---------------------------------------------------------------- revisions

/// Revisions are issued, and the pending one edited, on a release line only (design 020 §2.5.2,
/// owner answer 10): on another branch, the refusal names the way forward.
pub fn release_line_refusal(p: &Project, issuing: bool) -> Option<String> {
    let v = p.file.project()?;
    let off_line = (!v.branch_record().release_line).then(|| v.branch_record().name.as_str());
    rules::release_line_refusal(off_line, issuing)
}

/// Plan issuing the pending revision with the field writes `f` (as `issuerevision`).
///
/// # Errors
/// Off a release line, nothing done yet, Rev, Description or Drawn empty, or a rev used before:
/// in words, each with its way forward.
fn plan_issue_of(p: &Project, before: &[Op], f: &RevisionFields) -> Result<IssuePlan, CmdError> {
    rules::plan_issue(&p.ledger, release_line_refusal(p, true), before, f, today)
}

/// Plan issuing the pending revision with no field writes (Release, brief 027).
///
/// # Errors
/// As `issuerevision`, in words.
pub fn plan_issue(p: &Project) -> Result<IssuePlan, CmdError> {
    plan_issue_of(p, &[], &revision_fields([None, None, None, None, None, None], None, None))
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
            // Brief 026 (design §5.5): provisional tags and wire numbers are finalised first, or
            // issued as they are, as asked; without an answer the ways forward are named. S4: the
            // Finalise and the issue are planned together, on the state Finalise leaves, and
            // recorded as ONE transaction: one history entry, one undo, nothing to take back.
            let (finalise, note) = crate::releases::release::numbering_seam::plan_before_issue_of(p, self.provisional.as_deref())?;
            let IssuePlan { ops, rev, date, head, .. } = plan_issue_of(p, &finalise, &f)?;
            let label = format!("Issue Rev {rev}");
            let all: Vec<Op> = finalise.into_iter().chain(ops).collect();
            p.apply_edit(pg, "issuerevision", &label, all)?;
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
            let rev = row.rev_value.ok_or_else(|| CmdError::refused("That row has no rev."))?;
            let (named, _, then) = crate::history::issued_state(p, rev).map_err(CmdError::refused)?;
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
            let rev = p.ledger.document().project.rev_scheme.show(rev);
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
