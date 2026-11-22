//! The page commands (briefs 010, 019, 024, 028, 046, 054): `newpage`, `openpage`, `renamepage`
//! and `movepage`, and `ecadinspect` (a read-only snapshot).
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
use ecad_electrical::commands::input::nullable;

/// The commands of this module with a typed input.
pub const TYPED: &[Typed] = &[
    Typed::of::<NewpageParams>("newpage"),
    Typed::of::<OpenpageParams>("openpage"),
    Typed::of::<RenamepageParams>("renamepage"),
    Typed::of::<MovepageParams>("movepage"),
    Typed::of::<EcadinspectParams>("ecadinspect"),
];

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

/// `renamepage {page, title}` (design 010 §2.3, §7.5): one transaction, `RemovePage` + `AddPage`
/// with the same ID at the same position. The same title changes nothing.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RenamepageParams {
    /// The page.
    page: PageId,
    /// Its new title: 1 to 200 characters, no control characters.
    title: String,
}

impl Command for RenamepageParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let title = rules::page_title(&self.title).map_err(|e| err(e.to_string()))?;
        let page = self.page;
        on_project(s, |p, pg, _| {
            let done = rules::rename_page(p.ledger.document(), page, title.as_str())?;
            let changed = !done.ops.is_empty();
            if changed {
                p.apply_edit(pg, "renamepage", "Rename page", done.ops)?;
            }
            Ok(json!({"page": page.hex(), "title": done.title, "index": done.index, "changed": changed}))
        })
    }
}

/// `movepage {page, index}` (design 010 §2.3, §7.5: 1 = first, `RemovePage` at the old position +
/// `AddPage` at the new) or `{page, folder, index?}` (design 019 §5.3: into a folder, null the top
/// level, 1 = first of its pages, default last). The same position changes nothing.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct MovepageParams {
    /// The page.
    page: PageId,
    /// Its new position (1 = first); with `folder`, among the folder's pages.
    #[serde(default)]
    index: Option<usize>,
    /// The folder it moves into; null for the top level.
    #[serde(default, deserialize_with = "nullable")]
    folder: Option<Option<FolderId>>,
}

impl Command for MovepageParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let (page, index) = (self.page, self.index);
        match self.folder {
            Some(folder) => on_project(s, |p, pg, _| {
                let ops = rules::move_page_ops(p.ledger.document(), page, folder, index)?;
                if ops.is_empty() {
                    return Ok(json!({"page": page.hex(), "changed": false}));
                }
                p.apply_edit(pg, "movepage", "Move page", ops)?;
                Ok(json!({"page": page.hex(), "folder": folder.map(FolderId::hex), "changed": true}))
            }),
            None => on_project(s, |p, pg, _| {
                let done = rules::move_page_to(p.ledger.document(), page, index)?;
                let changed = !done.ops.is_empty();
                if changed {
                    p.apply_edit(pg, "movepage", "Move page", done.ops)?;
                }
                Ok(json!({"page": page.hex(), "title": done.title, "index": done.index, "changed": changed}))
            }),
        }
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
