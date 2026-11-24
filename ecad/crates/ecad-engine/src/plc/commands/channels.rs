//! `plcchannels` and placing a channel cell: channels
//! on the page. The rules are [`ecad_electrical::plc::channels`].

use cadcraft_engine::Session;
use ecad_electrical::CmdError;
use ecad_electrical::plc::commands as rules;
use ecad_model::geometry::units::{Degrees, Radians};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::commands::input::Command;
use crate::commands::{err, on_project, plain_style};
use ecad_electrical::commands::input::{Point, TextOrNumber};
use ecad_electrical::plc::channels::*;
use ecad_electrical::plc::rack::channel_count;
use ecad_electrical::symbols::Starter;
use ecad_model::{Placement, Pt};
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// `place {symbol: <channel symbol>, at, rotation?, mirror?, device?, tag?, channel?}`:
/// joins card `device` (or the card tagged `tag`, or a new card) as its next free channel or
/// the channel numbered `channel`; on a wire its first point lands there. One transaction.
pub(crate) fn place_channel(
    s: &mut Session, params: &crate::devices::commands::PlaceParams, starter: &'static Starter, at: Pt, degrees: Degrees, mirror: bool,
) -> cadcraft_engine::Result<Value> {
    let t = cell_type(starter).ok_or_else(|| err("That symbol is not a PLC channel."))?;
    let device = params.device_id()?;
    let tag = params.tag.clone();
    let named = match &params.channel {
        None => None,
        Some(v) => Some(rules::channel_number(&v.text()).map_err(|e| err(e.to_string()))?),
    };
    ecad_electrical::devices::commands::place::one_of(device.is_some(), tag.is_some(), "card").map_err(|e| err(e.to_string()))?;
    let modes = params.modes()?;
    let v = on_project(s, |p, pg, page| {
        let style = plain_style(p);
        let doc = p.ledger.document();
        let (card, new_tag) = ecad_electrical::devices::commands::place::card_target(doc, device, tag.as_deref())?;
        let mut ops = Vec::new();
        let def = ecad_electrical::terminal_strips::commands::terminal_strips::definition_for(doc, &mut p.ids, starter, &mut ops)?;
        let (card, card_tag, new) = match card {
            Some(c) => (c, ecad_electrical::devices::tags::tag_words(doc, c), false),
            None => {
                let (c, t2) = new_card_ops(&p.ledger, &mut p.ids, &mut p.tag_high, starter, def, t, new_tag, &mut ops)?;
                (c, t2, true)
            }
        };
        let doc = p.ledger.document();
        let j = if new {
            // The new card is made in `ops`: its count is the one it is made with.
            let n = ecad_electrical::plc::edit::current(doc, &ops, card).and_then(|d| match d.device_type {
                ecad_model::DeviceType::PlcModule(m) => Some(m.channel_count()),
                _ => None,
            });
            let n = n.unwrap_or(0);
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
            let line = ecad_electrical::plc::draw::runs_of_card(doc, card, page)
                .iter()
                .filter(|r| r.dir == dir && (r.top - q.y).abs() <= SNAP_TO_LINE)
                .map(|r| r.top)
                .min_by(|a, b| (a - q.y).abs().total_cmp(&(b - q.y).abs()));
            if let Some(y) = line.filter(|y| y.is_finite()) {
                pl.origin.y += y - q.y;
            }
        }
        // On a wire: its first point lands there, the symbol shifted onto it.
        let landing = first_point(starter, &pl).and_then(|q| ecad_electrical::wires::placing::landing_point(doc, page, q).map(|on| (q, on)));
        if let Some((q, on)) = &landing {
            pl.origin = Pt::new(pl.origin.x + on.at.x - q.x, pl.origin.y + on.at.y - q.y);
        }
        let cell = plan_cell(&mut p.ids, page, style, def, starter, card, &j, pl, &mut ops)?;
        let mut wire = None;
        if let (Some((_, on)), Some((_, t0, _, _))) = (landing, cell.connection_points.first()) {
            let land = ecad_electrical::connectors::plan_land(doc, &mut p.ids, on, *t0)?;
            wire = Some(land.route);
            ops.extend(land.ops);
        }
        // The place proposes for the card.
        let proposed = crate::symbols::draw::plan_place(p, &ops, modes, card, page, pl.origin, None).map_err(CmdError::refused)?;
        ops.extend(proposed.ops.iter().cloned());
        p.apply_edit(pg, "place", "Place channel", ops)?;
        p.last_placed = Some(ecad_electrical::commands::run::LastPlaced { starter: Some(starter.key), device: card });
        let doc = p.ledger.document();
        let first = first_number(doc, card);
        let pins: Vec<String> = cell.connection_points.iter().map(|(_, _, pin, _)| pin.clone()).collect();
        let on = wire
            .and_then(|r| ecad_electrical::wires::routes::wire_of_route(doc, r))
            .map(|w| format!(" on wire {}", ecad_electrical::cables::slash::wire_name(doc, w)))
            .unwrap_or_default();
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
            "connection_points": cell.connection_points.iter().map(|(r, t, pin, _)| json!({"role": r, "label": pin, "key": key(j.index, *r), "connection_point": t.hex()})).collect::<Vec<_>>(),
            "message": message,
        });
        crate::symbols::draw::finish_place(p, pg, card, !new, &proposed, &mut v);
        Ok(v)
    })?;
    if let Some(m) = v.get("message").and_then(Value::as_str) {
        s.echo(m.to_string());
    }
    Ok(v)
}

/// Draws PLC channel cells, in one transaction: one cell on each wire the line from `from` to `to` crosses, in
/// order (at most 256; a cell with several points spans its own wires when they are spaced as its points; turned
/// 180° where the wires run down), or with `{card, rest: true, at?}` every channel not drawn yet as cells in a
/// row, without wires.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlcchannelsParams {
    /// The card: an ID or a tag (absent: a new card).
    #[serde(default)]
    card: Option<String>,
    /// A new card's type (default DI; `DO` reads as DQ).
    #[serde(default, rename = "type")]
    ty: Option<ecad_electrical::commands::input::Said<ecad_electrical::plc::rack::IoType>>,
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
        let bad = |q: Pt| !(q.x.abs() <= ecad_electrical::commands::input::MAX_COORD && q.y.abs() <= ecad_electrical::commands::input::MAX_COORD);
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
                    return Err(CmdError::refused(ecad_electrical::commands::input::OUT_OF_RANGE));
                }
                if from.near(to, p.tolerance) {
                    return Err(CmdError::input("`from` and `to` must differ"));
                }
                let input = rules::FenceInput { ty: self.ty.map(|t| t.0.0), first, point: self.point.clone(), from, to };
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
            p.last_placed = Some(ecad_electrical::commands::run::LastPlaced { starter: Some(plan.starter), device: c });
            let k = plan.placed.len();
            let wires = if k == 1 { "1 wire".to_string() } else { format!("{k} wires") };
            let mut first_pins: Vec<String> = plan.pins.clone();
            first_pins.sort_by_key(|x| x.parse::<u64>().unwrap_or(u64::MAX));
            let message = format!("Placed {tag} {} on {wires} ({}).", channels_words(first, &plan.placed), pins_words(&first_pins));
            let doc = p.ledger.document();
            let count = channel_count(doc, c);
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
