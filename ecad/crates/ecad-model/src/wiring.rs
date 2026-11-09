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

use crate::connectivity::{
    self, CONTINUATION, KIND, NAME, NetData, NetId, Node, POTENTIAL, WIRE_NUMBER, WIRE_NUMBER_FIXED, name_key,
    split_number,
};
use crate::doc::{Document, Subject};
use crate::id::{ConductorId, PageId};
use crate::op::{ObjectRef, Op};

pub use crate::connectivity::{POTENTIAL as POTENTIAL_KEY, WIRE_NUMBER as WIRE_NUMBER_KEY};

/// Conductor property: the cross-section shown on its label (`0.5`, `18 AWG`).
pub const CROSS_SECTION: &str = "cross_section";
/// Conductor property: the colour shown on its label (`Blue`).
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
    pub colour: String,
    pub cross_section: String,
}

/// Project wiring settings (FP1): `ProjectSettings.wiring`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WiringSettings {
    pub scheme: Scheme,
    /// Digits the sequence is padded to (1-6; the `page-column` scheme does not pad).
    pub digits: u8,
    /// The prefix of a net with no potential (design 022 Q8: empty, so `07`).
    pub unclassified_prefix: String,
    pub potentials: Vec<PotentialDef>,
}

/// The default sequence padding.
pub const DEFAULT_DIGITS: u8 = 2;
/// The most digits a sequence is padded to.
pub const MAX_DIGITS: u8 = 6;

fn row(name: &str, prefix: &str, numbered: bool, colour: &str, cross_section: &str) -> PotentialDef {
    PotentialDef {
        name: name.into(),
        prefix: prefix.into(),
        numbered,
        colour: colour.into(),
        cross_section: cross_section.into(),
    }
}

/// The default potentials table (IEC 60445 and 60204-1 practice; owner answer Q10).
pub fn default_potentials() -> Vec<PotentialDef> {
    vec![
        row("L", "L-", true, "Brown", "2.5"),
        row("L1", "L1-", true, "Brown", "2.5"),
        row("L2", "L2-", true, "Black", "2.5"),
        row("L3", "L3-", true, "Grey", "2.5"),
        row("N", "N-", true, "Blue", "2.5"),
        row("PE", "PE", false, "Green-Yellow", "2.5"),
        row("24V", "24V-", true, "Blue", "0.5"),
        row("0V", "0V-", true, "Blue", "0.5"),
    ]
}

impl Default for WiringSettings {
    fn default() -> Self {
        Self {
            scheme: Scheme::default(),
            digits: DEFAULT_DIGITS,
            unclassified_prefix: String::new(),
            potentials: default_potentials(),
        }
    }
}

impl WiringSettings {
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
    pub colour: Option<String>,
    pub cross_section: Option<String>,
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
    NetPotential {
        numbered: row.is_none_or(|r| r.numbered),
        colour: row.map(|r| r.colour.clone()).filter(|c| !c.is_empty()),
        cross_section: row.map(|r| r.cross_section.clone()).filter(|c| !c.is_empty()),
        name,
        from_continuation,
        mixed,
        prefix,
    }
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
    numbers
        .iter()
        .max_by(|a, b| a.1.cmp(b.1).then_with(|| seq_of(b.0).cmp(&seq_of(a.0))).then_with(|| b.0.cmp(a.0)))
        .map(|(k, _)| k.clone())
}

fn seq_of(number: &str) -> u64 {
    split_number(number).map_or(u64::MAX, |(_, n)| n)
}

/// Whether `number` belongs to `prefix` and not to a longer potential prefix of the table.
fn has_prefix(doc: &Document, number: &str, prefix: &str) -> bool {
    if !number.starts_with(prefix) {
        return false;
    }
    !doc.settings
        .wiring
        .potentials
        .iter()
        .any(|p| p.prefix.len() > prefix.len() && !p.prefix.is_empty() && number.starts_with(p.prefix.as_str()))
}

/// The column letters of 1-based column `col` (`1` → `A`, `27` → `AA`).
pub fn column_letters(col: u32) -> String {
    let mut n = col.max(1);
    let mut out = Vec::new();
    while n > 0 {
        let r = (n - 1) % 26;
        out.push(char::from(b'A' + r as u8));
        n = (n - 1) / 26;
    }
    out.iter().rev().collect()
}

/// Where a net is, for the page schemes: the 1-based page number and column of its lowest-ID
/// conductor's route start on its first page in book order.
pub fn net_place(doc: &Document, net: &NetData) -> (usize, u32) {
    let Some(first) = net.first_conductor() else { return (1, 1) };
    let order: Option<(usize, PageId)> = doc
        .representations_of(Subject::Conductor(first))
        .filter_map(|(_, r)| doc.page_order.iter().position(|p| *p == r.page).map(|i| (i, r.page)))
        .min();
    let Some((i, page)) = order else { return (1, 1) };
    let col = crate::derive::route_ends_on(doc, first, page)
        .and_then(|(a, _)| crate::derive::column_of(a.x, crate::derive::column_width(doc)))
        .unwrap_or(1);
    (i + 1, col)
}

/// The sequence prefix (scope) and the format of a new number for a net with number prefix
/// `prefix` at page `page`, column `col`.
fn scope(doc: &Document, prefix: &str, page: usize, col: u32) -> String {
    match doc.settings.wiring.scheme {
        Scheme::Potential => prefix.to_string(),
        Scheme::Page => format!("{prefix}{page}."),
        Scheme::PageColumn => format!("{prefix}{page}{}.", column_letters(col)),
    }
}

fn format_in(doc: &Document, scope: &str, n: u64) -> String {
    match doc.settings.wiring.scheme {
        Scheme::PageColumn => format!("{scope}{n}"),
        _ => format!("{scope}{n:0width$}", width = doc.settings.wiring.digits()),
    }
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
            Scheme::Potential => (1, 1),
            _ => net_place(doc, net),
        };
        let sc = scope(doc, &pot.prefix, page, col);
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
            ops.push(Op::SetProperty {
                object,
                key: WIRE_NUMBER.into(),
                before: cur.cloned(),
                after: Some(number.to_string()),
            });
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
    conn.nets_numbered(number)
        .filter(|(n, _)| *n != id)
        .all(|(n, _)| conn.net(n).is_none_or(|o| net_rank(o, number) < mine))
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
fn decide(
    doc: &Document,
    id: NetId,
    net: &NetData,
    potential_changed: bool,
    alloc: &mut Allocator,
) -> Option<(String, bool)> {
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
    let Some(winner) = best_number(&net.numbers) else { return Some((alloc.next(doc, net, &pot), false)) };
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

/// Renumber Wires (design 022 §5.2): every net with no number (`all == false`), or every
/// automatic number (`all == true`, in book order: page, then route start top to bottom, left to
/// right). Fixed numbers never change.
pub fn renumber(doc: &Document, all: bool) -> Vec<Op> {
    let conn = doc.connectivity();
    let mut nets: Vec<(NetId, &NetData)> = conn
        .nets()
        .filter(|(_, n)| !n.conductors.is_empty() && n.fixed.is_empty())
        .filter(|(_, n)| all || n.numbers.is_empty() || n.conductors.iter().any(|c| unnumbered(doc, *c)))
        .collect();
    let order: BTreeMap<PageId, usize> = doc.page_order.iter().enumerate().map(|(i, p)| (*p, i)).collect();
    let key = |n: &NetData| {
        let first = n.first_conductor();
        let place = first.and_then(|c| {
            doc.representations_of(Subject::Conductor(c))
                .filter_map(|(_, r)| {
                    let at = crate::derive::route_ends_on(doc, c, r.page).map(|(a, _)| a)?;
                    Some((order.get(&r.page).copied().unwrap_or(usize::MAX), at))
                })
                .min_by(|a, b| a.0.cmp(&b.0))
        });
        let (page, y, x) = place.map_or((usize::MAX, 0.0, 0.0), |(p, at)| (p, -at.y, at.x));
        (page, y, x, first)
    };
    nets.sort_by(|a, b| {
        let (ka, kb) = (key(a.1), key(b.1));
        ka.0.cmp(&kb.0).then(ka.1.total_cmp(&kb.1)).then(ka.2.total_cmp(&kb.2)).then(ka.3.cmp(&kb.3))
    });
    let mut alloc = Allocator::default();
    let mut out = Vec::new();
    if all {
        // Numbers are handed out afresh: count from 1 in every scope, ignoring the automatic
        // numbers being replaced, but never reusing a fixed one.
        let mut fixed_seqs: BTreeMap<String, BTreeSet<u64>> = BTreeMap::new();
        for (_, n) in conn.nets() {
            for f in n.fixed.keys() {
                if let Some((p, s)) = split_number(f) {
                    fixed_seqs.entry(p.to_string()).or_default().insert(s);
                }
            }
        }
        let mut next: BTreeMap<String, u64> = BTreeMap::new();
        for (_, net) in nets {
            let pot = net_potential(doc, net);
            let number = if pot.numbered {
                let (page, col) = match doc.settings.wiring.scheme {
                    Scheme::Potential => (1, 1),
                    _ => net_place(doc, net),
                };
                let sc = scope(doc, &pot.prefix, page, col);
                let slot = next.entry(sc.clone()).or_insert(1);
                while fixed_seqs.get(&sc).is_some_and(|s| s.contains(slot)) {
                    *slot = slot.saturating_add(1);
                }
                let n = *slot;
                *slot = slot.saturating_add(1);
                format_in(doc, &sc, n)
            } else {
                pot.prefix.clone()
            };
            out.extend(write_ops(doc, net, &number, false));
        }
        return out;
    }
    for (id, net) in nets {
        let pot = net_potential(doc, net);
        let number = if !pot.numbered {
            pot.prefix.clone()
        } else if let Some(w) = best_number(&net.numbers).filter(|w| keeps(doc, id, w)) {
            w
        } else {
            alloc.next(doc, net, &pot)
        };
        out.extend(write_ops(doc, net, &number, false));
    }
    out
}

fn unnumbered(doc: &Document, c: ConductorId) -> bool {
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
}

/// The label of conductor `c`: its net's number, then its own cross-section and colour or its
/// potential's defaults. A cable core shows its number only.
pub fn label(doc: &Document, c: ConductorId) -> Option<Label> {
    let cond = doc.conductors.get(&c)?;
    let conn = doc.connectivity();
    let net = conn.net_of_conductor(c).and_then(|id| conn.net(id).map(|n| (id, n)));
    let number = net.and_then(|(_, n)| net_number(doc, n));
    let mut words = vec![number.clone().unwrap_or_else(|| "?".into())];
    if doc.cables_of(c).is_empty() {
        let pot = net.map(|(_, n)| net_potential(doc, n));
        let cs =
            cond.properties.get(CROSS_SECTION).cloned().or_else(|| pot.as_ref().and_then(|p| p.cross_section.clone()));
        let colour = cond.properties.get(COLOUR).cloned().or_else(|| pot.as_ref().and_then(|p| p.colour.clone()));
        words.extend(cs);
        words.extend(colour);
    }
    let error = match (net, number.as_deref()) {
        (Some((id, n)), Some(num)) => number_error(doc, id, n, num),
        _ => false,
    };
    Some(Label { text: words.join(" "), error })
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
