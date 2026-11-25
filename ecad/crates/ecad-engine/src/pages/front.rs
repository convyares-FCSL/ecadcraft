//! The engine half of [`crate::front`]: the cover's blocks drawn as parts in paper millimetres, in
//! the places [`ecad_electrical::pages::front::layout`] works out.

use cadcraft_engine::doc::{EntityKind, HAlign, Solid};
use cadcraft_engine::geom::Vec3;
use ecad_model::{DefinitionId, Document, Primitive, Pt};

use crate::convert::kind_for_primitive;
use crate::pages::frame::{FrameInput, Part};
use ecad_electrical::pages::frames::LABEL_ACI;
use ecad_electrical::pages::front::*;
use ecad_model::look::TextDraw;
use {crate::pages::generated::Draw, ecad_electrical::pages::generated::LAYER};
use {crate::styles::draw::DrawStyles, ecad_model::look::TextKind};

/// The layer the screen-only outlines are drawn on (not printed).
const OVERLAY: &str = crate::convert::MARKER_LAYER;
/// The cover's lines as fractions of the `Cover` text style's height: block titles, body text, column
/// heads, the customer line. The factors are `[layout]` data in `pages.toml`.
fn cover_factor(key: &str) -> f64 {
    ecad_electrical::defaults::data::DATA.layout(key)
}
/// `ecad.front.derived` and `ecad.front.typed` in ACI (blue, orange).
const DERIVED_ACI: u8 = 5;
const TYPED_ACI: u8 = 30;

/// A block's rectangle in paper millimetres: left, bottom, right, top.
type R = (f64, f64, f64, f64);

/// Dashed outline of `r` on the overlay layer, and its caption at the top right.
fn outline(d: &mut Draw, r: R, aci: u8, caption: &str, draw: TextDraw) {
    let (l, b, rr, t) = r;
    let dash = 3.0;
    let gap = 2.0;
    let mut seg = |a: (f64, f64), z: (f64, f64)| {
        let len = ((z.0 - a.0).powi(2) + (z.1 - a.1).powi(2)).sqrt();
        if !(len.is_finite() && len > 0.0) {
            return;
        }
        let n = ((len / (dash + gap)).ceil() as usize).min(2_000);
        for i in 0..n {
            let s0 = i as f64 * (dash + gap) / len;
            let s1 = ((i as f64 * (dash + gap) + dash) / len).min(1.0);
            let p = |s: f64| (a.0 + (z.0 - a.0) * s, a.1 + (z.1 - a.1) * s);
            let kind = kind_for_primitive(&Primitive::Line { a: d.p(p(s0).0, p(s0).1), b: d.p(p(s1).0, p(s1).1) });
            d.out.push(Part { kind, weight: Some(ecad_electrical::pages::frames::THIN_WEIGHT), aci: Some(aci), layer: OVERLAY });
        }
    };
    seg((l, t), (rr, t));
    seg((rr, t), (rr, b));
    seg((rr, b), (l, b));
    seg((l, b), (l, t));
    if !caption.is_empty() {
        let before = d.out.len();
        d.text((rr - 2.0, t - 2.0), draw, caption, HAlign::Right, Some(aci), Some(rr - l - 4.0));
        for p in d.out.iter_mut().skip(before) {
            p.layer = OVERLAY;
        }
    }
}

/// A block's title and the thin rule under it; returns the y the block's content starts at.
fn title(d: &mut Draw, r: R, text: &str, draw: TextDraw, s: f64) -> f64 {
    let (l, _, rr, t) = r;
    let y = t - 6.5 * s;
    d.text((l + 4.0 * s, y), draw, text, HAlign::Left, None, Some(rr - l - 8.0 * s));
    let rule = y - 4.5 * s;
    d.line((l + 4.0 * s, rule), (rr - 4.0 * s, rule));
    rule - 5.5 * s
}

/// A filled swatch (the insulation colour) with a thin ink outline; two colours as halves.
fn swatch(d: &mut Draw, at: (f64, f64), w: f64, h: f64, colours: Option<&ecad_model::objects::wire::Colours>) {
    let rgb = |c: ecad_model::geometry::colour::ConductorColour| {
        let c = crate::cables::draw::rgb(ecad_electrical::cables::words::drawn(c, &ecad_model::palette::sheet::PAPER));
        Some(cadcraft_engine::doc::color::nearest_aci(c))
    };
    let fill = |d: &mut Draw, x0: f64, x1: f64, aci: Option<u8>| {
        let (a, b) = (d.p(x0, at.1 - h / 2.0), d.p(x1, at.1 + h / 2.0));
        let v = |x: f64, y: f64| Vec3::new(x, y, 0.0);
        // A SOLID's third and fourth corners are the far side, in that order.
        let kind = EntityKind::Solid(Solid { corners: [v(a.x, a.y), v(b.x, a.y), v(a.x, b.y), v(b.x, b.y)] });
        d.out.push(Part { kind, weight: None, aci, layer: LAYER });
    };
    match colours.map(|c| c.list()) {
        Some([a, b]) => {
            fill(d, at.0, at.0 + w / 2.0, rgb(*a));
            fill(d, at.0 + w / 2.0, at.0 + w, rgb(*b));
        }
        Some([a]) => fill(d, at.0, at.0 + w, rgb(*a)),
        _ => {}
    }
    d.rect((at.0, at.1 - h / 2.0), (at.0 + w, at.1 + h / 2.0), ecad_electrical::cables::words::WIRE_WEIGHT, None);
}

/// A definition's graphics fitted into the box `cell` (paper millimetres, centred).
fn symbol(d: &mut Draw, doc: &Document, def: DefinitionId, cell: R) {
    let Some(def) = doc.definitions().get(&def) else { return };
    let mut lo = (f64::INFINITY, f64::INFINITY);
    let mut hi = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    let mut grow = |p: Pt, r: f64| {
        if p.x.is_finite() && p.y.is_finite() && r.is_finite() {
            lo = (lo.0.min(p.x - r), lo.1.min(p.y - r));
            hi = (hi.0.max(p.x + r), hi.1.max(p.y + r));
        }
    };
    let graphics: Vec<&Primitive> = def.graphics.iter().take(500).collect();
    for g in &graphics {
        match g {
            Primitive::Line { a, b } => {
                grow(*a, 0.0);
                grow(*b, 0.0);
            }
            Primitive::Polyline { vertices, .. } => vertices.iter().take(500).for_each(|v| grow(*v, 0.0)),
            Primitive::Arc { center, radius, .. } | Primitive::Circle { center, radius } => grow(*center, radius.abs()),
            _ => {}
        }
    }
    let (w, h) = (hi.0 - lo.0, hi.1 - lo.1);
    if !(w.is_finite() && h.is_finite()) || (w <= 0.0 && h <= 0.0) {
        return;
    }
    let (cl, cb, cr, ct) = cell;
    let k = ((cr - cl) / w.max(1e-9)).min((ct - cb) / h.max(1e-9));
    let k = if k.is_finite() { k } else { 1.0 };
    let (ox, oy) = ((cl + cr) / 2.0 - (lo.0 + w / 2.0) * k, (cb + ct) / 2.0 - (lo.1 + h / 2.0) * k);
    let map = |p: &Pt| Pt::new(p.x * k + ox, p.y * k + oy);
    for g in graphics {
        let mapped = match g {
            Primitive::Line { a, b } => {
                let (a, b) = (map(a), map(b));
                Primitive::Line { a: d.p(a.x, a.y), b: d.p(b.x, b.y) }
            }
            Primitive::Polyline { vertices, closed } => {
                Primitive::Polyline { vertices: vertices.iter().take(500).map(&map).map(|v| d.p(v.x, v.y)).collect(), closed: *closed }
            }
            Primitive::Arc { center, radius, start, end } => {
                let c = map(center);
                Primitive::Arc { center: d.p(c.x, c.y), radius: radius.abs() * k * d.k, start: *start, end: *end }
            }
            Primitive::Circle { center, radius } => {
                let c = map(center);
                Primitive::Circle { center: d.p(c.x, c.y), radius: radius.abs() * k * d.k }
            }
            _ => continue,
        };
        d.out.push(Part { kind: kind_for_primitive(&mapped), weight: Some(ecad_electrical::pages::frames::THIN_WEIGHT), aci: None, layer: LAYER });
    }
}

/// A drawn mark's cell glyph: a cable's slash, a continuation's arrow.
fn mark(d: &mut Draw, cell: &Cell, r: R) {
    let (l, b, rr, t) = r;
    let (cx, cy) = ((l + rr) / 2.0, (b + t) / 2.0);
    match cell {
        Cell::Cable => {
            d.line((l, cy), (rr, cy));
            d.line((cx - 3.0, b + 1.0), (cx + 3.0, t - 1.0));
        }
        Cell::Continuation => {
            d.line((l, cy), (rr - 3.0, cy));
            d.line((rr - 3.0, cy + 2.0), (rr, cy));
            d.line((rr - 3.0, cy - 2.0), (rr, cy));
            d.line((rr - 3.0, cy - 2.0), (rr - 3.0, cy + 2.0));
        }
        Cell::Symbol(..) => {}
    }
}

/// The cover's content inside the frame `input` (paper millimetres, sheet origin at `x0`), and how
/// the legend fitted.
pub(crate) fn cover(doc: &Document, input: &FrameInput, x0: f64) -> (Vec<Part>, LegendFit) {
    let k = ecad_model::document::sheet::unit_scale(input.unit);
    let mut d = Draw { k, x0, out: Vec::new() };
    let draws = DrawStyles::new(doc);
    let draw = |kind| ecad_drawing_style::text::fonts::text_draw_of(&draws, kind, input.unit);
    let cover_draw = draw(TextKind::Cover);
    let Layout {
        area: (l, _, r, _),
        s,
        heading_y,
        customer_y,
        blocks_y,
        gap,
        bl,
        br,
        half,
        line_h,
        notes: notes_list,
        wires,
        row1,
        revs,
        codes,
        row3,
        legend,
        cells,
        columns,
        legend_h,
        cell: (cw, ch, row_h),
        take,
        fit,
    } = layout(doc, input, cover_draw.mm);
    let cover = draw(TextKind::Cover);
    let sized = |key: &str| TextDraw { mm: cover.mm * cover_factor(key) * s, ..cover };
    let heading_draw = sized("front.cover.block_title");
    let small = sized("front.cover.body");
    let caption = sized("front.cover.column_head");
    let on = |b: Block| b.on(doc);
    let prop = |k: &str| ecad_electrical::pages::frames::project_text(doc, k).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());

    // Heading and customer line, centred.
    let title_text = prop(ecad_electrical::pages::frames::TITLE).unwrap_or_default();
    let heading = ecad_model::registry::resolve(doc, &ecad_electrical::defaults::pages::HEADING, ecad_model::registry::Scope::project()).text();
    let heading = heading.trim();
    let heading = if heading.eq_ignore_ascii_case(ecad_electrical::defaults::pages::NO_HEADING) { "" } else { heading };
    let line = match (title_text.is_empty(), heading.is_empty()) {
        (false, false) => format!("{title_text} · {heading}"),
        (false, true) => title_text,
        (true, _) => heading.to_string(),
    };
    let mid = (l + r) / 2.0;
    d.text((mid, heading_y), cover_draw, &line, HAlign::Center, None, Some(r - l));
    if on(Block::CustomerSite) {
        let cs: Vec<String> = [prop(ecad_electrical::pages::frames::CUSTOMER), prop("site")].into_iter().flatten().collect();
        if !cs.is_empty() {
            d.text((mid, customer_y), sized("front.cover.customer"), &cs.join(" · "), HAlign::Center, Some(LABEL_ACI), Some(r - l));
        }
    }
    let mut y = blocks_y;

    if row1 > 0.0 {
        let top = y;
        let bottom = top - row1;
        let mut x = bl;
        if on(Block::Notes) {
            let rect = (x, bottom, x + half, top);
            let mut yy = title(&mut d, rect, "Notes", heading_draw, s);
            for (i, n) in notes_list.iter().enumerate() {
                if yy < bottom + 2.0 * s {
                    break;
                }
                d.text((x + 4.0 * s, yy), small, &format!("{}. {n}", i + 1), HAlign::Left, None, Some(half - 8.0 * s));
                yy -= line_h;
            }
            outline(&mut d, rect, TYPED_ACI, "", caption);
            x += half + gap;
        }
        if on(Block::WireColours) {
            let rect = (x, bottom, x + half, top);
            let mut yy = title(&mut d, rect, "Wire colours", heading_draw, s);
            if wires.is_empty() {
                d.text((x + 4.0 * s, yy), small, "No wires yet.", HAlign::Left, Some(LABEL_ACI), Some(half - 8.0 * s));
            }
            for w in &wires {
                if yy < bottom + 2.0 * s {
                    break;
                }
                swatch(&mut d, (x + 4.0 * s, yy), 9.0 * s, 3.0 * s, w.colour.as_ref());
                d.text((x + 17.0 * s, yy), small, &w.text(), HAlign::Left, None, Some(half - 21.0 * s));
                yy -= line_h;
            }
            outline(&mut d, rect, DERIVED_ACI, "from wire categories", caption);
        }
        y = bottom - gap;
    }
    if on(Block::SymbolLegend) {
        let rect = (bl, y - legend_h, br, y);
        let used = cells.iter().filter(|c| matches!(c, Cell::Symbol(..))).count();
        let yy = title(&mut d, rect, "Symbol legend", heading_draw, s);
        if cells.is_empty() {
            d.text((bl + 4.0 * s, yy), small, "No symbols yet.", HAlign::Left, Some(LABEL_ACI), Some(br - bl));
        }
        let col_w = (br - bl - 8.0 * s) / columns as f64;
        for (i, c) in cells.iter().take(take).enumerate() {
            let cx = bl + 4.0 * s + col_w * (i % columns) as f64;
            let cy = yy + line_h / 2.0 - row_h * (i / columns) as f64 - row_h / 2.0;
            let cell = (cx, cy - ch / 2.0, cx + cw, cy + ch / 2.0);
            match c {
                Cell::Symbol(id, _) => symbol(&mut d, doc, *id, cell),
                m => mark(&mut d, m, cell),
            }
            let name = legend.get(i).map_or_else(|| c.name().to_string(), |(_, r)| r.label.clone());
            d.text((cx + cw + 4.0 * s, cy), small, &name, HAlign::Left, None, Some(col_w - cw - 6.0 * s));
        }
        if take < cells.len() {
            let i = take;
            let cx = bl + 4.0 * s + col_w * (i % columns) as f64;
            let cy = yy + line_h / 2.0 - row_h * (i / columns) as f64 - row_h / 2.0;
            d.text((cx, cy), small, &format!("and {} more", cells.len() - take), HAlign::Left, Some(LABEL_ACI), Some(col_w));
        }
        outline(&mut d, rect, DERIVED_ACI, &format!("symbols used in this project · {used}"), caption);
        y -= legend_h + gap;
    }
    if row3 > 0.0 {
        let top = y;
        let bottom = top - row3;
        let third = (br - bl - gap) / 3.0;
        if on(Block::Revisions) {
            let rect = (bl, bottom, bl + 2.0 * third + gap / 2.0, top);
            let mut yy = title(&mut d, rect, "Revisions", heading_draw, s);
            let (x0, x1) = (rect.0 + 4.0 * s, rect.2 - 4.0 * s);
            // Rev 15, Description 120, By 40, Date 40 on A3, the description taking what is left.
            let fixed = [15.0 * s, 0.0, 40.0 * s, 40.0 * s];
            let desc = (x1 - x0 - fixed.iter().sum::<f64>()).max(10.0 * s);
            let widths = [fixed[0], desc, fixed[2], fixed[3]];
            let heads = ["Rev", "Description", "By", "Date"];
            let mut x = x0;
            for (h, w) in heads.iter().zip(widths) {
                d.text((x + 1.0 * s, yy), caption, h, HAlign::Left, Some(LABEL_ACI), Some(w - 2.0 * s));
                x += w;
            }
            yy -= 3.0 * s;
            d.line((x0, yy), (x1, yy));
            yy -= line_h / 2.0;
            if revs.is_empty() {
                d.text((x0 + 1.0 * s, yy), small, "No revisions issued yet.", HAlign::Left, Some(LABEL_ACI), Some(x1 - x0));
            }
            // Oldest first, ending at the newest.
            for rv in revs.iter().rev() {
                let cells = [rv.rev().to_string(), rv.get("description").to_string(), rv.get("drawn").to_string(), rv.get("date").to_string()];
                let mut x = x0;
                for (c, w) in cells.iter().zip(widths) {
                    d.text((x + 1.0 * s, yy), small, c, HAlign::Left, None, Some(w - 2.0 * s));
                    x += w;
                }
                d.line((x0, yy - line_h / 2.0), (x1, yy - line_h / 2.0));
                yy -= line_h;
            }
            outline(&mut d, rect, DERIVED_ACI, "from History", caption);
        }
        if on(Block::StructureCodes) {
            let rect = (bl + 2.0 * third + gap * 1.5, bottom, br, top);
            let yy = title(&mut d, rect, "Structure codes", heading_draw, s);
            let cw = (rect.2 - rect.0 - 8.0 * s) / 2.0;
            for (j, list) in [&codes.0, &codes.1].into_iter().enumerate() {
                let x = rect.0 + 4.0 * s + cw * j as f64;
                for (i, c) in list.iter().take(8).enumerate() {
                    d.text((x, yy - line_h * i as f64), small, c, HAlign::Left, None, Some(cw - 2.0 * s));
                }
            }
            outline(&mut d, rect, DERIVED_ACI, "from functions and locations", caption);
        }
    }
    (d.out, fit)
}
