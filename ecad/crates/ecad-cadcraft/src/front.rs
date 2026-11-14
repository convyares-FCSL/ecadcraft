//! The generated cover (brief 054b, design 054 §6; UI rev 4 board 1.16): a heading, the customer
//! and site, typed notes, a wire-colour legend, a symbol legend of only the symbols used, the last
//! revisions and the structure codes, above the page's own title block.
//!
//! # Semantics
//! - Every block but the notes is derived. Notes are the project property [`NOTES_KEY`]: lines,
//!   each a note, numbered when drawn; at most [`MAX_NOTES`] of [`MAX_NOTE_CHARS`] characters (a
//!   hostile file's excess is cut and reported by Checks).
//! - Each block has a registry toggle (`front.cover.*`, standard and project). A block that is off
//!   frees its place: later rows move up and the symbol legend takes the height freed.
//! - Distances are millimetres on A3 and scale with the drawing area's width.
//! - On screen only, each derived block has a dashed outline with its source caption, and the
//!   notes block its own; they are drawn on a layer that does not print.
//!
//! # Update map (design 054 §10.2)
//! [`touches`] says which committed ops change the cover. A symbol placed or removed changes the
//! legend only when its definition's count of placed copies crosses zero (the document's
//! definition index); a cable only when the project's first one comes or its last goes.

use std::collections::BTreeSet;

use cadcraft_engine::doc::{EntityKind, HAlign, Solid};
use cadcraft_engine::geom::Vec3;
use ecad_model::{DefinitionId, Document, ItemKind, ObjectRef, Op, Primitive, Pt, StyleRef};

use crate::convert::kind_for_primitive;
use crate::frame::{FrameInput, LABEL_ACI, Part, TextLook};
use crate::generated::{Draw, LAYER};
use crate::text_style::{Look, TextKind};

/// The project property holding the cover's notes (FP4), and its bounds (design 054 §6.4).
pub use crate::sheet_cmds::{
    COVER_NOTES as NOTES_KEY, COVER_NOTES_TOO_LONG as NOTES_TOO_LONG, MAX_COVER_NOTE_CHARS as MAX_NOTE_CHARS, MAX_COVER_NOTES as MAX_NOTES,
};
/// The A3 drawing area's width the layout is measured on.
const A3_W: f64 = 390.0;
/// The layer the screen-only outlines are drawn on (not printed).
const OVERLAY: &str = crate::convert::MARKER_LAYER;
/// `ecad.front.derived` and `ecad.front.typed` in ACI (blue, orange).
const DERIVED_ACI: u8 = 5;
const TYPED_ACI: u8 = 30;

/// The cover's notes as stored, cut to the bounds: (notes, whether anything was cut).
pub fn notes(doc: &Document) -> (Vec<String>, bool) {
    let Some(text) = doc.properties.get(NOTES_KEY) else { return (Vec::new(), false) };
    let mut cut = false;
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if out.len() == MAX_NOTES {
            cut = true;
            break;
        }
        let n = line.chars().count();
        cut |= n > MAX_NOTE_CHARS;
        out.push(line.chars().take(MAX_NOTE_CHARS).filter(|c| !c.is_control()).collect());
    }
    (out, cut)
}

/// Whether `text` may be written as the cover's notes.
///
/// # Errors
/// [`NOTES_TOO_LONG`] past the bounds.
pub fn check_notes(text: &str) -> Result<(), String> {
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    if lines.len() > MAX_NOTES || lines.iter().any(|l| l.chars().count() > MAX_NOTE_CHARS) {
        return Err(NOTES_TOO_LONG.into());
    }
    Ok(())
}

/// The cover's blocks that can be turned off, in the Page tab's order (design 054 §6.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Block {
    Notes,
    WireColours,
    SymbolLegend,
    Revisions,
    StructureCodes,
    CustomerSite,
}

impl Block {
    pub const ALL: [Block; 6] = [Block::Notes, Block::WireColours, Block::SymbolLegend, Block::Revisions, Block::StructureCodes, Block::CustomerSite];

    /// Its registry entry.
    pub fn entry(self) -> &'static crate::defaults::Entry {
        use crate::defaults::pages as d;
        match self {
            Block::Notes => &d::NOTES,
            Block::WireColours => &d::WIRE_COLOURS,
            Block::SymbolLegend => &d::SYMBOL_LEGEND,
            Block::Revisions => &d::REVISIONS,
            Block::StructureCodes => &d::STRUCTURE_CODES,
            Block::CustomerSite => &d::CUSTOMER_SITE,
        }
    }

    /// Whether it is on in `doc`.
    pub fn on(self, doc: &Document) -> bool {
        crate::defaults::resolve(doc, self.entry(), crate::defaults::Scope::project()).word() != "off"
    }
}

/// The colour words of an IEC 60757 code (design 054 §6.2): `BK` black, `GNYE` green / yellow; a
/// free text as typed.
pub fn colour_words(code: &str) -> String {
    const WORDS: [&str; 14] =
        ["black", "brown", "red", "orange", "yellow", "green", "blue", "violet", "grey", "white", "pink", "gold", "turquoise", "silver"];
    match crate::look::colour_code(code) {
        Some((a, b)) => {
            let w = |i: usize| WORDS.get(i).copied().unwrap_or_default();
            match b {
                Some(b) => format!("{} / {}", w(a), w(b)),
                None => w(a).to_string(),
            }
        }
        None => code.trim().to_string(),
    }
}

/// One line of the wire-colour legend: the categories' names, the cross-section and the colour
/// code (both may be empty).
#[derive(Debug, Clone, PartialEq)]
pub struct WireLine {
    pub names: Vec<String>,
    pub cross_section: String,
    pub colour: String,
}

impl WireLine {
    /// `L1, L2, L3 · 2.5 mm² · black`.
    pub fn text(&self) -> String {
        let mut parts = vec![self.names.join(", ")];
        if !self.cross_section.is_empty() {
            parts.push(format!("{} mm²", self.cross_section));
        }
        if !self.colour.is_empty() {
            parts.push(colour_words(&self.colour));
        }
        parts.join(" · ")
    }
}

/// Most wire-colour lines drawn (a hostile project's excess is left out).
const MAX_WIRE_LINES: usize = 24;

/// The wire-colour legend (design 054 §6.2): one line per wire category in use, in category
/// table order, categories with equal colour and cross-section sharing a line.
pub fn wire_lines(doc: &Document) -> Vec<WireLine> {
    let looks = crate::styles::wire::WireLooks::new(doc);
    let mut used: BTreeSet<StyleRef> = BTreeSet::new();
    for c in doc.conductors.keys() {
        if let (Some(id), _) = looks.category(doc, *c) {
            used.insert(id);
        }
    }
    let mut out: Vec<WireLine> = Vec::new();
    for row in crate::styles::rows(doc, &crate::styles::wire::CATALOGUE) {
        if !used.contains(&row.id) {
            continue;
        }
        let f = |k: &str| crate::styles::field_of(doc, "wire", &row.id, k, None).and_then(|r| crate::styles::wire::filled(&r)).unwrap_or_default();
        let (colour, cross_section) = (f("colour"), f("cross_section"));
        match out.iter_mut().find(|l| l.colour.eq_ignore_ascii_case(&colour) && l.cross_section == cross_section) {
            Some(l) => l.names.push(row.name),
            None => out.push(WireLine { names: vec![row.name], cross_section, colour }),
        }
        if out.len() >= MAX_WIRE_LINES {
            break;
        }
    }
    out
}

/// A cell of the symbol legend: a definition in use, or a drawn mark.
#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    Symbol(DefinitionId, String),
    Cable,
    Continuation,
}

impl Cell {
    pub fn name(&self) -> &str {
        match self {
            Cell::Symbol(_, n) => n,
            Cell::Cable => "Cable",
            Cell::Continuation => "Continuation",
        }
    }
}

/// The symbol legend's cells (design 054 §6.3): each definition with a placed copy (through the
/// definition index), in definition order, then the marks in use.
pub fn legend_cells(doc: &Document) -> Vec<Cell> {
    let mut out: Vec<Cell> = doc
        .definitions
        .iter()
        .filter(|(id, _)| !doc.items_using(**id).is_empty())
        .map(|(id, d)| Cell::Symbol(*id, d.name.chars().take(40).filter(|c| !c.is_control()).collect()))
        .collect();
    if !doc.cables.is_empty() {
        out.push(Cell::Cable);
    }
    if doc.continuation_names().next().is_some() {
        out.push(Cell::Continuation);
    }
    out
}

/// The `=` functions and `+` locations in use, naturally sorted (design 054 §6.6).
pub fn structure_codes(doc: &Document) -> (Vec<String>, Vec<String>) {
    let mut f: BTreeSet<String> = BTreeSet::new();
    let mut l: BTreeSet<String> = BTreeSet::new();
    let mut add = |p: &ecad_model::Properties| {
        if let Some(v) = p.get(ecad_model::book::FUNCTION_KEY).filter(|v| !v.trim().is_empty()) {
            f.insert(v.trim().to_string());
        }
        if let Some(v) = p.get(ecad_model::book::LOCATION_KEY).filter(|v| !v.trim().is_empty()) {
            l.insert(v.trim().to_string());
        }
    };
    add(&doc.properties);
    for p in doc.pages.values() {
        add(&p.properties);
    }
    for c in doc.components.values() {
        add(&c.properties);
    }
    let sort = |s: BTreeSet<String>| {
        let mut v: Vec<String> = s.into_iter().take(200).collect();
        v.sort_by(|a, b| crate::locations::natural_cmp(a, b));
        v
    };
    (sort(f), sort(l))
}

/// Whether committed `ops` (now in `doc`) change the cover (design 054 §10.2): the heading,
/// customer, site and notes properties, the revisions, a block toggle; a wire category's look or
/// its use; a definition's first copy placed or last removed; the first cable or the last; a
/// continuation; a function or location code.
pub fn touches(doc: &Document, ops: &[Op]) -> bool {
    ops.iter().any(|op| match op {
        Op::SetProperty { object: ObjectRef::Project | ObjectRef::Standard, key, .. } => {
            matches!(key.as_str(), crate::frame::TITLE | crate::frame::CUSTOMER | "site" | NOTES_KEY)
                || key.starts_with("front.cover.")
                || key.starts_with(crate::revisions::PREFIX)
                || key == crate::revisions::COLUMNS_KEY
                || key == crate::revisions::PENDING_KEY
                || key == ecad_model::book::FUNCTION_KEY
                || key == ecad_model::book::LOCATION_KEY
                || key == crate::defaults::styles::WIRE_CATEGORY.key
        }
        Op::NameVersion { .. } | Op::UnnameVersion { .. } => true,
        Op::SetStyle { catalogue, .. } => catalogue == "wire" || catalogue == "text",
        Op::SetProjectSettings { before, after } => before.wiring.potentials != after.wiring.potentials,
        Op::AddItem { item, .. } => matches!(&item.kind, ItemKind::Symbol(s) if doc.items_using(s.definition).len() == 1),
        Op::RemoveItem { item, .. } => matches!(&item.kind, ItemKind::Symbol(s) if doc.items_using(s.definition).is_empty()),
        Op::CreateCable { .. } => doc.cables.len() == 1,
        Op::DeleteCable { .. } => doc.cables.is_empty(),
        Op::CreateConductor { .. } | Op::DeleteConductor { .. } => true,
        Op::SetProperty { object: ObjectRef::Conductor(_), key, .. } => key == crate::defaults::styles::WIRE_CATEGORY.key,
        Op::SetProperty { object: ObjectRef::Page(_) | ObjectRef::Component(_), key, .. } => {
            key == ecad_model::book::FUNCTION_KEY || key == ecad_model::book::LOCATION_KEY || key == ecad_model::connectivity::POTENTIAL
        }
        Op::CreateComponent { component, .. } | Op::DeleteComponent { component, .. } => {
            ecad_model::connectivity::continuation_key(&component.properties).is_some()
        }
        _ => false,
    })
}

/// A block's rectangle in paper millimetres: left, bottom, right, top.
type R = (f64, f64, f64, f64);

/// Dashed outline of `r` on the overlay layer, and its caption at the top right.
fn outline(d: &mut Draw, r: R, aci: u8, caption: &str, look: TextLook) {
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
            d.out.push(Part { kind, weight: Some(crate::frame::THIN_WEIGHT), aci: Some(aci), layer: OVERLAY });
        }
    };
    seg((l, t), (rr, t));
    seg((rr, t), (rr, b));
    seg((rr, b), (l, b));
    seg((l, b), (l, t));
    if !caption.is_empty() {
        let before = d.out.len();
        d.text((rr - 2.0, t - 2.0), look, caption, HAlign::Right, Some(aci), Some(rr - l - 4.0));
        for p in d.out.iter_mut().skip(before) {
            p.layer = OVERLAY;
        }
    }
}

/// A block's title and the thin rule under it; returns the y the block's content starts at.
fn title(d: &mut Draw, r: R, text: &str, look: TextLook, s: f64) -> f64 {
    let (l, _, rr, t) = r;
    let y = t - 5.0 * s;
    d.text((l + 4.0 * s, y), look, text, HAlign::Left, None, Some(rr - l - 8.0 * s));
    let rule = y - 3.5 * s;
    d.line((l + 4.0 * s, rule), (rr - 4.0 * s, rule));
    rule - 4.0 * s
}

/// A filled swatch (the insulation colour) with a thin ink outline; a two-colour code as halves.
fn swatch(d: &mut Draw, at: (f64, f64), w: f64, h: f64, code: &str) {
    let rgb = |i: usize| {
        crate::look::CODE_COLOURS.get(i).map(|(_, _, paper, _)| {
            let c = crate::look::rgb(*paper);
            cadcraft_engine::doc::color::nearest_aci(c)
        })
    };
    let fill = |d: &mut Draw, x0: f64, x1: f64, aci: Option<u8>| {
        let (a, b) = (d.p(x0, at.1 - h / 2.0), d.p(x1, at.1 + h / 2.0));
        let v = |x: f64, y: f64| Vec3::new(x, y, 0.0);
        // A SOLID's third and fourth corners are the far side, in that order.
        let kind = EntityKind::Solid(Solid { corners: [v(a.x, a.y), v(b.x, a.y), v(a.x, b.y), v(b.x, b.y)] });
        d.out.push(Part { kind, weight: None, aci, layer: LAYER });
    };
    match crate::look::colour_code(code) {
        Some((a, Some(b))) => {
            fill(d, at.0, at.0 + w / 2.0, rgb(a));
            fill(d, at.0 + w / 2.0, at.0 + w, rgb(b));
        }
        Some((a, None)) => fill(d, at.0, at.0 + w, rgb(a)),
        None => {}
    }
    d.rect((at.0, at.1 - h / 2.0), (at.0 + w, at.1 + h / 2.0), crate::look::WIRE_WEIGHT, None);
}

/// A definition's graphics fitted into the box `cell` (paper millimetres, centred).
fn symbol(d: &mut Draw, doc: &Document, def: DefinitionId, cell: R) {
    let Some(def) = doc.definitions.get(&def) else { return };
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
        d.out.push(Part { kind: kind_for_primitive(&mapped), weight: Some(crate::frame::THIN_WEIGHT), aci: None, layer: LAYER });
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

/// How the symbol legend was laid out: cells shown of all (Checks' `cover-legend-cut`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LegendFit {
    pub shown: usize,
    pub all: usize,
}

/// The cover's content inside the frame `input` (paper millimetres, sheet origin at `x0`), and how
/// the legend fitted.
pub fn cover(doc: &Document, input: &FrameInput, x0: f64) -> (Vec<Part>, LegendFit) {
    let (l, b, r, t) = crate::generated::area(input);
    let k = ecad_model::sheet::unit_scale(input.unit);
    let mut d = Draw { k, x0, out: Vec::new() };
    let s = ((r - l) / A3_W).clamp(0.5, 4.0);
    let looks = Look::new(doc);
    let look = |kind| TextLook::of(&looks, kind, input.unit);
    let sized = |kind, mm: f64| TextLook { mm: mm * s, ..look(kind) };
    let cover_look = look(TextKind::Cover);
    let heading_look = sized(TextKind::TableHeading, 3.5);
    let small = sized(TextKind::Table, 2.5);
    let caption = sized(TextKind::Table, 1.8);
    let on = |b: Block| b.on(doc);
    let prop = |k: &str| doc.properties.get(k).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());

    // Heading and customer line, centred.
    let title_text = prop(crate::frame::TITLE).unwrap_or_default();
    let heading = crate::defaults::resolve(doc, &crate::defaults::pages::HEADING, crate::defaults::Scope::project()).text();
    let heading = heading.trim();
    let line = match (title_text.is_empty(), heading.is_empty()) {
        (false, false) => format!("{title_text} · {heading}"),
        (false, true) => title_text,
        (true, _) => heading.to_string(),
    };
    let mid = (l + r) / 2.0;
    let mut y = t + 5.0 - 22.0 * s;
    d.text((mid, y), cover_look, &line, HAlign::Center, None, Some(r - l));
    y -= cover_look.mm / 2.0 + 6.0 * s;
    if on(Block::CustomerSite) {
        let cs: Vec<String> = [prop(crate::frame::CUSTOMER), prop("site")].into_iter().flatten().collect();
        if !cs.is_empty() {
            d.text((mid, y), sized(TextKind::Table, 3.5), &cs.join(" · "), HAlign::Center, Some(LABEL_ACI), Some(r - l));
        }
    }
    y -= 10.0 * s;

    let gap = 10.0 * s;
    let (bl, br) = (l + 15.0 * s, r - 15.0 * s);
    let half = (br - bl - gap) / 2.0;
    let line_h = 5.0 * s;
    let head_h = 13.0 * s;

    // Row 1: notes (left half), wire colours (right half).
    let (notes_list, _) = notes(doc);
    let wires = if on(Block::WireColours) { wire_lines(doc) } else { Vec::new() };
    let notes_h = if on(Block::Notes) { head_h + line_h * notes_list.len().max(4) as f64 + 4.0 * s } else { 0.0 };
    let wires_h = if on(Block::WireColours) { head_h + line_h * wires.len().max(4) as f64 + 4.0 * s } else { 0.0 };
    let row1 = notes_h.max(wires_h).max(if on(Block::Notes) || on(Block::WireColours) { 50.0 * s } else { 0.0 });
    // Row 3: revisions (left two thirds), structure codes (right third).
    let revs: Vec<crate::revisions::Revision> = crate::revisions::issued(doc).into_iter().take(8).collect();
    let codes = if on(Block::StructureCodes) { structure_codes(doc) } else { (Vec::new(), Vec::new()) };
    let rev_h = if on(Block::Revisions) { head_h + 6.0 * s + line_h * revs.len().max(1) as f64 + 4.0 * s } else { 0.0 };
    let codes_h = if on(Block::StructureCodes) { head_h + line_h * codes.0.len().max(codes.1.len()).clamp(2, 8) as f64 } else { 0.0 };
    let row3 = rev_h.max(codes_h);

    if row1 > 0.0 {
        let top = y;
        let bottom = top - row1;
        let mut x = bl;
        if on(Block::Notes) {
            let rect = (x, bottom, x + half, top);
            let mut yy = title(&mut d, rect, "Notes", heading_look, s);
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
            let mut yy = title(&mut d, rect, "Wire colours", heading_look, s);
            if wires.is_empty() {
                d.text((x + 4.0 * s, yy), small, "No wires yet.", HAlign::Left, Some(LABEL_ACI), Some(half - 8.0 * s));
            }
            for w in &wires {
                if yy < bottom + 2.0 * s {
                    break;
                }
                swatch(&mut d, (x + 4.0 * s, yy), 8.0 * s, 2.5 * s, &w.colour);
                d.text((x + 15.0 * s, yy), small, &w.text(), HAlign::Left, None, Some(half - 19.0 * s));
                yy -= line_h;
            }
            outline(&mut d, rect, DERIVED_ACI, "from wire categories", caption);
        }
        y = bottom - gap;
    }
    // Row 2: the symbol legend takes the height left above row 3.
    let floor = b + if row3 > 0.0 { row3 + gap } else { 0.0 };
    let mut fit = LegendFit::default();
    if on(Block::SymbolLegend) && y - floor > head_h + 8.0 * s {
        let rect = (bl, floor, br, y);
        let cells = legend_cells(doc);
        fit.all = cells.len();
        let used = cells.iter().filter(|c| matches!(c, Cell::Symbol(..))).count();
        let yy = title(&mut d, rect, "Symbol legend", heading_look, s);
        if cells.is_empty() {
            d.text((bl + 4.0 * s, yy), small, "No symbols yet.", HAlign::Left, Some(LABEL_ACI), Some(br - bl));
        }
        let col_w = (br - bl - 8.0 * s) / 5.0;
        let room = yy - floor - 2.0 * s;
        // 14 × 10 mm cells, shrinking once to 10 × 7 mm, then `and N more`.
        let (mut cw, mut ch, mut row_h) = (14.0 * s, 10.0 * s, 14.0 * s);
        let rows_needed = cells.len().div_ceil(5);
        if rows_needed as f64 * row_h > room {
            (cw, ch, row_h) = (10.0 * s, 7.0 * s, 10.0 * s);
        }
        let rows_fit = ((room / row_h).floor().max(0.0) as usize).min(1_000);
        let mut take = cells.len().min(rows_fit * 5);
        if take < cells.len() && take > 0 {
            take -= 1;
        }
        for (i, c) in cells.iter().take(take).enumerate() {
            let cx = bl + 4.0 * s + col_w * (i % 5) as f64;
            let cy = yy - row_h * (i / 5) as f64 - row_h / 2.0;
            let cell = (cx, cy - ch / 2.0, cx + cw, cy + ch / 2.0);
            match c {
                Cell::Symbol(id, _) => symbol(&mut d, doc, *id, cell),
                m => mark(&mut d, m, cell),
            }
            d.text((cx + cw + 3.0 * s, cy), small, c.name(), HAlign::Left, None, Some(col_w - cw - 5.0 * s));
        }
        if take < cells.len() {
            let i = take;
            let cx = bl + 4.0 * s + col_w * (i % 5) as f64;
            let cy = yy - row_h * (i / 5) as f64 - row_h / 2.0;
            d.text((cx, cy), small, &format!("and {} more", cells.len() - take), HAlign::Left, Some(LABEL_ACI), Some(col_w));
        }
        fit.shown = take;
        outline(&mut d, rect, DERIVED_ACI, &format!("symbols used in this project · {used}"), caption);
    }
    // Row 3.
    if row3 > 0.0 {
        let top = b + row3;
        let third = (br - bl - gap) / 3.0;
        if on(Block::Revisions) {
            let rect = (bl, b, bl + 2.0 * third + gap / 2.0, top);
            let mut yy = title(&mut d, rect, "Revisions", heading_look, s);
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
            let rect = (bl + 2.0 * third + gap * 1.5, b, br, top);
            let yy = title(&mut d, rect, "Structure codes", heading_look, s);
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
