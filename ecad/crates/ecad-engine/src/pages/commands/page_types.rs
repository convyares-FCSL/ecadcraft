//! The page type and folder commands: `folderprops`, `renumberpages` and
//! `deletepage`. The rules (folder starts, renumbering, check rows) are [`ecad_electrical::pages::page_types`].
//!
//! Each command is a thin shim: its params struct reads the JSON, the rules in
//! [`ecad_electrical::pages::commands::page_types`] decide, and the shim brings the reports up to date, opens
//! the page shown, commits the ops, echoes and answers in JSON.

use cadcraft_engine::{CommandSpec, Session};
use ecad_electrical::pages::commands::page_types::{self as rules, MAX_PAGES};
use ecad_model::PageId;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::commands::input::{Command, Typed};
use crate::commands::{err, on_project};
use ecad_electrical::commands::input::Bounded;

/// The commands of this module with a typed input.
pub(crate) const TYPED: &[Typed] = &[Typed::of::<DeletepageParams>("deletepage")];

/// The commands this module adds (registered in [`crate::lanes`]).
pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec::new("deletepage", "Delete Page", crate::commands::input::typed::<DeletepageParams>).noundo()]
}

/// Deletes generated pages only (a cover, a contents, a report…): `page`, or several `pages`. One transaction.
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
