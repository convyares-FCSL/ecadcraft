//! The kept PLC model (design 045 §11.1), built from scratch after every commit (see [`super`]). The rules that measure text
//! (an address wider than its cell) take the sheet font metrics, [`TextMetrics`]; ecad-engine's
//! `crate::plc::Plc` wraps this with today's signatures, passing CADCraft's fonts.

use std::collections::{BTreeMap, BTreeSet};

use ecad_model::document::effect::{Effect, Effects};
use ecad_model::{DeviceId, Document, ObjectRef};
use serde_json::{Value, json};

use crate::plc::*;
use crate::styles::text_style::fonts::TextMetrics;

/// What one commit's effects name for the I/O rows (a renamed target, a renumbered wire), noted
/// effect by effect ([`Plc::note`]); the model itself is rebuilt.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Touch {
    /// The cards whose I/O rows can change.
    rows: BTreeSet<DeviceId>,
}

/// The kept PLC model (design 045 §11.1).
#[derive(Debug, Clone, Default)]
pub struct Plc {
    pub racks: BTreeMap<DeviceId, RackData>,
    pub cards: BTreeMap<DeviceId, CardData>,
    /// The cards whose `plc.rack` holds each ID (valid or not): a rack's cards without a scan.
    pub naming: BTreeMap<DeviceId, BTreeSet<DeviceId>>,
    /// Checks rows by group, worked out for the groups an update touched.
    checks: BTreeMap<Group, Vec<Check>>,
    /// Cards in no rack (the `unracked` count row).
    unracked: BTreeSet<DeviceId>,
    /// Brief 045b: each racked card's address block (design 045 §5.1), worked out per rack.
    pub blocks: BTreeMap<DeviceId, address::Block>,
    /// Used channels without an address, by rack (the `no-address` count row).
    no_address: BTreeMap<Group, usize>,
    /// The racks and cards that read differently after the last update.
    pub last_changed: BTreeSet<DeviceId>,
    /// Brief 045c: the cards whose I/O rows the last update may have changed (their own data, a
    /// moved block, a target renamed, a wire renumbered): the I/O page and list patch these only.
    pub rows_changed: BTreeSet<DeviceId>,
    /// How many updates the model has had: a view that missed one rebuilds instead of patching.
    pub updates: u64,
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

impl Plc {
    /// The whole model, from scratch (open, reload, first show), with text measured in `m`.
    pub fn build_with(doc: &Document, m: &dyn TextMetrics) -> Plc {
        let mut p = Plc::default();
        let all: Vec<DeviceId> = doc.tagged().map(|(_, c)| c).collect();
        let mut groups = BTreeSet::new();
        for c in all {
            p.set(doc, c, &mut groups);
        }
        for g in groups {
            p.set_checks(doc, g, m);
        }
        p.last_changed.clear();
        p
    }

    /// Add device `c` (a rack, a card, or neither) and note the check groups it touches.
    fn set(&mut self, doc: &Document, c: DeviceId, groups: &mut BTreeSet<Group>) {
        let ty = doc.devices().get(&c).and_then(|_| card_type_of(doc, c));
        let Some(ty) = ty else { return };
        let tag = crate::devices::tags::tag_words(doc, c);
        let page = first_page(doc, c);
        self.last_changed.insert(c);
        groups.insert(c);
        if ty.is_rack() {
            let (first, slots) = rack::slot_range(doc, c);
            self.racks.insert(c, RackData { id: c, tag, ty, first, slots, page });
            return;
        }
        let r = rack::rack_ref(doc, c);
        let device = doc.devices().get(&c);
        let text = |k: &str| device.and_then(|d| d.get(k)).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
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
                channels: rack::channel_count(doc, c),
                page,
                part_number: text("part_number"),
                maker: text("maker"),
                rows: {
                    let chans = channel_data(doc, c);
                    let n = rack::channel_count(doc, c);
                    usize::try_from(n).unwrap_or(0) + chans.keys().filter(|i| **i >= n).count()
                },
                chans: channel_data(doc, c),
                first: channels::first_number(doc, c),
            },
        );
    }

    /// Note the cards whose I/O rows one effect can change.
    fn note_rows(&self, doc: &Document, effect: &Effect<'_>, out: &mut BTreeSet<DeviceId>) {
        match effect {
            Effect::Device { id: c, before: Some(before), after: Some(after) } if before.tag != after.tag && !self.cards.contains_key(c) => {
                out.extend(iolist::cards_on_nets_of(doc, self, Some(*c), None));
            }
            // A wire's property, or the wire changed in place.
            Effect::Prop { object: ObjectRef::Wire(w), .. } | Effect::Wire { id: w, before: Some(_), after: Some(_) } => {
                out.extend(iolist::cards_on_nets_of(doc, self, None, Some(*w)));
            }
            // Review of #84, 2: a terminal relabelled (renumber, `terminalblocklabel`) or added, on the
            // nets it is on.
            Effect::ConnectionPoint { id, after: Some(_), .. } => {
                out.extend(iolist::cards_on_nets_of_connection_points(doc, self, &[*id]));
            }
            // A continuation renamed joins or splits nets: every continuation of the old and
            // the new name, and the nets they are on now.
            Effect::Aid { id, before: Some(before), after: Some(after) } => {
                let mut ts: Vec<ecad_model::ConnectionPointId> = doc.connection_points_of(*id).map(|(t, _)| t).collect();
                for k in [before, after].into_iter().filter_map(|a| ecad_model::connectivity::continuation_key(a)) {
                    for a in doc.continuations_named(&k) {
                        ts.extend(doc.connection_points_of(a).map(|(t, _)| t));
                    }
                }
                out.extend(iolist::cards_on_nets_of_connection_points(doc, self, &ts));
            }
            // A strip link joins its positions' nets.
            Effect::Prop { object: ObjectRef::Device(c), key, .. } if ecad_model::connectivity::jumper_bars::jumper_bar_number(key.raw).is_some() => {
                out.extend(iolist::cards_on_nets_of(doc, self, Some(*c), None));
            }
            Effect::Device { .. }
            | Effect::Prop { .. }
            | Effect::Wire { .. }
            | Effect::ConnectionPoint { .. }
            | Effect::Page { .. }
            | Effect::Layer { .. }
            | Effect::Item { .. }
            | Effect::DeviceDefinition { .. }
            | Effect::Rep { .. }
            | Effect::Route { .. }
            | Effect::Cable { .. }
            | Effect::Definition { .. }
            | Effect::Settings { .. }
            | Effect::Version { .. }
            | Effect::Folder { .. }
            | Effect::Style { .. }
            | Effect::Standard { .. }
            | Effect::Project { .. }
            | Effect::Aid { .. }
            | Effect::LoosePart { .. }
            | Effect::DeviceLink { .. } => {}
        }
    }

    /// Note the I/O rows one effect of the transaction just committed on `doc` (the state after)
    /// can change, reading the model as it is before the update. Pair with
    /// [`Plc::update_touched`].
    pub fn note(&self, doc: &Document, effect: &Effect<'_>, t: &mut Touch) {
        self.note_rows(doc, effect, &mut t.rows);
    }

    /// Build the model again after `effects` (just committed; `doc` is the state after). Returns
    /// whether anything changed.
    pub fn update_with(&mut self, doc: &Document, effects: &Effects<'_>, m: &dyn TextMetrics) -> bool {
        let mut t = Touch::default();
        for effect in effects {
            self.note(doc, effect, &mut t);
        }
        self.update_touched(doc, t, m)
    }

    /// Build the model again from scratch on `doc`, the state after, and work out what reads
    /// differently from the model before; `t` names the I/O rows the effects reach besides
    /// ([`Plc::note`]). Returns whether anything changed.
    pub(crate) fn update_touched(&mut self, doc: &Document, t: Touch, m: &dyn TextMetrics) -> bool {
        let rebuilt = Plc::build_with(doc, m);
        let ids: BTreeSet<DeviceId> =
            self.cards.keys().chain(self.racks.keys()).chain(rebuilt.cards.keys()).chain(rebuilt.racks.keys()).copied().collect();
        let last_changed: BTreeSet<DeviceId> = ids
            .into_iter()
            .filter(|c| {
                self.cards.get(c) != rebuilt.cards.get(c) || self.racks.get(c) != rebuilt.racks.get(c) || self.blocks.get(c) != rebuilt.blocks.get(c)
            })
            .collect();
        let checks_changed = self.checks != rebuilt.checks || self.no_address != rebuilt.no_address || self.unracked != rebuilt.unracked;
        let mut rows_changed = t.rows;
        rows_changed.extend(last_changed.iter().filter(|c| rebuilt.cards.contains_key(c)).copied());
        let changed = !last_changed.is_empty() || checks_changed || !rows_changed.is_empty();
        let updates = self.updates + 1;
        *self = Plc { last_changed, rows_changed, updates, ..rebuilt };
        changed
    }

    /// Work out group `g`'s Checks rows; returns whether they changed.
    fn set_checks(&mut self, doc: &Document, g: Group, m: &dyn TextMetrics) -> bool {
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
        let rows = self.group_checks(doc, g, m);
        let old = if rows.is_empty() { self.checks.remove(&g) } else { self.checks.insert(g, rows.clone()) };
        moved || old.unwrap_or_default() != rows
    }

    /// The address of channel `i` of card `c` as shown (typed, else automatic) and whether typed.
    pub fn address_of(&self, doc: &Document, c: DeviceId, i: u32) -> (Option<String>, bool) {
        let Some(card) = self.cards.get(&c) else { return (None, false) };
        if let Some(t) = card.chans.get(&i).and_then(|x| x.typed.clone()) {
            return (Some(t), true);
        }
        (self.automatic(doc, c, i, address::shows_output(card.ty)), false)
    }

    /// The automatic address of channel `i` of card `c` (`output`: its output address).
    pub fn automatic(&self, doc: &Document, c: DeviceId, i: u32, output: bool) -> Option<String> {
        let card = self.cards.get(&c)?;
        let RackRef::Rack(r) = card.rack else { return None };
        let b = self.blocks.get(&c)?;
        let slot = card.slot?;
        let style = address::Style::of(doc, r);
        let shown = i.saturating_add(channels::first_number(doc, c));
        address::automatic(style, card.ty, b, slot, i, shown, address::word_size(doc, c, card.ty), output)
    }

    /// Used channels of rack `r`'s cards with no address.
    fn no_address_of(&self, doc: &Document, r: DeviceId) -> usize {
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

    fn tag_of(&self, c: DeviceId) -> String {
        self.cards.get(&c).map(|x| x.tag.clone()).or_else(|| self.racks.get(&c).map(|x| x.tag.clone())).unwrap_or_else(|| "?".into())
    }

    /// The next free slot of rack `r` in the model.
    pub(crate) fn next_free(&self, r: DeviceId) -> Option<u32> {
        let rack = self.racks.get(&r)?;
        let taken: BTreeSet<u32> = self.naming.get(&r).into_iter().flatten().filter_map(|c| self.cards.get(c)).filter_map(|c| c.slot).collect();
        (rack.first..rack.first.saturating_add(rack.slots)).find(|s| !taken.contains(s))
    }

    fn group_checks(&self, doc: &Document, g: Group, m: &dyn TextMetrics) -> Vec<Check> {
        let mut out = Vec::new();
        if let Some(card) = self.cards.get(&g) {
            self.channel_checks(doc, card, &mut out);
            // Brief 051 (design 051 §2.5, §2.8): the card on its pages.
            draw::through_checks(doc, card.id, &card.tag, &mut out);
            draw::wide_checks(doc, card.id, &card.tag, |i| self.address_of(doc, card.id, i).0, m, &mut out);
        }
        if let Some(card) = self.cards.get(&g)
            && matches!(card.rack, RackRef::Missing(_))
        {
            out.push(Check {
                kind: Kind::RackMissing,
                page: card.page,
                at: None,
                object: Object::Device(card.id),
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
                    object: Object::Device(card.id),
                    which: format!("{} is in slot {s}; {} has slots {}–{last}", card.tag, rack.tag, rack.first),
                    fix: Some(Fix::Plc(card.id, PlcFix::SlotRange)),
                }),
                None => out.push(Check {
                    kind: Kind::SlotRange,
                    page: card.page,
                    at: None,
                    object: Object::Device(card.id),
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
                    object: Object::Device(later.id),
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
            let Some(r) = doc.representations().get(&rid) else { return "?".into() };
            let at = crate::devices::tags::rep_definition(doc, r).map(|(_, pl)| pl.origin);
            at.and_then(|a| ecad_model::connectivity::wiring::place(doc, r.page, a))
                .map_or_else(|| crate::references::cables::page_number_words(doc, r.page), |p| p.text(doc.settings().reference_style()))
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
                    object: Object::Device(card.id),
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
                        object: Object::Device(card.id),
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
                object: Object::Device(card.id),
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
                    object: Object::Device(card.id),
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
                        object: Object::Device(later.id),
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
                    object: Object::Device(c.id),
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
                fix: Some(Fix::Plc(DeviceId(0), PlcFix::Unracked)),
            });
        }
        out.extend(self.signal_checks());
        let n: usize = self.no_address.values().sum();
        if n > 0 {
            let which = if n == 1 { "1 used channel has no address".to_string() } else { format!("{n} used channels have no address") };
            out.push(Check {
                kind: Kind::NoAddress,
                page: None,
                at: None,
                object: Object::None,
                which,
                fix: Some(Fix::Plc(DeviceId(0), PlcFix::NoAddress)),
            });
        }
        out
    }

    /// Brief 056's rows (design 056 §9, after the owner's change of 2026-11-14): a tag two channels
    /// of one PLC share (possible after a merge or a raw `setprop`), and a tag an export leaves
    /// out. Worked out from the kept model when asked; bounded by its channels.
    fn signal_checks(&self) -> Vec<Check> {
        let mut out = Vec::new();
        let format: &dyn tagtable::TagFormat = &tagtable::GenericCsv;
        let mut by_plc: BTreeMap<Option<DeviceId>, BTreeMap<String, Vec<(&CardData, u32)>>> = BTreeMap::new();
        for card in self.cards.values().filter(|c| c.ty.has_channels()) {
            let plc = match card.rack {
                RackRef::Rack(r) => Some(r),
                _ => None,
            };
            for (i, ch) in &card.chans {
                let Some(t) = &ch.tag else { continue };
                by_plc.entry(plc).or_default().entry(signals::name_key(t)).or_default().push((card, *i));
                if !format.name_ok(t) {
                    out.push(Check {
                        kind: Kind::PlcTagChars,
                        page: card.page,
                        at: None,
                        object: Object::Device(card.id),
                        which: format!("{} channel {}", card.tag, i + card.first),
                        fix: Some(Fix::Plc(card.id, PlcFix::Rename(*i))),
                    });
                }
            }
        }
        for tags in by_plc.values() {
            for holders in tags.values().filter(|h| h.len() > 1) {
                let words = |(c, i): &(&CardData, u32)| format!("{} channel {}", c.tag, i + c.first);
                for (k, h) in holders.iter().enumerate() {
                    let others: Vec<String> = holders.iter().enumerate().filter(|(j, _)| *j != k).take(3).map(|(_, o)| words(o)).collect();
                    let tag = h.0.chans.get(&h.1).and_then(|x| x.tag.clone()).unwrap_or_default();
                    out.push(Check {
                        kind: Kind::PlcTagTwice,
                        page: h.0.page,
                        at: None,
                        object: Object::Device(h.0.id),
                        which: format!("{}: \"{tag}\" is also the tag of {}", words(h), others.join(" and ")),
                        fix: Some(Fix::Plc(h.0.id, PlcFix::Rename(h.1))),
                    });
                }
            }
        }
        out
    }

    /// The buttons of a PLC Checks row's fix, each a label, a command and its parameters (design
    /// 045 §9). Empty when the row's card is gone.
    pub(crate) fn fix_commands(&self, c: DeviceId, fix: PlcFix) -> Vec<(String, &'static str, Value)> {
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
            PlcFix::Rename(_) => Vec::new(),
            PlcFix::Unracked => self
                .unracked_by_tag()
                .into_iter()
                .map(|c| (format!("Put {} in rack…", self.tag_of(c)), "plcslot", json!({"card": c.hex()})))
                .collect(),
        }
    }

    /// The first channel number of card `c` as the model last read it.
    fn first_of(&self, c: DeviceId) -> u32 {
        self.cards.get(&c).map_or(0, |x| x.first)
    }

    /// How many sheets the I/O page needs at `per` lines a sheet, from the kept row counts (no rows
    /// made): brief 051's packing (design 051 §4.1), [`sheets::pack`].
    pub fn iolist_sheets(&self, per: usize) -> usize {
        let rows: Vec<usize> = iolist::cards_in_order(self).iter().map(|c| c.rows).collect();
        sheets::pack(&rows, 2.0, 1.0, 1.0, per.max(3) as f64).len().max(1)
    }

    /// Brief 051: the Checks rows about device `c` (a card or a rack), from its own group and its
    /// rack's group (slot rows sit there).
    pub(crate) fn checks_naming(&self, c: DeviceId) -> impl Iterator<Item = &Check> {
        let rack = self.cards.get(&c).and_then(|x| match x.rack {
            RackRef::Rack(r) | RackRef::Missing(r) => Some(r),
            _ => None,
        });
        let own = self.checks.get(&c).into_iter().flatten();
        let more = rack.filter(|r| *r != c).and_then(|r| self.checks.get(&r)).into_iter().flatten();
        own.chain(more).filter(move |k| overview::names(&k.object, c))
    }

    /// The racks by tag.
    pub fn racks_by_tag(&self) -> Vec<&RackData> {
        let mut v: Vec<&RackData> = self.racks.values().collect();
        v.sort_by(|a, b| (a.tag.as_str(), a.id).cmp(&(b.tag.as_str(), b.id)));
        v
    }

    /// The cards in no rack (or naming one that is missing), by tag.
    pub(crate) fn unracked_by_tag(&self) -> Vec<DeviceId> {
        let mut v: Vec<&CardData> = self.cards.values().filter(|c| !matches!(c.rack, RackRef::Rack(_))).collect();
        v.sort_by(|a, b| (a.tag.as_str(), a.id).cmp(&(b.tag.as_str(), b.id)));
        v.into_iter().map(|c| c.id).collect()
    }

    /// The cards of rack `r` in slot order (then tag).
    pub fn cards_of(&self, r: DeviceId) -> Vec<&CardData> {
        let mut v: Vec<&CardData> =
            self.naming.get(&r).into_iter().flatten().filter_map(|c| self.cards.get(c)).filter(|c| c.rack == RackRef::Rack(r)).collect();
        v.sort_by(|a, b| (a.slot, a.tag.as_str(), a.id).cmp(&(b.slot, b.tag.as_str(), b.id)));
        v
    }

    /// One card as `plcracks` lists it.
    pub(crate) fn card_json(&self, doc: &Document, c: DeviceId) -> Value {
        let Some(card) = self.cards.get(&c) else { return Value::Null };
        // Brief 051 (design 051 §7): the tile's data.
        let tile = overview::card_tile(doc, self, card, String::new());
        let (inputs, outputs) = overview::spans(doc, self, card);
        let rack = match card.rack {
            RackRef::Rack(r) => json!(self.tag_of(r)),
            RackRef::Missing(_) => json!("missing"),
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
            "reserved": card.chans.values().filter(|x| !x.used && x.mark().is_some()).count(),
            "spare": card.channels.saturating_sub(u32::try_from(card.chans.values().filter(|x| x.used || x.mark().is_some()).count()).unwrap_or(0)),
            // Brief 056 (design 056 §10): the tile's kind words and channels in use.
            "kind_words": overview::kind_words(doc, card),
            "in_use": card.chans.iter().filter(|(i, x)| **i < card.channels && (x.used || x.mark().is_some())).count(),
            "inputs": self.blocks.get(&c).and_then(|b| b.input).map(|(s, n)| json!([s, s.saturating_add(n).saturating_sub(1)])),
            "outputs": self.blocks.get(&c).and_then(|b| b.output).map(|(s, n)| json!([s, s.saturating_add(n).saturating_sub(1)])),
            "pinned": self.blocks.get(&c).is_some_and(|b| b.pinned),
            "part_number": card.part_number,
            "maker": card.maker,
            "pages": tile.json(doc)["pages"],
            "checks": tile.checks,
            "range": json!({"inputs": inputs, "outputs": outputs}),
        })
    }

    /// One rack as `plcracks` lists it: its slots in order, free ones too.
    pub(crate) fn rack_json(&self, doc: &Document, r: &RackData) -> Value {
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
            "body": r.ty.body().map(ecad_model::Word::word),
            "type": r.ty.code(),
            "first": r.first,
            "slots": r.slots,
            "cards": slots,
        })
    }
}
