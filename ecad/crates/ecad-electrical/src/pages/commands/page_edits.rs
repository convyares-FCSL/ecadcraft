//! The generic page commands: `renamepage` and `movepage`. Their rules are
//! [`super::sheets`]; `newpage`, `openpage` and `ecadinspect` need the session and stay the
//! engine's.

use ecad_model::{FolderId, PageId};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use super::sheets::{move_page_ops, move_page_to, page_title, rename_page};
use crate::commands::input::nullable;
use crate::commands::run::{Cx, Generic, Outcome};
use crate::error::CmdError;

/// `renamepage {page, title}` (design 010 §2.3, §7.5): one transaction, `RemovePage` + `AddPage`
/// with the same ID at the same position. The same title changes nothing.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RenamepageParams {
    /// The page.
    page: PageId,
    /// Its new title: 1 to 200 characters, no control characters.
    title: String,
}

/// `movepage {page, index}` (design 010 §2.3, §7.5: 1 = first, `RemovePage` at the old position +
/// `AddPage` at the new) or `{page, folder, index?}` (design 019 §5.3: into a folder, null the top
/// level, 1 = first of its pages, default last). The same position changes nothing.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MovepageParams {
    /// The page.
    page: PageId,
    /// Its new position (1 = first); with `folder`, among the folder's pages.
    #[serde(default)]
    index: Option<usize>,
    /// The folder it moves into; null for the top level.
    #[serde(default, deserialize_with = "nullable")]
    folder: Option<Option<FolderId>>,
}

impl Generic for RenamepageParams {
    const ID: &'static str = "renamepage";
    const TITLE: &'static str = "Rename Page";
    const HELP: &'static str = "{page: id, title}";

    fn check(&self) -> Result<(), CmdError> {
        page_title(&self.title).map(|_| ())
    }

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let title = page_title(&self.title)?;
        let page = self.page;
        let done = rename_page(cx.doc, page, title.as_str())?;
        let answer = json!({"page": page.hex(), "title": done.title, "index": done.index, "changed": !done.ops.is_empty()});
        Ok(Outcome::edit_if_any("Rename page", done.ops, answer))
    }
}

impl Generic for MovepageParams {
    const ID: &'static str = "movepage";
    const TITLE: &'static str = "Move Page";
    const HELP: &'static str = "{page: id, index: n} (1 = first) | {page: id, folder: id|null, index?: n} (into a folder, 1 = first of its pages)";

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let (page, index) = (self.page, self.index);
        match self.folder {
            Some(folder) => {
                let ops = move_page_ops(cx.doc, page, folder, index)?;
                let answer = if ops.is_empty() {
                    json!({"page": page.hex(), "changed": false})
                } else {
                    json!({"page": page.hex(), "folder": folder.map(FolderId::hex), "changed": true})
                };
                Ok(Outcome::edit_if_any("Move page", ops, answer))
            }
            None => {
                let done = move_page_to(cx.doc, page, index)?;
                let answer = json!({"page": page.hex(), "title": done.title, "index": done.index, "changed": !done.ops.is_empty()});
                Ok(Outcome::edit_if_any("Move page", done.ops, answer))
            }
        }
    }
}
