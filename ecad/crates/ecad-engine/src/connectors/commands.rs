//! The commands `connector` (Place ▸ Connector) and `pinlabel`,
//! and `place` for a connector half or pin (Find Symbol).
//!
//! Every write is one transaction, committed through [`Project::apply_edit`]; nothing here opens a
//! dialog. The interactive form of `connector` ([`crate::connectors::interact::connector`]) commits through
//! this JSON form.
//!
//! `connector` and `pinlabel` are thin shims: their params structs read the JSON, the
//! rules in [`ecad_electrical::connectors::commands`] decide, and the shim commits the ops and answers in
//! JSON. `place`'s connector form is [`place_half`], part of `place`'s shim.
//!
//! [`Project::apply_edit`]: crate::project::Project::apply_edit

use ecad_electrical::commands::input::Said;
use ecad_electrical::defaults::connectors::{ConnectorSide, MakerLayout};
use ecad_model::geometry::units::{Degrees, Radians};
use std::collections::BTreeSet;

use cadcraft_engine::{CommandSpec, Session};
use ecad_electrical::CmdError;
use ecad_electrical::connectors::commands::{self as rules, ConnectorInput, Made, NamesInput, Pinlabel, PinlabelInput};
use ecad_model::{ConnectionPointId, PageId, Placement, Pt, WireId};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::commands::input::{Command, Typed};
use crate::commands::{err, on_project, plain_style};
use ecad_electrical::commands::input::{Bounded, Point};
use ecad_electrical::connectors::{self as cn, Names};
use ecad_electrical::symbols::Starter;

/// The commands of this module with a typed input.
pub const TYPED: &[Typed] = &[Typed::of::<ConnectorParams>("connector"), Typed::of::<PinlabelParams>("pinlabel")];

/// The commands this module adds (registered in `lanes.rs`).
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("connector", "Connector", crate::commands::input::typed::<ConnectorParams>)
            .interactive(crate::connectors::interact::connector)
            .noundo(),
        CommandSpec::new("pinlabel", "Rename Pin", crate::commands::input::typed::<PinlabelParams>).noundo(),
    ]
}

// ---------------------------------------------------------------- connector

/// Pin names: numbers, letters, the names as text, or a list.
#[derive(Deserialize, JsonSchema)]
#[serde(untagged)]
enum NamesParam {
    /// The pin names in order.
    List(Vec<String>),
    /// `numbers`, `letters`, or the names as text (`L,N,PE`).
    Word(String),
}

/// A connector: a row of whole halves, pins on the wires a line crosses (`line`) or on one wire (`wire`), or a
/// bulkhead. Absent fields take the registry's `connector.maker.*` answers. One transaction.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConnectorParams {
    /// The connector to join: an id or a tag (absent: a new connector).
    #[serde(default)]
    device: Option<String>,
    /// Socket, plug or both (also `s`, `p` or `b`).
    #[serde(default)]
    halves: Option<Said<ConnectorSide>>,
    /// The family (also `d-sub`, `sub-d` or `usb-a`).
    #[serde(default)]
    family: Option<Said<ecad_model::objects::devices::ConnectorFamily>>,
    /// The pin count, 1 to 64.
    #[serde(default)]
    pins: Option<u64>,
    /// numbers, letters, or the pin names ["L", "N", "PE"].
    #[serde(default)]
    names: Option<NamesParam>,
    /// Row (whole halves) or pins (one pin on each wire).
    #[serde(default)]
    layout: Option<MakerLayout>,
    /// Where a row (or a bulkhead) goes.
    #[serde(default)]
    at: Option<Point>,
    /// 0, 90, 180 or 270.
    #[serde(default)]
    rotation: Option<Degrees>,
    /// A row's halves swapped.
    #[serde(default)]
    swap: Option<bool>,
    /// Pins: the line across the wires, [[x, y], [x, y]].
    #[serde(default)]
    line: Option<Bounded<Point, 2, 2>>,
    /// Pins: the one wire a pin lands on.
    #[serde(default)]
    wire: Option<WireId>,
    /// The first pin's name.
    #[serde(default)]
    pin: Option<String>,
    /// The page (absent: the active one).
    #[serde(default)]
    page: Option<PageId>,
}

impl Command for ConnectorParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let degrees = self.rotation.unwrap_or_default();
        if ![0.0, 90.0, 180.0, 270.0].contains(&degrees.0) {
            return Err(err("`rotation` is 0, 90, 180 or 270"));
        }
        let at = self.at.map(|p| p.0);
        let line = self.line.and_then(|l| match l.0.as_slice() {
            [a, b] => Some((a.0, b.0)),
            _ => None,
        });
        for p in at.iter().chain(line.iter().flat_map(|(a, b)| [a, b])) {
            if rules::out_of_range(*p) {
                return Err(err(ecad_electrical::commands::input::OUT_OF_RANGE));
            }
        }
        let names = self.names.map(|n| match n {
            NamesParam::List(v) => NamesInput::List(v),
            NamesParam::Word(w) => NamesInput::Word(w),
        });
        let input = ConnectorInput {
            page: self.page,
            device: self.device,
            halves: self.halves.map(|h| h.0),
            family: self.family.map(|f| f.0),
            pins: self.pins,
            names,
            layout: self.layout,
            at,
            line,
            wire: self.wire,
            degrees,
            swap: self.swap.unwrap_or(false),
            pin: self.pin.map(|p| p.trim().to_string()),
        };
        let v = on_project(s, |p, pg, active| {
            let style = plain_style(p);
            let tol = p.tolerance;
            let done = rules::plan_connector(&p.ledger, &mut p.ids, &mut p.indexes.tag_high, active, style, input, tol)?;
            // One transaction.
            p.apply_edit(pg, "connector", &done.label, done.made.ops.clone())?;
            p.defaults.last_placed = Some(ecad_electrical::commands::run::LastPlaced { starter: None, device: done.made.component });
            let doc = p.ledger.document();
            let message = rules::echo(doc, &done);
            Ok(rules::result_json(doc, &done.made, &message))
        })?;
        if let Some(m) = v.get("message").and_then(Value::as_str) {
            s.echo(m.to_string());
        }
        Ok(v)
    }
}

// ---------------------------------------------------------------- place (Find Symbol)

/// `place {symbol: <connector half or pin>, at, rotation?, mirror?, device?, tag?, pin?}`:
/// a new connector, or joining the connector named by `device` (or `tag`); a pin takes
/// the next pin or `pin`, and on a wire it lands there. One transaction.
pub(crate) fn place_half(
    s: &mut Session, params: &crate::devices::commands::PlaceParams, starter: &'static Starter, at: Pt, degrees: Degrees, mirror: bool,
) -> cadcraft_engine::Result<Value> {
    let tag = params.tag.clone();
    let device = params.device.clone();
    let pin = params.pin.as_deref().map(|p| p.trim().to_string());
    let side = starter.side().ok_or_else(|| err("That symbol is not a connector half."))?;
    let modes = params.modes()?;
    let v = on_project(s, |p, pg, page| {
        let doc = p.ledger.document();
        let (device, new_tag) = ecad_electrical::devices::commands::place::connector_target(doc, side, device.as_deref(), tag.as_deref())?;
        let style = plain_style(p);
        let mut ops = Vec::new();
        let def = ecad_electrical::terminal_strips::commands::terminal_strips::definition_for(doc, &mut p.ids, starter, &mut ops)?;
        let taken = BTreeSet::new();
        let pin_starter = cn::is_pin_starter(starter);
        let names: Vec<String> = match (device, pin_starter) {
            (Some(c), true) => vec![match &pin {
                Some(n) => {
                    cn::check_named_pin(doc, c, side, n, &taken)?;
                    n.clone()
                }
                None => cn::next_pin(doc, c, side, &taken)?,
            }],
            (None, true) => {
                let n = pin.clone().unwrap_or_else(|| "1".into());
                cn::check_name(&n)?;
                vec![n]
            }
            (Some(c), false) => {
                cn::check_whole_half(doc, c, side, &taken)?;
                rules::join_names(doc, c, cn::size_of(starter).unwrap_or(0))
            }
            (None, false) => cn::names_for(&Names::Numbers, cn::size_of(starter).unwrap_or(0))?,
        };
        let (c, tag_text, new) = match device {
            Some(c) => (c, ecad_electrical::devices::tags::tag_words(doc, c), false),
            None => {
                let pins = cn::size_of(starter);
                let (c, t) = cn::create_ops(&p.ledger, &mut p.ids, &mut p.indexes.tag_high, starter, def, new_tag, pins, None, &mut ops)?;
                (c, t, true)
            }
        };
        let landing = if pin_starter { ecad_electrical::wires::placing::landing_point(doc, page, at) } else { None };
        let origin = landing.map_or(at, |on| on.at);
        let rotation = match landing {
            Some(on) if degrees.0 == 0.0 => rules::along_wire(doc, &on),
            _ => Radians::from(degrees),
        };
        let placement = Placement { origin, rotation, mirror, scale: 1.0 };
        let (rep, _, shown) = cn::symbol_ops(&mut p.ids, page, style, def, starter, c, &names, placement, &mut ops)?;
        let land = match landing {
            Some(on) => Some(cn::land_pin(doc, &mut p.ids, on, &shown, placement)?),
            None => None,
        };
        if let Some(l) = &land {
            ops.extend(l.ops.clone());
        }
        let label = if new { format!("Place connector {tag_text}") } else { format!("Add symbol to {tag_text}") };
        // The place proposes; a half's conflict may answer Half.
        let proposed = crate::symbols::draw::plan_place(p, &ops, modes, c, page, origin, starter.side()).map_err(CmdError::refused)?;
        ops.extend(proposed.ops.iter().cloned());
        p.apply_edit(pg, "place", &label, ops)?;
        p.defaults.last_placed = Some(ecad_electrical::commands::run::LastPlaced { starter: Some(starter.key), device: c });
        let doc = p.ledger.document();
        let made = Made {
            component: c,
            tag: tag_text.clone(),
            new,
            shown: shown.clone(),
            splits: land.iter().map(|l| (l.route, l.new)).collect(),
            ops: Vec::new(),
        };
        let names: Vec<String> = shown.iter().filter_map(|x| cn::pin_name(doc, x.connection_point)).collect();
        let message = format!(
            "Placed {}.",
            if names.len() > 2 {
                format!("{}…{}", names.first().cloned().unwrap_or_default(), names.last().cloned().unwrap_or_default())
            } else {
                names.join(" and ")
            }
        );
        let mut v = rules::result_json(doc, &made, &message);
        v["symbol"] = json!(starter.key);
        v["joined"] = json!(!new);
        v["representation"] = json!(rep.hex());
        v["definition"] = json!(def.hex());
        crate::symbols::draw::finish_place(p, pg, c, !new, &proposed, &mut v);
        Ok(v)
    })?;
    if let Some(m) = v.get("message").and_then(Value::as_str) {
        s.echo(m.to_string());
    }
    Ok(v)
}

// ---------------------------------------------------------------- pinlabel

/// Renames a connector pin on both halves in one transaction, or with `connection_point` and `to: "next"` moves
/// that one drawn pin to its half's next free pin.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PinlabelParams {
    /// The connector: an id or a tag.
    #[serde(default)]
    device: Option<String>,
    /// With `device`: the pin's current name.
    #[serde(default)]
    pin: Option<String>,
    /// One drawn pin.
    #[serde(default)]
    connection_point: Option<ConnectionPointId>,
    /// The new pin name, or "next".
    to: String,
}

impl Command for PinlabelParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let input = PinlabelInput { device: self.device, pin: self.pin, connection_point: self.connection_point, to: self.to };
        let v = on_project(s, |p, pg, _| {
            Ok(match rules::plan_pinlabel(p.ledger.document(), input)? {
                Pinlabel::Unchanged { component, to } => json!({"changed": false, "component": component.hex(), "to": to}),
                Pinlabel::Moved { component, from, to, ops, label, message } => {
                    p.apply_edit(pg, "pinlabel", &label, ops)?;
                    json!({"changed": true, "component": component.hex(), "from": from, "to": to, "message": message})
                }
                Pinlabel::Renamed { component, tag, from, to, ops, label } => {
                    p.apply_edit(pg, "pinlabel", &label, ops)?;
                    let message = rules::renamed_message(p.ledger.document(), component, &tag, &from, &to);
                    json!({"changed": true, "component": component.hex(), "from": from, "to": to, "message": message})
                }
            })
        })?;
        if let Some(m) = v.get("message").and_then(Value::as_str) {
            s.echo(m.to_string());
        }
        Ok(v)
    }
}
