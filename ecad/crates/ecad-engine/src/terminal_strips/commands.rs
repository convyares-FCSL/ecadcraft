//! The commands of brief 028 (design 028 §3, §4, §10): a terminal placed on a wire splits it
//! (through `place`), `connection_points` (Place Terminals: one per wire a fence line crosses), `bridge`
//! (Draw Bridge). The reads `stripplan` and `cablecores` are generic
//! ([`ecad_electrical::terminal_strips::commands::reads`]).
//!
//! Every write is one transaction, committed through [`Project::apply_edit`]; the reads record
//! nothing.
//!
//! Each command is a thin shim (brief 070): its params struct reads the JSON, the rules in
//! [`ecad_electrical::terminal_strips::commands::terminal_strips`] decide, and the shim commits the ops and answers in JSON.

use cadcraft_engine::{CommandSpec, Session};
use ecad_electrical::CmdError;
use ecad_electrical::terminal_strips::commands::terminal_strips::{self as rules, Bridge, TerminalBlocksInput};
pub use ecad_electrical::terminal_strips::commands::terminal_strips::{
    BRIDGE_STANDOFF, Fence, FenceKind, MAX_FENCE, definition_for, land_level, level_symbol, plan_fence, plan_fence_typed, staple,
};
use ecad_model::geometry::units::{Degrees, Radians};
use ecad_model::{ConnectionPointId, DeviceId, Document, Op, PageId, Placement, Pt, RouteEnd, RouteId, Style, WireId};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::commands::input::{Command, Typed};
use crate::commands::{err, on_project, plain_style};
use ecad_electrical::commands::input::Point;
pub use ecad_electrical::connectors::strip_connection_point_at;
pub use ecad_electrical::connectors::{Land, plan_land};
pub use ecad_electrical::terminal_strips::connection_point_words;
use ecad_electrical::terminal_strips::terminal_blocks as tt;
use {ecad_electrical::wires::ON_WIRE, ecad_electrical::wires::OnRoute};

/// The commands of this module with a typed input.
pub const TYPED: &[Typed] = &[Typed::of::<TerminalBlocksParams>("terminalblocks"), Typed::of::<BridgeParams>("bridge")];

/// The commands this module adds.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("terminalblocks", "Place Terminals", crate::commands::input::typed::<TerminalBlocksParams>)
            .params(
                "{strip?: id (null or absent: a new strip), pe?: bool, type?: Feedthrough|PE|x2|x2B|x3|LNPE|SEN|DIS|FUSE, level?: name, number \
or All, position?: label, from: [x,y], to: [x,y]} (one terminal, or one level, on each wire the line crosses; a level fills the next \
position of its type that lacks it; at most 200)",
            )
            .interactive(crate::interact::terminal_blocks)
            .noundo(),
        CommandSpec::new("bridge", "Draw Bridge", crate::commands::input::typed::<BridgeParams>)
            .params("{from: terminal id, to: terminal id} (a wire between two terminals; of one strip it is a bridge)")
            .interactive(crate::interact::bridge)
            .noundo(),
    ]
}

// ---------------------------------------------------------------- landing on a wire (§3.1)

/// The echo of a landed terminal.
fn land_words(doc: &Document, t: ConnectionPointId, land: &Land) -> String {
    format!("Placed {} on wire {} (split in two).", connection_point_words(doc, t), route_words(doc, land.route))
}

/// Where a terminal starter placed at `at` lands on a route (018's `place`), or `None` when `at`
/// is on no route (or on a route's free end): it is then placed as before.
pub(crate) fn landing_point(doc: &Document, page: PageId, at: Pt) -> Option<OnRoute> {
    let on = ecad_electrical::wires::route_at(doc, page, at, ON_WIRE, None)?;
    let route = doc.routes.get(&on.route)?;
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

// ---------------------------------------------------------------- Place Terminals (§3.2)

/// The level a prompt or JSON names for type `t` on strip `c` ([`rules::level_word`]), for the
/// prompt.
///
/// # Errors
/// A word that names no level, with the type's levels.
pub fn level_word(doc: &Document, c: Option<DeviceId>, t: &tt::TermType, word: &str) -> Result<Option<u32>, CmdError> {
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

/// `terminals {strip?, pe?, type?, level?, position?, from, to}` (Place Terminals, design 028
/// §3.2, brief 036 §3.2): one terminal, or one level, on each wire the line crosses.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct TerminalBlocksParams {
    /// The strip to join (null or absent: a new strip).
    #[serde(default)]
    strip: Option<DeviceId>,
    /// PE terminals (without `type`).
    #[serde(default)]
    pe: Option<bool>,
    /// The terminal type: Feedthrough, PE, x2, x2B, x3, LNPE, SEN, DIS or FUSE.
    #[serde(default, rename = "type")]
    ty: Option<String>,
    /// The level: a level name, a level number, or All.
    #[serde(default)]
    level: Option<String>,
    /// The position label for the first crossing.
    #[serde(default)]
    position: Option<String>,
    /// One end of the line.
    from: Point,
    /// The other end of the line.
    to: Point,
}

impl Command for TerminalBlocksParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let ty = rules::term_type(self.ty.as_deref(), self.pe.unwrap_or(false)).map_err(|e| err(e.to_string()))?;
        let (from, to) = (self.from.0, self.to.0);
        let bad = |q: Pt| !(q.x.abs() <= crate::interact::MAX_COORD && q.y.abs() <= crate::interact::MAX_COORD);
        if bad(from) || bad(to) {
            return Err(err(crate::interact::OUT_OF_RANGE));
        }
        let input = TerminalBlocksInput { strip: self.strip, ty, level: self.level, position: self.position, from, to };
        let v = on_project(s, |p, pg, page| {
            let style = plain_style(p);
            let tol = p.tolerance;
            let done = rules::plan_terminal_blocks(&p.ledger, &mut p.ids, &mut p.tag_high, page, style, input, tol)?;
            let f = done.fence;
            // The wire numbers on the crossed lines before: a half that ends up in another net (a
            // fuse position) does not keep its number (design 072 §4.4), which is said.
            let before: Vec<Option<ecad_model::objects::wire::WireNumber>> = f
                .placed
                .iter()
                .map(|(_, _, r, _)| {
                    let doc = p.ledger.document();
                    ecad_electrical::wires::wire_of_route(doc, *r).and_then(|w| doc.wires.get(&w)).and_then(|w| w.number.clone())
                })
                .collect();
            p.apply_edit(pg, "terminalblocks", done.label, f.ops.clone())?;
            p.last_placed = Some((ty.starter_key(done.level).to_string(), f.strip));
            let doc = p.ledger.document();
            let dropped = dropped_numbers(doc, &f.placed, &before);
            let (shown, message) = rules::connection_points_message(doc, ty, done.level, &f);
            let message = if dropped.is_empty() {
                message
            } else {
                let list: Vec<&str> = dropped.iter().map(|(_, n)| n.text.as_str()).collect();
                format!(
                    "{message} Out halves no longer carry wire number{} {}; they get their own.",
                    if list.len() == 1 { "" } else { "s" },
                    list.join(", ")
                )
            };
            Ok(json!({
                "strip": f.strip.hex(),
                "tag": f.tag,
                "type": ty.code,
                "connection_points": f.placed.iter().zip(&shown).map(|((t, l, r, n), sl)| json!({"connection_point": t.hex(), "position": l, "label": sl, "route": r.hex(), "new": n.hex()})).collect::<Vec<_>>(),
                "skipped": f.skipped,
                "dropped_wire_numbers": dropped.iter().map(|(w, n)| json!({"conductor": w.hex(), "number": n.text.as_str(), "fixed": n.fixed})).collect::<Vec<_>>(),
                "message": message,
            }))
        })?;
        if let Some(m) = v.get("message").and_then(Value::as_str) {
            s.echo(m.to_string());
        }
        Ok(v)
    }
}

/// The halves of crossed lines that no longer carry the number their line's wire carried before
/// (`before`, by placed terminal): each half's wire and the number it lost.
fn dropped_numbers(
    doc: &Document, placed: &[(ConnectionPointId, String, RouteId, RouteId)], before: &[Option<ecad_model::objects::wire::WireNumber>],
) -> Vec<(WireId, ecad_model::objects::wire::WireNumber)> {
    let mut out = Vec::new();
    for ((_, _, r, n), was) in placed.iter().zip(before) {
        let Some(was) = was else { continue };
        for half in [*r, *n] {
            let Some(w) = ecad_electrical::wires::wire_of_route(doc, half) else { continue };
            if doc.wires.get(&w).and_then(|x| x.number.as_ref()).map(|x| &x.text) != Some(&was.text) {
                out.push((w, was.clone()));
            }
        }
    }
    out
}

/// The net number of conductor `w`, else its name.
fn wire_words(doc: &Document, w: WireId) -> String {
    ecad_model::connectivity::wiring::wire_number(doc, w).unwrap_or_else(|| ecad_electrical::cables::slash::wire_name(doc, w))
}

/// The words for the wire route `r` carries ([`ecad_electrical::wires::wire_of_route`]), `?` for none.
fn route_words(doc: &Document, r: RouteId) -> String {
    ecad_electrical::wires::wire_of_route(doc, r).map_or_else(|| "?".to_string(), |w| wire_words(doc, w))
}

/// `place {symbol: <level starter>, at, rotation?, mirror?, device?, tag?, position?}` (brief 036
/// §3.3, §3.4): a level symbol (or whole terminal, disconnect or fuse terminal) joins strip `device`
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
            Some(c) => (c, ecad_electrical::devices::tag_words(doc, c)),
            None => {
                let c: DeviceId = p.ids.fresh();
                let t = match new_tag {
                    Some(t) => t,
                    None => p.tag_high.next_auto(&p.ledger, &ecad_electrical::devices::prefix_for(doc, starter))?,
                };
                ops.push(Op::CreateDevice {
                    id: c,
                    device: ecad_model::Device {
                        tag: Some(ecad_electrical::devices::stored_tag(&t)?),
                        description: ecad_electrical::devices::description_of("Terminal"),
                        definition: Some(def),
                        device_type: starter.device_type(),
                        ..ecad_model::Device::default()
                    },
                });
                (c, t)
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
        // Brief 052d (design 052 §5.3): the place proposes for the strip.
        let proposed = crate::symbols::draw::plan_place(p, &ops, modes, c, page, origin, None).map_err(CmdError::refused)?;
        ops.extend(proposed.ops.iter().cloned());
        p.apply_edit(pg, "place", "Place terminal level", ops)?;
        p.last_placed = Some((starter.key.to_string(), c));
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

// ---------------------------------------------------------------- Draw Bridge (§4)

/// `bridge {from, to}` (Draw Bridge, design 028 §4): a wire between two terminals; of one strip it
/// is a bridge.
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
                    let conductor = ecad_electrical::wires::wire_between(p.ledger.document(), self.from, self.to);
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
