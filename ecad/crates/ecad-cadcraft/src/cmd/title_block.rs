//! Brief 055's commands (design 055 §9): `titleblocks`, `titleblock` and `titleblockimport`.
//!
//! # Semantics
//! A title block is a `frame` style record (050a) with its block as the record's body (FP1).
//! Every writing call is one transaction (`SetStyle`), undoable like any other, and refused with
//! writes off (the MCP policy). Bodies are checked against design 055 §1.3 before anything is
//! written; refusals say what is allowed. Choosing a frame stays `setdefault print.frame.style` /
//! `setprop`; deleting stays `deletestyle` with a replacement.
//!
//! `titleblocks` and `titleblock` are thin shims (brief 070): their params structs read the JSON,
//! the rules in [`ecad_electrical::cmd::title_block`] decide, and the shim lays out built-in blocks
//! with this machine's fonts, commits the ops and answers in JSON. `titleblockimport` reads its
//! input through [`TitleblockimportParams`] and runs the DXF reader's own
//! ([`crate::title_block::dxf::run_import`]).

use std::collections::{BTreeMap, BTreeSet};

use cadcraft_engine::{CommandSpec, Session};
use ecad_electrical::CmdError;
use ecad_electrical::cmd::title_block::{self as rules, FrameEdit, NewFrame};
use ecad_model::sheet::{self, Margins, SheetSpec};
use ecad_model::style::StyleRef;
use ecad_model::title_block::TitleBlockLayout;
use ecad_model::{Document, PageId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::cmd::input::{Command, Typed};
use crate::cmd::style::{FieldParam, field_inputs};
use crate::cmd::{err, on_project};
use crate::frame::FrameText;
use crate::styles::{self, Layer};
use crate::title_block::{bind, house};

pub use rules::{BLANK_SIZE, BUILTIN_READ_ONLY, NOT_A_FRAME};

/// The commands of this module with a typed input.
pub const TYPED: &[Typed] = &[
    Typed::of::<TitleblocksParams>("titleblocks"),
    Typed::of::<TitleblockParams>("titleblock"),
    Typed::of::<TitleblockimportParams>("titleblockimport"),
];

/// The commands of this module, registered through [`crate::lanes`].
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("titleblocks", "Title Blocks", crate::cmd::input::typed::<TitleblocksParams>)
            .params("{} (read only): every frame with its sheet, items, users and keys")
            .noundo(),
        CommandSpec::new("titleblock", "Title Block", crate::cmd::input::typed::<TitleblockParams>)
            .params(
                "{id | name, sheet?} (read) | {id | name, name?: new name, fields?: {key: value|null}, body?: layout} (write) | \
{new: {from: id|\"blank\", sheet, name?, layer?}} (a new own frame); one transaction",
            )
            .noundo(),
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

/// The pages whose frame is `id`, or whose title block is borrowed from it.
pub fn pages_using(doc: &Document, id: &StyleRef) -> Vec<PageId> {
    doc.page_order.iter().copied().filter(|p| sheet::frame_style(doc, *p) == *id || sheet::title_block_style(doc, *p) == *id).collect()
}

/// The margins of frame `id` (its field, else 019's).
fn margins(doc: &Document, id: &StyleRef) -> Margins {
    sheet::frame_margins(doc, id).unwrap_or_default()
}

/// The layout frame `id` draws on `spec`: its own, or the house block for that sheet.
pub fn layout_of(doc: &Document, id: &StyleRef, spec: &SheetSpec) -> TitleBlockLayout {
    match sheet::frame_block(doc, id) {
        Some(l) => l.clone(),
        None => house::house(crate::title_block::frame_width(spec, &margins(doc, id)), &FrameText::of(doc)),
    }
}

/// The project keys `layout` reads beyond the built-in five (design 055 §7: Settings ▸ Project
/// info lists them).
pub fn own_keys(layout: &TitleBlockLayout) -> BTreeSet<String> {
    let (keys, _) = bind::keys(layout);
    keys.into_iter().filter(|k| !bind::PROJECT_KEYS.contains(&k.as_str())).collect()
}

/// The sheet frame `id` is drawn for (`page` for the page's own).
fn frame_sheet_text(doc: &Document, id: &StyleRef) -> String {
    sheet::frame_spec(doc, id).map_or_else(|| crate::styles::frame::PAGE_SHEET.to_string(), |s| s.text())
}

/// One row of `titleblocks`.
fn row_json(doc: &Document, row: &styles::Row) -> Value {
    let spec = sheet::frame_spec(doc, &row.id).unwrap_or(doc.settings.sheet());
    let layout = layout_of(doc, &row.id, &spec);
    let keys: Vec<String> = own_keys(&layout).into_iter().collect();
    json!({
        "id": row.id.text(),
        "name": row.name,
        "layer": row.layer.word(),
        "builtin": row.layer == Layer::Builtin,
        "sheet": frame_sheet_text(doc, &row.id),
        "items": layout.items.len(),
        "users": {"pages": pages_using(doc, &row.id).len()},
        "keys": keys,
    })
}

/// `titleblocks {}` (read only): every frame with its sheet, items, users and keys.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(extend("properties" = {}))]
pub(crate) struct TitleblocksParams {}

impl Command for TitleblocksParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        on_project(s, |p, _, _| {
            let doc = p.ledger.document();
            let rows: Vec<Value> = styles::rows(doc, &crate::styles::frame::CATALOGUE).iter().map(|r| row_json(doc, r)).collect();
            // The project keys every frame in use reads (Settings ▸ Project info).
            let mut keys = BTreeSet::new();
            let mut seen = BTreeSet::new();
            for page in &doc.page_order {
                let id = sheet::frame_style(doc, *page);
                if seen.insert(id.text()) {
                    keys.extend(own_keys(&layout_of(doc, &id, &sheet::sheet_of(doc, *page))));
                }
            }
            Ok(json!({"title_blocks": rows, "keys": keys.into_iter().collect::<Vec<_>>()}))
        })
    }
}

/// The JSON `titleblock {id}` returns.
fn block_json(doc: &Document, id: &StyleRef, spec: &SheetSpec) -> Result<Value, CmdError> {
    let cat = &crate::styles::frame::CATALOGUE;
    let row = styles::rows(doc, cat).into_iter().find(|r| r.id == *id).ok_or(CmdError::refused(NOT_A_FRAME))?;
    let mut v = crate::cmd::style::style_json(doc, cat, &row, false);
    let layout = layout_of(doc, id, spec);
    v["builtin"] = json!(row.layer == Layer::Builtin);
    v["body"] = serde_json::to_value(&layout).map_err(|e| CmdError::refused(e.to_string()))?;
    v["users"] = json!({"pages": pages_using(doc, id).len()});
    v["keys"] = json!(own_keys(&layout).into_iter().collect::<Vec<_>>());
    Ok(v)
}

/// A body from a call's `body`, checked against design 055 §1.3.
///
/// # Errors
/// The body does not read, or breaks a bound, in words.
pub fn body_param(v: &Value) -> Result<TitleBlockLayout, CmdError> {
    let l: TitleBlockLayout =
        serde_json::from_value(v.clone()).map_err(|e| CmdError::input(format!("The body does not read as a title block: {e}.")))?;
    rules::check_body(l)
}

/// A new own frame (design 055 §4.6).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct NewParams {
    /// The frame it starts from (an id or a name), or "blank" (default "sheet").
    #[serde(default)]
    from: Option<String>,
    /// The sheet it is drawn for (default the project's).
    #[serde(default)]
    sheet: Option<String>,
    /// Its name (default from the sheet's size: "A3 title block").
    #[serde(default)]
    name: Option<String>,
    /// project (the default) or standard.
    #[serde(default)]
    layer: Option<String>,
}

/// `titleblock {id | name, sheet?}` reads a frame; `{id | name, name? | rename?, fields?, body?}`
/// writes one; `{new: {from, sheet, name?, layer?}}` makes a new own frame. One transaction.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct TitleblockParams {
    /// The frame (an id or a name).
    #[serde(default)]
    id: Option<String>,
    /// With `id`: the frame's new name; without: the frame (a name or an id).
    #[serde(default)]
    name: Option<String>,
    /// Without `id`: the frame's new name.
    #[serde(default)]
    rename: Option<String>,
    /// The sheet the block is shown for (default the project's).
    #[serde(default)]
    sheet: Option<String>,
    /// The fields to set: {key: value | null} (null: back to the inherited value).
    #[serde(default)]
    fields: Option<BTreeMap<String, Option<FieldParam>>>,
    /// The block's layout (as `titleblock {id}` gives it).
    #[serde(default)]
    #[schemars(with = "Option<serde_json::Map<String, Value>>")]
    body: Option<Value>,
    /// A new own frame.
    #[serde(default)]
    new: Option<NewParams>,
}

impl Command for TitleblockParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        if let Some(new) = self.new {
            return run_new(s, new);
        }
        // With `id`, `name` renames (design 055 §9); without, it names the frame.
        let (frame_name, rename) = match &self.id {
            Some(_) => (None, self.name),
            None => (self.name, self.rename),
        };
        let body = self.body.filter(|b| !b.is_null());
        let writes = body.is_some() || self.fields.is_some() || rename.is_some();
        let id_text = self.id;
        let sheet_text = self.sheet;
        let fields = self.fields;
        on_project(s, |p, pg, _| {
            let doc = p.ledger.document();
            let id = rules::frame(doc, id_text.as_deref(), frame_name.as_deref())?;
            if !writes {
                let spec = rules::sheet(doc, sheet_text.as_deref())?;
                return block_json(doc, &id, &spec);
            }
            // The body is read when the frame is known to be an own one, as before.
            let body = match body {
                Some(b) if !id.is_builtin() && rules::layer_of(doc, &id).is_some() => Some(body_param(&b)?),
                _ => None,
            };
            let edit = FrameEdit { rename: rename.map(|n| n.trim().to_string()), fields: field_inputs(fields), body };
            let plan = rules::plan_edit(doc, &id, edit)?;
            let changed = !plan.ops.is_empty();
            if changed {
                p.apply_edit(pg, "titleblock", &plan.label, plan.ops)?;
            }
            let doc = p.ledger.document();
            let spec = rules::sheet(doc, sheet_text.as_deref())?;
            let mut v = block_json(doc, &id, &spec)?;
            v["changed"] = json!(changed);
            Ok(v)
        })
    }
}

/// `titleblock {new: {from, sheet, name?, layer?}}` (design 055 §4.6).
fn run_new(s: &mut Session, new: NewParams) -> cadcraft_engine::Result<Value> {
    on_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let spec = rules::sheet(doc, new.sheet.as_deref())?;
        let layer = rules::new_layer(doc, new.layer.as_deref())?;
        let source = rules::new_source(doc, new.from.as_deref())?;
        // Own frames keep their block as made: items stay at their place from the bottom-right
        // corner; built-ins are computed for the chosen sheet.
        let layout = match &source {
            Some(src) => layout_of(doc, src, &spec),
            None => TitleBlockLayout::blank(BLANK_SIZE.0, BLANK_SIZE.1),
        };
        let name = rules::new_name(doc, new.name.as_deref(), &spec)?;
        let frame = NewFrame { name: name.clone(), spec, layer, layout, source: source.as_ref() };
        let (ops, id) = rules::plan_new(doc, &mut p.ids, frame)?;
        p.apply_edit(pg, "titleblock", &format!("New title block {name}"), ops)?;
        Ok(json!({"changed": true, "id": id.text(), "name": name}))
    })
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
