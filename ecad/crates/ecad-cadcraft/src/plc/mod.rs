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
}

/// The type of device `c` as a PLC card or rack: its card body's.
pub fn card_type_of(doc: &Document, c: ComponentId) -> Option<CardType> {
    rack::body_type(doc, c)
}

/// The cards whose automatic addresses a move of `card` may shift, before it (a snapshot to compare
/// with [`moved_cards`]). Brief 045a has no addresses yet: empty.
pub fn moved_blocks_before(_doc: &Document, _card: ComponentId) -> BTreeMap<ComponentId, String> {
    BTreeMap::new()
}

/// How many cards other than `card` changed address since `before`.
pub fn moved_cards(_doc: &Document, _card: ComponentId, _before: &BTreeMap<ComponentId, String>) -> usize {
    0
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
    /// The racks and cards the last update recomputed.
    pub last_changed: BTreeSet<ComponentId>,
    /// How many racks and cards the last update recomputed (tests and bench).
    pub last_work: usize,
    /// How many racks and cards were recomputed in all (tests: an unrelated edit adds none).
    pub recomputed: u64,
}

impl PartialEq for Plc {
    fn eq(&self, o: &Self) -> bool {
        self.racks == o.racks && self.cards == o.cards && self.naming == o.naming && self.checks == o.checks && self.unracked == o.unracked
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
                Op::AddPage { .. } | Op::RemovePage { .. } => {}
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
        let rows = self.group_checks(doc, g);
        let old = if rows.is_empty() { self.checks.remove(&g) } else { self.checks.insert(g, rows.clone()) };
        old.unwrap_or_default() != rows
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

    fn group_checks(&self, _doc: &Document, g: Group) -> Vec<Check> {
        let mut out = Vec::new();
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
            PlcFix::Unracked => self
                .unracked_by_tag()
                .into_iter()
                .map(|c| (format!("Put {} in rack…", self.tag_of(c)), "plcslot", json!({"card": c.hex()})))
                .collect(),
        }
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
pub fn pages_touched(doc: &Document, c: ComponentId, key: &str) -> Vec<PageId> {
    if !(key == ecad_model::tag::TAG_KEY || key.starts_with("plc.")) || !rack::is_rack(doc, c) {
        return Vec::new();
    }
    rack::cards_naming(doc, c)
        .into_iter()
        .flat_map(|(card, _)| doc.representations_of(Subject::Component(card)).map(|(_, r)| r.page).collect::<Vec<_>>())
        .collect()
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
        _ => return None,
    };
    Some(format!("plcslot {j}"))
}
