//! Racks, slots and card types (design 045 §2, §3): the type catalogue, the card properties
//! `plc.rack` and `plc.slot` (FP1), and the commands `plcslot` and `plcracks`.
//!
//! # Semantics
//! - A **rack** is a device whose head is a rack body (`plccpu`, `plcim`); a **card** is a device
//!   whose head is any other card body. There is no new object.
//! - A card is in a slot when it has `plc.rack` (the rack device's ID, 32 hex digits) and
//!   `plc.slot` (0–999, digits only). Everything shown names the rack by its **tag**; only the file
//!   holds the ID, so retagging or renumbering a rack changes nothing on its cards.
//! - A card property that is not one of these forms, or names something that is not a rack, is a
//!   Checks row ([`crate::plc::Plc::checks`]), never a refusal on open and never a crash.
//!
//! Every write is one transaction, committed through [`crate::project::Project::apply_edit`].

use std::collections::BTreeSet;

use cadcraft_engine::Session;
use ecad_model::{ComponentId, Document, ObjectRef, Op, Subject};
use serde_json::{Value, json};

use crate::commands::{err, opt_typed, with_any_page, with_project};
use crate::defaults::{self, Scope};

/// The card property naming the card's rack (its component ID, 32 hex digits; FP1).
pub const RACK_KEY: &str = "plc.rack";
/// The card property holding the card's slot number (FP1).
pub const SLOT_KEY: &str = "plc.slot";
/// The card property holding the card's channel count (design 045 §2.3).
pub const CHANNELS_KEY: &str = "plc.channels";
/// The highest slot number read; a larger value is not a slot (hostile-input bound, §5.5).
pub const MAX_SLOT: u32 = 999;
/// The most channels a card has (hostile-input bound, §5.5).
pub const MAX_CHANNELS: u32 = 256;

/// A card type (design 045 §3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CardType {
    Cpu,
    Im,
    Di,
    Dq,
    Ai,
    Ao,
    Rtd,
    Iol,
    Ps,
    Pm,
    Cm,
}

/// Which way a card's channels go (design 045 §3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    None,
    Input,
    Output,
    Both,
}

impl CardType {
    pub const ALL: [CardType; 11] = [
        CardType::Cpu,
        CardType::Im,
        CardType::Di,
        CardType::Dq,
        CardType::Ai,
        CardType::Ao,
        CardType::Rtd,
        CardType::Iol,
        CardType::Ps,
        CardType::Pm,
        CardType::Cm,
    ];

    /// The type as written (`DI`, `CPU`).
    pub fn code(self) -> &'static str {
        match self {
            CardType::Cpu => "CPU",
            CardType::Im => "IM",
            CardType::Di => "DI",
            CardType::Dq => "DQ",
            CardType::Ai => "AI",
            CardType::Ao => "AO",
            CardType::Rtd => "RTD",
            CardType::Iol => "IOL",
            CardType::Ps => "PS",
            CardType::Pm => "PM",
            CardType::Cm => "CM",
        }
    }

    /// The type a typed word names (`di`, `DQ`, `iol`…), ignoring case.
    pub fn parse(word: &str) -> Option<CardType> {
        let w = word.trim();
        CardType::ALL.into_iter().find(|t| t.code().eq_ignore_ascii_case(w) || (w.eq_ignore_ascii_case("do") && *t == CardType::Dq))
    }

    /// Its card body's starter key.
    pub fn body(self) -> &'static str {
        match self {
            CardType::Cpu => "plccpu",
            CardType::Im => "plcim",
            CardType::Di => "plcdi",
            CardType::Dq => "plcdq",
            CardType::Ai => "plcai",
            CardType::Ao => "plcao",
            CardType::Rtd => "plcrtd",
            CardType::Iol => "plciol",
            CardType::Ps => "plcps",
            CardType::Pm => "plcpm",
            CardType::Cm => "plccm",
        }
    }

    /// The type whose body is starter `key`.
    pub fn of_body(key: &str) -> Option<CardType> {
        CardType::ALL.into_iter().find(|t| t.body() == key)
    }

    /// Whether a device of this type is a rack (a CPU or a head module).
    pub fn is_rack(self) -> bool {
        matches!(self, CardType::Cpu | CardType::Im)
    }

    /// Its direction.
    pub fn direction(self) -> Direction {
        match self {
            CardType::Di | CardType::Ai | CardType::Rtd => Direction::Input,
            CardType::Dq | CardType::Ao => Direction::Output,
            CardType::Iol => Direction::Both,
            _ => Direction::None,
        }
    }

    /// The roles of one channel, in the cell's order (design 045 §4.1); empty for a type without
    /// channels.
    pub fn roles(self) -> &'static [&'static str] {
        match self {
            CardType::Di => &["in"],
            CardType::Dq => &["out"],
            CardType::Ai | CardType::Ao => &["+", "-"],
            CardType::Rtd => &["I+", "S+", "S-", "I-"],
            CardType::Iol => &["L+", "C/Q", "L-"],
            _ => &[],
        }
    }

    /// Its channel cell's starter key (design 045 §4.1); empty for a type without channels.
    pub fn cell(self) -> &'static str {
        match self {
            CardType::Di => "chdi",
            CardType::Dq => "chdq",
            CardType::Ai => "chai",
            CardType::Ao => "chao",
            CardType::Rtd => "chrtd",
            CardType::Iol => "chiol",
            _ => "",
        }
    }

    /// Whether it has channels.
    pub fn has_channels(self) -> bool {
        !self.roles().is_empty()
    }

    /// The registry entry of its built-in channel count (`plc.<type>.channels`).
    pub fn channels_entry(self) -> Option<&'static defaults::Entry> {
        defaults::plc::channels_entry(&self.code().to_ascii_lowercase())
    }

    /// The noun in sentences: `DI card`, `CPU`, `head module`.
    pub fn noun(self) -> &'static str {
        match self {
            CardType::Cpu => "CPU",
            CardType::Im => "head module",
            CardType::Di => "DI card",
            CardType::Dq => "DQ card",
            CardType::Ai => "AI card",
            CardType::Ao => "AO card",
            CardType::Rtd => "RTD card",
            CardType::Iol => "IO-Link master",
            CardType::Ps => "power supply",
            CardType::Pm => "power module",
            CardType::Cm => "communication module",
        }
    }
}

/// The type of device `c` from its card body (its head), if it has one.
pub fn body_type(doc: &Document, c: ComponentId) -> Option<CardType> {
    doc.representations_of(Subject::Component(c)).filter_map(|(_, r)| crate::devices::rep_starter(doc, r)).find_map(|s| CardType::of_body(s.key))
}

/// Whether device `c` is a rack.
pub fn is_rack(doc: &Document, c: ComponentId) -> bool {
    body_type(doc, c).is_some_and(CardType::is_rack)
}

/// A slot number as stored: digits only, 0–[`MAX_SLOT`].
pub fn parse_slot(text: &str) -> Option<u32> {
    let t = text.trim();
    (!t.is_empty() && t.len() <= 3 && t.bytes().all(|b| b.is_ascii_digit())).then(|| t.parse::<u32>().ok()).flatten().filter(|n| *n <= MAX_SLOT)
}

/// A channel count as stored: digits only, 0–[`MAX_CHANNELS`].
pub fn parse_channels(text: &str) -> Option<u32> {
    let t = text.trim();
    (!t.is_empty() && t.len() <= 3 && t.bytes().all(|b| b.is_ascii_digit())).then(|| t.parse::<u32>().ok()).flatten().filter(|n| *n <= MAX_CHANNELS)
}

/// What a card's `plc.rack` names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RackRef {
    /// No `plc.rack`.
    None,
    /// A rack of this project.
    Rack(ComponentId),
    /// An ID that is not a rack of this project (deleted, never one, or not a rack device).
    Missing(ComponentId),
    /// A value that is not an ID.
    Bad,
}

/// What card `c`'s `plc.rack` names.
pub fn rack_ref(doc: &Document, c: ComponentId) -> RackRef {
    let Some(v) = doc.components.get(&c).and_then(|x| x.properties.get(RACK_KEY)) else { return RackRef::None };
    let t = v.trim();
    if t.len() != 32 {
        return RackRef::Bad;
    }
    match ComponentId::parse(t) {
        Some(r) if r != c && is_rack(doc, r) => RackRef::Rack(r),
        Some(r) => RackRef::Missing(r),
        None => RackRef::Bad,
    }
}

/// Card `c`'s slot, when `plc.slot` holds one.
pub fn slot_of(doc: &Document, c: ComponentId) -> Option<u32> {
    doc.components.get(&c).and_then(|x| x.properties.get(SLOT_KEY)).and_then(|v| parse_slot(v))
}

/// A rack's slot range: its first slot and how many (registry, rack level allowed).
pub fn slot_range(doc: &Document, rack: ComponentId) -> (u32, u32) {
    let at = Scope { component: Some(rack), ..Scope::project() };
    let n = defaults::resolve(doc, &defaults::plc::SLOTS, at).number();
    let first = defaults::resolve(doc, &defaults::plc::SLOT_FIRST, at).word();
    // The registry only returns values in range; the casts are of 1–999 and 0/1.
    let n = if n.is_finite() && n >= 1.0 { (n as u32).min(MAX_SLOT) } else { 32 };
    (if first == "0" { 0 } else { 1 }, n)
}

/// Card `c`'s channel count: its own `plc.channels`, else its type's registry value.
pub fn channel_count(doc: &Document, c: ComponentId, ty: CardType) -> u32 {
    if let Some(n) = doc.components.get(&c).and_then(|x| x.properties.get(CHANNELS_KEY)).and_then(|v| parse_channels(v)) {
        return n;
    }
    let Some(e) = ty.channels_entry() else { return 0 };
    let n = defaults::resolve(doc, e, Scope::project()).number();
    if n.is_finite() && n >= 0.0 { (n as u32).min(MAX_CHANNELS) } else { 0 }
}

/// The racks of the project, by tag (the order every list uses). Reads every component's head;
/// for prompts and commands, not per frame.
pub fn racks(doc: &Document) -> Vec<ComponentId> {
    let mut v: Vec<(String, ComponentId)> =
        doc.tagged().map(|(_, c)| c).filter(|c| is_rack(doc, *c)).map(|c| (crate::devices::tag_words(doc, c), c)).collect();
    v.sort();
    v.dedup_by_key(|x| x.1);
    v.into_iter().map(|(_, c)| c).collect()
}

/// The cards whose `plc.rack` names `rack` (valid or not), with their slots.
pub fn cards_naming(doc: &Document, rack: ComponentId) -> Vec<(ComponentId, Option<u32>)> {
    let hex = rack.hex();
    let mut v: Vec<(ComponentId, Option<u32>)> = doc
        .components
        .iter()
        .filter(|(_, x)| x.properties.get(RACK_KEY).is_some_and(|v| v.trim().eq_ignore_ascii_case(&hex)))
        .map(|(c, _)| (*c, slot_of(doc, *c)))
        .collect();
    v.sort_by_key(|(c, s)| (*s, crate::devices::tag_words(doc, *c)));
    v
}

/// Which card of `rack` (other than `except`) holds `slot`.
pub fn holder_of_slot(doc: &Document, rack: ComponentId, slot: u32, except: Option<ComponentId>) -> Option<ComponentId> {
    cards_naming(doc, rack).into_iter().find(|(c, s)| *s == Some(slot) && Some(*c) != except).map(|(c, _)| c)
}

/// The lowest free slot of `rack`, `except` not counting as holding one.
pub fn next_free_slot(doc: &Document, rack: ComponentId, except: Option<ComponentId>) -> Option<u32> {
    let taken: BTreeSet<u32> = cards_naming(doc, rack).into_iter().filter(|(c, _)| Some(*c) != except).filter_map(|(_, s)| s).collect();
    let (first, n) = slot_range(doc, rack);
    (first..first.saturating_add(n)).find(|s| !taken.contains(s))
}

/// `DI 16`: a card's type and channel count.
pub fn type_words(doc: &Document, c: ComponentId) -> String {
    match body_type(doc, c).or_else(|| crate::plc::card_type_of(doc, c)) {
        Some(t) if t.has_channels() => format!("{} {}", t.code(), channel_count(doc, c, t)),
        Some(t) => t.code().to_string(),
        None => "?".into(),
    }
}

/// The refusal of a taken slot, with the way out (design 045 §2.4).
pub fn taken_words(doc: &Document, rack: ComponentId, slot: u32, holder: ComponentId) -> String {
    // The card's own slot counts as taken: Enter offers a slot it is not in.
    let next = next_free_slot(doc, rack, None).map_or_else(|| "none is free".to_string(), |s| s.to_string());
    format!(
        "Slot {slot} of {} holds {} ({}). Pick another slot, or Enter for the next free one ({next}).",
        crate::devices::tag_words(doc, rack),
        crate::devices::tag_words(doc, holder),
        type_words(doc, holder)
    )
}

/// The ops putting `card` in `rack` at `slot` (or taking it out with `None`), only what changes.
pub fn slot_ops(doc: &Document, card: ComponentId, to: Option<(ComponentId, u32)>) -> Vec<Op> {
    let props = doc.components.get(&card).map(|c| &c.properties);
    let before = |k: &str| props.and_then(|p| p.get(k)).cloned();
    let (r, s) = match to {
        Some((r, s)) => (Some(r.hex()), Some(s.to_string())),
        None => (None, None),
    };
    let mut ops = Vec::new();
    for (key, after) in [(RACK_KEY, r), (SLOT_KEY, s)] {
        let b = before(key);
        if b != after {
            ops.push(Op::SetProperty { object: ObjectRef::Component(card), key: key.into(), before: b, after });
        }
    }
    ops
}

/// What deleting rack `rack` adds (design 045 §2.2, Q1, FP1 condition 2): `plc.rack` and
/// `plc.slot` removed from every card naming it, and how many cards that is. Nothing when `rack` is
/// not named by any card.
pub fn release_cards(doc: &Document, rack: ComponentId) -> (Vec<Op>, usize) {
    let cards = cards_naming(doc, rack);
    let mut ops = Vec::new();
    for (c, _) in &cards {
        ops.extend(slot_ops(doc, *c, None));
    }
    (ops, cards.len())
}

/// `6 cards are now in no rack.`
pub fn released_words(n: usize) -> String {
    if n == 1 { "1 card is now in no rack.".into() } else { format!("{n} cards are now in no rack.") }
}

/// A rack named by `v`: a hex ID or a tag.
///
/// # Errors
/// Nothing of that name, or it is not a rack.
pub fn rack_param(doc: &Document, v: &Value) -> Result<ComponentId, String> {
    let t = v.as_str().ok_or("`rack` must be a rack's ID or tag, or null")?.trim();
    let c = match ComponentId::parse(t).filter(|c| t.len() == 32 && doc.components.contains_key(c)) {
        Some(c) => c,
        None => {
            let tag = ecad_model::tag::normalise(t, crate::defaults::print::tag_style(doc)).unwrap_or_else(|_| t.to_string());
            *doc.components_tagged(&tag).first().ok_or_else(|| format!("No device {tag}."))?
        }
    };
    if !is_rack(doc, c) {
        return Err(format!("{} is not a rack; a rack is a CPU or a head module.", crate::devices::tag_words(doc, c)));
    }
    Ok(c)
}

/// A card named by `v`: a hex ID or a tag, of a device that is a card (not a rack).
///
/// # Errors
/// Nothing of that name, a rack, or a device that is not a PLC card.
pub fn card_param(doc: &Document, v: Option<&Value>) -> Result<ComponentId, String> {
    let t = v.and_then(Value::as_str).ok_or("`card` (a card's ID or tag) is required")?.trim();
    let c = match ComponentId::parse(t).filter(|c| t.len() == 32 && doc.components.contains_key(c)) {
        Some(c) => c,
        None => {
            let tag = ecad_model::tag::normalise(t, crate::defaults::print::tag_style(doc)).unwrap_or_else(|_| t.to_string());
            *doc.components_tagged(&tag).first().ok_or_else(|| format!("No device {tag}."))?
        }
    };
    match crate::plc::card_type_of(doc, c) {
        Some(t) if t.is_rack() => Err(format!("{} is a {}: it is a rack, not a card in a slot.", crate::devices::tag_words(doc, c), t.noun())),
        Some(_) => Ok(c),
        None => Err(format!("{} is not a PLC card.", crate::devices::tag_words(doc, c))),
    }
}

/// The rack a card goes in when none is named: its own, else the only rack.
///
/// # Errors
/// No rack, or several and none named.
pub fn default_rack(doc: &Document, card: Option<ComponentId>) -> Result<ComponentId, String> {
    if let Some(RackRef::Rack(r)) = card.map(|c| rack_ref(doc, c)) {
        return Ok(r);
    }
    let all = racks(doc);
    match all.as_slice() {
        [] => Err("The project has no rack: place a CPU (plccpu) or a head module (plcim) first.".into()),
        [only] => Ok(*only),
        many => {
            let tags: Vec<String> = many.iter().take(8).map(|c| crate::devices::tag_words(doc, *c)).collect();
            let more = if many.len() > 8 { ", …" } else { "" };
            Err(format!("Name a rack: the project has {} ({}{more}).", many.len(), tags.join(", ")))
        }
    }
}

/// A slot parameter: a whole number 0–999.
fn slot_value(v: &Value) -> Result<u32, String> {
    let n = v.as_u64().or_else(|| v.as_str().and_then(|s| parse_slot(s).map(u64::from)));
    n.filter(|n| *n <= u64::from(MAX_SLOT))
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| format!("A slot is a whole number from 0 to {MAX_SLOT}."))
}

/// Where a card goes, worked out against `doc` (design 045 §2.4, §2.5).
///
/// # Errors
/// The refusals of §2.4: no rack, a taken slot (unless `swap`), a slot outside the rack.
pub fn plan_slot(doc: &Document, card: ComponentId, rack: Option<ComponentId>, slot: Option<u32>, swap: bool) -> Result<Vec<Op>, String> {
    let rack = match rack {
        Some(r) => r,
        None => default_rack(doc, Some(card))?,
    };
    let (first, n) = slot_range(doc, rack);
    let last = first.saturating_add(n).saturating_sub(1);
    let rack_words = crate::devices::tag_words(doc, rack);
    let slot = match slot {
        Some(s) => s,
        None => {
            // Staying where it is when that slot is its own and in range.
            match (rack_ref(doc, card), slot_of(doc, card)) {
                (RackRef::Rack(r), Some(s)) if r == rack && (first..=last).contains(&s) && holder_of_slot(doc, rack, s, Some(card)).is_none() => s,
                _ => next_free_slot(doc, rack, Some(card))
                    .ok_or_else(|| format!("{rack_words}'s {n} slots are all taken. Raise its slot count (plc.slots), or pick another rack."))?,
            }
        }
    };
    if !(first..=last).contains(&slot) {
        return Err(format!("{rack_words} has slots {first}–{last}. Pick a slot in that range, or raise its slot count (plc.slots)."));
    }
    let mut ops = slot_ops(doc, card, Some((rack, slot)));
    if let Some(other) = holder_of_slot(doc, rack, slot, Some(card)) {
        if !swap {
            return Err(taken_words(doc, rack, slot, other));
        }
        // The other card takes this card's place: its old slot in a rack, else no rack.
        let back = match (rack_ref(doc, card), slot_of(doc, card)) {
            (RackRef::Rack(r), Some(s)) => Some((r, s)),
            _ => None,
        };
        ops.extend(slot_ops(doc, other, back));
    }
    Ok(ops)
}

/// `-A01 slot 5`, or `no rack`.
pub fn place_words(doc: &Document, card: ComponentId) -> String {
    match (rack_ref(doc, card), slot_of(doc, card)) {
        (RackRef::Rack(r), Some(s)) => format!("{} slot {s}", crate::devices::tag_words(doc, r)),
        (RackRef::Rack(r), None) => format!("{} (no slot)", crate::devices::tag_words(doc, r)),
        _ => "no rack".into(),
    }
}

/// `plcslot {card, rack?, slot?, swap?}` (design 045 §2.5): put a card in a slot, move it, swap it
/// with the card there (`swap: true`), or take it out (`rack: null`). One transaction.
pub(crate) fn run_plcslot(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let swap = opt_typed(params, "swap", "true or false", Value::as_bool).map_err(err)?.unwrap_or(false);
    let out = params.get("rack").is_some_and(Value::is_null);
    let slot = match params.get("slot") {
        None | Some(Value::Null) => None,
        Some(v) => Some(slot_value(v).map_err(err)?),
    };
    if out && slot.is_some() {
        return Err(err("`slot` is for a card in a rack; `rack: null` takes the card out."));
    }
    let v = with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let card = card_param(doc, params.get("card"))?;
        let who = crate::devices::tag_words(doc, card);
        let was = place_words(doc, card);
        let before = crate::plc::moved_blocks_before(doc, card);
        let ops = if out {
            slot_ops(doc, card, None)
        } else {
            let rack = match params.get("rack") {
                None => None,
                Some(v) => Some(rack_param(doc, v)?),
            };
            plan_slot(doc, card, rack, slot, swap)?
        };
        if ops.is_empty() {
            return Ok(json!({"card": card.hex(), "changed": false, "message": format!("{who} is already in {was}.")}));
        }
        p.apply_edit(pg, "plcslot", "Move card", ops)?;
        let doc = p.ledger.document();
        let now = place_words(doc, card);
        let moved = crate::plc::moved_cards(doc, card, &before);
        let tail = if moved > 0 { format!(" (addresses of {moved} card{} change)", if moved == 1 { "" } else { "s" }) } else { String::new() };
        let message = format!("{who}: {was} → {now}{tail}.");
        let (rack, slot) = match (rack_ref(doc, card), slot_of(doc, card)) {
            (RackRef::Rack(r), s) => (Some(r.hex()), s),
            _ => (None, None),
        };
        Ok(json!({"card": card.hex(), "rack": rack, "slot": slot, "changed": true, "moved": moved, "message": message}))
    })?;
    if let Some(m) = v.get("message").and_then(Value::as_str) {
        s.echo(m.to_string());
    }
    Ok(v)
}

/// `plcracks {rack?}` (design 045 §2.5): each rack with its slots in order, and the cards in no rack;
/// read only, from the kept model.
pub(crate) fn run_plcracks(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let only: Option<ComponentId> = match params.get("rack") {
        None | Some(Value::Null) => None,
        Some(v) => Some(with_any_page(s, |p, _| rack_param(p.ledger.document(), v))?),
    };
    with_any_page(s, |p, _| {
        p.outputs();
        let o = p.outputs.as_ref().ok_or("The PLC model is not available.")?;
        let doc = p.ledger.document();
        let plc = &o.plc;
        let mut racks = Vec::new();
        for r in plc.racks_by_tag() {
            if only.is_some_and(|x| x != r.id) {
                continue;
            }
            racks.push(plc.rack_json(doc, r));
        }
        let unracked: Vec<Value> =
            if only.is_some() { Vec::new() } else { plc.unracked_by_tag().into_iter().map(|c| plc.card_json(doc, c)).collect() };
        Ok(json!({"racks": racks, "unracked": unracked}))
    })
}

/// The `place` parameters of a card body (design 045 §2.4): `rack` (ID or tag; `null` for none;
/// absent: the rack of the last card placed, else the only rack, else none) and `slot` (absent: the
/// next free one). `Ok(None)` when the card goes in no rack.
///
/// # Errors
/// An unknown rack, a slot that is not a number or not in the rack, a taken slot.
pub fn place_target(doc: &Document, params: &Value, last_card: Option<ComponentId>) -> Result<Option<(ComponentId, u32)>, String> {
    if params.get("rack").is_some_and(Value::is_null) {
        return Ok(None);
    }
    let rack = match params.get("rack") {
        Some(v) => rack_param(doc, v)?,
        None => {
            let last = last_card.and_then(|c| match rack_ref(doc, c) {
                RackRef::Rack(r) => Some(r),
                _ => None,
            });
            match last.or_else(|| default_rack(doc, None).ok()) {
                Some(r) => r,
                None => return Ok(None),
            }
        }
    };
    let (first, n) = slot_range(doc, rack);
    let last = first.saturating_add(n).saturating_sub(1);
    let slot = match params.get("slot") {
        None | Some(Value::Null) => next_free_slot(doc, rack, None).ok_or_else(|| {
            format!(
                "{}'s {n} slots are all taken. Raise its slot count (plc.slots), or place the card in no rack (rack: null).",
                crate::devices::tag_words(doc, rack)
            )
        })?,
        Some(v) => slot_value(v)?,
    };
    if !(first..=last).contains(&slot) {
        return Err(format!("{} has slots {first}–{last}. Pick a slot in that range.", crate::devices::tag_words(doc, rack)));
    }
    if let Some(other) = holder_of_slot(doc, rack, slot, None) {
        return Err(taken_words(doc, rack, slot, other));
    }
    Ok(Some((rack, slot)))
}

/// The ops of a new card placed in a slot: its two properties, set on the component the place
/// transaction creates.
pub fn new_card_ops(card: ComponentId, rack: ComponentId, slot: u32) -> Vec<Op> {
    vec![
        Op::SetProperty { object: ObjectRef::Component(card), key: RACK_KEY.into(), before: None, after: Some(rack.hex()) },
        Op::SetProperty { object: ObjectRef::Component(card), key: SLOT_KEY.into(), before: None, after: Some(slot.to_string()) },
    ]
}

/// The value check of `plc.rack`, `plc.slot` and `plc.channels` written with `setprop` (one rule
/// with the commands). `None`: not one of these keys.
///
/// # Errors
/// A value that is not of the key's form; `plc.rack` naming something that is not a rack.
pub fn setprop_value(doc: &Document, c: ComponentId, key: &str, value: Option<&str>) -> Option<Result<Option<String>, String>> {
    let v = value.map(str::trim).filter(|v| !v.is_empty());
    Some(match key {
        RACK_KEY => match v {
            None => Ok(None),
            Some(t) => rack_param(doc, &json!(t)).and_then(|r| if r == c { Err("A card cannot be its own rack.".into()) } else { Ok(Some(r.hex())) }),
        },
        SLOT_KEY => match v {
            None => Ok(None),
            Some(t) => parse_slot(t).map(|s| Some(s.to_string())).ok_or_else(|| format!("A slot is a whole number from 0 to {MAX_SLOT}.")),
        },
        CHANNELS_KEY => match v {
            None => Ok(None),
            Some(t) => parse_channels(t).map(|n| Some(n.to_string())).ok_or_else(|| format!("A card has 0 to {MAX_CHANNELS} channels.")),
        },
        _ => return None,
    })
}
