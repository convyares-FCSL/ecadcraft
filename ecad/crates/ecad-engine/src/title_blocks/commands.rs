//! `titleblockimport` (brief 055, design 055 §9): a new frame from a DXF, or a logo into a title
//! block's item. It reads the DXF with CADCraft's reader ([`crate::title_block::dxf`]), so it is the
//! engine's; the other title block commands are generic ([`ecad_electrical::cmd::title_block`]).

use cadcraft_engine::{CommandSpec, Session};
use ecad_electrical::CmdError;
use ecad_electrical::cmd::title_block as rules;
use ecad_model::Document;
use ecad_model::style::StyleRef;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::cmd::err;
use crate::cmd::input::{Command, Typed};

/// The command of this module with a typed input.
pub const TYPED: &[Typed] = &[Typed::of::<TitleblockimportParams>("titleblockimport")];

/// The command of this module.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("titleblockimport", "Import Title Block", crate::cmd::input::typed::<TitleblockimportParams>)
            .params("{path | dxf, sheet, name?} (a new frame from a DXF) | {id | name, item, path | dxf} (a logo into item); one transaction")
            .noundo(),
    ]
}

/// The frame a JSON call names (`id` or `name`): `titleblockimport` still reads its JSON by hand.
pub(crate) fn target(doc: &Document, params: &Value) -> Result<StyleRef, CmdError> {
    let text = |k: &str| params.get(k).and_then(Value::as_str);
    rules::frame(doc, text("id"), text("name"))
}

/// `titleblockimport {path | dxf, sheet?, name?}` makes a new frame from a DXF drawing;
/// `{id | name, item, path | dxf}` puts a DXF drawing into a logo item. One transaction.
///
/// The import itself is the DXF reader's ([`crate::title_block::dxf::run_import`], lane 066d's
/// drawing code), which still takes JSON: [`Command::run`] writes the read parameters back
/// (`serde_json::to_value`) and calls it, so the input is checked strictly here. Lane 066d may take
/// this struct directly later.
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
        crate::title_block::dxf::run_import(s, &params)
    }
}
