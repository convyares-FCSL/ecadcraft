//! Channels (design 045 §4, §6): channel points, pins, joining a card, Place Channels (the fence
//! and the rest) and `plcchannel`.
//!
//! # Semantics
//! - A channel point is a `Terminal` of the card: its **label is the pin** (what is printed and
//!   typed after the tag, `-A02:4`), its **key is `.<i>.<role>`** (the 0-based channel index and
//!   the role, FP2). The channel number shown is `i + plc.channel.first`, so changing the first
//!   number never rewrites a terminal.
//! - Old `di`/`do` terminals keyed `I<a>.<b>`/`Q<a>.<b>` read as channel `a × 8 + b`, role
//!   `in`/`out` (FP2: not compatibility work, it keeps today's symbols working).
//! - Pins come from the card's pin layout (`plc.pins`, §4.3) once, when drawn, and are never
//!   recomputed; `plcchannel {pins}` relabels them.
//! - Spare and reserved are derived, never stored (§6).
//!
//! Every write is one transaction, committed through [`crate::project::Project::apply_edit`].

use std::collections::{BTreeMap, BTreeSet};

use cadcraft_engine::Session;
use ecad_model::{
    ComponentId, DefinitionId, Document, End, Form, IdAllocator, Item, ItemId, ItemKind, ObjectRef, Op, PageId, Placement, Pt, Representation,
    RepresentationId, Style, Subject, SymbolInstance, Terminal, TerminalId,
};
use serde_json::{Value, json};

use super::card_type_of;
use super::rack::{CardType, MAX_CHANNELS, channel_count};
use crate::commands::{err, opt_typed, plain_style, point, with_project};
use crate::defaults::{self, Scope};
use crate::symbols::{Kind, Starter};

/// The property prefix of a channel's own data (`plc.ch.<i>.text`, `plc.ch.<i>.address`; FP3).
pub const CH_PREFIX: &str = "plc.ch.";
/// Most characters of a channel description (design 045 §4.6).
pub const MAX_TEXT: usize = 80;
/// Most characters of a typed address.
pub const MAX_ADDRESS: usize = 32;
/// Most characters of a pin.
pub const MAX_PIN: usize = 16;
/// Most wires one Place Channels line lands on (design 045 §4.5).
pub const MAX_FENCE: usize = 256;
/// The highest channel index read from a key (larger keys are not channel points).
pub const MAX_INDEX: u32 = 9_999;

/// Every role a channel point may have (design 045 §4.1).
pub const ROLES: [&str; 11] = ["in", "out", "+", "-", "I+", "S+", "S-", "I-", "L+", "C/Q", "L-"];

/// A channel point's key: `.<i>.<role>`.
pub fn key(i: u32, role: &str) -> String {
    format!(".{i}.{role}")
}

/// The channel index and role of a key `.<i>.<role>`; `None` for anything else (`.3`, `.-1.in`,
/// `.3.nope`, `.99999.in`).
pub fn parse_key(k: &str) -> Option<(u32, &'static str)> {
    let rest = k.strip_prefix('.')?;
    let (i, role) = rest.split_once('.')?;
    if i.is_empty() || i.len() > 4 || !i.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let i: u32 = i.parse().ok().filter(|n| *n <= MAX_INDEX)?;
    let role = ROLES.into_iter().find(|r| *r == role)?;
    Some((i, role))
}

/// An old `di`/`do` key (`I<a>.<b>`, `Q<a>.<b>`): channel `a × 8 + b`, role `in`/`out`.
pub fn parse_legacy(k: &str) -> Option<(u32, &'static str)> {
    let (role, rest) = match k.as_bytes().first()? {
        b'I' => ("in", k.get(1..)?),
        b'Q' => ("out", k.get(1..)?),
        _ => return None,
    };
    let (a, b) = rest.split_once('.')?;
    if a.is_empty() || a.len() > 4 || b.len() != 1 || !a.bytes().all(|x| x.is_ascii_digit()) {
        return None;
    }
    let (a, b): (u32, u32) = (a.parse().ok()?, b.parse().ok()?);
    let i = a.checked_mul(8)?.checked_add(b).filter(|i| b < 8 && *i <= MAX_INDEX)?;
    Some((i, role))
}

/// The channel and role of terminal `t` of a card: from its key, or an old `di`/`do` key.
pub fn point_of(doc: &Document, t: TerminalId) -> Option<(u32, &'static str)> {
    let k = doc.terminals.get(&t)?.key.as_deref()?;
    parse_key(k).or_else(|| parse_legacy(k))
}

/// The channel points of card `c`: index → (role, terminal, pin), roles in their cell order.
pub fn points(doc: &Document, c: ComponentId) -> BTreeMap<u32, Vec<(&'static str, TerminalId, String)>> {
    let mut out: BTreeMap<u32, Vec<(&'static str, TerminalId, String)>> = BTreeMap::new();
    for (t, term) in doc.terminals_of(c) {
        if let Some((i, role)) = point_of(doc, t) {
            out.entry(i).or_default().push((role, t, term.label.clone().unwrap_or_default()));
        }
    }
    for v in out.values_mut() {
        v.sort_by_key(|(r, t, _)| (ROLES.iter().position(|x| x == r), *t));
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

/// The roles a channel symbol has, in its definition order: the cell's, or one for `di`/`do`.
pub fn roles_of(s: &Starter) -> Vec<&'static str> {
    let Some(t) = cell_type(s) else { return Vec::new() };
    let Ok(def) = s.definition() else { return Vec::new() };
    def.terminals
        .iter()
        .filter_map(|td| td.key.strip_prefix('.').and_then(|r| ROLES.into_iter().find(|x| *x == r)).or_else(|| parse_legacy(&td.key).map(|x| x.1)))
        .filter(|r| t.roles().contains(r))
        .collect()
}

/// `chdi`, `di` for a DI card: the symbols that draw its channels.
fn cells_words(t: CardType) -> String {
    let mut v = vec![t.cell().to_string()];
    match t {
        CardType::Di => v.push("di".into()),
        CardType::Dq => v.push("do".into()),
        _ => {}
    }
    v.join(", ")
}

/// The first channel number shown on card `c` (`plc.channel.first`, 0 or 1).
pub fn first_number(doc: &Document, c: ComponentId) -> u32 {
    let at = Scope { component: Some(c), ..Scope::project() };
    u32::from(defaults::resolve(doc, &defaults::plc::CHANNEL_FIRST, at).word() == "1")
}

/// Where channel `i` of card `c` is drawn, in words (`105-C-2`, or `page 3`).
fn drawn_words(doc: &Document, t: TerminalId) -> String {
    let Some((_, r)) = doc.representations_showing(t).next() else { return "no page".into() };
    let at = crate::devices::rep_definition(doc, r).map(|(_, pl)| pl.origin);
    match at.and_then(|a| ecad_model::wiring::place(doc, r.page, a)) {
        Some(p) => p.text(doc.settings.reference_style),
        None => crate::references::page_number_words(doc, r.page),
    }
}

/// The pins a new channel `i` of card `c` gets for `roles` (design 045 §4.3): from the pin layout,
/// or the next free whole number when the computed one is taken (`used`, which gains them).
pub fn pins_for(
    doc: &Document, c: Option<ComponentId>, ty: CardType, n: u32, i: u32, roles: &[&'static str], used: &mut BTreeSet<String>,
) -> Vec<String> {
    let at = Scope { component: c, ..Scope::project() };
    let by_channel = defaults::resolve(doc, &defaults::plc::PINS, at).word() == "channel";
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
    pub pins: Vec<(&'static str, String)>,
}

/// The channel and pins symbol `s` gets joining card `c` (design 045 §4.4): the channel numbered
/// `named` (as shown), else the lowest not drawn yet. `extra` are labels already given in the same
/// transaction; `taken` indexes too.
///
/// # Errors
/// The refusals of §4.4, each with the way out.
pub fn join(
    doc: &Document, c: ComponentId, s: &Starter, named: Option<u32>, extra: &BTreeSet<String>, taken: &BTreeSet<u32>,
) -> Result<Join, String> {
    let who = crate::devices::tag_words(doc, c);
    let t = cell_type(s).ok_or("That symbol is not a PLC channel.")?;
    let ct = match card_type_of(doc, c) {
        Some(ct) => ct,
        None => return Err(format!("{who} is not a PLC card; pick a card, or place the channel on its own for a new card.")),
    };
    if !ct.has_channels() {
        return Err(format!("{who} is a {}; channels go on its cards. Pick a card, or place a card body first.", ct.noun()));
    }
    if ct != t {
        return Err(format!("{who} is a {}; pick a {} channel ({}), or another card.", ct.noun(), ct.code(), cells_words(ct)));
    }
    let n = channel_count(doc, c, ct);
    let pts = points(doc, c);
    let drawn = |i: u32| pts.contains_key(&i) || taken.contains(&i);
    let first = first_number(doc, c);
    let next_free = (0..n.min(MAX_CHANNELS)).find(|i| !drawn(*i));
    let index = match named {
        Some(shown) => {
            let i = shown.checked_sub(first).ok_or_else(|| format!("{who}'s channels start at {first}."))?;
            if i > MAX_INDEX {
                return Err(format!("{who} has {n} channels."));
            }
            if let Some((_, t0, _)) = pts.get(&i).and_then(|v| v.first()) {
                let next = next_free.map_or_else(|| "none is free".to_string(), |x| (x + first).to_string());
                return Err(format!(
                    "Channel {shown} of {who} is drawn on {}. Pick another channel, or Enter for the next free one ({next}).",
                    drawn_words(doc, *t0)
                ));
            }
            if taken.contains(&i) {
                return Err(format!("Channel {shown} of {who} is already being placed."));
            }
            i
        }
        None => {
            next_free.ok_or_else(|| format!("{who}'s {n} channels are all drawn. Raise its channel count in Properties, or pick another card."))?
        }
    };
    let mut used = crate::devices::labels_of(doc, c);
    used.extend(extra.iter().cloned());
    let roles = roles_of(s);
    let pins = pins_for(doc, Some(c), ct, n, index, &roles, &mut used);
    Ok(Join { index, pins: roles.into_iter().zip(pins).collect() })
}

/// The pins `s` would get joining `c` (design 018's `join_labels` for [`Kind::Channel`]).
///
/// # Errors
/// As [`join`].
pub fn join_labels(doc: &Document, c: ComponentId, s: &Starter, extra: &BTreeSet<String>) -> Result<Vec<String>, String> {
    join(doc, c, s, None, extra, &BTreeSet::new()).map(|j| j.pins.into_iter().map(|(_, p)| p).collect())
}

/// One planned cell: its terminals (role, terminal, pin, local point) and its representation.
#[derive(Debug, Clone)]
pub struct Cell {
    pub index: u32,
    pub terminals: Vec<(&'static str, TerminalId, String, Pt)>,
    pub representation: RepresentationId,
    pub item: ItemId,
}

/// Plan one channel symbol of `card` for channel `j` at `placement` (design 045 §4.4): its points
/// (label = pin, key = `.<i>.<role>`), the symbol and its representation with explicit connection
/// points.
#[allow(clippy::too_many_arguments)]
pub fn plan_cell(
    ids: &mut IdAllocator, page: PageId, style: Style, def_id: DefinitionId, s: &Starter, card: ComponentId, j: &Join, placement: Placement,
    ops: &mut Vec<Op>,
) -> Result<Cell, String> {
    let def = s.definition()?;
    let mut terminals = Vec::new();
    let mut cps = BTreeMap::new();
    for (role, pin) in &j.pins {
        let at = def
            .terminals
            .iter()
            .find(|td| td.key.strip_prefix('.') == Some(*role) || parse_legacy(&td.key).is_some_and(|x| x.1 == *role))
            .map(|td| td.at)
            .ok_or("The channel symbol has no point for that role.")?;
        let t: TerminalId = ids.fresh();
        ops.push(Op::AddTerminal { id: t, terminal: Terminal { component: card, label: Some(pin.clone()), key: Some(key(j.index, role)) } });
        cps.insert(t, at);
        terminals.push((*role, t, pin.clone(), at));
    }
    let (item, representation): (ItemId, RepresentationId) = (ids.fresh(), ids.fresh());
    ops.push(Op::AddItem { id: item, item: Item { page, style, kind: ItemKind::Symbol(SymbolInstance { definition: def_id, placement }) } });
    ops.push(Op::LinkRepresentation {
        id: representation,
        rep: Representation {
            subject: Subject::Component(card),
            page,
            form: Form::Symbol,
            role: s.key.into(),
            items: vec![item],
            connection_points: cps,
        },
    });
    Ok(Cell { index: j.index, terminals, representation, item })
}

/// A new card device for channel symbols of type `t` (design 045 §4.4 rule 2): prefix `A`, the
/// body's name as its description, no rack.
pub fn new_card_ops(
    ledger: &ecad_model::Ledger, ids: &mut IdAllocator, high: &mut crate::devices::TagHigh, s: &Starter, def: DefinitionId, t: CardType,
    tag: Option<String>, ops: &mut Vec<Op>,
) -> Result<(ComponentId, String), String> {
    let doc = ledger.document();
    let tag = match tag {
        Some(t) => t,
        None => high.next_auto(ledger, &crate::devices::prefix_for(doc, s))?,
    };
    let c: ComponentId = ids.fresh();
    let mut properties = ecad_model::Properties::new();
    properties.insert(ecad_model::tag::TAG_KEY.into(), tag.clone());
    let name = crate::symbols::by_key(t.body()).map_or("PLC card", |b| b.name);
    properties.insert("description".into(), name.into());
    ops.push(Op::CreateComponent { id: c, component: ecad_model::Component { properties, definition: Some(def) } });
    Ok((c, tag))
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
fn pins_words(pins: &[String]) -> String {
    match pins {
        [] => "no pin".into(),
        [one] => format!("pin {one}"),
        [a, .., b] => format!("pins {a}–{b}"),
    }
}

/// A cell placed this close to the line of its card's cells on the page goes onto it, mm.
const SNAP_TO_LINE: f64 = 2.0 * crate::symbols::CELL_TOP;

/// The world point of a cell's first point at `pl`.
fn first_point(s: &Starter, pl: &Placement) -> Option<Pt> {
    let def = s.definition().ok()?;
    def.terminals.first().map(|td| pl.apply(td.at))
}

/// `place {symbol: <channel symbol>, at, rotation?, mirror?, device?, tag?, channel?}` (design 045
/// §4.4): joins card `device` (or the card tagged `tag`, or a new card) as its next free channel or
/// the channel numbered `channel`; on a wire its first point lands there. One transaction.
pub(crate) fn run_place_channel(
    s: &mut Session, params: &Value, starter: &'static Starter, at: Pt, degrees: f64, mirror: bool,
) -> cadcraft_engine::Result<Value> {
    let t = cell_type(starter).ok_or_else(|| err("That symbol is not a PLC channel."))?;
    let device: Option<ComponentId> = crate::commands::opt_id(params, "device").map_err(err)?;
    let tag = opt_typed(params, "tag", "a tag", Value::as_str).map_err(err)?.map(str::to_string);
    let named = channel_param(params).map_err(err)?;
    if device.is_some() && tag.is_some() {
        return Err(err("`tag` is for a new card; `device` joins an existing one"));
    }
    let v = with_project(s, |p, pg, page| {
        let style = plain_style(p);
        let doc = p.ledger.document();
        let card = match (device, &tag) {
            (Some(c), _) => {
                if !doc.components.contains_key(&c) {
                    return Err("No such device.".into());
                }
                Some(c)
            }
            (None, Some(t)) => {
                let t = ecad_model::tag::normalise(t, crate::defaults::print::tag_style(doc))?;
                crate::devices::holder_of(doc, &t, None)
            }
            (None, None) => None,
        };
        let new_tag = match (card, &tag) {
            (None, Some(t)) => Some(ecad_model::tag::normalise(t, crate::defaults::print::tag_style(doc))?),
            _ => None,
        };
        let mut ops = Vec::new();
        let def = crate::terminal_cmds::definition_for(doc, &mut p.ids, starter, &mut ops)?;
        let (card, card_tag, new) = match card {
            Some(c) => (c, crate::devices::tag_words(doc, c), false),
            None => {
                let (c, t2) = new_card_ops(&p.ledger, &mut p.ids, &mut p.tag_high, starter, def, t, new_tag, &mut ops)?;
                (c, t2, true)
            }
        };
        let doc = p.ledger.document();
        let j = if new {
            let n = channel_count(doc, card, t);
            let roles = roles_of(starter);
            let index = match named {
                Some(x) => x.checked_sub(first_number(doc, card)).ok_or("Channels start at 0.")?,
                None => 0,
            };
            let mut used = BTreeSet::new();
            let pins = pins_for(doc, None, t, n, index, &roles, &mut used);
            Join { index, pins: roles.into_iter().zip(pins).collect() }
        } else {
            join(doc, card, starter, named, &BTreeSet::new(), &BTreeSet::new())?
        };
        let mut pl = Placement { origin: at, rotation: degrees.to_radians(), mirror, scale: 1.0 };
        // Pack re-run, bug 5: a cell placed near the line of its card's cells on this page goes onto
        // that line, so the card keeps one frame.
        if !new && let Some(q) = first_point(starter, &pl) {
            let dir = if (degrees.rem_euclid(360.0) - 180.0).abs() < 0.5 { 1.0 } else { -1.0 };
            let line = super::look::runs_of_card(doc, card, page)
                .iter()
                .filter(|r| r.dir == dir && (r.top - q.y).abs() <= SNAP_TO_LINE)
                .map(|r| r.top)
                .min_by(|a, b| (a - q.y).abs().total_cmp(&(b - q.y).abs()));
            if let Some(y) = line.filter(|y| y.is_finite()) {
                pl.origin.y += y - q.y;
            }
        }
        // On a wire: its first point lands there (design 045 §4.4), the symbol shifted onto it.
        let landing = first_point(starter, &pl).and_then(|q| crate::terminal_cmds::landing_point(doc, page, q).map(|on| (q, on)));
        if let Some((q, on)) = &landing {
            pl.origin = Pt::new(pl.origin.x + on.at.x - q.x, pl.origin.y + on.at.y - q.y);
        }
        let cell = plan_cell(&mut p.ids, page, style, def, starter, card, &j, pl, &mut ops)?;
        let mut wire = None;
        if let (Some((_, on)), Some((_, t0, _, _))) = (landing, cell.terminals.first()) {
            let land = crate::terminal_cmds::plan_land(doc, &mut p.ids, page, on, *t0)?;
            wire = Some(land.conductor);
            ops.extend(land.ops);
        }
        p.apply_edit(pg, "place", "Place channel", ops)?;
        p.last_placed = Some((starter.key.to_string(), card));
        let doc = p.ledger.document();
        let first = first_number(doc, card);
        let pins: Vec<String> = cell.terminals.iter().map(|(_, _, pin, _)| pin.clone()).collect();
        let on = wire.map(|w| format!(" on wire {}", crate::slash::wire_name(doc, w))).unwrap_or_default();
        let message = format!("Placed {card_tag} channel {} ({}){on}.", j.index + first, pins_words(&pins));
        Ok(json!({
            "component": card.hex(),
            "tag": card_tag,
            "joined": !new,
            "channel": j.index + first,
            "index": j.index,
            "representation": cell.representation.hex(),
            "item": cell.item.hex(),
            "symbol": starter.key,
            "terminals": cell.terminals.iter().map(|(r, t, pin, _)| json!({"role": r, "label": pin, "key": key(j.index, r), "terminal": t.hex()})).collect::<Vec<_>>(),
            "message": message,
        }))
    })?;
    if let Some(m) = v.get("message").and_then(Value::as_str) {
        s.echo(m.to_string());
    }
    Ok(v)
}

/// The `channel` parameter: a number as shown.
fn channel_param(params: &Value) -> Result<Option<u32>, String> {
    match params.get("channel") {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
            .filter(|n| *n <= u64::from(MAX_INDEX))
            .and_then(|n| u32::try_from(n).ok())
            .map(Some)
            .ok_or_else(|| format!("A channel is a whole number from 0 to {MAX_INDEX}.")),
    }
}

/// The card a `card` parameter names (an ID or a tag), which must be a card with channels.
fn channel_card(doc: &Document, v: Option<&Value>) -> Result<(ComponentId, CardType), String> {
    let c = super::rack::card_param(doc, v)?;
    let t = card_type_of(doc, c).ok_or("Not a PLC card.")?;
    if !t.has_channels() {
        return Err(format!("{} is a {}; it has no channels.", crate::devices::tag_words(doc, c), t.noun()));
    }
    Ok((c, t))
}

/// `plcchannels {card?, type?, first?, from, to}` (the fence) or `{card, rest: true, at?}` (the rest;
/// design 045 §4.5). One transaction.
pub(crate) fn run_plcchannels(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let rest = opt_typed(params, "rest", "true or false", Value::as_bool).map_err(err)?.unwrap_or(false);
    let first_named = channel_param(&json!({"channel": params.get("first").cloned().unwrap_or(Value::Null)})).map_err(err)?;
    let ty_word = opt_typed(params, "type", "a card type: DI, DQ, AI, AO, RTD or IOL", Value::as_str).map_err(err)?.map(str::to_string);
    let point_word = opt_typed(params, "point", "a point's role, e.g. C/Q", Value::as_str).map_err(err)?.map(str::to_string);
    let bad = |q: Pt| !(q.x.abs() <= crate::interact::MAX_COORD && q.y.abs() <= crate::interact::MAX_COORD);
    let v = with_project(s, |p, pg, page| {
        let style = plain_style(p);
        let doc = p.ledger.document();
        let card = match params.get("card") {
            None | Some(Value::Null) => None,
            v => Some(channel_card(doc, v)?),
        };
        if rest {
            let (c, t) = card.ok_or("`card` is required with `rest`.")?;
            let at = match params.get("at") {
                None | Some(Value::Null) => None,
                Some(v) => Some(point(v).filter(|q| !bad(*q)).ok_or("`at` must be a point [x, y] on the sheet")?),
            };
            return draw_rest(p, pg, page, style, c, t, at);
        }
        let from = params.get("from").and_then(point).ok_or("`from` [x,y] is required")?;
        let to = params.get("to").and_then(point).ok_or("`to` [x,y] is required")?;
        if bad(from) || bad(to) {
            return Err(crate::interact::OUT_OF_RANGE.into());
        }
        if from.near(to, p.tolerance) {
            return Err("`from` and `to` must differ".into());
        }
        let t = match (card, &ty_word) {
            (Some((_, t)), _) => t,
            (None, Some(w)) => CardType::parse(w)
                .filter(|t| t.has_channels())
                .ok_or_else(|| format!("No card type \"{}\". Types: DI, DQ, AI, AO, RTD, IOL.", w.chars().take(16).collect::<String>()))?,
            (None, None) => CardType::Di,
        };
        let found = crate::slash::crossings(doc, page, from, to, p.tolerance);
        if found.members.len() > MAX_FENCE {
            return Err(format!("That line crosses {} wires; at most {MAX_FENCE} at once. Draw a shorter line.", found.members.len()));
        }
        if found.members.is_empty() {
            return Err("The line crosses no wire. Draw it across the wires to land.".into());
        }
        let starter = crate::symbols::by_key(t.cell()).ok_or("No channel symbol for that type.")?;
        let point_role = match &point_word {
            Some(w) => Some(role_named(starter, w)?),
            None => None,
        };
        let landings = landings(doc, page, starter, &found.members, point_role);
        // Brief 051 (design 051 §2.2, Q1): cells are one width apart at least, or they overlap.
        let anchors: Vec<crate::slash::Crossing> = landings.iter().filter_map(|l| l.points.first().map(|x| x.1)).collect();
        if let Some(e) = spacing_refusal(doc, starter, t, &anchors) {
            return Err(e);
        }
        let mut ops = Vec::new();
        let def = crate::terminal_cmds::definition_for(doc, &mut p.ids, starter, &mut ops)?;
        let (card, tag, new) = match card {
            Some((c, _)) => (c, crate::devices::tag_words(doc, c), false),
            None => {
                let (c, tag) = new_card_ops(&p.ledger, &mut p.ids, &mut p.tag_high, starter, def, t, None, &mut ops)?;
                (c, tag, true)
            }
        };
        let doc = p.ledger.document();
        let n = channel_count(doc, card, t);
        let first = first_number(doc, card);
        let roles = roles_of(starter);
        let pts = if new { BTreeMap::new() } else { points(doc, card) };
        let mut used: BTreeSet<String> = if new { BTreeSet::new() } else { crate::devices::labels_of(doc, card) };
        let mut next = match first_named {
            Some(x) => x.checked_sub(first).ok_or_else(|| format!("{tag}'s channels start at {first}."))?,
            None => (0..MAX_INDEX).find(|i| !pts.contains_key(i)).unwrap_or(0),
        };
        let mut placed: Vec<u32> = Vec::new();
        let mut all_pins: Vec<String> = Vec::new();
        let def_terms = &starter.definition()?.terminals;
        for l in &landings {
            while pts.contains_key(&next) && next < MAX_INDEX {
                next += 1;
            }
            if next >= n.max(1) && !new && next >= MAX_CHANNELS {
                return Err(format!("{tag} has no channel left to draw."));
            }
            let pins = pins_for(doc, (!new).then_some(card), t, n, next, &roles, &mut used);
            let j = Join { index: next, pins: roles.iter().copied().zip(pins.clone()).collect() };
            // The cell's first landing point sits on its crossing, the cell turned as its wire runs.
            let (role0, m0) = l.points.first().copied().ok_or("The channel symbol has no point.")?;
            let local = role_at(def_terms, role0).ok_or("The channel symbol has no point for that role.")?;
            let turn = Placement { origin: Pt::new(0.0, 0.0), rotation: l.rotation, mirror: false, scale: 1.0 }.apply(local);
            let pl = Placement { origin: Pt::new(m0.at.x - turn.x, m0.at.y - turn.y), rotation: l.rotation, mirror: false, scale: 1.0 };
            let cell = plan_cell(&mut p.ids, page, style.clone(), def, starter, card, &j, pl, &mut ops)?;
            for (role, m) in &l.points {
                let at = role_at(def_terms, role).map(|q| pl.apply(q)).ok_or("The channel symbol has no point for that role.")?;
                let on = crate::terminal_cmds::on_wire_of(doc, page, m.conductor, at).ok_or("A crossed wire has no route here.")?;
                let tid = cell.terminals.iter().find(|x| x.0 == *role).map(|x| x.1).ok_or("The channel symbol has no point.")?;
                let land =
                    land_cell(doc, &mut p.ids, page, on, tid).map_err(|e| format!("{e} (wire {})", crate::slash::wire_name(doc, m.conductor)))?;
                ops.extend(land);
            }
            placed.push(next);
            all_pins.extend(pins);
            next += 1;
        }
        p.apply_edit(pg, "plcchannels", "Place channels", ops)?;
        p.last_placed = Some((starter.key.to_string(), card));
        let k = placed.len();
        let wires = if k == 1 { "1 wire".to_string() } else { format!("{k} wires") };
        let mut first_pins: Vec<String> = all_pins.clone();
        first_pins.sort_by_key(|x| x.parse::<u64>().unwrap_or(u64::MAX));
        let message = format!("Placed {tag} {} on {wires} ({}).", channels_words(first, &placed), pins_words(&first_pins));
        let doc = p.ledger.document();
        let over = placed.iter().filter(|i| **i >= channel_count(doc, card, t)).count();
        let message = if over > 0 {
            format!("{message} {over} beyond its {} channels: raise its count in Properties.", channel_count(doc, card, t))
        } else {
            message
        };
        Ok(json!({"card": card.hex(), "tag": tag, "channels": placed.iter().map(|i| i + first).collect::<Vec<_>>(), "message": message}))
    })?;
    if let Some(m) = v.get("message").and_then(Value::as_str) {
        s.echo(m.to_string());
    }
    Ok(v)
}

/// One cell the fence lands: its rotation (radians) and which of its points lands on which crossed
/// wire, the first being the one the cell is placed by.
#[derive(Debug, Clone, PartialEq)]
pub struct Landing {
    pub rotation: f64,
    pub points: Vec<(&'static str, crate::slash::Crossing)>,
}

/// The local place of point `role` in a channel symbol's definition terminals.
fn role_at(terms: &[ecad_model::TerminalDef], role: &str) -> Option<Pt> {
    terms.iter().find(|td| td.key.strip_prefix('.') == Some(role) || parse_legacy(&td.key).is_some_and(|x| x.1 == role)).map(|td| td.at)
}

/// The role `word` names among channel symbol `s`'s points (case ignored).
///
/// # Errors
/// No such point, naming the cell's points.
fn role_named(s: &Starter, word: &str) -> Result<&'static str, String> {
    let roles = roles_of(s);
    let w = word.trim();
    roles
        .iter()
        .copied()
        .find(|r| r.eq_ignore_ascii_case(w))
        .ok_or_else(|| format!("A {} cell has no point \"{}\". Its points: {}.", s.key, w.chars().take(16).collect::<String>(), roles.join(", ")))
}

/// The point a cell lands on a single wire by default (pack re-run, bug 10): an IO-Link port's
/// C/Q (its signal; L+ and L- are its supply), else its first point.
pub fn landing_role(s: &Starter) -> Option<&'static str> {
    let roles = roles_of(s);
    if cell_type(s) == Some(CardType::Iol) && roles.contains(&"C/Q") {
        return Some("C/Q");
    }
    roles.first().copied()
}

/// Whether the wire crossed at `m` runs mostly down from the crossing (pack re-run, bug 8): its
/// route reaches further below than above. A cell on such a wire is turned 180°, its frame and
/// texts above its points and clear of the wire, as Make Card Page draws outputs.
fn runs_down(doc: &Document, page: PageId, m: &crate::slash::Crossing) -> bool {
    let Some((_, _, v)) = crate::wires::route_vertices(doc, m.conductor, page) else { return false };
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
/// 180° when its first wire runs down ([`runs_down`]).
pub fn landings(doc: &Document, page: PageId, s: &Starter, members: &[crate::slash::Crossing], role: Option<&'static str>) -> Vec<Landing> {
    let roles = roles_of(s);
    let terms = s.definition().map(|d| d.terminals.clone()).unwrap_or_default();
    let rotation = |m: &crate::slash::Crossing| if runs_down(doc, page, m) { std::f64::consts::PI } else { 0.0 };
    let n = roles.len();
    if role.is_none() && n > 1 && members.len().is_multiple_of(n) {
        let groups: Option<Vec<Landing>> = members
            .chunks(n)
            .map(|chunk| {
                let mut chunk: Vec<crate::slash::Crossing> = chunk.to_vec();
                chunk.sort_by(|a, b| a.at.x.total_cmp(&b.at.x));
                let first = *chunk.first()?;
                let rot = rotation(&first);
                // The points as they lie left to right at that rotation.
                let turn = Placement { origin: Pt::new(0.0, 0.0), rotation: rot, mirror: false, scale: 1.0 };
                let mut lie: Vec<(&'static str, Pt)> = roles.iter().filter_map(|r| Some((*r, turn.apply(role_at(&terms, r)?)))).collect();
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

/// The ops landing a channel cell's point `t` on its wire at `on` (design 045 §4.5). The wire is
/// split at the point as Place Terminals splits it (028 §3.1), but a cell has one point, not a way
/// through: a half whose far end is attached to nothing, and which no cable holds, carries nothing
/// past the cell, so it is not kept. The wire ends at the point instead of running on through the
/// cell box, where its number label covered the channel number. A half that reaches something is
/// kept, as before.
///
/// # Errors
/// As [`crate::terminal_cmds::plan_land`].
fn land_cell(doc: &Document, ids: &mut IdAllocator, page: PageId, on: crate::wires::OnWire, t: TerminalId) -> Result<Vec<Op>, String> {
    let land = crate::terminal_cmds::plan_land(doc, ids, page, on, t)?;
    let Some(cond) = doc.conductors.get(&land.conductor) else { return Ok(land.ops) };
    if !doc.cables_of(land.conductor).is_empty() {
        return Ok(land.ops);
    }
    let (a_loose, b_loose) = (cond.end(End::A).is_none(), cond.end(End::B).is_none());
    if a_loose == b_loose {
        return Ok(land.ops);
    }
    // The new half (the far side, end B) is dropped: its conductor, representation and item.
    let new = land.new;
    let new_items: Vec<ItemId> = land
        .ops
        .iter()
        .filter_map(|op| match op {
            Op::LinkRepresentation { rep, .. } if rep.subject == Subject::Conductor(new) => Some(rep.items.clone()),
            _ => None,
        })
        .flatten()
        .collect();
    let rest_path = land.ops.iter().find_map(|op| match op {
        Op::AddItem { id, item } if new_items.contains(id) => Some(item.kind.clone()),
        _ => None,
    });
    let mut out = Vec::with_capacity(land.ops.len());
    for op in land.ops {
        match op {
            Op::CreateConductor { id, .. } if id == new => {}
            Op::AddItem { id, .. } if new_items.contains(&id) => {}
            Op::LinkRepresentation { rep, .. } if rep.subject == Subject::Conductor(new) => {}
            // End A's side is the loose one: the wire keeps its own ID, now drawn along the far
            // side, its end A on the point and its end B where it was.
            Op::ModifyItem { id, before, mut after } if a_loose && id_of_route(doc, land.conductor, page) == Some(id) => {
                if let Some(k) = rest_path.clone() {
                    after.kind = k;
                }
                out.push(Op::ModifyItem { id, before, after });
            }
            Op::AttachEnd { conductor, end: End::B, .. } if a_loose && conductor == land.conductor => {
                out.push(Op::AttachEnd { conductor, end: End::A, before: cond.end(End::A), after: Some(t) });
            }
            op => out.push(op),
        }
    }
    Ok(out)
}

/// The item drawing wire `c`'s route on `page`.
fn id_of_route(doc: &Document, c: ecad_model::ConductorId, page: PageId) -> Option<ItemId> {
    crate::wires::route_vertices(doc, c, page).map(|(id, _, _)| id)
}

/// The rest (design 045 §4.5): every channel of `c` not drawn yet, as cells in a row to the right of
/// its last cell on this page, or at `at`. No wires.
fn draw_rest(
    p: &mut crate::project::Project, pg: &mut crate::access::Pages<'_>, page: PageId, style: Style, c: ComponentId, t: CardType, at: Option<Pt>,
) -> Result<Value, String> {
    let doc = p.ledger.document();
    let tag = crate::devices::tag_words(doc, c);
    let n = channel_count(doc, c, t);
    let pts = points(doc, c);
    let missing: Vec<u32> = (0..n).filter(|i| !pts.contains_key(i)).collect();
    let first = first_number(doc, c);
    if missing.is_empty() {
        return Err(format!("Every channel of {tag} is drawn ({n})."));
    }
    let starter = crate::symbols::by_key(t.cell()).ok_or("No channel symbol for that type.")?;
    let width = starter.size().0;
    // To the right of the card's last cell on this page, else at `at`, else under the body.
    let start = match at {
        Some(q) => q,
        None => {
            let mut right: Option<Pt> = None;
            for (_, r) in doc.representations_of(Subject::Component(c)).filter(|(_, r)| r.page == page) {
                let Some(st) = crate::devices::rep_starter(doc, r) else { continue };
                if cell_type(st).is_none() {
                    continue;
                }
                if let Some((_, pl)) = crate::devices::rep_definition(doc, r) {
                    let q = Pt::new(pl.origin.x + width, pl.origin.y);
                    if right.is_none_or(|x| q.x > x.x) {
                        right = Some(q);
                    }
                }
            }
            right.ok_or("No cell of this card is on this page: give `at`, the first cell's place.")?
        }
    };
    let mut ops = Vec::new();
    let def = crate::terminal_cmds::definition_for(doc, &mut p.ids, starter, &mut ops)?;
    let roles = roles_of(starter);
    let mut used = crate::devices::labels_of(doc, c);
    for (k, i) in missing.iter().enumerate() {
        let pins = pins_for(doc, Some(c), t, n, *i, &roles, &mut used);
        let j = Join { index: *i, pins: roles.iter().copied().zip(pins).collect() };
        let dx = width * k as f64;
        let pl = Placement { origin: Pt::new(start.x + dx, start.y), rotation: 0.0, mirror: false, scale: 1.0 };
        plan_cell(&mut p.ids, page, style.clone(), def, starter, c, &j, pl, &mut ops)?;
    }
    p.apply_edit(pg, "plcchannels", "Draw remaining channels", ops)?;
    let message = format!("Drew {tag} {} ({} spare).", channels_words(first, &missing), missing.len());
    Ok(json!({"card": c.hex(), "tag": tag, "channels": missing.iter().map(|i| i + first).collect::<Vec<_>>(), "message": message}))
}

/// A channel's own property key: `plc.ch.<i>.<what>`.
pub fn ch_key(i: u32, what: &str) -> String {
    format!("{CH_PREFIX}{i}.{what}")
}

/// The index and field of a channel key `plc.ch.<i>.text` or `plc.ch.<i>.address`.
pub fn parse_ch_key(k: &str) -> Option<(u32, &'static str)> {
    let rest = k.strip_prefix(CH_PREFIX)?;
    let (i, what) = rest.split_once('.')?;
    if i.is_empty() || i.len() > 4 || !i.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let i: u32 = i.parse().ok().filter(|n| *n <= MAX_INDEX)?;
    let what = ["text", "address"].into_iter().find(|w| *w == what)?;
    Some((i, what))
}

/// The value check of a channel's text or address (design 045 §4.6).
///
/// # Errors
/// Too long, or a control character.
pub fn ch_value(what: &str, value: Option<&str>) -> Result<Option<String>, String> {
    let Some(v) = value.map(str::trim).filter(|v| !v.is_empty()) else { return Ok(None) };
    let max = if what == "address" { MAX_ADDRESS } else { MAX_TEXT };
    if v.chars().count() > max || v.chars().any(char::is_control) {
        let noun = if what == "address" { "An address" } else { "A channel description" };
        return Err(format!("{noun} is 1 to {max} characters, with no control characters."));
    }
    Ok(Some(v.to_string()))
}

/// `plcchannel {card, channel, text?, address?, pins?, move_to?}` (design 045 §4.6): one channel's
/// data in one transaction. `move_to` (a Checks fix) gives the points of one drawn copy of the
/// channel (`terminal`, or the last-drawn copy) the channel numbered `move_to`.
pub(crate) fn run_plcchannel(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let named = channel_param(params).map_err(err)?;
    // `index` (0-based, Properties ▸ Channel's pin rows) stands for `channel` when that is absent.
    let index = channel_param(&json!({"channel": params.get("index").cloned().unwrap_or(Value::Null)})).map_err(err)?;
    if named.is_none() && index.is_none() {
        return Err(err("`channel` (a number as shown) is required"));
    }
    let v = with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let (c, t) = channel_card(doc, params.get("card"))?;
        let who = crate::devices::tag_words(doc, c);
        let first = first_number(doc, c);
        let shown = named.or(index.map(|i| i.saturating_add(first))).unwrap_or(first);
        let i = shown.checked_sub(first).ok_or_else(|| format!("{who}'s channels start at {first}."))?;
        let n = channel_count(doc, c, t);
        let pts = points(doc, c);
        if i >= n && !pts.contains_key(&i) {
            return Err(format!("{who} has {n} channels ({first}–{}).", (n + first).saturating_sub(1)));
        }
        let props = doc.components.get(&c).map(|x| &x.properties);
        let mut ops = Vec::new();
        let mut said = Vec::new();
        for what in ["text", "address"] {
            let Some(v) = params.get(what) else { continue };
            let value = match v {
                Value::Null => None,
                Value::String(x) => ch_value(what, Some(x))?,
                _ => return Err(format!("`{what}` must be a string, or null to clear it")),
            };
            let k = ch_key(i, what);
            let before = props.and_then(|p| p.get(&k)).cloned();
            if before != value {
                said.push(format!(
                    "{what} {} → {}",
                    before.as_deref().map_or("(none)".to_string(), |b| format!("\"{b}\"")),
                    value.as_deref().map_or("(none)".to_string(), |a| format!("\"{a}\""))
                ));
                ops.push(Op::SetProperty { object: ObjectRef::Component(c), key: k, before, after: value });
            }
        }
        if let Some(pins) = params.get("pins") {
            let m = pins.as_object().ok_or("`pins` must be {role: pin}")?;
            let mine = pts.get(&i).cloned().unwrap_or_default();
            let mut labels = crate::devices::labels_of(doc, c);
            for (role, v) in m {
                let pin = v.as_str().map(str::trim).ok_or("A pin is text")?;
                if pin.is_empty() || pin.chars().count() > MAX_PIN || pin.chars().any(|ch| ch.is_whitespace() || ch.is_control()) {
                    return Err(format!("A pin is 1 to {MAX_PIN} characters, with no spaces."));
                }
                let Some((_, tid, old)) = mine.iter().find(|(r, _, _)| *r == role.as_str()) else {
                    return Err(format!("Channel {shown} of {who} has no {role} point."));
                };
                if old == pin {
                    continue;
                }
                if labels.contains(pin) {
                    let holder = pts.iter().flat_map(|(j, v)| v.iter().map(move |(r, _, l)| (*j, *r, l))).find(|(_, _, l)| *l == pin);
                    let what = holder.map_or_else(String::new, |(j, r, _)| format!(" (channel {}, {r})", j + first));
                    return Err(format!("{who} already has pin {pin}{what}. Pick another label."));
                }
                labels.remove(old);
                labels.insert(pin.to_string());
                let term = doc.terminals.get(tid).cloned().ok_or("No such point.")?;
                let after = Terminal { label: Some(pin.to_string()), ..term.clone() };
                ops.push(Op::RemoveTerminal { id: *tid, terminal: term });
                ops.push(Op::AddTerminal { id: *tid, terminal: after });
                said.push(format!("{role} pin {old} → {pin}"));
            }
        }
        if let Some(to) = params.get("move_to") {
            let to = channel_param(&json!({"channel": to}))?.ok_or("`move_to` is a channel number")?;
            let j = to.checked_sub(first).ok_or("No such channel.")?;
            if pts.contains_key(&j) {
                return Err(format!("Channel {to} of {who} is drawn already."));
            }
            let copies = super::copies(doc, c, i);
            let pick: Option<TerminalId> = crate::commands::opt_id(params, "terminal")?;
            let copy = match pick {
                Some(t) => copies.iter().find(|v| v.contains(&t)).cloned(),
                None => copies.last().cloned(),
            }
            .ok_or("That channel has no drawn copy to move.")?;
            for tid in copy {
                let term = doc.terminals.get(&tid).cloned().ok_or("No such point.")?;
                let role = point_of(doc, tid).map(|x| x.1).ok_or("Not a channel point.")?;
                let after = Terminal { key: Some(key(j, role)), ..term.clone() };
                ops.push(Op::RemoveTerminal { id: tid, terminal: term });
                ops.push(Op::AddTerminal { id: tid, terminal: after });
            }
            said.push(format!("moved to channel {to}"));
        }
        if ops.is_empty() {
            return Ok(json!({"card": c.hex(), "channel": shown, "changed": false}));
        }
        p.apply_edit(pg, "plcchannel", "Channel data", ops)?;
        let message = format!("{who} channel {shown}: {}.", said.join(", "));
        Ok(json!({"card": c.hex(), "channel": shown, "changed": true, "message": message}))
    })?;
    if let Some(m) = v.get("message").and_then(Value::as_str) {
        s.echo(m.to_string());
    }
    Ok(v)
}

/// The value check of `plc.ch.<i>.text` and `plc.ch.<i>.address` with `setprop`. `None`: not a
/// channel key.
pub fn setprop_value(key: &str, value: Option<&str>) -> Option<Result<Option<String>, String>> {
    let (_, what) = parse_ch_key(key)?;
    Some(ch_value(what, value))
}

/// Whether `id` is a definition of a channel symbol (for callers holding only an embedded ID).
pub fn is_channel_definition(doc: &Document, id: DefinitionId) -> bool {
    doc.definitions.get(&id).and_then(crate::symbols::of_definition).is_some_and(|s| cell_type(s).is_some())
}

/// `-A02 channel 3`.
pub fn channel_words(doc: &Document, c: ComponentId, i: u32) -> String {
    format!("{} channel {}", crate::devices::tag_words(doc, c), i + first_number(doc, c))
}

/// The fence's refusal when two crossed wires are closer than a cell is wide (brief 051, design
/// 051 §2.2): `Wires 24V-12 and 24V-13 are 10 mm apart; a DI cell is 20 mm wide. Space the wires
/// 20 mm apart, or place compact di symbols.` Wires exactly one width apart give abutting cells.
fn spacing_refusal(doc: &Document, s: &Starter, t: CardType, members: &[crate::slash::Crossing]) -> Option<String> {
    let (w, _) = crate::symbols::cell_shape(s.key)?;
    let mut at: Vec<(f64, f64, ecad_model::ConductorId)> = members.iter().map(|m| (m.at.x, m.at.y, m.conductor)).collect();
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
fn wire_words(doc: &Document, c: ecad_model::ConductorId) -> String {
    let n = crate::slash::wire_name(doc, c);
    n.strip_prefix("wire ").map(str::to_string).unwrap_or(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_parse_only_their_own_form() {
        assert_eq!(parse_key(".3.in"), Some((3, "in")));
        assert_eq!(parse_key(".0.+"), Some((0, "+")));
        assert_eq!(parse_key(".2.C/Q"), Some((2, "C/Q")));
        for bad in [".3", ".-1.in", ".3.nope", ".99999.in", "3.in", "..in", ".3.in.x", ""] {
            assert_eq!(parse_key(bad), None, "{bad}");
        }
        assert_eq!(parse_legacy("I1.3"), Some((11, "in")));
        assert_eq!(parse_legacy("Q0.0"), Some((0, "out")));
        for bad in ["I1.8", "I.3", "X1.1", "I99999.1", "I1.", "I1.33"] {
            assert_eq!(parse_legacy(bad), None, "{bad}");
        }
        assert_eq!(parse_ch_key("plc.ch.12.text"), Some((12, "text")));
        assert_eq!(parse_ch_key("plc.ch.x.text"), None);
        assert_eq!(parse_ch_key("plc.ch.1.other"), None);
    }

    #[test]
    fn pin_layouts() {
        let doc = Document::default();
        let mut used = BTreeSet::new();
        assert_eq!(pins_for(&doc, None, CardType::Ai, 4, 0, &["+", "-"], &mut used), ["1", "5"]);
        assert_eq!(pins_for(&doc, None, CardType::Ai, 4, 1, &["+", "-"], &mut used), ["2", "6"]);
        let mut used = BTreeSet::new();
        assert_eq!(pins_for(&doc, None, CardType::Di, 16, 3, &["in"], &mut used), ["4"]);
        // A taken pin moves on to the next free whole number.
        let mut used: BTreeSet<String> = ["4".to_string()].into();
        assert_eq!(pins_for(&doc, None, CardType::Di, 16, 3, &["in"], &mut used), ["5"]);
    }
}
