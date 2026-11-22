//! The page commands that need the session (briefs 010, 019, 024, 028, 046, 054): `newpage`,
//! `openpage` and `ecadinspect` (a read-only snapshot). `renamepage` and `movepage` are generic
//! ([`ecad_electrical::pages::commands::page_edits`]).
//!
//! Each command is a thin shim (brief 070): its params struct reads the JSON, the rules in
//! [`ecad_electrical::pages::commands::sheets`] decide, and the shim commits the ops and answers in JSON.
//! Opening a page is the session's.

use cadcraft_engine::Session;
use ecad_electrical::CmdError;
use ecad_electrical::pages::commands::sheets as rules;
use ecad_model::{DeviceId, FolderId, PageId};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::commands::input::{Command, Typed};
use crate::commands::{err, on_any_page, on_project};

/// The commands of this module with a typed input.
pub const TYPED: &[Typed] =
    &[Typed::of::<NewpageParams>("newpage"), Typed::of::<OpenpageParams>("openpage"), Typed::of::<EcadinspectParams>("ecadinspect")];

/// Open `page` (or bring it to the front): its ID and document.
fn open(s: &mut Session, page: PageId) -> cadcraft_engine::Result<Value> {
    let uid = on_any_page(s, |p, pg| p.open_page(pg, page).map_err(CmdError::refused))?;
    Ok(json!({"page": page.hex(), "document": uid}))
}

/// `newpage {title?, number?, folder?, after?, type?, generated?, strip?, style?}`: a new page,
/// opened. Right after page `after` in its folder, else last in `folder`; numbered `number`, else
/// the next after the page before it (or a folder's start number for its first page).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct NewpageParams {
    /// The title (default: the kind's or type's name, else "new").
    #[serde(default)]
    title: Option<String>,
    /// The page number, such as "105"; null for the next number.
    #[serde(default)]
    number: Option<String>,
    /// The folder it goes last in.
    #[serde(default)]
    folder: Option<FolderId>,
    /// The page it goes right after, in that page's folder.
    #[serde(default)]
    after: Option<PageId>,
    /// The page type's ID or name, such as "schematic".
    #[serde(default, rename = "type")]
    ty: Option<String>,
    /// A generated page's kind: "cover", "contents", "revisions", "bom-location", "terminals" or
    /// "cables".
    #[serde(default)]
    generated: Option<String>,
    /// The one terminal strip a terminal plan shows.
    #[serde(default)]
    strip: Option<DeviceId>,
    /// A terminal plan's style: "table" or "graphic".
    #[serde(default)]
    style: Option<String>,
}

impl Command for NewpageParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let kind = rules::new_page_kind(self.generated.as_deref(), self.style.as_deref(), self.strip).map_err(|e| err(e.to_string()))?;
        let default_title = match (kind.generated, kind.style.as_deref()) {
            (Some(ecad_model::GenKind::TerminalBlocks), Some(ecad_electrical::defaults::strip_pages::GRAPHIC)) => crate::terminal_strips::draw::TITLE,
            (g, _) => g.map_or("new", |g| g.title()),
        };
        let (page, number) = on_project(s, |p, pg, _| {
            let (page_type, type_title) = rules::new_page_type(p.ledger.document(), &kind, self.after, self.ty.as_deref())?;
            let title = self.title.or(type_title).unwrap_or_else(|| default_title.to_string());
            let (id, number, mut ops) = crate::pages::commands::sheets::new_page_ops_at(p, title, self.number, self.folder, self.after)?;
            rules::stamp_new_page(&mut ops, &kind, page_type.as_ref());
            p.apply_edit(pg, "newpage", "New page", ops)?;
            p.pages.entry(id).or_default();
            Ok((id, number))
        })?;
        let mut v = open(s, page)?;
        v["number"] = json!(number);
        Ok(v)
    }
}

/// `openpage {page}`: open the page, or bring it to the front.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OpenpageParams {
    /// The page.
    page: PageId,
}

impl Command for OpenpageParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        open(s, self.page)
    }
}

/// `ecadinspect {seq?}`: a read-only snapshot of the project, as it is or at step `seq`.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct EcadinspectParams {
    /// The step to show the project at (default: as it is).
    #[serde(default)]
    seq: Option<u64>,
}

impl Command for EcadinspectParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        on_any_page(s, |p, pg| p.snapshot(&pg.as_ref(), self.seq).map_err(CmdError::refused))
    }
}
