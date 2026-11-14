//! The PLC sheets of the book (brief 051, design 051 §4): the I/O list as one ruled table per card,
//! cards packed on sheets (§4.1), and the rack overview `plcrack` with the tiles of
//! [`super::overview`] in ink (§4.2).
//!
//! # Semantics
//! - [`pack`]: groups follow each other on a sheet with a gap; a group goes to the next sheet only
//!   when it does not fit in what is left; a group taller than a whole sheet splits, its heading
//!   repeated with `(continued)`. The sheet count comes from kept counts only (no rows made).
//! - Drawn parts are the generated page's ([`crate::generated::Draw`]), in paper millimetres.
//!
//! Excludes: the CSV (unchanged, [`super::iolist::csv_table`]); Checks (not printed).

use cadcraft_engine::doc::HAlign;
use ecad_model::{Document, LengthUnit, PageId};

use super::overview::{self, Square, Tile, TileKind};
use super::{CardData, Plc, iolist};
use crate::frame::{LABEL_ACI, THIN_WEIGHT, TextLook};
use crate::generated::{Draw, ROW_MM, ROWS_PER_SHEET};
use crate::text_style::{Look, TextKind};

/// A sheet's drawing area in paper millimetres, and the A3 column scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Area {
    pub l: f64,
    pub b: f64,
    pub r: f64,
    pub t: f64,
    pub scale: f64,
}

/// The I/O table's columns with their A3 widths (design 051 §4.1): Slot and Card are in the heading.
pub const IO_COLUMNS: [(&str, f64); 8] =
    [("Ch", 12.0), ("Address", 26.0), ("Pins", 22.0), ("Description", 100.0), ("State", 18.0), ("Wired to", 80.0), ("Wire", 26.0), ("Place", 30.0)];

/// One group's part on one sheet: rows `start..end` of group `group`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Seg {
    pub group: usize,
    pub start: usize,
    pub end: usize,
    /// A later part of a group that began on an earlier sheet.
    pub continued: bool,
}

/// Lay out groups of `rows[g]` rows on sheets of `height`: each part costs `head` (its heading and
/// header row) plus `row` per row, with `gap` between parts on one sheet. Never an empty sheet but
/// the only one; each sheet holds at least one row.
pub fn pack(rows: &[usize], head: f64, row: f64, gap: f64, height: f64) -> Vec<Vec<Seg>> {
    let row = if row.is_finite() && row > 0.0 { row } else { 1.0 };
    let mut sheets: Vec<Vec<Seg>> = vec![Vec::new()];
    let mut used = 0.0;
    let eps = 1e-9;
    for (group, &n) in rows.iter().enumerate() {
        let mut start = 0usize;
        let mut continued = false;
        loop {
            let gap_here = if used > 0.0 { gap } else { 0.0 };
            let room = height - used - gap_here;
            let whole = head + row * n.saturating_sub(start) as f64;
            if whole <= room + eps {
                if let Some(s) = sheets.last_mut() {
                    s.push(Seg { group, start, end: n, continued });
                }
                used += gap_here + whole;
                break;
            }
            if used > 0.0 && whole <= height + eps {
                sheets.push(Vec::new());
                used = 0.0;
                continue;
            }
            let fit = if room > head { ((room - head) / row + eps).floor() as usize } else { 0 };
            let take = if used > 0.0 { fit } else { fit.max(1) }.min(n.saturating_sub(start));
            if take == 0 && used > 0.0 {
                sheets.push(Vec::new());
                used = 0.0;
                continue;
            }
            if let Some(s) = sheets.last_mut() {
                s.push(Seg { group, start, end: start + take, continued });
            }
            start += take;
            continued = true;
            sheets.push(Vec::new());
            used = 0.0;
            if start >= n {
                break;
            }
        }
    }
    while sheets.len() > 1 && sheets.last().is_some_and(Vec::is_empty) {
        sheets.pop();
    }
    sheets
}

/// The I/O list's sheets from the kept row counts: (lines a sheet holds) per [`ROWS_PER_SHEET`].
pub fn io_pack(plc: &Plc) -> Vec<Vec<Seg>> {
    let rows: Vec<usize> = iolist::cards_in_order(plc).iter().map(|c| c.rows).collect();
    // A heading line and the header row; one empty row between groups.
    pack(&rows, 2.0, 1.0, 1.0, ROWS_PER_SHEET as f64)
}

/// `-A01 slot 2 · -A03 · DQ 16 · <part number> · 9 used, 1 reserved, 6 spare` (design 051 §4.1).
pub fn group_heading(doc: &Document, card: &CardData, rows: &[iolist::Row]) -> String {
    let (u, r, s) = iolist::summary(rows);
    let caption = match super::caption(doc, card.id) {
        Some((c, _)) => c,
        None => "No rack".into(),
    };
    let mut parts = vec![caption, card.tag.clone(), format!("{} {}", card.ty.code(), card.channels)];
    if let Some(p) = card.part_number.clone().filter(|p| !p.is_empty()) {
        parts.push(p);
    }
    parts.push(format!("{u} used, {r} reserved, {s} spare"));
    parts.join(" \u{b7} ")
}

/// The cells of one I/O row, in [`IO_COLUMNS`] order.
fn io_cells(r: &iolist::Row, spare: &str) -> [String; 8] {
    let desc = match (&r.text, r.state) {
        (Some(t), _) => t.clone(),
        (None, "spare") => spare.to_string(),
        (None, _) => String::new(),
    };
    [
        r.channel.to_string(),
        iolist::address_words(r),
        iolist::pins_words(&r.pins),
        desc,
        r.state.to_string(),
        iolist::targets_words(&r.targets),
        r.wire.clone().unwrap_or_default(),
        r.place.clone().unwrap_or_default(),
    ]
}

/// Sheet `i` of the I/O list page.
pub(crate) fn io_sheet(doc: &Document, plc: Option<&Plc>, d: &mut Draw, looks: &Look, unit: LengthUnit, a: Area, i: usize) {
    let look = |k| TextLook::of(looks, k, unit);
    let (heading, header, cell) = (look(TextKind::TableHeading), look(TextKind::TableHeader), look(TextKind::Table));
    let cards = plc.map(iolist::cards_in_order).unwrap_or_default();
    let Some(plc) = plc.filter(|_| !cards.is_empty()) else {
        d.text((a.l + 1.0, a.t - ROW_MM / 2.0), cell, iolist::NO_CARDS, HAlign::Left, None, Some(a.r - a.l - 2.0));
        return;
    };
    let sheets = io_pack(plc);
    let Some(segs) = sheets.get(i) else { return };
    let spare = crate::defaults::resolve(doc, &crate::defaults::plc::SPARE_TEXT, crate::defaults::Scope::project()).text();
    let widths: Vec<f64> = IO_COLUMNS.iter().map(|(_, w)| w * a.scale).collect();
    let total: f64 = widths.iter().sum();
    let mut y = a.t;
    for (k, seg) in segs.iter().enumerate() {
        let Some(card) = cards.get(seg.group) else { continue };
        if k > 0 {
            y -= ROW_MM;
        }
        let rows = iolist::rows_of(doc, plc, card);
        let mut words = group_heading(doc, card, &rows);
        if seg.continued {
            words.push_str(" (continued)");
        }
        d.text((a.l, y - ROW_MM / 2.0), heading, &words, HAlign::Left, None, Some(a.r - a.l));
        y -= ROW_MM;
        let top = y;
        let mut x = a.l;
        for ((name, _), w) in IO_COLUMNS.iter().zip(&widths) {
            d.text((x + 1.0, y - ROW_MM / 2.0), header, name, HAlign::Left, Some(LABEL_ACI), Some(w - 2.0));
            x += w;
        }
        y -= ROW_MM;
        d.line((a.l, y), (a.l + total, y));
        for r in rows.get(seg.start..seg.end).unwrap_or_default() {
            let mut x = a.l;
            for (v, w) in io_cells(r, &spare).iter().zip(&widths) {
                d.text((x + 1.0, y - ROW_MM / 2.0), cell, v, HAlign::Left, None, Some(w - 2.0));
                x += w;
            }
            y -= ROW_MM;
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

// ---------------------------------------------------------------- the rack sheet (§4.2)

/// A tile's box, mm.
pub const TILE_W: f64 = 40.0;
pub const TILE_H: f64 = 46.0;
/// Between tiles in a row, and between rows of tiles, mm.
pub const TILE_GAP: f64 = 2.0;
pub const TILE_ROW_GAP: f64 = 6.0;
/// A rack's heading line, mm.
pub const RACK_HEAD: f64 = 8.0;
/// A channel square and the gap between squares, mm; squares in a row.
pub const SQUARE: f64 = 3.0;
pub const SQUARE_GAP: f64 = 1.0;
pub const SQUARES_PER_ROW: usize = 8;
/// Rows of squares a tile prints (32 channels); more are counted as `+n`.
pub const SQUARE_ROWS: usize = 4;

/// Tiles in a row of an area `width` wide (9 on A3).
pub fn tiles_per_row(width: f64) -> usize {
    (((width + TILE_GAP) / (TILE_W + TILE_GAP)).floor().max(1.0)) as usize
}

/// The drawing area of `page`'s sheet in paper millimetres (left, bottom, right, top), as
/// [`crate::generated`] lays it out.
fn page_area(doc: &Document, page: PageId) -> (f64, f64, f64, f64) {
    let spec = ecad_model::sheet::sheet_of(doc, page);
    let m = ecad_model::sheet::MARGIN_MM;
    (
        m + 5.0,
        m + ecad_model::sheet::TITLE_BLOCK_MM + 5.0,
        f64::from(spec.width) - m - 5.0,
        f64::from(spec.height) - ecad_model::sheet::TOP_MARGIN_MM - 5.0,
    )
}

/// The groups of the rack sheet: each rack by tag, then the cards in no rack. (rack or none, tiles)
fn rack_groups(plc: &Plc) -> Vec<(Option<ecad_model::ComponentId>, usize)> {
    let mut out: Vec<(Option<ecad_model::ComponentId>, usize)> =
        plc.racks_by_tag().into_iter().map(|r| (Some(r.id), overview::tile_count(plc, r))).collect();
    let n = plc.unracked_by_tag().len().min(overview::MAX_TILES);
    if n > 0 {
        out.push((None, n));
    }
    out
}

/// The rack sheet's layout: per sheet, the parts of groups in rows of tiles.
fn rack_pack(plc: &Plc, width: f64, height: f64) -> (Vec<(Option<ecad_model::ComponentId>, usize)>, Vec<Vec<Seg>>, usize) {
    let groups = rack_groups(plc);
    let per = tiles_per_row(width);
    let rows: Vec<usize> = groups.iter().map(|(_, n)| n.div_ceil(per)).collect();
    let sheets = pack(&rows, RACK_HEAD, TILE_H + TILE_ROW_GAP, TILE_ROW_GAP, height);
    (groups, sheets, per)
}

/// How many sheets the `plcrack` page `page` needs, from kept counts.
pub fn rack_sheets(doc: &Document, page: PageId, plc: &Plc) -> usize {
    let (l, b, r, t) = page_area(doc, page);
    rack_pack(plc, r - l, t - b).1.len().max(1)
}

/// `-A01 · CPU · <part number> · 32 slots, 6 used`.
fn rack_heading(doc: &Document, plc: &Plc, rack: &super::RackData) -> String {
    let mut parts = vec![rack.tag.clone(), rack.ty.code().to_string()];
    if let Some(p) = doc.components.get(&rack.id).and_then(|x| x.properties.get("part_number")).map(|v| v.trim()).filter(|v| !v.is_empty()) {
        parts.push(p.to_string());
    }
    parts.push(overview::rack_words(plc, rack));
    parts.join(" \u{b7} ")
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
    let look = |k| TextLook::of(looks, k, unit);
    let room = Some(TILE_W - 3.0);
    let at = |dy: f64| (x + 1.5, y - dy);
    if t.kind == TileKind::Free {
        dashed_rect(d, (x, y - TILE_H), (x + TILE_W, y));
        d.text(at(4.0), look(TextKind::TableHeader), &t.head, HAlign::Left, Some(LABEL_ACI), room);
        d.text(at(10.0), look(TextKind::Tag), &t.tag, HAlign::Left, None, room);
        return;
    }
    d.rect((x, y - TILE_H), (x + TILE_W, y), THIN_WEIGHT, None);
    d.text(at(4.0), look(TextKind::TableHeader), &t.head, HAlign::Left, Some(LABEL_ACI), room);
    d.text(at(10.0), look(TextKind::Tag), &t.tag, HAlign::Left, None, room);
    d.text(at(15.5), look(TextKind::Table), &t.what, HAlign::Left, None, room);
    d.text(at(20.5), look(TextKind::Table), &t.range, HAlign::Left, None, room);
    let shown = SQUARES_PER_ROW * SQUARE_ROWS;
    for (k, s) in t.squares.iter().take(shown).enumerate() {
        let (row, col) = (k / SQUARES_PER_ROW, k % SQUARES_PER_ROW);
        let sx = x + 1.5 + col as f64 * (SQUARE + SQUARE_GAP);
        let sy = y - 24.0 - SQUARE - row as f64 * (SQUARE + SQUARE_GAP);
        square(d, sx, sy, *s);
    }
    if t.squares.len() > shown {
        let more = format!("+{}", t.squares.len() - shown);
        d.text((x + TILE_W - 1.5, y - 24.0 - SQUARE / 2.0), look(TextKind::Table), &more, HAlign::Right, None, Some(8.0));
    }
    let foot = match (&t.place, t.kind) {
        (_, TileKind::Card(_)) if !t.drawn => "not drawn".into(),
        (Some(p), _) => format!("\u{2192}{p}"),
        (None, _) => String::new(),
    };
    let _ = doc;
    d.text((x + 1.5, y - TILE_H + 3.0), look(TextKind::Reference), &foot, HAlign::Left, None, room);
}

/// Sheet `i` of the `plcrack` page.
pub(crate) fn rack_sheet(doc: &Document, plc: Option<&Plc>, d: &mut Draw, looks: &Look, unit: LengthUnit, a: Area, i: usize) {
    let look = |k| TextLook::of(looks, k, unit);
    let Some(plc) = plc.filter(|p| !p.racks.is_empty() || !p.cards.is_empty()) else {
        d.text((a.l + 1.0, a.t - ROW_MM / 2.0), look(TextKind::Table), NO_RACKS, HAlign::Left, None, Some(a.r - a.l - 2.0));
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

/// What the `plcrack` page says when the project has no PLC racks or cards.
pub const NO_RACKS: &str = "No PLC racks.";

/// Whether `ops` (just committed; `doc` is after them) move a place the `plcrack` sheet prints
/// (review of #89, 3): a PLC card's or rack's symbol moved, or a page holding one renumbered,
/// resized, or any page moved in the book. Only the ops' own items and the pages they name are
/// read.
pub fn rack_places_touched(doc: &Document, plc: &Plc, ops: &[ecad_model::Op]) -> bool {
    use ecad_model::{ObjectRef, Op, Subject};
    let ours = |c: ecad_model::ComponentId| plc.cards.contains_key(&c) || plc.racks.contains_key(&c);
    let page_holds = |p: PageId| doc.representations_on(p).any(|(_, r)| matches!(r.subject, Subject::Component(c) if ours(c)));
    for op in ops {
        match op {
            Op::ModifyItem { id, .. } => {
                if let Some((_, r)) = doc.representation_of_item(*id)
                    && let Subject::Component(c) = r.subject
                    && ours(c)
                {
                    return true;
                }
            }
            Op::SetProperty { object: ObjectRef::Page(p), key, .. }
                if key == ecad_model::book::NUMBER_KEY || key == ecad_model::sheet::SHEET_KEY || key == ecad_model::book::FOLDER_KEY =>
            {
                if page_holds(*p) {
                    return true;
                }
            }
            _ => {}
        }
    }
    // A page moved in the book (rare): every place may change its page number.
    ecad_model::book::moves_pages(ops) && !(plc.cards.is_empty() && plc.racks.is_empty())
}
