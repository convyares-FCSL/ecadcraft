//! `titleblockimport`: a new frame from a DXF, or a logo into a title
//! block's item. It reads the DXF with CADCraft's reader ([`crate::title_blocks::dxf`]), so it is the
//! engine's; the other title block commands are generic ([`ecad_drawing_style::title_blocks::commands`]).

use cadcraft_engine::{CommandSpec, Session};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::commands::err;
use crate::commands::input::{Command, Typed};

/// The command of this module with a typed input.
pub(crate) const TYPED: &[Typed] = &[Typed::of::<TitleblockimportParams>("titleblockimport")];

/// The command of this module.
pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec::new("titleblockimport", "Import Title Block", crate::commands::input::typed::<TitleblockimportParams>).noundo()]
}

/// A new frame from a DXF drawing (`{path | dxf, sheet?, name?}`), or a DXF drawing put into a logo item (`{id |
/// name, item, path | dxf}`). One transaction.
// The import itself is the DXF reader's ([`crate::title_blocks::dxf::run_import`]
// drawing code), which still takes JSON: [`Command::run`] writes the read parameters back
// (`serde_json::to_value`) and calls it, so the input is checked strictly here. The import may take
// this struct directly later.
#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct TitleblockimportParams {
    /// The DXF file to read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    /// The DXF file's text (instead of `path`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    dxf: Option<String>,
    /// A new frame: the sheet it is drawn for (default the project's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sheet: Option<String>,
    /// A new frame: its name; a logo: the frame (or `id`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    /// A logo: the frame (an id or a name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    /// A logo: the logo item's number (from 0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    item: Option<u64>,
}

impl Command for TitleblockimportParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let params = serde_json::to_value(&self).map_err(|e| err(e.to_string()))?;
        crate::title_blocks::dxf::run_import(s, &params)
    }
}
