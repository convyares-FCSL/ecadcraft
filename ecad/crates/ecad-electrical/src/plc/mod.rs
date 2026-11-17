//! PLC racks and cards (brief 045, design 045): racks with numbered slots, cards in them, and the
//! kept `Plc` model the commands, Checks and lists read.
//!
//! # Semantics
//! - No new object and no new op: a rack is the device whose head is a rack body, a card a device
//!   whose head is another card body; slots and channel counts are card properties ([`rack`]).
//! - The model is kept (like 028's strips): built on first show (`Plc::build`) and brought up to
//!   date from each transaction's ops only (`Plc::update`, design 045 §11.1's update map). An op
//!   the map does not list recomputes nothing.
//!
//! Excludes: the PLC manager and the rack overview (brief 2.8), any PLC-vendor knowledge.

pub mod address;
pub mod channels;
pub mod exchange;
pub mod iolist;
pub mod kept;
pub mod look;
pub mod overview;
pub mod rack;
pub mod sheets;
pub mod signals;
pub mod tagtable;

use crate::CmdError;
use std::collections::{BTreeMap, BTreeSet};

use ecad_model::{ComponentId, Document, ObjectRef, PageId, Subject};
use serde_json::json;

use crate::checks::{Check, Fix, Kind, Object};
use rack::{CardType, RackRef};

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
    /// Brief 056 (design 056 §9): `plc-tag-twice`, `plc-tag-chars`: **Rename** (the channel's
    /// index; the UI focuses its Tag cell).
    Rename(u32),
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
    /// Brief 056 (FP1): its PLC tag and its own safe state.
    pub tag: Option<String>,
    pub safe: Option<String>,
}

impl ChannelData {
    /// The text that makes a free channel reserved: its description, else its tag (design 056
    /// §4.2: typing either reserves it).
    pub fn mark(&self) -> Option<&str> {
        self.text.as_deref().or(self.tag.as_deref())
    }
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
    /// Brief 045c: how many I/O rows it lists (`0..channels` and any channel beyond), kept for the
    /// I/O page's sheet count (review of #84, 3).
    pub rows: usize,
    /// Its first channel number (`plc.channel.first`).
    pub first: u32,
}

/// The group a Checks row belongs to: a rack (its cards' slot rows) or a card (its own rows).
type Group = ComponentId;

/// The page of device `c`'s first representation.
fn first_page(doc: &Document, c: ComponentId) -> Option<PageId> {
    let pages: BTreeSet<PageId> = doc.representations_of(Subject::Component(c)).map(|(_, r)| r.page).collect();
    doc.page_order.iter().find(|p| pages.contains(p)).copied()
}

/// The extra pages a property change on component `c` redraws (design 045 §11.1): a rack's tag or
/// slot range shows in its cards' captions.
pub fn pages_touched(doc: &Document, kept: Option<&kept::Plc>, c: ComponentId, key: &str, after: Option<&str>) -> Vec<PageId> {
    if !(key == ecad_model::tag::TAG_KEY || key.starts_with("plc.")) {
        return Vec::new();
    }
    // A rack's cards from the kept index (review of #84, 3); a walk only without the models.
    let pages_of = |r: ComponentId| -> Vec<PageId> {
        let cards: Vec<ComponentId> = match kept {
            Some(p) => p.naming.get(&r).into_iter().flatten().copied().collect(),
            None => rack::cards_naming(doc, r).into_iter().map(|(c, _)| c).collect(),
        };
        cards.into_iter().flat_map(|card| doc.representations_of(Subject::Component(card)).map(|(_, x)| x.page).collect::<Vec<_>>()).collect()
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
    let shown = crate::defaults::resolve(doc, &crate::defaults::plc::CAPTION_SHOW, at).flag();
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
    // Brief 066d: one kind for height and style, the one `TextOf::Caption` draws in.
    let height = crate::text_style::text_style(doc, crate::devices::kind_of(crate::devices::TextOf::Caption)).height;
    Some(crate::devices::PageText {
        text: crate::devices::DerivedText {
            rep,
            of: crate::devices::TextOf::Caption,
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

// ---------------------------------------------------------------- Properties (design 045 §2.6)

/// Properties ▸ Card and ▸ Rack rows of device `c` (design 045 §2.6): a card's Rack (its tag,
/// never the ID: FP1), Slot and Channels; a rack's Slots and First slot (registry values at object
/// level, faint when inherited).
pub fn props_rows(doc: &Document, c: ComponentId, rows: &mut Vec<crate::words::Row>) {
    let Some(ty) = card_type_of(doc, c) else { return };
    let props = doc.components.get(&c).map(|x| &x.properties);
    let own = |k: &str| props.and_then(|p| p.get(k)).cloned();
    let row = |key: &str, label: &str, value: String, inherited: bool, choices: Vec<String>| crate::words::Row {
        key: key.into(),
        label: label.into(),
        value,
        editable: true,
        inherited,
        varies: false,
        choices,
        proposal: None,
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
            match what {
                "text" => ch.text = Some(v.to_string()),
                signals::TAG => ch.tag = Some(v.to_string()),
                signals::SAFE => ch.safe = Some(v.to_string()),
                _ => ch.typed = Some(v.to_string()),
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
    kept: Option<&kept::Plc>, cache: &mut BTreeMap<ComponentId, BTreeMap<ComponentId, address::Block>>,
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
    let on = |e: &crate::defaults::Entry| crate::defaults::resolve(doc, e, at).flag();
    let first = channels::first_number(doc, c);
    let (auto, _) = address::of_channel(doc, c, i, kept, cache);
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
    let rotated = crate::defaults::resolve(doc, &crate::defaults::plc::ROTATION, at).choice() == Some(crate::defaults::plc::CardRotation::Quarter);
    let anchors = crate::symbols::anchors(s);
    let label = crate::text_style::text_style(doc, crate::text_style::TextKind::PointLabel).height;
    // Brief 051 (design 051 §5): the two text styles of their own.
    let addr_h = crate::text_style::text_style(doc, crate::text_style::TextKind::Address).height;
    let desc_h = crate::text_style::text_style(doc, crate::text_style::TextKind::ChannelText).height;
    // Brief 051 (design 051 §2.7): a cell at 180° keeps its texts upright and in their bands.
    let upside = pl.rotation.is_finite() && (pl.rotation.to_degrees().rem_euclid(360.0) - 180.0).abs() < 0.5;
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
                crate::symbols::HAlign::Left => ecad_model::TextHAlign::Right,
                crate::symbols::HAlign::Centre => ecad_model::TextHAlign::Center,
                crate::symbols::HAlign::Right => ecad_model::TextHAlign::Left,
            };
            let v = match a.v {
                crate::symbols::VAlign::Bottom => ecad_model::TextVAlign::Top,
                crate::symbols::VAlign::Middle => ecad_model::TextVAlign::Middle,
                crate::symbols::VAlign::Top => ecad_model::TextVAlign::Bottom,
            };
            (h, v)
        } else {
            let h = match a.h {
                crate::symbols::HAlign::Left => ecad_model::TextHAlign::Left,
                crate::symbols::HAlign::Centre => ecad_model::TextHAlign::Center,
                crate::symbols::HAlign::Right => ecad_model::TextHAlign::Right,
            };
            let v = match a.v {
                crate::symbols::VAlign::Bottom => ecad_model::TextVAlign::Bottom,
                crate::symbols::VAlign::Middle => ecad_model::TextVAlign::Middle,
                crate::symbols::VAlign::Top => ecad_model::TextVAlign::Top,
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

/// The one value rule of `plc.*` keys written with `setprop` (design 045 §4.6), called where 034's
/// is: a card's rack, slot and channel count, a channel's text and typed address, and the registry's
/// object-level entries. `None`: not a key of this brief.
pub fn setprop_rule(doc: &Document, c: ComponentId, key: &str, value: Option<&str>) -> Option<Result<Option<String>, CmdError>> {
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
            Some(t) if t.len() <= 5 && ecad_model::words::is_digits(t) && t.parse::<u32>().is_ok_and(|n| n <= max) => Ok(Some(t.to_string())),
            Some(_) => Err(CmdError::refused(format!("{key} is a whole number of bytes from 0 to {max}."))),
        });
    }
    let entry = crate::defaults::find(key).filter(|e| e.key.starts_with("plc."))?;
    let v = value.map(str::trim).filter(|v| !v.is_empty());
    Some(match v {
        None => Ok(None),
        Some(_) if !entry.allows(crate::defaults::Level::Object) => {
            Err(CmdError::refused(format!("{key} is a project setting; set it with setdefault at project level.")))
        }
        Some(t) if entry.parse(t).is_some() => Ok(Some(t.to_string())),
        Some(t) => Err(CmdError::refused(format!("{t} is not a value of {key}."))),
    })
}

/// Properties ▸ Channel rows of a channel symbol (design 045 §4.7): the channel (read), its
/// description and typed address (placeholder: the automatic one), and one pin per role.
pub fn channel_rows(
    doc: &Document, kept: Option<&kept::Plc>, c: ComponentId, symbol: Option<ecad_model::RepresentationId>, rows: &mut Vec<crate::words::Row>,
) {
    let Some(r) = symbol.and_then(|s| doc.representations.get(&s)) else { return };
    if crate::devices::rep_starter(doc, r).and_then(channels::cell_type).is_none() {
        return;
    }
    let shown = crate::devices::shown_terminals(doc, c, r);
    let Some(i) = shown.iter().find_map(|(t, _)| channels::point_of(doc, *t)).map(|x| x.0) else { return };
    let first = channels::first_number(doc, c);
    let row = |key: String, label: &str, value: String, editable: bool, inherited: bool| crate::words::Row {
        key,
        label: label.into(),
        value,
        editable,
        inherited,
        varies: false,
        choices: Vec::new(),
        proposal: None,
    };
    rows.push(row(format!("plc.channel#{i}"), "Channel", (i + first).to_string(), false, false));
    let props = doc.components.get(&c).map(|x| &x.properties);
    let own = |k: &str| props.and_then(|p| p.get(k)).cloned();
    rows.push(row(channels::ch_key(i, "text"), "Description", own(&channels::ch_key(i, "text")).unwrap_or_default(), true, false));
    let typed = own(&channels::ch_key(i, "address"));
    // The kept blocks when the models are built (review of #84, 3); a walk of one rack otherwise.
    let auto = address::of_channel(doc, c, i, kept, &mut BTreeMap::new()).0.unwrap_or_default();
    let inherited = typed.is_none();
    rows.push(row(channels::ch_key(i, "address"), "Address", typed.unwrap_or(auto), true, inherited));
    for (t, _) in &shown {
        if let (Some((_, role)), Some(term)) = (channels::point_of(doc, *t), doc.terminals.get(t)) {
            rows.push(row(format!("plc.pin#{i}#{role}"), &format!("Pin {role}"), term.label.clone().unwrap_or_default(), true, false));
        }
    }
}

/// Whether `page` shows a PLC channel cell (its texts read addresses).
pub fn page_has_cells(doc: &Document, page: PageId) -> bool {
    doc.representations_on(page).any(|(_, r)| crate::devices::rep_starter(doc, r).and_then(channels::cell_type).is_some())
}
