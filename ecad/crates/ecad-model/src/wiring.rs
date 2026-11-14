//! Wire numbers, potentials and conductor label texts (brief 022, design 022 §5-§7).
//!
//! The net carries the wire number; it is stored on every conductor of the net as the conductor
//! property [`WIRE_NUMBER`] (owner decision FP2: wire numbers are stable labels people mark on real
//! wires, so they are user data, not derived state). The **numbering pass** ([`assign`]) runs
//! inside every ECAD commit, after the command's own ops, on the nets the transaction touched only,
//! and returns `SetProperty` ops for the same transaction. It never runs on undo, redo, replay or
//! open.
//!
//! # Rules (design 022 §5.2), per touched net, in order of its lowest conductor ID
//! | Situation | Result |
//! | --- | --- |
//! | New net (no conductor carries a number) | the next free number of its scope |
//! | Conductor added to a numbered net | it gets the net's number |
//! | Merge, both automatic | the number on more conductors wins; a tie goes to the lower sequence |
//! | Merge, one fixed | the fixed number wins, written fixed on all |
//! | Merge, two different fixed | nothing is rewritten (a Checks error) |
//! | Split, automatic | the part with more conductors keeps it (tie: the lowest conductor ID); the other gets the next free |
//! | Split, fixed | both keep it (a Checks error) |
//! | Its potential changed and the automatic number has another prefix | the next free of the new potential |
//! | Settings changed | nothing until Renumber Wires |
//!
//! This module reads the document and returns ops; it changes nothing itself.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::connectivity::{self, CONTINUATION, KIND, NAME, NetData, NetId, Node, POTENTIAL, WIRE_NUMBER, WIRE_NUMBER_FIXED, name_key, split_number};
use crate::doc::{Document, Subject};
use crate::id::{ConductorId, PageId};
use crate::op::{ObjectRef, Op};

pub use crate::connectivity::{POTENTIAL as POTENTIAL_KEY, WIRE_NUMBER as WIRE_NUMBER_KEY};

/// Conductor property: the cross-section shown on its label (`0.5`, `18 AWG`).
pub const CROSS_SECTION: &str = "cross_section";
/// Conductor property: the colour shown on its label (`BU`, IEC 60757; free text allowed).
pub const COLOUR: &str = "colour";
/// Longest wire number, cross-section or colour, in characters.
pub const MAX_VALUE_CHARS: usize = 24;

/// How automatic wire numbers are made (design 022 §5.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Scheme {
    /// `{p}{n}`, sequence per prefix, project-wide (`24V-07`).
    #[default]
    #[serde(rename = "potential")]
    Potential,
    /// `{p}{page}.{n}`, sequence per prefix and page (`24V-105.03`).
    #[serde(rename = "page")]
    Page,
    /// `{p}{page}{col}.{k}`, sequence per prefix, page and column (`105C.1`).
    #[serde(rename = "page-column")]
    PageColumn,
}

impl Scheme {
    /// The written name (`potential`, `page`, `page-column`).
    pub fn name(self) -> &'static str {
        match self {
            Scheme::Potential => "potential",
            Scheme::Page => "page",
            Scheme::PageColumn => "page-column",
        }
    }

    /// The scheme a name gives, case-insensitively.
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "potential" => Some(Scheme::Potential),
            "page" => Some(Scheme::Page),
            "page-column" | "pagecolumn" | "page_column" => Some(Scheme::PageColumn),
            _ => None,
        }
    }
}

/// One row of the potentials table (design 022 §6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PotentialDef {
    pub name: String,
    pub prefix: String,
    /// `false`: every net of this potential is numbered with the prefix alone, shared and exempt
    /// from the duplicate check (`PE`).
    pub numbered: bool,
    /// Brief 050b (FP6): a colour an older build stored on the row. Never read for drawing or
    /// labels (wire categories give colours now); kept so Checks can say it is ignored, and
    /// cleared when the table is written again.
    ///
    /// Stored under its pre-050b name `colour`: a version 2 project chunk serialises this struct,
    /// so an older chunk's value is read here and an unchanged table keeps its bytes.
    #[serde(rename = "colour", default, skip_serializing_if = "Option::is_none")]
    pub legacy_colour: Option<String>,
    /// Brief 050b (FP6): as [`PotentialDef::legacy_colour`], for a cross-section (stored as
    /// `cross_section`).
    #[serde(rename = "cross_section", default, skip_serializing_if = "Option::is_none")]
    pub legacy_cross_section: Option<String>,
}

/// The conductor property naming its wire category (brief 050b, FP5).
pub const WIRE_CATEGORY: &str = "wire.category";

/// How colours are written on labels (owner, 2026-11-09).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ColourStyle {
    /// IEC 60757 two-letter codes: `BN`, `BU`, `GNYE`.
    #[default]
    #[serde(rename = "iec")]
    Iec,
    /// Three-letter codes: `BRN`, `BLU`, `GRN/YEL`.
    #[serde(rename = "three-letter")]
    ThreeLetter,
}

impl ColourStyle {
    pub fn name(self) -> &'static str {
        match self {
            ColourStyle::Iec => "iec",
            ColourStyle::ThreeLetter => "three-letter",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "iec" | "iec60757" | "iec 60757" | "two-letter" => Some(ColourStyle::Iec),
            "three-letter" | "three" | "3" | "three_letter" => Some(ColourStyle::ThreeLetter),
            _ => None,
        }
    }
}

/// The IEC 60757 colour codes and their three-letter forms.
pub const COLOUR_CODES: [(&str, &str); 15] = [
    ("BK", "BLK"),
    ("BN", "BRN"),
    ("RD", "RED"),
    ("OG", "ORG"),
    ("YE", "YEL"),
    ("GN", "GRN"),
    ("BU", "BLU"),
    ("VT", "VIO"),
    ("GY", "GRY"),
    ("WH", "WHT"),
    ("PK", "PNK"),
    ("GD", "GLD"),
    ("TQ", "TQS"),
    ("SR", "SLR"),
    ("GNYE", "GRN/YEL"),
];

/// A colour as labels write it in `style`: a known code (either form, any case) in that style;
/// anything else (free text) as typed.
pub fn colour_text(style: ColourStyle, value: &str) -> String {
    let v = value.trim();
    let found = COLOUR_CODES.iter().find(|(iec, three)| v.eq_ignore_ascii_case(iec) || v.eq_ignore_ascii_case(three));
    match (found, style) {
        (Some((iec, _)), ColourStyle::Iec) => (*iec).to_string(),
        (Some((_, three)), ColourStyle::ThreeLetter) => (*three).to_string(),
        (None, _) => v.to_string(),
    }
}

/// Project wiring settings (FP1): `ProjectSettings.wiring`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WiringSettings {
    pub scheme: Scheme,
    /// Digits the sequence is padded to (1-6; the `page-column` scheme does not pad).
    pub digits: u8,
    /// The prefix of a net with no potential (design 022 Q8: empty, so `07`).
    pub unclassified_prefix: String,
    pub potentials: Vec<PotentialDef>,
    /// How label colours are written (IEC 60757 codes by default).
    pub colour_style: ColourStyle,
    /// Text height of conductor labels, millimetres ([`DEFAULT_LABEL_HEIGHT`]; read through
    /// [`WiringSettings::label_height`], which keeps it in range).
    pub label_height: f64,
    /// Brief 032 (FP-W1): which wires get a label, [`LABEL_RULE_NET`] (one per net per page) or
    /// [`LABEL_RULE_EACH`] ([`DEFAULT_LABEL_RULE`]; registered as `wiring.label_rule`).
    pub label_rule: String,
    /// Brief 032 (FP-W1): with `each`, the shortest labelled wire (its longest segment),
    /// millimetres, in [`LABEL_MIN_LENGTHS`] ([`DEFAULT_LABEL_MIN_LENGTH`]; registered as
    /// `wiring.label_min_length`).
    pub label_min_length: f64,
}

/// Brief 032: one label per net per page.
pub const LABEL_RULE_NET: &str = "net";
/// Brief 032: a label on every wire long enough.
pub const LABEL_RULE_EACH: &str = "each";
/// Brief 032 (owner Q6): the label rule of every project that does not set one.
pub const DEFAULT_LABEL_RULE: &str = LABEL_RULE_NET;
/// Brief 032: the default shortest labelled wire, millimetres.
pub const DEFAULT_LABEL_MIN_LENGTH: f64 = 0.0;
/// Brief 032: the shortest labelled wire lengths a project may set, millimetres.
pub const LABEL_MIN_LENGTHS: std::ops::RangeInclusive<f64> = 0.0..=100.0;

/// The default conductor label height, millimetres (owner, 2026-11-09: smaller than tags; 1.8
/// until brief 031, FP6: 1.6, the dense-schematic size). Lane T's registry entry
/// `text.wire_label.height` names it as its built-in.
pub const DEFAULT_LABEL_HEIGHT: f64 = 1.6;
/// The label heights a project may set, millimetres.
pub const LABEL_HEIGHTS: std::ops::RangeInclusive<f64> = 0.5..=10.0;

/// The default sequence padding.
pub const DEFAULT_DIGITS: u8 = 2;
/// The most digits a sequence is padded to.
pub const MAX_DIGITS: u8 = 6;

fn row(name: &str, prefix: &str, numbered: bool) -> PotentialDef {
    PotentialDef { name: name.into(), prefix: prefix.into(), numbered, legacy_colour: None, legacy_cross_section: None }
}

/// The default potentials table (IEC 60445 and 60204-1 practice; owner answer Q10). Brief 050b
/// (FP6): their colours and cross-sections are the built-in wire categories' now.
pub fn default_potentials() -> Vec<PotentialDef> {
    vec![
        row("L", "L-", true),
        row("L1", "L1-", true),
        row("L2", "L2-", true),
        row("L3", "L3-", true),
        row("N", "N-", true),
        row("PE", "PE", false),
        row("24V", "24V-", true),
        row("0V", "0V-", true),
    ]
}

impl Default for WiringSettings {
    fn default() -> Self {
        Self {
            scheme: Scheme::default(),
            digits: DEFAULT_DIGITS,
            unclassified_prefix: String::new(),
            potentials: default_potentials(),
            colour_style: ColourStyle::default(),
            label_height: DEFAULT_LABEL_HEIGHT,
            label_rule: DEFAULT_LABEL_RULE.to_string(),
            label_min_length: DEFAULT_LABEL_MIN_LENGTH,
        }
    }
}

impl WiringSettings {
    /// The label height in range ([`LABEL_HEIGHTS`]; the default for a value out of it or not
    /// finite).
    pub fn label_height(&self) -> f64 {
        if LABEL_HEIGHTS.contains(&self.label_height) { self.label_height } else { DEFAULT_LABEL_HEIGHT }
    }

    /// `value` as a label writes it ([`colour_text`] in this project's style).
    pub fn colour(&self, value: &str) -> String {
        colour_text(self.colour_style, value)
    }

    /// The table row of potential `name`, case-insensitively.
    pub fn potential(&self, name: &str) -> Option<&PotentialDef> {
        let n = name.trim();
        self.potentials.iter().find(|p| p.name.eq_ignore_ascii_case(n))
    }

    /// Digits clamped to 1..=[`MAX_DIGITS`].
    pub fn digits(&self) -> usize {
        usize::from(self.digits.clamp(1, MAX_DIGITS))
    }
}

/// A net's potential and where it came from (design 022 §6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetPotential {
    /// The potential's name, `None` when the net has none.
    pub name: Option<String>,
    /// The name came from a continuation, not from the conductors.
    pub from_continuation: bool,
    /// The conductors carry more than one potential (Checks: mixed potentials).
    pub mixed: bool,
    /// The number prefix: the table's, or the unclassified prefix.
    pub prefix: String,
    /// `false` for an unnumbered potential (`PE`).
    pub numbered: bool,
}

/// The potential of `net` (design 022 §6), first match wins: the potential its conductors carry
/// most (ties: the first by name); the name of a continuation in it that is in the table; none.
pub fn net_potential(doc: &Document, net: &NetData) -> NetPotential {
    let w = &doc.settings.wiring;
    let mixed = net.potentials.len() > 1;
    let carried = net.potentials.iter().max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0))).map(|(k, _)| k.clone());
    let (name, from_continuation) = match carried {
        Some(n) => (Some(n), false),
        None => {
            let named = net.names.iter().find_map(|k| w.potential(k).map(|p| p.name.clone()));
            let from = named.is_some();
            (named, from)
        }
    };
    let row = name.as_deref().and_then(|n| w.potential(n));
    let prefix = match (&name, row) {
        (_, Some(r)) => r.prefix.clone(),
        (Some(n), None) => format!("{n}-"),
        (None, None) => w.unclassified_prefix.clone(),
    };
    NetPotential { numbered: row.is_none_or(|r| r.numbered), name, from_continuation, mixed, prefix }
}

/// The number a net shows (design 022 §5.2, §7): its one fixed number; with two different fixed
/// numbers, the number of its lowest-ID conductor; else the number most of its conductors carry
/// (ties: the lower sequence). `None` when no conductor carries one.
pub fn net_number(doc: &Document, net: &NetData) -> Option<String> {
    if net.fixed.len() > 1 {
        let first = net.first_conductor()?;
        return doc.conductors.get(&first).and_then(|c| c.properties.get(WIRE_NUMBER)).cloned();
    }
    if let Some(f) = net.fixed.keys().next() {
        return Some(f.clone());
    }
    best_number(&net.numbers)
}

/// Whether the net's number is fixed by hand.
pub fn net_fixed(net: &NetData) -> bool {
    !net.fixed.is_empty()
}

/// The number carried by most conductors; a tie goes to the lower sequence, then text order.
fn best_number(numbers: &BTreeMap<String, u32>) -> Option<String> {
    numbers.iter().max_by(|a, b| a.1.cmp(b.1).then_with(|| seq_of(b.0).cmp(&seq_of(a.0))).then_with(|| b.0.cmp(a.0))).map(|(k, _)| k.clone())
}

fn seq_of(number: &str) -> u64 {
    split_number(number).map_or(u64::MAX, |(_, n)| n)
}

/// Whether `number` belongs to `prefix` and not to a longer potential prefix of the table.
fn has_prefix(doc: &Document, number: &str, prefix: &str) -> bool {
    if !number.starts_with(prefix) {
        return false;
    }
    !doc.settings.wiring.potentials.iter().any(|p| p.prefix.len() > prefix.len() && !p.prefix.is_empty() && number.starts_with(p.prefix.as_str()))
}

/// Where `at` lies on `page` (brief 019): the page's shown number and its frame's column and row.
pub fn place(doc: &Document, page: PageId, at: crate::geom::Pt) -> Option<crate::derive::Place> {
    let grid = crate::sheet::Grid::of(doc, page);
    crate::derive::place_on(&grid, &crate::book::page_number(doc, page), at)
}

/// Where a net is, for the page schemes: the shown page number and the frame column label of its
/// lowest-ID conductor's route start on its first page in page order (`1`, `A` when it has none).
pub fn net_place(doc: &Document, net: &NetData) -> (String, String) {
    let fallback = || ("1".to_string(), "A".to_string());
    let Some(first) = net.first_conductor() else { return fallback() };
    let order: Option<(usize, PageId)> = doc
        .representations_of(Subject::Conductor(first))
        .filter_map(|(_, r)| doc.page_order.iter().position(|p| *p == r.page).map(|i| (i, r.page)))
        .min();
    let Some((_, page)) = order else { return fallback() };
    match crate::derive::route_ends_on(doc, first, page).and_then(|(a, _)| place(doc, page, a)) {
        Some(p) => (p.page, p.column_label),
        None => (crate::book::page_number(doc, page), "A".to_string()),
    }
}

/// The sequence prefix (scope) of a new number for a net with number prefix `prefix` on page
/// `page` (its shown number), column `col` (its label).
fn scope(doc: &Document, prefix: &str, page: &str, col: &str) -> String {
    match doc.settings.wiring.scheme {
        Scheme::Potential => prefix.to_string(),
        Scheme::Page => format!("{prefix}{page}."),
        Scheme::PageColumn => format!("{prefix}{page}{col}."),
    }
}

fn format_in(doc: &Document, scope: &str, n: u64) -> String {
    format_digits(doc, scope, n, None)
}

/// A number `n` in sequence scope `scope`, padded to `digits` (default: the project's wire
/// digits; the page-column scheme is never padded).
pub(crate) fn format_digits(doc: &Document, scope: &str, n: u64, digits: Option<usize>) -> String {
    match doc.settings.wiring.scheme {
        Scheme::PageColumn => format!("{scope}{n}"),
        _ => format!("{scope}{n:0width$}", width = digits.unwrap_or_else(|| doc.settings.wiring.digits())),
    }
}

/// The sequence scope a new number of `net` (with potential `pot`) belongs to.
pub(crate) fn scope_of(doc: &Document, net: &NetData, pot: &NetPotential) -> String {
    let (page, col) = match doc.settings.wiring.scheme {
        Scheme::Potential => (String::new(), String::new()),
        _ => net_place(doc, net),
    };
    scope(doc, &pot.prefix, &page, &col)
}

/// The number net `id` keeps when numbering only nets without one: its most carried number, if
/// no other net has a better claim to it.
pub(crate) fn kept_number(doc: &Document, id: NetId, net: &NetData) -> Option<String> {
    best_number(&net.numbers).filter(|w| keeps(doc, id, w))
}

/// Next free numbers handed out by one pass, so two new nets never get the same one.
#[derive(Debug, Default)]
pub struct Allocator {
    taken: BTreeMap<String, u64>,
}

impl Allocator {
    /// The next free number for `net` (with potential `pot`): highest used in its scope plus one.
    pub fn next(&mut self, doc: &Document, net: &NetData, pot: &NetPotential) -> String {
        let (page, col) = match doc.settings.wiring.scheme {
            Scheme::Potential => (String::new(), String::new()),
            _ => net_place(doc, net),
        };
        let sc = scope(doc, &pot.prefix, &page, &col);
        let used = doc.connectivity().next_in(&sc);
        let n = self.taken.get(&sc).map_or(used, |t| used.max(t.saturating_add(1)));
        self.taken.insert(sc.clone(), n);
        format_in(doc, &sc, n)
    }
}

/// `SetProperty` ops writing `number` (and the fixed flag) on every conductor of `net` that does
/// not already carry it.
pub fn write_ops(doc: &Document, net: &NetData, number: &str, fixed: bool) -> Vec<Op> {
    let mut ops = Vec::new();
    for c in &net.conductors {
        let Some(cond) = doc.conductors.get(c) else { continue };
        let object = ObjectRef::Conductor(*c);
        let cur = cond.properties.get(WIRE_NUMBER);
        if cur.map(String::as_str) != Some(number) {
            ops.push(Op::SetProperty { object, key: WIRE_NUMBER.into(), before: cur.cloned(), after: Some(number.to_string()) });
        }
        let flag = cond.properties.get(WIRE_NUMBER_FIXED);
        let want = fixed.then(|| "yes".to_string());
        if flag != want.as_ref() {
            ops.push(Op::SetProperty { object, key: WIRE_NUMBER_FIXED.into(), before: flag.cloned(), after: want });
        }
    }
    ops
}

/// Whether every conductor of `net` already carries `number` (fixed or not as `fixed` says), read
/// from the net's counts without visiting its conductors.
fn carries(net: &NetData, number: &str, fixed: bool) -> bool {
    let n = u32::try_from(net.conductors.len()).unwrap_or(u32::MAX);
    let all = net.numbers.len() == 1 && net.numbers.get(number) == Some(&n);
    let flags = if fixed { net.fixed.len() == 1 && net.fixed.get(number) == Some(&n) } else { net.fixed.is_empty() };
    all && flags
}

/// The nets a transaction's ops touched, in the state after them (design 022 §11).
pub fn nets_touched(doc: &Document, ops: &[Op]) -> (BTreeSet<NetId>, BTreeSet<NetId>) {
    let conn = doc.connectivity();
    let mut nets = BTreeSet::new();
    let mut potential = BTreeSet::new();
    let add = |n: Node, nets: &mut BTreeSet<NetId>| {
        if let Some(id) = conn.net_of(&n) {
            nets.insert(id);
        }
    };
    for op in ops {
        match op {
            Op::CreateConductor { id, .. } => add(Node::Conductor(*id), &mut nets),
            Op::DeleteConductor { conductor, .. } => {
                for t in conductor.ends.iter().flatten() {
                    add(Node::Terminal(*t), &mut nets);
                }
            }
            Op::AttachEnd { conductor, before, after, .. } => {
                add(Node::Conductor(*conductor), &mut nets);
                for t in [before, after].into_iter().flatten() {
                    add(Node::Terminal(*t), &mut nets);
                }
            }
            Op::SetProperty { object: ObjectRef::Conductor(c), key, .. } => {
                if key == POTENTIAL {
                    add(Node::Conductor(*c), &mut potential);
                }
                if key == POTENTIAL || key == WIRE_NUMBER || key == WIRE_NUMBER_FIXED {
                    add(Node::Conductor(*c), &mut nets);
                }
            }
            // Brief 048: another definition may bridge other points.
            Op::SetComponentDefinition { id, .. } => {
                for (t, _) in doc.terminals_of(*id) {
                    add(Node::Terminal(t), &mut nets);
                }
            }
            Op::AddTerminal { id, terminal } | Op::RemoveTerminal { id, terminal } => {
                add(Node::Terminal(*id), &mut nets);
                for (t, _) in doc.terminals_of(terminal.component) {
                    add(Node::Terminal(t), &mut nets);
                }
            }
            Op::CreateComponent { id, component } | Op::DeleteComponent { id, component } => {
                let cont = component.properties.get(KIND).is_some_and(|k| k == CONTINUATION);
                for (t, _) in doc.terminals_of(*id) {
                    add(Node::Terminal(t), &mut nets);
                    if cont {
                        add(Node::Terminal(t), &mut potential);
                    }
                }
                if let Some(n) = component.properties.get(NAME) {
                    add(Node::Name(name_key(n)), &mut nets);
                    if cont {
                        add(Node::Name(name_key(n)), &mut potential);
                    }
                }
            }
            Op::SetProperty { object: ObjectRef::Component(c), key, before, after } if key == KIND || key == NAME => {
                for (t, _) in doc.terminals_of(*c) {
                    add(Node::Terminal(t), &mut nets);
                    add(Node::Terminal(t), &mut potential);
                }
                if key == NAME {
                    for n in [before, after].into_iter().flatten() {
                        add(Node::Name(name_key(n)), &mut nets);
                        add(Node::Name(name_key(n)), &mut potential);
                    }
                }
            }
            // Brief 030 (FP7): a link edit touches the nets of its old and new terminals, so the
            // numbering pass merges or splits their numbers in the same transaction.
            Op::SetProperty { object: ObjectRef::Component(c), key, before, after } if crate::links::link_number(key).is_some() => {
                for v in [before, after] {
                    for t in crate::links::terminals_named(doc, *c, v.as_deref()) {
                        add(Node::Terminal(t), &mut nets);
                    }
                }
            }
            // Brief 036 (FP8a): a bridged level symbol joins or parts its position's levels.
            Op::LinkRepresentation { rep, .. } | Op::UnlinkRepresentation { rep, .. } => {
                for t in crate::connectivity::Connectivity::bridged_shown(doc, rep) {
                    add(Node::Terminal(t), &mut nets);
                }
            }
            Op::EmbedDefinition { id, .. } | Op::RemoveDefinition { id, .. } => {
                for comp in doc.components_using(*id) {
                    for (t, _) in doc.terminals_of(comp) {
                        add(Node::Terminal(t), &mut nets);
                    }
                }
            }
            _ => {}
        }
    }
    (nets, potential)
}

/// What [`net_rank`] orders nets carrying one number by: fixed first, then more conductors,
/// then the lower first conductor.
fn net_rank(net: &NetData, number: &str) -> (bool, usize, std::cmp::Reverse<Option<ConductorId>>) {
    (net.fixed.contains_key(number), net.conductors.len(), std::cmp::Reverse(net.first_conductor()))
}

/// Whether `id` keeps `number` among every net carrying it.
fn keeps(doc: &Document, id: NetId, number: &str) -> bool {
    let conn = doc.connectivity();
    let Some(me) = conn.net(id) else { return true };
    let mine = net_rank(me, number);
    conn.nets_numbered(number).filter(|(n, _)| *n != id).all(|(n, _)| conn.net(n).is_none_or(|o| net_rank(o, number) < mine))
}

/// The numbering pass (design 022 §5.1): `SetProperty` ops numbering the nets `ops` touched in
/// `doc` (the state after `ops`). Empty when nothing needs a number.
pub fn assign(doc: &Document, ops: &[Op]) -> Vec<Op> {
    let (touched, potential_changed) = nets_touched(doc, ops);
    assign_nets(doc, &touched, &potential_changed)
}

/// [`assign`] for the nets `touched`.
pub fn assign_nets(doc: &Document, touched: &BTreeSet<NetId>, potential_changed: &BTreeSet<NetId>) -> Vec<Op> {
    let conn = doc.connectivity();
    let mut order: Vec<(ConductorId, NetId)> =
        touched.iter().filter_map(|id| conn.net(*id).and_then(|n| n.first_conductor()).map(|c| (c, *id))).collect();
    order.sort();
    let mut alloc = Allocator::default();
    let mut out = Vec::new();
    for (_, id) in order {
        let Some(net) = conn.net(id) else { continue };
        if let Some((number, fixed)) = decide(doc, id, net, potential_changed.contains(&id), &mut alloc)
            && !carries(net, &number, fixed)
        {
            out.extend(write_ops(doc, net, &number, fixed));
        }
    }
    out
}

/// The number net `id` should carry, and whether fixed; `None` to leave it as it is.
fn decide(doc: &Document, id: NetId, net: &NetData, potential_changed: bool, alloc: &mut Allocator) -> Option<(String, bool)> {
    if net.conductors.is_empty() {
        return None;
    }
    match net.fixed.len() {
        0 => {}
        1 => return net.fixed.keys().next().map(|f| (f.clone(), true)),
        _ => return None,
    }
    let pot = net_potential(doc, net);
    if !pot.numbered {
        return Some((pot.prefix.clone(), false));
    }
    // Brief 026: with provisional tags on, a new net stays unnumbered (`24V-?`) until Finalise.
    let Some(winner) = best_number(&net.numbers) else {
        if crate::numbering::provisional_on(doc) {
            return None;
        }
        return Some((alloc.next(doc, net, &pot), false));
    };
    if potential_changed && !has_prefix(doc, &winner, &pot.prefix) {
        return Some((alloc.next(doc, net, &pot), false));
    }
    // An unnumbered potential's shared number (`PE`) is never kept by a numbered net.
    let shared_exempt = doc.settings.wiring.potentials.iter().any(|p| !p.numbered && p.prefix == winner);
    if shared_exempt || !keeps(doc, id, &winner) {
        return Some((alloc.next(doc, net, &pot), false));
    }
    Some((winner, false))
}

/// Renumber Wires (design 022 §5.2, design 026 §2): every net with no number (`all == false`),
/// or every automatic number (`all == true`), in drawing order (page in book order, column, row,
/// then top to bottom and left to right; [`crate::numbering`]). Fixed numbers never change.
pub fn renumber(doc: &Document, all: bool) -> Vec<Op> {
    let scope = if all { crate::numbering::Scope::All } else { crate::numbering::Scope::Provisional };
    let req = crate::numbering::Request {
        kinds: [crate::numbering::Kind::Wires].into_iter().collect(),
        scope,
        start: None,
        digits: None,
        strips: Default::default(),
        positions: Default::default(),
    };
    crate::numbering::plan(doc, &req, &crate::tag::HighWater::new()).map(|p| p.ops).unwrap_or_default()
}

pub(crate) fn unnumbered(doc: &Document, c: ConductorId) -> bool {
    doc.conductors.get(&c).is_some_and(|x| !x.properties.contains_key(WIRE_NUMBER))
}

/// How many nets with conductors carry no number at all (projects older than 022).
pub fn unnumbered_nets(doc: &Document) -> usize {
    doc.connectivity().nets().filter(|(_, n)| !n.conductors.is_empty() && n.numbers.is_empty()).count()
}

/// A typed wire number, checked (design 022 §5.2): trimmed, 1-24 characters, no spaces or
/// control characters.
///
/// # Errors
/// The rule it breaks, in words.
pub fn valid_number(text: &str) -> Result<String, String> {
    let t = text.trim();
    if t.is_empty() {
        return Err("A wire number needs at least one character.".into());
    }
    if t.chars().count() > MAX_VALUE_CHARS {
        return Err(format!("A wire number is at most {MAX_VALUE_CHARS} characters."));
    }
    if t.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("A wire number has no spaces or control characters.".into());
    }
    Ok(t.to_string())
}

/// A cross-section or colour as typed: trimmed, at most 24 characters, no control characters;
/// empty clears (`None`).
///
/// # Errors
/// Too long, or a control character, in words.
pub fn valid_text(what: &str, text: Option<&str>) -> Result<Option<String>, String> {
    let Some(t) = text.map(str::trim).filter(|t| !t.is_empty()) else { return Ok(None) };
    if t.chars().count() > MAX_VALUE_CHARS {
        return Err(format!("A {what} is at most {MAX_VALUE_CHARS} characters."));
    }
    if t.chars().any(char::is_control) {
        return Err(format!("A {what} has no control characters."));
    }
    Ok(Some(t.to_string()))
}

/// A conductor's label (design 022 §7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    /// `<number> <cross-section> <colour>`, missing parts left out; `?` for no number yet.
    pub text: String,
    /// The net is in a number error (two nets share it, or two fixed numbers): drawn in
    /// `ecad.orphan`.
    pub error: bool,
    /// The net has no number while provisional tags are on (brief 026): `24V-?`, drawn in
    /// `ecad.provisional`.
    pub provisional: bool,
}

/// The label of conductor `c`: its net's number, then its own cross-section and colour. A cable
/// core shows its number only. Brief 050b: the app labels with [`label_with`] and the wire's
/// category (its colour and cross-section when the conductor has none of its own).
pub fn label(doc: &Document, c: ConductorId) -> Option<Label> {
    let own = |k: &str| doc.conductors.get(&c).and_then(|x| x.properties.get(k)).cloned();
    label_with(doc, c, own(CROSS_SECTION), own(COLOUR))
}

/// [`label`] with the cross-section and colour (a colour code, written in the project's colour
/// style) given by the caller, which resolves them (brief 050b: the wire category's).
pub fn label_with(doc: &Document, c: ConductorId, cross_section: Option<String>, colour: Option<String>) -> Option<Label> {
    let cond = doc.conductors.get(&c)?;
    let conn = doc.connectivity();
    let net = conn.net_of_conductor(c).and_then(|id| conn.net(id).map(|n| (id, n)));
    let number = net.and_then(|(_, n)| net_number(doc, n));
    // Brief 026: with provisional tags on, an unnumbered net shows its prefix and `?` (`24V-?`).
    let provisional = number.is_none() && crate::numbering::provisional_on(doc);
    let unnumbered = match net {
        Some((_, n)) if provisional => {
            let pot = net_potential(doc, n);
            if pot.name.is_some() && pot.numbered { format!("{}?", pot.prefix) } else { "?".into() }
        }
        _ => "?".into(),
    };
    let mut words = vec![number.clone().unwrap_or(unnumbered)];
    if doc.cables_of(c).is_empty() {
        let _ = cond;
        words.extend(cross_section.filter(|v| !v.trim().is_empty()));
        words.extend(colour.filter(|v| !v.trim().is_empty()).map(|c| doc.settings.wiring.colour(&c)));
    }
    let error = match (net, number.as_deref()) {
        (Some((id, n)), Some(num)) => number_error(doc, id, n, num),
        _ => false,
    };
    Some(Label { text: words.join(" "), error, provisional })
}

/// Whether net `id` (showing `number`) is in a number error: two different fixed numbers, or
/// another net carrying the same number (unless it is an unnumbered potential's shared prefix).
pub fn number_error(doc: &Document, id: NetId, net: &NetData, number: &str) -> bool {
    if net.fixed.len() > 1 {
        return true;
    }
    let exempt = doc.settings.wiring.potentials.iter().any(|p| !p.numbered && p.prefix == number);
    !exempt && doc.connectivity().nets_numbered(number).any(|(n, _)| n != id)
}

/// The nets that share `number` with another net (for Checks), when it is not exempt.
pub fn sharing(doc: &Document, number: &str) -> Vec<NetId> {
    let exempt = doc.settings.wiring.potentials.iter().any(|p| !p.numbered && p.prefix == number);
    if exempt {
        return Vec::new();
    }
    let nets: Vec<NetId> = doc.connectivity().nets_numbered(number).map(|(n, _)| n).collect();
    if nets.len() > 1 { nets } else { Vec::new() }
}

/// The continuation name keys of `net` that disagree with its potential, or with each other
/// (Checks: "net joins 24V and 0V"): the distinct potentials named, when more than one.
pub fn joined_potentials(doc: &Document, net: &NetData) -> Vec<String> {
    let w = &doc.settings.wiring;
    let mut named: BTreeSet<String> = net.names.iter().filter_map(|k| w.potential(k).map(|p| p.name.clone())).collect();
    for p in net.potentials.keys() {
        if let Some(r) = w.potential(p)
            && !named.is_empty()
        {
            named.insert(r.name.clone());
        }
    }
    if named.len() > 1 { named.into_iter().collect() } else { Vec::new() }
}

/// For tests and debugging: [`connectivity::split_number`] re-exported.
pub fn sequence_of(number: &str) -> Option<(&str, u64)> {
    connectivity::split_number(number)
}
