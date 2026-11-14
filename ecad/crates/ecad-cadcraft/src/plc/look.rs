//! A PLC card on the page (brief 051, design 051 §2): its **runs** (cells and the card body of one
//! card, abutting on one top line, in one rotation) and each run's derived **frame**: its sides
//! (slanted **break edges** where the card continues), its bottom edge, the **name block** (tag,
//! caption, maker, part number) and the **type text**. Nothing here is stored.
//!
//! # Semantics
//! - Runs are found per page from the page's representations of PLC cells and card bodies, at
//!   rotation 0° (points on the top edge, the frame below) or 180° (points on the bottom edge, the
//!   frame above, design 051 §2.7). Other rotations get no frame.
//! - Band depths: header and mark bands are geometry ([`crate::symbols::HEADER_BAND`],
//!   [`crate::symbols::MARK_BAND`]); the name band is at least [`crate::symbols::NAME_BAND`] and
//!   grows with the name block's text styles, rounded up to 2.5 mm.
//! - The parts are 025's cross-reference parts ([`crate::xref::Part`]) of the run's first
//!   representation, indexed from [`INDEX_BASE`]: derived, left out of an edit with other objects,
//!   an edit of one alone refused.
//!
//! Excludes: the cells' own graphics ([`crate::symbols`]) and texts ([`super::channel_texts`]).

use std::collections::BTreeMap;

use cadcraft_engine::doc::{HAlign, VAlign};
use ecad_model::{ComponentId, Document, PageId, Pt, RepresentationId, Subject};

use super::rack::{self, CardType};
use crate::symbols::{CELL_TOP, HEADER_BAND, MARK_BAND, NAME_BAND};
use crate::text_style::{TextKind, text_style, text_width};
use crate::xref::{Part, Shape};

/// Frame parts are indexed from here among their representation's parts (a cell or body has no
/// cross-reference parts of its own; the base keeps them apart regardless).
pub const INDEX_BASE: u32 = 1 << 20;
/// Most representations of one card on one page that form runs (045's bound is 256 channels).
const MAX_PER_CARD: usize = 512;
/// How far a break edge's bottom end lies out from its top, mm.
pub const SLANT: f64 = 5.0;
/// Gap between two abutting symbols' edges still counted as one run, mm.
const ABUT: f64 = 0.5;
/// Top lines this close are one line, mm (031's `text.run.align`).
const ALIGN: f64 = 1.0;
/// Inset of the name block and type text from the frame's sides and bottom, mm.
const INSET_X: f64 = 2.0;
const INSET_Y: f64 = 1.0;
/// Between two lines of the name block, mm.
const LINE_GAP: f64 = 1.0;
/// The name band is a multiple of this, mm.
const BAND_STEP: f64 = 2.5;

/// One symbol of a run.
#[derive(Debug, Clone, Copy)]
struct Piece {
    rep: RepresentationId,
    left: f64,
    right: f64,
    /// The line of its points, page y.
    top: f64,
    /// Its channel index (a cell), or `None` (the body).
    channel: Option<u32>,
}

/// One run: a card's pieces in order along the top line, the frame growing in `dir` (−1 down, +1
/// up).
#[derive(Debug, Clone)]
pub struct Run {
    pub card: ComponentId,
    pub dir: f64,
    pub top: f64,
    pub left: f64,
    pub right: f64,
    pub first_rep: RepresentationId,
    pub first: Option<u32>,
    pub last: Option<u32>,
    /// Its leftmost and rightmost pieces are cells (not the body).
    pub left_cell: bool,
    pub right_cell: bool,
}

/// Whether the frame of card `c` is drawn (`plc.frame.show`).
pub fn frame_shown(doc: &Document, c: ComponentId) -> bool {
    let at = crate::defaults::Scope { component: Some(c), ..crate::defaults::Scope::project() };
    crate::defaults::resolve(doc, &crate::defaults::plc::FRAME_SHOW, at).word() == "on"
}

/// The piece of representation `r` of card `c`, when it is a PLC cell or card body at 0° or 180°.
fn piece(doc: &Document, c: ComponentId, rid: RepresentationId, r: &ecad_model::Representation) -> Option<(Piece, f64)> {
    let s = crate::devices::rep_starter(doc, r)?;
    let (_, pl) = crate::devices::rep_definition(doc, r)?;
    if !pl.origin.is_finite() || !pl.rotation.is_finite() {
        return None;
    }
    let deg = pl.rotation.to_degrees().rem_euclid(360.0);
    let up = if deg.abs() < 0.5 || (deg - 360.0).abs() < 0.5 {
        false
    } else if (deg - 180.0).abs() < 0.5 {
        true
    } else {
        return None;
    };
    let k = if pl.scale.is_finite() && pl.scale > 0.0 { pl.scale } else { 1.0 };
    let (x0, x1, local_top, channel) = if let Some((w, _)) = crate::symbols::cell_shape(s.key) {
        let i = crate::devices::shown_terminals(doc, c, r).iter().find_map(|(t, _)| super::channels::point_of(doc, *t)).map(|x| x.0);
        (-w / 2.0, w / 2.0, CELL_TOP, Some(i?))
    } else {
        let ty = CardType::of_body(s.key)?;
        if ty.is_rack() {
            return None;
        }
        let def = s.definition().ok()?;
        // Its points' edge: the top one, else (a communication module) the bottom one.
        let side = if def.terminals.iter().any(|t| t.at.y > 0.0) { 1.0 } else { -1.0 };
        let top: Vec<f64> = def.terminals.iter().filter(|t| t.at.y * side > 0.0).map(|t| t.at.x).collect();
        let lo = top.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = top.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        if !(lo.is_finite() && hi.is_finite()) {
            return None;
        }
        // As the body's graphics: its points' span plus 10, at least 20.
        let pad = ((20.0 - (hi - lo)) / 2.0).max(5.0);
        let y = def.terminals.iter().find(|t| t.at.y * side > 0.0).map_or(25.0 * side, |t| t.at.y);
        (lo - pad, hi + pad, y, None)
    };
    // A body with its points on its bottom edge frames upwards, as a card at 180° does.
    let turned = up;
    let up = if local_top < 0.0 { !up } else { up };
    let top = pl.origin.y + if turned { -local_top } else { local_top } * k;
    // At 180° the local x runs the other way.
    let (left, right) = if turned { (pl.origin.x - x1 * k, pl.origin.x - x0 * k) } else { (pl.origin.x + x0 * k, pl.origin.x + x1 * k) };
    let p = Piece { rep: rid, left, right, top, channel };
    Some((p, if up { 1.0 } else { -1.0 }))
}

/// The runs of every card on `page`.
pub fn runs_on_page(doc: &Document, page: PageId) -> Vec<Run> {
    let mut by_card: BTreeMap<ComponentId, Vec<(Piece, f64)>> = BTreeMap::new();
    for (rid, r) in doc.representations_on(page) {
        let Subject::Component(c) = r.subject else { continue };
        let entry = by_card.entry(c).or_default();
        if entry.len() >= MAX_PER_CARD {
            continue;
        }
        if let Some(p) = piece(doc, c, rid, r) {
            entry.push(p);
        }
    }
    let mut out = Vec::new();
    for (card, mut pieces) in by_card {
        if pieces.is_empty() || !frame_shown(doc, card) {
            continue;
        }
        pieces.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.top.total_cmp(&b.0.top)).then(a.0.left.total_cmp(&b.0.left)));
        // Group by direction and top line, then split where the pieces do not abut.
        let mut groups: Vec<Vec<(Piece, f64)>> = Vec::new();
        for p in pieces {
            match groups.iter_mut().find(|g| g.first().is_some_and(|f| f.1 == p.1 && (f.0.top - p.0.top).abs() <= ALIGN)) {
                Some(g) => g.push(p),
                None => groups.push(vec![p]),
            }
        }
        for mut g in groups {
            g.sort_by(|a, b| a.0.left.total_cmp(&b.0.left));
            let mut cur: Vec<Piece> = Vec::new();
            let dir = g.first().map_or(-1.0, |p| p.1);
            for (p, _) in g {
                if let Some(last) = cur.last()
                    && p.left - last.right > ABUT
                {
                    out.push(make_run(card, dir, &cur));
                    cur.clear();
                }
                cur.push(p);
            }
            if !cur.is_empty() {
                out.push(make_run(card, dir, &cur));
            }
        }
    }
    out
}

fn make_run(card: ComponentId, dir: f64, pieces: &[Piece]) -> Run {
    let first = pieces.iter().filter_map(|p| p.channel).min();
    let last = pieces.iter().filter_map(|p| p.channel).max();
    let left = pieces.iter().map(|p| p.left).fold(f64::INFINITY, f64::min);
    let right = pieces.iter().map(|p| p.right).fold(f64::NEG_INFINITY, f64::max);
    Run {
        card,
        dir,
        top: pieces.first().map_or(0.0, |p| p.top),
        left,
        right,
        first_rep: pieces.first().map_or(RepresentationId(0), |p| p.rep),
        first,
        last,
        left_cell: pieces.first().is_some_and(|p| p.channel.is_some()),
        right_cell: pieces.last().is_some_and(|p| p.channel.is_some()),
    }
}

/// The card's words for its name block, top to bottom, with their text kinds.
fn name_block(doc: &Document, c: ComponentId) -> Vec<(String, TextKind)> {
    let mut out = Vec::new();
    let at = crate::defaults::Scope { component: Some(c), ..crate::defaults::Scope::project() };
    if crate::defaults::resolve(doc, &crate::defaults::text::TAG_SHOW, at).word() != "off" {
        out.push((crate::devices::tag_words(doc, c), TextKind::Tag));
    }
    if let Some((caption, true)) = super::caption(doc, c) {
        out.push((caption, TextKind::PartNumber));
    }
    let prop = |k: &str| doc.components.get(&c).and_then(|x| x.properties.get(k)).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    for k in ["maker", "part_number"] {
        if let Some(v) = prop(k) {
            out.push((v, TextKind::PartNumber));
        }
    }
    out
}

/// The type text: the card's own description, else `DI 16`.
pub fn type_text(doc: &Document, c: ComponentId) -> String {
    let Some(ty) = super::card_type_of(doc, c) else { return String::new() };
    let builtin = crate::symbols::by_key(ty.body()).map(|s| s.name);
    let desc =
        doc.components.get(&c).and_then(|x| x.properties.get("description")).map(|v| v.trim()).filter(|v| !v.is_empty() && Some(*v) != builtin);
    match desc {
        Some(d) => d.to_string(),
        None if ty.has_channels() => format!("{} {}", ty.code(), rack::channel_count(doc, c, ty)),
        None => ty.code().to_string(),
    }
}

/// The name band's depth for a name block of these lines (design 051 §2.5).
pub fn name_band(doc: &Document, lines: &[(String, TextKind)]) -> f64 {
    let h: f64 = lines.iter().map(|(_, k)| text_style(doc, *k).height).sum::<f64>() + LINE_GAP * lines.len().saturating_sub(1) as f64;
    let band = (h + 2.0 * INSET_Y).max(NAME_BAND);
    (band / BAND_STEP).ceil() * BAND_STEP
}

/// Every frame part on `page` (design 051 §2.6).
pub fn parts_on_page(doc: &Document, page: PageId) -> Vec<Part> {
    let mut out = Vec::new();
    for run in runs_on_page(doc, page) {
        frame_parts(doc, &run, &mut out);
    }
    out
}

fn frame_parts(doc: &Document, run: &Run, out: &mut Vec<Part>) {
    let c = run.card;
    let Some(ty) = super::card_type_of(doc, c) else { return };
    let n = if ty.has_channels() { rack::channel_count(doc, c, ty) } else { 0 };
    let has_body = doc
        .representations_of(Subject::Component(c))
        .any(|(_, r)| crate::devices::rep_starter(doc, r).is_some_and(|s| CardType::of_body(s.key).is_some()));
    let lines = name_block(doc, c);
    let ttext = type_text(doc, c);
    let band = name_band(doc, &lines);
    let depth = HEADER_BAND + MARK_BAND + band;
    let bottom = run.top + run.dir * depth;
    // Wide enough for the name block and the type text.
    let block_w = lines.iter().map(|(t, k)| text_width(doc, *k, t)).fold(0.0, f64::max);
    let type_w = text_width(doc, TextKind::PartNumber, &ttext);
    let need = block_w + type_w + 3.0 * INSET_X;
    let (l, mut r) = (run.left, run.right);
    let widened = r - l < need;
    if widened {
        r = l + need;
    }
    let left_break = run.left_cell && run.first.is_some_and(|f| f > 0);
    let right_break = widened || (run.right_cell && (run.last.is_some_and(|x| x.saturating_add(1) < n) || has_body));
    let sl = if left_break { SLANT } else { 0.0 };
    let sr = if right_break { SLANT } else { 0.0 };
    let mut k = 0u32;
    let mut push = |shape: Shape, out: &mut Vec<Part>| {
        out.push(Part { rep: run.first_rep, index: INDEX_BASE + k, shape, aci: None, row: None, bar: false });
        k += 1;
    };
    let p = Pt::new;
    if widened {
        push(Shape::Line(p(run.right, run.top), p(r, run.top)), out);
    }
    push(Shape::Line(p(l, run.top), p(l - sl, bottom)), out);
    push(Shape::Line(p(r, run.top), p(r + sr, bottom)), out);
    push(Shape::Line(p(l - sl, bottom), p(r + sr, bottom)), out);
    // The name block and the type text, upright: at 0° bottom left and bottom right inside; at
    // 180° (the far edge is the top) top left and top right.
    let x = l + INSET_X;
    if run.dir < 0.0 {
        let mut y = bottom + INSET_Y;
        for (t, kind) in lines.iter().rev() {
            let h = text_style(doc, *kind).height;
            push(Shape::Text { at: p(x, y), value: t.clone(), height: h, halign: HAlign::Left, valign: VAlign::Bottom }, out);
            y += h + LINE_GAP;
        }
        let h = text_style(doc, TextKind::PartNumber).height;
        push(Shape::Text { at: p(r - INSET_X, bottom + INSET_Y), value: ttext, height: h, halign: HAlign::Right, valign: VAlign::Bottom }, out);
    } else {
        let mut y = bottom - INSET_Y;
        for (t, kind) in &lines {
            let h = text_style(doc, *kind).height;
            push(Shape::Text { at: p(x, y), value: t.clone(), height: h, halign: HAlign::Left, valign: VAlign::Top }, out);
            y -= h + LINE_GAP;
        }
        let h = text_style(doc, TextKind::PartNumber).height;
        push(Shape::Text { at: p(r - INSET_X, bottom - INSET_Y), value: ttext, height: h, halign: HAlign::Right, valign: VAlign::Top }, out);
    }
}

/// Whether representation `r` of `c` is a PLC cell or card body whose run is framed: its own tag
/// block and caption are then the frame's name block (design 051 §2.6, "tags once per card").
pub fn framed(doc: &Document, c: ComponentId, r: &ecad_model::Representation) -> bool {
    let Some(s) = crate::devices::rep_starter(doc, r) else { return false };
    let plc = crate::symbols::cell_shape(s.key).is_some() || CardType::of_body(s.key).is_some_and(|t| !t.is_rack());
    plc && frame_shown(doc, c)
}
