//! Racks, slots and card types: the type catalogue, a module's
//! station and slot, and the commands `plcslot` and `plcracks`.
//!
//! # Semantics
//! - A **rack** (a station) is a device whose stored type is [`ecad_model::objects::plc::PlcRack`]; a
//!   **card** (a module) one whose type is [`ecad_model::objects::plc::PlcModule`].
//! - A card is in a slot when its module names a station (`rack`, by ID) and a slot (0–999).
//!   Everything shown names the rack by its **tag**; only the file holds the ID, so retagging or
//!   renumbering a rack changes nothing on its cards.
//! - A module naming something that is not a station (deleted, or retyped) is a Checks row
//!   (`crate::plc::Plc::checks`), never a refusal on open and never a crash.
//! - `plc.rack`, `plc.slot` and `plc.channels` are the `setprop` names of the module's fields.
//!
//! Every write (the commands, in ecad-engine's `cmd::plc`) is one transaction.

use crate::CmdError;
use std::collections::BTreeSet;

use crate::symbols::StarterKey;
use ecad_model::{ChannelRole, DeviceId, Document, Op};

use crate::plc::edit;

use crate::defaults::{self};
use ecad_model::registry::Scope;

/// The `setprop` name of a module's station.
pub(crate) const RACK_KEY: &str = "plc.rack";
/// The `setprop` name of a module's slot.
pub(crate) const SLOT_KEY: &str = "plc.slot";
/// The `setprop` name of a module's channel count.
pub(crate) const CHANNELS_KEY: &str = "plc.channels";
/// The highest slot number.
pub const MAX_SLOT: u32 = ecad_model::objects::plc::MAX_SLOT as u32;
/// The most channels a card has.
pub const MAX_CHANNELS: u32 = ecad_model::objects::plc::MAX_CHANNELS as u32;

/// A card type.
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
    /// A module whose function has no card body of its own (mixed, position, drive,
    /// measurement, safety logic).
    Module,
}

/// Which way a card's channels go.
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
            CardType::Module => "MOD",
        }
    }

    /// The type a typed word names (`di`, `DQ`, `iol`…), ignoring case.
    pub fn parse(word: &str) -> Option<CardType> {
        let w = word.trim();
        CardType::ALL.into_iter().find(|t| t.code().eq_ignore_ascii_case(w) || (w.eq_ignore_ascii_case("do") && *t == CardType::Dq))
    }

    /// Its card body's starter; `None` for a module of no built-in type.
    pub fn body(self) -> Option<StarterKey> {
        Some(match self {
            CardType::Cpu => StarterKey::Plccpu,
            CardType::Im => StarterKey::Plcim,
            CardType::Di => StarterKey::Plcdi,
            CardType::Dq => StarterKey::Plcdq,
            CardType::Ai => StarterKey::Plcai,
            CardType::Ao => StarterKey::Plcao,
            CardType::Rtd => StarterKey::Plcrtd,
            CardType::Iol => StarterKey::Plciol,
            CardType::Ps => StarterKey::Plcps,
            CardType::Pm => StarterKey::Plcpm,
            CardType::Cm => StarterKey::Plccm,
            CardType::Module => return None,
        })
    }

    /// The card type a card body symbol `s` draws, from the type it declares;
    /// `None` for a channel symbol or a symbol that is not a PLC card's.
    pub fn of_symbol(s: &crate::symbols::Starter) -> Option<CardType> {
        match s.kind {
            crate::symbols::Kind::Channel(_) => None,
            _ => CardType::of_type(&s.device_type()),
        }
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

    /// The roles of one channel, in the cell's order; empty for a type without
    /// channels.
    pub fn roles(self) -> &'static [ChannelRole] {
        use ChannelRole as R;
        match self {
            CardType::Di => &[R::In],
            CardType::Dq => &[R::Out],
            CardType::Ai | CardType::Ao => &[R::Plus, R::Minus],
            CardType::Rtd => &[R::CurrentPlus, R::SensePlus, R::SenseMinus, R::CurrentMinus],
            CardType::Iol => &[R::SupplyPlus, R::Cq, R::SupplyMinus],
            _ => &[],
        }
    }

    /// Its channel cell's starter; `None` for a type without channels.
    pub fn cell(self) -> Option<StarterKey> {
        match self {
            CardType::Di => Some(StarterKey::Chdi),
            CardType::Dq => Some(StarterKey::Chdq),
            CardType::Ai => Some(StarterKey::Chai),
            CardType::Ao => Some(StarterKey::Chao),
            CardType::Rtd => Some(StarterKey::Chrtd),
            CardType::Iol => Some(StarterKey::Chiol),
            _ => None,
        }
    }

    /// Whether it has channels.
    pub fn has_channels(self) -> bool {
        !self.roles().is_empty()
    }

    /// The registry entry of its built-in channel count (`plc.<type>.channels`).
    pub(crate) fn channels_entry(self) -> Option<&'static ecad_model::registry::Entry> {
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
            CardType::Module => "module",
        }
    }
}

/// A channel count as stored: digits only, 0–[`MAX_CHANNELS`].
pub(crate) fn parse_channels(text: &str) -> Option<u32> {
    let t = text.trim();
    ecad_model::naming::words::uint(t, MAX_CHANNELS).filter(|_| t.len() <= 3)
}

/// What a module's station names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RackRef {
    /// No station.
    None,
    /// A station of this project.
    Rack(DeviceId),
    /// An ID that is not a station of this project (deleted, or no longer a station).
    Missing(DeviceId),
}

/// What card `c`'s station names.
pub fn rack_ref(doc: &Document, c: DeviceId) -> RackRef {
    match edit::module_of(doc, c).and_then(|m| m.rack) {
        None => RackRef::None,
        Some(r) if r != c && edit::station_of(doc, r).is_some() => RackRef::Rack(r),
        Some(r) => RackRef::Missing(r),
    }
}

/// Card `c`'s slot, when it has one.
pub(crate) fn slot_of(doc: &Document, c: DeviceId) -> Option<u32> {
    edit::module_of(doc, c).and_then(|m| m.slot).map(u32::from)
}

/// A rack's slot range: its first slot and how many (registry, rack level allowed).
pub(crate) fn slot_range(doc: &Document, rack: DeviceId) -> (u32, u32) {
    let at = Scope { component: Some(rack), ..Scope::project() };
    let n = ecad_model::registry::resolve(doc, &defaults::plc::SLOTS, at).number();
    let first = ecad_model::registry::resolve(doc, &defaults::plc::SLOT_FIRST, at).word();
    // The registry only returns values in range; the casts are of 1–999 and 0/1.
    let n = if n.is_finite() && n >= 1.0 { (n as u32).min(MAX_SLOT) } else { 32 };
    (if first == "0" { 0 } else { 1 }, n)
}

/// Card `c`'s channel count, over all its channel groups.
pub fn channel_count(doc: &Document, c: DeviceId) -> u32 {
    edit::module_of(doc, c).map_or(0, ecad_model::objects::plc::PlcModule::channel_count)
}

/// The racks of the project, by tag (the order every list uses). Reads every component's head;
/// for prompts and commands, not per frame.
pub fn racks(doc: &Document) -> Vec<DeviceId> {
    let mut v: Vec<(String, DeviceId)> =
        doc.tagged().map(|(_, c)| c).filter(|c| edit::station_of(doc, *c).is_some()).map(|c| (crate::devices::tags::tag_words(doc, c), c)).collect();
    v.sort();
    v.dedup_by_key(|x| x.1);
    v.into_iter().map(|(_, c)| c).collect()
}

/// The cards whose module names `rack` (a station or not), with their slots, by slot then tag:
/// the index's station → modules map (`PlcRack::modules`).
pub(crate) fn cards_naming(doc: &Document, rack: DeviceId) -> Vec<(DeviceId, Option<u32>)> {
    let mut v: Vec<(DeviceId, Option<u32>)> = doc.plc_modules(rack).map(|c| (c, slot_of(doc, c))).collect();
    v.sort_by_key(|(c, s)| (*s, crate::devices::tags::tag_words(doc, *c)));
    v
}

/// Which card of `rack` (other than `except`) holds `slot`.
pub(crate) fn holder_of_slot(doc: &Document, rack: DeviceId, slot: u32, except: Option<DeviceId>) -> Option<DeviceId> {
    cards_naming(doc, rack).into_iter().find(|(c, s)| *s == Some(slot) && Some(*c) != except).map(|(c, _)| c)
}

/// The lowest free slot of `rack`, `except` not counting as holding one.
pub fn next_free_slot(doc: &Document, rack: DeviceId, except: Option<DeviceId>) -> Option<u32> {
    let taken: BTreeSet<u32> = cards_naming(doc, rack).into_iter().filter(|(c, _)| Some(*c) != except).filter_map(|(_, s)| s).collect();
    let (first, n) = slot_range(doc, rack);
    (first..first.saturating_add(n)).find(|s| !taken.contains(s))
}

/// `DI 16`: a card's type and channel count.
pub(crate) fn type_words(doc: &Document, c: DeviceId) -> String {
    match crate::plc::card_type_of(doc, c) {
        Some(t) if t.has_channels() => format!("{} {}", t.code(), channel_count(doc, c)),
        Some(t) => t.code().to_string(),
        None => "?".into(),
    }
}

/// The refusal of a taken slot, with the way out.
pub(crate) fn taken_words(doc: &Document, rack: DeviceId, slot: u32, holder: DeviceId) -> String {
    // The card's own slot counts as taken: Enter offers a slot it is not in.
    let next = next_free_slot(doc, rack, None).map_or_else(|| "none is free".to_string(), |s| s.to_string());
    format!(
        "Slot {slot} of {} holds {} ({}). Pick another slot, or Enter for the next free one ({next}).",
        crate::devices::tags::tag_words(doc, rack),
        crate::devices::tags::tag_words(doc, holder),
        type_words(doc, holder)
    )
}

/// Put `card` in `rack` at `slot` (or take it out with `None`), adding the change to `ops`.
pub fn put(doc: &Document, ops: &mut Vec<Op>, card: DeviceId, to: Option<(DeviceId, u32)>) {
    edit::module(doc, ops, card, |m| {
        m.rack = to.map(|(r, _)| r);
        m.slot = to.and_then(|(_, s)| u16::try_from(s).ok());
    });
}

/// The ops putting `card` in `rack` at `slot` (or taking it out with `None`), only what changes.
pub(crate) fn slot_ops(doc: &Document, card: DeviceId, to: Option<(DeviceId, u32)>) -> Vec<Op> {
    let mut ops = Vec::new();
    put(doc, &mut ops, card, to);
    ops
}

/// What deleting rack `rack` adds: every card naming it
/// taken out of its slot, and how many cards that is. Nothing when `rack` is not named by any card.
pub(crate) fn release_cards(doc: &Document, rack: DeviceId) -> (Vec<Op>, usize) {
    let cards = cards_naming(doc, rack);
    let mut ops = Vec::new();
    for (c, _) in &cards {
        put(doc, &mut ops, *c, None);
    }
    (ops, cards.len())
}

/// `6 cards are now in no rack.`
pub(crate) fn released_words(n: usize) -> String {
    if n == 1 { "1 card is now in no rack.".into() } else { format!("{n} cards are now in no rack.") }
}

/// A device named by `t`: a hex ID (32 digits) of a device, else a tag.
///
/// # Errors
/// No device of that name ("No device -A9.").
fn device_named(doc: &Document, t: &str) -> Result<DeviceId, CmdError> {
    let t = t.trim();
    if let Some(c) = DeviceId::parse(t).filter(|c| t.len() == 32 && doc.devices().contains_key(c)) {
        return Ok(c);
    }
    let tag = ecad_model::naming::tag::normalise(t, crate::defaults::print::tag_style(doc)).unwrap_or_else(|_| t.to_string());
    doc.components_tagged(&tag).first().copied().ok_or_else(|| CmdError::refused(format!("No device {tag}.")))
}

/// A rack named by `t`: a hex ID or a tag.
///
/// # Errors
/// Nothing of that name, or it is not a rack.
pub fn rack_named(doc: &Document, t: &str) -> Result<DeviceId, CmdError> {
    let c = device_named(doc, t)?;
    if !edit::station_of(doc, c).is_some() {
        return Err(CmdError::refused(format!("{} is not a rack; a rack is a CPU or a head module.", crate::devices::tags::tag_words(doc, c))));
    }
    Ok(c)
}

/// A card named by `t`: a hex ID or a tag, of a device that is a card (not a rack).
///
/// # Errors
/// Nothing of that name, a rack, or a device that is not a PLC card.
pub(crate) fn card_named(doc: &Document, t: &str) -> Result<DeviceId, CmdError> {
    let c = device_named(doc, t)?;
    match crate::plc::card_type_of(doc, c) {
        Some(t) if t.is_rack() => {
            Err(CmdError::refused(format!("{} is a {}: it is a rack, not a card in a slot.", crate::devices::tags::tag_words(doc, c), t.noun())))
        }
        Some(_) => Ok(c),
        None => Err(CmdError::refused(format!("{} is not a PLC card.", crate::devices::tags::tag_words(doc, c)))),
    }
}

/// The rack a card goes in when none is named: its own, else the only rack.
///
/// # Errors
/// No rack, or several and none named.
pub fn default_rack(doc: &Document, card: Option<DeviceId>) -> Result<DeviceId, CmdError> {
    if let Some(RackRef::Rack(r)) = card.map(|c| rack_ref(doc, c)) {
        return Ok(r);
    }
    let all = racks(doc);
    match all.as_slice() {
        [] => Err(CmdError::refused("The project has no rack: place a CPU (plccpu) or a head module (plcim) first.")),
        [only] => Ok(*only),
        many => {
            let tags: Vec<String> = many.iter().take(8).map(|c| crate::devices::tags::tag_words(doc, *c)).collect();
            let more = if many.len() > 8 { ", …" } else { "" };
            Err(CmdError::refused(format!("Name a rack: the project has {} ({}{more}).", many.len(), tags.join(", "))))
        }
    }
}

/// A slot written as text: a whole number 0–999.
///
/// # Errors
/// Anything else, in words.
pub(crate) fn slot_number(text: &str) -> Result<u32, CmdError> {
    crate::fields::plc_slot(text)
        .ok()
        .filter(|n| *n <= MAX_SLOT)
        .ok_or_else(|| CmdError::refused(format!("A slot is a whole number from 0 to {MAX_SLOT}.")))
}

/// Where a card goes, worked out against `doc`.
///
/// # Errors
/// The refusals: no rack, a taken slot (unless `swap`), a slot outside the rack.
pub(crate) fn plan_slot(doc: &Document, card: DeviceId, rack: Option<DeviceId>, slot: Option<u32>, swap: bool) -> Result<Vec<Op>, CmdError> {
    let rack = match rack {
        Some(r) => r,
        None => default_rack(doc, Some(card))?,
    };
    let (first, n) = slot_range(doc, rack);
    let last = first.saturating_add(n).saturating_sub(1);
    let rack_words = crate::devices::tags::tag_words(doc, rack);
    let slot = match slot {
        Some(s) => s,
        // `slot` absent is the next free slot; the card's own slot
        // counts as taken, so a card already in the rack moves.
        None => next_free_slot(doc, rack, None).ok_or_else(|| {
            CmdError::refused(format!("{rack_words}'s {n} slots are all taken. Raise its slot count (plc.slots), or pick another rack."))
        })?,
    };
    if !(first..=last).contains(&slot) {
        return Err(CmdError::refused(format!(
            "{rack_words} has slots {first}–{last}. Pick a slot in that range, or raise its slot count (plc.slots)."
        )));
    }
    let mut ops = Vec::new();
    put(doc, &mut ops, card, Some((rack, slot)));
    if let Some(other) = holder_of_slot(doc, rack, slot, Some(card)) {
        if !swap {
            return Err(CmdError::refused(taken_words(doc, rack, slot, other)));
        }
        // The other card takes this card's place: its old slot in a rack, else no rack.
        let back = match (rack_ref(doc, card), slot_of(doc, card)) {
            (RackRef::Rack(r), Some(s)) => Some((r, s)),
            _ => None,
        };
        put(doc, &mut ops, other, back);
    }
    Ok(ops)
}

/// `-A01 slot 5`, or `no rack`.
pub fn place_words(doc: &Document, card: DeviceId) -> String {
    match (rack_ref(doc, card), slot_of(doc, card)) {
        (RackRef::Rack(r), Some(s)) => format!("{} slot {s}", crate::devices::tags::tag_words(doc, r)),
        (RackRef::Rack(r), None) => format!("{} (no slot)", crate::devices::tags::tag_words(doc, r)),
        _ => "no rack".into(),
    }
}

/// Where a card body `place`d goes: `rack` (`None`: absent, the rack of the
/// last card placed, else the only rack, else none; `Some(None)`: null, no rack; else an ID or a
/// tag) and `slot` as written (absent: the next free one). `Ok(None)` when the card goes in no rack.
///
/// # Errors
/// An unknown rack, a slot that is not a number or not in the rack, a taken slot.
pub fn place_target(
    doc: &Document, rack: Option<Option<&str>>, slot: Option<&str>, last_card: Option<DeviceId>,
) -> Result<Option<(DeviceId, u32)>, CmdError> {
    let rack = match rack {
        Some(None) => return Ok(None),
        Some(Some(t)) => rack_named(doc, t)?,
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
    let slot = match slot {
        None => next_free_slot(doc, rack, None).ok_or_else(|| {
            CmdError::refused(format!(
                "{}'s {n} slots are all taken. Raise its slot count (plc.slots), or place the card in no rack (rack: null).",
                crate::devices::tags::tag_words(doc, rack)
            ))
        })?,
        Some(t) => slot_number(t)?,
    };
    if !(first..=last).contains(&slot) {
        return Err(CmdError::refused(format!(
            "{} has slots {first}–{last}. Pick a slot in that range.",
            crate::devices::tags::tag_words(doc, rack)
        )));
    }
    if let Some(other) = holder_of_slot(doc, rack, slot, None) {
        return Err(CmdError::refused(taken_words(doc, rack, slot, other)));
    }
    Ok(Some((rack, slot)))
}

/// A new card placed in a slot: its station and slot set on the device the place transaction
/// creates (in `ops`).
pub fn put_new_card(doc: &Document, ops: &mut Vec<Op>, card: DeviceId, rack: DeviceId, slot: u32) {
    put(doc, ops, card, Some((rack, slot)));
}

/// What merging device `from` into `into` adds to `ops` (the merge's ops so far) for rack `from`:
/// `into` takes `from`'s station data with its rack body, unless it is a
/// station already, and `from`'s cards are re-pointed to it (a slot clash then shows as
/// `slot-twice`, with its fix). Nothing when `from` is not a rack.
pub(crate) fn merge_into(doc: &Document, ops: &mut Vec<Op>, from: DeviceId, into: DeviceId) {
    let Some(station) = edit::station_of(doc, from).cloned() else { return };
    // `from`'s representations, its rack body among them, move to `into`, which so becomes the rack
    // (a merge that would give `into` two heads is refused before this).
    if !edit::station_of(doc, into).is_some() {
        edit::modify(doc, ops, into, |t| *t = ecad_model::DeviceType::PlcRack(station));
    }
    for (c, _) in cards_naming(doc, from) {
        if c != into {
            edit::module(doc, ops, c, |m| m.rack = Some(into));
        }
    }
}

/// The I/O card types a card's type may be changed among:
/// their bodies have the same connection points (`L+`, `M`) at the same places, so the
/// body's symbol is swapped and nothing it is wired to moves.
pub const IO_TYPES: [CardType; 6] = [CardType::Di, CardType::Dq, CardType::Ai, CardType::Ao, CardType::Rtd, CardType::Iol];

/// `a DQ card`, `an AO card`.
pub fn with_article(t: CardType) -> String {
    let noun = t.noun();
    let an = matches!(t, CardType::Ai | CardType::Ao | CardType::Iol | CardType::Im | CardType::Rtd);
    format!("{} {noun}", if an { "an" } else { "a" })
}

/// What a card type's channels are: `digital outputs`, `analogue inputs`.
pub(crate) fn signal_words(t: CardType) -> &'static str {
    match t {
        CardType::Di => "digital inputs",
        CardType::Dq => "digital outputs",
        CardType::Ai => "analogue inputs",
        CardType::Ao => "analogue outputs",
        CardType::Rtd => "RTD inputs",
        CardType::Iol => "IO-Link ports",
        _ => "channels",
    }
}

/// What a type change does to the signals, for the confirm step and the echo: `Its channels
/// become analogue outputs (they were digital outputs); tags and descriptions stay.`
pub fn change_words(from: CardType, to: CardType) -> String {
    format!("Its channels become {} (they were {}); tags and descriptions stay.", signal_words(to), signal_words(from))
}
