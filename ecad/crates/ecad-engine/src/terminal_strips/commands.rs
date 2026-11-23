//! The commands: a terminal placed on a wire splits it
//! (through `place`), `connection_points` (Place Terminals: one per wire a fence line crosses), `bridge`
//! (Draw Bridge). The reads `stripplan` and `cablecores` are generic
//! ([`ecad_electrical::terminal_strips::commands::reads`]).
//!
//! Every write is one transaction, committed through [`Project::apply_edit`]; the reads record
//! nothing.
//!
//! Each command is a thin shim: its params struct reads the JSON, the rules in
//! [`ecad_electrical::terminal_strips::commands::terminal_strips`] decide, and the shim commits the ops and answers in JSON.

use cadcraft_engine::{CommandSpec, Session};
use ecad_electrical::CmdError;
use ecad_electrical::terminal_strips::commands::terminal_strips::{self as rules, Bridge};
use ecad_electrical::terminal_strips::commands::terminal_strips::{Fence, definition_for, land_level, level_symbol, plan_fence};
use ecad_model::geometry::units::{Degrees, Radians};
use ecad_model::{ConnectionPointId, DeviceId, Document, Op, PageId, Placement, Pt, RouteEnd, RouteId, Style, WireId};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::commands::input::{Command, Typed};
use crate::commands::{err, on_project, plain_style};
use ecad_electrical::connectors::Land;
use ecad_electrical::terminal_strips::connection_point_words;
use ecad_electrical::terminal_strips::terminal_blocks as tt;
use {ecad_electrical::wires::ON_WIRE, ecad_electrical::wires::placing::OnRoute};

/// The commands of this module with a typed input.
pub const TYPED: &[Typed] = &[Typed::of::<BridgeParams>("bridge")];

/// The commands this module adds.
pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec::new("bridge", "Draw Bridge", crate::commands::input::typed::<BridgeParams>).interactive(crate::interact::bridge).noundo()]
}

// ---------------------------------------------------------------- landing on a wire

/// The echo of a landed terminal.
fn land_words(doc: &Document, t: ConnectionPointId, land: &Land) -> String {
    format!("Placed {} on wire {} (split in two).", connection_point_words(doc, t), route_words(doc, land.route))
}

/// Where a terminal starter placed at `at` lands on a route (`place`), or `None` when `at`
/// is on no route (or on a route's free end): it is then placed as before.
pub(crate) fn landing_point(doc: &Document, page: PageId, at: Pt) -> Option<OnRoute> {
    let on = ecad_electrical::wires::placing::route_at(doc, page, at, ON_WIRE, None)?;
    let route = doc.routes().get(&on.route)?;
    let [first, last] = route.end_points()?;
    for (p, e) in [first, last].into_iter().zip(route.ends) {
        if p.near(on.at, ON_WIRE) && e == RouteEnd::Free {
            return None;
        }
    }
    Some(on)
}

/// The JSON fields and echo of a landed place.
pub(crate) fn land_result(doc: &Document, t: ConnectionPointId, land: &Land) -> (Value, String) {
    (json!({"split": {"route": land.route.hex(), "new": land.new.hex()}}), land_words(doc, t, land))
}

// ---------------------------------------------------------------- Place Terminals

/// The level a prompt or JSON names for type `t` on strip `c` ([`rules::level_word`]), for the
/// prompt.
///
/// # Errors
/// A word that names no level, with the type's levels.
pub fn level_word(doc: &Document, c: Option<DeviceId>, t: &tt::BlockType, word: &str) -> Result<Option<u32>, CmdError> {
    rules::level_word(doc, c, t, word)
}

/// [`plan_fence`] on project `p`, with its own IDs and tag numbers (tools such as the bench, which
/// commit the plan themselves).
///
/// # Errors
/// As [`plan_fence`].
pub fn plan_fence_in(
    p: &mut crate::project::Project, page: PageId, style: Style, strip: Option<DeviceId>, pe: bool, from: Pt, to: Pt,
) -> Result<Fence, CmdError> {
    let tol = p.tolerance;
    plan_fence(&p.ledger, &mut p.ids, &mut p.tag_high, page, style, strip, pe, from, to, tol)
}

/// The net number of conductor `w`, else its name.
fn wire_words(doc: &Document, w: WireId) -> String {
    ecad_model::connectivity::wiring::wire_number(doc, w).unwrap_or_else(|| ecad_electrical::cables::slash::wire_name(doc, w))
}

/// The words for the wire route `r` carries ([`ecad_electrical::wires::routes::wire_of_route`]), `?` for none.
fn route_words(doc: &Document, r: RouteId) -> String {
    ecad_electrical::wires::routes::wire_of_route(doc, r).map_or_else(|| "?".to_string(), |w| wire_words(doc, w))
}

/// `place {symbol: <level starter>, at, rotation?, mirror?, device?, tag?, position?}`:
/// a level symbol (or whole terminal, disconnect or fuse terminal) joins strip `device`
/// (or the strip tagged `tag`, or a new strip) at the next position of its type that lacks its
/// level, or at `position`; on a wire it lands there. One transaction.
pub(crate) fn place_level(
    s: &mut Session, params: &crate::devices::commands::PlaceParams, starter: &'static ecad_electrical::symbols::Starter, at: Pt, degrees: Degrees,
    mirror: bool,
) -> cadcraft_engine::Result<Value> {
    let (ty, level) = tt::of_starter(starter.key).ok_or_else(|| err("That symbol is not a terminal level."))?;
    let device = params.device_id()?;
    let tag = params.tag.clone();
    let position = params.position.clone();
    ecad_electrical::devices::commands::place::one_of(device.is_some(), tag.is_some(), "strip").map_err(|e| err(e.to_string()))?;
    let modes = params.modes()?;
    let v = on_project(s, |p, pg, page| {
        let style = plain_style(p);
        let doc = p.ledger.document();
        let (strip, new_tag) = ecad_electrical::devices::commands::place::strip_target(doc, device, tag.as_deref())?;
        let mut ops = Vec::new();
        let def = definition_for(doc, &mut p.ids, starter, &mut ops)?;
        let mut chooser = tt::Chooser::new(doc, strip);
        let position = chooser.choose(doc, strip, ty, level, position.as_deref())?;
        let (c, tag_text) = match strip {
            Some(c) => (c, ecad_electrical::devices::tags::tag_words(doc, c)),
            None => {
                let c: DeviceId = p.ids.fresh();
                let t = p.tag_high.given(&p.ledger, new_tag, &ecad_electrical::devices::prefix_for(doc, starter))?;
                ops.push(Op::CreateDevice {
                    id: c,
                    device: ecad_electrical::devices::with_given(
                        ecad_model::Device {
                            description: ecad_electrical::devices::description_of("Terminal"),
                            definition: Some(def),
                            device_type: starter.device_type(),
                            ..ecad_model::Device::default()
                        },
                        &t,
                    )?,
                });
                (c, t.shown())
            }
        };
        let landing = landing_point(doc, page, at);
        let origin = landing.map_or(at, |on| on.at);
        let placement = Placement { origin, rotation: Radians::from(degrees), mirror, scale: 1.0 };
        let shown = level_symbol(&mut p.ids, page, style, def, starter, c, &position, placement, &mut ops)?;
        let land = match landing {
            Some(on) => Some(land_level(doc, &mut p.ids, on, &shown, placement)?),
            None => None,
        };
        if let Some(l) = &land {
            ops.extend(l.ops.clone());
        }
        // The place proposes for the strip.
        let proposed = crate::symbols::draw::plan_place(p, &ops, modes, c, page, origin, None).map_err(CmdError::refused)?;
        ops.extend(proposed.ops.iter().cloned());
        p.apply_edit(pg, "place", "Place terminal level", ops)?;
        p.last_placed = Some(ecad_electrical::commands::run::LastPlaced { starter: Some(starter.key), device: c });
        let doc = p.ledger.document();
        let first = shown.first().map(|(_, t, _)| *t);
        let name = first.map(|t| connection_point_words(doc, t)).unwrap_or_else(|| tag_text.clone());
        let what = match level {
            Some(n) if ty.multi() => format!("position {position}, level {}", tt::level_name(doc, c, ty, n)),
            _ => format!("position {position}, {}", ty.name.to_lowercase()),
        };
        let message = match (&land, ty.breaks) {
            (Some(l), true) => format!(
                "Placed {} {name} on wire {}: in {}, out {}.",
                ty.word,
                route_words(doc, l.route),
                route_words(doc, l.route),
                route_words(doc, l.new)
            ),
            (Some(l), false) => match first {
                Some(t) => land_words(doc, t, l),
                None => format!("Placed {name}."),
            },
            (None, _) => format!("Placed {name} ({what})."),
        };
        let mut v = json!({
            "component": c.hex(),
            "symbol": starter.key,
            "tag": tag_text,
            "position": position,
            "joined": strip.is_some(),
            "connection_points": shown.iter().map(|(k, t, _)| json!({"key": k, "label": ecad_electrical::terminal_strips::terminal_blocks::connection_point_name(doc, *t), "connection_point": t.hex()})).collect::<Vec<_>>(),
            "message": message,
        });
        if let Some(l) = &land {
            v["split"] = json!({"route": l.route.hex(), "new": l.new.hex()});
        }
        crate::symbols::draw::finish_place(p, pg, c, strip.is_some(), &proposed, &mut v);
        Ok(v)
    })?;
    if let Some(m) = v.get("message").and_then(Value::as_str) {
        s.echo(m.to_string());
    }
    Ok(v)
}

// ---------------------------------------------------------------- Draw Bridge

/// A wire between two terminals; between terminals of one strip it is a bridge.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct BridgeParams {
    /// The first terminal (a connection point).
    from: ConnectionPointId,
    /// The second terminal.
    to: ConnectionPointId,
}

impl Command for BridgeParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let v = on_project(s, |p, pg, page| {
            let style = plain_style(p);
            match rules::plan_bridge(p.ledger.document(), &mut p.ids, page, style, self.from, self.to)? {
                Bridge::Already { conductor, same, message } => {
                    Ok(json!({"conductor": conductor.hex(), "bridge": same, "changed": false, "message": message}))
                }
                Bridge::Drawn { ops, label, route, same, message } => {
                    p.apply_edit(pg, "bridge", &label, ops)?;
                    let conductor = ecad_electrical::wires::routes::wire_between(p.ledger.document(), self.from, self.to);
                    Ok(json!({"conductor": conductor.map(|c| c.hex()), "route": route.hex(), "bridge": same, "changed": true, "message": message}))
                }
            }
        })?;
        if let Some(m) = v.get("message").and_then(Value::as_str) {
            s.echo(m.to_string());
        }
        Ok(v)
    }
}
