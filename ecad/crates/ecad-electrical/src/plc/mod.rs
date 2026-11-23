//! PLC racks and cards: stations with numbered slots,
//! modules in them, and the kept `Plc` model the commands, Checks and lists read.
//!
//! # Semantics
//! - A rack is a device whose stored type is a station ([`ecad_model::objects::plc::PlcRack`]), a card one
//!   whose type is a module ([`ecad_model::objects::plc::PlcModule`]); a module's station, slot, channel
//!   groups and channels are its fields. Every change is a `ModifyDevice` ([`edit`]).
//! - The model is kept with the report models: built on first show (`Plc::build`) and built again
//!   from scratch after every commit (3.9 ms on the large project, under the ~5 ms bar; no update
//!   map since 0.2.3). What changed is found by comparing with the model before.
//!
//! Excludes: the PLC manager and the rack overview, any PLC-vendor knowledge.

pub mod address;
pub mod channels;
pub mod commands;
pub mod draw;
pub mod edit;
pub mod exchange;
pub mod iolist;
pub mod kept;
pub mod overview;
pub mod rack;
pub mod sheets;
pub mod signals;
pub mod tagtable;
pub mod types;

use crate::CmdError;
use ecad_model::geometry::units::Degrees;
use std::collections::{BTreeMap, BTreeSet};

use ecad_model::{DeviceId, Document, ObjectRef, PageId, Subject};
use serde_json::json;

use crate::checks::{Check, Fix, Kind, Object};
use rack::{CardType, RackRef};

/// What a PLC Checks row's buttons do, with [`Fix::Plc`].
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
    /// `plc-tag-twice`, `plc-tag-chars`: **Rename** (the channel's
    /// index; the UI focuses its Tag cell).
    Rename(u32),
}

/// The type of device `c` as a PLC card or rack, read from its stored type.
pub fn card_type_of(doc: &Document, c: DeviceId) -> Option<CardType> {
    CardType::of_type(&doc.devices().get(&c)?.device_type)
}

/// Every racked card's block, read in one pass (for counting what a change moves).
pub(crate) fn all_blocks(doc: &Document) -> BTreeMap<DeviceId, address::Block> {
    let racks: BTreeSet<DeviceId> = doc
        .devices()
        .values()
        .filter_map(|x| match &x.device_type {
            ecad_model::DeviceType::PlcModule(m) => m.rack,
            _ => None,
        })
        .filter(|r| edit::station_of(doc, *r).is_some())
        .collect();
    racks.into_iter().flat_map(|r| address::snapshot(doc, r)).collect()
}

/// The cards whose automatic addresses a move of `card` may shift, before it (a snapshot to compare
/// with [`moved_cards`]).
pub(crate) fn moved_blocks_before(doc: &Document, _card: DeviceId) -> BTreeMap<DeviceId, address::Block> {
    all_blocks(doc)
}

/// How many cards other than `card` changed address since `before`.
pub(crate) fn moved_cards(doc: &Document, card: DeviceId, before: &BTreeMap<DeviceId, address::Block>) -> usize {
    let after = all_blocks(doc);
    let keys: BTreeSet<&DeviceId> = before.keys().chain(after.keys()).collect();
    keys.into_iter()
        .filter(|c| **c != card)
        .filter(|c| before.get(c).map(|b| (b.input, b.output)) != after.get(c).map(|b| (b.input, b.output)))
        .count()
}

/// The drawn copies of channel `i` of card `c`: the points of each symbol showing it.
pub fn copies(doc: &Document, c: DeviceId, i: u32) -> Vec<Vec<ecad_model::ConnectionPointId>> {
    let mut by_rep: BTreeMap<ecad_model::RepresentationId, Vec<ecad_model::ConnectionPointId>> = BTreeMap::new();
    let mut loose = Vec::new();
    for (t, _) in doc.connection_points_of(c) {
        if channels::point_of(doc, t).is_some_and(|(j, _)| j == i) {
            match doc.representations_showing(t).next() {
                Some((rid, _)) => by_rep.entry(rid).or_default().push(t),
                None => loose.push(t),
            }
        }
    }
    let mut out: Vec<Vec<ecad_model::ConnectionPointId>> = by_rep.into_values().collect();
    if !loose.is_empty() {
        out.push(loose);
    }
    out
}

/// One channel of a card in the kept model.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChannelData {
    /// (role, terminal, pin), roles in cell order; a role twice when the channel is drawn twice.
    pub points: Vec<(ecad_model::ChannelRole, ecad_model::ConnectionPointId, String)>,
    /// The symbols drawing it: (representation, page, the symbol's card type).
    pub cells: Vec<(ecad_model::RepresentationId, PageId, Option<CardType>)>,
    /// A point is on a net with another connection point.
    pub used: bool,
    pub text: Option<String>,
    pub typed: Option<String>,
    /// Its PLC tag and its own safe state.
    pub tag: Option<String>,
    pub safe: Option<String>,
}

impl ChannelData {
    /// The text that makes a free channel reserved: its description, else its tag
    /// Typing either reserves it).
    pub fn mark(&self) -> Option<&str> {
        self.text.as_deref().or(self.tag.as_deref())
    }
}

/// A channel's state.
pub(crate) fn state_word(used: bool, text: Option<&str>) -> &'static str {
    match (used, text.is_some_and(|t| !t.is_empty())) {
        (true, _) => "used",
        (false, true) => "reserved",
        (false, false) => "spare",
    }
}

/// A rack in the kept model.
#[derive(Debug, Clone, PartialEq)]
pub struct RackData {
    pub id: DeviceId,
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
    pub id: DeviceId,
    pub tag: String,
    pub ty: CardType,
    pub rack: RackRef,
    pub slot: Option<u32>,
    pub channels: u32,
    pub page: Option<PageId>,
    pub part_number: Option<String>,
    pub maker: Option<String>,
    /// Its channels with points, text or a typed address, by index.
    pub chans: BTreeMap<u32, ChannelData>,
    /// How many I/O rows it lists (`0..channels` and any channel beyond), kept for the
    /// I/O page's sheet count.
    pub rows: usize,
    /// Its first channel number (`plc.channel.first`).
    pub first: u32,
}

/// The group a Checks row belongs to: a rack (its cards' slot rows) or a card (its own rows).
type Group = DeviceId;

/// The page of device `c`'s first representation.
fn first_page(doc: &Document, c: DeviceId) -> Option<PageId> {
    let pages: BTreeSet<PageId> = doc.representations_of(Subject::Device(c)).map(|(_, r)| r.page).collect();
    doc.page_order().iter().find(|p| pages.contains(p)).copied()
}

/// The extra pages a property change on component `c` redraws: a rack's tag or
/// slot range shows in its cards' captions.
pub fn pages_touched(doc: &Document, kept: Option<&kept::Plc>, c: DeviceId, key: &str, after: Option<&str>) -> Vec<PageId> {
    if !(key == ecad_model::naming::tag::TAG_KEY || ecad_model::naming::keys::in_area(key, ecad_model::naming::keys::Area::Plc)) {
        return Vec::new();
    }
    // A rack's cards from the kept index; a walk only without the models.
    let pages_of = |r: DeviceId| -> Vec<PageId> {
        let cards: Vec<DeviceId> = match kept {
            Some(p) => p.naming.get(&r).into_iter().flatten().copied().collect(),
            None => rack::cards_naming(doc, r).into_iter().map(|(c, _)| c).collect(),
        };
        cards.into_iter().flat_map(|card| doc.representations_of(Subject::Device(card)).map(|(_, x)| x.page).collect::<Vec<_>>()).collect()
    };
    if edit::station_of(doc, c).is_some() {
        return pages_of(c);
    }
    // A card's own text or typed address shows on its own cells only.
    if key == ecad_model::naming::tag::TAG_KEY || key.starts_with(channels::CH_PREFIX) {
        return Vec::new();
    }
    let mut out = Vec::new();
    if let rack::RackRef::Rack(r) = rack::rack_ref(doc, c) {
        out.extend(pages_of(r));
    }
    if key == rack::RACK_KEY
        && let Some(r) = after.and_then(|v| DeviceId::parse(v.trim())).filter(|r| edit::station_of(doc, *r).is_some())
    {
        out.extend(pages_of(r));
    }
    out
}

/// Whether `d` is a PLC station or module.
pub(crate) fn is_plc(d: &ecad_model::Device) -> bool {
    CardType::of_type(&d.device_type).is_some()
}

/// The extra pages a `ModifyDevice` of station or module `c` redraws (the update
/// map: a station's cards' pages (they read its tag and address style), and a
/// module's old and new station's cards' pages (a slot, count or pinned block moves the addresses of
/// the cards after it).
pub(crate) fn pages_modified(
    doc: &Document, kept: Option<&kept::Plc>, c: DeviceId, before: &ecad_model::Device, after: &ecad_model::Device,
) -> Vec<PageId> {
    let pages_of = |r: DeviceId| -> Vec<PageId> {
        let cards: Vec<DeviceId> = match kept {
            Some(p) => p.naming.get(&r).into_iter().flatten().copied().collect(),
            None => rack::cards_naming(doc, r).into_iter().map(|(c, _)| c).collect(),
        };
        cards.into_iter().flat_map(|card| doc.representations_of(Subject::Device(card)).map(|(_, x)| x.page).collect::<Vec<_>>()).collect()
    };
    let rack_of = |d: &ecad_model::Device| match &d.device_type {
        ecad_model::DeviceType::PlcModule(m) => m.rack,
        ecad_model::DeviceType::PlcRack(_) => Some(c),
        _ => None,
    };
    // A module's own channel data and attributes show on its own pages only.
    let place = |d: &ecad_model::Device| match &d.device_type {
        ecad_model::DeviceType::PlcModule(m) => Some((m.rack, m.slot, m.address, m.channel_count(), CardType::of_type(&d.device_type))),
        _ => None,
    };
    if place(before).is_some() && place(before) == place(after) {
        return Vec::new();
    }
    let mut racks: BTreeSet<DeviceId> = BTreeSet::new();
    racks.extend(rack_of(before));
    racks.extend(rack_of(after));
    racks.into_iter().flat_map(pages_of).collect()
}

/// Card `c`'s caption and whether it is shown: the registry pattern with the
/// rack's tag and the slot, when the card is in a rack's slot; `None` otherwise. `plc.text.caption.show`
/// is read at the card's level.
pub fn caption(doc: &Document, c: DeviceId) -> Option<(String, bool)> {
    let (rack::RackRef::Rack(r), Some(slot)) = (rack::rack_ref(doc, c), rack::slot_of(doc, c)) else { return None };
    let at = ecad_model::registry::Scope { component: Some(c), ..ecad_model::registry::Scope::project() };
    let pattern = ecad_model::registry::resolve(doc, &crate::defaults::plc::CAPTION, at).text();
    let shown = ecad_model::registry::resolve(doc, &crate::defaults::plc::CAPTION_SHOW, at).flag();
    let text = pattern
        .replace("{rack}", &crate::devices::tags::tag_words(doc, r))
        .replace("{slot:02}", &format!("{slot:02}"))
        .replace("{slot}", &slot.to_string());
    Some((text, shown))
}

/// The caption text of representation `r` of card `c` at `pl`, when `r` shows the card's body and
/// the card is in a slot.
pub fn caption_text(
    doc: &Document, c: DeviceId, rep: ecad_model::RepresentationId, r: &ecad_model::Representation, pl: &ecad_model::Placement,
) -> Option<crate::devices::texts::PageText> {
    let s = crate::devices::tags::rep_starter(doc, r)?;
    let ty = CardType::of_symbol(s)?;
    if ty.is_rack() {
        return None;
    }
    let (value, shown) = caption(doc, c)?;
    let anchor = crate::symbols::anchors::anchors(s).into_iter().find(|a| a.role == crate::symbols::anchors::TextRole::Caption)?;
    let at = pl.apply(anchor.at);
    if !at.is_finite() {
        return None;
    }
    // One kind for height and style, the one `TextOf::Caption` draws in.
    let height = crate::styles::text_style::text_style(doc, crate::devices::texts::kind_of(crate::devices::texts::TextOf::Caption)).height;
    Some(crate::devices::texts::PageText {
        text: crate::devices::texts::DerivedText {
            rep,
            of: crate::devices::texts::TextOf::Caption,
            value,
            at,
            height,
            halign: ecad_model::TextHAlign::Left,
            valign: ecad_model::TextVAlign::Top,
            aci: None,
            own: None,
        },
        shown,
        moved: false,
        overlaps: false,
    })
}

// ---------------------------------------------------------------- Properties

/// Properties ▸ Card and ▸ Rack rows of device `c`: a card's Rack (its tag,
/// never the ID), Slot and Channels; a rack's Slots and First slot (registry values at object
/// level, faint when inherited).
pub(crate) fn props_rows(doc: &Document, c: DeviceId, rows: &mut Vec<crate::cables::words::Row>) {
    let Some(ty) = card_type_of(doc, c) else { return };
    let props = doc.devices().get(&c).map(|x| &x.properties);
    let own = |k: &str| props.and_then(|p| p.get(k)).cloned();
    let row = |key: &str, label: &str, value: String, inherited: bool, choices: Vec<String>| crate::cables::words::Row {
        kind: crate::cables::words::RowKind::Value,
        key: key.into(),
        label: label.into(),
        value,
        editable: true,
        inherited,
        varies: false,
        choices,
        proposal: None,
        tag_state: None,
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
    let mut choices: Vec<String> = rack::racks(doc).into_iter().take(64).map(|r| crate::devices::tags::tag_words(doc, r)).collect();
    choices.push("None".into());
    let rack_value = match rack::rack_ref(doc, c) {
        RackRef::Rack(r) => crate::devices::tags::tag_words(doc, r),
        RackRef::Missing(_) => "(missing rack)".into(),
        RackRef::None => "None".into(),
    };
    rows.push(row(rack::RACK_KEY, "Rack", rack_value, false, choices));
    rows.push(row(rack::SLOT_KEY, "Slot", rack::slot_of(doc, c).map(|s| s.to_string()).unwrap_or_default(), false, Vec::new()));
    if ty.has_channels() {
        rows.push(row(rack::CHANNELS_KEY, "Channels", rack::channel_count(doc, c).to_string(), false, Vec::new()));
    }
}

/// The objects Properties ▸ Card reads besides the card: its rack (its tag is shown).
pub fn props_deps(doc: &Document, c: DeviceId) -> Option<ObjectRef> {
    match rack::rack_ref(doc, c) {
        RackRef::Rack(r) | RackRef::Missing(r) => Some(ObjectRef::Device(r)),
        _ => None,
    }
}

/// The command line a Card row writes (`plcslot`), or `None` for any other key.
pub fn edit_line(c: DeviceId, key: &str, value: Option<&str>) -> Option<String> {
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
pub(crate) fn channel_data(doc: &Document, c: DeviceId) -> BTreeMap<u32, ChannelData> {
    let mut out: BTreeMap<u32, ChannelData> = BTreeMap::new();
    let conn = doc.connectivity();
    for (t, term) in doc.connection_points_of(c) {
        let Some((i, role)) = channels::point_of(doc, t) else { continue };
        let ch = out.entry(i).or_default();
        ch.points.push((role, t, term.label.clone().unwrap_or_default()));
        let used = conn.net_of_connection_point(t).and_then(|n| conn.net(n)).is_some_and(|n| n.connection_points.len() > 1);
        ch.used |= used;
        for (rid, r) in doc.representations_showing(t) {
            if !ch.cells.iter().any(|(x, _, _)| *x == rid) {
                let ty = crate::devices::tags::rep_starter(doc, r).and_then(channels::cell_type);
                ch.cells.push((rid, r.page, ty));
            }
        }
    }
    if let Some(m) = edit::module_of(doc, c) {
        for (i, x) in m.channels() {
            let ch = out.entry(i).or_default();
            ch.text = x.text.as_ref().map(ToString::to_string);
            ch.tag = x.tag.as_ref().map(ToString::to_string);
            ch.safe = x.safe.as_ref().map(ToString::to_string);
            ch.typed = x.address.as_ref().map(ToString::to_string);
        }
    }
    for ch in out.values_mut() {
        ch.points.sort_by_key(|(r, t, _)| (*r, *t));
    }
    out
}

/// The channel texts of cell `rid` (representation `r` at `pl`) of card `c`: its
/// channel number, its address and its description (`Spare` for a drawn spare), at the cell's
/// anchors. `cache` holds the racks' blocks for one page draw.
pub(crate) fn channel_texts(
    doc: &Document, c: DeviceId, rid: ecad_model::RepresentationId, r: &ecad_model::Representation, pl: &ecad_model::Placement,
    kept: Option<&kept::Plc>, cache: &mut BTreeMap<DeviceId, BTreeMap<DeviceId, address::Block>>,
) -> Vec<crate::devices::texts::PageText> {
    use crate::devices::texts::{DerivedText, PageText, TextOf};
    use crate::symbols::anchors::TextRole;
    let Some(s) = crate::devices::tags::rep_starter(doc, r) else { return Vec::new() };
    if channels::cell_type(s).is_none() {
        return Vec::new();
    }
    // The channel this symbol shows: its first point's.
    let shown_points = crate::devices::merge::shown_connection_points(doc, c, r);
    let Some(i) = shown_points.iter().find_map(|(t, _)| channels::point_of(doc, *t)).map(|x| x.0) else { return Vec::new() };
    let at = ecad_model::registry::Scope { component: Some(c), ..ecad_model::registry::Scope::project() };
    let on = |e: &ecad_model::registry::Entry| ecad_model::registry::resolve(doc, e, at).flag();
    let first = channels::first_number(doc, c);
    let (auto, _) = address::of_channel(doc, c, i, kept, cache);
    let (addr, _) = address::shown(doc, c, i, auto);
    let text = edit::module_of(doc, c).and_then(|m| m.channel(i)).and_then(|x| x.text.as_ref()).map(ToString::to_string);
    let conn = doc.connectivity();
    let used = doc
        .connection_points_of(c)
        .filter(|(t, _)| channels::point_of(doc, *t).is_some_and(|x| x.0 == i))
        .any(|(t, _)| conn.net_of_connection_point(t).and_then(|n| conn.net(n)).is_some_and(|n| n.connection_points.len() > 1));
    let description = match (&text, used) {
        (Some(t), _) => Some(t.clone()),
        (None, false) => Some(ecad_model::registry::resolve(doc, &crate::defaults::plc::SPARE_TEXT, ecad_model::registry::Scope::project()).text()),
        (None, true) => None,
    };
    let rotated =
        ecad_model::registry::resolve(doc, &crate::defaults::plc::ROTATION, at).choice() == Some(crate::defaults::plc::CardRotation::Quarter);
    let anchors = crate::symbols::anchors::anchors(s);
    let label = crate::styles::text_style::text_style(doc, crate::styles::text_style::TextKind::PointLabel).height;
    // The two text styles of their own.
    let addr_h = crate::styles::text_style::text_style(doc, crate::styles::text_style::TextKind::Address).height;
    let desc_h = crate::styles::text_style::text_style(doc, crate::styles::text_style::TextKind::ChannelText).height;
    // A cell at 180° keeps its texts upright and in their bands.
    let upside = pl.rotation.is_finite() && (Degrees::from(pl.rotation).0.rem_euclid(360.0) - 180.0).abs() < 0.5;
    let mut out = Vec::new();
    let mut push = |role: TextRole, of: TextOf, value: String, height: f64, shown: bool, horizontal: bool| {
        let Some(a) = anchors.iter().find(|x| x.role == role) else { return };
        let p = pl.apply(a.at);
        if !p.is_finite() {
            return;
        }
        let (h, v) = if horizontal && role == TextRole::ChannelText {
            let v = if upside { ecad_model::TextVAlign::Bottom } else { ecad_model::TextVAlign::Top };
            (ecad_model::TextHAlign::Center, v)
        } else if upside && role == TextRole::ChannelText {
            // Above the card, reading upwards from 2 over its frame.
            (ecad_model::TextHAlign::Left, ecad_model::TextVAlign::Middle)
        } else if upside {
            // Turned with the cell, the anchor's sides swap: swap the alignment back.
            let h = match a.h {
                crate::symbols::anchors::HAlign::Left => ecad_model::TextHAlign::Right,
                crate::symbols::anchors::HAlign::Centre => ecad_model::TextHAlign::Center,
                crate::symbols::anchors::HAlign::Right => ecad_model::TextHAlign::Left,
            };
            let v = match a.v {
                crate::symbols::anchors::VAlign::Bottom => ecad_model::TextVAlign::Top,
                crate::symbols::anchors::VAlign::Middle => ecad_model::TextVAlign::Middle,
                crate::symbols::anchors::VAlign::Top => ecad_model::TextVAlign::Bottom,
            };
            (h, v)
        } else {
            let h = match a.h {
                crate::symbols::anchors::HAlign::Left => ecad_model::TextHAlign::Left,
                crate::symbols::anchors::HAlign::Centre => ecad_model::TextHAlign::Center,
                crate::symbols::anchors::HAlign::Right => ecad_model::TextHAlign::Right,
            };
            let v = match a.v {
                crate::symbols::anchors::VAlign::Bottom => ecad_model::TextVAlign::Bottom,
                crate::symbols::anchors::VAlign::Middle => ecad_model::TextVAlign::Middle,
                crate::symbols::anchors::VAlign::Top => ecad_model::TextVAlign::Top,
            };
            (h, v)
        };
        out.push(PageText {
            text: DerivedText { rep: rid, of, value, at: p, height, halign: h, valign: v, aci: None, own: None },
            shown,
            moved: false,
            overlaps: false,
        });
    };
    push(TextRole::ChannelNo, TextOf::ChannelNo, (i + first).to_string(), label, on(&crate::defaults::plc::CHANNEL_SHOW), false);
    if let Some(a) = addr {
        push(TextRole::Address, TextOf::Address, a, addr_h, on(&crate::defaults::plc::ADDRESS_SHOW), false);
    }
    if let Some(d) = description {
        push(TextRole::ChannelText, TextOf::ChannelText { rotated }, d, desc_h, true, !rotated);
    }
    out
}

/// The value rule of the registry's object-level `plc.*` entries written with `setprop` (design
/// called where the symbol one is. A module's and station's own fields are the `setprop` rows of
/// `crate::properties::commands::plc`. `None`: not a registry entry of this brief.
pub(crate) fn setprop_rule(_doc: &Document, _c: DeviceId, key: &str, value: Option<&str>) -> Option<Result<Option<String>, CmdError>> {
    let entry = crate::defaults::find(key).filter(|e| ecad_model::naming::keys::in_area(e.key, ecad_model::naming::keys::Area::Plc))?;
    let v = value.map(str::trim).filter(|v| !v.is_empty());
    Some(match v {
        None => Ok(None),
        Some(_) if !entry.allows(ecad_model::registry::Level::Object) => {
            Err(CmdError::refused(format!("{key} is a project setting; set it with setdefault at project level.")))
        }
        Some(t) if entry.parse(t).is_some() => Ok(Some(t.to_string())),
        Some(t) => Err(CmdError::refused(format!("{t} is not a value of {key}."))),
    })
}

/// Properties ▸ Channel rows of a channel symbol: the channel (read), its
/// description and typed address (placeholder: the automatic one), and one pin per role.
pub(crate) fn channel_rows(
    doc: &Document, kept: Option<&kept::Plc>, c: DeviceId, symbol: Option<ecad_model::RepresentationId>, rows: &mut Vec<crate::cables::words::Row>,
) {
    let Some(r) = symbol.and_then(|s| doc.representations().get(&s)) else { return };
    if crate::devices::tags::rep_starter(doc, r).and_then(channels::cell_type).is_none() {
        return;
    }
    let shown = crate::devices::merge::shown_connection_points(doc, c, r);
    let Some(i) = shown.iter().find_map(|(t, _)| channels::point_of(doc, *t)).map(|x| x.0) else { return };
    let first = channels::first_number(doc, c);
    let row = |key: String, label: &str, value: String, editable: bool, inherited: bool| crate::cables::words::Row {
        kind: crate::cables::words::RowKind::Value,
        key,
        label: label.into(),
        value,
        editable,
        inherited,
        varies: false,
        choices: Vec::new(),
        proposal: None,
        tag_state: None,
    };
    rows.push(row(format!("plc.channel#{i}"), "Channel", (i + first).to_string(), false, false));
    let own = edit::module_of(doc, c).and_then(|m| m.channel(i));
    let text = own.and_then(|x| x.text.as_ref()).map(ToString::to_string);
    rows.push(row(channels::ch_key(i, "text"), "Description", text.unwrap_or_default(), true, false));
    let typed = own.and_then(|x| x.address.as_ref()).map(ToString::to_string);
    // The kept blocks when the models are built; a walk of one rack otherwise.
    let auto = address::of_channel(doc, c, i, kept, &mut BTreeMap::new()).0.unwrap_or_default();
    let inherited = typed.is_none();
    rows.push(row(channels::ch_key(i, "address"), "Address", typed.unwrap_or(auto), true, inherited));
    for (t, _) in &shown {
        if let (Some((_, role)), Some(term)) = (channels::point_of(doc, *t), doc.connection_points().get(t)) {
            rows.push(row(
                format!("plc.pin#{i}#{}", ecad_model::Word::word(role)),
                &format!("Pin {}", ecad_model::Word::word(role)),
                term.label.clone().unwrap_or_default(),
                true,
                false,
            ));
        }
    }
}

/// Whether `page` shows a PLC channel cell (its texts read addresses).
pub fn page_has_cells(doc: &Document, page: PageId) -> bool {
    doc.representations_on(page).any(|(_, r)| crate::devices::tags::rep_starter(doc, r).and_then(channels::cell_type).is_some())
}

/// `pages 106, 107`; more than three: `pages 106–112 (5)`.
pub fn pages_words(numbers: &[String]) -> Vec<String> {
    match numbers {
        [] => Vec::new(),
        [one] => vec![format!("page {one}")],
        many if many.len() <= 3 => {
            // `pages 106, 107`: each number its own link, the comma with the one before.
            let last = many.len() - 1;
            many.iter()
                .enumerate()
                .map(|(k, n)| {
                    let comma = if k < last { "," } else { "" };
                    if k == 0 { format!("pages {n}{comma}") } else { format!("{n}{comma}") }
                })
                .collect()
        }
        many => vec![format!(
            "pages {}\u{2013}{} ({})",
            many.first().map(String::as_str).unwrap_or_default(),
            many.last().map(String::as_str).unwrap_or_default(),
            many.len()
        )],
    }
}
