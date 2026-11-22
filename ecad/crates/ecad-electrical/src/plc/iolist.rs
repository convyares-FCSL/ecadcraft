//! The I/O list (design 045 §8): one row per channel of every card with channels, drawn or not,
//! as `iolist` (read only), the generated page `iolist`, the release CSV and Lists ▸ I/O.
//!
//! # Semantics
//! - Order: racks by tag, their cards by slot, then cards in no rack by tag; channels by index.
//!   A card lists its channels `0..count` and any channel drawn or described beyond them.
//! - Rows are derived from the kept `crate::plc::Plc` model and the document at the moment they are
//!   asked for; the page, the list and `iolist` callers redraw only when the model says a card
//!   changed (`crate::plc::Plc::rows_changed`).
//! - `editable` names the fields the PLC manager (2.8) will edit, and the command that writes each.

use ecad_model::{DeviceId, Document, PageId, Pt};
use serde_json::{Value, json};

use crate::plc::rack::RackRef;
use crate::plc::{CardData, channels, kept::Plc, state_word};
use crate::reports::{Cell, Table};

/// Said by the release when the project has no card with channels (design 045 §8.3).
pub const NO_CARDS: &str = "No PLC cards with channels.";
/// Most targets one row names.
pub const MAX_TARGETS: usize = 8;
/// The fields of a row the PLC manager edits, with the command that writes each.
pub const EDITABLE: [(&str, &str); 5] =
    [("address", "plcchannel"), ("pins", "plcchannel"), ("text", "plcchannel"), ("tag", "plcchannel"), ("safe", "plcchannel")];

/// What a channel is wired to: another connection point on its nets.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    pub device: String,
    pub point: String,
    pub place: String,
    /// Brief 056 (design 056 §4.5): a terminal or connector half, which the channel passes through.
    pub through: bool,
    /// Where it is drawn (a go-to).
    pub page: Option<PageId>,
    pub at: Option<Pt>,
}

/// One I/O row (design 045 §8.1).
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub card: DeviceId,
    pub card_tag: String,
    pub rack: Option<String>,
    pub slot: Option<u32>,
    pub ty: &'static str,
    /// The channel number shown, and its index.
    pub channel: u32,
    pub index: u32,
    pub address: Option<String>,
    pub address_out: Option<String>,
    pub typed: bool,
    pub pins: Vec<(ecad_model::ChannelRole, String)>,
    pub text: Option<String>,
    pub state: &'static str,
    pub drawn: bool,
    /// Where its cell is (`105-C-2`), its page and point.
    pub place: Option<String>,
    pub page: Option<PageId>,
    pub at: Option<Pt>,
    pub targets: Vec<Target>,
    pub wire: Option<String>,
    /// Brief 056 (design 056 §4.3, §4.4): the PLC tag, the own safe state, and the default an
    /// output without one takes (`None` for inputs).
    pub tag: Option<String>,
    pub safe: Option<String>,
    pub safe_default: Option<String>,
    /// The Test column (design 056 §4.6), from the Tests brief; always `None` in 056.
    pub test: crate::plc::signals::TestState,
}

impl Row {
    /// The safe state shown: the own one, else the default; and whether it is the default.
    pub fn safe_shown(&self) -> Option<(&str, bool)> {
        match (&self.safe, &self.safe_default) {
            (Some(s), _) => Some((s.as_str(), false)),
            (None, Some(d)) => Some((d.as_str(), true)),
            (None, None) => None,
        }
    }

    /// A channel nobody uses or named: no tag, no description, not wired.
    pub fn is_free(&self) -> bool {
        self.state == "spare" && self.tag.is_none()
    }
}

/// The place of a symbol representation, in the project's reference style.
pub(crate) fn rep_place(doc: &Document, r: &ecad_model::Representation) -> (Option<String>, Option<Pt>) {
    let at = crate::devices::rep_definition(doc, r).map(|(_, pl)| pl.origin).or_else(|| r.connection_points.values().next().copied());
    let place = at
        .and_then(|a| ecad_model::connectivity::wiring::place(doc, r.page, a))
        .map(|p| p.text(doc.settings.reference_style()))
        .or_else(|| Some(crate::references::cables::page_number_words(doc, r.page)));
    (place, at)
}

/// The cards with channels in list order.
pub fn cards_in_order(plc: &Plc) -> Vec<&CardData> {
    let mut out: Vec<&CardData> = Vec::new();
    for r in plc.racks_by_tag() {
        out.extend(plc.cards_of(r.id).into_iter().filter(|c| c.ty.has_channels()));
    }
    out.extend(plc.unracked_by_tag().into_iter().filter_map(|c| plc.cards.get(&c)).filter(|c| c.ty.has_channels()));
    out
}

/// The indexes a card lists: `0..count` and any channel drawn or described beyond.
pub fn indexes(card: &CardData) -> Vec<u32> {
    let mut v: std::collections::BTreeSet<u32> = (0..card.channels).collect();
    v.extend(card.chans.keys().copied());
    v.into_iter().collect()
}

/// Card `card`'s rows.
pub fn rows_of(doc: &Document, plc: &Plc, card: &CardData) -> Vec<Row> {
    let conn = doc.connectivity();
    let rack = match card.rack {
        RackRef::Rack(r) => plc.racks.get(&r).map(|x| x.tag.clone()),
        _ => None,
    };
    let mut out = Vec::new();
    let safe_default = crate::plc::signals::safe_default(doc, card.id, card.ty);
    for i in indexes(card) {
        let ch = card.chans.get(&i);
        let (address, typed) = plc.address_of(doc, card.id, i);
        let address_out = (card.ty == crate::plc::rack::CardType::Iol).then(|| plc.automatic(doc, card.id, i, true)).flatten();
        let pins: Vec<(ecad_model::ChannelRole, String)> = ch.map(|c| c.points.iter().map(|(r, _, p)| (*r, p.clone())).collect()).unwrap_or_default();
        let (place, page, at) = match ch.and_then(|c| c.cells.first()).and_then(|(rid, _, _)| doc.representations.get(rid)) {
            Some(r) => {
                let (p, a) = rep_place(doc, r);
                (p, Some(r.page), a)
            }
            None => (None, None, None),
        };
        let mut targets = Vec::new();
        let mut wire = None;
        for (_, t, _) in ch.map(|c| c.points.as_slice()).unwrap_or_default() {
            let Some(net) = conn.net_of_connection_point(*t).and_then(|n| conn.net(n)) else { continue };
            if wire.is_none() {
                wire = ecad_model::connectivity::wiring::net_number(doc, net);
            }
            for o in &net.terminals {
                let Some(term) = doc.terminals.get(o) else { continue };
                if term.device() == Some(card.id) || targets.len() >= 2 * MAX_TARGETS {
                    continue;
                }
                // A junction or continuation is not a target.
                let Some(device) = term.device().filter(|c| crate::devices::tag_of(doc, *c).is_some()) else {
                    continue;
                };
                // Brief 056: pass-throughs and end devices are capped apart, so terminals never
                // crowd out what the channel drives.
                let through = crate::plc::signals::is_pass_through(doc, device);
                if targets.iter().filter(|t: &&Target| t.through == through).count() >= MAX_TARGETS {
                    continue;
                }
                let shown = doc.representations_showing(*o).next();
                let (place, at) = shown.map(|(_, r)| rep_place(doc, r)).unwrap_or((None, None));
                let at = shown.and_then(|(_, r)| r.connection_points.get(o).copied()).or(at);
                targets.push(Target {
                    device: crate::devices::tag_words(doc, device),
                    point: crate::terminal_strips::terminal_blocks::connection_point_name(doc, *o),
                    place: place.unwrap_or_default(),
                    through,
                    page: shown.map(|(_, r)| r.page),
                    at,
                });
            }
        }
        targets.sort_by(|a, b| (a.device.as_str(), a.point.as_str()).cmp(&(b.device.as_str(), b.point.as_str())));
        targets.dedup();
        let used = ch.is_some_and(|c| c.used);
        let text = ch.and_then(|c| c.text.clone());
        let tag = ch.and_then(|c| c.tag.clone());
        out.push(Row {
            card: card.id,
            card_tag: card.tag.clone(),
            rack: rack.clone(),
            slot: matches!(card.rack, RackRef::Rack(_)).then_some(card.slot).flatten(),
            ty: card.ty.code(),
            channel: i.saturating_add(card.first),
            index: i,
            address,
            address_out,
            typed,
            pins,
            state: state_word(used, text.as_deref().or(tag.as_deref())),
            text,
            safe: ch.and_then(|c| c.safe.clone()).filter(|_| safe_default.is_some()),
            safe_default: safe_default.clone(),
            tag,
            test: crate::plc::signals::TestState::None,
            drawn: ch.is_some_and(|c| !c.points.is_empty()),
            place,
            page,
            at,
            targets,
            wire,
        });
    }
    out
}

/// `+:1 -:5` (one column for the CSV and the page).
pub fn pins_words(pins: &[(ecad_model::ChannelRole, String)]) -> String {
    if pins.len() == 1 {
        return pins.first().map(|(_, p)| p.clone()).unwrap_or_default();
    }
    pins.iter().map(|(r, p)| format!("{}:{p}", ecad_model::Word::word(*r))).collect::<Vec<_>>().join(" ")
}

/// `-S03:14, -K01:A1`.
pub fn targets_words(t: &[Target]) -> String {
    let t: Vec<&Target> = t.iter().take(MAX_TARGETS).collect();
    t.iter().map(|x| format!("{}:{}", x.device, x.point)).collect::<Vec<_>>().join(", ")
}

/// The address as listed: a typed one marked `*`.
pub fn address_words(r: &Row) -> String {
    match (&r.address, r.typed) {
        (Some(a), true) => format!("{a}*"),
        (Some(a), false) => a.clone(),
        (None, _) => String::new(),
    }
}

/// A row as `iolist` returns it.
pub fn row_json(r: &Row) -> Value {
    json!({
        "rack": r.rack,
        "slot": r.slot,
        "card": r.card.hex(),
        "tag": r.card_tag,
        "type": r.ty,
        "channel": r.channel,
        "index": r.index,
        "address": r.address,
        "address_out": r.address_out,
        "address_typed": r.typed,
        "pins": r.pins.iter().map(|(role, pin)| json!({"role": role, "pin": pin})).collect::<Vec<_>>(),
        "text": r.text,
        "state": r.state,
        "drawn": r.drawn,
        "place": r.place,
        "targets": r.targets.iter().map(|t| json!({"device": t.device, "point": t.point, "place": t.place})).collect::<Vec<_>>(),
        "wire": r.wire,
        // Brief 056 (design 056 §10): `tag` is the card's already, so the PLC tag is `plc_tag`.
        "plc_tag": r.tag,
        "safe": r.safe_shown().map(|(s, _)| s),
        "safe_default": r.safe_shown().is_some_and(|(_, d)| d),
        "drives": crate::plc::signals::drives(&r.targets).0.iter().map(|t| json!({"device": t.device, "point": t.point, "place": t.place})).collect::<Vec<_>>(),
        "test": r.test.json(),
        "editable": EDITABLE.iter().map(|(f, c)| json!({"field": f, "command": c})).collect::<Vec<_>>(),
    })
}

/// `16 channels: 9 used, 1 reserved, 6 spare`.
pub fn summary(rows: &[Row]) -> (usize, usize, usize) {
    let n = |s: &str| rows.iter().filter(|r| r.state == s).count();
    (n("used"), n("reserved"), n("spare"))
}

/// A card's heading line on the page (design 045 §8.2).
pub fn heading(card: &CardData, rows: &[Row], plc: &Plc) -> String {
    let (u, r, s) = summary(rows);
    let place = match card.rack {
        RackRef::Rack(x) => format!("{} slot {}", plc.racks.get(&x).map_or("?", |r| r.tag.as_str()), card.slot.map_or("?".into(), |s| s.to_string())),
        _ => "no rack".into(),
    };
    let part = card.part_number.clone().unwrap_or_default();
    format!("{place}  {}  {} {}  {part}  {u} used, {r} reserved, {s} spare", card.tag, card.ty.code(), card.channels)
}

/// The page's lines: per card a heading, then its rows (`Slot | Card | Ch | Address | Tag |
/// Description | Safe | Pins | Wired to | Wire | Place`).
pub fn page_lines(doc: &Document, plc: &Plc) -> Vec<(bool, Vec<String>)> {
    let spare = crate::defaults::resolve(doc, &crate::defaults::plc::SPARE_TEXT, crate::defaults::Scope::project()).text();
    let mut out = Vec::new();
    for card in cards_in_order(plc) {
        let rows = rows_of(doc, plc, card);
        out.push((true, vec![heading(card, &rows, plc)]));
        for r in &rows {
            let desc = match (&r.text, r.state) {
                (Some(t), _) => t.clone(),
                (None, "spare") => spare.clone(),
                (None, _) => String::new(),
            };
            out.push((
                false,
                vec![
                    r.slot.map(|s| s.to_string()).unwrap_or_default(),
                    r.card_tag.clone(),
                    r.channel.to_string(),
                    address_words(r),
                    // Brief 056 (Q9): the tag and the safe state.
                    r.tag.clone().unwrap_or_default(),
                    desc,
                    r.safe_shown().map(|(s, _)| crate::plc::signals::safe_words(s)).unwrap_or_default(),
                    pins_words(&r.pins),
                    targets_words(&r.targets),
                    r.wire.clone().unwrap_or_default(),
                    r.place.clone().unwrap_or_default(),
                ],
            ));
        }
    }
    out
}

/// The page's columns with their A3 widths (brief 056, Q9: Tag and Safe; Description narrower).
pub const PAGE_COLUMNS: [(&str, f64); 11] = [
    ("Slot", 14.0),
    ("Card", 22.0),
    ("Ch", 12.0),
    ("Address", 30.0),
    ("Tag", 28.0),
    ("Description", 70.0),
    ("Safe", 12.0),
    ("Pins", 30.0),
    ("Wired to", 90.0),
    ("Wire", 30.0),
    ("Place", 52.0),
];

/// How many lines the page has, from the model's counts (no rows made): a heading and the
/// listed channels per card.
pub fn page_heads(plc: &Plc) -> Vec<Option<String>> {
    let mut v = Vec::new();
    for card in cards_in_order(plc) {
        v.push(Some(card.tag.clone()));
        v.extend(std::iter::repeat_n(None, indexes(card).len()));
    }
    v
}

/// The release CSV (design 045 §8.3), `None` when no card has channels.
pub fn csv_table(doc: &Document, plc: &Plc) -> Option<Table> {
    let cards = cards_in_order(plc);
    if cards.is_empty() {
        return None;
    }
    let c = |t: String| Cell { text: t, flag: None };
    let mut rows = Vec::new();
    for card in cards {
        for r in rows_of(doc, plc, card) {
            rows.push(vec![
                c(r.rack.clone().unwrap_or_default()),
                c(r.slot.map(|s| s.to_string()).unwrap_or_default()),
                c(r.card_tag.clone()),
                c(r.ty.to_string()),
                c(r.channel.to_string()),
                c(r.address.clone().unwrap_or_default()),
                c(r.address_out.clone().unwrap_or_default()),
                c(if r.typed { "yes".into() } else { String::new() }),
                c(pins_words(&r.pins)),
                c(r.text.clone().unwrap_or_default()),
                c(r.state.to_string()),
                c(if r.drawn { "yes".into() } else { "no".into() }),
                c(r.place.clone().unwrap_or_default()),
                c(targets_words(&r.targets)),
                c(r.wire.clone().unwrap_or_default()),
                // Brief 056 (design 056 §7.3): the tag and the safe state.
                c(r.tag.clone().unwrap_or_default()),
                c(r.safe_shown().map(|(s, _)| s.to_string()).unwrap_or_default()),
            ]);
        }
    }
    Some(Table {
        headers: vec![
            "Rack",
            "Slot",
            "Card",
            "Type",
            "Channel",
            "Address",
            "Output address",
            "Typed",
            "Pins",
            "Description",
            "State",
            "Drawn",
            "Place",
            "Wired to",
            "Wire",
            "Tag",
            "Safe",
        ],
        rows,
    })
}

/// The cards a change of device `d` (its tag) or conductor `w` (its number) touches through their
/// nets: the cards with a channel point on them.
pub fn cards_on_nets_of(doc: &Document, plc: &Plc, d: Option<DeviceId>, w: Option<ecad_model::WireId>) -> Vec<DeviceId> {
    let conn = doc.connectivity();
    let mut nets = Vec::new();
    if let Some(d) = d {
        nets.extend(doc.connection_points_of(d).filter_map(|(t, _)| conn.net_of_connection_point(t)));
    }
    if let Some(w) = w {
        nets.extend(conn.net_of_conductor(w));
    }
    let mut out = Vec::new();
    for n in nets {
        let Some(net) = conn.net(n) else { continue };
        for t in &net.terminals {
            if let Some(x) = doc.terminals.get(t)
                && let Some(c) = x.device()
                && plc.cards.contains_key(&c)
                && channels::point_of(doc, *t).is_some()
            {
                out.extend(x.device());
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// The cards with a channel point on the nets of terminals `ts`.
pub fn cards_on_nets_of_terminals(doc: &Document, plc: &Plc, ts: &[ecad_model::ConnectionPointId]) -> Vec<DeviceId> {
    let conn = doc.connectivity();
    let mut out = Vec::new();
    for n in ts.iter().filter_map(|t| conn.net_of_connection_point(*t)) {
        let Some(net) = conn.net(n) else { continue };
        for t in &net.terminals {
            if let Some(x) = doc.terminals.get(t)
                && let Some(c) = x.device()
                && plc.cards.contains_key(&c)
                && channels::point_of(doc, *t).is_some()
            {
                out.extend(x.device());
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// The cells of one I/O row, in [`crate::plc::sheets::IO_COLUMNS`] order.
pub fn io_cells(r: &Row, spare: &str) -> [String; 8] {
    let desc = match (&r.text, r.state) {
        (Some(t), _) => t.clone(),
        (None, "spare") => spare.to_string(),
        (None, _) => String::new(),
    };
    [
        r.channel.to_string(),
        address_words(r),
        pins_words(&r.pins),
        desc,
        r.state.to_string(),
        targets_words(&r.targets),
        r.wire.clone().unwrap_or_default(),
        r.place.clone().unwrap_or_default(),
    ]
}
