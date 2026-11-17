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

use ecad_model::{DefinitionId, Document, ItemKind, ObjectRef, Op, StyleRef};

use crate::CmdError;
use crate::frames::FrameInput;
use crate::text_style::TextKind;

/// The project property holding the cover's notes (FP4), and its bounds (design 054 §6.4): at most
/// [`MAX_NOTES`] of [`MAX_NOTE_CHARS`] characters.
pub const NOTES_KEY: &str = "cover.notes";
pub const MAX_NOTES: usize = 40;
pub const MAX_NOTE_CHARS: usize = 400;
pub const NOTES_TOO_LONG: &str = "The cover takes up to 40 notes of 400 characters.";
/// The A3 drawing area's width the layout is measured on.
const A3_W: f64 = 390.0;
/// Board 1.16's sizes, millimetres on A3: a line of text; a block's title band; the gap between
/// blocks; a legend cell and its row.
const LINE_MM: f64 = 7.0;
const HEAD_MM: f64 = 16.0;
const BLOCK_GAP_MM: f64 = 8.0;
const CELL_W_MM: f64 = 18.0;
const CELL_H_MM: f64 = 13.0;
const CELL_ROW_MM: f64 = 17.0;
/// Most legend columns; the cells spread evenly over the rows they need (8 cells: two rows of 4),
/// so no row is left half empty (owner, review of 054).
const LEGEND_COLUMNS: usize = 5;

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
pub fn check_notes(text: &str) -> Result<(), CmdError> {
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    if lines.len() > MAX_NOTES || lines.iter().any(|l| l.chars().count() > MAX_NOTE_CHARS) {
        return Err(CmdError::refused(NOTES_TOO_LONG));
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
pub fn check_overrides(text: &str) -> Result<(), CmdError> {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.len() > MAX_ROWS {
        return Err(CmdError::refused(ROWS_TOO_LONG));
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
            return Err(CmdError::refused(ROWS_TOO_LONG));
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
        ecad_model::words::flag(crate::defaults::resolve(doc, self.entry(), crate::defaults::Scope::project()).word()) != Some(false)
    }
}

/// The colour words of an IEC 60757 code (design 054 §6.2): `BK` black, `GNYE` green / yellow; a
/// free text as typed.
pub fn colour_words(code: &str) -> String {
    const WORDS: [&str; 14] =
        ["black", "brown", "red", "orange", "yellow", "green", "blue", "violet", "grey", "white", "pink", "gold", "turquoise", "silver"];
    match crate::words::colour_code(code) {
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
    for c in doc.wires.keys() {
        if let (Some(id), _) = looks.category(doc, *c) {
            used.insert(id);
        }
    }
    let mut out: Vec<WireLine> = Vec::new();
    for row in crate::styles::rows(doc, &crate::styles::wire::CATALOGUE) {
        if !used.contains(&row.id) {
            continue;
        }
        let f = |k: &str| {
            crate::styles::field_of(doc, ecad_model::StyleCatalogue::Wire, &row.id, k, None)
                .and_then(|r| crate::styles::wire::filled(&r))
                .unwrap_or_default()
        };
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
    use crate::designations::{Aspect, Uses, code_of, description_of};
    let uses = Uses::of(doc);
    let list = |a: Aspect| {
        let shown: BTreeSet<String> = uses
            .keys(a)
            .take(200)
            .map(|k| match (code_of(doc, a, k), description_of(doc, a, k).filter(|d| !d.trim().is_empty())) {
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
            matches!(key.as_str(), crate::frames::TITLE | crate::frames::CUSTOMER | "site" | NOTES_KEY | WIRE_ROWS_KEY | LEGEND_ROWS_KEY)
                || key.starts_with("front.cover.")
                || key.starts_with(crate::revisions::PREFIX)
                || key == crate::revisions::COLUMNS_KEY
                || key == crate::revisions::PENDING_KEY
                || (codes_on && crate::designations::Aspect::of_property(key).is_some())
        }
        Op::NameVersion { .. } | Op::UnnameVersion { .. } => true,
        // A text style's size changes the cover's texts; a wire category's name, colour or
        // cross-section its legend line.
        Op::SetStyle { catalogue, .. } => *catalogue == ecad_model::StyleCatalogue::Text || *catalogue == ecad_model::StyleCatalogue::Wire,
        // A definition in use redrawn or renamed (048's Edit symbol), or a device re-pointed.
        Op::EmbedDefinition { id, .. } | Op::RemoveDefinition { id, .. } => !doc.items_using(*id).is_empty(),
        Op::SetDeviceDefinition { .. } => true,
        Op::CreateCable { .. } => doc.cables.len() == 1,
        Op::DeleteCable { .. } => doc.cables.is_empty(),
        Op::SetProperty { object: ObjectRef::Page(_) | ObjectRef::Device(_), key, .. } => {
            codes_on && crate::designations::Aspect::of_property(key).is_some()
        }
        Op::ModifyProject { before, after } => {
            codes_on && crate::designations::Aspect::ALL.into_iter().any(|a| crate::designations::shown_changed(before, after, a))
        }
        Op::ModifyPage { before, after, .. } => {
            let ch = before.change(after);
            codes_on && (ch.location || ch.function)
        }
        Op::CreateDevice { device: component, .. } | Op::DeleteDevice { device: component, .. } => {
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
    of: std::collections::BTreeMap<ecad_model::WireId, Option<StyleRef>>,
    counts: std::collections::BTreeMap<StyleRef, usize>,
}

impl Uses {
    /// Every conductor's category in `doc`.
    pub fn build(doc: &Document) -> Uses {
        let looks = crate::styles::wire::WireLooks::new(doc);
        let mut u = Uses::default();
        for c in doc.wires.keys() {
            let k = looks.category(doc, *c).0;
            u.add(*c, k);
        }
        u
    }

    fn add(&mut self, c: ecad_model::WireId, k: Option<StyleRef>) {
        if let Some(k) = &k {
            *self.counts.entry(k.clone()).or_default() += 1;
        }
        self.of.insert(c, k);
    }

    fn drop_one(&mut self, c: ecad_model::WireId) -> bool {
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
            Op::SetStyle { catalogue, .. } => *catalogue == ecad_model::StyleCatalogue::Wire,
            Op::SetProjectSettings { before, after } => before.wiring.potentials != after.wiring.potentials,
            Op::SetProperty { object: ObjectRef::Project | ObjectRef::Standard, key, .. } => key == crate::defaults::styles::WIRE_CATEGORY.key,
            Op::CreateDevice { device: component, .. } | Op::DeleteDevice { device: component, .. } => {
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
        let mut seeds: BTreeSet<ecad_model::WireId> = BTreeSet::new();
        let mut gone: BTreeSet<ecad_model::WireId> = BTreeSet::new();
        let at = |t: Option<ecad_model::TerminalId>, s: &mut BTreeSet<ecad_model::WireId>| {
            if let Some(t) = t {
                s.extend(doc.ends_at(t).into_iter().map(|(c, _)| c));
            }
        };
        for op in ops {
            match op {
                Op::CreateWire { id, .. } | Op::ModifyWire { id, .. } | Op::SetProperty { object: ObjectRef::Wire(id), .. } => {
                    seeds.insert(*id);
                }
                Op::DeleteWire { id, .. } => {
                    gone.insert(*id);
                }
                Op::AttachEnd { conductor, before, after, .. } => {
                    seeds.insert(*conductor);
                    at(*before, &mut seeds);
                    at(*after, &mut seeds);
                }
                Op::SetProperty { object: ObjectRef::Device(c), key, .. }
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
        let mut all: BTreeSet<ecad_model::WireId> = BTreeSet::new();
        for c in &seeds {
            match conn.net_of_conductor(*c).and_then(|n| conn.net(n)) {
                Some(net) => all.extend(net.wires.iter().copied()),
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
            if doc.wires.contains_key(&c) {
                let k = looks.category(doc, c).0;
                self.add(c, k);
            }
        }
        before != self.counts.keys().cloned().collect::<BTreeSet<_>>()
    }
}

/// How the symbol legend was laid out: cells shown of all (Checks' `cover-legend-cut`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LegendFit {
    pub shown: usize,
    pub all: usize,
}

/// Where the cover's blocks go inside a frame, in paper millimetres (board 1.16): what the cover's
/// drawing places and what [`check_rows`] reads of the symbol legend.
#[derive(Debug, Clone)]
pub struct Layout {
    /// The drawing area: left, bottom, right, top.
    pub area: (f64, f64, f64, f64),
    /// The scale against A3.
    pub s: f64,
    /// The heading line's and the customer line's y, and the top of the blocks.
    pub heading_y: f64,
    pub customer_y: f64,
    pub blocks_y: f64,
    pub gap: f64,
    /// The blocks' left and right edges, and a half-width block's width.
    pub bl: f64,
    pub br: f64,
    pub half: f64,
    pub line_h: f64,
    /// Row 1 (notes, wire colours) and its contents.
    pub notes: Vec<String>,
    pub wires: Vec<WireLine>,
    pub row1: f64,
    /// Row 3 (revisions, structure codes) and its contents.
    pub revs: Vec<crate::revisions::Revision>,
    pub codes: (Vec<String>, Vec<String>),
    pub row3: f64,
    /// The symbol legend: its rows shown, their cells, columns, height, a cell's width and height
    /// and its row's height, and how many cells are drawn.
    pub legend: Vec<(Cell, EditedRow)>,
    pub cells: Vec<Cell>,
    pub columns: usize,
    pub legend_h: f64,
    pub cell: (f64, f64, f64),
    pub take: usize,
    pub fit: LegendFit,
}

/// The cover's [`Layout`] inside the frame `input`, its heading `cover_mm` high.
pub fn layout(doc: &Document, input: &FrameInput, cover_mm: f64) -> Layout {
    let (l, b, r, t) = crate::generated::area(input);
    let s = ((r - l) / A3_W).clamp(0.5, 4.0);
    let on = |b: Block| b.on(doc);
    let heading_y = t + 5.0 - 22.0 * s;
    let customer_y = heading_y - (cover_mm / 2.0 + 6.0 * s);
    let y = customer_y - 10.0 * s;

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
    let mut take = cells.len().min(legend_rows * columns);
    if take < cells.len() && take > 0 {
        take -= 1;
    }
    let fit = LegendFit { shown: if on(Block::SymbolLegend) { take } else { 0 }, all: cells.len() };
    Layout {
        area: (l, b, r, t),
        s,
        heading_y,
        customer_y,
        blocks_y: y,
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
    }
}

/// The cover's rows as a [`crate::checks::CheckSource`] (brief 060): kept on pages with checks
/// off. No commit scope reaches them: the cover's own update calls `crate::checks::Checks::set_cover`.
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
    let input = FrameInput::with(doc, page, &ecad_model::book::Book::of(doc), crate::frames::FrameText::sizes(doc));
    let cover_mm = crate::frames::height_mm(crate::text_style::text_style(doc, TextKind::Cover).height, input.unit);
    let fit = layout(doc, &input, cover_mm).fit;
    if fit.shown < fit.all {
        let which = format!("The cover's symbol legend shows {} of {} symbols.", fit.shown, fit.all);
        out.push(Check { kind: Kind::CoverLegendCut, page: Some(page), at: None, object: Object::None, which, fix: None });
    }
    out
}
