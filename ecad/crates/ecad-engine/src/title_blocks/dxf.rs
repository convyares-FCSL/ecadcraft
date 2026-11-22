//! DXF logos and title blocks (brief 055, design 055 §4.6, §6).
//!
//! # Semantics
//! - A DXF is read with CADCraft's reader; its model space is exploded (block references to a
//!   depth of [`MAX_DEPTH`]). Lines, polylines, arcs, circles, solids and texts are kept;
//!   everything else is skipped and counted.
//! - **Logo**: every kept shape becomes the logo item's primitives, fitted to its box when drawn.
//!   More than [`MAX_LOGO_PRIMS`] shapes, an empty drawing or an unreadable file are refused in
//!   words.
//! - **Import DXF**: a whole block at 1:1. Straight lines become line items, texts static text
//!   items (the user turns one into a field with Make field), and curves and solids one drawing
//!   item (a logo) over their extents. The drawing's extents must fit the frame's block area.
//!
//! Only DXF in 0.2.2 (owner, Q3): raster logos need CADCraft to write images to PDF.

use cadcraft_engine::Session;
use cadcraft_engine::doc::{Drawing, Entity, EntityKind};
use cadcraft_engine::geom::{Mat3, Vec2};
use ecad_electrical::CmdError;
use ecad_model::document::sheet::{FRAME_CATALOGUE, SheetSpec};
use ecad_model::objects::styles::{StyleLayer, StyleRecord, StyleRef};
use ecad_model::objects::title_block::{
    Align, BlockItem, LineWeight, MAX_BODY_PRIMS, MAX_ITEMS, MAX_LOGO_PRIMS, MAX_TEXT_MM, MIN_TEXT_MM, Rect, TextDraw, TitleBlockLayout, logo_words,
};
use ecad_model::{Op, Primitive, Pt};
use serde_json::{Value, json};

use crate::commands::{err, on_project};

/// Largest DXF read, in bytes (a guard on hostile input).
pub const MAX_DXF_BYTES: usize = 16 * 1024 * 1024;
/// Deepest block nesting exploded (a guard on cyclic or deep blocks).
pub const MAX_DEPTH: usize = 8;
/// Most shapes read before refusing (a guard: reading stops past it).
pub const MAX_READ: usize = 50_000;
/// Refusals, in words.
pub const EMPTY: &str = "The DXF has no drawing in it.";

/// What a DXF holds, exploded: its shapes in drawing units, and how many entities were skipped.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Read {
    pub prims: Vec<Primitive>,
    pub skipped: usize,
    /// More shapes than [`MAX_READ`]: reading stopped.
    pub over: bool,
}

/// The refusal for a file that does not read.
pub fn unreadable(reason: &str) -> String {
    format!("This file is not a DXF we can read: {reason}.")
}

/// Read `bytes` as a DXF and explode its model space.
///
/// # Errors
/// The file is too large or does not read as a DXF, in words.
pub fn read(bytes: &[u8]) -> Result<Read, String> {
    if bytes.len() > MAX_DXF_BYTES {
        return Err(unreadable(&format!("it is larger than {} MB", MAX_DXF_BYTES / (1024 * 1024))));
    }
    let d = cadcraft_io::read_dxf(bytes).map_err(|e| unreadable(e.to_string().trim_end_matches('.')))?;
    let mut out = Read::default();
    let ents: Vec<&Entity> = d.model.iter().map(|e| &**e).collect();
    walk(&d, &ents, Mat3::translate(Vec2::new(0.0, 0.0)), 0, &mut out);
    Ok(out)
}

fn push(out: &mut Read, p: Primitive) {
    if out.prims.len() >= MAX_READ {
        out.over = true;
        return;
    }
    out.prims.push(p);
}

fn walk(d: &Drawing, ents: &[&Entity], m: Mat3, depth: usize, out: &mut Read) {
    for e in ents {
        if out.over {
            return;
        }
        let mut kind = e.kind.clone();
        if let EntityKind::Insert(i) = &e.kind {
            let Some(block) = d.blocks.get(&i.block).filter(|_| depth < MAX_DEPTH) else {
                out.skipped += 1;
                continue;
            };
            let local = Mat3::translate(Vec2::new(i.insert.x, i.insert.y))
                .then_before(Mat3::rotate(i.rotation))
                .then_before(Mat3::scale(i.scale.x, i.scale.y))
                .then_before(Mat3::translate(Vec2::new(-block.base.x, -block.base.y)));
            let inner: Vec<&Entity> = block.entities.iter().map(|x| &**x).collect();
            walk(d, &inner, m.then_before(local), depth + 1, out);
            continue;
        }
        kind.transform(&m);
        match &kind {
            EntityKind::Line(_) | EntityKind::Arc(_) | EntityKind::Circle(_) | EntityKind::Text(_) => match crate::convert::primitive_from(&kind) {
                Some(p) => push(out, p),
                None => out.skipped += 1,
            },
            EntityKind::LwPolyline(p) if p.vertices.iter().all(|v| v.bulge == 0.0) => {
                push(out, Primitive::Polyline { vertices: p.vertices.iter().map(|v| Pt::new(v.p.x, v.p.y)).collect(), closed: p.closed });
            }
            EntityKind::LwPolyline(_) => {
                for prim in kind.prims() {
                    match prim {
                        cadcraft_engine::doc::Prim::Seg(cadcraft_engine::geom::Segment::Line(l)) => {
                            push(out, Primitive::Line { a: Pt::new(l.a.x, l.a.y), b: Pt::new(l.b.x, l.b.y) });
                        }
                        cadcraft_engine::doc::Prim::Seg(cadcraft_engine::geom::Segment::Arc { arc, .. }) => {
                            push(
                                out,
                                Primitive::Arc { center: Pt::new(arc.center.x, arc.center.y), radius: arc.radius, start: arc.start, end: arc.end },
                            );
                        }
                        _ => out.skipped += 1,
                    }
                }
            }
            EntityKind::Solid(s) | EntityKind::Trace(s) => {
                // DXF corner order 1, 2, 4, 3 around the outline.
                let order = [0usize, 1, 3, 2];
                let v: Vec<Pt> = order.iter().filter_map(|i| s.corners.get(*i)).map(|c| Pt::new(c.x, c.y)).collect();
                push(out, Primitive::Polyline { vertices: v, closed: true });
            }
            _ => out.skipped += 1,
        }
    }
}

/// Read `bytes` as a logo (design 055 §6).
///
/// # Errors
/// Unreadable, empty, or more than [`MAX_LOGO_PRIMS`] shapes, in words.
pub fn logo(bytes: &[u8]) -> Result<Vec<Primitive>, String> {
    let r = read(bytes)?;
    if r.over || r.prims.len() > MAX_LOGO_PRIMS {
        return Err(logo_words(if r.over { MAX_READ } else { r.prims.len() }));
    }
    if r.prims.is_empty() {
        return Err(EMPTY.into());
    }
    if ecad_model::geometry::geom::extents(&r.prims).is_none() {
        return Err(EMPTY.into());
    }
    Ok(r.prims)
}

fn mm(v: f64) -> String {
    let r = (v * 10.0).round() / 10.0;
    if r.fract() == 0.0 { format!("{r:.0}") } else { format!("{r:.1}") }
}

/// A whole title block from `bytes` at 1:1, for a frame on `spec` (design 055 §4.6). Also the
/// number of entities skipped.
///
/// # Errors
/// Unreadable, empty, too large for the block area, or past the bounds, in words.
pub fn block(bytes: &[u8], spec: &SheetSpec, margins: &ecad_model::document::sheet::Margins) -> Result<(TitleBlockLayout, usize), String> {
    let r = read(bytes)?;
    if r.over {
        return Err(format!("The DXF has more than {} shapes; simplify it first.", ecad_model::objects::title_block::group(MAX_READ)));
    }
    let (lo, hi) = ecad_model::geometry::geom::extents(&r.prims).ok_or(EMPTY)?;
    let (w, h) = (hi.x - lo.x, hi.y - lo.y);
    let (aw, ah) = ecad_electrical::title_blocks::block_area(spec, margins);
    if w > aw + 1e-6 || h > ah + 1e-6 {
        let name = ecad_electrical::title_blocks::sheet_name(spec);
        return Err(format!(
            "The drawing is {} × {} mm; the block area of {name} is {} × {} mm. Scale it in the DXF first.",
            mm(w),
            mm(h),
            mm(aw),
            mm(ah)
        ));
    }
    let size = (w.max(1.0), h.max(1.0));
    let at = |p: Pt| ((p.x - lo.x).max(0.0), (p.y - lo.y).max(0.0));
    let mut items = Vec::new();
    let mut curves = Vec::new();
    for p in r.prims {
        match &p {
            Primitive::Line { a, b } => items.push(BlockItem::Line { a: at(*a), b: at(*b), weight: LineWeight::Thin }),
            Primitive::Polyline { vertices, closed } => {
                let mut pts: Vec<Pt> = vertices.clone();
                if *closed && let Some(first) = vertices.first() {
                    pts.push(*first);
                }
                for pair in pts.windows(2) {
                    if let [a, b] = pair {
                        items.push(BlockItem::Line { a: at(*a), b: at(*b), weight: LineWeight::Thin });
                    }
                }
            }
            Primitive::Text { at: t, height, value, halign, .. } if !value.trim().is_empty() => {
                let size = height.clamp(MIN_TEXT_MM, MAX_TEXT_MM);
                let tw = (size * 0.8 * value.chars().count() as f64 + 2.0).min(size.max(1.0) * 200.0);
                let (x, y) = at(*t);
                let (x, align) = match halign {
                    ecad_model::TextHAlign::Left => (x - 1.0, Align::Left),
                    ecad_model::TextHAlign::Center => (x - tw / 2.0, Align::Centre),
                    ecad_model::TextHAlign::Right => (x - tw + 1.0, Align::Right),
                };
                let x = x.max(0.0);
                let rect = Rect::new(x, (y - size * 0.4).max(0.0), tw.max(1.0), size * 1.8);
                items.push(BlockItem::Text {
                    at: rect,
                    text: value.chars().take(ecad_model::objects::title_block::MAX_BODY_TEXT).collect(),
                    draw: TextDraw { size, align, ..TextDraw::default() },
                });
            }
            Primitive::Text { .. } => {}
            _ => curves.push(p.clone()),
        }
    }
    if !curves.is_empty() {
        if curves.len() > MAX_BODY_PRIMS {
            return Err(format!(
                "The drawing has {} curved shapes; {} at most. Simplify it first.",
                ecad_model::objects::title_block::group(curves.len()),
                ecad_model::objects::title_block::group(MAX_BODY_PRIMS)
            ));
        }
        if let Some((clo, chi)) = ecad_model::geometry::geom::extents(&curves) {
            let (x, y) = at(clo);
            let rect = Rect::new(x, y, (chi.x - clo.x).max(0.5), (chi.y - clo.y).max(0.5));
            // Chunks within the logo bound, each over the same box.
            for chunk in curves.chunks(MAX_LOGO_PRIMS) {
                items.push(BlockItem::Logo { at: rect, name: "Drawing".into(), prims: chunk.to_vec() });
            }
        }
    }
    if items.len() > MAX_ITEMS {
        return Err(format!("The drawing becomes {} items; a block holds {MAX_ITEMS} at most. Simplify it first.", items.len()));
    }
    let layout = TitleBlockLayout { size, items };
    layout.check()?;
    Ok((layout, r.skipped))
}

/// The bytes a call names: `dxf` (the file's text) or `path`.
fn bytes_param(params: &Value) -> Result<Vec<u8>, String> {
    if let Some(t) = params.get("dxf").and_then(Value::as_str) {
        if t.len() > MAX_DXF_BYTES {
            return Err(unreadable(&format!("it is larger than {} MB", MAX_DXF_BYTES / (1024 * 1024))));
        }
        return Ok(t.as_bytes().to_vec());
    }
    let path = params.get("path").and_then(Value::as_str).map(str::trim).ok_or("Give `path` or `dxf` (the file's text).")?;
    let meta = std::fs::metadata(path).map_err(|e| format!("Cannot read {path}: {e}."))?;
    if meta.len() > MAX_DXF_BYTES as u64 {
        return Err(unreadable(&format!("it is larger than {} MB", MAX_DXF_BYTES / (1024 * 1024))));
    }
    std::fs::read(path).map_err(|e| format!("Cannot read {path}: {e}."))
}

/// `titleblockimport {path | dxf, sheet, name?}` or `{id | name, item, path | dxf}`.
pub fn run_import(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let bytes = bytes_param(params).map_err(err)?;
    if params.get("item").is_some() {
        let n = params.get("item").and_then(Value::as_u64).ok_or_else(|| err("`item` is the logo item's number (from 0)."))?;
        let prims = logo(&bytes).map_err(err)?;
        // Brief 070: this reader still refuses in words; they become a CmdError here.
        return on_project(s, |p, pg, _| {
            (|| {
                let doc = p.ledger.document();
                let id = crate::title_blocks::commands::target(doc, params)?;
                if id.is_builtin() {
                    return Err(ecad_electrical::title_blocks::commands::BUILTIN_READ_ONLY.into());
                }
                let layer = [StyleLayer::Project, StyleLayer::Standard]
                    .into_iter()
                    .find(|l| ecad_electrical::styles::record(doc, *l, FRAME_CATALOGUE, &id).is_some())
                    .ok_or(ecad_electrical::title_blocks::commands::NOT_A_FRAME)?;
                let before = ecad_electrical::styles::record(doc, layer, FRAME_CATALOGUE, &id).cloned();
                let mut after = before.clone().unwrap_or_else(|| StyleRecord::empty(FRAME_CATALOGUE));
                let mut layout = after.title_block().cloned().ok_or("This frame has no title block yet; add a Logo item first.")?;
                let count = prims.len();
                match usize::try_from(n).ok().and_then(|i| layout.items.get_mut(i)) {
                    Some(BlockItem::Logo { prims: slot, .. }) => *slot = prims,
                    _ => return Err(format!("Item {n} is not a logo.")),
                }
                layout.check()?;
                if let ecad_model::objects::styles::StyleBody::Frame(f) = &mut after.body {
                    f.title_block = Some(layout);
                }
                let name = ecad_electrical::styles::name(doc, FRAME_CATALOGUE, &id);
                let ops = vec![Op::SetStyle { layer, catalogue: FRAME_CATALOGUE, id: id.clone(), before, after: Some(after) }];
                p.apply_edit(pg, "titleblockimport", &format!("Logo of {name}"), ops)?;
                Ok(json!({"changed": true, "id": id.text(), "item": n, "shapes": count}))
            })()
            .map_err(|e: String| CmdError::refused(e))
        });
    }
    on_project(s, |p, pg, _| {
        (|| {
            let doc = p.ledger.document();
            let spec = match params.get("sheet").and_then(Value::as_str) {
                Some(t) => SheetSpec::parse(t)?,
                None => doc.settings.sheet(),
            };
            let margins = ecad_model::document::sheet::Margins::default();
            let (layout, skipped) = block(&bytes, &spec, &margins)?;
            let items = layout.items.len();
            let base = format!("{} imported block", ecad_electrical::title_blocks::sheet_name(&spec));
            let name = match params.get("name").and_then(Value::as_str).map(str::trim) {
                Some(n) if ecad_model::objects::styles::is_name(n) => n.to_string(),
                Some(_) => return Err(ecad_electrical::styles::commands::styles::STYLE_NEEDS_NAME.into()),
                None => {
                    let taken: Vec<String> = ecad_electrical::styles::rows(doc, &ecad_electrical::styles::frame::CATALOGUE)
                        .into_iter()
                        .map(|r| r.name.to_lowercase())
                        .collect();
                    (1..10_000)
                        .map(|i| if i == 1 { base.clone() } else { format!("{base} {i}") })
                        .find(|n| !taken.contains(&n.to_lowercase()))
                        .unwrap_or(base)
                }
            };
            let body = ecad_model::objects::styles::StyleBody::Frame(ecad_model::objects::styles::FrameStyle {
                sheet: Some(ecad_model::objects::styles::frame::FrameSheet::Sheet(spec)),
                block: Some(ecad_model::objects::styles::frame::BlockAt::BottomRight),
                title_block: Some(layout),
                ..Default::default()
            });
            let rec = StyleRecord::new(name.clone(), body);
            let id = StyleRef::Own(p.ids.fresh::<u128>());
            ecad_electrical::styles::check_record(doc, &ecad_electrical::styles::frame::CATALOGUE, &id, StyleLayer::Project, &rec)?;
            let ops = vec![Op::SetStyle { layer: StyleLayer::Project, catalogue: FRAME_CATALOGUE, id: id.clone(), before: None, after: Some(rec) }];
            p.apply_edit(pg, "titleblockimport", &format!("Import title block {name}"), ops)?;
            Ok(json!({"changed": true, "id": id.text(), "name": name, "items": items, "skipped": skipped}))
        })()
        .map_err(|e: String| CmdError::refused(e))
    })
}
