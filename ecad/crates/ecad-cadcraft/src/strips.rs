//! Terminal strips and cable cores (brief 028, design 028 §2–§9): the terminal plan per strip
//! and the core count per cable, derived from the drawing and never stored.
//!
//! # Semantics
//! - A **strip** is a device whose definition is a terminal starter (`terminal`, `terminalpe`);
//!   each terminal is one symbol with one connection point (018's model, owner answer Q1). Its
//!   **type** is its symbol's: `terminalpe` is PE, anything else feed-through.
//! - The strip's **offer** is its `terminals` property, `12` or `12+2PE`: feed-through positions,
//!   then PE positions (design 028 §3.3, owner decision 2026-11-12: `12+2PE` is 14 positions; a
//!   BOM of 12 feed-through and 2 PE). Positions with no drawn terminal are **spare**.
//! - A conductor whose two ends are on two terminals of one strip is a **bridge** (Q5). Every other
//!   conductor on a terminal is **inside** or **outside** by §5.2's rule: a cable core is outside;
//!   else the far device's location against the strip's; else the route's direction at the
//!   terminal (up or left is inside).
//! - A cable's **core offer** is its `cores` property, else read from its `type` (`7G1.5` → `7G`)
//!   (FP2, Q9). Members are matched to it by label first, then in member order: **used**,
//!   **spare** and **over** (§6.2).
//! - Nothing here is refused because of a check; it is flagged (§8).
//! - **Brief 036:** a strip's rows are its **levels**, grouped by position in plan order
//!   ([`crate::terminal_types::positions`]); a position whose type has more levels than are drawn
//!   shows them as **free** rows; spare positions come per type from the offer (`12+2PE+4x3`), each
//!   with its type's levels. Counts (`drawn`, `spare`) and BOM quantities are in positions.
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
//! | brief 036: a strip's `term_label.*`, `rating.*`, `terminal.*`, `part_number.<code>` (its own properties: a device in scope) | that strip |
//! | brief 036: project `terminal.level.separator` or `terminal.<code>.levels` | the strips inheriting it (`inherits`, recorded only by strips with a multi-level position): rebuilt |
//! | brief 036: project `strip_<code>_*` or `terminal.<code>.description` | the strips inheriting it: their type part data and accessories only ([`Strips::recompute_acc`]) |
//! | brief 036: a fuse terminal device created, changed or deleted | the `fuse-device` set ([`Strips::note_devices`]) |
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

/// A strip's drawn terminals with the page and point each is drawn at (brief 030: link lines).
pub fn drawn_points(doc: &Document, c: ComponentId) -> Vec<(TerminalId, RepresentationId, PageId, Pt)> {
    drawn_at(doc, c).into_iter().filter_map(|(t, rid, _, at)| Some((t, rid, doc.representations.get(&rid)?.page, at?))).collect()
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

/// A strip's offer (FP1, brief 036 §7.1): positions per terminal type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Offer {
    /// Every position, of every type (`12+2PE+4x3`: 18).
    pub positions: u32,
    /// How many of them are PE.
    pub pe: u32,
    /// Positions per type, in [`crate::terminal_types::TYPES`] order.
    pub counts: crate::terminal_types::Counts,
}

impl Offer {
    /// As stored: `12`, `12+2PE`, `12+2PE+4x3` (catalogue order).
    pub fn text(self) -> String {
        crate::terminal_types::counts_text(&self.counts)
    }

    /// Positions offered of type `t`.
    pub fn of(&self, t: &crate::terminal_types::TermType) -> u32 {
        self.counts.get(crate::terminal_types::index_of(t)).copied().unwrap_or(0)
    }
}

/// Read an offer: `+`-separated `<count><code>` items (`12+2PE+4x3`), spaces ignored, codes
/// case-insensitive.
///
/// # Errors
/// Unreadable text, an unknown code, no positions, or more than [`MAX_POSITIONS`] in all, in
/// words with the way forward.
pub fn parse_offer(text: &str) -> Result<Offer, String> {
    let counts = crate::terminal_types::parse_counts(text)?;
    let positions = counts.iter().fold(0u32, |a, b| a.saturating_add(*b));
    Ok(Offer { positions, pe: counts.get(1).copied().unwrap_or(0), counts })
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

/// The offer Checks' **Offer drawn** writes (design 028 §8, brief 036 §7.1): every type at
/// least its drawn positions; with no type codes, 028's rule (every drawn terminal, PE kept).
pub fn offer_drawn_value(doc: &Document, c: ComponentId) -> String {
    use crate::terminal_types as tt;
    let ps = tt::positions(doc, c);
    let mut drawn: tt::Counts = [0; tt::TYPE_COUNT];
    for p in &ps {
        if let Some(n) = drawn.get_mut(tt::index_of(p.ty)) {
            *n = n.saturating_add(1);
        }
    }
    let offer = offer_of(doc, c);
    let legacy = offer.is_none_or(|o| o.counts.iter().skip(2).all(|n| *n == 0)) && ps.iter().all(|p| p.ty.legacy());
    let mut counts: tt::Counts = [0; tt::TYPE_COUNT];
    if legacy {
        let total = u32::try_from(ps.len()).unwrap_or(MAX_POSITIONS).min(MAX_POSITIONS);
        let pe = offer.map_or(0, |o| o.pe).max(drawn.get(1).copied().unwrap_or(0)).min(total);
        if let Some(x) = counts.get_mut(0) {
            *x = total - pe;
        }
        if let Some(x) = counts.get_mut(1) {
            *x = pe;
        }
    } else {
        for (i, x) in counts.iter_mut().enumerate() {
            let offered = offer.and_then(|o| o.counts.get(i).copied()).unwrap_or(0);
            *x = offered.max(drawn.get(i).copied().unwrap_or(0));
        }
    }
    tt::counts_text(&counts)
}

/// A strip's stored offer, if readable.
pub fn offer_of(doc: &Document, c: ComponentId) -> Option<Offer> {
    doc.components.get(&c)?.properties.get(OFFER_KEY).and_then(|v| parse_offer(v).ok())
}

// ---------------------------------------------------------------- cores (§6)

fn cores_refusal(text: &str) -> String {
    let shown: String = text.chars().take(24).collect();
    let more = if text.chars().count() > 24 { "\u{2026}" } else { "" };
    format!("\"{shown}{more}\" is not a core list. Write 7, 7G, 5X, 4x2x0.25 (pairs), or BN,BK,GY,BU,GNYE.")
}

/// A core count as a cable type writes it (design 028 §6.1, owner decisions 2026-11-12).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Notation {
    /// Cores, or pairs or triples when `group` is 2 or 3.
    pub n: usize,
    /// `G` (one of the n is green-yellow): `Some(true)`; `X` (none): `Some(false)`; not stated:
    /// `None` (no green-yellow expected).
    pub pe: Option<bool>,
    /// 1, or 2 for pairs and 3 for triples.
    pub group: usize,
}

impl Notation {
    /// How many cores.
    pub fn cores(self) -> usize {
        self.n.saturating_mul(self.group)
    }

    /// The cores' labels: `1`…`n` (`G`: the last is GNYE), or `1a`, `1b`, `2a`… in pairs.
    pub fn labels(self) -> Vec<String> {
        if self.group > 1 {
            let letters = ['a', 'b', 'c'];
            return (1..=self.n).flat_map(|i| letters.iter().take(self.group).map(move |l| format!("{i}{l}"))).collect();
        }
        match self.pe {
            Some(true) => (1..self.n).map(|i| i.to_string()).chain(std::iter::once(GNYE.to_string())).collect(),
            _ => (1..=self.n).map(|i| i.to_string()).collect(),
        }
    }

    /// As shown: `7G`, `7X`, `3`, `4x2`.
    pub fn text(self) -> String {
        match (self.group, self.pe) {
            (g, _) if g > 1 => format!("{}x{g}", self.n),
            (_, Some(true)) => format!("{}G", self.n),
            (_, Some(false)) => format!("{}X", self.n),
            (_, None) => self.n.to_string(),
        }
    }
}

/// Most characters of a type the notation scan reads.
const MAX_TYPE_CHARS: usize = 256;

fn is_times(c: char) -> bool {
    matches!(c, 'x' | 'X' | '\u{d7}')
}

/// The digits at `i` as a number (capped) and the index after them.
fn digits_at(c: &[char], i: usize) -> Option<(usize, usize)> {
    let mut j = i;
    let mut n: usize = 0;
    while let Some(d) = c.get(j).and_then(|ch| ch.to_digit(10)) {
        n = n.saturating_mul(10).saturating_add(d as usize).min(1_000_000);
        j += 1;
    }
    (j > i).then_some((n, j))
}

fn skip_spaces(c: &[char], mut i: usize) -> usize {
    while c.get(i).is_some_and(|ch| *ch == ' ') {
        i += 1;
    }
    i
}

/// The first core count `text` writes (design 028 §6.1): `nGs` / `nXs` (G: green-yellow among
/// the n; X: none); `n x s` with a lower-case multiplier (n cores, green-yellow not stated); pairs
/// and triples `n(2xs)`, `nx2xs`, `n x 2 x s` (`3` for triples). Words such as `C`, `CY` and
/// `(TP)` hold no count and are passed over; `4(2x0.25)` is 4 pairs, never 2 cores.
pub fn notation(text: &str) -> Option<Notation> {
    let c: Vec<char> = text.chars().take(MAX_TYPE_CHARS).collect();
    let digit = |i: usize| c.get(i).is_some_and(char::is_ascii_digit);
    let mut i = 0;
    while i < c.len() {
        let starts = digit(i) && (i == 0 || !c.get(i - 1).is_some_and(|p| p.is_ascii_digit() || *p == '.' || *p == ','));
        if !starts {
            i += 1;
            continue;
        }
        let Some((n, j)) = digits_at(&c, i) else { break };
        let ok_n = (1..=MAX_CORES).contains(&n);
        let k = skip_spaces(&c, j);
        match c.get(k) {
            // n(2xs): pairs or triples.
            Some('(') if ok_n => {
                let k2 = skip_spaces(&c, k + 1);
                if let Some((g, k3)) = digits_at(&c, k2) {
                    let k4 = skip_spaces(&c, k3);
                    if (2..=3).contains(&g) && c.get(k4).is_some_and(|x| is_times(*x)) && digit(skip_spaces(&c, k4 + 1)) {
                        return Some(Notation { n, pe: None, group: g });
                    }
                }
            }
            Some(x) if ok_n && is_times(*x) => {
                let k2 = skip_spaces(&c, k + 1);
                if let Some((m, k3)) = digits_at(&c, k2) {
                    // n x 2 x s: pairs or triples.
                    let k4 = skip_spaces(&c, k3);
                    if (2..=3).contains(&m) && c.get(k4).is_some_and(|y| is_times(*y)) && digit(skip_spaces(&c, k4 + 1)) {
                        return Some(Notation { n, pe: None, group: m });
                    }
                    // nXs (an upper-case X straight after the count): no green-yellow; n x s: not stated.
                    let pe = (*x == 'X' && k == j).then_some(false);
                    return Some(Notation { n, pe, group: 1 });
                }
            }
            Some('G' | 'g') if ok_n && k == j && digit(k + 1) => return Some(Notation { n, pe: Some(true), group: 1 }),
            _ => {}
        }
        i = j;
    }
    None
}

/// The core count a cable type names (`YSLY-JZ 7G1.5` → 7 with green-yellow): [`notation`],
/// with `-JZ` meaning G and `-OZ` meaning X when the count itself does not say.
pub fn type_notation(kind: &str) -> Option<Notation> {
    let mut n = notation(kind)?;
    if n.group == 1 && n.pe.is_none() {
        let upper = kind.to_ascii_uppercase();
        if upper.contains("-JZ") {
            n.pe = Some(true);
        } else if upper.contains("-OZ") {
            n.pe = Some(false);
        }
    }
    (n.cores() <= MAX_CORES).then_some(n)
}

/// Read a core offer (§6.1): `7`, `7G`, `5X`, a type's notation (`4x2x0.25`, `4(2x0.25)`,
/// `3x1.5`, `12G0.5`), or a list `BN,BK,GY,BU,GNYE`.
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
        let pe = match letter {
            Some('G') => Some(true),
            Some('X') => Some(false),
            _ => None,
        };
        return Ok(Notation { n, pe, group: 1 }.labels());
    }
    if !t.contains(',') && t.chars().any(|c| c.is_ascii_digit()) {
        let n = type_notation(t).ok_or_else(|| cores_refusal(text))?;
        return Ok(n.labels());
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

/// The `cores` value as stored (counts upper-case, lists trimmed, notations as written); empty
/// clears it.
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
    if !v.contains(',') && v.chars().any(|c| c.is_ascii_digit()) {
        return Ok(Some(v.to_string()));
    }
    Ok(Some(cores.join(",")))
}

/// The core count a cable type names, as shown (`YSLY-JZ 7G1.5` → `7G`, `4x2x0.25` → `4x2`).
pub fn offer_from_type(kind: &str) -> Option<String> {
    type_notation(kind).map(Notation::text)
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
    let n = type_notation(props.get("type")?)?;
    Some((n.labels(), OfferSource::Type, n.text()))
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

/// One row of a strip's plan: one level of a position (brief 036 §7.3; a one-level position is
/// one row, as in 028).
#[derive(Debug, Clone, PartialEq)]
pub struct TermRow {
    /// The level's terminal (the in point of a two-point level); `None` for a free or spare row.
    pub terminal: Option<TerminalId>,
    /// The out point of a two-point level (its outside cells come from it, Q6).
    pub out: Option<TerminalId>,
    /// The **position** label (what links, accessories and renumbering name).
    pub label: String,
    /// The shown label (§4.1): `1.L`, or the level's own label.
    pub shown: String,
    pub label_overridden: bool,
    /// The level number (1 = top deck) and its name.
    pub level: u32,
    pub level_name: String,
    /// The position's type (§2.2).
    pub ty: &'static crate::terminal_types::TermType,
    /// A level of a drawn position that no symbol draws.
    pub free: bool,
    pub pe: bool,
    /// A fuse position's rating (`rating.<p>`).
    pub rating: Option<String>,
    /// The internal bridge group of the position's type this level is in (1…).
    pub internal_bridge: Option<usize>,
    pub inside: Vec<SideCell>,
    pub outside: Vec<SideCell>,
    /// The bridge group (1, 2… per strip) this terminal is in.
    pub bridge_group: Option<usize>,
    pub place: Option<Place>,
    pub go: Option<(PageId, Pt)>,
}

impl TermRow {
    /// A position the offer adds, with no level drawn.
    pub fn spare(&self) -> bool {
        self.terminal.is_none() && !self.free
    }

    /// `drawn`, `free` or `spare`.
    pub fn state(&self) -> &'static str {
        if self.free {
            "free"
        } else if self.terminal.is_none() {
            "spare"
        } else {
            "drawn"
        }
    }

    /// Whether the position's type breaks the net (disconnect, fuse).
    pub fn breaks(&self) -> bool {
        self.ty.breaks
    }

    fn blank(label: String, ty: &'static crate::terminal_types::TermType, level: u32, level_name: String) -> TermRow {
        TermRow {
            terminal: None,
            out: None,
            shown: label.clone(),
            label,
            label_overridden: false,
            level,
            level_name,
            ty,
            free: false,
            pe: ty.pe_level == Some(level),
            rating: None,
            internal_bridge: None,
            inside: Vec::new(),
            outside: Vec::new(),
            bridge_group: None,
            place: None,
            go: None,
        }
    }
}

/// One terminal type on a strip: positions drawn and spare, and its BOM part data (§7.2).
#[derive(Debug, Clone, PartialEq)]
pub struct TypeCount {
    pub ty: &'static crate::terminal_types::TermType,
    pub drawn: usize,
    pub spare: usize,
    pub part_number: Option<String>,
    pub maker: Option<String>,
    pub description: String,
}

impl TypeCount {
    /// BOM quantity: positions, drawn and spare.
    pub fn qty(&self) -> usize {
        self.drawn + self.spare
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
    /// Terminals drawn with a label an earlier one has: (label, the second terminal); a level
    /// drawn twice (brief 036: `level-twice`).
    pub twice: Vec<(String, TerminalId)>,
    /// Brief 036: positions and their BOM part data per type, in catalogue order (types with
    /// positions only).
    pub types: Vec<TypeCount>,
    /// Positions whose levels are drawn by symbols of another type: (position, a terminal).
    pub mixed: Vec<(String, TerminalId)>,
    /// Links naming levels their positions lack: (link number, the items as written).
    pub level_missing: Vec<(u32, Vec<String>)>,
    /// Fuse positions with no rating.
    pub no_rating: usize,
    /// Each row's position index (brief 036).
    pos_of_row: Vec<usize>,
    /// The type word of each `mixed` terminal's own symbol.
    mixed_types: BTreeMap<TerminalId, &'static str>,
    /// Sheets of its terminal plan section and rows of its BOM (review of PR #55, finding 1):
    /// kept with the strip, so sheet counts never walk the rows.
    pub sheets: usize,
    pub bom_rows: usize,
    /// Brief 030: the accessories placed and the links (design 030 §7, §11.1).
    pub acc: crate::accessories::StripAcc,
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
        let q = |code: &str| self.types.iter().find(|t| t.ty.code == code).map_or(0, TypeCount::qty);
        (q("ft"), q("pe"))
    }

    /// The types whose drawn positions are more than the offer gives them (brief 036 §7.1).
    pub fn over_offer(&self) -> Vec<(&TypeCount, u32)> {
        let Some(o) = self.offer else { return Vec::new() };
        // 028's rule with no type codes: feed-through and PE counted together.
        if o.counts.iter().skip(2).all(|n| *n == 0) && self.types.iter().all(|t| t.ty.legacy()) {
            return match self.types.first() {
                Some(t) if self.drawn > o.positions as usize => vec![(t, o.positions)],
                _ => Vec::new(),
            };
        }
        self.types.iter().filter(|t| t.drawn > o.of(t.ty) as usize).map(|t| (t, o.of(t.ty))).collect()
    }

    /// Its Checks rows: (`terminal-twice` errors, `too-many-terminals` and `position-mixed`
    /// warnings).
    pub fn check_rows(&self) -> (usize, usize) {
        (self.twice.len(), self.over_offer().len() + self.mixed.len() + self.level_missing.len() + self.acc.warnings())
    }

    /// The highest drawn terminal (by label), for the too-many-terminals go-to.
    pub fn highest(&self) -> Option<&TermRow> {
        self.rows.iter().rev().find(|r| r.terminal.is_some())
    }

    /// The highest drawn row of type `t`.
    pub fn highest_of(&self, t: &crate::terminal_types::TermType) -> Option<&TermRow> {
        self.rows.iter().rev().find(|r| r.terminal.is_some() && r.ty == t)
    }

    /// The plan index of row `i`'s position (rows of one position share it).
    pub fn position_index(&self, i: usize) -> usize {
        self.pos_of_row.get(i).copied().unwrap_or(0)
    }

    /// The type word of the symbol that draws terminal `t` (`position-mixed`).
    pub fn other_type_word(&self, t: TerminalId) -> &'static str {
        self.mixed_types.get(&t).copied().unwrap_or("?")
    }

    /// Whether row `i` is the first row of its position.
    pub fn first_of_position(&self, i: usize) -> bool {
        i == 0 || self.rows.get(i - 1).zip(self.rows.get(i)).is_some_and(|(a, b)| a.label != b.label)
    }

    /// A drawn row by terminal.
    pub fn row_of(&self, t: TerminalId) -> Option<&TermRow> {
        self.rows.iter().find(|r| r.terminal == Some(t))
    }

    /// The plan in physical order (design 030 §7.1): `acc.start`'s items, each position followed
    /// by its `acc.after` items, then `acc.end`'s items. With `accessories` off, 028's rows only.
    pub fn plan(&self, accessories: bool) -> Vec<PlanItem<'_>> {
        let mut out = Vec::with_capacity(self.rows.len() + self.acc.gap_rows());
        fn gap<'a>(out: &mut Vec<PlanItem<'a>>, on: bool, g: &'a str, items: &[crate::accessories::Acc]) {
            if on {
                out.extend(items.iter().map(|a| PlanItem::Gap { gap: g, acc: *a }));
            }
        }
        gap(&mut out, accessories, "start", &self.acc.start);
        for (i, r) in self.rows.iter().enumerate() {
            out.push(PlanItem::Term(i, r));
            let last_of_label = self.rows.get(i + 1).is_none_or(|n| n.label != r.label);
            if last_of_label && let Some(items) = self.acc.after.get(&r.label) {
                gap(&mut out, accessories, &r.label, items);
            }
        }
        gap(&mut out, accessories, "end", &self.acc.end);
        out
    }

    /// The links through plan row `i`: (link, whether its level of its position is one the link
    /// names). Brief 036: a link runs on its own level; a cross-level link only through the
    /// levels of its position.
    pub fn links_at(&self, i: usize) -> Vec<(&crate::accessories::LinkInfo, bool)> {
        let Some(r) = self.rows.get(i) else { return Vec::new() };
        let pi = self.position_index(i);
        self.acc
            .links
            .iter()
            .filter(|l| l.span > 0 && l.first <= pi && pi <= l.last && l.runs_on(r.level))
            .map(|l| (l, l.labels.contains(&r.label) && l.names_level(r.level)))
            .collect()
    }

    /// The Bridge cell of a terminal row (design 030 §7.1): 028's `● n`, then one mark per link
    /// lane: `●` on a linked position, `┆` past a skipped one.
    pub fn bridge_cell(&self, i: usize) -> String {
        let mut text = self.rows.get(i).and_then(|r| r.bridge_group).map_or_else(String::new, |g| format!("\u{25cf} {g}"));
        let at = self.links_at(i);
        let lanes = at.iter().map(|(l, _)| l.lane + 1).max().unwrap_or(0);
        if lanes > 0 {
            let mut marks = vec![' '; lanes];
            for (l, on) in &at {
                if let Some(m) = marks.get_mut(l.lane) {
                    *m = if *on { '\u{25cf}' } else { '\u{2506}' };
                }
            }
            if !text.is_empty() {
                text.push(' ');
            }
            text.extend(marks);
        }
        text
    }
}

/// One row of a strip's plan in physical order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlanItem<'a> {
    /// An accessory in a gap: `start`, `end` or the label it follows.
    Gap { gap: &'a str, acc: crate::accessories::Acc },
    /// A position (its index in [`StripData::rows`]).
    Term(usize, &'a TermRow),
}

/// The strips of a project (see the module's update map).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Strips {
    pub strips: BTreeMap<ComponentId, StripData>,
    by_conductor: BTreeMap<ConductorId, BTreeSet<ComponentId>>,
    by_far: BTreeMap<ComponentId, BTreeSet<ComponentId>>,
    by_page: BTreeMap<PageId, BTreeSet<ComponentId>>,
    by_cable: BTreeMap<CableId, BTreeSet<ComponentId>>,
    /// Brief 030: the strips reading each project `strip_*` key (no value of their own).
    inherits: BTreeMap<String, BTreeSet<ComponentId>>,
    /// Brief 030: placed accessories with no part number, over every strip (one info row).
    pub acc_no_part: usize,
    /// Brief 036: brief 034's fuse terminal devices (the `fuse-device` Info row).
    pub fuse_devices: BTreeSet<ComponentId>,
    /// How many strips the last update recomputed (tests, bench).
    pub last_work: usize,
    /// Kept totals (review of PR #55, finding 1): every strip's plan sheets, and BOM rows per
    /// location.
    pub sheets_total: usize,
    pub bom_rows: BTreeMap<Option<String>, usize>,
    /// Kept Checks rows of the strips (brief 027's badge): `terminal-twice` errors and
    /// `too-many-terminals` warnings.
    pub check_errors: usize,
    pub check_warnings: usize,
    /// Bumped by an update that changed a strip, and the value at each strip's last change: what
    /// a copy of the plan (Reports ▸ Terminals) is keyed on (review of PR #55, finding 2).
    pub revision: u64,
    changed_at: BTreeMap<ComponentId, u64>,
    /// The strips whose data the last update changed (added, removed or different): what a shown
    /// terminal plan page depends on (tester 028, defect 2).
    pub last_changed: BTreeSet<ComponentId>,
}

fn add_to<K: Ord + Clone>(map: &mut BTreeMap<K, BTreeSet<ComponentId>>, keys: &BTreeSet<K>, c: ComponentId) {
    for k in keys {
        map.entry(k.clone()).or_default().insert(c);
    }
}

fn remove_from<K: Ord + Clone>(map: &mut BTreeMap<K, BTreeSet<ComponentId>>, keys: &BTreeSet<K>, c: ComponentId) {
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
    /// Brief 030: project `strip_*` keys that changed.
    pub project_keys: &'a BTreeSet<String>,
}

impl Strips {
    /// Every strip, from scratch.
    pub fn build(doc: &Document, o: &Outputs, cx: &PageCx) -> Strips {
        let mut s = Strips::default();
        for c in doc.components.keys() {
            if is_strip(doc, *c) {
                s.set(doc, *c, o, cx);
            }
            if crate::level_cmds::is_fuse_device(doc, *c) {
                s.fuse_devices.insert(*c);
            }
        }
        s
    }

    /// Brief 036: the strips that read project key `key` (no value of their own).
    pub fn inheriting(&self, key: &str) -> Vec<ComponentId> {
        self.inherits.get(key).map(|s| s.iter().copied().collect()).unwrap_or_default()
    }

    /// Brief 036: keep the fuse terminal devices among `devices` (the ops' devices) up to date.
    /// Returns whether the set changed.
    pub fn note_devices(&mut self, doc: &Document, devices: &BTreeSet<ComponentId>) -> bool {
        let mut changed = false;
        for c in devices {
            changed |= if crate::level_cmds::is_fuse_device(doc, *c) { self.fuse_devices.insert(*c) } else { self.fuse_devices.remove(c) };
        }
        changed
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
        // Brief 036: a project level separator or level names relabel the strips inheriting them.
        for k in scope.project_keys.iter().filter(|k| crate::defaults::terminals::is_label_key(k)) {
            out.extend(self.inherits.get(k).into_iter().flatten().copied());
        }
        out
    }

    /// Brief 030: the strips reading a changed project `strip_*` key (no value of their own), not
    /// already in `full`: only their accessories need recomputing.
    pub fn touched_acc(&self, project_keys: &BTreeSet<String>, full: &BTreeSet<ComponentId>) -> BTreeSet<ComponentId> {
        project_keys.iter().flat_map(|k| self.inherits.get(k).into_iter().flatten().copied()).filter(|c| !full.contains(c)).collect()
    }

    /// Recompute only the accessories of `which` (design 030 §11.1: a project default changes
    /// part data, never terminal rows). Adds to [`Strips::last_changed`] and `last_work`.
    pub fn recompute_acc(&mut self, doc: &Document, which: &BTreeSet<ComponentId>) {
        let mut changed = false;
        for c in which {
            let Some(old) = self.strips.get(c) else { continue };
            let Some(props) = doc.components.get(c).map(|x| &x.properties) else { continue };
            let mut acc = crate::accessories::strip_acc(doc, props, &old.rows);
            // Brief 036: a project part or description default reaches the type rows too.
            let mut d = old.clone();
            acc.inherits.extend(d.refresh_types(doc, props));
            // The level label keys a strip inherits are not this pass's (they rebuild the strip).
            acc.inherits.extend(old.acc.inherits.iter().filter(|k| crate::defaults::terminals::is_label_key(k)).cloned());
            if acc == old.acc && d.types == old.types {
                continue;
            }
            d.acc = acc;
            d.finish();
            self.remove(*c);
            self.insert(*c, d);
            self.last_changed.insert(*c);
            changed = true;
        }
        if changed {
            self.revision += 1;
            for c in which.iter().filter(|c| self.last_changed.contains(c)) {
                self.changed_at.insert(*c, self.revision);
            }
        }
        self.last_work += which.len();
    }

    /// Recompute `which` on `doc`.
    pub fn recompute(&mut self, doc: &Document, which: &BTreeSet<ComponentId>, o: &Outputs, cx: &PageCx) {
        self.last_changed.clear();
        for c in which {
            let before = self.strips.get(c).cloned();
            self.set(doc, *c, o, cx);
            if self.strips.get(c) != before.as_ref() {
                self.last_changed.insert(*c);
            }
        }
        if !self.last_changed.is_empty() {
            self.revision += 1;
            for c in &self.last_changed {
                self.changed_at.insert(*c, self.revision);
            }
        }
        self.last_work = which.len();
    }

    fn remove(&mut self, c: ComponentId) {
        let Some(old) = self.strips.remove(&c) else { return };
        self.sheets_total = self.sheets_total.saturating_sub(old.sheets);
        let (er, wa) = old.check_rows();
        self.check_errors = self.check_errors.saturating_sub(er);
        self.check_warnings = self.check_warnings.saturating_sub(wa);
        if let Some(n) = self.bom_rows.get_mut(&old.location) {
            *n = n.saturating_sub(old.bom_rows);
            if *n == 0 {
                self.bom_rows.remove(&old.location);
            }
        }
        remove_from(&mut self.by_conductor, &old.conductors, c);
        remove_from(&mut self.by_far, &old.far, c);
        remove_from(&mut self.by_page, &old.pages, c);
        remove_from(&mut self.by_cable, &old.cables, c);
        remove_from(&mut self.inherits, &old.acc.inherits, c);
        self.acc_no_part = self.acc_no_part.saturating_sub(old.acc.no_part);
    }

    fn set(&mut self, doc: &Document, c: ComponentId, o: &Outputs, cx: &PageCx) {
        self.remove(c);
        if !is_strip(doc, c) {
            return;
        }
        let d = strip_data(doc, c, o, cx);
        self.insert(c, d);
    }

    fn insert(&mut self, c: ComponentId, d: StripData) {
        add_to(&mut self.by_conductor, &d.conductors, c);
        add_to(&mut self.by_far, &d.far, c);
        add_to(&mut self.by_page, &d.pages, c);
        add_to(&mut self.by_cable, &d.cables, c);
        add_to(&mut self.inherits, &d.acc.inherits, c);
        self.acc_no_part += d.acc.no_part;
        self.sheets_total += d.sheets;
        let (er, wa) = d.check_rows();
        self.check_errors += er;
        self.check_warnings += wa;
        if d.bom_rows > 0 {
            *self.bom_rows.entry(d.location.clone()).or_default() += d.bom_rows;
        }
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
            && self.inherits == other.inherits
            && self.acc_no_part == other.acc_no_part
            && self.fuse_devices == other.fuse_devices
            && self.sheets_total == other.sheets_total
            && self.check_errors == other.check_errors
            && self.check_warnings == other.check_warnings
            && self.bom_rows == other.bom_rows
    }

    /// The revision at which strip `c` last changed (0: not since the build); with `None`, the
    /// revision of any strip's change.
    pub fn changed_at(&self, c: Option<ComponentId>) -> u64 {
        match c {
            Some(c) => self.changed_at.get(&c).copied().unwrap_or(0),
            None => self.revision,
        }
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
    use crate::terminal_types as tt;
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
        types: Vec::new(),
        mixed: Vec::new(),
        level_missing: Vec::new(),
        no_rating: 0,
        pos_of_row: Vec::new(),
        mixed_types: BTreeMap::new(),
        sheets: 1,
        bom_rows: 0,
        acc: crate::accessories::StripAcc::default(),
        conductors: BTreeSet::new(),
        far: BTreeSet::new(),
        pages: BTreeSet::new(),
        cables: BTreeSet::new(),
    };
    let drawn = drawn_at(doc, c);
    let shown_at: BTreeMap<TerminalId, (RepresentationId, bool, Option<Pt>)> = drawn.iter().map(|(t, rid, pe, at)| (*t, (*rid, *pe, *at))).collect();
    let mine: BTreeSet<TerminalId> = doc.terminals_of(c).map(|(t, _)| t).collect();
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
    let separator = tt::separator(doc, c);
    let mut names_of: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();
    let mut inherits: BTreeSet<String> = BTreeSet::new();
    let own = |k: &str| props.get(k).is_some_and(|v| !v.trim().is_empty());
    // The conductors on one terminal, sorted into inside and outside cells (§5.2; two-point levels
    // by their point, Q6).
    let mut cells = |t: TerminalId, side: Option<bool>, page: Option<PageId>, row: &mut TermRow, d: &mut StripData| {
        for (w, end) in doc.ends_at(t) {
            d.conductors.insert(w);
            let Some(cond) = doc.conductors.get(&w) else { continue };
            let far_end = match end {
                End::A => End::B,
                End::B => End::A,
            };
            let other = cond.end(far_end);
            if let Some(o2) = other
                && o2 != t
                && mine.contains(&o2)
            {
                // A bridge.
                bridged.insert(t);
                bridged.insert(o2);
                let (ra, rb) = (find(&mut parent, t), find(&mut parent, o2));
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
            let inside = match side {
                Some(s) => s,
                None if cell.cable.is_some() => false,
                None => {
                    let far_loc = far.filter(|f| crate::locations::is_device(doc, *f)).and_then(|f| match o.device(f) {
                        Some(x) => x.location.value.clone(),
                        None => crate::locations::resolve(doc, f).value,
                    });
                    match (&location, far_loc) {
                        (Some(a), Some(b)) => *a == b,
                        _ => page.and_then(|p| geometric_inside(doc, w, end, p)).unwrap_or(true),
                    }
                }
            };
            if inside {
                row.inside.push(cell);
            } else {
                row.outside.push(cell);
            }
        }
    };
    let positions = tt::positions(doc, c);
    let mut drawn_by_type: [usize; tt::TYPE_COUNT] = [0; tt::TYPE_COUNT];
    for pos in &positions {
        let ty = pos.ty;
        if let Some(n) = drawn_by_type.get_mut(tt::index_of(ty)) {
            *n += 1;
        }
        if let Some(t) = pos.mixed.first() {
            d.mixed.push((pos.label.clone(), *t));
            d.mixed_types.insert(*t, tt::type_of_terminal(doc, *t).word);
        }
        if ty.code == "fuse" && clean(props.get(&tt::rating_key(&pos.label))).is_none() {
            d.no_rating += 1;
        }
        let names = names_of.entry(ty.key).or_insert_with(|| tt::level_names(doc, c, ty)).clone();
        if ty.multi() {
            if !own(crate::defaults::terminals::SEPARATOR.key) {
                inherits.insert(crate::defaults::terminals::SEPARATOR.key.to_string());
            }
            if let Some(e) = crate::defaults::terminals::levels_entry(ty.code)
                && !own(e.key)
            {
                inherits.insert(e.key.to_string());
            }
        }
        let rating = ty.breaks.then(|| clean(props.get(&tt::rating_key(&pos.label)))).flatten();
        let mut seen_levels: BTreeSet<u32> = BTreeSet::new();
        // One row per level, free levels included; a level drawn twice adds a row (and a Checks row).
        let max_level = ty.count().max(pos.levels.keys().copied().max().unwrap_or(1));
        for n in 1..=max_level {
            let level_name = names.get(usize::try_from(n.saturating_sub(1)).unwrap_or(0)).cloned().unwrap_or_else(|| n.to_string());
            let derived = if ty.multi() { format!("{}{separator}{level_name}", pos.label) } else { pos.label.clone() };
            let over = clean(props.get(&tt::label_key(&pos.label, n)));
            let terms: Vec<TerminalId> = pos.levels.get(&n).cloned().unwrap_or_default();
            if terms.is_empty() {
                if n > ty.count() {
                    continue;
                }
                let mut r = TermRow::blank(pos.label.clone(), ty, n, level_name);
                r.label_overridden = over.is_some();
                r.shown = over.unwrap_or(derived);
                r.free = true;
                r.rating = rating.clone();
                rows.push(r);
                continue;
            }
            // Split into drawings: in/out pairs for a two-point level, one terminal otherwise.
            let mut drawings: Vec<(TerminalId, Option<TerminalId>)> = Vec::new();
            if ty.breaks {
                let ins: Vec<TerminalId> = terms.iter().copied().filter(|t| tt::level_of(doc, *t).1 != Some('o')).collect();
                let outs: Vec<TerminalId> = terms.iter().copied().filter(|t| tt::level_of(doc, *t).1 == Some('o')).collect();
                for (i, t) in ins.iter().enumerate() {
                    drawings.push((*t, outs.get(i).copied()));
                }
                for t in outs.iter().skip(ins.len()) {
                    drawings.push((*t, None));
                }
            } else {
                drawings.extend(terms.iter().map(|t| (*t, None)));
            }
            for (k, (t, out)) in drawings.into_iter().enumerate() {
                let (rid, pe, at) = match shown_at.get(&t).or_else(|| out.and_then(|x| shown_at.get(&x))) {
                    Some((rid, pe, at)) => (Some(*rid), *pe, *at),
                    None => (None, false, None),
                };
                let page = rid.and_then(|r| doc.representations.get(&r)).map(|r| r.page);
                if let Some(p) = page {
                    d.pages.insert(p);
                }
                let mut row = TermRow::blank(pos.label.clone(), ty, n, level_name.clone());
                row.terminal = Some(t);
                row.out = out;
                row.pe = pe || ty.pe_level == Some(n);
                row.label_overridden = over.is_some();
                row.shown = over.clone().unwrap_or_else(|| derived.clone());
                row.rating = rating.clone();
                row.internal_bridge = ty
                    .bridges
                    .iter()
                    .position(|g| g.iter().any(|key| ecad_model::links::level_of_key(Some(key)).is_some_and(|(m, _)| m == n)))
                    .map(|i| i + 1);
                row.place = page.zip(at).and_then(|(p, a)| {
                    let grid = grids.entry(p).or_insert_with(|| ecad_model::sheet::Grid::of(doc, p));
                    ecad_model::derive::place_on(grid, cx.number(p)?, a)
                });
                row.go = page.zip(at);
                if ty.breaks {
                    cells(t, Some(true), page, &mut row, &mut d);
                    if let Some(x) = out {
                        cells(x, Some(false), page, &mut row, &mut d);
                    }
                } else {
                    cells(t, None, page, &mut row, &mut d);
                }
                if k > 0 || !seen_levels.insert(n) {
                    d.twice.push((row.shown.clone(), t));
                }
                rows.push(row);
            }
        }
    }
    // Bridge groups, numbered in plan order.
    let mut groups: BTreeMap<TerminalId, usize> = BTreeMap::new();
    for r in &mut rows {
        let Some(t) = r.terminal else { continue };
        let t = if bridged.contains(&t) {
            t
        } else {
            match r.out.filter(|x| bridged.contains(x)) {
                Some(x) => x,
                None => continue,
            }
        };
        let root = find(&mut parent, t);
        let n = groups.len() + 1;
        r.bridge_group = Some(*groups.entry(root).or_insert(n));
    }
    d.drawn = positions.len();
    d.drawn_pe = drawn_by_type.get(1).copied().unwrap_or(0);
    // Spare positions (FP1): labels 1…n no drawn position has, given to the offer's types in its
    // order (feed-through first), each with its type's levels.
    let mut spare_by_type: [usize; tt::TYPE_COUNT] = [0; tt::TYPE_COUNT];
    if let Some(off) = d.offer {
        let taken: BTreeSet<&str> = positions.iter().map(|p| p.label.as_str()).collect();
        let mut free: std::collections::VecDeque<u32> = (1..=off.positions).filter(|n| !taken.contains(n.to_string().as_str())).collect();
        let legacy_only = off.counts.iter().skip(2).all(|n| *n == 0) && positions.iter().all(|p| p.ty.legacy());
        let mut wanted: Vec<(usize, usize)> = Vec::new();
        if legacy_only {
            // 028's rule: the PE spares follow the last drawn PE terminal; feed-through the rest.
            let pe_wanted = (off.pe as usize).saturating_sub(d.drawn_pe).min(free.len());
            let last_pe = positions.iter().filter(|p| p.ty.code == "pe").filter_map(|p| p.label.parse::<u32>().ok()).max().unwrap_or(0);
            let spare: Vec<u32> = free.iter().copied().collect();
            let mut pe_set: BTreeSet<u32> = spare.iter().copied().filter(|n| *n > last_pe).take(pe_wanted).collect();
            for n in spare.iter().rev() {
                if pe_set.len() >= pe_wanted {
                    break;
                }
                pe_set.insert(*n);
            }
            let (ft, pe) = (tt::feedthrough(), tt::by_code("pe").unwrap_or_else(tt::feedthrough));
            for n in &spare {
                let ty = if pe_set.contains(n) { pe } else { ft };
                rows.push(TermRow::blank(n.to_string(), ty, 1, "1".into()));
                if let Some(x) = spare_by_type.get_mut(tt::index_of(ty)) {
                    *x += 1;
                }
            }
            free.clear();
        } else {
            for (i, t) in tt::TYPES.iter().enumerate() {
                let drawn_n = drawn_by_type.get(i).copied().unwrap_or(0);
                wanted.push((i, (off.of(t) as usize).saturating_sub(drawn_n)));
            }
        }
        for (i, want) in wanted {
            let Some(ty) = tt::TYPES.get(i) else { continue };
            // Review 036, 3: spare multi-level positions read the separator and level names too.
            if want > 0 && ty.multi() {
                if !own(crate::defaults::terminals::SEPARATOR.key) {
                    inherits.insert(crate::defaults::terminals::SEPARATOR.key.to_string());
                }
                if let Some(e) = crate::defaults::terminals::levels_entry(ty.code)
                    && !own(e.key)
                {
                    inherits.insert(e.key.to_string());
                }
            }
            let names = names_of.entry(ty.key).or_insert_with(|| tt::level_names(doc, c, ty)).clone();
            for _ in 0..want {
                let Some(n) = free.pop_front() else { break };
                for lvl in 1..=ty.count() {
                    let name = names.get(usize::try_from(lvl - 1).unwrap_or(0)).cloned().unwrap_or_else(|| lvl.to_string());
                    let mut r = TermRow::blank(n.to_string(), ty, lvl, name.clone());
                    r.shown = if ty.multi() { format!("{n}{separator}{name}") } else { n.to_string() };
                    rows.push(r);
                }
                if let Some(x) = spare_by_type.get_mut(i) {
                    *x += 1;
                }
            }
        }
        d.spare = spare_by_type.iter().sum();
        d.spare_pe = spare_by_type.get(1).copied().unwrap_or(0);
        rows.sort_by_cached_key(|r| (crate::xref::natural_key(&r.label), r.level, r.terminal));
    }
    // Part data per type (§7.2): strip, then project, then the registry's description.
    for (i, ty) in tt::TYPES.iter().enumerate() {
        let (dn, sp) = (drawn_by_type.get(i).copied().unwrap_or(0), spare_by_type.get(i).copied().unwrap_or(0));
        if dn + sp == 0 {
            continue;
        }
        d.types.push(type_parts(doc, &props, ty, dn, sp, &mut inherits));
    }
    // Each row's position index.
    let mut n = 0usize;
    for (i, r) in rows.iter().enumerate() {
        if i > 0 && rows.get(i - 1).is_some_and(|p| p.label != r.label) {
            n += 1;
        }
        d.pos_of_row.push(n);
    }
    // Links naming levels their positions lack (`level-missing`, §8).
    let count_of: BTreeMap<&str, u32> = positions.iter().map(|p| (p.label.as_str(), p.ty.count())).collect();
    let order: Vec<String> = positions.iter().map(|p| p.label.clone()).collect();
    for (n, p) in ecad_model::links::of(&props) {
        let mut items = Vec::new();
        for item in &p.0 {
            let Some(sp) = item.span() else { continue };
            let missing = match &sp {
                ecad_model::links::Span::Levels { position, hi, .. } => count_of.get(position.as_str()).is_some_and(|c| hi > c),
                ecad_model::links::Span::Positions { level, .. } => {
                    let one = ecad_model::links::Positions(vec![item.clone()]);
                    one.select(&order).iter().any(|l| count_of.get(l.as_str()).is_some_and(|c| level > c))
                }
            };
            if missing {
                items.push(match item {
                    ecad_model::links::Item::One(x) => x.clone(),
                    ecad_model::links::Item::Range(a, b) => format!("{a}-{b}"),
                });
            }
        }
        if !items.is_empty() {
            d.level_missing.push((n, items));
        }
    }
    d.rows = rows;
    d.acc = crate::accessories::strip_acc(doc, &props, &d.rows);
    d.acc.inherits.extend(inherits);
    d.finish();
    d
}

/// The strip key of a type's part field (§7.2): feed-through keeps `part_number`, `maker`; every
/// other type `<field>.<code>` (`part_number.lnpe`; 028's `part_number.pe` is this rule).
pub fn type_key(t: &crate::terminal_types::TermType, field: &str) -> String {
    if t.code == "ft" { field.to_string() } else { format!("{field}.{}", t.code) }
}

/// One type's BOM part data on a strip with `props`: the strip's value, else the project's
/// `strip_<code>_<field>` (recorded in `inherits`), else the registry's description.
fn type_parts(
    doc: &Document, props: &ecad_model::Properties, ty: &'static crate::terminal_types::TermType, drawn: usize, spare: usize,
    inherits: &mut BTreeSet<String>,
) -> TypeCount {
    let mut get = |field: &str| -> Option<String> {
        if let Some(v) = clean(props.get(&type_key(ty, field))) {
            return Some(v);
        }
        let pk = format!("strip_{}_{field}", ty.code);
        inherits.insert(pk.clone());
        clean(doc.properties.get(&pk))
    };
    let part_number = get("part_number");
    let maker = get("maker");
    let own_description = get("description").or_else(|| if ty.code == "ft" { clean(props.get("description")) } else { None });
    let description = own_description.unwrap_or_else(|| {
        let e = crate::defaults::terminals::description_entry(ty.code);
        if let Some(e) = e {
            inherits.insert(e.key.to_string());
        }
        e.map_or_else(|| ty.name.to_string(), |e| crate::defaults::resolve(doc, e, crate::defaults::Scope::project()).text())
    });
    TypeCount { ty, drawn, spare, part_number, maker, description }
}

impl StripData {
    /// Its kept sheet and BOM row counts, from its rows and accessories.
    fn finish(&mut self) {
        self.sheets = (plan_lines(self) - 1).div_ceil(crate::generated::ROWS_PER_SHEET - 1).max(1);
        self.bom_rows = self.types.iter().filter(|t| t.qty() > 0).count() + self.acc.bom.len();
    }

    /// Recompute its part data per type (a project default changed, §11.1).
    fn refresh_types(&mut self, doc: &Document, props: &ecad_model::Properties) -> BTreeSet<String> {
        let mut inherits = BTreeSet::new();
        for t in &mut self.types {
            *t = type_parts(doc, props, t.ty, t.drawn, t.spare, &mut inherits);
        }
        inherits
    }
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
        // Brief 036: the type, and the level of a multi-level position.
        let ty = crate::terminal_types::type_of_terminal(doc, t);
        let kind = if ty.multi() {
            format!("{} level {}", ty.word, crate::terminal_types::level_name(doc, c, ty, crate::terminal_types::level_of(doc, t).0))
        } else if pe {
            "PE".to_string()
        } else {
            ty.word.to_string()
        };
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
    // Brief 036: counted in positions (a three-level terminal is one).
    let labels: BTreeSet<String> = doc.terminals_of(c).filter_map(|(_, t)| t.label.clone()).collect();
    let drawn = labels.len();
    let tag = crate::devices::tag_words(doc, c);
    lines.push(match offer_of(doc, c) {
        Some(o) => {
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
    1 + s.rows.iter().map(|r| r.inside.len().max(r.outside.len()).max(1)).sum::<usize>() + s.acc.gap_rows()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offers_read_and_refuse_with_a_way_forward() {
        let pos = |t: &str| parse_offer(t).map(|o| (o.positions, o.pe));
        assert_eq!(pos("12"), Ok((12, 0)));
        assert_eq!(pos(" 12 + 2pe "), Ok((14, 2)), "12 feed-through and 2 PE");
        assert_eq!(pos("0+2PE"), Ok((2, 2)));
        assert_eq!(pos("12+2PE+4x3"), Ok((18, 2)), "brief 036: type codes");
        assert!(parse_offer("990+10PE").is_err(), "999 in all");
        assert_eq!(offer_value(Some("12 + 2 PE")), Ok(Some("12+2PE".into())));
        assert_eq!(offer_value(Some("  ")), Ok(None));
        assert!(parse_offer("12X").unwrap_err().contains("Write 12, 12+2PE or 12+4x3"));
        assert!(parse_offer("1000").is_err());
        assert!(parse_offer("0").is_err());
        assert_eq!(parse_offer("2+3PE").map(Offer::text), Ok("2+3PE".into()));
        assert!(parse_offer(&"9".repeat(10_000)).is_err());
    }

    #[test]
    fn core_offers_from_cores_and_from_type() {
        assert_eq!(parse_cores("3").unwrap(), ["1", "2", "3"]);
        assert_eq!(parse_cores("3g").unwrap(), ["1", "2", GNYE]);
        assert_eq!(parse_cores("3X").unwrap(), ["1", "2", "3"]);
        assert_eq!(parse_cores("BN, BK ,GNYE").unwrap(), ["BN", "BK", "GNYE"]);
        assert!(parse_cores("7Q").unwrap_err().contains("Write 7, 7G, 5X, 4x2x0.25 (pairs)"));
        assert!(parse_cores("1000").is_err());
        assert!(parse_cores("BN,BN").is_err());
        assert!(parse_cores("BN,,BK").is_err());
        assert_eq!(cores_value(Some("7g")), Ok(Some("7G".into())));
        assert_eq!(offer_from_type("YSLY-JZ 7G1.5").as_deref(), Some("7G"));
        assert_eq!(offer_from_type("H07RN-F"), None);
        assert_eq!(offer_from_type("12G"), None);
    }

    /// Owner decisions 2026-11-12: the core notations of a cable type.
    #[test]
    fn cable_type_notations() {
        let cores = |t: &str| {
            let n = type_notation(t).unwrap_or_else(|| panic!("no count in {t}"));
            (n.cores(), n.labels().iter().filter(|l| *l == GNYE).count(), n.group)
        };
        assert_eq!(cores("12G0.5"), (12, 1, 1));
        assert_eq!(cores("12X0.5"), (12, 0, 1));
        assert_eq!(cores("4(2x0.25)"), (8, 0, 2), "4 pairs, never 2 cores");
        assert_eq!(cores("4x2x0.25"), (8, 0, 2));
        assert_eq!(cores("4 x 2 x 0.25"), (8, 0, 2));
        assert_eq!(cores("3x1.5"), (3, 0, 1));
        assert_eq!(cores("3x2.5"), (3, 0, 1), "2.5 mm² is no pair");
        assert_eq!(cores("LiYCY (TP) 4x2x0.25"), (8, 0, 2));
        assert_eq!(cores("YSLY-JZ 7G1.5"), (7, 1, 1));
        assert_eq!(cores("YSLY-OZ 7X1.5"), (7, 0, 1));
        assert_eq!(cores("YSLY-JZ 7x1.5"), (7, 1, 1), "-JZ means G");
        assert_eq!(cores("YSLY-OZ 7x1.5"), (7, 0, 1), "-OZ means X");
        assert_eq!(cores("LiYY 3(3x0.5)"), (9, 0, 3));
        let pairs = type_notation("4(2x0.25)").map(Notation::labels).unwrap_or_default();
        assert_eq!(pairs.get(..3), Some(&["1a".to_string(), "1b".to_string(), "2a".to_string()][..]));
        assert_eq!(offer_from_type("4x2x0.25").as_deref(), Some("4x2"));
        assert_eq!(parse_cores("4(2x0.25)").map(|v| v.len()), Ok(8));
        assert_eq!(cores_value(Some(" 4(2x0.25) ")), Ok(Some("4(2x0.25)".into())));
        assert!(type_notation(&"9".repeat(10_000)).is_none());
        assert!(type_notation("60x2x0.5").is_none(), "120 cores is past the cap");
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
