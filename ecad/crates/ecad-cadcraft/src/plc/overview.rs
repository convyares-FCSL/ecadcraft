//! The rack overview (brief 051, design 051 §3.3 and §4.2): one [`Tile`] per slot of a rack, shared
//! by Lists ▸ PLC (screen m of the UI spec) and the generated `plcrack` sheet.
//!
//! # Semantics
//! - [`rack_tiles`]: the rack tile first, then one tile per slot from the rack's first slot to the
//!   highest occupied one (gaps as free tiles), then one free tile for the next free slot unless the
//!   rack is full. Never one tile per slot of a 999-slot rack.
//! - [`unracked_tiles`]: one tile per card in no rack, by tag.
//! - Everything is read from the kept [`Plc`] model and the document; nothing is stored.
//!
//! Excludes: drawing. Lists ▸ PLC draws the tiles in `ecad-trial`; the sheet in
//! [`crate::generated`] (via [`sheet_parts`]).

use ecad_model::{ComponentId, Document, PageId, Subject};
use serde_json::{Value, json};

use super::rack::{CardType, RackRef};
use super::{CardData, Plc, PlcFix, RackData};
use crate::checks::{Fix, Object};

/// Most tiles one rack shows (the free tile included): a rack is at most [`super::rack::MAX_SLOT`]
/// slots, but a rack tile per slot of a hostile `plc.slots 999` with one card in slot 999 is still
/// bounded here.
pub const MAX_TILES: usize = 1_001;

/// One channel square (design 051 §3.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Square {
    /// On a net with another connection point.
    Used,
    /// No wire and no description.
    Spare,
    /// A description and no wire yet (Q10).
    Reserved,
    /// A Checks row names this channel (wins over the others).
    Check,
}

impl Square {
    /// The word of the legend and of a hover.
    pub fn words(self) -> &'static str {
        match self {
            Square::Used => "in use",
            Square::Spare => "free",
            Square::Reserved => "reserved",
            Square::Check => "has a check",
        }
    }
}

/// What a tile stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TileKind {
    /// The rack itself (its body).
    Rack(ComponentId),
    /// A card in a slot (or in no rack).
    Card(ComponentId),
    /// A free slot.
    Free,
}

/// One tile.
#[derive(Debug, Clone, PartialEq)]
pub struct Tile {
    pub kind: TileKind,
    /// Its slot (none for the rack tile and a card in no rack).
    pub slot: Option<u32>,
    /// `Slot 2`, `Rack`, `No rack`.
    pub head: String,
    /// The tag, or `Free`.
    pub tag: String,
    /// `DQ 16`, or the card's description; the rack's type.
    pub what: String,
    /// `Q0.0–Q1.7`; `—` without addresses; the rack's `32 slots, 6 used`.
    pub range: String,
    /// One per channel, in channel order.
    pub squares: Vec<Square>,
    /// The pages it is drawn on, in book order.
    pub pages: Vec<PageId>,
    /// Its checks.
    pub checks: usize,
    /// Where its first drawing is, in the project's reference style (`108-B-2`).
    pub place: Option<String>,
    /// Drawn: a card with channels has a channel cell on a page; any other tile has a drawing.
    pub drawn: bool,
}

impl Tile {
    /// The element name's part for this tile (design 051 §3.8): `rack`, the slot, `free.<slot>`,
    /// `norack.<tag>`.
    pub fn element(&self) -> String {
        match (self.kind, self.slot) {
            (TileKind::Rack(_), _) => "rack".into(),
            (TileKind::Free, Some(s)) => format!("free.{s}"),
            (TileKind::Free, None) => "free".into(),
            (TileKind::Card(_), Some(s)) => s.to_string(),
            (TileKind::Card(_), None) => format!("norack.{}", self.tag),
        }
    }

    /// The card it shows, if any.
    pub fn card(&self) -> Option<ComponentId> {
        match self.kind {
            TileKind::Card(c) => Some(c),
            _ => None,
        }
    }

    /// The tile's data as `plcracks` adds it to a slot (design 051 §7).
    pub fn json(&self, doc: &Document) -> Value {
        json!({
            "pages": self.pages.iter().map(|p| crate::references::page_number(doc, *p)).collect::<Vec<_>>(),
            "checks": self.checks,
            "range": self.range,
        })
    }
}

/// The pages `c` is drawn on, in book order.
pub fn pages_of(doc: &Document, c: ComponentId) -> Vec<PageId> {
    let pages: std::collections::BTreeSet<PageId> = doc.representations_of(Subject::Component(c)).map(|(_, r)| r.page).collect();
    if pages.is_empty() {
        return Vec::new();
    }
    doc.page_order.iter().filter(|p| pages.contains(p)).copied().collect()
}

/// The place of `c`'s first drawing in book order (its body or a cell), as `108-B-2`.
pub fn first_place(doc: &Document, c: ComponentId) -> Option<String> {
    let first = pages_of(doc, c).into_iter().next()?;
    let r = doc.representations_of(Subject::Component(c)).map(|(_, r)| r).find(|r| r.page == first)?;
    super::iolist::rep_place(doc, r).0
}

/// Where `c` is first drawn on `page` (a symbol's origin), for a go-to.
pub fn at_on(doc: &Document, c: ComponentId, page: PageId) -> Option<ecad_model::Pt> {
    doc.representations_of(Subject::Component(c))
        .filter(|(_, r)| r.page == page)
        .find_map(|(_, r)| crate::devices::rep_definition(doc, r).map(|(_, pl)| pl.origin).or_else(|| r.connection_points.values().next().copied()))
}

/// Card `card`'s Checks rows, and the channels they name (`ChannelTwice`, `ChannelOver`,
/// `ClearTyped` carry the channel's index).
pub fn checks_of(plc: &Plc, card: ComponentId) -> (usize, Vec<u32>) {
    let mut n = 0;
    let mut chans = Vec::new();
    for c in plc.checks_naming(card) {
        n += 1;
        if let Some(Fix::Plc(_, PlcFix::ChannelTwice(i) | PlcFix::ChannelOver(i) | PlcFix::ClearTyped(i))) = &c.fix {
            chans.push(*i);
        }
    }
    (n, chans)
}

/// The square of each channel `0..channels` of `card`.
pub fn squares_of(card: &CardData, checked: &[u32]) -> Vec<Square> {
    (0..card.channels)
        .map(|i| {
            if checked.contains(&i) {
                return Square::Check;
            }
            match card.chans.get(&i) {
                Some(ch) if ch.used => Square::Used,
                Some(ch) if ch.text.as_deref().is_some_and(|t| !t.is_empty()) => Square::Reserved,
                _ => Square::Spare,
            }
        })
        .collect()
}

/// The card's address spans (inputs, outputs) as `I0.0–I1.7`, each `None` when it has none.
pub fn spans(doc: &Document, plc: &Plc, card: &CardData) -> (Option<String>, Option<String>) {
    let last = card.channels.saturating_sub(1);
    let span = |output: bool| -> Option<String> {
        if card.channels == 0 {
            return None;
        }
        let a = plc.automatic(doc, card.id, 0, output)?;
        let b = plc.automatic(doc, card.id, last, output)?;
        Some(if a == b { a } else { format!("{a}\u{2013}{b}") })
    };
    match card.ty {
        CardType::Iol => (span(false), span(true)),
        t if super::address::shows_output(t) => (None, span(true)),
        _ => (span(false), None),
    }
}

/// `I0.0–I1.7` (inputs, then outputs for IO-Link: `IW12–IW18 / QW0–QW6`); `—` when the card has no
/// automatic addresses.
pub fn range_words(doc: &Document, plc: &Plc, card: &CardData) -> String {
    match spans(doc, plc, card) {
        (Some(i), Some(o)) => format!("{i} / {o}"),
        (Some(x), None) | (None, Some(x)) => x,
        (None, None) => "\u{2014}".into(),
    }
}
/// `DQ 16`, or the card's description when it has one.
pub fn what_words(doc: &Document, card: &CardData) -> String {
    // The body's own name, which placing a card writes as its description, is not the user's.
    let builtin = crate::symbols::by_key(card.ty.body()).map(|s| s.name);
    let desc =
        doc.components.get(&card.id).and_then(|x| x.properties.get("description")).map(|v| v.trim()).filter(|v| !v.is_empty() && Some(*v) != builtin);
    match desc {
        Some(d) => d.to_string(),
        None if card.ty.has_channels() => format!("{} {}", card.ty.code(), card.channels),
        None => card.ty.code().to_string(),
    }
}

/// The tile of card `card` in `slot`.
pub fn card_tile(doc: &Document, plc: &Plc, card: &CardData, head: String) -> Tile {
    let (checks, checked) = checks_of(plc, card.id);
    Tile {
        kind: TileKind::Card(card.id),
        slot: card.slot.filter(|_| matches!(card.rack, RackRef::Rack(_))),
        head,
        tag: card.tag.clone(),
        what: what_words(doc, card),
        range: if card.ty.has_channels() { range_words(doc, plc, card) } else { String::new() },
        squares: if card.ty.has_channels() { squares_of(card, &checked) } else { Vec::new() },
        pages: pages_of(doc, card.id),
        checks,
        place: first_place(doc, card.id),
        drawn: if card.ty.has_channels() { card.chans.values().any(|c| !c.cells.is_empty()) } else { !pages_of(doc, card.id).is_empty() },
    }
}

/// A free tile for `slot`.
fn free_tile(slot: u32) -> Tile {
    Tile {
        kind: TileKind::Free,
        slot: Some(slot),
        head: format!("Slot {slot}"),
        tag: "Free".into(),
        what: String::new(),
        range: String::new(),
        squares: Vec::new(),
        pages: Vec::new(),
        checks: 0,
        place: None,
        drawn: false,
    }
}

/// `32 slots, 6 used`.
pub fn rack_words(plc: &Plc, rack: &RackData) -> String {
    let last = rack.first.saturating_add(rack.slots).saturating_sub(1);
    let used: std::collections::BTreeSet<u32> =
        plc.cards_of(rack.id).into_iter().filter_map(|c| c.slot).filter(|s| (rack.first..=last).contains(s)).collect();
    let s = if rack.slots == 1 { "slot" } else { "slots" };
    format!("{} {s}, {} used", rack.slots, used.len())
}

/// The tiles after a rack's own, as (head, slot, card): slots from the first to the highest
/// occupied (gaps free), the next free slot unless the rack is full, then cards outside the range
/// or in no slot. At most [`MAX_TILES`] − 1.
pub fn layout(plc: &Plc, rack: &RackData) -> Vec<(String, Option<u32>, Option<ComponentId>)> {
    let mut out = Vec::new();
    let cards = plc.cards_of(rack.id);
    let last = rack.first.saturating_add(rack.slots).saturating_sub(1);
    let in_range = |c: &&&CardData| c.slot.is_some_and(|s| (rack.first..=last).contains(&s));
    let highest = cards.iter().filter(in_range).filter_map(|c| c.slot).max();
    let cap = MAX_TILES - 1;
    if let Some(h) = highest {
        // `cards` is in slot order: one walk.
        let mut it = cards.iter().filter(in_range).peekable();
        for s in rack.first..=h {
            if out.len() >= cap {
                break;
            }
            let mut any = false;
            while let Some(c) = it.next_if(|c| c.slot == Some(s)) {
                any = true;
                out.push((format!("Slot {s}"), Some(s), Some(c.id)));
            }
            if !any {
                out.push((format!("Slot {s}"), Some(s), None));
            }
        }
    }
    let next = highest.map_or(Some(rack.first), |h| h.checked_add(1).filter(|n| *n <= last));
    if let Some(n) = next
        && out.len() < cap
    {
        out.push((format!("Slot {n}"), Some(n), None));
    }
    for c in cards.iter().filter(|c| !in_range(c)) {
        if out.len() >= cap {
            break;
        }
        let head = c.slot.map_or_else(|| "No slot".to_string(), |s| format!("Slot {s}"));
        out.push((head, None, Some(c.id)));
    }
    out
}

/// How many tiles rack `rack` shows (its own included), without making them.
pub fn tile_count(plc: &Plc, rack: &RackData) -> usize {
    1 + layout(plc, rack).len()
}

/// The tiles of rack `rack` (module docs).
pub fn rack_tiles(doc: &Document, plc: &Plc, rack: &RackData) -> Vec<Tile> {
    let mut out = vec![Tile {
        kind: TileKind::Rack(rack.id),
        slot: None,
        head: "Rack".into(),
        tag: rack.tag.clone(),
        what: rack.ty.code().to_string(),
        range: rack_words(plc, rack),
        squares: Vec::new(),
        pages: pages_of(doc, rack.id),
        checks: plc.checks_naming(rack.id).count(),
        place: first_place(doc, rack.id),
        drawn: true,
    }];
    for (head, slot, card) in layout(plc, rack) {
        match card.and_then(|c| plc.cards.get(&c)) {
            Some(c) => out.push(card_tile(doc, plc, c, head)),
            None => out.push(free_tile(slot.unwrap_or(rack.first))),
        }
    }
    out
}

/// The tiles of the cards in no rack (or naming a missing one), by tag.
pub fn unracked_tiles(doc: &Document, plc: &Plc) -> Vec<Tile> {
    plc.unracked_by_tag().into_iter().filter_map(|c| plc.cards.get(&c)).take(MAX_TILES).map(|c| card_tile(doc, plc, c, "No rack".into())).collect()
}

/// `n channels, m in use` over `cards`.
pub fn totals(cards: &[&CardData]) -> (u64, u64) {
    let n: u64 = cards.iter().filter(|c| c.ty.has_channels()).map(|c| u64::from(c.channels)).sum();
    let used: u64 =
        cards.iter().filter(|c| c.ty.has_channels()).map(|c| c.chans.iter().filter(|(i, ch)| **i < c.channels && ch.used).count() as u64).sum();
    (n, used)
}

/// `66 channels, 44 in use`.
pub fn totals_words((n, used): (u64, u64)) -> String {
    let ch = if n == 1 { "channel" } else { "channels" };
    format!("{n} {ch}, {used} in use")
}

/// One choice of Lists ▸ PLC's rack chips (design 051 §3.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chip {
    Rack(ComponentId, String),
    /// The cards in no rack, and how many.
    NoRack(usize),
}

impl Chip {
    /// `-A01 CPU`, `No rack 2`.
    pub fn words(&self) -> String {
        match self {
            Chip::Rack(_, w) => w.clone(),
            Chip::NoRack(n) => format!("No rack {n}"),
        }
    }

    /// The element name's part: the rack's tag, or `none`.
    pub fn element(&self, plc: &Plc) -> String {
        match self {
            Chip::Rack(r, _) => format!("ecad.plc.rack.{}", plc.racks.get(r).map_or("?", |x| x.tag.as_str())),
            Chip::NoRack(_) => "ecad.plc.rack.none".into(),
        }
    }
}

/// The rack chips: one per rack by tag, then `No rack n` when cards are in no rack.
pub fn chips(plc: &Plc) -> Vec<Chip> {
    let mut out: Vec<Chip> = plc.racks_by_tag().into_iter().map(|r| Chip::Rack(r.id, format!("{} {}", r.tag, r.ty.code()))).collect();
    let n = plc.unracked_by_tag().len();
    if n > 0 {
        out.push(Chip::NoRack(n));
    }
    out
}

/// The tiles of a chip, and its totals.
pub fn chip_tiles(doc: &Document, plc: &Plc, chip: &Chip) -> (Vec<Tile>, (u64, u64)) {
    match chip {
        Chip::Rack(r, _) => match plc.racks.get(r) {
            Some(rack) => (rack_tiles(doc, plc, rack), totals(&plc.cards_of(*r))),
            None => (Vec::new(), (0, 0)),
        },
        Chip::NoRack(_) => {
            let cards: Vec<&CardData> = plc.unracked_by_tag().into_iter().filter_map(|c| plc.cards.get(&c)).collect();
            (unracked_tiles(doc, plc), totals(&cards))
        }
    }
}

/// The totals of a chip, without making its tiles.
pub fn chip_totals(plc: &Plc, chip: &Chip) -> (u64, u64) {
    match chip {
        Chip::Rack(r, _) => totals(&plc.cards_of(*r)),
        Chip::NoRack(_) => {
            let cards: Vec<&CardData> = plc.unracked_by_tag().into_iter().filter_map(|c| plc.cards.get(&c)).collect();
            totals(&cards)
        }
    }
}

/// Whether a Checks row is about `c`.
pub(crate) fn names(o: &Object, c: ComponentId) -> bool {
    *o == Object::Component(c)
}
