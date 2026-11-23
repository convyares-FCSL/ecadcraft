//! The generic folder commands: `newfolder`, `renamefolder`, `movefolder` and `deletefolder`. Their
//! rules are [`super::sheets`].

use ecad_model::FolderId;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use super::sheets::{delete_folder, folder_name, move_folder, new_folder, rename_folder};
use crate::commands::input::nullable;
use crate::commands::run::{Cx, Generic, Outcome};
use crate::error::CmdError;

/// A new folder, last among its siblings.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NewfolderParams {
    /// The folder's name: 1 to 100 characters.
    name: String,
    /// The folder it goes in (absent or null: the top level).
    #[serde(default)]
    parent: Option<FolderId>,
}

/// Renames a folder; the same name changes nothing.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RenamefolderParams {
    /// The folder.
    folder: FolderId,
    /// Its new name: 1 to 100 characters.
    name: String,
}

/// Moves a folder into another folder or to the top level.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MovefolderParams {
    /// The folder.
    folder: FolderId,
    /// The folder it goes in, or null for the top level.
    #[serde(deserialize_with = "nullable")]
    #[schemars(required, with = "crate::commands::input::OrNull<FolderId>")]
    parent: Option<Option<FolderId>>,
    /// Its place among its new siblings, 1 = first (default last).
    #[serde(default)]
    index: Option<usize>,
}

/// Deletes a folder; its pages and sub-folders move to its parent.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeletefolderParams {
    /// The folder.
    folder: FolderId,
}

impl Generic for NewfolderParams {
    const ID: &'static str = "newfolder";
    const TITLE: &'static str = "New Folder";

    fn check(&self, _registry: &ecad_model::registry::data::Registry) -> Result<(), CmdError> {
        folder_name(&self.name).map(|_| ())
    }

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let name = folder_name(&self.name)?;
        let (id, op) = new_folder(cx.doc, cx.ids, &name, self.parent)?;
        Ok(Outcome::edit("New folder", vec![op], json!({"folder": id.hex(), "name": name})))
    }
}

impl Generic for RenamefolderParams {
    const ID: &'static str = "renamefolder";
    const TITLE: &'static str = "Rename Folder";

    fn check(&self, _registry: &ecad_model::registry::data::Registry) -> Result<(), CmdError> {
        folder_name(&self.name).map(|_| ())
    }

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let folder = self.folder;
        let name = folder_name(&self.name)?;
        Ok(match rename_folder(cx.doc, folder, &name)? {
            None => Outcome::answer(json!({"folder": folder.hex(), "name": name, "changed": false})),
            Some(op) => Outcome::edit("Rename folder", vec![op], json!({"folder": folder.hex(), "name": name, "changed": true})),
        })
    }
}

impl Generic for MovefolderParams {
    const ID: &'static str = "movefolder";
    const TITLE: &'static str = "Move Folder";

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let (folder, parent, index) = (self.folder, self.parent.flatten(), self.index);
        let ops = move_folder(cx.doc, folder, parent, index)?;
        let changed = !ops.is_empty();
        Ok(Outcome::edit_if_any("Move folder", ops, json!({"folder": folder.hex(), "changed": changed})))
    }
}

impl Generic for DeletefolderParams {
    const ID: &'static str = "deletefolder";
    const TITLE: &'static str = "Delete Folder";

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let folder = self.folder;
        let done = delete_folder(cx.doc, folder)?;
        let answer = json!({"folder": folder.hex(), "pages_moved": done.pages, "message": done.message});
        Ok(Outcome::edit("Delete folder", done.ops, answer).echo(Some(done.message)))
    }
}
