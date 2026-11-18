//! The rules of the PLC commands (briefs 045, 051 and 056): a card's slot (`plcslot`), its type
//! (`plccardtype`), its channels on the page (`plcchannels`, `plcchannel`), its page (`plcpage`),
//! the addresses (`plcaddress`) and the tags (`plcmaketags`). Each function reads the document and
//! returns the ops to commit and what to report; the engine crate's shims read the parameters, the
//! kept PLC model and the files, commit and answer in JSON.
//!
//! Excludes: the kept model (`plcracks`, `iolist`, `plcexport` and `plcimport` read it in the
//! engine crate) and the tag file formats ([`crate::plc::exchange`]).

use ecad_model::units::{Degrees, Radians};
use std::collections::{BTreeMap, BTreeSet};

use ecad_model::book::Book;
use ecad_model::{DeviceId, Document, IdAllocator, Ledger, ObjectRef, Op, PageId, Placement, Pt, Style, Subject, Terminal, TerminalId};

use crate::defaults;
use crate::devices::TagHigh;
use crate::error::CmdError;
use crate::plc::address::{MAX_BYTE, MAX_SIZE, SIZE_KEY, START_KEY, snapshot};
use crate::plc::card_type_of;
use crate::plc::channels::{
    Join, MAX_FENCE, MAX_INDEX, MAX_PIN, ch_key, ch_value, channels_words, first_number, land_cell, landings, new_card_ops, pins_for, plan_cell,
    point_of, points, role_at, role_named, roles_of, spacing_refusal,
};
use crate::plc::rack::{self, CardType, IO_TYPES, MAX_CHANNELS, RackRef, card_named, channel_count, rack_named, with_article};
use crate::plc::signals;

// ---------------------------------------------------------------- slots and card types

/// The rack `plcslot` puts a card in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RackChoice<'a> {
    /// Its own rack, else the only one.
    Default,
    /// No rack: take the card out.
    Out,
    /// The rack named (an ID or a tag).
    Named(&'a str),
}

/// The slot `plcslot` names, read (a whole number 0–999), checked against taking the card out.
///
/// # Errors
/// A slot that does not read, or a slot with `rack: null`.
pub fn slot_given(out: bool, slot: Option<&str>) -> Result<Option<u32>, CmdError> {
    let slot = slot.map(rack::slot_number).transpose()?;
    if out && slot.is_some() {
        return Err(CmdError::input("`slot` is for a card in a rack; `rack: null` takes the card out."));
    }
    Ok(slot)
}

/// `plcslot` (design 045 §2.5): the card named `card` and the ops that put it in `slot` of `rack`
/// (swapping with the card there when `swap`), or take it out of its rack.
///
/// # Errors
/// No such card or rack, and the refusals of [`rack::plan_slot`].
pub fn slot_plan(doc: &Document, card: &str, rack: RackChoice<'_>, slot: Option<u32>, swap: bool) -> Result<(DeviceId, Vec<Op>), CmdError> {
    let card = card_named(doc, card)?;
    let ops = match rack {
        RackChoice::Out => rack::slot_ops(doc, card, None),
        RackChoice::Default => rack::plan_slot(doc, card, None, slot, swap)?,
        RackChoice::Named(t) => {
            let r = rack_named(doc, t)?;
            rack::plan_slot(doc, card, Some(r), slot, swap)?
        }
    };
    Ok((card, ops))
}

/// The I/O card type `word` names (DI, DQ, AI, AO, RTD or IOL).
///
/// # Errors
/// Any other word.
pub fn io_type(word: &str) -> Result<CardType, CmdError> {
    CardType::parse(word).filter(|t| IO_TYPES.contains(t)).ok_or(CmdError::refused("`type` is DI, DQ, AI, AO, RTD or IOL"))
}

/// What `plccardtype` does.
#[derive(Debug, Clone)]
pub struct CardTypePlan {
    pub card: DeviceId,
    /// The ops; empty when the card is that type already.
    pub ops: Vec<Op>,
    pub message: String,
}

/// `plccardtype {card, type}`: change what an I/O card is. Its body symbols show the new type's
/// body; the component's own symbol follows; a description that was the old body's name becomes
/// the new one's; safe states the new type cannot hold are cleared. Channel cells already drawn keep
/// their own symbols (a `channel-type` Check says so).
///
/// # Errors
/// No such card, not an I/O card, no card body drawn, or a signal (a channel in use, named or
/// drawn) at or past the new type's channel count.
pub fn plan_card_type(doc: &Document, ids: &mut IdAllocator, card: &str, to: CardType) -> Result<CardTypePlan, CmdError> {
    let c = card_named(doc, card)?;
    let who = crate::devices::tag_words(doc, c);
    let from = card_type_of(doc, c).ok_or_else(|| CmdError::refused(format!("{who} is not a PLC card.")))?;
    if !IO_TYPES.contains(&from) {
        return Err(CmdError::refused(format!("{who} is a {}; only an I/O card's type can be changed.", from.noun())));
    }
    if from == to {
        return Ok(CardTypePlan { card: c, ops: Vec::new(), message: format!("{who} is {} already.", with_article(to)) });
    }
    // No signal may be left on a channel the new type does not have (overseer's look check of
    // 2026-11-14): a channel in use, named or drawn at or past the new count refuses it.
    let room = channel_count(doc, c, to);
    let chans = crate::plc::channel_data(doc, c);
    let in_use = |ch: &crate::plc::ChannelData| ch.used || ch.mark().is_some() || !ch.points.is_empty();
    let signals_n = chans.values().filter(|ch| in_use(ch)).count();
    let outside = chans.iter().filter(|(i, ch)| **i >= room && in_use(ch)).count();
    if outside > 0 {
        let n = |k: usize, one: &str, many: &str| if k == 1 { format!("1 {one}") } else { format!("{k} {many}") };
        return Err(CmdError::refused(format!(
            "{who} has {}; {} has {}. Move or remove {} first.",
            n(signals_n, "signal", "signals"),
            with_article(to),
            n(usize::try_from(room).unwrap_or(usize::MAX), "channel", "channels"),
            n(outside, "signal", "signals")
        )));
    }
    let new_starter = crate::symbols::by_key(to.body()).ok_or(CmdError::refused("No card body for that type."))?;
    let mut ops = Vec::new();
    let def = match crate::symbols::embedded(doc, new_starter) {
        Some(d) => d,
        None => {
            let d: ecad_model::DefinitionId = ids.fresh();
            ops.push(Op::EmbedDefinition { id: d, definition: new_starter.definition()?.clone() });
            d
        }
    };
    let mut bodies = 0;
    let mut old_defs = BTreeSet::new();
    for (_, r) in doc.representations_of(Subject::Device(c)) {
        let Some(st) = crate::devices::rep_starter(doc, r) else { continue };
        if CardType::of_body(st.key) != Some(from) {
            continue;
        }
        for item in &r.items {
            let Some(before) = doc.items.get(item).cloned() else { continue };
            let ecad_model::ItemKind::Symbol(sym) = &before.kind else { continue };
            old_defs.insert(sym.definition);
            let after = ecad_model::Item {
                kind: ecad_model::ItemKind::Symbol(ecad_model::SymbolInstance { definition: def, placement: sym.placement }),
                ..before.clone()
            };
            ops.push(Op::ModifyItem { id: *item, before, after });
            bodies += 1;
        }
    }
    if bodies == 0 {
        return Err(CmdError::refused(format!(
            "{who} has no card body drawn; its type comes from its channel symbols. Place a {} body for it.",
            to.noun()
        )));
    }
    let comp = doc.devices.get(&c).ok_or(CmdError::NotFound(ecad_model::Noun::Device))?;
    if let Some(d) = comp.definition.filter(|d| old_defs.contains(d)) {
        ops.push(Op::SetDeviceDefinition { id: c, before: Some(d), after: Some(def) });
        ops.extend(crate::devices::retype(c, comp, def, &new_starter.device_type()));
    }
    // A description that was the old body's name (placing writes it) follows the type.
    let old_name = crate::symbols::by_key(from.body()).map(|s| s.name);
    if comp.description.as_ref().is_some_and(|d| Some(d.as_str()) == old_name) {
        // The device as the definition swap and retype above leave it.
        let swapped = comp.definition.is_some_and(|d| old_defs.contains(&d));
        let retyped = ops.iter().rev().find_map(|o| match o {
            Op::ModifyDevice { id, after, .. } if *id == c => Some(after.clone()),
            _ => None,
        });
        let before = match (retyped, swapped) {
            (Some(d), _) => d,
            (None, true) => ecad_model::Device { definition: Some(def), ..comp.clone() },
            (None, false) => comp.clone(),
        };
        let after = ecad_model::Device { description: crate::devices::description_of(new_starter.name), ..before.clone() };
        ops.push(Op::ModifyDevice { id: c, before, after });
    }
    // Safe states the new type cannot hold go.
    for (k, v) in comp.properties.range(crate::plc::channels::CH_PREFIX.to_string()..) {
        if !k.starts_with(crate::plc::channels::CH_PREFIX) {
            break;
        }
        if crate::plc::channels::parse_ch_key(k).is_some_and(|(_, w)| w == signals::SAFE) && signals::safe_value(Some(to), Some(v)).is_err() {
            ops.push(Op::SetProperty { object: ObjectRef::Device(c), key: k.clone(), before: Some(v.clone()), after: None });
        }
    }
    let message = format!("{who} is now {}. {}", with_article(to), rack::change_words(from, to));
    Ok(CardTypePlan { card: c, ops, message })
}

// ---------------------------------------------------------------- tags

/// The cards `plcmaketags` fills: card `card`, or every card of rack `rack`, and their words.
///
/// # Errors
/// Neither or both, or no such card or rack.
pub fn tag_cards(doc: &Document, card: Option<&str>, rack: Option<&str>) -> Result<(Vec<DeviceId>, String), CmdError> {
    match (card, rack) {
        (Some(c), None) => {
            let c = card_named(doc, c)?;
            Ok((vec![c], crate::devices::tag_words(doc, c)))
        }
        (None, Some(r)) => {
            let r = rack_named(doc, r)?;
            let cards: Vec<DeviceId> = rack::cards_naming(doc, r).into_iter().map(|(c, _)| c).collect();
            Ok((cards, format!("rack {}", crate::devices::tag_words(doc, r))))
        }
        _ => Err(CmdError::input("Give `card` or `rack` (an ID or tag).")),
    }
}

// ---------------------------------------------------------------- addresses

/// What `plcaddress` is given. Each value: absent (`None`), null (`Some(None)`, unpins or goes
/// back to the default) or the text written.
#[derive(Debug, Clone, Default)]
pub struct AddressInput {
    pub rack: Option<String>,
    pub card: Option<String>,
    pub style: Option<Option<String>>,
    pub input_start: Option<Option<String>>,
    pub output_start: Option<Option<String>>,
    pub start: Option<Option<String>>,
    pub size: Option<Option<String>>,
    pub fix: bool,
    pub clear: bool,
}

/// What `plcaddress` changes.
#[derive(Debug, Clone)]
pub struct AddressPlan {
    /// The rack's or card's tag.
    pub who: String,
    /// The rack whose cards' addresses may move.
    pub rack: Option<DeviceId>,
    /// What `fix` or `clear` did, in words (empty without them).
    pub tail: String,
    pub ops: Vec<Op>,
}

/// `plcaddress {rack | card, …}` (design 045 §5.3): a rack's address style and starts (and
/// pinning or unpinning every card), or a card's pinned start and size.
///
/// # Errors
/// No such rack or card, a value that is not one of its setting's, `fix` with `clear`, or a start
/// or size out of range.
pub fn plan_address(doc: &Document, input: &AddressInput) -> Result<AddressPlan, CmdError> {
    let mut ops = Vec::new();
    let set = |ops: &mut Vec<Op>, c: DeviceId, key: &str, after: Option<String>| {
        let before = doc.devices.get(&c).and_then(|x| x.properties.get(key)).cloned();
        if before != after {
            ops.push(Op::SetProperty { object: ObjectRef::Device(c), key: key.into(), before, after });
        }
    };
    let word = |v: &Option<String>| v.as_deref().map(|s| s.trim().to_string());
    if let Some(rv) = &input.rack {
        let r = rack_named(doc, rv)?;
        let before = snapshot(doc, r);
        for (key, value, entry) in [
            ("style", &input.style, &defaults::plc::ADDRESS_STYLE),
            ("input_start", &input.input_start, &defaults::plc::START_INPUT),
            ("output_start", &input.output_start, &defaults::plc::START_OUTPUT),
        ] {
            let Some(v) = value else { continue };
            let value = word(v);
            if let Some(x) = &value
                && entry.parse(x).is_none()
            {
                return Err(CmdError::refused(format!("`{key}` {x} is not a value of {}.", entry.key)));
            }
            set(&mut ops, r, entry.key, value);
        }
        if input.fix && input.clear {
            return Err(CmdError::input("`fix` pins every card; `clear` unpins them: give one."));
        }
        let mut pinned_n = 0;
        if input.fix || input.clear {
            for (c, b) in &before {
                let start = b.input.or(b.output).map(|x| x.0);
                let after = if input.fix { start.map(|x| x.to_string()) } else { None };
                let had = ops.len();
                set(&mut ops, *c, START_KEY, after);
                pinned_n += usize::from(ops.len() > had);
            }
        }
        let tail = if input.fix {
            format!("{pinned_n} card{} pinned at their current addresses", if pinned_n == 1 { "" } else { "s" })
        } else if input.clear {
            format!("{pinned_n} card{} unpinned", if pinned_n == 1 { "" } else { "s" })
        } else {
            String::new()
        };
        return Ok(AddressPlan { who: crate::devices::tag_words(doc, r), rack: Some(r), tail, ops });
    }
    let c = card_named(doc, input.card.as_deref().ok_or(CmdError::input("`card` (a card's ID or tag) is required"))?)?;
    for (key, value, prop, max) in [("start", &input.start, START_KEY, MAX_BYTE), ("size", &input.size, SIZE_KEY, MAX_SIZE)] {
        let Some(v) = value else { continue };
        let value = word(v);
        if let Some(x) = &value
            && !(ecad_model::words::is_digits(x) && x.len() <= 5 && x.parse::<u32>().is_ok_and(|n| n <= max && (key == "start" || n >= 1)))
        {
            return Err(CmdError::refused(format!("`{key}` is a whole number of bytes from {} to {max}.", u32::from(key == "size"))));
        }
        set(&mut ops, c, prop, value);
    }
    let r = match rack::rack_ref(doc, c) {
        RackRef::Rack(r) => Some(r),
        _ => None,
    };
    Ok(AddressPlan { who: crate::devices::tag_words(doc, c), rack: r, tail: String::new(), ops })
}

// ---------------------------------------------------------------- channels

/// A channel number written as text (a number as shown, or a 0-based index): 0 to [`MAX_INDEX`].
///
/// # Errors
/// Anything else, in words.
pub fn channel_number(text: &str) -> Result<u32, CmdError> {
    text.trim()
        .parse::<u64>()
        .ok()
        .filter(|n| *n <= u64::from(MAX_INDEX))
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| CmdError::refused(format!("A channel is a whole number from 0 to {MAX_INDEX}.")))
}

/// The card `card` names (an ID or a tag), which must be a card with channels, and its type.
///
/// # Errors
/// No such card, or a card without channels.
pub fn channel_card(doc: &Document, card: &str) -> Result<(DeviceId, CardType), CmdError> {
    let c = card_named(doc, card)?;
    let t = card_type_of(doc, c).ok_or(CmdError::refused("Not a PLC card."))?;
    if !t.has_channels() {
        return Err(CmdError::refused(format!("{} is a {}; it has no channels.", crate::devices::tag_words(doc, c), t.noun())));
    }
    Ok((c, t))
}

/// What the fence form of `plcchannels` is given (besides its card).
#[derive(Debug, Clone)]
pub struct FenceInput {
    /// A new card's type (DI, DQ…); absent: DI.
    pub ty: Option<String>,
    /// The first channel number, as shown.
    pub first: Option<u32>,
    /// The point role landed on each wire.
    pub point: Option<String>,
    pub from: Pt,
    pub to: Pt,
}

/// A planned Place Channels line or rest.
#[derive(Debug, Clone)]
pub struct ChannelsPlan {
    pub ops: Vec<Op>,
    pub card: DeviceId,
    pub tag: String,
    pub ty: CardType,
    /// The starter key of the cells (the next place's default).
    pub starter: crate::symbols::StarterKey,
    /// The channel indexes drawn, in order.
    pub placed: Vec<u32>,
    /// The card's first channel number.
    pub first: u32,
    /// The pins drawn.
    pub pins: Vec<String>,
}

/// `plcchannels {card?, type?, first?, point?, from, to}` (design 045 §4.5): one channel cell on
/// each wire the line crosses, in order, of card `card` (or a new card of type `ty`).
///
/// # Errors
/// An unknown type or point, no wire or too many crossed, cells too close, no channel left, or a
/// refusal of one landing.
#[allow(clippy::too_many_arguments)]
pub fn plan_channels(
    ledger: &Ledger, ids: &mut IdAllocator, high: &mut TagHigh, page: PageId, style: Style, card: Option<(DeviceId, CardType)>, input: &FenceInput,
    tol: f64,
) -> Result<ChannelsPlan, CmdError> {
    let doc = ledger.document();
    let t = match (card, &input.ty) {
        (Some((_, t)), _) => t,
        (None, Some(w)) => CardType::parse(w).filter(|t| t.has_channels()).ok_or_else(|| {
            CmdError::refused(format!("No card type \"{}\". Types: DI, DQ, AI, AO, RTD, IOL.", w.chars().take(16).collect::<String>()))
        })?,
        (None, None) => CardType::Di,
    };
    let found = crate::slash::crossings(doc, page, input.from, input.to, tol);
    if found.members.len() > MAX_FENCE {
        return Err(CmdError::refused(format!("That line crosses {} wires; at most {MAX_FENCE} at once. Draw a shorter line.", found.members.len())));
    }
    if found.members.is_empty() {
        return Err(CmdError::refused("The line crosses no wire. Draw it across the wires to land."));
    }
    let starter = crate::symbols::by_key(t.cell()).ok_or(CmdError::refused("No channel symbol for that type."))?;
    let point_role = match &input.point {
        Some(w) => Some(role_named(starter, w)?),
        None => None,
    };
    let landings = landings(doc, page, starter, &found.members, point_role);
    // Brief 051 (design 051 §2.2, Q1): cells are one width apart at least, or they overlap.
    let anchors: Vec<crate::slash::Crossing> = landings.iter().filter_map(|l| l.points.first().map(|x| x.1)).collect();
    if let Some(e) = spacing_refusal(doc, starter, t, &anchors) {
        return Err(CmdError::refused(e));
    }
    let mut ops = Vec::new();
    let def = crate::cmd::terminal::definition_for(doc, ids, starter, &mut ops)?;
    let (card, tag, new) = match card {
        Some((c, _)) => (c, crate::devices::tag_words(doc, c), false),
        None => {
            let (c, tag) = new_card_ops(ledger, ids, high, starter, def, t, None, &mut ops)?;
            (c, tag, true)
        }
    };
    let n = channel_count(doc, card, t);
    let first = first_number(doc, card);
    let roles = roles_of(starter);
    let pts = if new { BTreeMap::new() } else { points(doc, card) };
    let mut used: BTreeSet<String> = if new { BTreeSet::new() } else { crate::devices::labels_of(doc, card) };
    let mut next = match input.first {
        Some(x) => x.checked_sub(first).ok_or_else(|| CmdError::refused(format!("{tag}'s channels start at {first}.")))?,
        None => (0..MAX_INDEX).find(|i| !pts.contains_key(i)).unwrap_or(0),
    };
    let mut placed: Vec<u32> = Vec::new();
    let mut all_pins: Vec<String> = Vec::new();
    let def_terms = &starter.definition()?.terminals;
    const NO_ROLE: &str = "The channel symbol has no point for that role.";
    const NO_POINT: &str = "The channel symbol has no point.";
    for l in &landings {
        while pts.contains_key(&next) && next < MAX_INDEX {
            next += 1;
        }
        if next >= n.max(1) && !new && next >= MAX_CHANNELS {
            return Err(CmdError::refused(format!("{tag} has no channel left to draw.")));
        }
        let pins = pins_for(doc, (!new).then_some(card), t, n, next, &roles, &mut used);
        let j = Join { index: next, pins: roles.iter().copied().zip(pins.clone()).collect() };
        // The cell's first landing point sits on its crossing, the cell turned as its wire runs.
        let (role0, m0) = l.points.first().copied().ok_or(CmdError::refused(NO_POINT))?;
        let local = role_at(def_terms, role0).ok_or(CmdError::refused(NO_ROLE))?;
        let turn = Placement { origin: Pt::new(0.0, 0.0), rotation: l.rotation, mirror: false, scale: 1.0 }.apply(local);
        let pl = Placement { origin: Pt::new(m0.at.x - turn.x, m0.at.y - turn.y), rotation: l.rotation, mirror: false, scale: 1.0 };
        let cell = plan_cell(ids, page, style.clone(), def, starter, card, &j, pl, &mut ops)?;
        for (role, m) in &l.points {
            let at = role_at(def_terms, role).map(|q| pl.apply(q)).ok_or(CmdError::refused(NO_ROLE))?;
            let on = crate::cmd::terminal::on_wire_of(doc, page, m.conductor, at).ok_or(CmdError::refused("A crossed wire has no route here."))?;
            let tid = cell.terminals.iter().find(|x| x.0 == *role).map(|x| x.1).ok_or(CmdError::refused(NO_POINT))?;
            let land = land_cell(doc, ids, page, on, tid)
                .map_err(|e| CmdError::refused(format!("{e} (wire {})", crate::slash::wire_name(doc, m.conductor))))?;
            ops.extend(land);
        }
        placed.push(next);
        all_pins.extend(pins);
        next += 1;
    }
    Ok(ChannelsPlan { ops, card, tag, ty: t, starter: starter.key, placed, first, pins: all_pins })
}

/// The rest (design 045 §4.5): every channel of `c` not drawn yet, as cells in a row to the right
/// of its last cell on `page`, or at `at`. No wires.
///
/// # Errors
/// Every channel is drawn, or no cell is on the page and no `at`.
pub fn plan_rest(
    doc: &Document, ids: &mut IdAllocator, page: PageId, style: Style, c: DeviceId, t: CardType, at: Option<Pt>,
) -> Result<ChannelsPlan, CmdError> {
    let tag = crate::devices::tag_words(doc, c);
    let n = channel_count(doc, c, t);
    let pts = points(doc, c);
    let missing: Vec<u32> = (0..n).filter(|i| !pts.contains_key(i)).collect();
    let first = first_number(doc, c);
    if missing.is_empty() {
        return Err(CmdError::refused(format!("Every channel of {tag} is drawn ({n}).")));
    }
    let starter = crate::symbols::by_key(t.cell()).ok_or(CmdError::refused("No channel symbol for that type."))?;
    let width = starter.size().0;
    // To the right of the card's last cell on this page, else at `at`, else under the body.
    let start = match at {
        Some(q) => q,
        None => {
            let mut right: Option<Pt> = None;
            for (_, r) in doc.representations_of(Subject::Device(c)).filter(|(_, r)| r.page == page) {
                let Some(st) = crate::devices::rep_starter(doc, r) else { continue };
                if crate::plc::channels::cell_type(st).is_none() {
                    continue;
                }
                if let Some((_, pl)) = crate::devices::rep_definition(doc, r) {
                    let q = Pt::new(pl.origin.x + width, pl.origin.y);
                    if right.is_none_or(|x| q.x > x.x) {
                        right = Some(q);
                    }
                }
            }
            right.ok_or(CmdError::refused("No cell of this card is on this page: give `at`, the first cell's place."))?
        }
    };
    let mut ops = Vec::new();
    let def = crate::cmd::terminal::definition_for(doc, ids, starter, &mut ops)?;
    let roles = roles_of(starter);
    let mut used = crate::devices::labels_of(doc, c);
    for (k, i) in missing.iter().enumerate() {
        let pins = pins_for(doc, Some(c), t, n, *i, &roles, &mut used);
        let j = Join { index: *i, pins: roles.iter().copied().zip(pins).collect() };
        let dx = width * k as f64;
        let pl = Placement { origin: Pt::new(start.x + dx, start.y), rotation: Radians(0.0), mirror: false, scale: 1.0 };
        plan_cell(ids, page, style.clone(), def, starter, c, &j, pl, &mut ops)?;
    }
    Ok(ChannelsPlan { ops, card: c, tag, ty: t, starter: starter.key, placed: missing, first, pins: Vec::new() })
}

/// What `plcchannel` is given (besides its card).
#[derive(Debug, Clone, Default)]
pub struct ChannelInput {
    /// The channel number as shown.
    pub channel: Option<u32>,
    /// The 0-based index, standing for `channel` when that is absent.
    pub index: Option<u32>,
    /// `text`, `address`, `tag` and `safe` as given, in that order: null (`None`) clears.
    pub values: Vec<(&'static str, Option<String>)>,
    /// `{role: pin}`; `Some(None)`: given as null.
    pub pins: Option<Option<BTreeMap<String, String>>>,
    /// The channel number a drawn copy moves to, as text; `Some(None)`: given as null.
    pub move_to: Option<Option<String>>,
    /// The drawn copy to move (absent: the last drawn).
    pub terminal: Option<TerminalId>,
}

/// What `plcchannel` changes.
#[derive(Debug, Clone)]
pub struct ChannelPlan {
    pub card: DeviceId,
    /// The channel number as shown.
    pub shown: u32,
    pub ops: Vec<Op>,
    /// Each change in words (`text (none) → "Start"`).
    pub said: Vec<String>,
    /// The card's tag.
    pub who: String,
}

/// `plcchannel {card, channel, text?, address?, tag?, safe?, pins?, move_to?, terminal?}` (design
/// 045 §4.6): one channel's data. `move_to` (a Checks fix) gives the points of one drawn copy of
/// the channel (`terminal`, or the last-drawn copy) the channel numbered `move_to`.
///
/// # Errors
/// No such card or channel, a value that does not read, a duplicate tag or pin, or a channel drawn
/// already.
pub fn plan_channel(doc: &Document, card: &str, input: &ChannelInput) -> Result<ChannelPlan, CmdError> {
    let (c, t) = channel_card(doc, card)?;
    let who = crate::devices::tag_words(doc, c);
    let first = first_number(doc, c);
    let shown = input.channel.or(input.index.map(|i| i.saturating_add(first))).unwrap_or(first);
    let i = shown.checked_sub(first).ok_or_else(|| CmdError::refused(format!("{who}'s channels start at {first}.")))?;
    let n = channel_count(doc, c, t);
    let pts = points(doc, c);
    if i >= n && !pts.contains_key(&i) {
        return Err(CmdError::refused(format!("{who} has {n} channels ({first}–{}).", (n + first).saturating_sub(1))));
    }
    let props = doc.devices.get(&c).map(|x| &x.properties);
    let mut ops = Vec::new();
    let mut said = Vec::new();
    for (what, given) in &input.values {
        let value = match given {
            None => None,
            // Brief 056 (design 056 §4.4): the safe state by the card's type.
            Some(x) if *what == signals::SAFE => signals::safe_value(Some(t), Some(x))?,
            Some(x) => ch_value(what, Some(x))?,
        };
        // Brief 056 (design 056 §4.3): a tag is unique in its PLC, compared without case.
        if *what == signals::TAG
            && let Some(tag) = &value
            && let Some(holder) = signals::tag_holder(doc, c, i, tag)
        {
            return Err(CmdError::refused(signals::duplicate_words(doc, tag, holder)));
        }
        let k = ch_key(i, what);
        let before = props.and_then(|p| p.get(&k)).cloned();
        if before != value {
            said.push(format!(
                "{what} {} → {}",
                before.as_deref().map_or("(none)".to_string(), |b| format!("\"{b}\"")),
                value.as_deref().map_or("(none)".to_string(), |a| format!("\"{a}\""))
            ));
            ops.push(Op::SetProperty { object: ObjectRef::Device(c), key: k, before, after: value });
        }
    }
    if let Some(pins) = &input.pins {
        let m = pins.as_ref().ok_or(CmdError::input("`pins` must be {role: pin}"))?;
        let mine = pts.get(&i).cloned().unwrap_or_default();
        let mut labels = crate::devices::labels_of(doc, c);
        for (role, pin) in m {
            let pin = pin.trim();
            if pin.is_empty() || pin.chars().count() > MAX_PIN || pin.chars().any(|ch| ch.is_whitespace() || ch.is_control()) {
                return Err(CmdError::refused(format!("A pin is 1 to {MAX_PIN} characters, with no spaces.")));
            }
            let Some((_, tid, old)) = mine.iter().find(|(r, _, _)| *r == role.as_str()) else {
                return Err(CmdError::refused(format!("Channel {shown} of {who} has no {role} point.")));
            };
            if old == pin {
                continue;
            }
            if labels.contains(pin) {
                let holder = pts.iter().flat_map(|(j, v)| v.iter().map(move |(r, _, l)| (*j, *r, l))).find(|(_, _, l)| *l == pin);
                let what = holder.map_or_else(String::new, |(j, r, _)| format!(" (channel {}, {r})", j + first));
                return Err(CmdError::refused(format!("{who} already has pin {pin}{what}. Pick another label.")));
            }
            labels.remove(old);
            labels.insert(pin.to_string());
            let term = doc.terminals.get(tid).cloned().ok_or(CmdError::NotFound(ecad_model::Noun::Point))?;
            let after = Terminal { label: Some(pin.to_string()), ..term.clone() };
            ops.push(Op::RemoveTerminal { id: *tid, terminal: term });
            ops.push(Op::AddTerminal { id: *tid, terminal: after });
            said.push(format!("{role} pin {old} → {pin}"));
        }
    }
    if let Some(to) = &input.move_to {
        let to = match to {
            Some(t) => channel_number(t)?,
            None => return Err(CmdError::refused("`move_to` is a channel number")),
        };
        let j = to.checked_sub(first).ok_or(CmdError::NotFound(ecad_model::Noun::Channel))?;
        if pts.contains_key(&j) {
            return Err(CmdError::refused(format!("Channel {to} of {who} is drawn already.")));
        }
        let copies = crate::plc::copies(doc, c, i);
        let copy = match input.terminal {
            Some(t) => copies.iter().find(|v| v.contains(&t)).cloned(),
            None => copies.last().cloned(),
        }
        .ok_or(CmdError::refused("That channel has no drawn copy to move."))?;
        for tid in copy {
            let term = doc.terminals.get(&tid).cloned().ok_or(CmdError::NotFound(ecad_model::Noun::Point))?;
            let role = point_of(doc, tid).map(|x| x.1).ok_or(CmdError::refused("Not a channel point."))?;
            let after = Terminal { key: Some(crate::plc::channels::key(j, role)), ..term.clone() };
            ops.push(Op::RemoveTerminal { id: tid, terminal: term });
            ops.push(Op::AddTerminal { id: tid, terminal: after });
        }
        said.push(format!("moved to channel {to}"));
    }
    Ok(ChannelPlan { card: c, shown, ops, said, who })
}

// ---------------------------------------------------------------- a card's page

/// The first cell's distance from the left of the drawing area, mm (design 051 §3.6).
pub const LEFT_MM: f64 = 40.0;
/// From one row of cells to the next, mm: a card's frame (about 40 deep) and 30 clear under it for
/// its rotated channel texts (design 051 §3.6, "30 apart").
pub const ROW_PITCH_MM: f64 = 70.0;
/// Most characters of a page number `after` names (longer cannot be a page).
const MAX_AFTER: usize = 64;

/// `plc.page.title` filled in for `card`: `{tag}`, `{type}`, `{channels}`, `{rack}`, `{slot}`,
/// `{slot:02}`.
pub fn title(doc: &Document, card: DeviceId, ty: CardType) -> String {
    let pattern = defaults::resolve(doc, &defaults::plc::PAGE_TITLE, defaults::Scope::project()).text();
    let (rack, slot) = match (rack::rack_ref(doc, card), rack::slot_of(doc, card)) {
        (RackRef::Rack(r), s) => (crate::devices::tag_words(doc, r), s),
        (_, s) => (String::new(), s),
    };
    let channels = if ty.has_channels() { rack::channel_count(doc, card, ty).to_string() } else { String::new() };
    let t = pattern
        .replace("{tag}", &crate::devices::tag_words(doc, card))
        .replace("{type}", ty.code())
        .replace("{channels}", &channels)
        .replace("{rack}", &rack)
        .replace("{slot:02}", &slot.map(|s| format!("{s:02}")).unwrap_or_default())
        .replace("{slot}", &slot.map(|s| s.to_string()).unwrap_or_default());
    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.is_empty() { crate::devices::tag_words(doc, card) } else { t }
}

/// Whether `c` has a card body drawn anywhere.
fn body_drawn(doc: &Document, c: DeviceId) -> bool {
    doc.representations_of(Subject::Device(c)).any(|(_, r)| crate::devices::rep_starter(doc, r).is_some_and(|s| CardType::of_body(s.key).is_some()))
}

/// What a card's page shows (design 051 §3.6).
#[derive(Debug, Clone)]
pub struct PageCard {
    pub card: DeviceId,
    pub ty: CardType,
    /// The card's tag.
    pub tag: String,
    /// Its channel count (0 without channels).
    pub n: u32,
    /// The channel indexes not drawn yet.
    pub missing: Vec<u32>,
    /// Whether the body is drawn too (it is drawn nowhere yet).
    pub body: bool,
}

/// The card `card` names and what its page will show.
///
/// # Errors
/// No such card, a rack, or a card already drawn whole.
pub fn page_card(doc: &Document, card: &str) -> Result<PageCard, CmdError> {
    let card = card_named(doc, card)?;
    let ty = card_type_of(doc, card).ok_or(CmdError::refused("Not a PLC card."))?;
    let tag = crate::devices::tag_words(doc, card);
    if ty.is_rack() {
        return Err(CmdError::refused(format!("{tag} is a rack; Make its page is for a card.")));
    }
    let n = if ty.has_channels() { rack::channel_count(doc, card, ty) } else { 0 };
    let pts = points(doc, card);
    let missing: Vec<u32> = (0..n).filter(|i| !pts.contains_key(i)).collect();
    let body = !body_drawn(doc, card);
    if missing.is_empty() && !body {
        let pages = crate::plc::overview::pages_of(doc, card);
        let at = pages.first().map(|p| format!(" ({})", crate::references::page_number_words(doc, *p))).unwrap_or_default();
        return Err(CmdError::refused(if ty.has_channels() {
            format!("All {n} channels of {tag} are drawn{at}.")
        } else {
            format!("{tag} is drawn{at}.")
        }));
    }
    Ok(PageCard { card, ty, tag, n, missing, body })
}

/// The page a card's new page goes after: `after` (a page number as shown), else the last page on
/// which a card of the same rack (or the rack) is drawn, else the last schematic page.
///
/// # Errors
/// An `after` that names no page.
pub fn after_page(doc: &Document, book: &Book, card: DeviceId, after: Option<&str>) -> Result<Option<PageId>, CmdError> {
    if let Some(n) = after {
        let n = n.trim();
        if n.is_empty() || n.chars().count() > MAX_AFTER {
            return Err(CmdError::refused("`after` is a page number such as \"107\"."));
        }
        return book.order.iter().copied().find(|p| book.number(doc, *p) == n).map(Some).ok_or_else(|| CmdError::refused(format!("No page {n}.")));
    }
    // The last page on which a card of the same rack (or the rack) is drawn.
    let mut mates: Vec<DeviceId> = Vec::new();
    if let RackRef::Rack(r) = rack::rack_ref(doc, card) {
        mates.push(r);
        mates.extend(rack::cards_naming(doc, r).into_iter().map(|(c, _)| c));
    }
    let pages: BTreeSet<PageId> =
        mates.iter().flat_map(|c| doc.representations_of(Subject::Device(*c)).map(|(_, r)| r.page).collect::<Vec<_>>()).collect();
    if let Some(p) = book.order.iter().rev().find(|p| pages.contains(p)) {
        return Ok(Some(*p));
    }
    // Else the last schematic (not generated) page.
    Ok(book.order.iter().rev().find(|p| crate::generated::kind_of(doc, **p).is_none()).copied())
}

/// The cells and body of card page `id` (not yet in the document; `ops` add it), appended to
/// `ops` (design 051 §3.6): cells start [`LEFT_MM`] from the left of the drawing area at 60 % of
/// its height (outputs at 180°, at 40 %); a row wider than the drawing area wraps into further rows
/// [`ROW_PITCH_MM`] apart; the body goes after the last cell, its points on the cells' line.
///
/// # Errors
/// A symbol that cannot be built, or the body's placement refused.
#[allow(clippy::too_many_arguments)]
pub fn plan_page_contents(
    ledger: &Ledger, ids: &mut IdAllocator, high: &mut TagHigh, id: PageId, style: Style, pc: &PageCard, ops: &mut Vec<Op>,
) -> Result<(), CmdError> {
    let doc = ledger.document();
    let (card, ty) = (pc.card, pc.ty);
    // The drawing area, in model units.
    let spec = ecad_model::sheet::sheet_of(doc, id);
    let k = ecad_model::sheet::unit_scale(doc.settings.length_unit);
    // Brief 055 (FP4): the frame's margins and its block's height.
    let m = ecad_model::sheet::margins_of(doc, id);
    let (l, b, r, t) = (
        m.left + 5.0,
        m.bottom + ecad_model::sheet::keep_out(doc, id) + 5.0,
        f64::from(spec.width) - m.right - 5.0,
        f64::from(spec.height) - m.top - 5.0,
    );
    let output = crate::plc::address::shows_output(ty);
    let (y0, degrees) = if output { (b + 0.4 * (t - b), 180.0_f64) } else { (b + 0.6 * (t - b), 0.0) };
    let mut x_next = l + LEFT_MM;
    let mut y = y0;
    let mut cell_point: Option<Pt> = None;
    if !pc.missing.is_empty() {
        let starter = crate::symbols::by_key(ty.cell()).ok_or(CmdError::refused("No channel symbol for that type."))?;
        let def = crate::cmd::terminal::definition_for(doc, ids, starter, ops)?;
        let w = starter.size().0;
        let per_row = (((r - l - LEFT_MM) / w).floor() as usize).max(1);
        let roles = roles_of(starter);
        let mut used = crate::devices::labels_of(doc, card);
        let local = starter.definition()?.terminals.first().map(|td| td.at).unwrap_or(Pt::new(0.0, 0.0));
        for (j, i) in pc.missing.iter().enumerate() {
            let col = j % per_row;
            if j > 0 && col == 0 {
                y -= ROW_PITCH_MM;
            }
            let x = l + LEFT_MM + w * col as f64 + w / 2.0;
            let pins = pins_for(doc, Some(card), ty, pc.n, *i, &roles, &mut used);
            let join = Join { index: *i, pins: roles.iter().copied().zip(pins).collect() };
            let pl = Placement { origin: Pt::new(x * k, y * k), rotation: Radians::from(Degrees(degrees)), mirror: false, scale: 1.0 };
            plan_cell(ids, id, style.clone(), def, starter, card, &join, pl, ops)?;
            x_next = x + w / 2.0;
            cell_point = Some(pl.apply(local));
        }
    }
    if pc.body {
        let starter = crate::symbols::by_key(ty.body()).ok_or(CmdError::refused("No body symbol for that type."))?;
        let def = starter.definition()?;
        let (bw, _) = starter.size();
        // Its points on the line of the cells' points (it is the run's power end).
        let local = def.terminals.first().map(|td| td.at).unwrap_or(Pt::new(0.0, 0.0));
        let rot = Radians::from(Degrees(degrees));
        let x = x_next + bw / 2.0;
        let origin = match cell_point {
            Some(q) => {
                let lp = Placement { origin: Pt::new(0.0, 0.0), rotation: rot, mirror: false, scale: 1.0 }.apply(local);
                Pt::new(x * k, q.y - lp.y)
            }
            None => Pt::new(x * k, y * k),
        };
        let pl = Placement { origin, rotation: rot, mirror: false, scale: 1.0 };
        let plan = crate::devices::plan_place(ledger, ids, high, id, style, starter, pl, crate::devices::Target::Join(card))?;
        ops.extend(plan.ops);
    }
    Ok(())
}

/// The message of a made card page.
pub fn page_message(number: &str, title: &str, tag: &str, first: u32, missing: &[u32]) -> String {
    let what = if missing.is_empty() { "its body".to_string() } else { format!("{tag} {}", channels_words(first, missing)) };
    format!("Made page {number} \"{title}\" with {what}.")
}
