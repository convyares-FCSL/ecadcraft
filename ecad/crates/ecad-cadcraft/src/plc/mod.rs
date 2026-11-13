//! PLC racks and cards (brief 045, design 045): racks with numbered slots, cards in them, and the
//! kept [`Plc`] model the commands, Checks and lists read.
//!
//! # Semantics
//! - No new object and no new op: a rack is the device whose head is a rack body, a card a device
//!   whose head is another card body; slots and channel counts are card properties ([`rack`]).
//! - The model is kept (like 028's strips): built on first show ([`Plc::build`]) and brought up to
//!   date from each transaction's ops only ([`Plc::update`], design 045 §11.1's update map). An op
//!   the map does not list recomputes nothing.
//!
//! Excludes: the PLC manager and the rack overview (brief 2.8), any PLC-vendor knowledge.

pub mod address;
pub mod channels;
pub mod rack;

use std::collections::{BTreeMap, BTreeSet};

use cadcraft_engine::CommandSpec;
use ecad_model::{ComponentId, Document, ObjectRef, Op, PageId, Subject};
use serde_json::{Value, json};

use crate::checks::{Check, Fix, Kind, Object};
use rack::{CardType, RackRef};

/// The commands of brief 045.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("plcslot", "Card Slot", rack::run_plcslot)
            .params(
                "{card: id or tag, rack?: id or tag (null: take the card out of its rack; absent: its own rack, else the only one), \
slot?: number (absent: the next free slot), swap?: bool (swap with the card in that slot)}",
            )
            .noundo(),
        CommandSpec::new("plcracks", "PLC Racks", rack::run_plcracks)
            .params("{rack?: id or tag} (each rack with its slots in order, and the cards in no rack; read only)")
            .noundo(),
        CommandSpec::new("plcchannels", "Place Channels", channels::run_plcchannels)
            .params(
                "{card?: id or tag (absent: a new card), type?: DI|DQ|AI|AO|RTD|IOL (a new card's), first?: channel number, from: [x,y], to: [x,y]} \
(one channel cell on each wire the line crosses, in order; at most 256), or {card, rest: true, at?: [x,y]} (every channel not drawn yet, \
as cells in a row, no wires)",
            )
            .noundo(),
        CommandSpec::new("plcchannel", "Channel", channels::run_plcchannel)
            .params(
                "{card: id or tag, channel: number as shown, text?: description (null clears), address?: typed address (null clears), \
pins?: {role: pin}, move_to?: channel number (with terminal?: id; a drawn copy takes that channel)}",
            )
            .noundo(),
        CommandSpec::new("plcaddress", "PLC Addresses", address::run_plcaddress)
            .params(
                "{rack: id or tag, style?: byte.bit|iec|slot|none, input_start?: byte, output_start?: byte, fix?: bool (pin every card at its \
current start), clear?: bool (unpin every card)} or {card: id or tag, start?: byte (null unpins), size?: bytes (null: by type)}",
            )
            .noundo(),
    ]
}

/// What a PLC Checks row's buttons do (design 045 §9), with [`Fix::Plc`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PlcFix {
    /// `slot-twice`: **Move to slot n** (the next free slot of its rack).
    NextFreeSlot,
    /// `rack-missing`: **Put in rack…**, **Take out of rack**.
    RackMissing,
    /// `slot-range`: **Raise the rack's slots**, **Move to slot n**.
    SlotRange,
    /// `unracked`: **Show** the cards in no rack, each with **Put in rack…**.
    Unracked,
    /// `channel-twice`: **Move to channel n** (the channel's index).
    ChannelTwice(u32),
    /// `channel-over`: **Raise to n channels** (the highest drawn index).
    ChannelOver(u32),
    /// `address-overlap`: **Move after** (pin at this byte), **Unpin**.
    Overlap(u32),
    /// `address-overflow`: **Unpin**.
    Unpin,
    /// `address-twice`: **Clear the typed address** (the channel's index).
    ClearTyped(u32),
    /// `no-address`: **Show** the racks with style `none`.
    NoAddress,
}

/// The type of device `c` as a PLC card or rack: its card body's, else (a card drawn only by
/// channel symbols) its first channel symbol's (design 045 §3.2).
pub fn card_type_of(doc: &Document, c: ComponentId) -> Option<CardType> {
    let mut by_channel = None;
    for (_, r) in doc.representations_of(Subject::Component(c)) {
        let Some(s) = crate::devices::rep_starter(doc, r) else { continue };
        if let Some(t) = CardType::of_body(s.key) {
            return Some(t);
        }
        if by_channel.is_none() {
            by_channel = channels::cell_type(s);
        }
    }
    by_channel
}

/// Every racked card's block, read in one pass (for counting what a change moves).
pub fn all_blocks(doc: &Document) -> BTreeMap<ComponentId, address::Block> {
    let racks: BTreeSet<ComponentId> = doc
        .components
        .values()
        .filter_map(|x| x.properties.get(rack::RACK_KEY))
        .filter_map(|v| ComponentId::parse(v.trim()))
        .filter(|r| rack::is_rack(doc, *r))
        .collect();
    racks.into_iter().flat_map(|r| address::snapshot(doc, r)).collect()
}

/// The cards whose automatic addresses a move of `card` may shift, before it (a snapshot to compare
/// with [`moved_cards`]).
pub fn moved_blocks_before(doc: &Document, _card: ComponentId) -> BTreeMap<ComponentId, address::Block> {
    all_blocks(doc)
}

/// How many cards other than `card` changed address since `before`.
pub fn moved_cards(doc: &Document, card: ComponentId, before: &BTreeMap<ComponentId, address::Block>) -> usize {
    let after = all_blocks(doc);
    let keys: BTreeSet<&ComponentId> = before.keys().chain(after.keys()).collect();
    keys.into_iter()
        .filter(|c| **c != card)
        .filter(|c| before.get(c).map(|b| (b.input, b.output)) != after.get(c).map(|b| (b.input, b.output)))
        .count()
}

/// The drawn copies of channel `i` of card `c`: the points of each symbol showing it.
pub fn copies(doc: &Document, c: ComponentId, i: u32) -> Vec<Vec<ecad_model::TerminalId>> {
    let mut by_rep: BTreeMap<ecad_model::RepresentationId, Vec<ecad_model::TerminalId>> = BTreeMap::new();
    let mut loose = Vec::new();
    for (t, _) in doc.terminals_of(c) {
        if channels::point_of(doc, t).is_some_and(|(j, _)| j == i) {
            match doc.representations_showing(t).next() {
                Some((rid, _)) => by_rep.entry(rid).or_default().push(t),
                None => loose.push(t),
            }
        }
    }
    let mut out: Vec<Vec<ecad_model::TerminalId>> = by_rep.into_values().collect();
    if !loose.is_empty() {
        out.push(loose);
    }
    out
}

/// One channel of a card in the kept model (design 045 §4, §6).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChannelData {
    /// (role, terminal, pin), roles in cell order; a role twice when the channel is drawn twice.
    pub points: Vec<(&'static str, ecad_model::TerminalId, String)>,
    /// The symbols drawing it: (representation, page, the symbol's card type).
    pub cells: Vec<(ecad_model::RepresentationId, PageId, Option<CardType>)>,
    /// A point is on a net with another connection point.
    pub used: bool,
    pub text: Option<String>,
    pub typed: Option<String>,
}

/// A channel's state (design 045 §6).
pub fn state_word(used: bool, text: Option<&str>) -> &'static str {
    match (used, text.is_some_and(|t| !t.is_empty())) {
        (true, _) => "used",
        (false, true) => "reserved",
        (false, false) => "spare",
    }
}

/// A rack in the kept model.
#[derive(Debug, Clone, PartialEq)]
pub struct RackData {
    pub id: ComponentId,
    pub tag: String,
    pub ty: CardType,
    /// Its first slot and how many slots.
    pub first: u32,
    pub slots: u32,
    /// The page of its body (go-to).
    pub page: Option<PageId>,
}

/// A card in the kept model (a device with a card body that is not a rack).
#[derive(Debug, Clone, PartialEq)]
pub struct CardData {
    pub id: ComponentId,
    pub tag: String,
    pub ty: CardType,
    pub rack: RackRef,
    pub slot: Option<u32>,
    pub channels: u32,
    pub page: Option<PageId>,
    pub part_number: Option<String>,
    pub maker: Option<String>,
    /// Brief 045b: its channels with points, text or a typed address, by index.
    pub chans: BTreeMap<u32, ChannelData>,
    /// Its first channel number (`plc.channel.first`).
    pub first: u32,
}

/// The group a Checks row belongs to: a rack (its cards' slot rows) or a card (its own rows).
type Group = ComponentId;

/// The kept PLC model (design 045 §11.1).
#[derive(Debug, Clone, Default)]
pub struct Plc {
    pub racks: BTreeMap<ComponentId, RackData>,
    pub cards: BTreeMap<ComponentId, CardData>,
    /// The cards whose `plc.rack` holds each ID (valid or not): a rack's cards without a scan.
    naming: BTreeMap<ComponentId, BTreeSet<ComponentId>>,
    /// Checks rows by group, worked out for the groups an update touched.
    checks: BTreeMap<Group, Vec<Check>>,
    /// Cards in no rack (the `unracked` count row).
    unracked: BTreeSet<ComponentId>,
    /// Brief 045b: each racked card's address block (design 045 §5.1), worked out per rack.
    pub blocks: BTreeMap<ComponentId, address::Block>,
    /// Used channels without an address, by rack (the `no-address` count row).
    no_address: BTreeMap<Group, usize>,
    /// The racks and cards the last update recomputed.
    pub last_changed: BTreeSet<ComponentId>,
    /// How many racks and cards the last update recomputed (tests and bench).
    pub last_work: usize,
    /// How many racks and cards were recomputed in all (tests: an unrelated edit adds none).
    pub recomputed: u64,
}

impl PartialEq for Plc {
    fn eq(&self, o: &Self) -> bool {
        self.racks == o.racks
            && self.cards == o.cards
            && self.naming == o.naming
            && self.checks == o.checks
            && self.unracked == o.unracked
            && self.blocks == o.blocks
            && self.no_address == o.no_address
    }
}

/// The page of device `c`'s first representation.
fn first_page(doc: &Document, c: ComponentId) -> Option<PageId> {
    let pages: BTreeSet<PageId> = doc.representations_of(Subject::Component(c)).map(|(_, r)| r.page).collect();
    doc.page_order.iter().find(|p| pages.contains(p)).copied()
}

impl Plc {
    /// The whole model, from scratch (open, reload, first show).
    pub fn build(doc: &Document) -> Plc {
        let mut p = Plc::default();
        let all: Vec<ComponentId> = doc.tagged().map(|(_, c)| c).collect();
        let mut groups = BTreeSet::new();
        for c in all {
            p.set(doc, c, &mut groups);
        }
        for g in groups {
            p.set_checks(doc, g);
        }
        p.last_changed.clear();
        p.last_work = 0;
        p
    }

    /// Whether the kept model equals `other` (verify mode).
    pub fn same_as(&self, other: &Plc) -> bool {
        self == other
    }

    /// Recompute device `c` (a rack, a card, or neither now) and note the check groups it touches.
    fn set(&mut self, doc: &Document, c: ComponentId, groups: &mut BTreeSet<Group>) {
        let ty = doc.components.get(&c).and_then(|_| card_type_of(doc, c));
        if ty.is_none() && !self.cards.contains_key(&c) && !self.racks.contains_key(&c) {
            // Neither before nor now: nothing to do, and no work counted.
            return;
        }
        self.recomputed += 1;
        self.last_work += 1;
        // Out with the old.
        self.blocks.remove(&c);
        if let Some(old) = self.cards.remove(&c) {
            if let RackRef::Rack(r) | RackRef::Missing(r) = old.rack {
                groups.insert(r);
                if let Some(set) = self.naming.get_mut(&r) {
                    set.remove(&c);
                    if set.is_empty() {
                        self.naming.remove(&r);
                    }
                }
            }
            self.unracked.remove(&c);
            groups.insert(c);
            self.last_changed.insert(c);
        }
        if self.racks.remove(&c).is_some() {
            groups.insert(c);
            self.last_changed.insert(c);
        }
        let Some(ty) = ty else { return };
        let tag = crate::devices::tag_words(doc, c);
        let page = first_page(doc, c);
        self.last_changed.insert(c);
        groups.insert(c);
        if ty.is_rack() {
            let (first, slots) = rack::slot_range(doc, c);
            self.racks.insert(c, RackData { id: c, tag, ty, first, slots, page });
            return;
        }
        let r = rack::rack_ref(doc, c);
        let props = doc.components.get(&c).map(|x| &x.properties);
        let text = |k: &str| props.and_then(|p| p.get(k)).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        if let RackRef::Rack(x) | RackRef::Missing(x) = r {
            self.naming.entry(x).or_default().insert(c);
            groups.insert(x);
        }
        if r == RackRef::None {
            self.unracked.insert(c);
        }
        self.cards.insert(
            c,
            CardData {
                id: c,
                tag,
                ty,
                rack: r,
                slot: rack::slot_of(doc, c),
                channels: rack::channel_count(doc, c, ty),
                page,
                part_number: text("part_number"),
                maker: text("maker"),
                chans: channel_data(doc, c),
                first: channels::first_number(doc, c),
            },
        );
    }

    /// The devices `ops` (the transaction just committed on `doc`) touch, by design 045 §11.1's
    /// map; anything not listed touches nothing.
    pub fn touched(&self, doc: &Document, ops: &[Op]) -> BTreeSet<ComponentId> {
        let mut out = BTreeSet::new();
        let mut all = false;
        for op in ops {
            match op {
                Op::CreateComponent { id, .. } | Op::DeleteComponent { id, .. } => {
                    out.insert(*id);
                }
                Op::SetProperty { object: ObjectRef::Component(c), key, .. } => {
                    if key.starts_with("plc.") || key == ecad_model::tag::TAG_KEY || key == "part_number" || key == "maker" {
                        if self.cards.contains_key(c) || self.racks.contains_key(c) || key.starts_with("plc.") {
                            out.insert(*c);
                        }
                        // A card naming this device as its rack reads its tag and slots.
                        out.extend(self.naming.get(c).into_iter().flatten().copied());
                    }
                }
                Op::SetProperty { object: ObjectRef::Project, key, .. } if key.starts_with("plc.") => all = true,
                Op::LinkRepresentation { rep, .. } | Op::UnlinkRepresentation { rep, .. } => {
                    if let Subject::Component(c) = rep.subject {
                        out.insert(c);
                    }
                }
                // Brief 045b: a card's channel points, and the nets they are on (used or spare).
                Op::AddTerminal { terminal, .. } | Op::RemoveTerminal { terminal, .. } => {
                    out.insert(terminal.component);
                }
                Op::AttachEnd { conductor, before, after, .. } => {
                    for t in [before, after].into_iter().flatten() {
                        if let Some(x) = doc.terminals.get(t) {
                            out.insert(x.component);
                        }
                    }
                    let conn = doc.connectivity();
                    if let Some(net) = conn.net_of_conductor(*conductor).and_then(|n| conn.net(n)) {
                        out.extend(
                            net.terminals.iter().filter_map(|t| doc.terminals.get(t)).map(|x| x.component).filter(|c| self.cards.contains_key(c)),
                        );
                    }
                }
                _ => {}
            }
        }
        if all {
            // A project default: only the racks and cards that inherit it (they have no own value
            // of a `plc.` key read at object level; reading all of them is bounded by the model).
            let keys: Vec<&str> = ops
                .iter()
                .filter_map(|op| match op {
                    Op::SetProperty { object: ObjectRef::Project, key, .. } if key.starts_with("plc.") => Some(key.as_str()),
                    _ => None,
                })
                .collect();
            let inherits = |c: &ComponentId| doc.components.get(c).is_none_or(|x| keys.iter().any(|k| !x.properties.contains_key(*k)));
            out.extend(self.racks.keys().filter(|c| inherits(c)).copied());
            out.extend(self.cards.keys().filter(|c| inherits(c)).copied());
        }
        // Only racks and cards, before or now (a device that is neither is not touched).
        out.retain(|c| self.cards.contains_key(c) || self.racks.contains_key(c) || card_type_of(doc, *c).is_some());
        // A rack's cards read its tag and slot range.
        let racks: Vec<ComponentId> = out.iter().filter(|c| self.racks.contains_key(c)).copied().collect();
        for r in racks {
            out.extend(self.naming.get(&r).into_iter().flatten().copied());
        }
        out
    }

    /// Bring the model in line with `ops` (just committed; `doc` is the state after). Returns
    /// whether anything changed.
    pub fn update(&mut self, doc: &Document, ops: &[Op]) -> bool {
        self.last_changed.clear();
        self.last_work = 0;
        let touched = self.touched(doc, ops);
        if touched.is_empty() {
            return false;
        }
        let before: Vec<(ComponentId, Option<CardData>, Option<RackData>)> =
            touched.iter().map(|c| (*c, self.cards.get(c).cloned(), self.racks.get(c).cloned())).collect();
        let mut groups = BTreeSet::new();
        for c in &touched {
            self.set(doc, *c, &mut groups);
        }
        // Only what reads differently counts as changed.
        for (c, card, rack) in before {
            if self.cards.get(&c) == card.as_ref() && self.racks.get(&c) == rack.as_ref() {
                self.last_changed.remove(&c);
            }
        }
        let mut changed = !self.last_changed.is_empty();
        for g in groups {
            changed |= self.set_checks(doc, g);
        }
        changed
    }

    /// Work out group `g`'s Checks rows; returns whether they changed.
    fn set_checks(&mut self, doc: &Document, g: Group) -> bool {
        let mut moved = false;
        if self.racks.contains_key(&g) {
            let members: Vec<address::Member> = self.cards_of(g).into_iter().filter_map(|c| Some((c.id, c.ty, c.channels, c.slot?))).collect();
            for (c, b) in address::blocks(doc, g, &members) {
                if self.blocks.insert(c, b) != Some(b) {
                    moved = true;
                    self.last_changed.insert(c);
                }
            }
            let n = self.no_address_of(doc, g);
            if n > 0 {
                self.no_address.insert(g, n);
            } else {
                self.no_address.remove(&g);
            }
        }
        let rows = self.group_checks(doc, g);
        let old = if rows.is_empty() { self.checks.remove(&g) } else { self.checks.insert(g, rows.clone()) };
        moved || old.unwrap_or_default() != rows
    }

    /// The address of channel `i` of card `c` as shown (typed, else automatic) and whether typed.
    pub fn address_of(&self, doc: &Document, c: ComponentId, i: u32) -> (Option<String>, bool) {
        let Some(card) = self.cards.get(&c) else { return (None, false) };
        if let Some(t) = card.chans.get(&i).and_then(|x| x.typed.clone()) {
            return (Some(t), true);
        }
        (self.automatic(doc, c, i, address::shows_output(card.ty)), false)
    }

    /// The automatic address of channel `i` of card `c` (`output`: its output address).
    pub fn automatic(&self, doc: &Document, c: ComponentId, i: u32, output: bool) -> Option<String> {
        let card = self.cards.get(&c)?;
        let RackRef::Rack(r) = card.rack else { return None };
        let b = self.blocks.get(&c)?;
        let slot = card.slot?;
        let style = address::Style::of(doc, r);
        let shown = i.saturating_add(channels::first_number(doc, c));
        address::automatic(style, card.ty, b, slot, i, shown, address::word_size(doc, c, card.ty), output)
    }

    /// Used channels of rack `r`'s cards with no address.
    fn no_address_of(&self, doc: &Document, r: ComponentId) -> usize {
        let mut n = 0;
        for card in self.cards_of(r) {
            for (i, ch) in &card.chans {
                if ch.used && ch.typed.is_none() && self.automatic(doc, card.id, *i, address::shows_output(card.ty)).is_none() {
                    n += 1;
                }
            }
        }
        n
    }

    fn tag_of(&self, c: ComponentId) -> String {
        self.cards.get(&c).map(|x| x.tag.clone()).or_else(|| self.racks.get(&c).map(|x| x.tag.clone())).unwrap_or_else(|| "?".into())
    }

    /// The next free slot of rack `r` in the model.
    pub fn next_free(&self, r: ComponentId) -> Option<u32> {
        let rack = self.racks.get(&r)?;
        let taken: BTreeSet<u32> = self.naming.get(&r).into_iter().flatten().filter_map(|c| self.cards.get(c)).filter_map(|c| c.slot).collect();
        (rack.first..rack.first.saturating_add(rack.slots)).find(|s| !taken.contains(s))
    }

    fn group_checks(&self, doc: &Document, g: Group) -> Vec<Check> {
        let mut out = Vec::new();
        if let Some(card) = self.cards.get(&g) {
            self.channel_checks(doc, card, &mut out);
        }
        if let Some(card) = self.cards.get(&g)
            && matches!(card.rack, RackRef::Missing(_) | RackRef::Bad)
        {
            out.push(Check {
                kind: Kind::RackMissing,
                page: card.page,
                at: None,
                object: Object::Component(card.id),
                which: format!("{} names a rack that is not in the project", card.tag),
                fix: Some(Fix::Plc(card.id, PlcFix::RackMissing)),
            });
        }
        let Some(rack) = self.racks.get(&g) else { return out };
        let mut by_slot: BTreeMap<u32, Vec<&CardData>> = BTreeMap::new();
        let last = rack.first.saturating_add(rack.slots).saturating_sub(1);
        for card in self.naming.get(&g).into_iter().flatten().filter_map(|c| self.cards.get(c)) {
            match card.slot {
                Some(s) if (rack.first..=last).contains(&s) => by_slot.entry(s).or_default().push(card),
                Some(s) => out.push(Check {
                    kind: Kind::SlotRange,
                    page: card.page,
                    at: None,
                    object: Object::Component(card.id),
                    which: format!("{} is in slot {s}; {} has slots {}–{last}", card.tag, rack.tag, rack.first),
                    fix: Some(Fix::Plc(card.id, PlcFix::SlotRange)),
                }),
                None => out.push(Check {
                    kind: Kind::SlotRange,
                    page: card.page,
                    at: None,
                    object: Object::Component(card.id),
                    which: format!("{} is in {} but in no slot", card.tag, rack.tag),
                    fix: Some(Fix::Plc(card.id, PlcFix::SlotRange)),
                }),
            }
        }
        self.address_checks(doc, rack, &mut out);
        for (s, mut cards) in by_slot {
            if cards.len() < 2 {
                continue;
            }
            cards.sort_by(|a, b| (a.tag.as_str(), a.id).cmp(&(b.tag.as_str(), b.id)));
            let names: Vec<&str> = cards.iter().map(|c| c.tag.as_str()).collect();
            for later in cards.iter().skip(1) {
                out.push(Check {
                    kind: Kind::SlotTwice,
                    page: later.page,
                    at: None,
                    object: Object::Component(later.id),
                    which: format!("{} slot {s} holds {}", rack.tag, names.join(" and ")),
                    fix: Some(Fix::Plc(later.id, PlcFix::NextFreeSlot)),
                });
            }
        }
        out
    }

    /// Brief 045b's rows of one card (design 045 §9): a channel drawn twice, beyond the count, or by a
    /// symbol of another type.
    fn channel_checks(&self, doc: &Document, card: &CardData, out: &mut Vec<Check>) {
        let first = channels::first_number(doc, card.id);
        let place = |rid: ecad_model::RepresentationId| -> String {
            let Some(r) = doc.representations.get(&rid) else { return "?".into() };
            let at = crate::devices::rep_definition(doc, r).map(|(_, pl)| pl.origin);
            at.and_then(|a| ecad_model::wiring::place(doc, r.page, a))
                .map_or_else(|| crate::references::page_number_words(doc, r.page), |p| p.text(doc.settings.reference_style))
        };
        let mut highest: Option<u32> = None;
        for (i, ch) in &card.chans {
            if ch.points.is_empty() {
                continue;
            }
            if ch.cells.len() > 1 {
                let places: Vec<String> = ch.cells.iter().take(4).map(|(r, _, _)| place(*r)).collect();
                let (r, page, _) = ch.cells.get(1).copied().unwrap_or((ch.cells[0].0, ch.cells[0].1, None));
                let _ = r;
                out.push(Check {
                    kind: Kind::ChannelTwice,
                    page: Some(page),
                    at: None,
                    object: Object::Component(card.id),
                    which: format!("{} channel {} is drawn twice ({})", card.tag, i + first, places.join(", ")),
                    fix: Some(Fix::Plc(card.id, PlcFix::ChannelTwice(*i))),
                });
            }
            if *i >= card.channels {
                highest = Some(highest.map_or(*i, |h| h.max(*i)));
            }
            for (_, page, t) in &ch.cells {
                if let Some(t) = t
                    && *t != card.ty
                {
                    out.push(Check {
                        kind: Kind::ChannelType,
                        page: Some(*page),
                        at: None,
                        object: Object::Component(card.id),
                        which: format!("{} is a {}; channel {} is drawn as {}", card.tag, card.ty.noun(), i + first, t.code()),
                        fix: None,
                    });
                }
            }
        }
        if let Some(h) = highest {
            out.push(Check {
                kind: Kind::ChannelOver,
                page: card.page,
                at: None,
                object: Object::Component(card.id),
                which: format!("{} has channel {} drawn; it has {} channels", card.tag, h + first, card.channels),
                fix: Some(Fix::Plc(card.id, PlcFix::ChannelOver(h))),
            });
        }
    }

    /// Brief 045b's rows of one rack (design 045 §9): blocks that overlap or overflow, an address
    /// given twice.
    fn address_checks(&self, doc: &Document, rack: &RackData, out: &mut Vec<Check>) {
        let style = address::Style::of(doc, rack.id);
        let cards = self.cards_of(rack.id);
        for card in &cards {
            if self.blocks.get(&card.id).is_some_and(|b| b.overflow) {
                out.push(Check {
                    kind: Kind::AddressOverflow,
                    page: card.page,
                    at: None,
                    object: Object::Component(card.id),
                    which: format!("{}'s block passes byte {}", card.tag, address::MAX_BYTE),
                    fix: Some(Fix::Plc(card.id, PlcFix::Unpin)),
                });
            }
        }
        for output in [false, true] {
            let spans: Vec<(&CardData, (u32, u32))> = cards
                .iter()
                .filter_map(|c| {
                    let b = self.blocks.get(&c.id)?;
                    Some((*c, if output { b.output? } else { b.input? }))
                })
                .collect();
            // The row goes on the later card in slot order, naming the first earlier one it overlaps.
            for (k, (later, (s, n))) in spans.iter().enumerate() {
                let earlier = spans.iter().take(k).find(|(_, (s2, n2))| s2.saturating_add(*n2) > *s && *s2 < s.saturating_add(*n));
                if let Some((e, span)) = earlier {
                    let dir = if output { "outputs" } else { "inputs" };
                    let free = spans.iter().map(|(_, (a, b))| a.saturating_add(*b)).max().unwrap_or(0);
                    out.push(Check {
                        kind: Kind::AddressOverlap,
                        page: later.page,
                        at: None,
                        object: Object::Component(later.id),
                        which: format!(
                            "{} {dir} {} overlap {} {dir} {} ({})",
                            later.tag,
                            address::block_words(style, output, (*s, *n)),
                            e.tag,
                            address::block_words(style, output, *span),
                            rack.tag
                        ),
                        fix: Some(Fix::Plc(later.id, PlcFix::Overlap(free))),
                    });
                }
            }
        }
        // An address given twice in this rack (at least one of them typed).
        let mut seen: BTreeMap<String, Vec<(&CardData, u32, bool)>> = BTreeMap::new();
        for card in &cards {
            let typed: BTreeSet<u32> = card.chans.iter().filter(|(_, ch)| ch.typed.is_some()).map(|(i, _)| *i).collect();
            for i in 0..card.channels.max(card.chans.keys().next_back().map_or(0, |x| x + 1)) {
                let (a, t) = self.address_of(doc, card.id, i);
                if let Some(a) = a {
                    seen.entry(a).or_default().push((card, i, t || typed.contains(&i)));
                }
            }
        }
        for (a, who) in seen {
            if who.len() < 2 || !who.iter().any(|(_, _, t)| *t) {
                continue;
            }
            let words: Vec<String> = who
                .iter()
                .take(4)
                .map(|(c, i, t)| format!("{} channel {}{}", c.tag, i + channels::first_number(doc, c.id), if *t { " (typed)" } else { "" }))
                .collect();
            if let Some((c, i, _)) = who.iter().find(|(_, _, t)| *t) {
                out.push(Check {
                    kind: Kind::AddressTwice,
                    page: c.page,
                    at: None,
                    object: Object::Component(c.id),
                    which: format!("{a} is the address of {}", words.join(" and ")),
                    fix: Some(Fix::Plc(c.id, PlcFix::ClearTyped(*i))),
                });
            }
        }
    }

    /// Every PLC Checks row (design 045 §9), from the kept rows.
    pub fn checks(&self) -> Vec<Check> {
        let mut out: Vec<Check> = self.checks.values().flatten().cloned().collect();
        let n = self.unracked.len();
        if n > 0 {
            let which = if n == 1 { "1 PLC card is in no rack".to_string() } else { format!("{n} PLC cards are in no rack") };
            out.push(Check {
                kind: Kind::Unracked,
                page: None,
                at: None,
                object: Object::None,
                which,
                fix: Some(Fix::Plc(ComponentId(0), PlcFix::Unracked)),
            });
        }
        let n: usize = self.no_address.values().sum();
        if n > 0 {
            let which = if n == 1 { "1 used channel has no address".to_string() } else { format!("{n} used channels have no address") };
            out.push(Check {
                kind: Kind::NoAddress,
                page: None,
                at: None,
                object: Object::None,
                which,
                fix: Some(Fix::Plc(ComponentId(0), PlcFix::NoAddress)),
            });
        }
        out
    }

    /// The buttons of a PLC Checks row's fix, each a label, a command and its parameters (design
    /// 045 §9). Empty when the row's card is gone.
    pub fn fix_commands(&self, c: ComponentId, fix: PlcFix) -> Vec<(String, &'static str, Value)> {
        let tag = self.tag_of(c);
        match fix {
            PlcFix::NextFreeSlot => {
                let Some(card) = self.cards.get(&c) else { return Vec::new() };
                let RackRef::Rack(r) = card.rack else { return Vec::new() };
                match self.next_free(r) {
                    Some(s) => vec![(format!("Move {tag} to slot {s}"), "plcslot", json!({"card": c.hex(), "slot": s}))],
                    None => Vec::new(),
                }
            }
            PlcFix::RackMissing => vec![
                (format!("Put {tag} in rack…"), "plcslot", json!({"card": c.hex()})),
                ("Take out of rack".to_string(), "plcslot", json!({"card": c.hex(), "rack": null})),
            ],
            PlcFix::SlotRange => {
                let Some(card) = self.cards.get(&c) else { return Vec::new() };
                let RackRef::Rack(r) = card.rack else { return Vec::new() };
                let Some(rack) = self.racks.get(&r) else { return Vec::new() };
                let mut out = Vec::new();
                if let Some(s) = card.slot {
                    let n = s.saturating_sub(rack.first).saturating_add(1);
                    if n > rack.slots && n <= rack::MAX_SLOT {
                        out.push((
                            format!("Raise {} to {n} slots", rack.tag),
                            "setprop",
                            json!({"component": r.hex(), "key": crate::defaults::plc::SLOTS.key, "value": n.to_string()}),
                        ));
                    }
                }
                if let Some(s) = self.next_free(r) {
                    out.push((format!("Move to slot {s}"), "plcslot", json!({"card": c.hex(), "slot": s})));
                }
                out
            }
            PlcFix::ChannelTwice(i) => {
                let Some(card) = self.cards.get(&c) else { return Vec::new() };
                let free = (0..channels::MAX_INDEX).find(|j| !card.chans.get(j).is_some_and(|x| !x.points.is_empty())).unwrap_or(0);
                let (first, i_shown) = (self.first_of(c), i);
                vec![(
                    format!("Move to channel {}", free + first),
                    "plcchannel",
                    json!({"card": c.hex(), "channel": i_shown + first, "move_to": free + first}),
                )]
            }
            PlcFix::ChannelOver(h) => {
                let n = h.saturating_add(1);
                if n > rack::MAX_CHANNELS {
                    return Vec::new();
                }
                vec![(format!("Raise to {n} channels"), "setprop", json!({"component": c.hex(), "key": rack::CHANNELS_KEY, "value": n.to_string()}))]
            }
            PlcFix::Overlap(free) => vec![
                (format!("Move {tag} after the others (pin at byte {free})"), "plcaddress", json!({"card": c.hex(), "start": free})),
                (format!("Unpin {tag}"), "plcaddress", json!({"card": c.hex(), "start": null})),
            ],
            PlcFix::Unpin => vec![(format!("Unpin {tag}"), "plcaddress", json!({"card": c.hex(), "start": null}))],
            PlcFix::ClearTyped(i) => {
                let first = self.first_of(c);
                vec![("Clear the typed address".to_string(), "plcchannel", json!({"card": c.hex(), "channel": i + first, "address": null}))]
            }
            PlcFix::NoAddress => self
                .racks_by_tag()
                .into_iter()
                .filter(|r| self.no_address.contains_key(&r.id))
                .map(|r| (format!("Set {}'s address style", r.tag), "plcaddress", json!({"rack": r.id.hex(), "style": "byte.bit"})))
                .collect(),
            PlcFix::Unracked => self
                .unracked_by_tag()
                .into_iter()
                .map(|c| (format!("Put {} in rack…", self.tag_of(c)), "plcslot", json!({"card": c.hex()})))
                .collect(),
        }
    }

    /// The first channel number of card `c` as the model last read it.
    fn first_of(&self, c: ComponentId) -> u32 {
        self.cards.get(&c).map_or(0, |x| x.first)
    }

    /// The racks by tag.
    pub fn racks_by_tag(&self) -> Vec<&RackData> {
        let mut v: Vec<&RackData> = self.racks.values().collect();
        v.sort_by(|a, b| (a.tag.as_str(), a.id).cmp(&(b.tag.as_str(), b.id)));
        v
    }

    /// The cards in no rack (or naming one that is missing), by tag.
    pub fn unracked_by_tag(&self) -> Vec<ComponentId> {
        let mut v: Vec<&CardData> = self.cards.values().filter(|c| !matches!(c.rack, RackRef::Rack(_))).collect();
        v.sort_by(|a, b| (a.tag.as_str(), a.id).cmp(&(b.tag.as_str(), b.id)));
        v.into_iter().map(|c| c.id).collect()
    }

    /// The cards of rack `r` in slot order (then tag).
    pub fn cards_of(&self, r: ComponentId) -> Vec<&CardData> {
        let mut v: Vec<&CardData> =
            self.naming.get(&r).into_iter().flatten().filter_map(|c| self.cards.get(c)).filter(|c| c.rack == RackRef::Rack(r)).collect();
        v.sort_by(|a, b| (a.slot, a.tag.as_str(), a.id).cmp(&(b.slot, b.tag.as_str(), b.id)));
        v
    }

    /// One card as `plcracks` lists it.
    pub fn card_json(&self, _doc: &Document, c: ComponentId) -> Value {
        let Some(card) = self.cards.get(&c) else { return Value::Null };
        let rack = match card.rack {
            RackRef::Rack(r) => json!(self.tag_of(r)),
            RackRef::Missing(_) | RackRef::Bad => json!("missing"),
            RackRef::None => Value::Null,
        };
        json!({
            "card": c.hex(),
            "tag": card.tag,
            "type": card.ty.code(),
            "rack": rack,
            "slot": card.slot,
            "channels": card.channels,
            "used": card.chans.values().filter(|x| x.used).count(),
            "reserved": card.chans.values().filter(|x| !x.used && x.text.is_some()).count(),
            "spare": card.channels.saturating_sub(u32::try_from(card.chans.values().filter(|x| x.used || x.text.is_some()).count()).unwrap_or(0)),
            "inputs": self.blocks.get(&c).and_then(|b| b.input).map(|(s, n)| json!([s, s.saturating_add(n).saturating_sub(1)])),
            "outputs": self.blocks.get(&c).and_then(|b| b.output).map(|(s, n)| json!([s, s.saturating_add(n).saturating_sub(1)])),
            "pinned": self.blocks.get(&c).is_some_and(|b| b.pinned),
            "part_number": card.part_number,
            "maker": card.maker,
        })
    }

    /// One rack as `plcracks` lists it: its slots in order, free ones too.
    pub fn rack_json(&self, doc: &Document, r: &RackData) -> Value {
        let cards = self.cards_of(r.id);
        let mut slots = Vec::new();
        let last = r.first.saturating_add(r.slots).saturating_sub(1);
        for s in r.first..=last {
            let here: Vec<&&CardData> = cards.iter().filter(|c| c.slot == Some(s)).collect();
            if here.is_empty() {
                slots.push(json!({"slot": s, "free": true}));
            }
            for c in here {
                let mut v = self.card_json(doc, c.id);
                v["free"] = json!(false);
                slots.push(v);
            }
        }
        // Cards outside the range or in no slot (Checks rows), after the slots.
        for c in cards.iter().filter(|c| c.slot.is_none_or(|s| s > last || s < r.first)) {
            slots.push(self.card_json(doc, c.id));
        }
        json!({
            "rack": r.id.hex(),
            "tag": r.tag,
            "body": r.ty.body(),
            "type": r.ty.code(),
            "first": r.first,
            "slots": r.slots,
            "cards": slots,
        })
    }
}

/// The extra pages a property change on component `c` redraws (design 045 §11.1): a rack's tag or
/// slot range shows in its cards' captions.
pub fn pages_touched(doc: &Document, c: ComponentId, key: &str, after: Option<&str>) -> Vec<PageId> {
    if !(key == ecad_model::tag::TAG_KEY || key.starts_with("plc.")) {
        return Vec::new();
    }
    let pages_of = |r: ComponentId| -> Vec<PageId> {
        rack::cards_naming(doc, r)
            .into_iter()
            .flat_map(|(card, _)| doc.representations_of(Subject::Component(card)).map(|(_, x)| x.page).collect::<Vec<_>>())
            .collect()
    };
    if rack::is_rack(doc, c) {
        return pages_of(c);
    }
    // A card's own text or typed address shows on its own cells only.
    if key == ecad_model::tag::TAG_KEY || key.starts_with(channels::CH_PREFIX) {
        return Vec::new();
    }
    let mut out = Vec::new();
    if let rack::RackRef::Rack(r) = rack::rack_ref(doc, c) {
        out.extend(pages_of(r));
    }
    if key == rack::RACK_KEY
        && let Some(r) = after.and_then(|v| ComponentId::parse(v.trim())).filter(|r| rack::is_rack(doc, *r))
    {
        out.extend(pages_of(r));
    }
    out
}

/// Card `c`'s caption (design 045 §7.2) and whether it is shown: the registry pattern with the
/// rack's tag and the slot, when the card is in a rack's slot; `None` otherwise. `plc.text.caption.show`
/// is read at the card's level.
pub fn caption(doc: &Document, c: ComponentId) -> Option<(String, bool)> {
    let (rack::RackRef::Rack(r), Some(slot)) = (rack::rack_ref(doc, c), rack::slot_of(doc, c)) else { return None };
    let at = crate::defaults::Scope { component: Some(c), ..crate::defaults::Scope::project() };
    let pattern = crate::defaults::resolve(doc, &crate::defaults::plc::CAPTION, at).text();
    let shown = crate::defaults::resolve(doc, &crate::defaults::plc::CAPTION_SHOW, at).word() == "on";
    let text = pattern
        .replace("{rack}", &crate::devices::tag_words(doc, r))
        .replace("{slot:02}", &format!("{slot:02}"))
        .replace("{slot}", &slot.to_string());
    Some((text, shown))
}

/// The caption text of representation `r` of card `c` at `pl`, when `r` shows the card's body and
/// the card is in a slot.
pub fn caption_text(
    doc: &Document, c: ComponentId, rep: ecad_model::RepresentationId, r: &ecad_model::Representation, pl: &ecad_model::Placement,
) -> Option<crate::devices::PageText> {
    let s = crate::devices::rep_starter(doc, r)?;
    let ty = CardType::of_body(s.key)?;
    if ty.is_rack() {
        return None;
    }
    let (value, shown) = caption(doc, c)?;
    let anchor = crate::symbols::anchors(s).into_iter().find(|a| a.role == crate::symbols::TextRole::Caption)?;
    let at = pl.apply(anchor.at);
    if !at.is_finite() {
        return None;
    }
    let height = crate::text_style::text_style(doc, crate::text_style::TextKind::Description).height;
    Some(crate::devices::PageText {
        text: crate::devices::DerivedText {
            rep,
            of: crate::devices::TextOf::Caption,
            value,
            at,
            height,
            halign: cadcraft_engine::doc::HAlign::Left,
            valign: cadcraft_engine::doc::VAlign::Top,
            aci: None,
        },
        shown,
        moved: false,
        overlaps: false,
    })
}

// ---------------------------------------------------------------- Properties (design 045 §2.6)

/// Properties ▸ Card and ▸ Rack rows of device `c` (design 045 §2.6): a card's Rack (its tag,
/// never the ID: FP1), Slot and Channels; a rack's Slots and First slot (registry values at object
/// level, faint when inherited).
pub fn props_rows(doc: &Document, c: ComponentId, rows: &mut Vec<crate::props::Row>) {
    let Some(ty) = card_type_of(doc, c) else { return };
    let props = doc.components.get(&c).map(|x| &x.properties);
    let own = |k: &str| props.and_then(|p| p.get(k)).cloned();
    let row = |key: &str, label: &str, value: String, inherited: bool, choices: Vec<String>| crate::props::Row {
        key: key.into(),
        label: label.into(),
        value,
        editable: true,
        inherited,
        varies: false,
        choices,
    };
    if ty.is_rack() {
        let (first, n) = rack::slot_range(doc, c);
        rows.push(row(crate::defaults::plc::SLOTS.key, "Slots", n.to_string(), own(crate::defaults::plc::SLOTS.key).is_none(), Vec::new()));
        rows.push(row(
            crate::defaults::plc::SLOT_FIRST.key,
            "First slot",
            first.to_string(),
            own(crate::defaults::plc::SLOT_FIRST.key).is_none(),
            vec!["0".into(), "1".into()],
        ));
        return;
    }
    let mut choices: Vec<String> = rack::racks(doc).into_iter().take(64).map(|r| crate::devices::tag_words(doc, r)).collect();
    choices.push("None".into());
    let rack_value = match rack::rack_ref(doc, c) {
        RackRef::Rack(r) => crate::devices::tag_words(doc, r),
        RackRef::Missing(_) | RackRef::Bad => "(missing rack)".into(),
        RackRef::None => "None".into(),
    };
    rows.push(row(rack::RACK_KEY, "Rack", rack_value, false, choices));
    rows.push(row(rack::SLOT_KEY, "Slot", rack::slot_of(doc, c).map(|s| s.to_string()).unwrap_or_default(), false, Vec::new()));
    if ty.has_channels() {
        let inherited = own(rack::CHANNELS_KEY).and_then(|v| rack::parse_channels(&v)).is_none();
        rows.push(row(rack::CHANNELS_KEY, "Channels", rack::channel_count(doc, c, ty).to_string(), inherited, Vec::new()));
    }
}

/// The objects Properties ▸ Card reads besides the card: its rack (its tag is shown).
pub fn props_deps(doc: &Document, c: ComponentId) -> Option<ObjectRef> {
    match rack::rack_ref(doc, c) {
        RackRef::Rack(r) | RackRef::Missing(r) => Some(ObjectRef::Component(r)),
        _ => None,
    }
}

/// The command line a Card row writes (`plcslot`), or `None` for any other key.
pub fn edit_line(c: ComponentId, key: &str, value: Option<&str>) -> Option<String> {
    let v = value.map(str::trim).filter(|v| !v.is_empty());
    let j = match key {
        rack::RACK_KEY => match v {
            None => json!({"card": c.hex(), "rack": null}),
            Some(t) if t.eq_ignore_ascii_case("none") => json!({"card": c.hex(), "rack": null}),
            Some(t) => json!({"card": c.hex(), "rack": t}),
        },
        rack::SLOT_KEY => match v {
            None => json!({"card": c.hex()}),
            Some(t) => json!({"card": c.hex(), "slot": t}),
        },
        k => {
            // Properties ▸ Channel: a pin row (`plc.pin#<i>#<role>`) writes `plcchannel {pins}`.
            let (i, role) = k.strip_prefix("plc.pin#")?.split_once('#')?;
            let i: u32 = i.parse().ok()?;
            return Some(format!("plcchannel {}", json!({"card": c.hex(), "index": i, "pins": {role: v.unwrap_or_default()}})));
        }
    };
    Some(format!("plcslot {j}"))
}

/// Card `c`'s channels read from the document: points, cells, net state, text and typed address.
pub fn channel_data(doc: &Document, c: ComponentId) -> BTreeMap<u32, ChannelData> {
    let mut out: BTreeMap<u32, ChannelData> = BTreeMap::new();
    let conn = doc.connectivity();
    for (t, term) in doc.terminals_of(c) {
        let Some((i, role)) = channels::point_of(doc, t) else { continue };
        let ch = out.entry(i).or_default();
        ch.points.push((role, t, term.label.clone().unwrap_or_default()));
        let used = conn.net_of_terminal(t).and_then(|n| conn.net(n)).is_some_and(|n| n.terminals.len() > 1);
        ch.used |= used;
        for (rid, r) in doc.representations_showing(t) {
            if !ch.cells.iter().any(|(x, _, _)| *x == rid) {
                let ty = crate::devices::rep_starter(doc, r).and_then(channels::cell_type);
                ch.cells.push((rid, r.page, ty));
            }
        }
    }
    if let Some(x) = doc.components.get(&c) {
        for (k, v) in x.properties.range(channels::CH_PREFIX.to_string()..) {
            if !k.starts_with(channels::CH_PREFIX) {
                break;
            }
            let Some((i, what)) = channels::parse_ch_key(k) else { continue };
            let v = v.trim();
            if v.is_empty() {
                continue;
            }
            let ch = out.entry(i).or_default();
            if what == "text" {
                ch.text = Some(v.to_string());
            } else {
                ch.typed = Some(v.to_string());
            }
        }
    }
    for ch in out.values_mut() {
        ch.points.sort_by_key(|(r, t, _)| (channels::ROLES.iter().position(|x| x == r), *t));
    }
    out
}

/// The channel texts of cell `rid` (representation `r` at `pl`) of card `c` (design 045 §7.1): its
/// channel number, its address and its description (`Spare` for a drawn spare), at the cell's
/// anchors. `cache` holds the racks' blocks for one page draw.
pub fn channel_texts(
    doc: &Document, c: ComponentId, rid: ecad_model::RepresentationId, r: &ecad_model::Representation, pl: &ecad_model::Placement,
    cache: &mut BTreeMap<ComponentId, BTreeMap<ComponentId, address::Block>>,
) -> Vec<crate::devices::PageText> {
    use crate::devices::{DerivedText, PageText, TextOf};
    use crate::symbols::TextRole;
    let Some(s) = crate::devices::rep_starter(doc, r) else { return Vec::new() };
    if channels::cell_type(s).is_none() {
        return Vec::new();
    }
    // The channel this symbol shows: its first point's.
    let shown_points = crate::devices::shown_terminals(doc, c, r);
    let Some(i) = shown_points.iter().find_map(|(t, _)| channels::point_of(doc, *t)).map(|x| x.0) else { return Vec::new() };
    let at = crate::defaults::Scope { component: Some(c), ..crate::defaults::Scope::project() };
    let on = |e: &crate::defaults::Entry| crate::defaults::resolve(doc, e, at).word() == "on";
    let first = channels::first_number(doc, c);
    let (auto, _) = address::of_channel(doc, c, i, cache);
    let (addr, _) = address::shown(doc, c, i, auto);
    let text =
        doc.components.get(&c).and_then(|x| x.properties.get(&channels::ch_key(i, "text"))).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    let conn = doc.connectivity();
    let used = doc
        .terminals_of(c)
        .filter(|(t, _)| channels::point_of(doc, *t).is_some_and(|x| x.0 == i))
        .any(|(t, _)| conn.net_of_terminal(t).and_then(|n| conn.net(n)).is_some_and(|n| n.terminals.len() > 1));
    let description = match (&text, used) {
        (Some(t), _) => Some(t.clone()),
        (None, false) => Some(crate::defaults::resolve(doc, &crate::defaults::plc::SPARE_TEXT, crate::defaults::Scope::project()).text()),
        (None, true) => None,
    };
    let rotated = crate::defaults::resolve(doc, &crate::defaults::plc::ROTATION, at).word() == "90";
    let anchors = crate::symbols::anchors(s);
    let label = crate::text_style::text_style(doc, crate::text_style::TextKind::PointLabel).height;
    let desc_h = crate::text_style::text_style(doc, crate::text_style::TextKind::Description).height;
    let mut out = Vec::new();
    let mut push = |role: TextRole, of: TextOf, value: String, height: f64, shown: bool, horizontal: bool| {
        let Some(a) = anchors.iter().find(|x| x.role == role) else { return };
        let p = pl.apply(a.at);
        if !p.is_finite() {
            return;
        }
        let (h, v) = if horizontal && role == TextRole::ChannelText {
            (cadcraft_engine::doc::HAlign::Center, cadcraft_engine::doc::VAlign::Top)
        } else {
            let h = match a.h {
                crate::symbols::HAlign::Left => cadcraft_engine::doc::HAlign::Left,
                crate::symbols::HAlign::Centre => cadcraft_engine::doc::HAlign::Center,
                crate::symbols::HAlign::Right => cadcraft_engine::doc::HAlign::Right,
            };
            let v = match a.v {
                crate::symbols::VAlign::Bottom => cadcraft_engine::doc::VAlign::Bottom,
                crate::symbols::VAlign::Middle => cadcraft_engine::doc::VAlign::Middle,
                crate::symbols::VAlign::Top => cadcraft_engine::doc::VAlign::Top,
            };
            (h, v)
        };
        out.push(PageText {
            text: DerivedText { rep: rid, of, value, at: p, height, halign: h, valign: v, aci: None },
            shown,
            moved: false,
            overlaps: false,
        });
    };
    push(TextRole::ChannelNo, TextOf::ChannelNo, (i + first).to_string(), label, on(&crate::defaults::plc::CHANNEL_SHOW), false);
    if let Some(a) = addr {
        push(TextRole::Address, TextOf::Address, a, label, on(&crate::defaults::plc::ADDRESS_SHOW), false);
    }
    if let Some(d) = description {
        push(TextRole::ChannelText, TextOf::ChannelText { rotated }, d, desc_h, true, !rotated);
    }
    out
}

/// The one value rule of `plc.*` keys written with `setprop` (design 045 §4.6), called where 034's
/// is: a card's rack, slot and channel count, a channel's text and typed address, and the registry's
/// object-level entries. `None`: not a key of this brief.
pub fn setprop_rule(doc: &Document, c: ComponentId, key: &str, value: Option<&str>) -> Option<Result<Option<String>, String>> {
    if let Some(r) = rack::setprop_value(doc, c, key, value) {
        return Some(r);
    }
    if let Some(r) = channels::setprop_value(key, value) {
        return Some(r);
    }
    if key == address::START_KEY || key == address::SIZE_KEY {
        let v = value.map(str::trim).filter(|v| !v.is_empty());
        let max = if key == address::START_KEY { address::MAX_BYTE } else { address::MAX_SIZE };
        return Some(match v {
            None => Ok(None),
            Some(t) if t.len() <= 5 && t.bytes().all(|b| b.is_ascii_digit()) && t.parse::<u32>().is_ok_and(|n| n <= max) => Ok(Some(t.to_string())),
            Some(_) => Err(format!("{key} is a whole number of bytes from 0 to {max}.")),
        });
    }
    let entry = crate::defaults::find(key).filter(|e| e.key.starts_with("plc."))?;
    let v = value.map(str::trim).filter(|v| !v.is_empty());
    Some(match v {
        None => Ok(None),
        Some(_) if !entry.allows(crate::defaults::Level::Object) => {
            Err(format!("{key} is a project setting; set it with setdefault at project level."))
        }
        Some(t) if entry.parse(t).is_some() => Ok(Some(t.to_string())),
        Some(t) => Err(format!("{t} is not a value of {key}.")),
    })
}

/// Properties ▸ Channel rows of a channel symbol (design 045 §4.7): the channel (read), its
/// description and typed address (placeholder: the automatic one), and one pin per role.
pub fn channel_rows(doc: &Document, c: ComponentId, symbol: Option<ecad_model::RepresentationId>, rows: &mut Vec<crate::props::Row>) {
    let Some(r) = symbol.and_then(|s| doc.representations.get(&s)) else { return };
    if crate::devices::rep_starter(doc, r).and_then(channels::cell_type).is_none() {
        return;
    }
    let shown = crate::devices::shown_terminals(doc, c, r);
    let Some(i) = shown.iter().find_map(|(t, _)| channels::point_of(doc, *t)).map(|x| x.0) else { return };
    let first = channels::first_number(doc, c);
    let row = |key: String, label: &str, value: String, editable: bool, inherited: bool| crate::props::Row {
        key,
        label: label.into(),
        value,
        editable,
        inherited,
        varies: false,
        choices: Vec::new(),
    };
    rows.push(row(format!("plc.channel#{i}"), "Channel", (i + first).to_string(), false, false));
    let props = doc.components.get(&c).map(|x| &x.properties);
    let own = |k: &str| props.and_then(|p| p.get(k)).cloned();
    rows.push(row(channels::ch_key(i, "text"), "Description", own(&channels::ch_key(i, "text")).unwrap_or_default(), true, false));
    let typed = own(&channels::ch_key(i, "address"));
    let auto = address::of_channel(doc, c, i, &mut BTreeMap::new()).0.unwrap_or_default();
    let inherited = typed.is_none();
    rows.push(row(channels::ch_key(i, "address"), "Address", typed.unwrap_or(auto), true, inherited));
    for (t, _) in &shown {
        if let (Some((_, role)), Some(term)) = (channels::point_of(doc, *t), doc.terminals.get(t)) {
            rows.push(row(format!("plc.pin#{i}#{role}"), &format!("Pin {role}"), term.label.clone().unwrap_or_default(), true, false));
        }
    }
}
