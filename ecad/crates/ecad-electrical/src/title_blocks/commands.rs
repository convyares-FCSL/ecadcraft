//! The rules of the title block commands: which frame `titleblock` names, the record a
//! write or a new frame makes, and the checks a body passes. A title block is a `frame` style
//! record with its block as the record's body. The block's layout and drawing are
//! [`crate::title_blocks`]'s. The JSON answers (`titleblocks_json`, `block_json`) take the frame
//! text styles, so the engine crate's shims pass this machine's fonts, commit the ops and answer.

use std::collections::{BTreeMap, BTreeSet};

use ecad_model::document::sheet::{self, FRAME_CATALOGUE, FRAME_SHEET_FIELD, Margins, SheetSpec};
use ecad_model::objects::styles::{StyleBody, StyleLayer, StyleRef, is_name};
use ecad_model::objects::title_block::TitleBlockLayout;
use ecad_model::{Document, IdAllocator, Op, PageId};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::commands::run::{Access, Cx, Generic, Outcome};
use crate::error::CmdError;
use crate::styles;
use crate::styles::commands::styles::{FieldInput, FieldParam, STANDARD_FIRST, STYLE_NEEDS_NAME, field_inputs, fields};
use crate::title_blocks::{bind, house};
use ecad_model::look::FrameText;

/// Refusals, in words.
pub const BUILTIN_READ_ONLY: &str = "Built-in. Duplicate it to make your own (`titleblock {new: {from: \"sheet\", sheet}}`).";
pub const NOT_A_FRAME: &str = "Name a frame: `id` or `name` (`titleblocks` lists them).";
/// A blank block's size (mm).
pub(crate) const BLANK_SIZE: (f64, f64) = (180.0, 40.0);

/// The frame catalogue.
fn catalogue() -> &'static ecad_model::registry::styles::Catalogue {
    &styles::frame::CATALOGUE
}

/// The frame `id` names, else the one `name` names (trimmed).
///
/// # Errors
/// Neither given, or no such frame.
pub fn frame(doc: &Document, id: Option<&str>, name: Option<&str>) -> Result<StyleRef, CmdError> {
    let t = id.or(name).map(str::trim).ok_or(CmdError::refused(NOT_A_FRAME))?;
    styles::find(doc, &styles::frame::CATALOGUE, t)
}

/// The sheet `text` names, else the project's.
///
/// # Errors
/// A sheet that does not read.
pub fn sheet(doc: &Document, text: Option<&str>) -> Result<SheetSpec, CmdError> {
    match text.map(str::trim) {
        Some(t) => Ok(SheetSpec::parse(t)?),
        None => Ok(doc.settings().sheet()),
    }
}

/// The layer that holds own frame `id` (the project's when both do).
pub fn layer_of(doc: &Document, id: &StyleRef) -> Option<StyleLayer> {
    [StyleLayer::Project, StyleLayer::Standard].into_iter().find(|l| ecad_model::registry::styles::record(doc, *l, FRAME_CATALOGUE, id).is_some())
}

/// A body, checked against the title block bounds.
///
/// # Errors
/// A bound broken (an unknown item kind does not read).
pub(crate) fn check_body(l: TitleBlockLayout) -> Result<TitleBlockLayout, CmdError> {
    l.check().map_err(CmdError::refused)?;
    Ok(l)
}

/// The refusal for a frame name in use.
fn name_taken(n: &str) -> CmdError {
    CmdError::refused(format!("A frame style is already called {n:?}; choose another name."))
}

/// A write to an own frame: a new name, fields and a body.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FrameEdit {
    /// The new name (trimmed); the same name is no rename.
    pub rename: Option<String>,
    pub(crate) fields: Vec<(String, FieldInput)>,
    pub body: Option<TitleBlockLayout>,
}

/// What a `titleblock` write records: one `SetStyle` (none when nothing changes).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EditPlan {
    pub ops: Vec<Op>,
    pub label: String,
}

/// `titleblock {id | name, name? | rename?, fields?, body?}`: own frame `id` with a new name,
/// fields and body, as one `SetStyle`.
///
/// # Errors
/// A built-in frame, no such own frame, a field or name that does not read, a name in use, or a
/// record the catalogue refuses.
pub(crate) fn plan_edit(doc: &Document, id: &StyleRef, edit: FrameEdit) -> Result<EditPlan, CmdError> {
    if id.is_builtin() {
        return Err(CmdError::refused(BUILTIN_READ_ONLY));
    }
    let cat = catalogue();
    let layer = layer_of(doc, id).ok_or(CmdError::refused(NOT_A_FRAME))?;
    let before = ecad_model::registry::styles::record(doc, layer, FRAME_CATALOGUE, id).cloned();
    let fields = fields(doc, cat, edit.fields)?;
    let current = ecad_model::registry::styles::name(doc, &crate::styles::frame::CATALOGUE, id);
    let rename = edit.rename.as_deref().filter(|n| *n != current);
    if let Some(n) = rename {
        if !is_name(n) {
            return Err(CmdError::refused(STYLE_NEEDS_NAME));
        }
        if ecad_model::registry::styles::rows(doc, cat).iter().any(|r| r.id != *id && r.name.eq_ignore_ascii_case(n)) {
            return Err(name_taken(n));
        }
    }
    let mut after = styles::with_fields(&styles::frame::CATALOGUE, before.as_ref(), rename, &fields)?;
    if let (Some(b), StyleBody::Frame(f)) = (edit.body, &mut after.body) {
        f.title_block = Some(b);
    }
    styles::check_record(doc, cat, id, layer, &after)?;
    let name = rename.map_or(current, str::to_string);
    let ops = if before.as_ref() == Some(&after) {
        Vec::new()
    } else {
        vec![Op::SetStyle { layer, catalogue: FRAME_CATALOGUE, id: id.clone(), before, after: Some(after) }]
    };
    Ok(EditPlan { ops, label: format!("Title block {name}") })
}

/// The layer a new frame goes in: `project` (the default) or `standard` (when the project has
/// one).
///
/// # Errors
/// A standard layer without a standard, or another word.
pub(crate) fn new_layer(doc: &Document, word: Option<&str>) -> Result<StyleLayer, CmdError> {
    match word.map(str::trim) {
        None | Some("project") => Ok(StyleLayer::Project),
        Some("standard") if doc.standard().is_some() => Ok(StyleLayer::Standard),
        Some("standard") => Err(CmdError::refused(STANDARD_FIRST)),
        Some(other) => Err(CmdError::refused(format!("Unknown layer {other:?}: project or standard."))),
    }
}

/// The frame a new one starts from: `None` for `blank` (any case), else the frame `from` names
/// (default `sheet`).
///
/// # Errors
/// No such frame.
pub(crate) fn new_source(doc: &Document, from: Option<&str>) -> Result<Option<StyleRef>, CmdError> {
    let from = from.map(str::trim).unwrap_or("sheet");
    if from.eq_ignore_ascii_case("blank") {
        return Ok(None);
    }
    styles::find(doc, &styles::frame::CATALOGUE, from).map(Some)
}

/// A new frame's name: the one given, else a unique one from the sheet's size (`A3 title block`,
/// `A3 title block 2`…).
///
/// # Errors
/// A name that does not read, or one in use.
pub(crate) fn new_name(doc: &Document, given: Option<&str>, spec: &SheetSpec) -> Result<String, CmdError> {
    let rows = ecad_model::registry::styles::rows(doc, catalogue());
    match given.map(str::trim) {
        Some(n) if is_name(n) => {
            if rows.iter().any(|r| r.name.eq_ignore_ascii_case(n)) {
                return Err(name_taken(n));
            }
            Ok(n.to_string())
        }
        Some(_) => Err(CmdError::refused(STYLE_NEEDS_NAME)),
        None => {
            let size = spec.preset_name().map_or_else(|| format!("{}×{}", spec.width, spec.height), str::to_string);
            let base = format!("{size} title block");
            let taken: BTreeSet<String> = rows.into_iter().map(|r| r.name.to_lowercase()).collect();
            if !taken.contains(&base.to_lowercase()) {
                return Ok(base);
            }
            Ok((2..10_000).map(|n| format!("{base} {n}")).find(|n| !taken.contains(&n.to_lowercase())).unwrap_or(base))
        }
    }
}

/// A new own frame.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NewFrame<'a> {
    pub name: String,
    pub spec: SheetSpec,
    pub layer: StyleLayer,
    /// Its block: the source's, as made for the sheet, or a blank one.
    pub layout: TitleBlockLayout,
    /// The frame it copies the fields of.
    pub source: Option<&'a StyleRef>,
}

/// `titleblock {new: {from, sheet, name?, layer?}}`: one `SetStyle` creating the frame, and its
/// id.
///
/// # Errors
/// A record the catalogue refuses.
pub(crate) fn plan_new(doc: &Document, ids: &mut IdAllocator, new: NewFrame<'_>) -> Result<(Vec<Op>, StyleRef), CmdError> {
    let cat = catalogue();
    let mut set: Vec<(String, Option<String>)> = Vec::new();
    if let Some(src) = new.source {
        for f in cat.fields {
            let v = ecad_model::registry::styles::field(doc, cat, src, f, None).text();
            if !v.is_empty() {
                set.push((f.key.to_string(), Some(v)));
            }
        }
    }
    set.push((FRAME_SHEET_FIELD.into(), Some(new.spec.text())));
    set.push(("block".into(), Some("bottom_right".into())));
    let mut rec = styles::with_fields(&styles::frame::CATALOGUE, None, Some(&new.name), &set)?;
    if let StyleBody::Frame(f) = &mut rec.body {
        f.title_block = Some(new.layout);
    }
    let id = StyleRef::Own(ids.fresh::<u128>());
    styles::check_record(doc, cat, &id, new.layer, &rec)?;
    let ops = vec![Op::SetStyle { layer: new.layer, catalogue: FRAME_CATALOGUE, id: id.clone(), before: None, after: Some(rec) }];
    Ok((ops, id))
}

/// The pages whose frame is `id`, or whose title block is borrowed from it.
pub fn pages_using(doc: &Document, id: &StyleRef) -> Vec<PageId> {
    doc.page_order().iter().copied().filter(|p| sheet::frame_style(doc, *p) == *id || sheet::title_block_style(doc, *p) == *id).collect()
}

/// The margins of frame `id` (its field, else the default).
fn margins(doc: &Document, id: &StyleRef) -> Margins {
    sheet::frame_margins(doc, id).unwrap_or_default()
}

/// The layout frame `id` draws on `spec`: its own, or the house block for that sheet in `text`'s
/// text styles (the engine passes this machine's fonts).
pub fn layout_of(doc: &Document, id: &StyleRef, spec: &SheetSpec, text: &FrameText) -> TitleBlockLayout {
    match sheet::frame_block(doc, id) {
        Some(l) => l.clone(),
        None => house::house(ecad_model::document::sheet::frame_width(spec, &margins(doc, id)), text),
    }
}

/// The project keys `layout` reads beyond the built-in five (Settings ▸ Project
/// info lists them).
pub(crate) fn own_keys(layout: &TitleBlockLayout) -> BTreeSet<String> {
    let (keys, _) = bind::keys(layout);
    keys.into_iter().filter(|k| !bind::PROJECT_KEYS.contains(&k.as_str())).collect()
}

/// The sheet frame `id` is drawn for (`page` for the page's own).
fn frame_sheet_text(doc: &Document, id: &StyleRef) -> String {
    sheet::frame_spec(doc, id).map_or_else(|| styles::frame::PAGE_SHEET.to_string(), |s| s.text())
}

/// One row of `titleblocks`.
fn row_json(doc: &Document, row: &ecad_model::registry::styles::Row, text: &FrameText) -> Value {
    let spec = sheet::frame_spec(doc, &row.id).unwrap_or(doc.settings().sheet());
    let layout = layout_of(doc, &row.id, &spec, text);
    let keys: Vec<String> = own_keys(&layout).into_iter().collect();
    json!({
        "id": row.id.text(),
        "name": row.name,
        "layer": row.layer.word(),
        "builtin": row.layer == ecad_model::registry::styles::Layer::Builtin,
        "sheet": frame_sheet_text(doc, &row.id),
        "items": layout.items.len(),
        "users": {"pages": pages_using(doc, &row.id).len()},
        "keys": keys,
    })
}

/// The JSON `titleblocks {}` returns: every frame with its sheet, items, users and keys, and the
/// project keys every frame in use reads.
pub(crate) fn titleblocks_json(doc: &Document, text: &FrameText) -> Value {
    let rows: Vec<Value> = ecad_model::registry::styles::rows(doc, &styles::frame::CATALOGUE).iter().map(|r| row_json(doc, r, text)).collect();
    // The project keys every frame in use reads (Settings ▸ Project info).
    let mut keys = BTreeSet::new();
    let mut seen = BTreeSet::new();
    for page in doc.page_order() {
        let id = sheet::frame_style(doc, *page);
        if seen.insert(id.text()) {
            keys.extend(own_keys(&layout_of(doc, &id, &sheet::sheet_of(doc, *page), text)));
        }
    }
    json!({"title_blocks": rows, "keys": keys.into_iter().collect::<Vec<_>>()})
}

/// The JSON `titleblock {id}` returns, built-in blocks laid out in `text`'s styles.
///
/// # Errors
/// `id` is not a frame.
pub(crate) fn block_json(doc: &Document, id: &StyleRef, spec: &SheetSpec, text: &FrameText) -> Result<Value, CmdError> {
    let cat = &styles::frame::CATALOGUE;
    let row = ecad_model::registry::styles::rows(doc, cat).into_iter().find(|r| r.id == *id).ok_or(CmdError::refused(NOT_A_FRAME))?;
    let mut v = crate::styles::commands::styles::style_json(doc, cat, &row, false);
    let layout = layout_of(doc, id, spec, text);
    v["builtin"] = json!(row.layer == ecad_model::registry::styles::Layer::Builtin);
    v["body"] = serde_json::to_value(&layout).map_err(|e| CmdError::refused(e.to_string()))?;
    v["users"] = json!({"pages": pages_using(doc, id).len()});
    v["keys"] = json!(own_keys(&layout).into_iter().collect::<Vec<_>>());
    Ok(v)
}

// ---------------------------------------------------------------------------------------------
// The commands (generic: crate::commands::run). `titleblockimport` reads a DXF with
// CADCraft's reader and stays the engine's.

/// `titleblocks {}` (read only): every frame with its sheet, items, users and keys.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(extend("properties" = {}))]
pub struct TitleblocksParams {}

/// A new own frame.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NewParams {
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
pub struct TitleblockParams {
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

/// A title block's body as given (`body`), checked.
///
/// # Errors
/// A body that does not read as a title block, or fails its check.
pub(crate) fn body_param(v: &Value) -> Result<TitleBlockLayout, CmdError> {
    let l: TitleBlockLayout =
        serde_json::from_value(v.clone()).map_err(|e| CmdError::input(format!("The body does not read as a title block: {e}.")))?;
    check_body(l)
}

impl Generic for TitleblocksParams {
    const ID: &'static str = "titleblocks";
    const TITLE: &'static str = "Title Blocks";
    const HELP: &'static str = "{} (read only): every frame with its sheet, items, users and keys";
    const ACCESS: Access = Access::Reads;

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        Ok(Outcome::answer(titleblocks_json(cx.doc, &cx.look.frame_text(cx.doc))))
    }
}

impl Generic for TitleblockParams {
    const ID: &'static str = "titleblock";
    const TITLE: &'static str = "Title Block";
    const HELP: &'static str = "{id | name, sheet?} (read) | {id | name, name?: new name, fields?: {key: value|null}, body?: layout} (write) | \
{new: {from: id|\"blank\", sheet, name?, layer?}} (a new own frame); one transaction";

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        if let Some(new) = self.new {
            return new_frame(cx, new);
        }
        // With `id`, `name` renames; without, it names the frame.
        let (frame_name, rename) = match &self.id {
            Some(_) => (None, self.name),
            None => (self.name, self.rename),
        };
        let body = self.body.filter(|b| !b.is_null());
        let writes = body.is_some() || self.fields.is_some() || rename.is_some();
        let doc = cx.doc;
        let look = cx.look;
        let id = frame(doc, self.id.as_deref(), frame_name.as_deref())?;
        let sheet_text = self.sheet;
        if !writes {
            let spec = sheet(doc, sheet_text.as_deref())?;
            return Ok(Outcome::answer(block_json(doc, &id, &spec, &look.frame_text(doc))?));
        }
        // The body is read when the frame is known to be an own one, as before.
        let body = match body {
            Some(b) if !id.is_builtin() && layer_of(doc, &id).is_some() => Some(body_param(&b)?),
            _ => None,
        };
        let edit = FrameEdit { rename: rename.map(|n| n.trim().to_string()), fields: field_inputs(self.fields), body };
        let plan = plan_edit(doc, &id, edit)?;
        let changed = !plan.ops.is_empty();
        Ok(Outcome::edit_then_try(plan.label, plan.ops, move |doc| {
            let spec = sheet(doc, sheet_text.as_deref())?;
            let mut v = block_json(doc, &id, &spec, &look.frame_text(doc))?;
            v["changed"] = json!(changed);
            Ok(v)
        }))
    }
}

/// `titleblock {new: …}`: a new own frame.
fn new_frame(cx: &mut Cx<'_>, new: NewParams) -> Result<Outcome, CmdError> {
    let doc = cx.doc;
    let spec = sheet(doc, new.sheet.as_deref())?;
    let layer = new_layer(doc, new.layer.as_deref())?;
    let source = new_source(doc, new.from.as_deref())?;
    // Own frames keep their block as made: items stay at their place from the bottom-right
    // corner; built-ins are computed for the chosen sheet.
    let layout = match &source {
        Some(src) => layout_of(doc, src, &spec, &cx.look.frame_text(doc)),
        None => TitleBlockLayout::blank(BLANK_SIZE.0, BLANK_SIZE.1),
    };
    let name = new_name(doc, new.name.as_deref(), &spec)?;
    let frame = NewFrame { name: name.clone(), spec, layer, layout, source: source.as_ref() };
    let (ops, id) = plan_new(doc, cx.ids, frame)?;
    Ok(Outcome::edit(format!("New title block {name}"), ops, json!({"changed": true, "id": id.text(), "name": name})))
}
