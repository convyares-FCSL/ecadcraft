//! Brief 055's commands (design 055 §9): `titleblocks`, `titleblock` and `titleblockimport`.
//!
//! # Semantics
//! A title block is a `frame` style record (050a) with its block as the record's body (FP1).
//! Every writing call is one transaction (`SetStyle`), undoable like any other, and refused with
//! writes off (the MCP policy). Bodies are checked against design 055 §1.3 before anything is
//! written; refusals say what is allowed. Choosing a frame stays `setdefault print.frame.style` /
//! `setprop`; deleting stays `deletestyle` with a replacement.

use std::collections::BTreeSet;

use cadcraft_engine::{CommandSpec, Session};
use ecad_model::sheet::{self, FRAME_CATALOGUE, Margins, SheetSpec};
use ecad_model::style::{StyleLayer, StyleRecord, StyleRef, is_name};
use ecad_model::title_block::{StyleBody, TitleBlockLayout};
use ecad_model::{Document, Op, PageId};
use serde_json::{Value, json};

use crate::commands::with_project;
use crate::frame::FrameText;
use crate::styles::{self, Layer};
use crate::title_block::{bind, house};

/// Refusals, in words.
pub const BUILTIN_READ_ONLY: &str = "Built-in. Duplicate it to make your own (`titleblock {new: {from: \"sheet\", sheet}}`).";
pub const NOT_A_FRAME: &str = "Name a frame: `id` or `name` (`titleblocks` lists them).";
/// A blank block's size (mm).
pub const BLANK_SIZE: (f64, f64) = (180.0, 40.0);

/// The commands of this module, registered through [`crate::lanes`].
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("titleblocks", "Title Blocks", run_titleblocks)
            .params("{} (read only): every frame with its sheet, items, users and keys")
            .noundo(),
        CommandSpec::new("titleblock", "Title Block", run_titleblock)
            .params(
                "{id | name, sheet?} (read) | {id | name, name?: new name, fields?: {key: value|null}, body?: layout} (write) | \
{new: {from: id|\"blank\", sheet, name?, layer?}} (a new own frame); one transaction",
            )
            .noundo(),
        CommandSpec::new("titleblockimport", "Import Title Block", crate::title_block::dxf::run_import)
            .params("{path | dxf, sheet, name?} (a new frame from a DXF) | {id | name, item, path | dxf} (a logo into item); one transaction")
            .noundo(),
    ]
}

fn text_param<'a>(params: &'a Value, key: &str) -> Option<&'a str> {
    params.get(key).and_then(Value::as_str).map(str::trim)
}

/// The frame a call names (`id` or `name`).
pub(crate) fn target(doc: &Document, params: &Value) -> Result<StyleRef, String> {
    let t = text_param(params, "id").or_else(|| text_param(params, "name")).ok_or(NOT_A_FRAME)?;
    styles::find(doc, FRAME_CATALOGUE, t)
}

/// The layer that holds own frame `id` (the project's when both do).
fn layer_of(doc: &Document, id: &StyleRef) -> Option<StyleLayer> {
    [StyleLayer::Project, StyleLayer::Standard].into_iter().find(|l| styles::record(doc, *l, FRAME_CATALOGUE, id).is_some())
}

/// The pages whose frame is `id`, or whose title block is borrowed from it.
pub fn pages_using(doc: &Document, id: &StyleRef) -> Vec<PageId> {
    doc.page_order.iter().copied().filter(|p| sheet::frame_style(doc, *p) == *id || sheet::title_block_style(doc, *p) == *id).collect()
}

/// The sheet a call names (`sheet`), else the project's.
fn sheet_param(doc: &Document, params: &Value) -> Result<SheetSpec, String> {
    match text_param(params, "sheet") {
        Some(t) => SheetSpec::parse(t),
        None => Ok(doc.settings.sheet.check().map(|()| doc.settings.sheet).unwrap_or_default()),
    }
}

/// The margins of frame `id` (its field, else 019's).
fn margins(doc: &Document, id: &StyleRef) -> Margins {
    sheet::frame_field(doc, id, sheet::FRAME_MARGINS_FIELD).and_then(sheet::parse_margins).unwrap_or_default()
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
    sheet::frame_field(doc, id, sheet::FRAME_SHEET_FIELD)
        .and_then(sheet::parse_frame_sheet)
        .map_or_else(|| crate::styles::frame::PAGE_SHEET.to_string(), |s| s.text())
}

/// One row of `titleblocks`.
fn row_json(doc: &Document, row: &styles::Row) -> Value {
    let spec = sheet::frame_field(doc, &row.id, sheet::FRAME_SHEET_FIELD).and_then(sheet::parse_frame_sheet).unwrap_or(doc.settings.sheet);
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

/// `titleblocks {}` (read only).
fn run_titleblocks(s: &mut Session, _params: &Value) -> cadcraft_engine::Result<Value> {
    with_project(s, |p, _, _| {
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

/// The JSON `titleblock {id}` returns.
fn block_json(doc: &Document, id: &StyleRef, spec: &SheetSpec) -> Result<Value, String> {
    let cat = &crate::styles::frame::CATALOGUE;
    let row = styles::rows(doc, cat).into_iter().find(|r| r.id == *id).ok_or(NOT_A_FRAME)?;
    let mut v = crate::style_cmds::style_json(doc, cat, &row, false);
    let layout = layout_of(doc, id, spec);
    v["builtin"] = json!(row.layer == Layer::Builtin);
    v["body"] = serde_json::to_value(&layout).map_err(|e| e.to_string())?;
    v["users"] = json!({"pages": pages_using(doc, id).len()});
    v["keys"] = json!(own_keys(&layout).into_iter().collect::<Vec<_>>());
    Ok(v)
}

/// A unique frame name from `base` (`A3 title block`, `A3 title block 2`…).
fn unique_name(doc: &Document, base: &str) -> String {
    let taken: BTreeSet<String> = styles::rows(doc, &crate::styles::frame::CATALOGUE).into_iter().map(|r| r.name.to_lowercase()).collect();
    if !taken.contains(&base.to_lowercase()) {
        return base.to_string();
    }
    (2..10_000).map(|n| format!("{base} {n}")).find(|n| !taken.contains(&n.to_lowercase())).unwrap_or_else(|| base.to_string())
}

/// A body from a call's `body`, checked against design 055 §1.3.
///
/// # Errors
/// The body does not read, or breaks a bound, in words.
pub fn body_param(v: &Value) -> Result<TitleBlockLayout, String> {
    let l: TitleBlockLayout = serde_json::from_value(v.clone()).map_err(|e| format!("The body does not read as a title block: {e}."))?;
    l.check()?;
    if l.items.iter().any(|i| matches!(i, ecad_model::title_block::BlockItem::Unknown)) {
        return Err("The body holds an item of a kind this build does not know.".into());
    }
    Ok(l)
}

/// `titleblock {…}` (design 055 §9).
fn run_titleblock(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    if let Some(new) = params.get("new") {
        return run_new(s, new);
    }
    // With `id`, `name` renames (design 055 §9); without, it names the frame.
    let rename_key = if params.get("id").is_some() { "name" } else { "rename" };
    let writes = ["body", "fields"].iter().any(|k| params.get(*k).is_some_and(|v| !v.is_null())) || params.get(rename_key).is_some();
    with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let id = match text_param(params, "id") {
            Some(t) => styles::find(doc, FRAME_CATALOGUE, t)?,
            None => target(doc, params)?,
        };
        if !writes {
            let spec = sheet_param(doc, params)?;
            return block_json(doc, &id, &spec);
        }
        if id.is_builtin() {
            return Err(BUILTIN_READ_ONLY.into());
        }
        let cat = &crate::styles::frame::CATALOGUE;
        let layer = layer_of(doc, &id).ok_or(NOT_A_FRAME)?;
        let before = styles::record(doc, layer, FRAME_CATALOGUE, &id).cloned();
        let fields = crate::style_cmds::fields_param(doc, cat, params)?;
        let rename = text_param(params, rename_key).filter(|n| *n != styles::name(doc, FRAME_CATALOGUE, &id));
        if let Some(n) = rename {
            if !is_name(n) {
                return Err(crate::style_cmds::STYLE_NEEDS_NAME.into());
            }
            if styles::rows(doc, cat).iter().any(|r| r.id != id && r.name.eq_ignore_ascii_case(n)) {
                return Err(format!("A frame style is already called {n:?}; choose another name."));
            }
        }
        let mut after = styles::with_fields(before.as_ref(), rename, &fields);
        if let Some(b) = params.get("body").filter(|v| !v.is_null()) {
            after.body = Some(StyleBody::title_block(body_param(b)?));
        }
        styles::check_record(doc, cat, &id, layer, &after)?;
        let changed = before.as_ref() != Some(&after);
        if changed {
            let name = rename.map_or_else(|| styles::name(doc, FRAME_CATALOGUE, &id), str::to_string);
            let ops = vec![Op::SetStyle { layer, catalogue: FRAME_CATALOGUE.into(), id: id.clone(), before, after: Some(after) }];
            p.apply_edit(pg, "titleblock", &format!("Title block {name}"), ops)?;
        }
        let doc = p.ledger.document();
        let spec = sheet_param(doc, params)?;
        let mut v = block_json(doc, &id, &spec)?;
        v["changed"] = json!(changed);
        Ok(v)
    })
}

/// `titleblock {new: {from, sheet, name?, layer?}}` (design 055 §4.6).
fn run_new(s: &mut Session, new: &Value) -> cadcraft_engine::Result<Value> {
    with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let spec = sheet_param(doc, new)?;
        let from = text_param(new, "from").unwrap_or("sheet");
        let layer = match text_param(new, "layer") {
            None | Some("project") => StyleLayer::Project,
            Some("standard") if doc.standard.is_some() => StyleLayer::Standard,
            Some("standard") => return Err(crate::style_cmds::STANDARD_FIRST.into()),
            Some(other) => return Err(format!("Unknown layer {other:?}: project or standard.")),
        };
        let (layout, src) = if from.eq_ignore_ascii_case("blank") {
            (TitleBlockLayout::blank(BLANK_SIZE.0, BLANK_SIZE.1), None)
        } else {
            let src = styles::find(doc, FRAME_CATALOGUE, from)?;
            // Own frames keep their block as made: items stay at their place from the
            // bottom-right corner; built-ins are computed for the chosen sheet.
            (layout_of(doc, &src, &spec), Some(src))
        };
        let size = spec.preset_name().map_or_else(|| format!("{}×{}", spec.width, spec.height), str::to_string);
        let name = match text_param(new, "name") {
            Some(n) if is_name(n) => {
                if styles::rows(doc, &crate::styles::frame::CATALOGUE).iter().any(|r| r.name.eq_ignore_ascii_case(n)) {
                    return Err(format!("A frame style is already called {n:?}; choose another name."));
                }
                n.to_string()
            }
            Some(_) => return Err(crate::style_cmds::STYLE_NEEDS_NAME.into()),
            None => unique_name(doc, &format!("{size} title block")),
        };
        let cat = &crate::styles::frame::CATALOGUE;
        let mut rec = StyleRecord::new(name.clone(), Default::default());
        if let Some(src) = &src {
            for f in cat.fields {
                let v = styles::field(doc, cat, src, f, None).text();
                if !v.is_empty() {
                    rec.fields.insert(f.key.to_string(), v);
                }
            }
        }
        rec.fields.insert(sheet::FRAME_SHEET_FIELD.into(), spec.text());
        rec.fields.insert("block".into(), "bottom_right".into());
        rec.body = Some(StyleBody::title_block(layout));
        let id = StyleRef::Own(p.ids.fresh::<u128>());
        styles::check_record(doc, cat, &id, layer, &rec)?;
        let ops = vec![Op::SetStyle { layer, catalogue: FRAME_CATALOGUE.into(), id: id.clone(), before: None, after: Some(rec) }];
        p.apply_edit(pg, "titleblock", &format!("New title block {name}"), ops)?;
        Ok(json!({"changed": true, "id": id.text(), "name": name}))
    })
}
