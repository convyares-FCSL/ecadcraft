//! The engine half of [`crate::generated`]: a generated page's sheets drawn as frame and content
//! parts in paper millimetres, from the rows and sheets the rules work out.

use cadcraft_engine::doc::{EntityKind, HAlign, Text, VAlign};
use cadcraft_engine::geom::Vec3;
use ecad_model::document::book::Book;
use ecad_model::document::sheet::unit_scale;
use ecad_model::{Document, GenKind, PageId, Primitive, Pt};

use crate::convert::kind_for_primitive;
use crate::pages::frame::{FrameInput, Part};
use ecad_electrical::pages::frames::LABEL_ACI;
use ecad_electrical::pages::frames::THIN_WEIGHT;
use ecad_electrical::pages::generated::*;
use ecad_electrical::reports::kept::ReportModels;
use ecad_model::look::TextDraw;
use {crate::styles::draw::DrawStyles, ecad_model::look::TextKind};

/// Draws in paper millimetres, sheet origin at `x0`.
pub struct Draw {
    pub k: f64,
    pub x0: f64,
    pub out: Vec<Part>,
}

impl Draw {
    pub fn p(&self, x: f64, y: f64) -> Pt {
        Pt::new((x + self.x0) * self.k, y * self.k)
    }
    pub fn line(&mut self, a: (f64, f64), b: (f64, f64)) {
        let kind = kind_for_primitive(&Primitive::Line { a: self.p(a.0, a.1), b: self.p(b.0, b.1) });
        self.out.push(Part { kind, weight: Some(THIN_WEIGHT), aci: None, layer: LAYER });
    }
    /// A line of weight `weight` (a heading's rule).
    pub fn rule(&mut self, a: (f64, f64), b: (f64, f64), weight: u16) {
        let kind = kind_for_primitive(&Primitive::Line { a: self.p(a.0, a.1), b: self.p(b.0, b.1) });
        self.out.push(Part { kind, weight: Some(weight), aci: None, layer: LAYER });
    }
    /// A filled dot of `radius` millimetres (concentric circles, like the junction glyph).
    pub fn dot(&mut self, at: (f64, f64), radius: f64) {
        let center = self.p(at.0, at.1);
        for k in [1.0, 0.66, 0.33] {
            let kind = kind_for_primitive(&Primitive::Circle { center, radius: radius * k * self.k });
            self.out.push(Part { kind, weight: Some(THIN_WEIGHT), aci: None, layer: LAYER });
        }
    }
    /// A closed rectangle from `a` to `b` (paper millimetres), weight `weight`, colour `aci`.
    pub fn rect(&mut self, a: (f64, f64), b: (f64, f64), weight: u16, aci: Option<u8>) {
        let v = vec![self.p(a.0, a.1), self.p(b.0, a.1), self.p(b.0, b.1), self.p(a.0, b.1)];
        let kind = kind_for_primitive(&Primitive::Polyline { vertices: v, closed: true });
        self.out.push(Part { kind, weight: Some(weight), aci, layer: LAYER });
    }
    pub fn text(&mut self, at: (f64, f64), draw: TextDraw, value: &str, h: HAlign, aci: Option<u8>, room: Option<f64>) {
        if value.is_empty() {
            return;
        }
        let height = draw.mm;
        let (shown, wf) = ecad_electrical::pages::frames::fit(value, height, room);
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
            style: draw.style.into(),
            halign: h,
            valign: VAlign::Middle,
        });
        self.out.push(Part { kind, weight: draw.weight, aci, layer: LAYER });
    }
}

/// The missing-data mark (`ecad.missing-box`): `?` in a thin box inset by the
/// registry's `print.missing_box_inset` into the cell from `lo` to `hi` (model coordinates, `k`
/// drawing units per millimetre), the text in `draw`. Both parts are [`FLAGGED_ACI`] on
/// [`LAYER`]: yellow on screen; Print Book prints them in [`FLAGGED_PRINT_ACI`].
pub fn missing_mark(doc: &Document, lo: Pt, hi: Pt, k: f64, draw: TextDraw) -> Vec<Part> {
    if !(k.is_finite() && k > 0.0) || !lo.is_finite() || !hi.is_finite() {
        return Vec::new();
    }
    let inset_mm =
        ecad_model::registry::resolve(doc, &ecad_electrical::defaults::print::MISSING_BOX_INSET, ecad_model::registry::Scope::project()).number();
    let mut d = Draw { k, x0: 0.0, out: Vec::new() };
    let (x0, y0, x1, y1) = (lo.x.min(hi.x) / k, lo.y.min(hi.y) / k, lo.x.max(hi.x) / k, lo.y.max(hi.y) / k);
    let inset = inset_mm.max(0.0).min((x1 - x0) / 4.0).min((y1 - y0) / 4.0);
    d.rect((x0 + inset, y0 + inset), (x1 - inset, y1 - inset), ecad_electrical::cables::words::WIRE_WEIGHT, Some(FLAGGED_ACI));
    d.text((x0 + inset + 1.0, (y0 + y1) / 2.0), draw, MISSING, HAlign::Left, Some(FLAGGED_ACI), Some(x1 - x0 - 2.0 * inset - 1.0));
    d.out
}

/// The content of sheet `i` of a generated page of `kind`, with its frame `input` (paper
/// millimetres, sheet origin at x = `x0`).
#[allow(clippy::too_many_arguments)]
fn content(
    doc: &Document, page: PageId, kind: GenKind, input: &FrameInput, i: usize, x0: f64, reports: Option<&ReportModels>, _total: usize,
    laid: &mut Option<Laid>,
) -> Vec<Part> {
    let (l, b, r, t) = area(input);
    let mut d = Draw { k: unit_scale(input.unit), x0, out: Vec::new() };
    // Sizes and fonts only: the parts take their colours from the frame.
    let draws = DrawStyles::new(doc, &ecad_model::palette::sheet::PAPER);
    let draw = |kind| ecad_drawing_style::text::fonts::text_draw_of(&draws, kind, input.unit);
    let (header, cell_draw, heading) = (draw(TextKind::TableHeader), draw(TextKind::Table), draw(TextKind::TableHeading));
    // The cover's blocks.
    if kind == GenKind::Cover {
        return crate::pages::front::cover(doc, input, x0).0;
    }
    // A graphic terminal plan draws its strip sheet.
    if kind == GenKind::TerminalBlocks
        && ecad_electrical::terminal_strips::pages::style_of(doc, page) == ecad_electrical::terminal_strips::pages::Style::Graphic
    {
        return reports.map_or_else(Vec::new, |o| crate::terminal_strips::draw::content(doc, page, input, i, x0, o));
    }
    // The I/O list as ruled tables per card, the rack overview as tiles.
    if matches!(kind, GenKind::IoList | GenKind::PlcRack) {
        let a = ecad_electrical::plc::sheets::Area { l, b, r, t, scale: (r - l) / A3_AREA };
        let plc = reports.map(|o| &o.plc);
        if kind == GenKind::IoList {
            crate::plc::draw::sheets::io_sheet(doc, plc, &mut d, &draws, input.unit, a, i);
        } else {
            crate::plc::draw::sheets::rack_sheet(doc, plc, &mut d, &draws, input.unit, a, i);
        }
        return d.out;
    }
    // The table and its sheets are laid out once per page, not once per sheet.
    let Laid { cols, rows, sheets } = laid.get_or_insert_with(|| Laid::of(doc, page, kind, reports));
    let scale = (r - l) / A3_AREA;
    let widths: Vec<f64> = cols.iter().map(|(_, w)| w * scale).collect();
    let mut y = t;
    // The BOM's Qty column is right aligned, and each location heading is
    // ruled off with the frame's weight.
    let bom = kind == GenKind::BomLocation;
    let right = |c: usize| bom && c == BOM_QTY_COLUMN;
    // Header.
    let mut x = l;
    for (c, ((name, _), w)) in cols.iter().zip(&widths).enumerate() {
        if right(c) {
            d.text((x + w - 1.0, y - header_mm() / 2.0), header, name, HAlign::Right, Some(LABEL_ACI), Some(w - 2.0));
        } else {
            d.text((x + 1.0, y - header_mm() / 2.0), header, name, HAlign::Left, Some(LABEL_ACI), Some(w - 2.0));
        }
        x += w;
    }
    y -= header_mm();
    d.line((l, y), (r, y));
    // The sheets as the sheet count makes them; a sheet starting inside a location block repeats it
    // as `(continued)`, which takes one of its rows.
    let (start, end, prev_heading) = sheets.get(i).cloned().unwrap_or((0, 0, None));
    let page_rows: Vec<&TRow> = rows.get(start..end).unwrap_or_default().iter().collect();
    if let Some(h) = prev_heading {
        d.text((l + 1.0, y - row_mm() / 2.0), heading, &format!("{h} (continued)"), HAlign::Left, None, Some(r - l - 2.0));
        y -= row_mm();
        d.line((l, y), (r, y));
    }
    let left_of = |c: usize| l + widths.iter().take(c).sum::<f64>();
    let mut last_dot: Option<(usize, f64)> = None;
    for row in page_rows {
        if row.heading {
            let text = row.cells.first().map_or("", String::as_str);
            if text == MISSING {
                // Devices with no location head a `?` group,
                // flagged like a missing cell: the mark in the first column's width.
                let w = widths.first().copied().unwrap_or(r - l).min(r - l);
                d.out.extend(missing_mark(doc, d.p(l, y - row_mm()), d.p(l + w, y), d.k, heading));
            } else {
                d.text((l + 1.0, y - row_mm() / 2.0), heading, text, HAlign::Left, None, Some(r - l - 2.0));
            }
            last_dot = None;
        } else {
            let mut x = l;
            let mid = y - row_mm() / 2.0;
            for (c, (cell, w)) in row.cells.iter().zip(&widths).enumerate() {
                let aci = (cell == MISSING).then_some(FLAGGED_ACI);
                if cell == MISSING && row.marks.terminal_block_box != Some(c) {
                    d.out.extend(missing_mark(doc, d.p(x, y - row_mm()), d.p(x + w, y), d.k, cell_draw));
                } else if row.marks.terminal_block_box == Some(c) {
                    let cx = x + w / 2.0;
                    d.line((cx - 3.0, mid - 2.0), (cx + 3.0, mid - 2.0));
                    d.line((cx + 3.0, mid - 2.0), (cx + 3.0, mid + 2.0));
                    d.line((cx + 3.0, mid + 2.0), (cx - 3.0, mid + 2.0));
                    d.line((cx - 3.0, mid + 2.0), (cx - 3.0, mid - 2.0));
                    d.text((cx, mid), cell_draw, cell, HAlign::Center, aci, Some(*w - 2.0));
                } else if right(c) {
                    d.text((x + w - 1.0, mid), cell_draw, cell, HAlign::Right, aci, Some(w - 2.0));
                } else {
                    d.text((x + 1.0, mid), cell_draw, cell, HAlign::Left, aci, Some(w - 2.0));
                }
                x += w;
            }
            if let Some((c, group)) = row.marks.bridge {
                let cx = left_of(c) + widths.get(c).copied().unwrap_or(0.0) / 2.0;
                d.dot((cx, mid), 0.5);
                if let Some((g, y0)) = last_dot
                    && g == group
                {
                    d.line((cx, y0), (cx, mid));
                }
                last_dot = Some((group, mid));
            }
            if let Some((c, starts)) = row.marks.cable {
                let x0 = left_of(c) + 0.5;
                let top = if starts { mid } else { y };
                d.line((x0, top), (x0, y - row_mm()));
            }
        }
        y -= row_mm();
        if y < b {
            break;
        }
        if bom && row.heading {
            d.rule((l, y), (r, y), input.draw.border);
        } else {
            d.line((l, y), (r, y));
        }
    }
    d.out
}

/// Every frame and content part of `page`: one sheet for an ordinary page, every sheet of a
/// generated one side by side (what Print Book and PDF cut into sheets). `input` is the page's
/// frame input with Total already counting sheets.
pub fn page_parts(doc: &Document, page: PageId, input: &FrameInput, reports: Option<&ReportModels>, total: usize) -> Vec<Part> {
    sheets_parts(doc, page, input, reports, total, None)
}

/// The frame and content parts of sheet `sheet` (0-based, clamped to the last) of `page`, drawn
/// at the origin: what the page window shows. An ordinary page has one sheet.
pub(crate) fn sheet_parts(doc: &Document, page: PageId, input: &FrameInput, reports: Option<&ReportModels>, total: usize, sheet: usize) -> Vec<Part> {
    sheets_parts(doc, page, input, reports, total, Some(sheet))
}

/// [`page_parts`] for every sheet side by side (`only` `None`), or [`sheet_parts`] for one.
fn sheets_parts(doc: &Document, page: PageId, input: &FrameInput, reports: Option<&ReportModels>, total: usize, only: Option<usize>) -> Vec<Part> {
    let Some(kind) = kind_of(doc, page) else { return crate::pages::frame::parts(input) };
    let n = sheets_of(doc, page, reports);
    let width = f64::from(input.sheet.width) + SHEET_GAP;
    // A graphic page titles each sheet by its strip.
    let graphic = (kind == GenKind::TerminalBlocks
        && ecad_electrical::terminal_strips::pages::style_of(doc, page) == ecad_electrical::terminal_strips::pages::Style::Graphic)
        .then_some(reports)
        .flatten();
    let mut out = Vec::new();
    let mut laid = None;
    let which = match only {
        Some(i) => i.min(n.saturating_sub(1))..i.min(n.saturating_sub(1)).saturating_add(1),
        None => 0..n,
    };
    for i in which {
        let mut inp = input.clone();
        if let Some(o) = graphic {
            if let Some(t) = ecad_electrical::terminal_strips::pages::sheet_title(doc, page, o, i) {
                inp.page_title = format!("{}: {t}", input.page_title);
            }
        } else if n > 1 {
            inp.page_title = format!("{} ({}/{n})", input.page_title, i + 1);
        }
        if n > 1 {
            // Each sheet its own number; Next is the next sheet's.
            inp.page = sheet_number(&input.page, i);
            if i + 1 < n {
                inp.next = sheet_number(&input.page, i + 1);
            }
        }
        let x0 = if only.is_some() { 0.0 } else { width * i as f64 };
        let mut frame = crate::pages::frame::parts(&inp);
        if x0 != 0.0 {
            let m = cadcraft_engine::geom::Mat3::translate(cadcraft_engine::geom::Vec2::new(x0 * unit_scale(input.unit), 0.0));
            for p in &mut frame {
                p.kind.transform(&m);
            }
        }
        out.extend(frame);
        out.extend(content(doc, page, kind, &inp, i, x0, reports, total, &mut laid));
    }
    out
}

/// The frame input of `page` with Total counting sheets, and that total.
pub fn input_of(doc: &Document, page: PageId, reports: Option<&ReportModels>) -> (FrameInput, usize) {
    let mut input = FrameInput::of(doc, page, &Book::of(doc));
    let total = book_sheets_of(doc, reports);
    input.total = total.to_string();
    (input, total)
}

/// Every frame and content part of `page`, the pages drawn from the report models with
/// `reports` (the project's kept models, or [`ecad_electrical::views::Views::report_models`] for a state
/// it does not keep).
/// Without them those pages are drawn empty, one sheet each.
pub fn parts_of(doc: &Document, page: PageId, reports: Option<&ReportModels>) -> Vec<Part> {
    parts_of_pending(doc, page, reports, false)
}

/// [`parts_of`] with the project's pending flag: the Rev box and the cover say
/// when the drawing has changed since the newest issue.
pub fn parts_of_pending(doc: &Document, page: PageId, reports: Option<&ReportModels>, pending: bool) -> Vec<Part> {
    let (input, total) = input_of(doc, page, reports);
    let input = input.with_pending(doc, pending);
    page_parts(doc, page, &input, reports, total)
}

/// [`parts_of_pending`] for sheet `sheet` only, at the origin ([`sheet_parts`]): the page as the
/// page window shows it.
pub(crate) fn shown_parts(doc: &Document, page: PageId, reports: Option<&ReportModels>, pending: bool, sheet: usize) -> Vec<Part> {
    let (input, total) = input_of(doc, page, reports);
    let input = input.with_pending(doc, pending);
    sheet_parts(doc, page, &input, reports, total, sheet)
}
