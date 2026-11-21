//! The page type and folder commands (brief 054, design 054): `folderprops`, `renumberpages` and
//! `deletepage`. The rules (folder starts, renumbering, check rows) are [`crate::page_types`].
//!
//! Each command is a thin shim (brief 070): its params struct reads the JSON, the rules in
//! [`ecad_electrical::cmd::page_type`] decide, and the shim brings the outputs up to date, opens
//! the page shown, commits the ops, echoes and answers in JSON.

use cadcraft_engine::{CommandSpec, Session};
use ecad_electrical::cmd::page_type::{self as rules, MAX_PAGES};
use ecad_model::{FolderId, PageId};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::cmd::input::{Bounded, Command, Typed, nullable};
use crate::cmd::{err, on_project};
use crate::generated;

/// The commands of this module with a typed input.
pub const TYPED: &[Typed] =
    &[Typed::of::<FolderpropsParams>("folderprops"), Typed::of::<RenumberpagesParams>("renumberpages"), Typed::of::<DeletepageParams>("deletepage")];

/// The commands this module adds (registered in [`crate::lanes`]).
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("folderprops", "Folder Properties", crate::cmd::input::typed::<FolderpropsParams>)
            .params("{folder: id, start?: \"100\"|null} (a folder's start number: 1 to 6 digits, leading zeros set the width; null clears; no start: read only)")
            .noundo(),
        CommandSpec::new("renumberpages", "Renumber Pages", crate::cmd::input::typed::<RenumberpagesParams>)
            .params("{folder?: id} (renumbers each folder from its start number, the rest from 1; with folder, that folder and its sub-folders only; one transaction)")
            .noundo(),
        CommandSpec::new("deletepage", "Delete Page", crate::cmd::input::typed::<DeletepageParams>)
            .params("{page: id} or {pages: [id]} (generated pages only: a cover, a contents, a report…; one transaction)")
            .noundo(),
    ]
}

/// `folderprops {folder, start?}`: a folder's start number (1 to 6 digits, leading zeros set the
/// width); null or blank clears it; no `start` reads it.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct FolderpropsParams {
    /// The folder.
    folder: FolderId,
    /// The start number ("100"), or null to clear it.
    #[serde(default, deserialize_with = "nullable")]
    start: Option<Option<String>>,
}

impl Command for FolderpropsParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let start = match self.start {
            None => None,
            Some(None) => Some(None),
            Some(Some(t)) => Some(rules::start_value(&t).map_err(|e| err(e.to_string()))?),
        };
        let folder = self.folder;
        on_project(s, |p, pg, _| {
            let planned = rules::plan_folderprops(p.ledger.document(), folder, start)?;
            let name = planned.name;
            let Some((after, ops)) = planned.write else {
                return Ok(json!({"folder": folder.hex(), "name": name, "start": planned.before}));
            };
            if ops.is_empty() {
                return Ok(json!({"folder": folder.hex(), "name": name, "start": after, "changed": false}));
            }
            p.apply_edit(pg, "folderprops", "Folder start number", ops)?;
            let line = rules::start_words(&name, after.as_deref());
            pg.echo(line.clone());
            Ok(json!({"folder": folder.hex(), "name": name, "start": after, "changed": true, "message": line}))
        })
    }
}

/// `renumberpages {folder?}`: renumbers each folder from its start number, the rest from 1; with
/// `folder`, that folder and its sub-folders only (one transaction).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RenumberpagesParams {
    /// Only this folder and its sub-folders.
    #[serde(default)]
    folder: Option<FolderId>,
}

impl Command for RenumberpagesParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        on_project(s, |p, pg, _| {
            let scope = rules::folder_scope(p.ledger.document(), self.folder)?;
            if generated::has_bom_page(p.ledger.document()) {
                p.outputs();
            }
            let doc = p.ledger.document();
            let plan = rules::plan_renumber(doc, p.kept_outputs(), scope);
            if plan.ops.is_empty() {
                let line = "Page numbers already in order.".to_string();
                pg.echo(line.clone());
                return Ok(json!({"renumbered": 0, "message": line, "overlaps": plan.overlaps}));
            }
            let count = plan.pages.len();
            let pages: Vec<Value> = plan.pages.iter().map(|(pg, n)| json!({"page": pg.hex(), "number": n})).collect();
            p.apply_edit(pg, "renumberpages", "Renumber pages", plan.ops)?;
            let line = format!("Renumbered {count} page{}.", if count == 1 { "" } else { "s" });
            pg.echo(line.clone());
            for o in &plan.overlaps {
                pg.echo(o.clone());
            }
            Ok(json!({"renumbered": count, "pages": pages, "message": line, "overlaps": plan.overlaps}))
        })
    }
}

/// `deletepage {page | pages}`: generated pages only (a cover, a contents, a report…), in one
/// transaction.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct DeletepageParams {
    /// The page.
    #[serde(default)]
    page: Option<PageId>,
    /// Several pages (at most 1000).
    #[serde(default)]
    pages: Option<Bounded<PageId, 0, MAX_PAGES>>,
}

impl Command for DeletepageParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let ids: Vec<PageId> = match (self.pages, self.page) {
            (Some(pages), _) => pages.0,
            (None, Some(page)) => vec![page],
            (None, None) => return Err(err("`page` (hex id) is required")),
        };
        on_project(s, |p, pg, active| {
            let plan = rules::plan_deletepage(p.ledger.document(), &ids, active)?;
            if let Some(to) = plan.show {
                p.open_page(pg, to).map_err(ecad_electrical::CmdError::refused)?;
            }
            p.apply_edit(pg, "deletepage", "Delete page", plan.ops)?;
            pg.echo(plan.message.clone());
            Ok(json!({"deleted": ids.iter().map(|p| p.hex()).collect::<Vec<_>>(), "message": plan.message}))
        })
    }
}
