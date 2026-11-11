//! Generated pages (brief 024, design 024 §5): a page whose content ECAD draws from the project
//! (cover, contents, revision list, BOM by location). It is a page like any other for numbers,
//! folders and the book; its kind is the page property [`KEY`] (design 024 FP1), set only when it
//! is made. Nothing drawn on it is stored.
//!
//! # Semantics
//! - A generated page has one or more **sheets**: a table that does not fit continues on the next
//!   sheet of the same page, header repeated ([`ROWS_PER_SHEET`] rows per A3 sheet). Sheets are
//!   drawn side by side, sheet *i* at x = *i* × (paper width + [`SHEET_GAP`]).
//! - Each sheet has the page's frame and title block: the page title gets ` (2/3)`, Next shows the
//!   same number on all but the last sheet, and Total counts sheets in the whole book (owner
//!   answer Q4).
//! - Content lies on the reserved, locked layer [`LAYER`]; it is mapped like the frame, so
//!   selecting it selects nothing and editing it is refused.
//!
//! # Update map (design 024 §9)
//! The project keeps the active page's parts with its frame. They are worked out again after an
//! op of the frame's map ([`crate::frame::touches`]), after any op [`touches`] names for the
//! active generated page's kind, and when the report models' BOM changes while a BOM page is shown
//! or the book's sheet count moves (`Project::update_outputs`). Contents reads the book order
//! (O(pages)) only when it is drawn.

use std::collections::BTreeSet;

use cadcraft_engine::doc::{EntityKind, HAlign, Text, VAlign};
use cadcraft_engine::geom::Vec3;
use ecad_model::book::{self, Book};
use ecad_model::sheet::{MARGIN_MM, TITLE_BLOCK_MM, TOP_MARGIN_MM, unit_scale};
use ecad_model::{Document, ObjectRef, Op, PageId, Primitive, Pt};

use crate::convert::kind_for_primitive;
use crate::frame::{FrameInput, LABEL_ACI, Part, THIN_WEIGHT};
use crate::outputs::{Group, Outputs};

/// The page property that names a generated page's kind.
pub const KEY: &str = "generated";
/// The reserved layer of generated content (`ecad.generated`), locked.
pub const LAYER: &str = "ECAD-GENERATED";
/// Rows per A3 sheet.
pub const ROWS_PER_SHEET: usize = crate::outputs::ROWS_PER_SHEET;
/// Gap between the sheets of one page, in millimetres.
pub const SHEET_GAP: f64 = 50.0;
/// `ecad.table-header` and `ecad.table-row`.
pub const HEADER_MM: f64 = 7.0;
pub const ROW_MM: f64 = 5.0;
/// The column widths of design 024 §5.2 are for an A3 drawing area this wide.
const A3_AREA: f64 = 390.0;
/// `ecad.flagged` (`ecad.check-warning`, ACI 2): `?` values on screen.
pub const FLAGGED_ACI: u8 = 2;
/// Refusal of `pageprops` changing the kind.
pub const KIND_FIXED: &str = "A page's kind is set when it is made; add a new page instead.";

/// What a generated page shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GenKind {
    Cover,
    Contents,
    Revisions,
    BomLocation,
}

impl GenKind {
    pub const ALL: [GenKind; 4] = [GenKind::Cover, GenKind::Contents, GenKind::Revisions, GenKind::BomLocation];

    /// The stored value.
    pub fn value(self) -> &'static str {
        match self {
            GenKind::Cover => "cover",
            GenKind::Contents => "contents",
            GenKind::Revisions => "revisions",
            GenKind::BomLocation => "bom-location",
        }
    }

    /// The kind a stored value names.
    pub fn parse(t: &str) -> Option<GenKind> {
        GenKind::ALL.into_iter().find(|k| k.value() == t.trim())
    }

    /// The default title.
    pub fn title(self) -> &'static str {
        match self {
            GenKind::Cover => "Cover",
            GenKind::Contents => "Contents",
            GenKind::Revisions => "Revision list",
            GenKind::BomLocation => "BOM by location",
        }
    }
}

/// The kind of `page`, if it is generated.
pub fn kind_of(doc: &Document, page: PageId) -> Option<GenKind> {
    doc.pages.get(&page).and_then(|p| p.properties.get(KEY)).and_then(|v| GenKind::parse(v))
}

/// Whether the project has a BOM page (its sheet count needs the report models).
pub fn has_bom_page(doc: &Document) -> bool {
    doc.pages.values().any(|p| p.properties.get(KEY).is_some_and(|v| v == GenKind::BomLocation.value()))
}

/// How many sheets the BOM-by-location table needs.
pub fn bom_sheets(o: &Outputs) -> usize {
    let mut heads: Vec<Option<String>> = Vec::new();
    let mut last: Option<Option<String>> = None;
    for r in o.bom(Group::Location) {
        if last.as_ref() != Some(&r.location) {
            heads.push(Some(r.location.clone().unwrap_or_else(|| "?".into())));
            last = Some(r.location.clone());
        }
        heads.push(None);
    }
    paginate(&heads).len()
}

/// The sheets of a table whose row `i` is a heading when `heads[i]` is `Some`: (first row, end,
/// the heading repeated as `(continued)` at the top). A continued heading takes a row of the
/// sheet, so every sheet holds at most [`ROWS_PER_SHEET`] lines (tester 024, bug 4).
pub fn paginate(heads: &[Option<String>]) -> Vec<(usize, usize, Option<String>)> {
    let n = heads.len();
    let mut out = Vec::new();
    let mut start = 0;
    let mut current: Option<String> = None;
    loop {
        let cont = if start > 0 && start < n && heads.get(start).is_some_and(Option::is_none) { current.clone() } else { None };
        let cap = ROWS_PER_SHEET - usize::from(cont.is_some());
        let end = (start + cap).min(n);
        for h in heads.get(start..end).unwrap_or_default().iter().flatten() {
            current = Some(h.clone());
        }
        out.push((start, end, cont));
        if end >= n {
            break;
        }
        start = end;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_continued_heading_takes_a_row_of_its_sheet() {
        let mut heads = vec![Some("+CP1".to_string())];
        heads.extend(std::iter::repeat_n(None, 88));
        let sheets = paginate(&heads);
        assert_eq!(sheets.len(), 3);
        for (start, end, cont) in &sheets {
            assert!(end - start + usize::from(cont.is_some()) <= ROWS_PER_SHEET);
        }
        assert_eq!(sheets[1].2.as_deref(), Some("+CP1"));
        assert_eq!(paginate(&[]).len(), 1);
    }
}

/// How many sheets `page` prints on.
pub fn sheets(doc: &Document, page: PageId, outputs: Option<&Outputs>) -> usize {
    let rows = match kind_of(doc, page) {
        None | Some(GenKind::Cover) => return 1,
        Some(GenKind::Contents) => doc.page_order.len(),
        Some(GenKind::Revisions) => crate::revisions::issued(doc).len(),
        Some(GenKind::BomLocation) => return outputs.map_or(1, bom_sheets),
    };
    rows.div_ceil(ROWS_PER_SHEET).max(1)
}

/// How many sheets the whole book has (Total).
pub fn book_sheets(doc: &Document, outputs: Option<&Outputs>) -> usize {
    let extra: usize =
        doc.pages.iter().filter(|(_, p)| p.properties.contains_key(KEY)).map(|(id, _)| sheets(doc, *id, outputs).saturating_sub(1)).sum();
    doc.page_order.len() + extra
}

/// Whether `ops` can change what a generated page of `kind` shows (beyond the frame's own map
/// and the report models, which the project watches itself).
pub fn touches(ops: &[Op], kind: GenKind) -> bool {
    ops.iter().any(|op| match (kind, op) {
        (
            GenKind::Contents,
            Op::AddPage { .. } | Op::RemovePage { .. } | Op::AddFolder { .. } | Op::RemoveFolder { .. } | Op::ModifyFolder { .. },
        ) => true,
        (GenKind::Contents, Op::SetProperty { object: ObjectRef::Page(_), key, .. }) => key == book::NUMBER_KEY || key == book::FOLDER_KEY,
        (GenKind::Cover | GenKind::Revisions, Op::SetProperty { object: ObjectRef::Project, .. }) => true,
        (GenKind::Cover | GenKind::Revisions, Op::NameVersion { .. } | Op::UnnameVersion { .. }) => true,
        (GenKind::Cover, Op::AddPage { .. } | Op::RemovePage { .. }) => true,
        _ => false,
    })
}

/// The refusal of `ops` when they would draw on a generated page (design 024 §5.3), before
/// anything is written.
pub fn refusal(doc: &Document, ops: &[Op]) -> Option<String> {
    ops.iter()
        .filter_map(|op| match op {
            Op::AddItem { item, .. } => Some(item.page),
            Op::LinkRepresentation { rep, .. } => Some(rep.page),
            Op::ModifyItem { after, .. } => Some(after.page),
            _ => None,
        })
        .find_map(|p| kind_of(doc, p).map(|k| refusal_words(doc, p, k)))
}

/// `Page 900 is generated (BOM by location). Draw on another page, or add one after it with +.`
pub fn refusal_words(doc: &Document, page: PageId, kind: GenKind) -> String {
    format!("Page {} is generated ({}). Draw on another page, or add one after it with +.", book::page_number(doc, page), kind.title())
}

/// Draws in paper millimetres, sheet origin at `x0`.
struct Draw {
    k: f64,
    x0: f64,
    out: Vec<Part>,
}

impl Draw {
    fn p(&self, x: f64, y: f64) -> Pt {
        Pt::new((x + self.x0) * self.k, y * self.k)
    }
    fn line(&mut self, a: (f64, f64), b: (f64, f64)) {
        let kind = kind_for_primitive(&Primitive::Line { a: self.p(a.0, a.1), b: self.p(b.0, b.1) });
        self.out.push(Part { kind, weight: Some(THIN_WEIGHT), aci: None, layer: LAYER });
    }
    fn text(&mut self, at: (f64, f64), height: f64, value: &str, h: HAlign, aci: Option<u8>, room: Option<f64>) {
        if value.is_empty() {
            return;
        }
        let (shown, wf) = crate::frame::fit(value, height, room);
        let at = self.p(at.0, at.1);
        let pt = Vec3::new(at.x, at.y, 0.0);
        let kind = EntityKind::Text(Text {
            insert: pt,
            align_pt: Some(pt),
            height: height * self.k,
            value: shown,
            rotation: 0.0,
            width_factor: wf,
            oblique: 0.0,
            style: "Standard".into(),
            halign: h,
            valign: VAlign::Middle,
        });
        self.out.push(Part { kind, weight: None, aci, layer: LAYER });
    }
}

/// The drawing area of a sheet in paper millimetres: left, bottom, right, top.
fn area(input: &FrameInput) -> (f64, f64, f64, f64) {
    let (w, h) = (f64::from(input.sheet.width), f64::from(input.sheet.height));
    (MARGIN_MM + 5.0, MARGIN_MM + TITLE_BLOCK_MM + 5.0, w - MARGIN_MM - 5.0, h - TOP_MARGIN_MM - 5.0)
}

/// A table row: cells, and whether it is a heading (BOM location blocks).
#[derive(Debug, Clone)]
struct TRow {
    cells: Vec<String>,
    heading: bool,
}

/// What a generated page shows: (columns with A3 widths, rows).
fn table(doc: &Document, kind: GenKind, outputs: Option<&Outputs>) -> (Vec<(&'static str, f64)>, Vec<TRow>) {
    let row = |cells: Vec<String>| TRow { cells, heading: false };
    match kind {
        GenKind::Cover => (Vec::new(), Vec::new()),
        GenKind::Contents => {
            let b = Book::of(doc);
            let rows = b
                .order
                .iter()
                .map(|p| {
                    let folder = book::folder_of(doc, *p).filter(|f| doc.folders.contains_key(f));
                    let title = doc.pages.get(p).map(|x| x.title.clone()).unwrap_or_default();
                    row(vec![b.number(doc, *p), title, crate::sheet_cmds::folder_path(doc, folder)])
                })
                .collect();
            (vec![("Page", 25.0), ("Title", 220.0), ("Folder", 145.0)], rows)
        }
        GenKind::Revisions => {
            let issued = crate::revisions::issued(doc);
            let rows = if issued.is_empty() {
                vec![row(vec![String::new(), String::new(), "No revisions issued yet.".into()])]
            } else {
                issued
                    .iter()
                    .map(|r| {
                        let mut c: Vec<String> = r.fields.to_vec();
                        c.push(r.issued.clone().unwrap_or_default());
                        row(c)
                    })
                    .collect()
            };
            let cols = vec![
                ("Rev", 15.0),
                ("Date", 25.0),
                ("Description", 200.0),
                ("Drawn", 30.0),
                ("Checked", 30.0),
                ("Approved", 30.0),
                ("Version", 60.0),
            ];
            (cols, rows)
        }
        GenKind::BomLocation => {
            let cols = vec![("Qty", 15.0), ("Tags", 70.0), ("Description", 140.0), ("Maker", 70.0), ("Part number", 95.0)];
            let Some(o) = outputs else { return (cols, Vec::new()) };
            let mut rows = Vec::new();
            let mut last: Option<Option<String>> = None;
            let q = |v: &Option<String>| v.clone().unwrap_or_else(|| "?".into());
            for r in o.bom(Group::Location) {
                if last.as_ref() != Some(&r.location) {
                    rows.push(TRow { cells: vec![q(&r.location)], heading: true });
                    last = Some(r.location.clone());
                }
                rows.push(row(vec![r.qty.to_string(), r.tags.join(", "), q(&r.description), q(&r.maker), q(&r.part_number)]));
            }
            if rows.is_empty() {
                rows.push(row(vec![String::new(), "No devices yet.".into()]));
            }
            (cols, rows)
        }
    }
}

/// The cover's lines: (text, height).
fn cover_lines(doc: &Document, total: usize) -> Vec<(String, f64)> {
    let prop = |k: &str| doc.properties.get(k).cloned().filter(|v| !v.trim().is_empty());
    let mut out = Vec::new();
    for (k, h) in [(crate::frame::TITLE, 10.0), (crate::frame::DRAWING_NUMBER, 7.0), (crate::frame::CUSTOMER, 5.0), (crate::frame::STATUS, 5.0)] {
        if let Some(v) = prop(k) {
            out.push((v, h));
        }
    }
    let rev = match crate::revisions::issued(doc).first() {
        Some(r) => {
            let date = r.get("date");
            if date.is_empty() { format!("Revision {}", r.rev()) } else { format!("Revision {}, {date}", r.rev()) }
        }
        None => "Not issued".into(),
    };
    out.push((rev, 5.0));
    out.push((if total == 1 { "1 sheet".into() } else { format!("{total} sheets") }, 3.5));
    out
}

/// The content of sheet `i` of a generated page of `kind`, with its frame `input` (paper
/// millimetres, sheet origin at x = `x0`).
fn content(doc: &Document, kind: GenKind, input: &FrameInput, i: usize, x0: f64, outputs: Option<&Outputs>, total: usize) -> Vec<Part> {
    let (l, b, r, t) = area(input);
    let mut d = Draw { k: unit_scale(input.unit), x0, out: Vec::new() };
    if kind == GenKind::Cover {
        let lines = cover_lines(doc, total);
        let gap = 6.0;
        let height: f64 = lines.iter().map(|(_, h)| h + gap).sum();
        let mut y = (b + t) / 2.0 + height / 2.0;
        for (text, h) in lines {
            y -= h / 2.0 + gap / 2.0;
            d.text(((l + r) / 2.0, y), h, &text, HAlign::Center, None, Some(r - l));
            y -= h / 2.0 + gap / 2.0;
        }
        return d.out;
    }
    let (cols, rows) = table(doc, kind, outputs);
    let scale = (r - l) / A3_AREA;
    let widths: Vec<f64> = cols.iter().map(|(_, w)| w * scale).collect();
    let mut y = t;
    // Header.
    let mut x = l;
    for ((name, _), w) in cols.iter().zip(&widths) {
        d.text((x + 1.0, y - HEADER_MM / 2.0), 2.5, name, HAlign::Left, Some(LABEL_ACI), Some(w - 2.0));
        x += w;
    }
    y -= HEADER_MM;
    d.line((l, y), (r, y));
    // The sheets as the sheet count makes them; a sheet starting inside a location block repeats it
    // as `(continued)`, which takes one of its rows.
    let heads: Vec<Option<String>> = rows.iter().map(|r| r.heading.then(|| r.cells.first().cloned().unwrap_or_default())).collect();
    let (start, end, prev_heading) = paginate(&heads).get(i).cloned().unwrap_or((0, 0, None));
    let page_rows: Vec<&TRow> = rows.get(start..end).unwrap_or_default().iter().collect();
    if let Some(h) = prev_heading {
        d.text((l + 1.0, y - ROW_MM / 2.0), 3.5, &format!("{h} (continued)"), HAlign::Left, None, Some(r - l - 2.0));
        y -= ROW_MM;
        d.line((l, y), (r, y));
    }
    for row in page_rows {
        if row.heading {
            d.text((l + 1.0, y - ROW_MM / 2.0), 3.5, row.cells.first().map_or("", String::as_str), HAlign::Left, None, Some(r - l - 2.0));
        } else {
            let mut x = l;
            for (cell, w) in row.cells.iter().zip(&widths) {
                let aci = (cell == "?").then_some(FLAGGED_ACI);
                d.text((x + 1.0, y - ROW_MM / 2.0), 2.5, cell, HAlign::Left, aci, Some(w - 2.0));
                x += w;
            }
        }
        y -= ROW_MM;
        if y < b {
            break;
        }
        d.line((l, y), (r, y));
    }
    d.out
}

/// Every frame and content part of `page`: one sheet for an ordinary page, every sheet of a
/// generated one side by side. `input` is the page's frame input with Total already counting
/// sheets.
pub fn page_parts(doc: &Document, page: PageId, input: &FrameInput, outputs: Option<&Outputs>, total: usize) -> Vec<Part> {
    let Some(kind) = kind_of(doc, page) else { return crate::frame::parts(input) };
    let n = sheets(doc, page, outputs);
    let width = f64::from(input.sheet.width) + SHEET_GAP;
    let mut out = Vec::new();
    for i in 0..n {
        let mut inp = input.clone();
        if n > 1 {
            inp.page_title = format!("{} ({}/{n})", input.page_title, i + 1);
            if i + 1 < n {
                inp.next = input.page.clone();
            }
        }
        let x0 = width * i as f64;
        let mut frame = crate::frame::parts(&inp);
        if x0 != 0.0 {
            let m = cadcraft_engine::geom::Mat3::translate(cadcraft_engine::geom::Vec2::new(x0 * unit_scale(input.unit), 0.0));
            for p in &mut frame {
                p.kind.transform(&m);
            }
        }
        out.extend(frame);
        out.extend(content(doc, kind, &inp, i, x0, outputs, total));
    }
    out
}

/// The sheet windows of `page` in model coordinates: (bottom-left, top-right) per sheet.
pub fn sheet_windows(doc: &Document, page: PageId, outputs: Option<&Outputs>) -> Vec<(Pt, Pt)> {
    let spec = ecad_model::sheet::sheet_of(doc, page);
    let k = unit_scale(doc.settings.length_unit);
    let (w, h) = (f64::from(spec.width), f64::from(spec.height));
    (0..sheets(doc, page, outputs))
        .map(|i| {
            let x0 = (w + SHEET_GAP) * i as f64;
            (Pt::new(x0 * k, 0.0), Pt::new((x0 + w) * k, h * k))
        })
        .collect()
}

/// How many generated values in `pages` are `?` (Print Book's warning line).
pub fn flagged(doc: &Document, pages: &BTreeSet<PageId>, outputs: Option<&Outputs>) -> usize {
    pages
        .iter()
        .filter(|p| kind_of(doc, **p) == Some(GenKind::BomLocation))
        .map(|_| outputs.map_or(0, |o| o.bom_table(Group::Location).flagged()))
        .sum()
}

/// The frame input of `page` with Total counting sheets, and that total.
pub fn input_of(doc: &Document, page: PageId, outputs: Option<&Outputs>) -> (FrameInput, usize) {
    let mut input = FrameInput::of(doc, page, &Book::of(doc));
    let total = book_sheets(doc, outputs);
    input.total = total.to_string();
    (input, total)
}

/// Every frame and content part of `page`, building the report models only when the project has a
/// BOM page and none are given (debugger, verification, tests).
pub fn parts_of(doc: &Document, page: PageId, outputs: Option<&Outputs>) -> Vec<Part> {
    let built;
    let outputs = match outputs {
        Some(o) => Some(o),
        None if has_bom_page(doc) => {
            built = Outputs::build(doc);
            Some(&built)
        }
        None => None,
    };
    let (input, total) = input_of(doc, page, outputs);
    page_parts(doc, page, &input, outputs, total)
}
