//! Drawing the PLC sheets (brief 051, design 051 §4): the I/O tables and the rack overview as the
//! generated page's parts ([`crate::generated::Draw`]), in paper millimetres. Packing and the
//! sheet counts are [`crate::plc::sheets`].

use cadcraft_engine::doc::HAlign;
use ecad_model::{Document, LengthUnit};

use crate::pages::frame::{LABEL_ACI, THIN_WEIGHT, TextDraw};
use crate::generated::{Draw, row_mm};
use crate::plc::overview::{self, Square, Tile, TileKind};
use crate::plc::sheets::*;
use crate::plc::{iolist, kept::Plc};
use crate::text_style::{Look, TextKind};

/// Sheet `i` of the I/O list page.
pub(crate) fn io_sheet(doc: &Document, plc: Option<&Plc>, d: &mut Draw, looks: &Look, unit: LengthUnit, a: Area, i: usize) {
    let look = |k| TextDraw::of(looks, k, unit);
    let (heading, header, cell) = (look(TextKind::TableHeading), look(TextKind::TableHeader), look(TextKind::Table));
    let cards = plc.map(iolist::cards_in_order).unwrap_or_default();
    let Some(plc) = plc.filter(|_| !cards.is_empty()) else {
        d.text((a.l + 1.0, a.t - row_mm() / 2.0), cell, iolist::NO_CARDS, HAlign::Left, None, Some(a.r - a.l - 2.0));
        return;
    };
    let sheets = io_pack(plc);
    let Some(segs) = sheets.get(i) else { return };
    let spare = ecad_electrical::defaults::resolve(doc, &ecad_electrical::defaults::plc::SPARE_TEXT, ecad_electrical::defaults::Scope::project()).text();
    let widths: Vec<f64> = IO_COLUMNS.iter().map(|(_, w)| w * a.scale).collect();
    let total: f64 = widths.iter().sum();
    let mut y = a.t;
    for (k, seg) in segs.iter().enumerate() {
        let Some(card) = cards.get(seg.group) else { continue };
        if k > 0 {
            y -= row_mm();
        }
        let rows = iolist::rows_of(doc, plc, card);
        let mut words = group_heading(doc, card, &rows);
        if seg.continued {
            words.push_str(" (continued)");
        }
        d.text((a.l, y - row_mm() / 2.0), heading, &words, HAlign::Left, None, Some(a.r - a.l));
        y -= row_mm();
        let top = y;
        let mut x = a.l;
        for ((name, _), w) in IO_COLUMNS.iter().zip(&widths) {
            d.text((x + 1.0, y - row_mm() / 2.0), header, name, HAlign::Left, Some(LABEL_ACI), Some(w - 2.0));
            x += w;
        }
        y -= row_mm();
        d.line((a.l, y), (a.l + total, y));
        for r in rows.get(seg.start..seg.end).unwrap_or_default() {
            let mut x = a.l;
            for (v, w) in iolist::io_cells(r, &spare).iter().zip(&widths) {
                d.text((x + 1.0, y - row_mm() / 2.0), cell, v, HAlign::Left, None, Some(w - 2.0));
                x += w;
            }
            y -= row_mm();
            d.line((a.l, y), (a.l + total, y));
        }
        // The box: its top, and every column line from top to bottom.
        d.line((a.l, top), (a.l + total, top));
        let mut x = a.l;
        d.line((x, top), (x, y));
        for w in &widths {
            x += w;
            d.line((x, top), (x, y));
        }
        if y < a.b {
            break;
        }
    }
}

/// A dashed rectangle (024's dashed look, drawn as dashes).
fn dashed_rect(d: &mut Draw, (x0, y0): (f64, f64), (x1, y1): (f64, f64)) {
    let (dash, gap) = (2.0, 1.5);
    let mut side = |a: (f64, f64), b: (f64, f64)| {
        let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
        if !(len.is_finite() && len > 0.0) {
            return;
        }
        let (ux, uy) = ((b.0 - a.0) / len, (b.1 - a.1) / len);
        let mut s = 0.0;
        // Bounded: a tile's side is 46 mm at most.
        for _ in 0..64 {
            if s >= len {
                break;
            }
            let e = (s + dash).min(len);
            d.line((a.0 + ux * s, a.1 + uy * s), (a.0 + ux * e, a.1 + uy * e));
            s = e + gap;
        }
    };
    side((x0, y0), (x1, y0));
    side((x1, y0), (x1, y1));
    side((x1, y1), (x0, y1));
    side((x0, y1), (x0, y0));
}

/// One square in ink at (x, y) its lower left: used filled (hatched), spare open, reserved half
/// filled on the diagonal.
fn square(d: &mut Draw, x: f64, y: f64, s: Square) {
    d.rect((x, y), (x + SQUARE, y + SQUARE), THIN_WEIGHT, None);
    let step = SQUARE / 6.0;
    match s {
        Square::Used => {
            for k in 1..6 {
                let yy = y + step * k as f64;
                d.line((x, yy), (x + SQUARE, yy));
            }
        }
        Square::Reserved => {
            // The lower-left triangle, hatched.
            for k in 1..6 {
                let yy = y + step * k as f64;
                d.line((x, yy), (x + SQUARE - step * k as f64, yy));
            }
            d.line((x, y + SQUARE), (x + SQUARE, y));
        }
        Square::Spare | Square::Check => {}
    }
}

/// One tile with its top-left corner at (x, y).
fn tile(doc: &Document, d: &mut Draw, looks: &Look, unit: LengthUnit, t: &Tile, x: f64, y: f64) {
    let look = |k| TextDraw::of(looks, k, unit);
    let room = Some(TILE_W - 3.0);
    let at = |dy: f64| (x + 1.5, y - dy);
    if t.kind == TileKind::Free {
        dashed_rect(d, (x, y - TILE_H), (x + TILE_W, y));
        d.text(at(4.0), look(TextKind::TableHeader), &t.head, HAlign::Left, Some(LABEL_ACI), room);
        d.text(at(10.0), look(TextKind::Tag), &t.tag, HAlign::Left, None, room);
        return;
    }
    d.rect((x, y - TILE_H), (x + TILE_W, y), THIN_WEIGHT, None);
    // Brief 056 (design 056 §8): `Slot 2` and the kind words, the tag, the description (else the
    // type name), the squares, the range, then `9 of 16 in use` and the place of the first run.
    let half = Some(TILE_W / 2.0 - 2.0);
    d.text(at(4.0), look(TextKind::TableHeader), &t.head, HAlign::Left, Some(LABEL_ACI), half);
    d.text((x + TILE_W - 1.5, y - 4.0), look(TextKind::TableHeader), &t.kind_words, HAlign::Right, Some(LABEL_ACI), half);
    d.text(at(10.0), look(TextKind::Tag), &t.tag, HAlign::Left, None, room);
    let title = if t.title.is_empty() { &t.what } else { &t.title };
    d.text(at(15.5), look(TextKind::Table), title, HAlign::Left, None, room);
    let shown = SQUARES_PER_ROW * SQUARE_ROWS;
    for (k, s) in t.squares.iter().take(shown).enumerate() {
        let (row, col) = (k / SQUARES_PER_ROW, k % SQUARES_PER_ROW);
        let sx = x + 1.5 + col as f64 * (SQUARE + SQUARE_GAP);
        let sy = y - 18.5 - SQUARE - row as f64 * (SQUARE + SQUARE_GAP);
        square(d, sx, sy, *s);
    }
    if t.squares.len() > shown {
        let more = format!("+{}", t.squares.len() - shown);
        d.text((x + TILE_W - 1.5, y - 18.5 - SQUARE / 2.0), look(TextKind::Table), &more, HAlign::Right, None, Some(8.0));
    }
    let rows = t.squares.len().min(shown).div_ceil(SQUARES_PER_ROW) as f64;
    let range_at = if t.squares.is_empty() { 20.5 } else { 18.5 + rows * (SQUARE + SQUARE_GAP) + 3.0 };
    d.text(at(range_at), look(TextKind::Table), &t.range, HAlign::Left, None, room);
    let foot = match (&t.place, t.kind) {
        (_, TileKind::Card(_)) if !t.drawn => "not drawn".into(),
        (Some(p), _) => format!("\u{2192}{p}"),
        (None, _) => String::new(),
    };
    let _ = doc;
    let in_use = t.in_use_words();
    if !in_use.is_empty() {
        d.text((x + 1.5, y - TILE_H + 3.0), look(TextKind::Table), &in_use, HAlign::Left, None, half);
    }
    d.text((x + TILE_W - 1.5, y - TILE_H + 3.0), look(TextKind::Reference), &foot, HAlign::Right, None, half);
}

/// Sheet `i` of the `plcrack` page.
pub(crate) fn rack_sheet(doc: &Document, plc: Option<&Plc>, d: &mut Draw, looks: &Look, unit: LengthUnit, a: Area, i: usize) {
    let look = |k| TextDraw::of(looks, k, unit);
    let Some(plc) = plc.filter(|p| !p.racks.is_empty() || !p.cards.is_empty()) else {
        d.text((a.l + 1.0, a.t - row_mm() / 2.0), look(TextKind::Table), NO_RACKS, HAlign::Left, None, Some(a.r - a.l - 2.0));
        return;
    };
    let (groups, sheets, per) = rack_pack(plc, a.r - a.l, a.t - a.b);
    let Some(segs) = sheets.get(i) else { return };
    let mut y = a.t;
    for (k, seg) in segs.iter().enumerate() {
        let Some((rack, _)) = groups.get(seg.group) else { continue };
        if k > 0 {
            y -= TILE_ROW_GAP;
        }
        let (words, tiles) = match rack.and_then(|r| plc.racks.get(&r)) {
            Some(r) => (rack_heading(doc, plc, r), overview::rack_tiles(doc, plc, r)),
            None => ("No rack".to_string(), overview::unracked_tiles(doc, plc)),
        };
        let words = if seg.continued { format!("{words} (continued)") } else { words };
        d.text((a.l, y - RACK_HEAD / 2.0), look(TextKind::TableHeading), &words, HAlign::Left, None, Some(a.r - a.l));
        y -= RACK_HEAD;
        for row in seg.start..seg.end {
            for (c, t) in tiles.iter().skip(row * per).take(per).enumerate() {
                tile(doc, d, looks, unit, t, a.l + c as f64 * (TILE_W + TILE_GAP), y);
            }
            y -= TILE_H + TILE_ROW_GAP;
        }
        y += TILE_ROW_GAP;
    }
}
