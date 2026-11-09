//! The sheet frame and title block of a page (brief 019, design 019 §3, §4): derived graphics on
//! the reserved, locked layer [`LAYER`], drawn by the projection and never stored.
//!
//! # Semantics
//! - [`FrameInput::of`] reads everything a page's frame shows: its sheet, the project and page
//!   properties, the newest issued revisions, the page's number, the next page's number and the
//!   page count. [`parts`] turns it into lines and texts.
//! - **Update map** (design 019 §9, `ecad/CLAUDE.md` "Derived views"): [`touches`] says which ops
//!   can change the frame of the page shown; the project keeps the last parts and recomputes them
//!   only after such an op, on open and when another page becomes active. Every other op leaves
//!   them as they are.
//!
//! This module has no UI and records nothing.

use std::collections::BTreeSet;

use cadcraft_engine::doc::{Common, Entity, EntityKind, Handle, Lineweight, Text, color::Color};
use cadcraft_engine::doc::{HAlign, VAlign};
use cadcraft_engine::geom::Vec3;
use ecad_model::book::{self, Book, FUNCTION_KEY, LOCATION_KEY};
use ecad_model::sheet::{Grid, MARGIN_MM, SheetSpec, TITLE_BLOCK_MM, TOP_MARGIN_MM, unit_scale};
use ecad_model::{Document, LengthUnit, ObjectRef, Op, PageId, Primitive, Pt};

use crate::convert::kind_for_primitive;

/// The reserved layer of the frame (`ecad.frame`), locked so CADCraft's own commands leave it.
pub const LAYER: &str = "ECAD-FRAME";
/// `ecad.frame`: ACI 7.
pub const FRAME_ACI: u8 = 7;
/// `ecad.frame-label` and `ecad.sheet-edge`: ACI 8.
pub const LABEL_ACI: u8 = 8;
/// `ecad.frame-weight` (hundredths of a millimetre).
pub const FRAME_WEIGHT: u16 = 50;
/// `ecad.wire-weight`.
pub const THIN_WEIGHT: u16 = 25;
/// Refusal of an edit of the frame (design 019 §3.2).
pub const DERIVED: &str = "The sheet frame is derived; change it in Project settings or the page's Sheet.";
/// Most issued revisions the title block lists (owner answer Q12).
pub const TABLE_ROWS: usize = 7;
/// Project property keys the title block shows.
pub const TITLE: &str = "title";
pub const CUSTOMER: &str = "customer";
pub const DRAWING_NUMBER: &str = "drawing_number";
pub const STATUS: &str = "status";

/// The layer as the projection adds it.
pub fn layer() -> cadcraft_engine::doc::Layer {
    let mut l = cadcraft_engine::doc::Layer::new(LAYER);
    l.color = Color::Index(FRAME_ACI);
    l.locked = true;
    l
}

/// Take the sheet frame's entities out of the active document's selection (design 019 §3.2:
/// selecting the frame is refused; CADCraft selects objects on locked layers). Returns `true`
/// when the selection held nothing else, so the caller can say why it is empty. Cheap: reads the
/// selection only.
pub fn drop_from_selection(s: &mut cadcraft_engine::Session) -> bool {
    let sel = s.selection();
    if sel.is_empty() {
        return false;
    }
    let Ok(d) = s.doc() else { return false };
    let keep: Vec<Handle> =
        sel.iter().copied().filter(|h| d.entity(*h).is_none_or(|e| e.common.layer != LAYER)).collect();
    if keep.len() == sel.len() {
        return false;
    }
    let only_frame = keep.is_empty();
    s.set_selection(keep);
    only_frame
}

/// Everything one page's frame shows.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameInput {
    pub sheet: SheetSpec,
    pub unit: LengthUnit,
    pub project_title: String,
    pub customer: String,
    pub drawing_number: String,
    pub status: String,
    pub page_title: String,
    pub function: String,
    pub location: String,
    pub page: String,
    pub rev: String,
    /// The next page in book order, and its number.
    pub next_page: Option<PageId>,
    pub next: String,
    pub total: String,
    /// Newest issued first, at most [`TABLE_ROWS`]: rev, date, description, drawn, checked,
    /// approved.
    pub revisions: Vec<[String; 6]>,
}

impl FrameInput {
    /// What `page`'s frame shows in `doc` (`book`: its book order).
    pub fn of(doc: &Document, page: PageId, book: &Book) -> FrameInput {
        let prop = |k: &str| doc.properties.get(k).cloned().unwrap_or_default();
        let pprops = doc.pages.get(&page).map(|p| &p.properties);
        let inherited = |k: &str| pprops.and_then(|p| p.get(k)).cloned().unwrap_or_else(|| prop(k));
        let issued = crate::revisions::issued(doc);
        let next_page = book.next(page);
        // Checked there, so the frame and the references always use the same sheet.
        let sheet = ecad_model::sheet::sheet_of(doc, page);
        FrameInput {
            sheet,
            unit: doc.settings.length_unit,
            project_title: prop(TITLE),
            customer: prop(CUSTOMER),
            drawing_number: prop(DRAWING_NUMBER),
            status: prop(STATUS),
            page_title: doc.pages.get(&page).map(|p| p.title.clone()).unwrap_or_default(),
            function: inherited(FUNCTION_KEY),
            location: inherited(LOCATION_KEY),
            page: book.number(doc, page),
            rev: issued.first().map_or_else(|| "-".to_string(), |r| r.rev().to_string()),
            next_page,
            next: next_page.map_or_else(|| "-".to_string(), |n| book.number(doc, n)),
            total: book.order.len().to_string(),
            revisions: issued.iter().take(TABLE_ROWS).map(|r| r.fields.clone()).collect(),
        }
    }
}

/// Whether `ops` can change the frame of `page`, whose frame
/// last showed `shown` (design 019 §9, the frame and title block rows). Every op not listed
/// changes nothing there.
pub fn touches(ops: &[Op], page: PageId, shown: &FrameInput) -> bool {
    // A rename or move is a RemovePage + AddPage pair of one page.
    let removed: BTreeSet<PageId> =
        ops.iter().filter_map(|o| if let Op::RemovePage { id, .. } = o { Some(*id) } else { None }).collect();
    let added: BTreeSet<PageId> =
        ops.iter().filter_map(|o| if let Op::AddPage { id, .. } = o { Some(*id) } else { None }).collect();
    let pair = |id: &PageId| removed.contains(id) && added.contains(id);
    ops.iter().any(|op| match op {
        Op::SetProjectSettings { before, after } => {
            before.sheet != after.sheet || before.length_unit != after.length_unit
        }
        // Only what the title block shows: its fields, the defaults pages inherit, and issuing.
        Op::SetProperty { object: ObjectRef::Project, key, .. } => {
            [TITLE, CUSTOMER, DRAWING_NUMBER, STATUS, FUNCTION_KEY, LOCATION_KEY].contains(&key.as_str())
                || (key.starts_with(crate::revisions::PREFIX) && key.ends_with(".issued"))
        }
        Op::SetProperty { object: ObjectRef::Page(p), key, .. } => {
            *p == page || key == book::FOLDER_KEY || (key == book::NUMBER_KEY && shown.next_page == Some(*p))
        }
        // A page renamed in place changes only its own title box; a page moved changes the book
        // order (Next, and the numbers of unnumbered pages).
        Op::AddPage { id, index, .. } | Op::RemovePage { id, index, .. } if pair(id) => {
            let moved = ops.iter().any(|o| match o {
                Op::AddPage { id: a, index: i, .. } | Op::RemovePage { id: a, index: i, .. } => a == id && i != index,
                _ => false,
            });
            *id == page || moved
        }
        Op::AddPage { .. } | Op::RemovePage { .. } => true,
        Op::AddFolder { .. } | Op::RemoveFolder { .. } | Op::ModifyFolder { .. } => true,
        Op::NameVersion { .. } | Op::UnnameVersion { .. } => true,
        _ => false,
    })
}

/// One part of a frame drawing.
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub kind: EntityKind,
    /// Hundredths of a millimetre, or `None` for ByLayer.
    pub weight: Option<u16>,
    /// `None`: ByLayer (ACI 7).
    pub aci: Option<u8>,
}

/// The entity of `part` with `handle`.
pub fn entity(part: &Part, handle: Handle) -> Entity {
    Entity {
        handle,
        common: Common {
            layer: LAYER.into(),
            lineweight: part.weight.map_or(Lineweight::ByLayer, Lineweight::Mm100),
            color: part.aci.map_or(Color::ByLayer, Color::Index),
            ..Common::default()
        },
        kind: part.kind.clone(),
    }
}

/// Draws in the frame's own coordinates (origin at the top-left corner of the title block, the
/// layout of design 019 §4), placed so the paper's bottom-left corner is the model origin.
struct Draw {
    k: f64,
    out: Vec<Part>,
}

/// Where the frame's own origin sits on the paper, in millimetres.
const OFFSET: (f64, f64) = (MARGIN_MM, MARGIN_MM + TITLE_BLOCK_MM);

impl Draw {
    fn p(&self, x: f64, y: f64) -> Pt {
        Pt::new((x + OFFSET.0) * self.k, (y + OFFSET.1) * self.k)
    }
    fn line(&mut self, a: (f64, f64), b: (f64, f64), weight: u16, aci: Option<u8>) {
        let kind = kind_for_primitive(&Primitive::Line { a: self.p(a.0, a.1), b: self.p(b.0, b.1) });
        self.out.push(Part { kind, weight: Some(weight), aci });
    }
    fn rect(&mut self, a: (f64, f64), b: (f64, f64), weight: u16, aci: Option<u8>) {
        let v = vec![self.p(a.0, a.1), self.p(b.0, a.1), self.p(b.0, b.1), self.p(a.0, b.1)];
        let kind = kind_for_primitive(&Primitive::Polyline { vertices: v, closed: true });
        self.out.push(Part { kind, weight: Some(weight), aci });
    }
    /// Text at `at` (millimetres), `height` mm, aligned; fitted into `room` mm when given.
    fn text(
        &mut self,
        at: (f64, f64),
        height: f64,
        value: &str,
        align: (HAlign, VAlign),
        aci: Option<u8>,
        room: Option<f64>,
    ) {
        if value.is_empty() {
            return;
        }
        let (shown, wf) = fit(value, height, room);
        let at = self.p(at.0, at.1);
        let pt = Vec3::new(at.x, at.y, 0.0);
        let left_base = matches!(align, (HAlign::Left, VAlign::Baseline));
        let kind = EntityKind::Text(Text {
            insert: pt,
            align_pt: if left_base { None } else { Some(pt) },
            height: height * self.k,
            value: shown,
            rotation: 0.0,
            width_factor: wf,
            oblique: 0.0,
            style: "Standard".into(),
            halign: align.0,
            valign: align.1,
        });
        self.out.push(Part { kind, weight: None, aci });
    }
}

/// Roughly how wide a character of the standard font is, per unit height.
const CHAR_WIDTH: f64 = 0.8;
/// The narrowest width factor a title-block text is squeezed to before it is cut.
const MIN_WIDTH_FACTOR: f64 = 0.6;

/// `value` fitted in `room` mm at `height` (design 019 §4): narrowed down to
/// [`MIN_WIDTH_FACTOR`], then cut with `…`.
fn fit(value: &str, height: f64, room: Option<f64>) -> (String, f64) {
    let Some(room) = room.filter(|r| *r > 0.0) else { return (value.to_string(), 1.0) };
    let n = value.chars().count() as f64;
    let wide = n * height * CHAR_WIDTH;
    if wide <= room {
        return (value.to_string(), 1.0);
    }
    let wf = room / wide;
    if wf >= MIN_WIDTH_FACTOR {
        return (value.to_string(), wf);
    }
    let keep = ((room / (height * CHAR_WIDTH * MIN_WIDTH_FACTOR)).floor() as usize).saturating_sub(1).max(1);
    let mut s: String = value.chars().take(keep).collect();
    s.push('…');
    (s, MIN_WIDTH_FACTOR)
}

/// How a page's frame and title block are drawn from what they show (owner note of 2026-11-08:
/// users will define their own borders and title blocks). [`BuiltIn`] is the one layout today; a
/// user-defined template becomes another implementation, chosen per project or page.
pub trait FrameLayout: Send + Sync {
    /// Every line and text of the frame `input` describes, in model coordinates (the paper's
    /// bottom-left corner at the origin).
    fn parts(&self, input: &FrameInput) -> Vec<Part>;
}

/// The built-in frame and title block (design 019 §3, §4).
#[derive(Debug, Clone, Copy, Default)]
pub struct BuiltIn;

impl FrameLayout for BuiltIn {
    fn parts(&self, input: &FrameInput) -> Vec<Part> {
        built_in(input)
    }
}

/// Every line and text of the frame `input` describes, with the built-in layout.
pub fn parts(input: &FrameInput) -> Vec<Part> {
    BuiltIn.parts(input)
}

fn built_in(input: &FrameInput) -> Vec<Part> {
    let s = input.sheet;
    let k = unit_scale(input.unit);
    let grid = Grid::new(s, LengthUnit::Millimetre);
    let (w, h) = (f64::from(s.width), f64::from(s.height));
    let a = grid.area_width();
    // The frame's top edge and bottom edge in the frame's own coordinates.
    let top = h - MARGIN_MM - TOP_MARGIN_MM - TITLE_BLOCK_MM;
    let bottom = -TITLE_BLOCK_MM;
    let mut d = Draw { k, out: Vec::new() };
    let ml = (HAlign::Middle, VAlign::Middle);
    let left = (HAlign::Left, VAlign::Baseline);
    let tl = (HAlign::Left, VAlign::Top);
    // Paper edge, frame, title block's top line.
    d.rect(
        (-MARGIN_MM, -TITLE_BLOCK_MM - MARGIN_MM),
        (w - MARGIN_MM, h - TITLE_BLOCK_MM - MARGIN_MM),
        THIN_WEIGHT,
        Some(LABEL_ACI),
    );
    d.rect((0.0, -TITLE_BLOCK_MM), (a, top), FRAME_WEIGHT, None);
    d.line((0.0, 0.0), (a, 0.0), FRAME_WEIGHT, None);
    // Grid: zones over the whole frame (title-block area included), counted from the top-left;
    // ticks and labels on all four sides.
    let (cw, rh) = (grid.column_width(), grid.row_height());
    for ci in 0..s.columns {
        let x0 = f64::from(ci) * cw;
        let c = grid.column_number(ci);
        if ci > 0 {
            d.line((x0, top), (x0, top + 3.0), THIN_WEIGHT, None);
            d.line((x0, bottom), (x0, bottom - 3.0), THIN_WEIGHT, None);
        }
        let label = grid.column_label(c);
        d.text((x0 + cw / 2.0, top + TOP_MARGIN_MM / 2.0), 3.0, &label, ml, None, None);
        d.text((x0 + cw / 2.0, bottom - MARGIN_MM / 2.0), 3.0, &label, ml, None, None);
    }
    for ri in 0..s.rows {
        let y1 = top - f64::from(ri) * rh;
        let r = grid.row_number(ri);
        if ri > 0 {
            d.line((0.0, y1), (-3.0, y1), THIN_WEIGHT, None);
            d.line((a, y1), (a + 3.0, y1), THIN_WEIGHT, None);
        }
        let label = grid.row_label(r);
        d.text((-MARGIN_MM / 2.0, y1 - rh / 2.0), 3.5, &label, ml, None, None);
        d.text((a + MARGIN_MM / 2.0, y1 - rh / 2.0), 3.5, &label, ml, None, None);
    }

    // Title block (design 019 §4): the revision table stretches; the middle and right blocks keep
    // their widths against the right edge.
    let x_r = a - 110.0;
    let x_m = a - 230.0;
    let t = x_m;
    let cols = [0.0, 12.0, 36.0, t - 64.0, t - 42.0, t - 20.0, t];
    let thin = THIN_WEIGHT;
    for x in cols.iter().skip(1) {
        d.line((*x, -TITLE_BLOCK_MM), (*x, 0.0), thin, None);
    }
    for i in 1..=TABLE_ROWS {
        let y = -5.0 * i as f64;
        d.line((0.0, y), (t, y), thin, None);
    }
    for (i, name) in ["Rev", "Date", "Description", "Drawn", "Checked", "Approved"].iter().enumerate() {
        let x = cols.get(i).copied().unwrap_or_default();
        d.text((x + 1.0, -1.0), 1.8, name, tl, Some(LABEL_ACI), None);
    }
    for (row, rev) in input.revisions.iter().enumerate() {
        let y = -5.0 * (row + 2) as f64 + 1.25;
        for (i, value) in rev.iter().enumerate() {
            let (x0, x1) = (cols.get(i).copied().unwrap_or_default(), cols.get(i + 1).copied().unwrap_or_default());
            d.text((x0 + 1.0, y), 2.5, value, left, None, Some(x1 - x0 - 2.0));
        }
    }
    // Middle block.
    d.line((x_r, -TITLE_BLOCK_MM), (x_r, 0.0), thin, None);
    for y in [-10.0, -20.0, -30.0] {
        d.line((x_m, y), (x_r, y), thin, None);
    }
    let boxes = [
        ((x_m, -10.0), (x_r, 0.0), "Project", input.project_title.as_str(), 3.5),
        ((x_m, -20.0), (x_r, -10.0), "Customer", input.customer.as_str(), 2.5),
        ((x_m, -30.0), (x_r, -20.0), "Drawing no.", input.drawing_number.as_str(), 2.5),
        ((x_m, -40.0), (x_r, -30.0), "Status", input.status.as_str(), 2.5),
        ((x_r, -10.0), (a, 0.0), "Page title", input.page_title.as_str(), 3.5),
        ((x_r, -20.0), (x_r + 55.0, -10.0), "Function", input.function.as_str(), 2.5),
        ((x_r + 55.0, -20.0), (a, -10.0), "Location", input.location.as_str(), 2.5),
        ((x_r + 65.0, -30.0), (a, -20.0), "Next", input.next.as_str(), 2.5),
        ((x_r + 65.0, -40.0), (a, -30.0), "Total", input.total.as_str(), 2.5),
    ];
    // Right block lines.
    d.line((x_r, -10.0), (a, -10.0), thin, None);
    d.line((x_r, -20.0), (a, -20.0), thin, None);
    d.line((x_r + 65.0, -30.0), (a, -30.0), thin, None);
    d.line((x_r + 55.0, -20.0), (x_r + 55.0, -10.0), thin, None);
    d.line((x_r + 40.0, -40.0), (x_r + 40.0, -20.0), thin, None);
    d.line((x_r + 65.0, -40.0), (x_r + 65.0, -20.0), thin, None);
    for (lo, hi, label, value, height) in boxes {
        d.text((lo.0 + 1.0, hi.1 - 1.0), 1.8, label, tl, Some(LABEL_ACI), None);
        d.text((lo.0 + 1.0, lo.1 + 1.5), height, value, left, None, Some(hi.0 - lo.0 - 2.0));
    }
    for ((x0, x1), label, value, height) in [
        ((x_r, x_r + 40.0), "Page", input.page.as_str(), 7.0),
        ((x_r + 40.0, x_r + 65.0), "Rev", input.rev.as_str(), 5.0),
    ] {
        d.text((x0 + 1.0, -21.0), 1.8, label, tl, Some(LABEL_ACI), None);
        d.text(((x0 + x1) / 2.0, -31.0), height, value, ml, None, Some(x1 - x0 - 2.0));
    }
    d.out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_text_is_narrowed_then_cut() {
        assert_eq!(fit("abc", 2.5, Some(100.0)), ("abc".to_string(), 1.0));
        let (s, wf) = fit("abcdefghij", 2.5, Some(15.0));
        assert_eq!(s, "abcdefghij");
        assert!((wf - 0.75).abs() < 1e-9);
        let (s, wf) = fit(&"x".repeat(100), 2.5, Some(20.0));
        assert!(s.ends_with('…') && s.chars().count() < 20);
        assert!((wf - MIN_WIDTH_FACTOR).abs() < 1e-9);
    }
}
