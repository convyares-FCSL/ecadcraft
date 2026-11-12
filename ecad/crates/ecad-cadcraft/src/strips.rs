//! Terminal strips and cable cores (brief 028, design 028 §2–§9): the terminal plan per strip
//! and the core count per cable, derived from the drawing and never stored.
//!
//! # Semantics
//! - A **strip** is a device whose definition is a terminal starter (`terminal`, `terminalpe`);
//!   each terminal is one symbol with one connection point (018's model, owner answer Q1). Its
//!   **type** is its symbol's: `terminalpe` is PE, anything else feed-through.
//! - The strip's **offer** is its `terminals` property, `12` or `12+2PE`: the number of positions,
//!   and how many of them are PE (design 028 FP1; read so that acceptance G's `12+2PE` gives a
//!   BOM of 10 feed-through and 2 PE). Positions with no drawn terminal are **spare**.
//! - A conductor whose two ends are on two terminals of one strip is a **bridge** (Q5). Every other
//!   conductor on a terminal is **inside** or **outside** by §5.2's rule: a cable core is outside;
//!   else the far device's location against the strip's; else the route's direction at the
//!   terminal (up or left is inside).
//! - A cable's **core offer** is its `cores` property, else read from its `type` (`7G1.5` → `7G`)
//!   (FP2, Q9). Members are matched to it by label first, then in member order: **used**,
//!   **spare** and **over** (§6.2).
//! - Nothing here is refused because of a check; it is flagged (§8).
//!
//! # Update map (design 028 §11.1)
//! [`Strips`] lives inside [`crate::outputs::Outputs`] and is updated with it from the same
//! [`crate::outputs::Scope`], after the device, wire and cable rows:
//!
//! | In scope (from the ops) | Recomputed |
//! | --- | --- |
//! | a device that is a strip (its create, delete, terminals, symbols, items, properties) | that strip |
//! | a device or tagless component (tag, location, junction place, continuation name) | the strips whose rows end on it ([`Strips::far`]) |
//! | a conductor (create, delete, attach, route, properties) and its net mates (wire numbers) | the strips it was on (kept) and is on now |
//! | a cable (create, delete, members, properties) | the strips its members were on (kept) and are on now |
//! | a page whose number or position changed (`SetProperty` number, book order, the sheet) | the strips with terminals on it (their Place) |
//! | anything else | nothing |
//!
//! A full build happens only with the report models: open, reload, first show.

use std::collections::{BTreeMap, BTreeSet};

use ecad_model::derive::Place;
use ecad_model::{CableId, CableMember, ComponentId, ConductorId, Document, End, PageId, Pt, RepresentationId, Subject, TerminalId};

use crate::outputs::{Outputs, PageCx};
use crate::symbols::Kind;

/// The strip property that offers positions (FP1).
pub const OFFER_KEY: &str = "terminals";
/// The cable property that offers cores (FP2).
pub const CORES_KEY: &str = "cores";
/// The strip properties of its PE terminals' BOM row (FP4).
pub const PE_PART_KEY: &str = "part_number.pe";
pub const PE_MAKER_KEY: &str = "maker.pe";
/// Most positions a strip offers.
pub const MAX_POSITIONS: u32 = 999;
/// Most cores a cable offers.
pub const MAX_CORES: usize = 100;
/// The green-yellow core of an IEC `G` cable.
pub const GNYE: &str = "GNYE";
/// The PE starter's key.
pub const PE_KEY: &str = "terminalpe";

// ---------------------------------------------------------------- strips and terminals

/// Whether `c` is a terminal strip: its definition is a terminal starter.
pub fn is_strip(doc: &Document, c: ComponentId) -> bool {
    doc.components
        .get(&c)
        .and_then(|x| x.definition)
        .and_then(|d| doc.definitions.get(&d))
        .and_then(crate::symbols::of_definition)
        .is_some_and(|s| s.kind == Kind::Terminal)
}

/// Whether representation `r` draws a PE terminal.
pub fn rep_is_pe(doc: &Document, r: &ecad_model::Representation) -> bool {
    crate::xref::starter_of(doc, r).is_some_and(|s| s.key == PE_KEY)
}

/// A strip's drawn terminals: (terminal, its representation, whether PE).
pub fn drawn_terminals(doc: &Document, c: ComponentId) -> Vec<(TerminalId, RepresentationId, bool)> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for (rid, r) in doc.representations_of(Subject::Component(c)) {
        let pe = rep_is_pe(doc, r);
        for (t, _) in crate::devices::shown_terminals(doc, c, r) {
            if seen.insert(t) {
                out.push((t, rid, pe));
            }
        }
    }
    out
}

/// [`drawn_terminals`] with where each sits on its page, worked out from its own representation
/// (O(the strip's symbols), never a search per terminal).
fn drawn_at(doc: &Document, c: ComponentId) -> Vec<(TerminalId, RepresentationId, bool, Option<Pt>)> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for (rid, r) in doc.representations_of(Subject::Component(c)) {
        let pe = rep_is_pe(doc, r);
        let placement = crate::devices::rep_definition(doc, r).map(|(_, pl)| pl);
        for (t, p) in crate::devices::shown_terminals(doc, c, r) {
            let at = match (r.form, placement) {
                (ecad_model::Form::Symbol, Some(pl)) => Some(pl.apply(p)),
                (ecad_model::Form::Symbol, None) => None,
                (ecad_model::Form::Drawn, _) => Some(p),
            };
            if seen.insert(t) {
                out.push((t, rid, pe, at));
            }
        }
    }
    out
}

/// The terminal of a strip a conductor end is on: (strip, terminal).
pub fn strip_of_terminal(doc: &Document, t: TerminalId) -> Option<ComponentId> {
    let c = doc.terminals.get(&t)?.component;
    is_strip(doc, c).then_some(c)
}

/// The strip a conductor bridges, when its two ends are on two terminals of one strip.
pub fn bridge_of(doc: &Document, w: ConductorId) -> Option<ComponentId> {
    let cond = doc.conductors.get(&w)?;
    let (a, b) = (cond.end(End::A)?, cond.end(End::B)?);
    if a == b {
        return None;
    }
    let (sa, sb) = (strip_of_terminal(doc, a)?, strip_of_terminal(doc, b)?);
    (sa == sb).then_some(sa)
}

/// A strip's offer (FP1): positions and how many of them are PE.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Offer {
    pub positions: u32,
    pub pe: u32,
}

impl Offer {
    /// As stored: `12` or `12+2PE`.
    pub fn text(self) -> String {
        if self.pe == 0 { self.positions.to_string() } else { format!("{}+{}PE", self.positions, self.pe) }
    }
}

fn offer_refusal(text: &str) -> String {
    let shown: String = text.chars().take(24).collect();
    let more = if text.chars().count() > 24 { "\u{2026}" } else { "" };
    format!("\"{shown}{more}\" is not a terminal count. Write 12, or 12+2PE.")
}

/// Read an offer: `12` or `12+2PE`, spaces ignored, case-insensitive.
///
/// # Errors
/// Unreadable text, no positions, more than [`MAX_POSITIONS`], or more PE than positions, in
/// words with the way forward.
pub fn parse_offer(text: &str) -> Result<Offer, String> {
    let t: String = text.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_ascii_uppercase();
    if t.len() > 16 {
        return Err(offer_refusal(text));
    }
    let num = |s: &str| -> Option<u32> { (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())).then(|| s.parse::<u32>().ok()).flatten() };
    let (n, pe) = match t.split_once('+') {
        None => (num(&t).ok_or_else(|| offer_refusal(text))?, 0),
        Some((a, b)) => {
            let n = num(a).ok_or_else(|| offer_refusal(text))?;
            let pe = num(b.strip_suffix("PE").ok_or_else(|| offer_refusal(text))?).ok_or_else(|| offer_refusal(text))?;
            (n, pe)
        }
    };
    if n == 0 {
        return Err("A strip offers at least 1 position. Write 12, or 12+2PE; empty clears it.".into());
    }
    if n > MAX_POSITIONS {
        return Err(format!("A strip offers at most {MAX_POSITIONS} positions."));
    }
    if pe > n {
        return Err(format!("{pe} PE is more than the {n} positions offered. Write the positions first: {}+{pe}PE.", pe.max(n)));
    }
    Ok(Offer { positions: n, pe })
}

/// The `terminals` value as stored (canonical `12+2PE`); empty clears it.
///
/// # Errors
/// [`parse_offer`]'s.
pub fn offer_value(value: Option<&str>) -> Result<Option<String>, String> {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        None => Ok(None),
        Some(v) => Ok(Some(parse_offer(v)?.text())),
    }
}

/// A strip's stored offer, if readable.
pub fn offer_of(doc: &Document, c: ComponentId) -> Option<Offer> {
    doc.components.get(&c)?.properties.get(OFFER_KEY).and_then(|v| parse_offer(v).ok())
}

// ---------------------------------------------------------------- cores (§6)

fn cores_refusal(text: &str) -> String {
    let shown: String = text.chars().take(24).collect();
    let more = if text.chars().count() > 24 { "\u{2026}" } else { "" };
    format!("\"{shown}{more}\" is not a core list. Write 7, 7G, 5X, or BN,BK,GY,BU,GNYE.")
}

/// The cores a count names: `7` → 1…7, `7G` → 1…6 and GNYE, `5X` → 1…5.
fn counted(n: usize, letter: Option<char>) -> Vec<String> {
    match letter {
        Some('G') => (1..n).map(|i| i.to_string()).chain(std::iter::once(GNYE.to_string())).collect(),
        _ => (1..=n).map(|i| i.to_string()).collect(),
    }
}

/// Read a core offer (§6.1): `7`, `7G`, `5X`, or a list `BN,BK,GY,BU,GNYE`.
///
/// # Errors
/// Unreadable text, no cores, more than [`MAX_CORES`], an empty or repeated list entry.
pub fn parse_cores(text: &str) -> Result<Vec<String>, String> {
    let t = text.trim();
    if t.chars().count() > MAX_CORES * (crate::look::MAX_CORE_CHARS + 1) {
        return Err(format!("A cable offers at most {MAX_CORES} cores."));
    }
    let upper = t.to_ascii_uppercase();
    let (digits, letter) = match upper.char_indices().last() {
        Some((i, c @ ('G' | 'X'))) => (upper.get(..i).unwrap_or(""), Some(c)),
        _ => (upper.as_str(), None),
    };
    if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
        let n: usize = digits.parse().map_err(|_| format!("A cable offers at most {MAX_CORES} cores."))?;
        if n == 0 {
            return Err("A cable offers at least 1 core. Write 7, 7G or 5X; empty clears it.".into());
        }
        if n > MAX_CORES {
            return Err(format!("A cable offers at most {MAX_CORES} cores."));
        }
        return Ok(counted(n, letter));
    }
    if !t.contains(',') && t.chars().any(|c| c.is_ascii_digit()) {
        return Err(cores_refusal(text));
    }
    let mut out: Vec<String> = Vec::new();
    for part in t.split(',') {
        let label =
            crate::commands::core_label(Some(part)).map_err(|e| format!("{e} {}", cores_refusal(text)))?.ok_or_else(|| cores_refusal(text))?;
        if out.iter().any(|x| x.eq_ignore_ascii_case(&label)) {
            return Err(format!("The core list names {label} twice. Write each core once."));
        }
        out.push(label);
        if out.len() > MAX_CORES {
            return Err(format!("A cable offers at most {MAX_CORES} cores."));
        }
    }
    Ok(out)
}

/// The `cores` value as stored (counts upper-case, lists trimmed); empty clears it.
///
/// # Errors
/// [`parse_cores`]'s.
pub fn cores_value(value: Option<&str>) -> Result<Option<String>, String> {
    let Some(v) = value.map(str::trim).filter(|v| !v.is_empty()) else { return Ok(None) };
    let cores = parse_cores(v)?;
    let upper = v.to_ascii_uppercase();
    if upper.bytes().all(|b| b.is_ascii_digit() || b == b'G' || b == b'X') {
        return Ok(Some(upper));
    }
    Ok(Some(cores.join(",")))
}

/// The core count a cable type names: the first `<n><G|X>` followed by a digit (`YSLY-JZ 7G1.5`
/// → `7G`).
pub fn offer_from_type(kind: &str) -> Option<String> {
    let b = kind.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b.get(i).is_some_and(u8::is_ascii_digit) && (i == 0 || !b.get(i - 1).is_some_and(u8::is_ascii_digit)) {
            let start = i;
            while b.get(i).is_some_and(u8::is_ascii_digit) {
                i += 1;
            }
            let letter = b.get(i).map(u8::to_ascii_uppercase);
            if matches!(letter, Some(b'G' | b'X')) && b.get(i + 1).is_some_and(u8::is_ascii_digit) {
                let n = kind.get(start..i)?;
                return Some(format!("{n}{}", char::from(letter?)));
            }
        } else {
            i += 1;
        }
    }
    None
}

/// Where a cable's offer comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfferSource {
    Cores,
    Type,
}

impl OfferSource {
    pub fn word(self) -> &'static str {
        match self {
            OfferSource::Cores => "cores",
            OfferSource::Type => "type",
        }
    }
}

/// A cable's core offer and where it comes from; `None` when unknown.
pub fn cable_offer(props: &ecad_model::Properties) -> Option<(Vec<String>, OfferSource, String)> {
    if let Some(v) = props.get(CORES_KEY).map(|v| v.trim()).filter(|v| !v.is_empty())
        && let Ok(c) = parse_cores(v)
    {
        return Some((c, OfferSource::Cores, v.to_string()));
    }
    let t = props.get("type")?;
    let n = offer_from_type(t)?;
    parse_cores(&n).ok().map(|c| (c, OfferSource::Type, n))
}

/// How a cable's members meet its offer (§6.2).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CoreCount {
    /// The offer, when known.
    pub offer: Option<Vec<String>>,
    pub source: Option<OfferSource>,
    /// The offer as written (`7G`), for words.
    pub offer_text: Option<String>,
    /// Per member: the offered core it is counted as (`None`: over, or no offer).
    pub matched: Vec<Option<String>>,
    /// Offered cores no member uses, in offer order.
    pub spare: Vec<String>,
    /// Members beyond the offer (member indexes).
    pub over: Vec<usize>,
    /// Members whose stored label an earlier member already has (member indexes).
    pub twice: Vec<usize>,
}

impl CoreCount {
    /// Used: the members counted against the offer (every member when the offer is unknown).
    pub fn used(&self) -> usize {
        match &self.offer {
            Some(_) => self.matched.iter().filter(|m| m.is_some()).count(),
            None => self.matched.len(),
        }
    }

    /// `2 (6, GNYE)`, `0`, or `None` when the offer is unknown.
    pub fn spare_text(&self) -> Option<String> {
        self.offer.as_ref()?;
        Some(if self.spare.is_empty() { "0".into() } else { format!("{} ({})", self.spare.len(), self.spare.join(", ")) })
    }

    /// The core a new member takes: the first spare one.
    pub fn next_free(&self) -> Option<String> {
        self.spare.first().cloned()
    }
}

/// Match `members` to `offer` (§6.2): by label first, then the rest in member order.
pub fn count_cores(offer: Option<(Vec<String>, OfferSource, String)>, members: &[CableMember]) -> CoreCount {
    let mut twice = Vec::new();
    let mut labels: BTreeSet<String> = BTreeSet::new();
    for (i, m) in members.iter().enumerate() {
        if let Some(l) = m.core.as_deref().map(str::trim).filter(|l| !l.is_empty())
            && !labels.insert(l.to_ascii_uppercase())
        {
            twice.push(i);
        }
    }
    let Some((offer, source, text)) = offer else {
        return CoreCount { matched: vec![None; members.len()], twice, ..CoreCount::default() };
    };
    let mut free: Vec<Option<&String>> = offer.iter().map(Some).collect();
    let mut matched: Vec<Option<String>> = vec![None; members.len()];
    let mut done = vec![false; members.len()];
    for (i, m) in members.iter().enumerate() {
        let Some(l) = m.core.as_deref().map(str::trim) else { continue };
        if let Some(slot) = free.iter_mut().find(|s| s.is_some_and(|c| c.eq_ignore_ascii_case(l)))
            && let Some(c) = slot.take()
        {
            if let Some(x) = matched.get_mut(i) {
                *x = Some(c.clone());
            }
            if let Some(d) = done.get_mut(i) {
                *d = true;
            }
        }
    }
    let mut over = Vec::new();
    for i in 0..members.len() {
        if done.get(i).copied().unwrap_or(true) {
            continue;
        }
        match free.iter_mut().find(|s| s.is_some()).and_then(Option::take) {
            Some(c) => {
                if let Some(x) = matched.get_mut(i) {
                    *x = Some(c.clone());
                }
            }
            None => over.push(i),
        }
    }
    let spare = free.into_iter().flatten().cloned().collect();
    CoreCount { offer: Some(offer), source: Some(source), offer_text: Some(text), matched, spare, over, twice }
}

/// A cable's core count on `doc`.
pub fn cable_cores(doc: &Document, k: CableId) -> Option<CoreCount> {
    let cab = doc.cables.get(&k)?;
    Some(count_cores(cable_offer(&cab.properties), &cab.members))
}

/// The label `cablecore` gives a member whose label another member has (the `core-twice` fix):
/// the first spare core, else (no offer, or none spare) the next number after the highest
/// numeric label.
pub fn next_free_label(doc: &Document, k: CableId) -> Option<String> {
    let cab = doc.cables.get(&k)?;
    let count = count_cores(cable_offer(&cab.properties), &cab.members);
    if let Some(c) = count.next_free() {
        return Some(c);
    }
    let high = cab.members.iter().filter_map(|m| m.core.as_deref()?.trim().parse::<u64>().ok()).filter(|n| *n < 1_000_000).max().unwrap_or(0);
    Some((high + 1).to_string())
}

// ---------------------------------------------------------------- the strip model (§5, §11)

/// One conductor on one side of a terminal.
#[derive(Debug, Clone, PartialEq)]
pub struct SideCell {
    pub conductor: ConductorId,
    /// The far end as shown (`-K01:A1`, `junction 105-C-4`, `24V >`); `None` for a loose end.
    pub target: Option<String>,
    pub wire: Option<String>,
    pub cable: Option<(CableId, Option<String>)>,
    /// The member's stored core label.
    pub core: Option<String>,
    /// The far end's component (a device, a junction, a continuation).
    pub far: Option<ComponentId>,
}

/// One position of a strip's plan.
#[derive(Debug, Clone, PartialEq)]
pub struct TermRow {
    pub terminal: Option<TerminalId>,
    pub label: String,
    pub pe: bool,
    pub inside: Vec<SideCell>,
    pub outside: Vec<SideCell>,
    /// The bridge group (1, 2… per strip) this terminal is in.
    pub bridge_group: Option<usize>,
    pub place: Option<Place>,
    pub go: Option<(PageId, Pt)>,
}

impl TermRow {
    pub fn spare(&self) -> bool {
        self.terminal.is_none()
    }
}

/// One strip's derived data.
#[derive(Debug, Clone, PartialEq)]
pub struct StripData {
    pub component: ComponentId,
    pub tag: String,
    pub location: Option<String>,
    pub offer: Option<Offer>,
    /// The `terminals` text when it could not be read (shown, not used).
    pub drawn: usize,
    pub drawn_pe: usize,
    pub spare: usize,
    pub spare_pe: usize,
    pub rows: Vec<TermRow>,
    pub part_number: Option<String>,
    pub maker: Option<String>,
    pub part_number_pe: Option<String>,
    pub maker_pe: Option<String>,
    pub description: Option<String>,
    /// Terminals drawn with a label an earlier one has: (label, the second terminal).
    pub twice: Vec<(String, TerminalId)>,
    /// What this strip's rows read, for the update map.
    conductors: BTreeSet<ConductorId>,
    far: BTreeSet<ComponentId>,
    pages: BTreeSet<PageId>,
    cables: BTreeSet<CableId>,
}

impl StripData {
    /// `12 positions: 10 drawn, 2 spare`, or `4 drawn` with no offer.
    pub fn counts_text(&self) -> String {
        match self.offer {
            Some(o) => format!("{} positions: {} drawn, {} spare", o.positions, self.drawn, self.spare),
            None => format!("{} drawn", self.drawn),
        }
    }

    /// Feed-through and PE quantities for the BOM: drawn plus spare of each type.
    pub fn quantities(&self) -> (usize, usize) {
        (self.drawn - self.drawn_pe + self.spare - self.spare_pe, self.drawn_pe + self.spare_pe)
    }

    /// The highest drawn terminal (by label), for the too-many-terminals go-to.
    pub fn highest(&self) -> Option<&TermRow> {
        self.rows.iter().rev().find(|r| !r.spare())
    }

    /// A drawn row by terminal.
    pub fn row_of(&self, t: TerminalId) -> Option<&TermRow> {
        self.rows.iter().find(|r| r.terminal == Some(t))
    }
}

/// The strips of a project (see the module's update map).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Strips {
    pub strips: BTreeMap<ComponentId, StripData>,
    by_conductor: BTreeMap<ConductorId, BTreeSet<ComponentId>>,
    by_far: BTreeMap<ComponentId, BTreeSet<ComponentId>>,
    by_page: BTreeMap<PageId, BTreeSet<ComponentId>>,
    by_cable: BTreeMap<CableId, BTreeSet<ComponentId>>,
    /// How many strips the last update recomputed (tests, bench).
    pub last_work: usize,
}

fn add_to<K: Ord + Copy>(map: &mut BTreeMap<K, BTreeSet<ComponentId>>, keys: &BTreeSet<K>, c: ComponentId) {
    for k in keys {
        map.entry(*k).or_default().insert(c);
    }
}

fn remove_from<K: Ord + Copy>(map: &mut BTreeMap<K, BTreeSet<ComponentId>>, keys: &BTreeSet<K>, c: ComponentId) {
    for k in keys {
        if let Some(set) = map.get_mut(k) {
            set.remove(&c);
            if set.is_empty() {
                map.remove(k);
            }
        }
    }
}

/// What a recompute of the strips needs to know beyond the document.
pub struct StripScope<'a> {
    pub devices: &'a BTreeSet<ComponentId>,
    pub conductors: &'a BTreeSet<ConductorId>,
    pub cables: &'a BTreeSet<CableId>,
    pub pages: &'a BTreeSet<PageId>,
}

impl Strips {
    /// Every strip, from scratch.
    pub fn build(doc: &Document, o: &Outputs, cx: &PageCx) -> Strips {
        let mut s = Strips::default();
        for c in doc.components.keys() {
            if is_strip(doc, *c) {
                s.set(doc, *c, o, cx);
            }
        }
        s
    }

    /// The strips `scope` touches, before recomputing.
    pub fn touched(&self, doc: &Document, scope: &StripScope<'_>) -> BTreeSet<ComponentId> {
        let mut out = BTreeSet::new();
        for c in scope.devices {
            if self.strips.contains_key(c) || is_strip(doc, *c) {
                out.insert(*c);
            }
            out.extend(self.by_far.get(c).into_iter().flatten().copied());
        }
        let mut wires: BTreeSet<ConductorId> = scope.conductors.clone();
        for k in scope.cables {
            out.extend(self.by_cable.get(k).into_iter().flatten().copied());
            if let Some(cab) = doc.cables.get(k) {
                wires.extend(cab.members.iter().map(|m| m.conductor));
            }
        }
        for w in &wires {
            out.extend(self.by_conductor.get(w).into_iter().flatten().copied());
            if let Some(cond) = doc.conductors.get(w) {
                out.extend(cond.ends.iter().flatten().filter_map(|t| strip_of_terminal(doc, *t)));
            }
        }
        for p in scope.pages {
            out.extend(self.by_page.get(p).into_iter().flatten().copied());
        }
        out
    }

    /// Recompute `which` on `doc`.
    pub fn recompute(&mut self, doc: &Document, which: &BTreeSet<ComponentId>, o: &Outputs, cx: &PageCx) {
        for c in which {
            self.set(doc, *c, o, cx);
        }
        self.last_work = which.len();
    }

    fn remove(&mut self, c: ComponentId) {
        let Some(old) = self.strips.remove(&c) else { return };
        remove_from(&mut self.by_conductor, &old.conductors, c);
        remove_from(&mut self.by_far, &old.far, c);
        remove_from(&mut self.by_page, &old.pages, c);
        remove_from(&mut self.by_cable, &old.cables, c);
    }

    fn set(&mut self, doc: &Document, c: ComponentId, o: &Outputs, cx: &PageCx) {
        self.remove(c);
        if !is_strip(doc, c) {
            return;
        }
        let d = strip_data(doc, c, o, cx);
        add_to(&mut self.by_conductor, &d.conductors, c);
        add_to(&mut self.by_far, &d.far, c);
        add_to(&mut self.by_page, &d.pages, c);
        add_to(&mut self.by_cable, &d.cables, c);
        self.strips.insert(c, d);
    }

    /// The strips sorted by tag naturally.
    pub fn sorted(&self) -> Vec<&StripData> {
        let mut v: Vec<&StripData> = self.strips.values().collect();
        v.sort_by(|a, b| crate::locations::natural_key(&a.tag).cmp(&crate::locations::natural_key(&b.tag)).then(a.component.cmp(&b.component)));
        v
    }

    /// Whether `other` holds the same strips and indexes (verify mode: the kept model against a
    /// rebuild).
    pub fn same_as(&self, other: &Strips) -> bool {
        self.strips == other.strips
            && self.by_conductor == other.by_conductor
            && self.by_far == other.by_far
            && self.by_page == other.by_page
            && self.by_cable == other.by_cable
    }

    pub fn get(&self, c: ComponentId) -> Option<&StripData> {
        self.strips.get(&c)
    }

    /// How many strips.
    pub fn len(&self) -> usize {
        self.strips.len()
    }

    pub fn is_empty(&self) -> bool {
        self.strips.is_empty()
    }
}

/// Which way a route leaves a point: `true` for up or left (inside, §5.2 step 3).
fn leaves_inward(from: Pt, next: Pt) -> bool {
    let (dx, dy) = (next.x - from.x, next.y - from.y);
    if dy.abs() >= dx.abs() { dy > 0.0 } else { dx < 0.0 }
}

/// The route direction of conductor `w` at its end `end` on `page`: inside (`true`) or not;
/// `None` when it has no route there.
fn geometric_inside(doc: &Document, w: ConductorId, end: End, page: PageId) -> Option<bool> {
    let (_, _, v) = crate::wires::route_vertices(doc, w, page)?;
    let (a, b) = match end {
        End::A => (*v.first()?, *v.get(1)?),
        End::B => (*v.last()?, *v.get(v.len().checked_sub(2)?)?),
    };
    Some(leaves_inward(a, b))
}

/// The cell of a terminal's conductor's far end, worked out like the wire report's.
fn far_cell(doc: &Document, o: &Outputs, w: ConductorId, far: End, cx: &PageCx) -> (Option<String>, Option<ComponentId>) {
    let Some(cond) = doc.conductors.get(&w) else { return (None, None) };
    let t = cond.end(far);
    let comp = t.and_then(|t| doc.terminals.get(&t)).map(|x| x.component);
    // Only a junction's cell needs where the end is drawn (its place); the rest are named.
    let at = match comp.and_then(|c| crate::wires::kind_of(doc, c)) {
        Some(ecad_model::connectivity::JUNCTION) => {
            let [a, b] = crate::outputs::end_points(doc, w, cx);
            match far {
                End::A => a,
                End::B => b,
            }
        }
        _ => None,
    };
    let cell = o.end_cell_of(doc, t, at, cx);
    (cell.text(doc.settings.reference_style), comp)
}

fn clean(v: Option<&String>) -> Option<String> {
    v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// One strip's data on `doc`.
fn strip_data(doc: &Document, c: ComponentId, o: &Outputs, cx: &PageCx) -> StripData {
    let props = doc.components.get(&c).map(|x| x.properties.clone()).unwrap_or_default();
    let device = o.device(c);
    let location = match device {
        Some(d) => d.location.value.clone(),
        None => crate::locations::resolve(doc, c).value,
    };
    let mut d = StripData {
        component: c,
        tag: props.get(ecad_model::tag::TAG_KEY).cloned().unwrap_or_else(|| "?".into()),
        location: location.clone(),
        offer: props.get(OFFER_KEY).and_then(|v| parse_offer(v).ok()),
        part_number: clean(props.get("part_number")),
        maker: clean(props.get("maker")),
        part_number_pe: clean(props.get(PE_PART_KEY)),
        maker_pe: clean(props.get(PE_MAKER_KEY)),
        description: clean(props.get("description")),
        drawn: 0,
        drawn_pe: 0,
        spare: 0,
        spare_pe: 0,
        rows: Vec::new(),
        twice: Vec::new(),
        conductors: BTreeSet::new(),
        far: BTreeSet::new(),
        pages: BTreeSet::new(),
        cables: BTreeSet::new(),
    };
    let drawn = drawn_at(doc, c);
    let mine: BTreeSet<TerminalId> = drawn.iter().map(|(t, _, _, _)| *t).collect();
    let mut grids: BTreeMap<PageId, ecad_model::sheet::Grid> = BTreeMap::new();
    let mut rows: Vec<TermRow> = Vec::new();
    // Bridges, for groups: union of terminals joined by a bridge.
    let mut parent: BTreeMap<TerminalId, TerminalId> = mine.iter().map(|t| (*t, *t)).collect();
    fn find(p: &mut BTreeMap<TerminalId, TerminalId>, t: TerminalId) -> TerminalId {
        let mut x = t;
        // Bounded by the strip's terminals.
        for _ in 0..=p.len() {
            match p.get(&x) {
                Some(n) if *n != x => x = *n,
                _ => break,
            }
        }
        x
    }
    let mut bridged: BTreeSet<TerminalId> = BTreeSet::new();
    for (t, rid, pe, at) in &drawn {
        let r = doc.representations.get(rid);
        let page = r.map(|r| r.page);
        let at = *at;
        if let Some(p) = page {
            d.pages.insert(p);
        }
        let label = crate::interact::terminal_name(doc, *t);
        let mut row = TermRow {
            terminal: Some(*t),
            label,
            pe: *pe,
            inside: Vec::new(),
            outside: Vec::new(),
            bridge_group: None,
            place: page.zip(at).and_then(|(p, a)| {
                let grid = grids.entry(p).or_insert_with(|| ecad_model::sheet::Grid::of(doc, p));
                ecad_model::derive::place_on(grid, cx.number(p)?, a)
            }),
            go: page.zip(at),
        };
        for (w, end) in doc.ends_at(*t) {
            d.conductors.insert(w);
            let Some(cond) = doc.conductors.get(&w) else { continue };
            let far_end = match end {
                End::A => End::B,
                End::B => End::A,
            };
            let other = cond.end(far_end);
            if let Some(o2) = other
                && o2 != *t
                && mine.contains(&o2)
            {
                // A bridge.
                bridged.insert(*t);
                bridged.insert(o2);
                let (ra, rb) = (find(&mut parent, *t), find(&mut parent, o2));
                if ra != rb {
                    parent.insert(ra.max(rb), ra.min(rb));
                }
                continue;
            }
            let (target, far) = far_cell(doc, o, w, far_end, cx);
            if let Some(f) = far {
                d.far.insert(f);
            }
            let cable = doc.cables_of(w).into_iter().next().map(|k| {
                d.cables.insert(k);
                let tag = doc.cables.get(&k).and_then(|x| clean(x.properties.get("tag")));
                (k, tag)
            });
            let core = cable.as_ref().and_then(|(k, _)| doc.cables.get(k)?.members.iter().find(|m| m.conductor == w)?.core.clone());
            let cell = SideCell { conductor: w, target, wire: o.wire(w).and_then(|r| r.wire_number.clone()), cable, core, far };
            // §5.2: cable core → outside; far device's location against the strip's; geometry.
            let inside = if cell.cable.is_some() {
                false
            } else {
                let far_loc = far.filter(|f| crate::locations::is_device(doc, *f)).and_then(|f| match o.device(f) {
                    Some(x) => x.location.value.clone(),
                    None => crate::locations::resolve(doc, f).value,
                });
                match (&location, far_loc) {
                    (Some(a), Some(b)) => *a == b,
                    _ => page.and_then(|p| geometric_inside(doc, w, end, p)).unwrap_or(true),
                }
            };
            if inside {
                row.inside.push(cell);
            } else {
                row.outside.push(cell);
            }
        }
        rows.push(row);
    }
    // Bridge groups, numbered in label order.
    let label_key = |r: &TermRow| (crate::xref::natural_key(&r.label), r.terminal);
    rows.sort_by_key(label_key);
    let mut groups: BTreeMap<TerminalId, usize> = BTreeMap::new();
    for r in &mut rows {
        let Some(t) = r.terminal else { continue };
        if !bridged.contains(&t) {
            continue;
        }
        let root = find(&mut parent, t);
        let n = groups.len() + 1;
        r.bridge_group = Some(*groups.entry(root).or_insert(n));
    }
    // Labels drawn twice (merged branches, a hostile file).
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for r in &rows {
        if let Some(t) = r.terminal
            && !seen.insert(r.label.clone())
        {
            d.twice.push((r.label.clone(), t));
        }
    }
    d.drawn = rows.len();
    d.drawn_pe = rows.iter().filter(|r| r.pe).count();
    // Spare positions (FP1): positions 1…n with no drawn terminal of that label.
    if let Some(off) = d.offer {
        let spare: Vec<u32> = (1..=off.positions).filter(|n| !seen.contains(&n.to_string())).collect();
        let pe_wanted = (off.pe as usize).saturating_sub(d.drawn_pe).min(spare.len());
        let last_pe = rows.iter().filter(|r| r.pe).filter_map(|r| r.label.parse::<u32>().ok()).max().unwrap_or(0);
        let mut pe_set: BTreeSet<u32> = spare.iter().copied().filter(|n| *n > last_pe).take(pe_wanted).collect();
        for n in spare.iter().rev() {
            if pe_set.len() >= pe_wanted {
                break;
            }
            pe_set.insert(*n);
        }
        for n in &spare {
            rows.push(TermRow {
                terminal: None,
                label: n.to_string(),
                pe: pe_set.contains(n),
                inside: Vec::new(),
                outside: Vec::new(),
                bridge_group: None,
                place: None,
                go: None,
            });
        }
        d.spare = spare.len();
        d.spare_pe = pe_set.len();
        rows.sort_by_key(label_key);
    }
    d.rows = rows;
    d
}

// ---------------------------------------------------------------- hover (§7.1)

/// The far end of `w` from end `mine`, in words (`-K01:A1`, `junction`, `24V >`, `loose`).
fn far_words(doc: &Document, w: ConductorId, mine: End) -> (String, Option<ComponentId>) {
    let far = match mine {
        End::A => End::B,
        End::B => End::A,
    };
    let Some(t) = doc.conductors.get(&w).and_then(|c| c.end(far)) else { return ("loose".into(), None) };
    let Some(c) = doc.terminals.get(&t).map(|x| x.component) else { return ("loose".into(), None) };
    match crate::wires::kind_of(doc, c) {
        Some(ecad_model::connectivity::JUNCTION) => ("junction".into(), Some(c)),
        Some(ecad_model::connectivity::CONTINUATION) => {
            let name = doc.components.get(&c).and_then(|x| x.properties.get(ecad_model::connectivity::NAME)).cloned().unwrap_or_default();
            (format!("{name} >"), Some(c))
        }
        _ => (crate::terminal_cmds::terminal_words(doc, t), Some(c)),
    }
}

/// The hover lines of a terminal symbol `here` of strip `c` (design 028 §7.1), from the document
/// alone: `-X01:3  feed-through  +CP1`, `inside  -K01:A1  24V-07`, `outside  -W01 core 3 → -M01:U1`,
/// `bridged to 4`, then the strip's positions.
pub fn terminal_hover(doc: &Document, c: ComponentId, here: RepresentationId) -> Vec<String> {
    let mut lines = Vec::new();
    let Some(r) = doc.representations.get(&here) else { return lines };
    let pe = rep_is_pe(doc, r);
    let location = crate::locations::resolve(doc, c).value;
    let conn = doc.connectivity();
    let number = |w: ConductorId| {
        conn.net_of_conductor(w).and_then(|n| conn.net(n)).and_then(|n| ecad_model::wiring::net_number(doc, n)).unwrap_or_else(|| "?".into())
    };
    let mine: BTreeSet<TerminalId> = doc.terminals_of(c).map(|(t, _)| t).collect();
    for (t, _) in crate::devices::shown_terminals(doc, c, r) {
        let kind = if pe { "PE" } else { "feed-through" };
        lines.push(format!("{}  {kind}  {}", crate::terminal_cmds::terminal_words(doc, t), location.clone().unwrap_or_else(|| "?".into())));
        let mut bridged = Vec::new();
        for (w, end) in doc.ends_at(t) {
            let far_t = doc.conductors.get(&w).and_then(|x| {
                x.end(match end {
                    End::A => End::B,
                    End::B => End::A,
                })
            });
            if let Some(o) = far_t.filter(|o| *o != t && mine.contains(o)) {
                bridged.push(crate::interact::terminal_name(doc, o));
                continue;
            }
            let (target, far) = far_words(doc, w, end);
            let cable = doc.cables_of(w).into_iter().next();
            let inside = if cable.is_some() {
                false
            } else {
                let far_loc = far.filter(|f| crate::locations::is_device(doc, *f)).and_then(|f| crate::locations::resolve(doc, f).value);
                match (&location, far_loc) {
                    (Some(a), Some(b)) => *a == b,
                    _ => geometric_inside(doc, w, end, r.page).unwrap_or(true),
                }
            };
            match cable {
                Some(k) => {
                    let core = doc
                        .cables
                        .get(&k)
                        .and_then(|x| x.members.iter().find(|m| m.conductor == w))
                        .and_then(|m| m.core.clone())
                        .unwrap_or_else(|| "?".into());
                    lines.push(format!("outside  {} core {core} \u{2192} {target}", crate::slash::cable_name(doc, k)));
                }
                None => lines.push(format!("{}  {target}  {}", if inside { "inside" } else { "outside" }, number(w))),
            }
        }
        if !bridged.is_empty() {
            lines.push(format!("bridged to {}", bridged.join(", ")));
        }
    }
    let drawn = drawn_terminals(doc, c).len();
    let tag = crate::devices::tag_words(doc, c);
    lines.push(match offer_of(doc, c) {
        Some(o) => {
            let labels: BTreeSet<String> = doc.terminals_of(c).filter_map(|(_, t)| t.label.clone()).collect();
            let spare = (1..=o.positions).filter(|n| !labels.contains(&n.to_string())).count();
            format!("{tag}: {} positions, {drawn} drawn, {spare} spare", o.positions)
        }
        None => format!("{tag}: {drawn} drawn"),
    });
    lines
}

/// A cable's hover line: `-W01  7G1.5: 5 used, 2 spare (6, GNYE)`; `None` with no offer.
pub fn cable_hover(doc: &Document, k: CableId) -> Option<String> {
    let cab = doc.cables.get(&k)?;
    let count = count_cores(cable_offer(&cab.properties), &cab.members);
    let named = cab.properties.get("type").cloned().or_else(|| count.offer_text.clone())?;
    count.offer.as_ref()?;
    let spare = if count.spare.is_empty() { "0 spare".to_string() } else { format!("{} spare ({})", count.spare.len(), count.spare.join(", ")) };
    let over = if count.over.is_empty() { String::new() } else { format!(", {} over", count.over.len()) };
    Some(format!("{}  {named}: {} used, {spare}{over}", crate::slash::cable_name(doc, k), count.used()))
}

/// The rows a generated `terminals` sheet needs for `strip` (`None`: every strip): one heading per
/// strip and one per position's conductor line (at least one).
pub fn plan_lines(s: &StripData) -> usize {
    1 + s.rows.iter().map(|r| r.inside.len().max(r.outside.len()).max(1)).sum::<usize>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offers_read_and_refuse_with_a_way_forward() {
        assert_eq!(parse_offer("12"), Ok(Offer { positions: 12, pe: 0 }));
        assert_eq!(parse_offer(" 12 + 2pe "), Ok(Offer { positions: 12, pe: 2 }));
        assert_eq!(offer_value(Some("12 + 2 PE")), Ok(Some("12+2PE".into())));
        assert_eq!(offer_value(Some("  ")), Ok(None));
        assert!(parse_offer("12X").unwrap_err().contains("Write 12, or 12+2PE"));
        assert!(parse_offer("1000").is_err());
        assert!(parse_offer("0").is_err());
        assert!(parse_offer("2+3PE").unwrap_err().contains("3+3PE"));
        assert!(parse_offer(&"9".repeat(10_000)).is_err());
    }

    #[test]
    fn core_offers_from_cores_and_from_type() {
        assert_eq!(parse_cores("3").unwrap(), ["1", "2", "3"]);
        assert_eq!(parse_cores("3g").unwrap(), ["1", "2", GNYE]);
        assert_eq!(parse_cores("3X").unwrap(), ["1", "2", "3"]);
        assert_eq!(parse_cores("BN, BK ,GNYE").unwrap(), ["BN", "BK", "GNYE"]);
        assert!(parse_cores("7Q").unwrap_err().contains("Write 7, 7G, 5X"));
        assert!(parse_cores("1000").is_err());
        assert!(parse_cores("BN,BN").is_err());
        assert!(parse_cores("BN,,BK").is_err());
        assert_eq!(cores_value(Some("7g")), Ok(Some("7G".into())));
        assert_eq!(offer_from_type("YSLY-JZ 7G1.5").as_deref(), Some("7G"));
        assert_eq!(offer_from_type("4x0.75").as_deref(), Some("4X"));
        assert_eq!(offer_from_type("H07RN-F"), None);
        assert_eq!(offer_from_type("12G"), None);
    }

    #[test]
    fn members_meet_the_offer_by_label_then_in_order() {
        let m = |core: Option<&str>| CableMember { conductor: ConductorId(1), core: core.map(str::to_string) };
        let offer = |t: &str| Some((parse_cores(t).unwrap(), OfferSource::Cores, t.to_string()));
        let c = count_cores(offer("7G"), &[m(None), m(None), m(Some("3")), m(None), m(None)]);
        assert_eq!(c.used(), 5);
        assert_eq!(c.spare, ["6", GNYE]);
        assert_eq!(c.spare_text().as_deref(), Some("2 (6, GNYE)"));
        assert_eq!(c.matched[0].as_deref(), Some("1"));
        assert_eq!(c.matched[2].as_deref(), Some("3"));
        assert_eq!(c.matched[3].as_deref(), Some("4"));
        let c = count_cores(offer("3"), &[m(Some("1")), m(Some("1")), m(None), m(None)]);
        assert_eq!(c.twice, [1]);
        assert_eq!(c.over, [3]);
        assert!(c.spare.is_empty());
        let c = count_cores(None, &[m(None), m(None)]);
        assert_eq!(c.used(), 2);
        assert_eq!(c.spare_text(), None);
    }
}
