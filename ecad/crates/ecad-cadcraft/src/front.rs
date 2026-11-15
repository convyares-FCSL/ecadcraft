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
/// Board 1.16's sizes, millimetres on A3: block titles, body text, captions, the customer line;
/// a line of text; a block's title band; the gap between blocks; a legend cell and its row.
const BLOCK_TITLE_MM: f64 = 5.0;
const BODY_MM: f64 = 3.5;
const CAPTION_MM: f64 = 2.4;
const CUSTOMER_MM: f64 = 3.8;
const LINE_MM: f64 = 7.0;
const HEAD_MM: f64 = 16.0;
const BLOCK_GAP_MM: f64 = 8.0;
const CELL_W_MM: f64 = 18.0;
const CELL_H_MM: f64 = 13.0;
const CELL_ROW_MM: f64 = 17.0;
/// Most legend columns; the cells spread evenly over the rows they need (8 cells: two rows of 4),
/// so no row is left half empty (owner, review of 054).
const LEGEND_COLUMNS: usize = 5;
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

/// The project properties holding the person's rows of the wire-colour legend and the symbol
/// legend (design 054 §8.1a): one row per line, `<key>\t<shown 1|0>\t<label>`.
pub const WIRE_ROWS_KEY: &str = "cover.wire_colours";
pub const LEGEND_ROWS_KEY: &str = "cover.symbol_legend";
/// Most rows, and most characters of a label.
pub const MAX_ROWS: usize = 64;
pub const MAX_LABEL_CHARS: usize = 64;
/// The refusal for rows past the bounds.
pub const ROWS_TOO_LONG: &str = "A cover table takes up to 64 rows with labels of 64 characters.";
/// Most characters of a row key (a definition ID, a mark word, a colour and cross-section).
const MAX_KEY_CHARS: usize = 64;

/// One row the person set: its key, whether it is shown, and its label (empty: the generated).
#[derive(Debug, Clone, PartialEq)]
pub struct RowOverride {
    pub key: String,
    pub shown: bool,
    pub label: String,
}

/// A generated row as the editor shows it: its key, its generated words, the label drawn, and
/// whether it is shown.
#[derive(Debug, Clone, PartialEq)]
pub struct EditedRow {
    pub key: String,
    pub generated: String,
    pub label: String,
    pub shown: bool,
}

/// Read stored rows, within the bounds: malformed lines and the excess are left out.
pub fn parse_overrides(text: &str) -> Vec<RowOverride> {
    text.lines()
        .filter_map(|l| {
            let mut parts = l.splitn(3, '\t');
            let key = parts.next()?.trim();
            let shown = parts.next().map(str::trim) != Some("0");
            let label: String = parts.next().unwrap_or_default().trim().chars().take(MAX_LABEL_CHARS).filter(|c| !c.is_control()).collect();
            (!key.is_empty() && key.chars().count() <= MAX_KEY_CHARS).then(|| RowOverride { key: key.to_string(), shown, label })
        })
        .take(MAX_ROWS)
        .collect()
}

/// Whether `text` may be written as a block's rows.
///
/// # Errors
/// [`ROWS_TOO_LONG`] past the bounds or for a malformed row.
pub fn check_overrides(text: &str) -> Result<(), String> {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.len() > MAX_ROWS {
        return Err(ROWS_TOO_LONG.into());
    }
    for l in lines {
        let parts: Vec<&str> = l.splitn(3, '\t').collect();
        let ok = match parts.as_slice() {
            [k, s, label] => {
                !k.trim().is_empty()
                    && k.chars().count() <= MAX_KEY_CHARS
                    && matches!(s.trim(), "0" | "1")
                    && label.chars().count() <= MAX_LABEL_CHARS
                    && !label.chars().any(char::is_control)
            }
            _ => false,
        };
        if !ok {
            return Err(ROWS_TOO_LONG.into());
        }
    }
    Ok(())
}

/// The rows the project stores under `key`.
pub fn overrides(doc: &Document, key: &str) -> Vec<RowOverride> {
    doc.properties.get(key).map(|t| parse_overrides(t)).unwrap_or_default()
}

/// The text a block's rows are stored as: every row, its label only when the person changed it.
pub fn overrides_text(rows: &[EditedRow]) -> String {
    rows.iter()
        .take(MAX_ROWS)
        .map(|r| {
            let label =
                if r.label == r.generated { String::new() } else { r.label.chars().take(MAX_LABEL_CHARS).filter(|c| !c.is_control()).collect() };
            format!("{}\t{}\t{label}", r.key, if r.shown { 1 } else { 0 })
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The generated rows `(key, words)` in the order the overrides give (the rows they name first,
/// then the rest in generated order), each with its index in `generated`. A key no longer
/// generated is dropped.
fn arrange(generated: Vec<(String, String)>, ov: &[RowOverride]) -> Vec<(usize, EditedRow)> {
    let mut out = Vec::new();
    let mut used = vec![false; generated.len()];
    for o in ov {
        if let Some(i) = generated.iter().position(|(k, _)| *k == o.key)
            && !used.get(i).copied().unwrap_or(true)
            && let Some((k, g)) = generated.get(i)
        {
            if let Some(u) = used.get_mut(i) {
                *u = true;
            }
            let label = if o.label.is_empty() { g.clone() } else { o.label.clone() };
            out.push((i, EditedRow { key: k.clone(), generated: g.clone(), label, shown: o.shown }));
        }
    }
    for (i, (k, g)) in generated.iter().enumerate() {
        if !used.get(i).copied().unwrap_or(true) {
            out.push((i, EditedRow { key: k.clone(), generated: g.clone(), label: g.clone(), shown: true }));
        }
    }
    out
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
    /// The person's own words for the line (the table editor), shown instead of the generated.
    pub label: Option<String>,
}

impl WireLine {
    /// Its key for the overrides: the colour and cross-section it merges on (`bk/2.5`).
    pub fn key(&self) -> String {
        format!("{}/{}", self.colour.to_ascii_lowercase(), self.cross_section)
    }

    /// `L1, L2, L3 · 2.5 mm² · black`, or the person's label.
    pub fn text(&self) -> String {
        if let Some(l) = &self.label {
            return l.clone();
        }
        self.generated()
    }

    /// The generated words, whatever the label.
    pub fn generated(&self) -> String {
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

/// The wire-colour legend as drawn: [`wire_rows`] shown, in their order, with their labels.
pub fn wire_lines(doc: &Document) -> Vec<WireLine> {
    wire_rows(doc).into_iter().filter(|(_, r)| r.shown).map(|(l, _)| l).collect()
}

/// Every generated wire-colour line with the person's overrides (design 054 §8.1a): their
/// order, whether each is shown, its label.
pub fn wire_rows(doc: &Document) -> Vec<(WireLine, EditedRow)> {
    let lines = generated_wire_lines(doc);
    let rows = arrange(lines.iter().map(|l| (l.key(), l.generated())).collect(), &overrides(doc, WIRE_ROWS_KEY));
    rows.into_iter()
        .filter_map(|(i, r)| {
            let mut l = lines.get(i)?.clone();
            l.label = (r.label != r.generated).then(|| r.label.clone());
            Some((l, r))
        })
        .collect()
}

/// The wire-colour legend as generated (design 054 §6.2): one line per wire category in use, in
/// category table order, categories with equal colour and cross-section sharing a line.
fn generated_wire_lines(doc: &Document) -> Vec<WireLine> {
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
            None => out.push(WireLine { names: vec![row.name], cross_section, colour, label: None }),
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

impl Cell {
    /// Its key for the overrides: the definition's ID, or the mark's word.
    pub fn key(&self) -> String {
        match self {
            Cell::Symbol(id, _) => format!("{:032x}", id.0),
            Cell::Cable => "cable".into(),
            Cell::Continuation => "continuation".into(),
        }
    }
}

/// The symbol legend's cells as drawn: [`legend_rows`] shown, in their order.
pub fn legend_cells(doc: &Document) -> Vec<Cell> {
    legend_rows(doc).into_iter().filter(|(_, r)| r.shown).map(|(c, _)| c).collect()
}

/// Every generated legend cell with the person's overrides (design 054 §8.1a).
pub fn legend_rows(doc: &Document) -> Vec<(Cell, EditedRow)> {
    let cells = generated_cells(doc);
    let rows = arrange(cells.iter().map(|c| (c.key(), c.name().to_string())).collect(), &overrides(doc, LEGEND_ROWS_KEY));
    rows.into_iter().filter_map(|(i, r)| Some((cells.get(i)?.clone(), r))).collect()
}

/// The symbol legend's cells as generated (design 054 §6.3): each definition with a placed copy
/// (through the definition index), in definition order, then the marks in use.
fn generated_cells(doc: &Document) -> Vec<Cell> {
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

/// The `=` functions and `+` locations in use, as the project's list shows them (design 054
/// §6.6, design 052 FP8): each stored entry key as its code and description (`=F1 Pumps`), `?` for
/// a key with no entry, naturally sorted, at most 200 of each.
pub fn structure_codes(doc: &Document) -> (Vec<String>, Vec<String>) {
    use crate::designations::{Aspect, Uses, code_of, description_property};
    let uses = Uses::of(doc);
    let list = |a: Aspect| {
        let shown: BTreeSet<String> = uses
            .keys(a)
            .take(200)
            .map(|k| match (code_of(doc, a, k), doc.properties.get(&description_property(a, k)).filter(|d| !d.trim().is_empty())) {
                (Some(code), Some(d)) => format!("{code} {d}"),
                (Some(code), None) => code,
                (None, _) => "?".to_string(),
            })
            .collect();
        let mut v: Vec<String> = shown.into_iter().collect();
        v.sort_by(|a, b| crate::locations::natural_cmp(a, b));
        v
    };
    (list(Aspect::Function), list(Aspect::Location))
}

/// Whether committed `ops` (now in `doc`) change the cover through anything but the wire
/// categories in use ([`Uses`] keeps those): the heading, customer, site and notes, the revisions,
/// a block toggle; a definition's first copy placed or last removed (counting the copies the
/// transaction adds, so several new copies at once are seen), a definition in use redrawn or
/// renamed; the first cable or the last; a continuation; a stored function or location, or an
/// entry of the project's list (052), while the structure codes block is on (design 054 §10.2).
pub fn touches(doc: &Document, ops: &[Op]) -> bool {
    use std::collections::BTreeMap;
    // Copies of each definition the transaction adds and removes.
    let mut added: BTreeMap<DefinitionId, usize> = BTreeMap::new();
    let mut removed: BTreeSet<DefinitionId> = BTreeSet::new();
    for op in ops {
        match op {
            Op::AddItem { item, .. } => {
                if let ItemKind::Symbol(s) = &item.kind {
                    *added.entry(s.definition).or_default() += 1;
                }
            }
            Op::RemoveItem { item, .. } => {
                if let ItemKind::Symbol(s) = &item.kind {
                    removed.insert(s.definition);
                }
            }
            Op::ModifyItem { before, after, .. } => {
                if let (ItemKind::Symbol(a), ItemKind::Symbol(b)) = (&before.kind, &after.kind)
                    && a.definition != b.definition
                {
                    removed.insert(a.definition);
                    *added.entry(b.definition).or_default() += 1;
                }
            }
            _ => {}
        }
    }
    // A definition is new to the legend when every copy it has came in this transaction, and
    // gone when it has none left.
    if added.iter().any(|(d, n)| doc.items_using(*d).len() <= *n) || removed.iter().any(|d| doc.items_using(*d).is_empty()) {
        return true;
    }
    let codes_on = Block::StructureCodes.on(doc);
    ops.iter().any(|op| match op {
        Op::SetProperty { object: ObjectRef::Project | ObjectRef::Standard, key, .. } => {
            matches!(key.as_str(), crate::frame::TITLE | crate::frame::CUSTOMER | "site" | NOTES_KEY | WIRE_ROWS_KEY | LEGEND_ROWS_KEY)
                || key.starts_with("front.cover.")
                || key.starts_with(crate::revisions::PREFIX)
                || key == crate::revisions::COLUMNS_KEY
                || key == crate::revisions::PENDING_KEY
                || (codes_on && (crate::designations::Aspect::of_property(key).is_some() || crate::designations::is_list_property(key)))
        }
        Op::NameVersion { .. } | Op::UnnameVersion { .. } => true,
        // A text style's size changes the cover's texts; a wire category's name, colour or
        // cross-section its legend line.
        Op::SetStyle { catalogue, .. } => catalogue == "text" || catalogue == "wire",
        // A definition in use redrawn or renamed (048's Edit symbol), or a device re-pointed.
        Op::EmbedDefinition { id, .. } | Op::RemoveDefinition { id, .. } => !doc.items_using(*id).is_empty(),
        Op::SetComponentDefinition { .. } => true,
        Op::CreateCable { .. } => doc.cables.len() == 1,
        Op::DeleteCable { .. } => doc.cables.is_empty(),
        Op::SetProperty { object: ObjectRef::Page(_) | ObjectRef::Component(_), key, .. } => {
            codes_on && crate::designations::Aspect::of_property(key).is_some()
        }
        Op::CreateComponent { component, .. } | Op::DeleteComponent { component, .. } => {
            ecad_model::connectivity::continuation_key(&component.properties).is_some()
        }
        _ => false,
    })
}

/// The wire categories in use (design 054 §10.1): each conductor's category, and how many
/// conductors each category has. Built once, on the cover's first show; then [`Uses::update`]
/// re-reads only the conductors of the nets a transaction touched, and says whether a category's
/// count crossed zero (the only change the wire-colour legend shows).
#[derive(Debug, Clone, Default)]
pub struct Uses {
    of: std::collections::BTreeMap<ecad_model::ConductorId, Option<StyleRef>>,
    counts: std::collections::BTreeMap<StyleRef, usize>,
}

impl Uses {
    /// Every conductor's category in `doc`.
    pub fn build(doc: &Document) -> Uses {
        let looks = crate::styles::wire::WireLooks::new(doc);
        let mut u = Uses::default();
        for c in doc.conductors.keys() {
            let k = looks.category(doc, *c).0;
            u.add(*c, k);
        }
        u
    }

    fn add(&mut self, c: ecad_model::ConductorId, k: Option<StyleRef>) {
        if let Some(k) = &k {
            *self.counts.entry(k.clone()).or_default() += 1;
        }
        self.of.insert(c, k);
    }

    fn drop_one(&mut self, c: ecad_model::ConductorId) -> bool {
        let Some(Some(k)) = self.of.remove(&c) else { return false };
        let n = self.counts.entry(k.clone()).or_default();
        *n = n.saturating_sub(1);
        if *n == 0 {
            self.counts.remove(&k);
            return true;
        }
        false
    }

    /// The categories in use.
    pub fn in_use(&self) -> impl Iterator<Item = &StyleRef> {
        self.counts.keys()
    }

    /// Follow committed `ops` (now in `doc`). Returns whether the set of categories in use
    /// changed. A change of a category's look or of the potentials is not this map's (the
    /// cover's `touches` and a full rebuild handle those).
    pub fn update(&mut self, doc: &Document, ops: &[Op]) -> bool {
        let wide = ops.iter().any(|op| match op {
            Op::SetStyle { catalogue, .. } => catalogue == "wire",
            Op::SetProjectSettings { before, after } => before.wiring.potentials != after.wiring.potentials,
            Op::SetProperty { object: ObjectRef::Project | ObjectRef::Standard, key, .. } => key == crate::defaults::styles::WIRE_CATEGORY.key,
            Op::CreateComponent { component, .. } | Op::DeleteComponent { component, .. } => {
                ecad_model::connectivity::continuation_key(&component.properties).is_some()
            }
            _ => false,
        });
        if wide {
            let before: BTreeSet<StyleRef> = self.counts.keys().cloned().collect();
            *self = Uses::build(doc);
            return before != self.counts.keys().cloned().collect::<BTreeSet<_>>();
        }
        // The conductors whose category may have changed: those the ops name, the ones at the
        // terminals an end left or reached, and every conductor of their nets now.
        let mut seeds: BTreeSet<ecad_model::ConductorId> = BTreeSet::new();
        let mut gone: BTreeSet<ecad_model::ConductorId> = BTreeSet::new();
        let at = |t: Option<ecad_model::TerminalId>, s: &mut BTreeSet<ecad_model::ConductorId>| {
            if let Some(t) = t {
                s.extend(doc.ends_at(t).into_iter().map(|(c, _)| c));
            }
        };
        for op in ops {
            match op {
                Op::CreateConductor { id, .. } | Op::SetProperty { object: ObjectRef::Conductor(id), .. } => {
                    seeds.insert(*id);
                }
                Op::DeleteConductor { id, .. } => {
                    gone.insert(*id);
                }
                Op::AttachEnd { conductor, before, after, .. } => {
                    seeds.insert(*conductor);
                    at(*before, &mut seeds);
                    at(*after, &mut seeds);
                }
                Op::SetProperty { object: ObjectRef::Component(c), key, .. }
                    if key == ecad_model::connectivity::POTENTIAL
                        || key == ecad_model::connectivity::NAME
                        || key == ecad_model::connectivity::KIND =>
                {
                    for (t, _) in doc.terminals_of(*c) {
                        at(Some(t), &mut seeds);
                    }
                }
                _ => {}
            }
        }
        if seeds.is_empty() && gone.is_empty() {
            return false;
        }
        let conn = doc.connectivity();
        let mut all: BTreeSet<ecad_model::ConductorId> = BTreeSet::new();
        for c in &seeds {
            match conn.net_of_conductor(*c).and_then(|n| conn.net(n)) {
                Some(net) => all.extend(net.conductors.iter().copied()),
                None => {
                    all.insert(*c);
                }
            }
        }
        let before: BTreeSet<StyleRef> = self.counts.keys().cloned().collect();
        for c in gone.iter().chain(all.iter()) {
            self.drop_one(*c);
        }
        let looks = crate::styles::wire::WireLooks::new(doc);
        for c in all {
            if doc.conductors.contains_key(&c) {
                let k = looks.category(doc, c).0;
                self.add(c, k);
            }
        }
        before != self.counts.keys().cloned().collect::<BTreeSet<_>>()
    }
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
    let y = t - 6.5 * s;
    d.text((l + 4.0 * s, y), look, text, HAlign::Left, None, Some(rr - l - 8.0 * s));
    let rule = y - 4.5 * s;
    d.line((l + 4.0 * s, rule), (rr - 4.0 * s, rule));
    rule - 5.5 * s
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
    let heading_look = sized(TextKind::TableHeading, BLOCK_TITLE_MM);
    let small = sized(TextKind::Table, BODY_MM);
    let caption = sized(TextKind::Table, CAPTION_MM);
    let on = |b: Block| b.on(doc);
    let prop = |k: &str| doc.properties.get(k).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());

    // Heading and customer line, centred.
    let title_text = prop(crate::frame::TITLE).unwrap_or_default();
    let heading = crate::defaults::resolve(doc, &crate::defaults::pages::HEADING, crate::defaults::Scope::project()).text();
    let heading = heading.trim();
    let heading = if heading.eq_ignore_ascii_case(crate::defaults::pages::NO_HEADING) { "" } else { heading };
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
            d.text((mid, y), sized(TextKind::Table, CUSTOMER_MM), &cs.join(" · "), HAlign::Center, Some(LABEL_ACI), Some(r - l));
        }
    }
    y -= 10.0 * s;

    let gap = BLOCK_GAP_MM * s;
    let (bl, br) = (l + 10.0 * s, r - 10.0 * s);
    let half = (br - bl - gap) / 2.0;
    let line_h = LINE_MM * s;
    let head_h = HEAD_MM * s;
    let pad = 5.0 * s;

    // Each block is as tall as what it shows (board 1.16), top to bottom: row 1 (notes, wire
    // colours), the symbol legend, row 3 (revisions, structure codes). The legend gives way when
    // the three do not fit above the title block.
    let (notes_list, _) = notes(doc);
    let wires = if on(Block::WireColours) { wire_lines(doc) } else { Vec::new() };
    let notes_h = if on(Block::Notes) { head_h + line_h * notes_list.len().max(3) as f64 + pad } else { 0.0 };
    let wires_h = if on(Block::WireColours) { head_h + line_h * wires.len().max(3) as f64 + pad } else { 0.0 };
    let row1 = notes_h.max(wires_h);
    let revs: Vec<crate::revisions::Revision> = crate::revisions::issued(doc).into_iter().take(8).collect();
    let codes = if on(Block::StructureCodes) { structure_codes(doc) } else { (Vec::new(), Vec::new()) };
    let rev_h = if on(Block::Revisions) { head_h + 6.0 * s + line_h * revs.len().max(1) as f64 + pad } else { 0.0 };
    let codes_h = if on(Block::StructureCodes) { head_h + line_h * codes.0.len().max(codes.1.len()).clamp(2, 8) as f64 + pad } else { 0.0 };
    let row3 = rev_h.max(codes_h);
    let legend: Vec<(Cell, EditedRow)> =
        if on(Block::SymbolLegend) { legend_rows(doc).into_iter().filter(|(_, r)| r.shown).collect() } else { Vec::new() };
    let cells: Vec<Cell> = legend.iter().map(|(c, _)| c.clone()).collect();
    let (big, small_cell) = ((CELL_W_MM * s, CELL_H_MM * s, CELL_ROW_MM * s), (10.0 * s, 7.0 * s, 10.0 * s));
    let legend_room = (y - b) - row1 - row3 - gap * [row1, row3].iter().filter(|h| **h > 0.0).count() as f64;
    let rows_needed = cells.len().div_ceil(LEGEND_COLUMNS).max(1);
    let columns = cells.len().div_ceil(rows_needed).clamp(3, LEGEND_COLUMNS);
    let legend_h_for = |row_h: f64, rows: usize| head_h + row_h * rows as f64 + pad;
    let (mut cw, mut ch, mut row_h) = big;
    if legend_h_for(row_h, rows_needed) > legend_room {
        (cw, ch, row_h) = small_cell;
    }
    let rows_fit = (((legend_room - head_h - pad) / row_h).floor().max(1.0) as usize).min(1_000);
    let legend_rows = rows_needed.min(rows_fit);
    let legend_h = if on(Block::SymbolLegend) { legend_h_for(row_h, legend_rows) } else { 0.0 };

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
                swatch(&mut d, (x + 4.0 * s, yy), 9.0 * s, 3.0 * s, &w.colour);
                d.text((x + 17.0 * s, yy), small, &w.text(), HAlign::Left, None, Some(half - 21.0 * s));
                yy -= line_h;
            }
            outline(&mut d, rect, DERIVED_ACI, "from wire categories", caption);
        }
        y = bottom - gap;
    }
    let mut fit = LegendFit { shown: 0, all: cells.len() };
    if on(Block::SymbolLegend) {
        let rect = (bl, y - legend_h, br, y);
        let used = cells.iter().filter(|c| matches!(c, Cell::Symbol(..))).count();
        let yy = title(&mut d, rect, "Symbol legend", heading_look, s);
        if cells.is_empty() {
            d.text((bl + 4.0 * s, yy), small, "No symbols yet.", HAlign::Left, Some(LABEL_ACI), Some(br - bl));
        }
        let col_w = (br - bl - 8.0 * s) / columns as f64;
        let mut take = cells.len().min(legend_rows * columns);
        if take < cells.len() && take > 0 {
            take -= 1;
        }
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
        fit.shown = take;
        outline(&mut d, rect, DERIVED_ACI, &format!("symbols used in this project · {used}"), caption);
        y -= legend_h + gap;
    }
    if row3 > 0.0 {
        let top = y;
        let bottom = top - row3;
        let third = (br - bl - gap) / 3.0;
        if on(Block::Revisions) {
            let rect = (bl, bottom, bl + 2.0 * third + gap / 2.0, top);
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
            let rect = (bl + 2.0 * third + gap * 1.5, bottom, br, top);
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

/// The cover's rows as a [`crate::checks::CheckSource`] (brief 060): kept on pages with checks
/// off. No commit scope reaches them: the cover's own update calls [`crate::checks::Checks::set_cover`].
pub struct CoverRows;

impl crate::checks::CheckSource for CoverRows {
    fn key(&self) -> crate::checks::Key {
        crate::checks::Key::Cover
    }
    fn touched(&self, _scope: &crate::checks::Scope) -> bool {
        false
    }
    fn kept_on_pages_off(&self) -> bool {
        true
    }
    fn checks(&self, doc: &Document) -> Vec<crate::checks::Check> {
        check_rows(doc)
    }
}

/// The cover's Checks rows (design 054 §9): `cover-legend-cut` when the legend shows fewer cells
/// than are used, `cover-notes-cut` when the stored notes are past the bounds. Worked out on open
/// and again only after a change [`touches`] names (the cover is laid out once for it).
pub fn check_rows(doc: &Document) -> Vec<crate::checks::Check> {
    use crate::checks::{Check, Kind, Object};
    let cover_page = doc.page_order.iter().copied().find(|p| crate::generated::kind_of(doc, *p) == Some(crate::generated::GenKind::Cover));
    let Some(page) = cover_page else { return Vec::new() };
    let mut out = Vec::new();
    if notes(doc).1 {
        out.push(Check { kind: Kind::CoverNotesCut, page: Some(page), at: None, object: Object::None, which: String::new(), fix: None });
    }
    let input = crate::frame::FrameInput::of(doc, page, &ecad_model::book::Book::of(doc));
    let fit = cover(doc, &input, 0.0).1;
    if fit.shown < fit.all {
        let which = format!("The cover's symbol legend shows {} of {} symbols.", fit.shown, fit.all);
        out.push(Check { kind: Kind::CoverLegendCut, page: Some(page), at: None, object: Object::None, which, fix: None });
    }
    out
}
