//! Terminal strips and cable cores: the terminal plan per strip
//! and the core count per cable, derived from the drawing and never stored.
//!
//! # Semantics
//! - A **strip** is a device whose definition is a terminal starter (`connection_point`, `terminal_block_pe`);
//!   each terminal is one symbol with one connection point. Its
//!   **type** is its symbol's: `terminal_block_pe` is PE, anything else feed-through.
//! - The strip's **offer** is its `connection_points` property, `12` or `12+2PE`: feed-through positions,
//!   then PE positions (`12+2PE` is 14 positions; a
//!   BOM of 12 feed-through and 2 PE). Positions with no drawn terminal are **spare**.
//! - A conductor whose two ends are on two terminals of one strip is a **bridge**. Every other
//!   conductor on a terminal is **inside** or **outside** by this rule: a cable core is outside;
//!   else the far device's location against the strip's; else the route's direction at the
//!   terminal (up or left is inside).
//! - A cable's **core offer** is its `cores` property, else read from its `type` (`7G1.5` → `7G`).
//!   Members are matched to it by label first, then in member order: **used**,
//!   **spare** and **over**.
//! - Nothing here is refused because of a check; it is flagged.
//! - A strip's rows are its **levels**, grouped by position in plan order
//!   ([`crate::terminal_strips::terminal_blocks::positions`]); a position whose type has more levels than are drawn
//!   shows them as **free** rows; spare positions come per type from the offer (`12+2PE+4x3`), each
//!   with its type's levels. Counts (`drawn`, `spare`) and BOM quantities are in positions.
//!
//! # Update map
//! [`Strips`] lives inside `crate::reports::kept::ReportModels` and is updated with it from the same
//! [`crate::reports::Scope`], after the device, wire and cable rows:
//!
//! | In scope (from the ops) | Recomputed |
//! | --- | --- |
//! | a device that is a strip (its create, delete, terminals, symbols, items, properties) | that strip |
//! | a device or tagless component (tag, location, continuation name) | the strips whose rows end on it (`Strips::far`) |
//! | a conductor (create, delete, attach, route, properties) and its net mates (wire numbers) | the strips it was on (kept) and is on now |
//! | a cable (create, delete, members, properties) | the strips its members were on (kept) and are on now |
//! | a page whose number or position changed (`SetProperty` number, book order, the sheet) | the strips with terminals on it (their Place) |
//! | a strip's `term_label.*`, `terminal.*`, `part_number.<code>` (its own properties), and its fuse position ratings (its `ModifyDevice`): a device in scope | that strip |
//! | project `terminal_block.level.separator` or `terminal_block.<code>.levels` | the strips inheriting it (`inherits`, recorded only by strips with a multi-level position): rebuilt |
//! | project `strip_<code>_*` or `terminal_block.<code>.description` | the strips inheriting it: their type part data and accessories only ([`Strips::recompute_acc`]) |
//! | a fuse terminal device created, changed or deleted | the `fuse-device` set ([`Strips::note_devices`]) |
//! | anything else | nothing |
//!
//! A full build happens only with the report models: open, reload, first show.

pub mod accessories;
pub mod commands;
pub mod cores;
pub mod hover;
pub mod jumper_bars;
pub mod pages;
pub mod strip_model;
pub mod terminal_blocks;

use std::collections::BTreeSet;

use ecad_model::{ConnectionPointId, DeviceId, Document, End, PageId, Pt, RepresentationId, Subject, WireId};

use crate::CmdError;
use cores::*;
use strip_model::{StripData, TermRow};

/// The strip property that offers positions.
pub(crate) const OFFER_KEY: &str = "terminal_blocks";
/// The cable property that offers cores.
pub(crate) const CORES_KEY: &str = "cores";
/// The strip properties of its PE terminals' BOM row.
pub(crate) const PE_PART_KEY: &str = "part_number.pe";
pub(crate) const PE_MAKER_KEY: &str = "maker.pe";
/// Most positions a strip offers.
pub(crate) const MAX_POSITIONS: u32 = 999;
/// Most cores a cable offers.
pub const MAX_CORES: usize = 100;
/// The green-yellow core of an IEC `G` cable.
pub const GNYE: &str = "GNYE";
/// The PE starter's key.
pub(crate) const PE_KEY: crate::symbols::StarterKey = crate::symbols::StarterKey::TerminalBlockPe;

// ---------------------------------------------------------------- strips and terminals

/// Strip `c`'s own data (its type's), when it is a terminal strip.
pub fn strip_of(doc: &Document, c: DeviceId) -> Option<&ecad_model::objects::devices::TerminalStrip> {
    doc.devices().get(&c)?.device_type.as_strip()
}

/// Whether representation `r` draws a PE terminal.
pub(crate) fn rep_is_pe(doc: &Document, r: &ecad_model::Representation) -> bool {
    crate::devices::tags::rep_starter(doc, r).is_some_and(|s| s.key == PE_KEY)
}

/// A strip's drawn terminals: (terminal, its representation, whether PE).
pub fn drawn_connection_points(doc: &Document, c: DeviceId) -> Vec<(ConnectionPointId, RepresentationId, bool)> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for (rid, r) in doc.representations_of(Subject::Device(c)) {
        let pe = rep_is_pe(doc, r);
        for (t, _) in crate::devices::merge::shown_connection_points(doc, c, r) {
            if seen.insert(t) {
                out.push((t, rid, pe));
            }
        }
    }
    out
}

/// [`drawn_connection_points`] with where each sits on its page, worked out from its own representation
/// (O(the strip's symbols), never a search per terminal).
fn drawn_at(doc: &Document, c: DeviceId) -> Vec<(ConnectionPointId, RepresentationId, bool, Option<Pt>)> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for (rid, r) in doc.representations_of(Subject::Device(c)) {
        let pe = rep_is_pe(doc, r);
        let placement = crate::devices::tags::rep_definition(doc, r).map(|(_, pl)| pl);
        for (t, p) in crate::devices::merge::shown_connection_points(doc, c, r) {
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

/// A strip's drawn terminals with the page and point each is drawn at (for jumper bar lines).
pub(crate) fn drawn_points(doc: &Document, c: DeviceId) -> Vec<(ConnectionPointId, RepresentationId, PageId, Pt)> {
    drawn_at(doc, c).into_iter().filter_map(|(t, rid, _, at)| Some((t, rid, doc.representations().get(&rid)?.page, at?))).collect()
}

/// The terminal of a strip a conductor end is on: (strip, terminal).
pub fn strip_of_connection_point(doc: &Document, t: ConnectionPointId) -> Option<DeviceId> {
    let c = doc.connection_points().get(&t)?.device()?;
    strip_of(doc, c).map(|_| c)
}

/// The strip a conductor bridges, when its two ends are on two terminals of one strip.
pub(crate) fn bridge_of(doc: &Document, w: WireId) -> Option<DeviceId> {
    let cond = doc.wires().get(&w)?;
    let (a, b) = (cond.end(End::A), cond.end(End::B));
    if a == b {
        return None;
    }
    let (sa, sb) = (strip_of_connection_point(doc, a)?, strip_of_connection_point(doc, b)?);
    (sa == sb).then_some(sa)
}

/// A strip's offer: positions per terminal type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Offer {
    /// Every position, of every type (`12+2PE+4x3`: 18).
    pub positions: u32,
    /// How many of them are PE.
    pub pe: u32,
    /// Positions per type, in [`crate::terminal_strips::terminal_blocks::TYPES`] order.
    pub counts: crate::terminal_strips::terminal_blocks::Counts,
}

impl Offer {
    /// As stored: `12`, `12+2PE`, `12+2PE+4x3` (catalogue order).
    pub fn text(self) -> String {
        crate::terminal_strips::terminal_blocks::counts_text(&self.counts)
    }

    /// Positions offered of type `t`.
    pub fn of(&self, t: &crate::terminal_strips::terminal_blocks::TermType) -> u32 {
        self.counts.get(crate::terminal_strips::terminal_blocks::index_of(t)).copied().unwrap_or(0)
    }
}

/// Read an offer: `+`-separated `<count><code>` items (`12+2PE+4x3`), spaces ignored, codes
/// case-insensitive.
///
/// # Errors
/// Unreadable text, an unknown code, no positions, or more than [`MAX_POSITIONS`] in all, in
/// words with the way forward.
pub fn parse_offer(text: &str) -> Result<Offer, CmdError> {
    let counts = crate::terminal_strips::terminal_blocks::parse_counts(text)?;
    let positions = counts.iter().fold(0u32, |a, b| a.saturating_add(*b));
    Ok(Offer { positions, pe: counts.get(1).copied().unwrap_or(0), counts })
}

/// The `connection_points` value as stored (canonical `12+2PE`); empty clears it.
///
/// # Errors
/// [`parse_offer`]'s.
pub fn offer_value(value: Option<&str>) -> Result<Option<String>, CmdError> {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        None => Ok(None),
        Some(v) => Ok(Some(parse_offer(v)?.text())),
    }
}

/// The offer Checks' **Offer drawn** writes: every type at
/// least its drawn positions; with no type codes, the plain rule (every drawn terminal, PE kept).
pub fn offer_drawn_value(doc: &Document, c: DeviceId) -> String {
    use crate::terminal_strips::terminal_blocks as tt;
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
pub fn offer_of(doc: &Document, c: DeviceId) -> Option<Offer> {
    doc.devices().get(&c)?.properties.get(OFFER_KEY).and_then(|v| parse_offer(v).ok())
}

/// The rows a generated `connection_points` sheet needs for `strip` (`None`: every strip): one heading per
/// strip and one per position's conductor line (at least one).
pub(crate) fn plan_lines(s: &StripData) -> usize {
    1 + s.rows.iter().map(|r| r.inside.len().max(r.outside.len()).max(1)).sum::<usize>() + s.acc.gap_rows()
}

/// A core label as given: trimmed; empty or only spaces clears it (`None`).
///
/// # Errors
/// Longer than [`crate::cables::words::MAX_CORE_CHARS`] characters, or a control character, in words.
pub(crate) fn core_label(text: Option<&str>) -> Result<Option<String>, CmdError> {
    let Some(t) = text.map(str::trim).filter(|t| !t.is_empty()) else { return Ok(None) };
    if t.chars().any(char::is_control) {
        return Err(CmdError::refused("A core label cannot contain control characters."));
    }
    if t.chars().count() > crate::cables::words::MAX_CORE_CHARS {
        return Err(CmdError::refused(format!("A core label can be at most {} characters.", crate::cables::words::MAX_CORE_CHARS)));
    }
    Ok(Some(t.to_string()))
}

/// `-X01:3`.
pub fn connection_point_words(doc: &Document, t: ConnectionPointId) -> String {
    let c = doc.connection_points().get(&t).and_then(|x| x.device());
    // A connector pin is named by its half (`-X10.S:3`).
    if let Some(name) = crate::connectors::pin_name(doc, t) {
        return name;
    }
    let tag = c.and_then(|c| crate::devices::tags::tag_shown_of(doc, c)).unwrap_or_else(|| "?".into());
    format!("{tag}:{}", crate::terminal_strips::terminal_blocks::connection_point_name(doc, t))
}

/// Fuse terminal device `c`'s fuse (a `fuseterminalblock`): its stored
/// [`ecad_model::objects::devices::Protection::FuseTerminalBlock`].
pub fn fuse_terminal_block_of(doc: &Document, c: DeviceId) -> Option<&ecad_model::objects::devices::Fuse> {
    doc.devices().get(&c)?.device_type.as_protection()?.as_fuse_terminal_block()
}

/// Level stagger: level *n* is raised by (*n* − 1) × this.
pub(crate) const STAGGER_MM: f64 = 1.5;

/// How far level `r` is raised.
pub fn raise(r: &TermRow) -> f64 {
    if r.ty.multi() { f64::from(r.level.saturating_sub(1).min(16)) * STAGGER_MM } else { 0.0 }
}

/// Whether `r` is the first plan row of its position (a marker is drawn once per position).
pub fn first_of_label(s: &StripData, r: &TermRow) -> bool {
    s.rows.iter().find(|x| x.label == r.label).is_some_and(|x| std::ptr::eq(x, r))
}
