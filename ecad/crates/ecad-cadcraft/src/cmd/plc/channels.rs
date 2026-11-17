//! `plcchannels`, `plcchannel` and placing a channel cell (brief 045, design 045 §4): channels
//! on the page. The rules are [`crate::plc::channels`].

use cadcraft_engine::Session;
use ecad_electrical::CmdError;
use ecad_electrical::cmd::plc as rules;
use ecad_model::units::{Degrees, Radians};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::cmd::input::{Command, Point, TextOrNumber, nullable};
use crate::cmd::{err, on_project, plain_style};
use crate::plc::channels::*;
use crate::plc::rack::channel_count;
use crate::symbols::Starter;
use ecad_model::{Placement, Pt, TerminalId};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// `place {symbol: <channel symbol>, at, rotation?, mirror?, device?, tag?, channel?}` (design 045
/// §4.4): joins card `device` (or the card tagged `tag`, or a new card) as its next free channel or
/// the channel numbered `channel`; on a wire its first point lands there. One transaction.
pub(crate) fn place_channel(
    s: &mut Session, params: &crate::cmd::device::PlaceParams, starter: &'static Starter, at: Pt, degrees: Degrees, mirror: bool,
) -> cadcraft_engine::Result<Value> {
    let t = cell_type(starter).ok_or_else(|| err("That symbol is not a PLC channel."))?;
    let device = params.device_id()?;
    let tag = params.tag.clone();
    let named = match &params.channel {
        None => None,
        Some(v) => Some(rules::channel_number(&v.text()).map_err(|e| err(e.to_string()))?),
    };
    ecad_electrical::cmd::place::one_of(device.is_some(), tag.is_some(), "card").map_err(|e| err(e.to_string()))?;
    let modes = params.modes()?;
    let v = on_project(s, |p, pg, page| {
        let style = plain_style(p);
        let doc = p.ledger.document();
        let (card, new_tag) = ecad_electrical::cmd::place::card_target(doc, device, tag.as_deref())?;
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
                Some(x) => x.checked_sub(first_number(doc, card)).ok_or(CmdError::refused("Channels start at 0."))?,
                None => 0,
            };
            let mut used = BTreeSet::new();
            let pins = pins_for(doc, None, t, n, index, &roles, &mut used);
            Join { index, pins: roles.into_iter().zip(pins).collect() }
        } else {
            join(doc, card, starter, named, &BTreeSet::new(), &BTreeSet::new())?
        };
        let mut pl = Placement { origin: at, rotation: Radians::from(degrees), mirror, scale: 1.0 };
        // Pack re-run, bug 5: a cell placed near the line of its card's cells on this page goes onto
        // that line, so the card keeps one frame.
        if !new && let Some(q) = first_point(starter, &pl) {
            let dir = if (degrees.0.rem_euclid(360.0) - 180.0).abs() < 0.5 { 1.0 } else { -1.0 };
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
        let proposed = crate::draw::proposals::plan_place(p, modes, card, page, pl.origin, None).map_err(CmdError::refused)?;
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

/// `plcchannels {card?, type?, first?, point?, from, to}` (the fence) or `{card, rest: true, at?}`
/// (the rest; design 045 §4.5). One transaction.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlcchannelsParams {
    /// The card: an ID or a tag (absent: a new card).
    #[serde(default)]
    card: Option<String>,
    /// A new card's type: DI, DQ, AI, AO, RTD or IOL.
    #[serde(default, rename = "type")]
    ty: Option<String>,
    /// The first channel number, as shown.
    #[serde(default)]
    first: Option<TextOrNumber>,
    /// The point role landed on each wire (e.g. C/Q).
    #[serde(default)]
    point: Option<String>,
    /// One end of the line.
    #[serde(default)]
    from: Option<Point>,
    /// The other end of the line.
    #[serde(default)]
    to: Option<Point>,
    /// Every channel not drawn yet, as cells in a row (no wires).
    #[serde(default)]
    rest: Option<bool>,
    /// With `rest`: the first cell's place.
    #[serde(default)]
    at: Option<Point>,
}

impl Command for PlcchannelsParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let rest = self.rest.unwrap_or(false);
        let first = self.first.map(|t| rules::channel_number(&t.text())).transpose().map_err(|e| err(e.to_string()))?;
        let bad = |q: Pt| !(q.x.abs() <= crate::interact::MAX_COORD && q.y.abs() <= crate::interact::MAX_COORD);
        let v = on_project(s, |p, pg, page| {
            let style = plain_style(p);
            let doc = p.ledger.document();
            let card = match &self.card {
                Some(t) => Some(rules::channel_card(doc, t)?),
                None => None,
            };
            let plan = if rest {
                let (c, t) = card.ok_or(CmdError::input("`card` is required with `rest`."))?;
                let at = match self.at {
                    Some(q) if bad(q.0) => return Err(CmdError::input("`at` must be a point [x, y] on the sheet")),
                    other => other.map(|q| q.0),
                };
                rules::plan_rest(doc, &mut p.ids, page, style, c, t, at)?
            } else {
                let from = self.from.ok_or(CmdError::input("`from` [x,y] is required"))?.0;
                let to = self.to.ok_or(CmdError::input("`to` [x,y] is required"))?.0;
                if bad(from) || bad(to) {
                    return Err(CmdError::refused(crate::interact::OUT_OF_RANGE));
                }
                if from.near(to, p.tolerance) {
                    return Err(CmdError::input("`from` and `to` must differ"));
                }
                let input = rules::FenceInput { ty: self.ty.clone(), first, point: self.point.clone(), from, to };
                let tol = p.tolerance;
                rules::plan_channels(&p.ledger, &mut p.ids, &mut p.tag_high, page, style, card, &input, tol)?
            };
            let (c, tag, first) = (plan.card, plan.tag.clone(), plan.first);
            let channels: Vec<u32> = plan.placed.iter().map(|i| i + first).collect();
            if rest {
                p.apply_edit(pg, "plcchannels", "Draw remaining channels", plan.ops)?;
                let message = format!("Drew {tag} {} ({} spare).", channels_words(first, &plan.placed), plan.placed.len());
                return Ok(json!({"card": c.hex(), "tag": tag, "channels": channels, "message": message}));
            }
            p.apply_edit(pg, "plcchannels", "Place channels", plan.ops)?;
            p.last_placed = Some((plan.starter.to_string(), c));
            let k = plan.placed.len();
            let wires = if k == 1 { "1 wire".to_string() } else { format!("{k} wires") };
            let mut first_pins: Vec<String> = plan.pins.clone();
            first_pins.sort_by_key(|x| x.parse::<u64>().unwrap_or(u64::MAX));
            let message = format!("Placed {tag} {} on {wires} ({}).", channels_words(first, &plan.placed), pins_words(&first_pins));
            let doc = p.ledger.document();
            let count = channel_count(doc, c, plan.ty);
            let over = plan.placed.iter().filter(|i| **i >= count).count();
            let message = if over > 0 { format!("{message} {over} beyond its {count} channels: raise its count in Properties.") } else { message };
            Ok(json!({"card": c.hex(), "tag": tag, "channels": channels, "message": message}))
        })?;
        if let Some(m) = v.get("message").and_then(Value::as_str) {
            s.echo(m.to_string());
        }
        Ok(v)
    }
}

/// `plcchannel {card, channel, text?, address?, tag?, safe?, pins?, move_to?, terminal?}` (design
/// 045 §4.6): one channel's data in one transaction. `move_to` (a Checks fix) gives the points of
/// one drawn copy of the channel (`terminal`, or the last-drawn copy) the channel numbered
/// `move_to`.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlcchannelParams {
    /// The card: an ID or a tag.
    card: String,
    /// The channel number, as shown.
    #[serde(default)]
    channel: Option<TextOrNumber>,
    /// The 0-based channel index (stands for `channel` when that is absent).
    #[serde(default)]
    index: Option<TextOrNumber>,
    /// The channel's description; null clears it.
    #[serde(default, deserialize_with = "nullable")]
    text: Option<Option<String>>,
    /// A typed address; null clears it.
    #[serde(default, deserialize_with = "nullable")]
    address: Option<Option<String>>,
    /// The PLC tag; null clears it.
    #[serde(default, deserialize_with = "nullable")]
    tag: Option<Option<String>>,
    /// The safe state (outputs); null clears it.
    #[serde(default, deserialize_with = "nullable")]
    safe: Option<Option<String>>,
    /// New pins: {role: pin}.
    #[serde(default, deserialize_with = "nullable")]
    pins: Option<Option<BTreeMap<String, String>>>,
    /// Give a drawn copy of the channel this channel number.
    #[serde(default, deserialize_with = "nullable")]
    move_to: Option<Option<TextOrNumber>>,
    /// With `move_to`: a connection point of the copy to move (absent: the last drawn).
    #[serde(default)]
    terminal: Option<TerminalId>,
}

impl Command for PlcchannelParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let number = |t: Option<TextOrNumber>| t.map(|t| rules::channel_number(&t.text())).transpose().map_err(|e| err(e.to_string()));
        let channel = number(self.channel)?;
        // `index` (0-based, Properties ▸ Channel's pin rows) stands for `channel` when that is absent.
        let index = number(self.index)?;
        if channel.is_none() && index.is_none() {
            return Err(err("`channel` (a number as shown) is required"));
        }
        let values: Vec<(&'static str, Option<String>)> =
            [("text", self.text), ("address", self.address), (crate::plc::signals::TAG, self.tag), (crate::plc::signals::SAFE, self.safe)]
                .into_iter()
                .filter_map(|(what, v)| v.map(|v| (what, v)))
                .collect();
        let input = rules::ChannelInput {
            channel,
            index,
            values,
            pins: self.pins,
            move_to: self.move_to.map(|m| m.map(|t| t.text())),
            terminal: self.terminal,
        };
        let v = on_project(s, |p, pg, _| {
            let plan = rules::plan_channel(p.ledger.document(), &self.card, &input)?;
            let (c, shown) = (plan.card, plan.shown);
            if plan.ops.is_empty() {
                return Ok(json!({"card": c.hex(), "channel": shown, "changed": false}));
            }
            p.apply_edit(pg, "plcchannel", "Channel data", plan.ops)?;
            let message = format!("{} channel {shown}: {}.", plan.who, plan.said.join(", "));
            Ok(json!({"card": c.hex(), "channel": shown, "changed": true, "message": message}))
        })?;
        if let Some(m) = v.get("message").and_then(Value::as_str) {
            s.echo(m.to_string());
        }
        Ok(v)
    }
}
