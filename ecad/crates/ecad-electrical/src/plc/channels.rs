//! Channels (design 045 §4, §6): channel points, pins, joining a card, Place Channels (the fence
//! and the rest) and `plcchannel`.
//!
//! # Semantics
//! - A channel point is a `ConnectionPoint` of the card: its **label is the pin** (what is printed and
//!   typed after the tag, `-A02:4`), its **key is `.<i>.<role>`** (the 0-based channel index and
//!   the role, FP2). The channel number shown is `i + plc.channel.first`, so changing the first
//!   number never rewrites a terminal.
//! - A channel symbol's own points are keyed by role alone (`.in`, [`PointKey::Role`]); joining
//!   a card gives each the channel's index ([`PointKey::Channel`], design 072 §3).
//! - Pins come from the card's pin layout (`plc.pins`, §4.3) once, when drawn, and are never
//!   recomputed; `plcchannel {pins}` relabels them.
//! - Spare and reserved are derived, never stored (§6).
//!
//! Every write (the commands, in ecad-engine's `cmd::plc`) is one transaction.

use crate::CmdError;
use ecad_model::Word;
use ecad_model::geometry::units::Radians;
use std::collections::{BTreeMap, BTreeSet};

use ecad_model::{
    ChannelRole, ConnectionPoint, ConnectionPointId, DefinitionId, DeviceId, Document, Form, IdAllocator, Item, ItemId, ItemKind, Op, PageId,
    Placement, PointKey, Pt, Representation, RepresentationId, Style, Subject, SymbolInstance,
};

use crate::defaults::{self, Scope};
use crate::plc::card_type_of;
use crate::plc::rack::{CardType, MAX_CHANNELS, channel_count};
use crate::symbols::{Kind, Starter};

/// The property prefix of a channel's own data (`plc.ch.<i>.text`, `plc.ch.<i>.address`; FP3).
pub(crate) const CH_PREFIX: &str = "plc.ch.";
/// Most characters of a channel description (design 045 §4.6).
pub const MAX_TEXT: usize = 80;
/// Most characters of a typed address.
pub(crate) const MAX_ADDRESS: usize = 32;
/// Most characters of a pin.
pub(crate) const MAX_PIN: usize = 16;
/// Most wires one Place Channels line lands on (design 045 §4.5).
pub const MAX_FENCE: usize = 256;
/// The highest channel index read from a key (larger keys are not channel points).
pub const MAX_INDEX: u32 = 9_999;

/// A channel point's key: `.<i>.<role>`; `None` for an index over [`MAX_INDEX`].
pub fn key(i: u32, role: ChannelRole) -> Option<PointKey> {
    let index = u16::try_from(i).ok().filter(|n| u32::from(*n) <= MAX_INDEX)?;
    Some(PointKey::Channel { index, role })
}

/// The channel and role of terminal `t` of a card; `None` for a point that is not a channel's.
pub(crate) fn point_of(doc: &Document, t: ConnectionPointId) -> Option<(u32, ChannelRole)> {
    let (i, role) = doc.connection_points.get(&t)?.key.as_ref()?.channel()?;
    Some((u32::from(i), role))
}

/// The channel points of card `c`: index → (role, terminal, pin), roles in their cell order.
pub fn points(doc: &Document, c: DeviceId) -> BTreeMap<u32, Vec<(ChannelRole, ConnectionPointId, String)>> {
    let mut out: BTreeMap<u32, Vec<(ChannelRole, ConnectionPointId, String)>> = BTreeMap::new();
    for (t, term) in doc.connection_points_of(c) {
        if let Some((i, role)) = point_of(doc, t) {
            out.entry(i).or_default().push((role, t, term.label.clone().unwrap_or_default()));
        }
    }
    for v in out.values_mut() {
        v.sort_by_key(|(r, t, _)| (*r, *t));
    }
    out
}

/// The card type a channel symbol draws.
pub fn cell_type(s: &Starter) -> Option<CardType> {
    match s.kind {
        Kind::Channel(t) => Some(t),
        _ => None,
    }
}

/// The roles a channel symbol has, in its definition order.
pub fn roles_of(s: &Starter) -> Vec<ChannelRole> {
    let Some(t) = cell_type(s) else { return Vec::new() };
    let Ok(def) = s.definition() else { return Vec::new() };
    def.connection_points.iter().filter_map(|td| td.key.role()).filter(|r| t.roles().contains(r)).collect()
}

/// `chdi`, `di` for a DI card: the symbols that draw its channels.
fn cells_words(t: CardType) -> String {
    let mut v: Vec<String> = t.cell().map(|k| k.word().to_string()).into_iter().collect();
    match t {
        CardType::Di => v.push(crate::symbols::StarterKey::Di.word().into()),
        CardType::Dq => v.push(crate::symbols::StarterKey::Do.word().into()),
        _ => {}
    }
    v.join(", ")
}

/// The first channel number shown on card `c` (`plc.channel.first`, 0 or 1).
pub fn first_number(doc: &Document, c: DeviceId) -> u32 {
    let at = Scope { component: Some(c), ..Scope::project() };
    u32::from(defaults::resolve(doc, &defaults::plc::CHANNEL_FIRST, at).choice() == Some(defaults::plc::ChannelFirst::One))
}

/// Where channel `i` of card `c` is drawn, in words (`105-C-2`, or `page 3`).
fn drawn_words(doc: &Document, t: ConnectionPointId) -> String {
    let Some((_, r)) = doc.representations_showing(t).next() else { return "no page".into() };
    let at = crate::devices::tags::rep_definition(doc, r).map(|(_, pl)| pl.origin);
    match at.and_then(|a| ecad_model::connectivity::wiring::place(doc, r.page, a)) {
        Some(p) => p.text(doc.settings.reference_style()),
        None => crate::references::cables::page_number_words(doc, r.page),
    }
}

/// The pins a new channel `i` of card `c` gets for `roles` (design 045 §4.3): from the pin layout,
/// or the next free whole number when the computed one is taken (`used`, which gains them).
pub fn pins_for(
    doc: &Document, c: Option<DeviceId>, ty: CardType, n: u32, i: u32, roles: &[ChannelRole], used: &mut BTreeSet<String>,
) -> Vec<String> {
    let at = Scope { component: c, ..Scope::project() };
    let by_channel = defaults::resolve(doc, &defaults::plc::PINS, at).choice() == Some(defaults::plc::PlcPins::Channel);
    let all = ty.roles();
    let r_count = u32::try_from(all.len()).unwrap_or(1).max(1);
    let n = n.max(i.saturating_add(1));
    let mut out = Vec::new();
    for role in roles {
        let r = u32::try_from(all.iter().position(|x| x == role).unwrap_or(0)).unwrap_or(0);
        let pin = if by_channel {
            i.saturating_mul(r_count).saturating_add(r).saturating_add(1)
        } else {
            r.saturating_mul(n).saturating_add(i).saturating_add(1)
        };
        let mut p = pin;
        // Bounded: at most a few thousand labels on a card.
        let mut tries = 0;
        while used.contains(&p.to_string()) && tries < 100_000 {
            p = p.saturating_add(1);
            tries += 1;
        }
        used.insert(p.to_string());
        out.push(p.to_string());
    }
    out
}

/// What a channel symbol gets joining a card (design 045 §4.4).
#[derive(Debug, Clone, PartialEq)]
pub struct Join {
    pub index: u32,
    /// (role, pin), in the symbol's definition order.
    pub pins: Vec<(ChannelRole, String)>,
}

/// The channel and pins symbol `s` gets joining card `c` (design 045 §4.4): the channel numbered
/// `named` (as shown), else the lowest not drawn yet. `extra` are labels already given in the same
/// transaction; `taken` indexes too.
///
/// # Errors
/// The refusals of §4.4, each with the way out.
pub fn join(doc: &Document, c: DeviceId, s: &Starter, named: Option<u32>, extra: &BTreeSet<String>, taken: &BTreeSet<u32>) -> Result<Join, CmdError> {
    let who = crate::devices::tags::tag_words(doc, c);
    let t = cell_type(s).ok_or(CmdError::refused("That symbol is not a PLC channel."))?;
    let ct = match card_type_of(doc, c) {
        Some(ct) => ct,
        None => return Err(CmdError::refused(format!("{who} is not a PLC card; pick a card, or place the channel on its own for a new card."))),
    };
    if !ct.has_channels() {
        return Err(CmdError::refused(format!("{who} is a {}; channels go on its cards. Pick a card, or place a card body first.", ct.noun())));
    }
    if ct != t {
        return Err(CmdError::refused(format!("{who} is a {}; pick a {} channel ({}), or another card.", ct.noun(), ct.code(), cells_words(ct))));
    }
    let n = channel_count(doc, c);
    let pts = points(doc, c);
    let drawn = |i: u32| pts.contains_key(&i) || taken.contains(&i);
    let first = first_number(doc, c);
    let next_free = (0..n.min(MAX_CHANNELS)).find(|i| !drawn(*i));
    let index = match named {
        Some(shown) => {
            let i = shown.checked_sub(first).ok_or_else(|| CmdError::refused(format!("{who}'s channels start at {first}.")))?;
            if i > MAX_INDEX {
                return Err(CmdError::refused(format!("{who} has {n} channels.")));
            }
            if let Some((_, t0, _)) = pts.get(&i).and_then(|v| v.first()) {
                let next = next_free.map_or_else(|| "none is free".to_string(), |x| (x + first).to_string());
                return Err(CmdError::refused(format!(
                    "Channel {shown} of {who} is drawn on {}. Pick another channel, or Enter for the next free one ({next}).",
                    drawn_words(doc, *t0)
                )));
            }
            if taken.contains(&i) {
                return Err(CmdError::refused(format!("Channel {shown} of {who} is already being placed.")));
            }
            i
        }
        None => next_free.ok_or_else(|| {
            CmdError::refused(format!("{who}'s {n} channels are all drawn. Raise its channel count in Properties, or pick another card."))
        })?,
    };
    let mut used = crate::devices::placing::labels_of(doc, c);
    used.extend(extra.iter().cloned());
    let roles = roles_of(s);
    let pins = pins_for(doc, Some(c), ct, n, index, &roles, &mut used);
    Ok(Join { index, pins: roles.into_iter().zip(pins).collect() })
}

/// The pins `s` would get joining `c` (design 018's `join_labels` for [`Kind::Channel`]).
///
/// # Errors
/// As [`join`].
pub fn join_labels(doc: &Document, c: DeviceId, s: &Starter, extra: &BTreeSet<String>) -> Result<Vec<String>, CmdError> {
    join(doc, c, s, None, extra, &BTreeSet::new()).map(|j| j.pins.into_iter().map(|(_, p)| p).collect())
}

/// One planned cell: its terminals (role, terminal, pin, local point) and its representation.
#[derive(Debug, Clone)]
pub struct Cell {
    pub index: u32,
    pub connection_points: Vec<(ChannelRole, ConnectionPointId, String, Pt)>,
    pub representation: RepresentationId,
    pub item: ItemId,
}

/// Plan one channel symbol of `card` for channel `j` at `placement` (design 045 §4.4): its points
/// (label = pin, key = `.<i>.<role>`), the symbol and its representation with explicit connection
/// points.
#[allow(clippy::too_many_arguments)]
pub fn plan_cell(
    ids: &mut IdAllocator, page: PageId, style: Style, def_id: DefinitionId, s: &Starter, card: DeviceId, j: &Join, placement: Placement,
    ops: &mut Vec<Op>,
) -> Result<Cell, CmdError> {
    let def = s.definition()?;
    let mut connection_points = Vec::new();
    let mut cps = BTreeMap::new();
    for (role, pin) in &j.pins {
        let at = def
            .connection_points
            .iter()
            .find(|td| td.key.role() == Some(*role))
            .map(|td| td.at)
            .ok_or(CmdError::refused("The channel symbol has no point for that role."))?;
        let t: ConnectionPointId = ids.fresh();
        ops.push(Op::AddConnectionPoint {
            id: t,
            connection_point: ConnectionPoint {
                owner: card.into(),
                label: Some(pin.clone()),
                key: Some(key(j.index, *role).ok_or(CmdError::refused("That channel has no such point."))?),
                accepts: Default::default(),
            },
        });
        cps.insert(t, at);
        connection_points.push((*role, t, pin.clone(), at));
    }
    let (item, representation): (ItemId, RepresentationId) = (ids.fresh(), ids.fresh());
    ops.push(Op::AddItem { id: item, item: Item { page, style, kind: ItemKind::Symbol(SymbolInstance { definition: def_id, placement }) } });
    ops.push(Op::LinkRepresentation {
        id: representation,
        rep: Representation {
            subject: Subject::Device(card),
            page,
            form: Form::Symbol,
            role: ecad_model::Role::Part(s.part_role()),
            items: vec![item],
            connection_points: cps,
        },
    });
    Ok(Cell { index: j.index, connection_points, representation, item })
}

/// A new card device for channel symbols of type `t` (design 045 §4.4 rule 2): prefix `A`, the
/// body's name as its description, no rack.
pub fn new_card_ops(
    ledger: &ecad_model::Ledger, ids: &mut IdAllocator, high: &mut crate::devices::tags::TagHigh, s: &Starter, def: DefinitionId, t: CardType,
    tag: Option<ecad_model::naming::tag::Entry>, ops: &mut Vec<Op>,
) -> Result<(DeviceId, String), CmdError> {
    let doc = ledger.document();
    let tag = high.given(ledger, tag, &crate::devices::prefix_for(doc, s))?;
    let c: DeviceId = ids.fresh();
    let name = t.body().map(crate::symbols::catalogue::starter).map_or("PLC card", |b| b.name);
    ops.push(Op::CreateDevice {
        id: c,
        device: crate::devices::with_given(
            ecad_model::Device {
                description: crate::devices::description_of(name),
                definition: Some(def),
                device_type: crate::plc::types::new_type(doc, &s.device_type()),
                ..ecad_model::Device::default()
            },
            &tag,
        )?,
    });
    Ok((c, tag.shown()))
}

/// `channel 3` / `channels 0–7`, as shown.
pub fn channels_words(first: u32, idx: &[u32]) -> String {
    match idx {
        [] => "no channel".into(),
        [one] => format!("channel {}", one + first),
        [a, .., b] => format!("channels {}–{}", a + first, b + first),
    }
}

/// `pin 4` / `pins 1–8`.
pub fn pins_words(pins: &[String]) -> String {
    match pins {
        [] => "no pin".into(),
        [one] => format!("pin {one}"),
        [a, .., b] => format!("pins {a}–{b}"),
    }
}

/// A cell placed this close to the line of its card's cells on the page goes onto it, mm.
pub const SNAP_TO_LINE: f64 = 2.0 * crate::symbols::cells::CELL_TOP;

/// The world point of a cell's first point at `pl`.
pub fn first_point(s: &Starter, pl: &Placement) -> Option<Pt> {
    let def = s.definition().ok()?;
    def.connection_points.first().map(|td| pl.apply(td.at))
}

/// One cell the fence lands: its rotation (radians) and which of its points lands on which crossed
/// wire, the first being the one the cell is placed by.
#[derive(Debug, Clone, PartialEq)]
pub struct Landing {
    pub rotation: Radians,
    pub points: Vec<(ChannelRole, crate::cables::slash::Crossing)>,
}

/// The local place of point `role` in a channel symbol's definition terminals.
pub(crate) fn role_at(terms: &[ecad_model::ConnectionPointDef], role: ChannelRole) -> Option<Pt> {
    terms.iter().find(|td| td.key.role() == Some(role)).map(|td| td.at)
}

/// The role `word` names among channel symbol `s`'s points (case ignored).
///
/// # Errors
/// No such point, naming the cell's points.
pub(crate) fn role_named(s: &Starter, word: &str) -> Result<ChannelRole, CmdError> {
    let roles = roles_of(s);
    let w = word.trim();
    ChannelRole::parse(w).filter(|r| roles.contains(r)).ok_or_else(|| {
        let names: Vec<&str> = roles.iter().map(|r| r.word()).collect();
        CmdError::refused(format!("A {} cell has no point \"{}\". Its points: {}.", s.key, w.chars().take(16).collect::<String>(), names.join(", ")))
    })
}

/// The point a cell lands on a single wire by default (pack re-run, bug 10): an IO-Link port's
/// C/Q (its signal; L+ and L- are its supply), else its first point.
pub(crate) fn landing_role(s: &Starter) -> Option<ChannelRole> {
    let roles = roles_of(s);
    if cell_type(s) == Some(CardType::Iol) && roles.contains(&ChannelRole::Cq) {
        return Some(ChannelRole::Cq);
    }
    roles.first().copied()
}

/// Whether the wire crossed at `m` runs mostly down from the crossing (pack re-run, bug 8): its
/// route reaches further below than above. A cell on such a wire is turned 180°, its frame and
/// texts above its points and clear of the wire, as Make Card Page draws outputs.
fn runs_down(doc: &Document, m: &crate::cables::slash::Crossing) -> bool {
    let Some(v) = doc.routes.get(&m.route).map(|r| &r.points) else { return false };
    let up = v.iter().map(|p| p.y - m.at.y).fold(0.0, f64::max);
    let down = v.iter().map(|p| m.at.y - p.y).fold(0.0, f64::max);
    down > up + 1e-6
}

/// Points of one cell closer than this to its wires' spacing still match them, mm.
const POINT_MATCH: f64 = 0.5;

/// The cells a fence over `members` lands (in crossing order). A cell with more than one point
/// (AI, AO, RTD, IO-Link) lands each point on its own wire when the crossings come in groups of
/// its point count spaced as its points are (pack re-run, bug 6); otherwise, or when `role` names
/// one point, one cell per wire with that point (else [`landing_role`]) on it. Each cell is turned
/// 180° when its first wire runs down (`runs_down`).
pub fn landings(doc: &Document, s: &Starter, members: &[crate::cables::slash::Crossing], role: Option<ChannelRole>) -> Vec<Landing> {
    let roles = roles_of(s);
    let terms = s.definition().map(|d| d.connection_points.clone()).unwrap_or_default();
    let rotation = |m: &crate::cables::slash::Crossing| if runs_down(doc, m) { Radians(std::f64::consts::PI) } else { Radians(0.0) };
    let n = roles.len();
    if role.is_none() && n > 1 && members.len().is_multiple_of(n) {
        let groups: Option<Vec<Landing>> = members
            .chunks(n)
            .map(|chunk| {
                let mut chunk: Vec<crate::cables::slash::Crossing> = chunk.to_vec();
                chunk.sort_by(|a, b| a.at.x.total_cmp(&b.at.x));
                let first = *chunk.first()?;
                let rot = rotation(&first);
                // The points as they lie left to right at that rotation.
                let turn = Placement { origin: Pt::new(0.0, 0.0), rotation: rot, mirror: false, scale: 1.0 };
                let mut lie: Vec<(ChannelRole, Pt)> = roles.iter().filter_map(|r| Some((*r, turn.apply(role_at(&terms, *r)?)))).collect();
                lie.sort_by(|a, b| a.1.x.total_cmp(&b.1.x));
                let p0 = lie.first()?.1;
                let fits = lie.len() == chunk.len()
                    && lie
                        .iter()
                        .zip(&chunk)
                        .all(|((_, p), m)| ((m.at.x - first.at.x) - (p.x - p0.x)).abs() <= POINT_MATCH && (m.at.y - first.at.y).abs() <= POINT_MATCH);
                fits.then(|| Landing { rotation: rot, points: lie.iter().map(|(r, _)| *r).zip(chunk.iter().copied()).collect() })
            })
            .collect();
        if let Some(g) = groups {
            return g;
        }
    }
    let Some(role) = role.or_else(|| landing_role(s)) else { return Vec::new() };
    members.iter().map(|m| Landing { rotation: rotation(m), points: vec![(role, *m)] }).collect()
}

/// The ops landing a channel cell's point `t` on its route at `on` (design 045 §4.5, design 072
/// §4.3). The route is split at the point as Place Terminals splits it (028 §3.1), but a cell has
/// one point, not a way through: a part whose far end lands on nothing (a free end, and no route
/// landing on it) carries nothing past the cell, so it is not kept. The route ends at the point
/// instead of running on through the cell box, where its wire's number label covered the channel
/// number. A part that reaches something is kept, as before.
///
/// # Errors
/// As [`crate::connectors::plan_land`].
pub(crate) fn land_cell(
    doc: &Document, ids: &mut IdAllocator, on: crate::wires::placing::OnRoute, t: ConnectionPointId,
) -> Result<Vec<Op>, CmdError> {
    let route = doc.routes.get(&on.route).ok_or(CmdError::NotFound(ecad_model::Noun::Wire))?;
    let (a_free, b_free) = (route.ends[0] == ecad_model::RouteEnd::Free, route.ends[1] == ecad_model::RouteEnd::Free);
    if a_free == b_free || doc.tees_of(on.route).next().is_some() {
        return Ok(crate::connectors::plan_land(doc, ids, on, t)?.ops);
    }
    // Refusals as a split's (a route end, a strip terminal there).
    crate::connectors::plan_land(doc, ids, on, t)?;
    let v = &route.points;
    let seg = on.segment;
    let points: Vec<Pt> = if a_free {
        // The far side (end B's) is kept, from the point.
        std::iter::once(on.at).chain(v.get(seg + 1..).unwrap_or_default().iter().copied().filter(|p| !p.near(on.at, 1e-9))).collect()
    } else {
        v.get(..=seg).unwrap_or_default().iter().copied().filter(|p| !p.near(on.at, 1e-9)).chain(std::iter::once(on.at)).collect()
    };
    let mut edits = crate::wires::routes::RouteEdits::new(doc);
    if let Some(x) = edits.get(on.route) {
        x.points = points;
        let end = if a_free { 0 } else { 1 };
        if let Some(e) = x.ends.get_mut(end) {
            *e = ecad_model::RouteEnd::Point(t);
        }
    }
    Ok(edits.ops())
}

/// A channel's own field as `setprop` names it: `plc.ch.<i>.<what>`.
pub(crate) fn ch_key(i: u32, what: &str) -> String {
    format!("{CH_PREFIX}{i}.{what}")
}

/// The index and field of a channel key `plc.ch.<i>.text`, `.address`, `.tag` or `.safe`.
pub fn parse_ch_key(k: &str) -> Option<(u32, &'static str)> {
    let rest = k.strip_prefix(CH_PREFIX)?;
    let (i, what) = rest.split_once('.')?;
    if i.len() > 4 || !ecad_model::naming::words::is_digits(i) {
        return None;
    }
    let i: u32 = i.parse().ok().filter(|n| *n <= MAX_INDEX)?;
    // Brief 056 (FP1): a channel's PLC tag and safe state.
    let what = ["text", "address", crate::plc::signals::TAG, crate::plc::signals::SAFE].into_iter().find(|w| *w == what)?;
    Some((i, what))
}

/// The value check of a channel's text or address (design 045 §4.6).
///
/// # Errors
/// Too long, or a control character.
pub(crate) fn ch_value(what: &str, value: Option<&str>) -> Result<Option<String>, CmdError> {
    match what {
        crate::plc::signals::TAG => return crate::plc::signals::tag_value(value),
        crate::plc::signals::SAFE => return crate::plc::signals::safe_value(None, value),
        _ => {}
    }
    let Some(v) = value.map(str::trim).filter(|v| !v.is_empty()) else { return Ok(None) };
    let max = if what == "address" { MAX_ADDRESS } else { MAX_TEXT };
    if v.chars().count() > max || v.chars().any(char::is_control) {
        let noun = if what == "address" { "An address" } else { "A channel description" };
        return Err(CmdError::refused(format!("{noun} is 1 to {max} characters, with no control characters.")));
    }
    Ok(Some(v.to_string()))
}

/// Field `what` (`text`, `address`, `tag` or `safe`) of channel `i` of module `c`, as text.
pub fn field(doc: &Document, c: DeviceId, i: u32, what: &str) -> Option<String> {
    let ch = crate::plc::edit::module_of(doc, c)?.channel(i)?;
    match what {
        "text" => ch.text.as_ref().map(ToString::to_string),
        "address" => ch.address.as_ref().map(ToString::to_string),
        crate::plc::signals::TAG => ch.tag.as_ref().map(ToString::to_string),
        crate::plc::signals::SAFE => ch.safe.as_ref().map(ToString::to_string),
        _ => None,
    }
}

/// Set field `what` (`text`, `address`, `tag` or `safe`, as [`parse_ch_key`] gives it) of channel
/// `i` of module `c` to `value` (already checked with [`ch_value`]; `None` clears it), adding the
/// change to `ops`.
///
/// # Errors
/// `c` is not a module with channels; a value with a hidden character.
pub fn set_field(doc: &Document, ops: &mut Vec<Op>, c: DeviceId, i: u32, what: &str, value: Option<&str>) -> Result<(), CmdError> {
    use ecad_model::objects::plc::SafeState;
    let refused = |e: ecad_model::naming::text::TextError| CmdError::refused(format!("{e}."));
    let mut field: Box<dyn FnMut(&mut ecad_model::objects::plc::Channel)> = match what {
        "text" => {
            let v = value.map(ecad_model::objects::plc::ChannelText::new).transpose().map_err(refused)?;
            Box::new(move |ch| ch.text = v.clone())
        }
        "address" => {
            let v = value.map(ecad_model::objects::plc::ChannelAddress::new).transpose().map_err(refused)?;
            Box::new(move |ch| ch.address = v.clone())
        }
        crate::plc::signals::TAG => {
            let v = value.map(ecad_model::objects::plc::PlcTag::new).transpose().map_err(refused)?;
            Box::new(move |ch| ch.tag = v.clone())
        }
        crate::plc::signals::SAFE => {
            let v = match value {
                None => None,
                Some(t) => Some(SafeState::parse(t).ok_or_else(|| CmdError::refused(format!("{t} is not a safe state.")))?),
            };
            Box::new(move |ch| ch.safe = v.clone())
        }
        _ => return Err(CmdError::input("a channel has text, address, tag and safe")),
    };
    let mut fits = true;
    let module = crate::plc::edit::module(doc, ops, c, |m| fits = m.edit_channel(i, &mut *field));
    if module && fits { Ok(()) } else { Err(CmdError::refused(format!("{} has no channels.", crate::devices::tags::tag_words(doc, c)))) }
}

/// `-A02 channel 3`.
pub(crate) fn channel_words(doc: &Document, c: DeviceId, i: u32) -> String {
    format!("{} channel {}", crate::devices::tags::tag_words(doc, c), i + first_number(doc, c))
}

/// The fence's refusal when two crossed wires are closer than a cell is wide (brief 051, design
/// 051 §2.2): `Wires 24V-12 and 24V-13 are 10 mm apart; a DI cell is 20 mm wide. Space the wires
/// 20 mm apart, or place compact di symbols.` Wires exactly one width apart give abutting cells.
pub(crate) fn spacing_refusal(doc: &Document, s: &Starter, t: CardType, members: &[crate::cables::slash::Crossing]) -> Option<String> {
    let (w, _) = crate::symbols::cells::cell_shape(s.key)?;
    let mut at: Vec<(f64, f64, ecad_model::CorePosition)> = members.iter().map(|m| (m.at.x, m.at.y, m.position)).collect();
    at.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    for pair in at.windows(2) {
        let (Some(a), Some(b)) = (pair.first(), pair.get(1)) else { continue };
        let d = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
        if d.is_finite() && d + 1e-6 < w {
            let compact = match t {
                CardType::Di => ", or place compact di symbols",
                CardType::Dq => ", or place compact do symbols",
                _ => "",
            };
            return Some(format!(
                "Wires {} and {} are {} mm apart; a {} cell is {} mm wide. Space the wires {} mm apart{compact}.",
                wire_words(doc, a.2),
                wire_words(doc, b.2),
                fmt_mm(d),
                t.code(),
                fmt_mm(w),
                fmt_mm(w)
            ));
        }
    }
    None
}

/// `10`, `12.5`.
fn fmt_mm(v: f64) -> String {
    let r = (v * 10.0).round() / 10.0;
    if (r - r.round()).abs() < 1e-9 { format!("{}", r.round() as i64) } else { format!("{r:.1}") }
}

/// A wire in a sentence that already says "Wires": its number, without a leading `wire `.
fn wire_words(doc: &Document, c: ecad_model::CorePosition) -> String {
    let n = crate::cables::slash::position_name(doc, c);
    n.strip_prefix("wire ").map(str::to_string).unwrap_or(n)
}
