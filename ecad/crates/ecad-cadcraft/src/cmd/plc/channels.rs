//! `plcchannels`, `plcchannel` and placing a channel cell (brief 045, design 045 §4): channels
//! on the page. The rules are [`crate::plc::channels`].

use cadcraft_engine::Session;

use crate::cmd::{err, opt_typed, plain_style, point, with_project};
use crate::plc::card_type_of;
use crate::plc::channels::*;
use crate::plc::rack::{CardType, MAX_CHANNELS, channel_count};
use crate::symbols::Starter;
use ecad_model::{ComponentId, Document, ObjectRef, Op, PageId, Placement, Pt, Style, Subject, Terminal, TerminalId};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// `place {symbol: <channel symbol>, at, rotation?, mirror?, device?, tag?, channel?}` (design 045
/// §4.4): joins card `device` (or the card tagged `tag`, or a new card) as its next free channel or
/// the channel numbered `channel`; on a wire its first point lands there. One transaction.
pub(crate) fn run_place_channel(
    s: &mut Session, params: &Value, starter: &'static Starter, at: Pt, degrees: f64, mirror: bool,
) -> cadcraft_engine::Result<Value> {
    let t = cell_type(starter).ok_or_else(|| err("That symbol is not a PLC channel."))?;
    let device: Option<ComponentId> = crate::cmd::opt_id(params, "device").map_err(err)?;
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
        let def = crate::cmd::terminal::definition_for(doc, &mut p.ids, starter, &mut ops)?;
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
            let line = crate::plc::look::runs_of_card(doc, card, page)
                .iter()
                .filter(|r| r.dir == dir && (r.top - q.y).abs() <= SNAP_TO_LINE)
                .map(|r| r.top)
                .min_by(|a, b| (a - q.y).abs().total_cmp(&(b - q.y).abs()));
            if let Some(y) = line.filter(|y| y.is_finite()) {
                pl.origin.y += y - q.y;
            }
        }
        // On a wire: its first point lands there (design 045 §4.4), the symbol shifted onto it.
        let landing = first_point(starter, &pl).and_then(|q| crate::cmd::terminal::landing_point(doc, page, q).map(|on| (q, on)));
        if let Some((q, on)) = &landing {
            pl.origin = Pt::new(pl.origin.x + on.at.x - q.x, pl.origin.y + on.at.y - q.y);
        }
        let cell = plan_cell(&mut p.ids, page, style, def, starter, card, &j, pl, &mut ops)?;
        let mut wire = None;
        if let (Some((_, on)), Some((_, t0, _, _))) = (landing, cell.terminals.first()) {
            let land = crate::connectors::plan_land(doc, &mut p.ids, page, on, *t0)?;
            wire = Some(land.conductor);
            ops.extend(land.ops);
        }
        // Brief 052d (design 052 §5.3): the place proposes for the card.
        let proposed = crate::draw::proposals::plan_place(p, params, card, page, pl.origin, None)?;
        ops.extend(proposed.ops.iter().cloned());
        p.apply_edit(pg, "place", "Place channel", ops)?;
        p.last_placed = Some((starter.key.to_string(), card));
        let doc = p.ledger.document();
        let first = first_number(doc, card);
        let pins: Vec<String> = cell.terminals.iter().map(|(_, _, pin, _)| pin.clone()).collect();
        let on = wire.map(|w| format!(" on wire {}", crate::slash::wire_name(doc, w))).unwrap_or_default();
        let message = format!("Placed {card_tag} channel {} ({}){on}.", j.index + first, pins_words(&pins));
        let mut v = json!({
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
        });
        crate::draw::proposals::finish_place(p, pg, card, !new, &proposed, &mut v);
        Ok(v)
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
    let c = crate::plc::rack::card_param(doc, v)?;
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
        let def = crate::cmd::terminal::definition_for(doc, &mut p.ids, starter, &mut ops)?;
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
                let on = crate::cmd::terminal::on_wire_of(doc, page, m.conductor, at).ok_or("A crossed wire has no route here.")?;
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
    let def = crate::cmd::terminal::definition_for(doc, &mut p.ids, starter, &mut ops)?;
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
        for what in ["text", "address", crate::plc::signals::TAG, crate::plc::signals::SAFE] {
            let Some(v) = params.get(what) else { continue };
            let value = match v {
                Value::Null => None,
                // Brief 056 (design 056 §4.4): the safe state by the card's type.
                Value::String(x) if what == crate::plc::signals::SAFE => crate::plc::signals::safe_value(Some(t), Some(x))?,
                Value::String(x) => ch_value(what, Some(x))?,
                _ => return Err(format!("`{what}` must be a string, or null to clear it")),
            };
            // Brief 056 (design 056 §4.3): a tag is unique in its PLC, compared without case.
            if what == crate::plc::signals::TAG
                && let Some(tag) = &value
                && let Some(holder) = crate::plc::signals::tag_holder(doc, c, i, tag)
            {
                return Err(crate::plc::signals::duplicate_words(doc, tag, holder));
            }
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
            let copies = crate::plc::copies(doc, c, i);
            let pick: Option<TerminalId> = crate::cmd::opt_id(params, "terminal")?;
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
