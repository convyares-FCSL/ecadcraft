//! The PLC sheets of the book (brief 051, design 051 §4): the I/O list as one ruled table per card,
//! cards packed on sheets (§4.1), and the rack overview `plcrack` with the tiles of
//! [`super::overview`] in ink (§4.2).
//!
//! # Semantics
//! - [`pack`]: groups follow each other on a sheet with a gap; a group goes to the next sheet only
//!   when it does not fit in what is left; a group taller than a whole sheet splits, its heading
//!   repeated with `(continued)`. The sheet count comes from kept counts only (no rows made).
//! - Drawing the sheets is ecad-cadcraft's (`draw::plc::sheets`).
//!
//! Excludes: the CSV (unchanged, [`super::iolist::csv_table`]); Checks (not printed).

use ecad_model::{Document, PageId};

use super::overview::{self, Square, Tile, TileKind};
use super::{CardData, iolist, kept::Plc};
use crate::generated::rows_per_sheet;

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

/// The I/O list's sheets from the kept row counts: (lines a sheet holds) per [`rows_per_sheet`].
pub fn io_pack(plc: &Plc) -> Vec<Vec<Seg>> {
    let rows: Vec<usize> = iolist::cards_in_order(plc).iter().map(|c| c.rows).collect();
    // A heading line and the header row; one empty row between groups.
    pack(&rows, 2.0, 1.0, 1.0, rows_per_sheet() as f64)
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
    // Brief 055 (FP4): the frame's margins and its block's height.
    let m = ecad_model::sheet::margins_of(doc, page);
    (
        m.left + 5.0,
        m.bottom + ecad_model::sheet::keep_out(doc, page) + 5.0,
        f64::from(spec.width) - m.right - 5.0,
        f64::from(spec.height) - m.top - 5.0,
    )
}

/// The groups of the rack sheet: each rack by tag, then the cards in no rack. (rack or none, tiles)
fn rack_groups(plc: &Plc) -> Vec<(Option<ecad_model::DeviceId>, usize)> {
    let mut out: Vec<(Option<ecad_model::DeviceId>, usize)> =
        plc.racks_by_tag().into_iter().map(|r| (Some(r.id), overview::tile_count(plc, r))).collect();
    let n = plc.unracked_by_tag().len().min(overview::MAX_TILES);
    if n > 0 {
        out.push((None, n));
    }
    out
}

/// The rack sheet's layout: per sheet, the parts of groups in rows of tiles.
pub fn rack_pack(plc: &Plc, width: f64, height: f64) -> (Vec<(Option<ecad_model::DeviceId>, usize)>, Vec<Vec<Seg>>, usize) {
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
pub fn rack_heading(doc: &Document, plc: &Plc, rack: &super::RackData) -> String {
    let mut parts = vec![rack.tag.clone(), rack.ty.code().to_string()];
    if let Some(p) = doc.devices.get(&rack.id).and_then(|x| x.part_number.as_ref()).map(|v| v.as_str()).filter(|v| !v.is_empty()) {
        parts.push(p.to_string());
    }
    parts.push(overview::rack_words(plc, rack));
    parts.join(" \u{b7} ")
}

/// What the `plcrack` page says when the project has no PLC racks or cards.
pub const NO_RACKS: &str = "No PLC racks.";

/// Whether `effects` (just committed; `doc` is after them) move a place the `plcrack` sheet prints
/// (review of #89, 3): a PLC card's or rack's symbol moved, or a page holding one renumbered,
/// resized, or any page moved in the book. Only the effects' own items and the pages they name are
/// read.
pub fn rack_places_touched(doc: &Document, plc: &Plc, effects: &ecad_model::effect::Effects<'_>) -> bool {
    use ecad_model::Subject;
    use ecad_model::effect::Effect;
    let ours = |c: ecad_model::DeviceId| plc.cards.contains_key(&c) || plc.racks.contains_key(&c);
    let page_holds = |p: PageId| doc.representations_on(p).any(|(_, r)| matches!(r.subject, Subject::Device(c) if ours(c)));
    for effect in effects {
        match effect {
            // An item changed in place (a symbol moved).
            Effect::Item { id, before: Some(_), after: Some(_) } => {
                if let Some((_, r)) = doc.representation_of_item(*id)
                    && let Subject::Device(c) = r.subject
                    && ours(c)
                {
                    return true;
                }
            }
            // A page changed in place (`ModifyPage` states no index).
            Effect::Page { id: p, before: Some(before), after: Some(after), index_before: None, index_after: None } => {
                let ch = before.change(after);
                if (ch.number || ch.sheet || ch.folder) && page_holds(*p) {
                    return true;
                }
            }
            Effect::Item { .. }
            | Effect::Page { .. }
            | Effect::Layer { .. }
            | Effect::Device { .. }
            | Effect::DeviceDefinition { .. }
            | Effect::Terminal { .. }
            | Effect::Rep { .. }
            | Effect::Wire { .. }
            | Effect::Route { .. }
            | Effect::Cable { .. }
            | Effect::Prop { .. }
            | Effect::Definition { .. }
            | Effect::Settings { .. }
            | Effect::Version { .. }
            | Effect::Folder { .. }
            | Effect::Style { .. }
            | Effect::Standard { .. }
            | Effect::Project { .. }
            | Effect::DeviceLink { .. } => {}
        }
    }
    // A page moved in the book (rare): every place may change its page number.
    ecad_model::book::moves_pages_by(effects) && !(plc.cards.is_empty() && plc.racks.is_empty())
}

// The release's PLC racks list (design 051 §4.2: in the release like the I/O list): one row per
/// tile, racks by tag and then the cards in no rack. `None` when the project has no PLC racks or
/// cards; the release then names [`NO_RACKS`].
pub fn rack_csv(doc: &Document, plc: &Plc) -> Option<crate::outputs::Table> {
    use crate::outputs::{Cell, Table};
    if plc.racks.is_empty() && plc.cards.is_empty() {
        return None;
    }
    let c = |t: String| Cell { text: t, flag: None };
    let mut rows = Vec::new();
    let mut push = |rack: &str, t: &Tile| {
        let (used, reserved) =
            (t.squares.iter().filter(|s| **s == Square::Used).count(), t.squares.iter().filter(|s| **s == Square::Reserved).count());
        rows.push(vec![
            c(rack.to_string()),
            c(t.slot.map(|s| s.to_string()).unwrap_or_default()),
            c(if t.kind == TileKind::Free { String::new() } else { t.tag.clone() }),
            c(t.what.clone()),
            c(t.range.clone()),
            c(t.squares.len().to_string()),
            c(used.to_string()),
            c(reserved.to_string()),
            c(t.place.clone().unwrap_or_default()),
        ]);
    };
    for r in plc.racks_by_tag() {
        for t in overview::rack_tiles(doc, plc, r) {
            push(&r.tag, &t);
        }
    }
    for t in overview::unracked_tiles(doc, plc) {
        push("", &t);
    }
    Some(Table { headers: vec!["Rack", "Slot", "Tag", "Type", "Addresses", "Channels", "Used", "Reserved", "Place"], rows })
}
