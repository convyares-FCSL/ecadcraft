//! The commands of brief 028 (design 028 §3, §4, §10): a terminal placed on a wire splits it
//! (through `place`), `terminals` (Place Terminals: one per wire a fence line crosses), `bridge`
//! (Draw Bridge), and the reads `stripplan` and `cablecores`.
//!
//! Every write is one transaction, committed through [`Project::apply_edit`]; the reads record
//! nothing.
//!
//! Each command is a thin shim (brief 070): its params struct reads the JSON, the rules in
//! [`ecad_electrical::cmd::terminal`] decide, and the shim commits the ops and answers in JSON.

use cadcraft_engine::{CommandSpec, Session};
use ecad_electrical::CmdError;
use ecad_electrical::cmd::terminal::{self as rules, Bridge, TerminalsInput};
pub use ecad_electrical::cmd::terminal::{
    BRIDGE_STANDOFF, Fence, FenceKind, MAX_FENCE, definition_for, land_level, level_symbol, plan_fence, plan_fence_typed, staple,
};
use ecad_model::units::{Degrees, Radians};
use ecad_model::{CableId, DeviceId, Document, Noun, Op, PageId, Placement, Pt, RouteEnd, RouteId, Style, TerminalId, WireId};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::cmd::input::{Command, Typed};
use crate::cmd::{err, on_any_page, on_project, plain_style};
pub use crate::connectors::strip_terminal_at;
pub use crate::connectors::{Land, plan_land};
use crate::strips::StripData;
pub use crate::strips::terminal_words;
use crate::terminal_types as tt;
use crate::wires::{ON_WIRE, OnRoute};
use ecad_electrical::cmd::input::Point;

/// The commands of this module with a typed input.
pub const TYPED: &[Typed] = &[
    Typed::of::<TerminalsParams>("terminals"),
    Typed::of::<BridgeParams>("bridge"),
    Typed::of::<StripplanParams>("stripplan"),
    Typed::of::<CablecoresParams>("cablecores"),
];

/// The commands this module adds.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("terminals", "Place Terminals", crate::cmd::input::typed::<TerminalsParams>)
            .params(
                "{strip?: id (null or absent: a new strip), pe?: bool, type?: Feedthrough|PE|x2|x2B|x3|LNPE|SEN|DIS|FUSE, level?: name, number \
or All, position?: label, from: [x,y], to: [x,y]} (one terminal, or one level, on each wire the line crosses; a level fills the next \
position of its type that lacks it; at most 200)",
            )
            .interactive(crate::interact::terminals)
            .noundo(),
        CommandSpec::new("bridge", "Draw Bridge", crate::cmd::input::typed::<BridgeParams>)
            .params("{from: terminal id, to: terminal id} (a wire between two terminals; of one strip it is a bridge)")
            .interactive(crate::interact::bridge)
            .noundo(),
        CommandSpec::new("stripplan", "Terminal Plan", crate::cmd::input::typed::<StripplanParams>)
            .params(
                "{strip?: id, limit?: rows (500, max 5000), after?: row index, accessories?: bool (true: gap rows, markers and links)} \
(every strip's terminals, drawn and spare; read only)",
            )
            .noundo(),
        CommandSpec::new("cablecores", "Cable Cores", crate::cmd::input::typed::<CablecoresParams>)
            .params("{cable: id} (the cable's offered cores, used, spare and over; read only)")
            .noundo(),
    ]
}

// ---------------------------------------------------------------- landing on a wire (§3.1)

/// The echo of a landed terminal.
fn land_words(doc: &Document, t: TerminalId, land: &Land) -> String {
    format!("Placed {} on wire {} (split in two).", terminal_words(doc, t), route_words(doc, land.route))
}

/// Where a terminal starter placed at `at` lands on a route (018's `place`), or `None` when `at`
/// is on no route (or on a route's free end): it is then placed as before.
pub(crate) fn landing_point(doc: &Document, page: PageId, at: Pt) -> Option<OnRoute> {
    let on = crate::wires::route_at(doc, page, at, ON_WIRE, None)?;
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
pub(crate) fn land_result(doc: &Document, t: TerminalId, land: &Land) -> (Value, String) {
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
pub(crate) struct TerminalsParams {
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

impl Command for TerminalsParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let ty = rules::term_type(self.ty.as_deref(), self.pe.unwrap_or(false)).map_err(|e| err(e.to_string()))?;
        let (from, to) = (self.from.0, self.to.0);
        let bad = |q: Pt| !(q.x.abs() <= crate::interact::MAX_COORD && q.y.abs() <= crate::interact::MAX_COORD);
        if bad(from) || bad(to) {
            return Err(err(crate::interact::OUT_OF_RANGE));
        }
        let input = TerminalsInput { strip: self.strip, ty, level: self.level, position: self.position, from, to };
        let v = on_project(s, |p, pg, page| {
            let style = plain_style(p);
            let tol = p.tolerance;
            let done = rules::plan_terminals(&p.ledger, &mut p.ids, &mut p.tag_high, page, style, input, tol)?;
            let f = done.fence;
            // The wire numbers on the crossed lines before: a half that ends up in another net (a
            // fuse position) does not keep its number (design 072 §4.4), which is said.
            let before: Vec<Option<ecad_model::wire::WireNumber>> = f
                .placed
                .iter()
                .map(|(_, _, r, _)| {
                    let doc = p.ledger.document();
                    crate::wires::wire_of_route(doc, *r).and_then(|w| doc.wires.get(&w)).and_then(|w| w.number.clone())
                })
                .collect();
            p.apply_edit(pg, "terminals", done.label, f.ops.clone())?;
            p.last_placed = Some((ty.starter_key(done.level).to_string(), f.strip));
            let doc = p.ledger.document();
            let dropped = dropped_numbers(doc, &f.placed, &before);
            let (shown, message) = rules::terminals_message(doc, ty, done.level, &f);
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
                "terminals": f.placed.iter().zip(&shown).map(|((t, l, r, n), sl)| json!({"terminal": t.hex(), "position": l, "label": sl, "route": r.hex(), "new": n.hex()})).collect::<Vec<_>>(),
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
    doc: &Document, placed: &[(TerminalId, String, RouteId, RouteId)], before: &[Option<ecad_model::wire::WireNumber>],
) -> Vec<(WireId, ecad_model::wire::WireNumber)> {
    let mut out = Vec::new();
    for ((_, _, r, n), was) in placed.iter().zip(before) {
        let Some(was) = was else { continue };
        for half in [*r, *n] {
            let Some(w) = crate::wires::wire_of_route(doc, half) else { continue };
            if doc.wires.get(&w).and_then(|x| x.number.as_ref()).map(|x| &x.text) != Some(&was.text) {
                out.push((w, was.clone()));
            }
        }
    }
    out
}

/// The net number of conductor `w`, else its name.
fn wire_words(doc: &Document, w: WireId) -> String {
    ecad_model::wiring::wire_number(doc, w).unwrap_or_else(|| crate::slash::wire_name(doc, w))
}

/// The words for the wire route `r` carries ([`crate::wires::wire_of_route`]), `?` for none.
fn route_words(doc: &Document, r: RouteId) -> String {
    crate::wires::wire_of_route(doc, r).map_or_else(|| "?".to_string(), |w| wire_words(doc, w))
}

/// `place {symbol: <level starter>, at, rotation?, mirror?, device?, tag?, position?}` (brief 036
/// §3.3, §3.4): a level symbol (or whole terminal, disconnect or fuse terminal) joins strip `device`
/// (or the strip tagged `tag`, or a new strip) at the next position of its type that lacks its
/// level, or at `position`; on a wire it lands there. One transaction.
pub(crate) fn place_level(
    s: &mut Session, params: &crate::cmd::device::PlaceParams, starter: &'static crate::symbols::Starter, at: Pt, degrees: Degrees, mirror: bool,
) -> cadcraft_engine::Result<Value> {
    let (ty, level) = tt::of_starter(starter.key).ok_or_else(|| err("That symbol is not a terminal level."))?;
    let device = params.device_id()?;
    let tag = params.tag.clone();
    let position = params.position.clone();
    ecad_electrical::cmd::place::one_of(device.is_some(), tag.is_some(), "strip").map_err(|e| err(e.to_string()))?;
    let modes = params.modes()?;
    let v = on_project(s, |p, pg, page| {
        let style = plain_style(p);
        let doc = p.ledger.document();
        let (strip, new_tag) = ecad_electrical::cmd::place::strip_target(doc, device, tag.as_deref())?;
        let mut ops = Vec::new();
        let def = definition_for(doc, &mut p.ids, starter, &mut ops)?;
        let mut chooser = tt::Chooser::new(doc, strip);
        let position = chooser.choose(doc, strip, ty, level, position.as_deref())?;
        let (c, tag_text) = match strip {
            Some(c) => (c, crate::devices::tag_words(doc, c)),
            None => {
                let c: DeviceId = p.ids.fresh();
                let t = match new_tag {
                    Some(t) => t,
                    None => p.tag_high.next_auto(&p.ledger, &crate::devices::prefix_for(doc, starter))?,
                };
                ops.push(Op::CreateDevice {
                    id: c,
                    device: ecad_model::Device {
                        tag: Some(crate::devices::stored_tag(&t)?),
                        description: crate::devices::description_of("Terminal"),
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
        let proposed = crate::draw::proposals::plan_place(p, &ops, modes, c, page, origin, None).map_err(CmdError::refused)?;
        ops.extend(proposed.ops.iter().cloned());
        p.apply_edit(pg, "place", "Place terminal level", ops)?;
        p.last_placed = Some((starter.key.to_string(), c));
        let doc = p.ledger.document();
        let first = shown.first().map(|(_, t, _)| *t);
        let name = first.map(|t| terminal_words(doc, t)).unwrap_or_else(|| tag_text.clone());
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
            "terminals": shown.iter().map(|(k, t, _)| json!({"key": k, "label": crate::terminal_types::terminal_name(doc, *t), "terminal": t.hex()})).collect::<Vec<_>>(),
            "message": message,
        });
        if let Some(l) = &land {
            v["split"] = json!({"route": l.route.hex(), "new": l.new.hex()});
        }
        crate::draw::proposals::finish_place(p, pg, c, strip.is_some(), &proposed, &mut v);
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
    from: TerminalId,
    /// The second terminal.
    to: TerminalId,
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
                    let conductor = crate::wires::wire_between(p.ledger.document(), self.from, self.to);
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

// ---------------------------------------------------------------- reads (§10)

/// `stripplan {strip?, limit?, after?, accessories?}` (read only): the strips in tag order (or
/// one), paged by rows: `after` rows are skipped and at most `limit` returned, a strip split across
/// pages carrying `first_row`; `total` counts the rows, `next` is the next `after`.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct StripplanParams {
    /// Only this strip.
    #[serde(default)]
    strip: Option<DeviceId>,
    /// At most this many rows (500, at most 5000).
    #[serde(default)]
    limit: Option<u64>,
    /// Skip this many rows (the `next` of the last call).
    #[serde(default)]
    after: Option<u64>,
    /// Gap rows, markers and links (default true).
    #[serde(default)]
    accessories: Option<bool>,
}

impl Command for StripplanParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let limit =
            self.limit.map_or(crate::outputs::DEFAULT_LIMIT, |n| usize::try_from(n).unwrap_or(usize::MAX)).clamp(1, crate::outputs::MAX_LIMIT);
        let after = self.after.map_or(0, |n| usize::try_from(n).unwrap_or(usize::MAX));
        let accessories = self.accessories.unwrap_or(true);
        let strip = self.strip;
        on_any_page(s, |p, _| {
            let style = p.ledger.document().settings.reference_style();
            let o = p.outputs();
            let shown: Vec<&StripData> = match strip {
                Some(c) => vec![o.strips.get(c).ok_or(CmdError::NotFound(Noun::TerminalStrip))?],
                None => o.sorted_strips(),
            };
            let lens: Vec<usize> = shown.iter().map(|d| if accessories { d.rows.len() + d.acc.gap_rows() } else { d.rows.len() }).collect();
            let page = rules::plan_page(&lens, after, limit);
            let out: Vec<Value> = page
                .parts
                .iter()
                .filter_map(|(i, first, end)| shown.get(*i).map(|d| rules::strip_json(d, style, *first, *end, accessories)))
                .collect();
            let mut v = json!({"strips": out, "total": page.total, "fields": rules::plan_fields_json()});
            if let Some(n) = page.next {
                v["next"] = json!(n);
            }
            Ok(v)
        })
    }
}

/// `cablecores {cable}` (read only): the cable's offered cores, used, spare and over.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CablecoresParams {
    /// The cable.
    cable: CableId,
}

impl Command for CablecoresParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        let k = self.cable;
        on_any_page(s, |p, _| {
            p.outputs();
            let o = p.kept_outputs().ok_or(CmdError::refused("The reports could not be built."))?;
            let row = o.cable_row(k).cloned().ok_or(CmdError::refused("no such cable"))?;
            let wire = |w: WireId| o.wire(w).cloned();
            let doc = p.ledger.document();
            let style = doc.settings.reference_style();
            let o_chain = |i: usize| o.chain_ends(&row, i, style);
            let count = &row.count;
            let member_json = |i: usize, label: Option<String>, state: &str| {
                let w = row.members.get(i).copied();
                let r = w.and_then(wire);
                let mut o = serde_json::Map::new();
                rules::put(&mut o, "label", label);
                o.insert("state".into(), json!(state));
                rules::put(&mut o, "conductor", w.map(|w| w.hex()));
                rules::put(&mut o, "wire", r.as_ref().and_then(|r| r.wire_number.clone()));
                // Brief 032: a core is a chain of members through continuations, From to To.
                let (from, to) = o_chain(i);
                rules::put(&mut o, "from", from);
                rules::put(&mut o, "to", to);
                Value::Object(o)
            };
            // Brief 032: the rows are the cable's core chains, each with its stored label.
            let stored = |i: usize| row.member_cores.get(i).cloned().flatten();
            let mut cores = Vec::new();
            match &count.offer {
                Some(offer) => {
                    for label in offer {
                        match count.matched.iter().position(|m| m.as_deref() == Some(label.as_str())) {
                            Some(i) => cores.push(member_json(i, Some(label.clone()), "used")),
                            // A spare core has no ends (review of PR #55, finding 3).
                            None => cores.push(json!({"label": label, "state": "spare"})),
                        }
                    }
                }
                None => {
                    for i in 0..row.members.len() {
                        cores.push(member_json(i, stored(i), "used"));
                    }
                }
            }
            let over: Vec<Value> = count.over.iter().map(|i| member_json(*i, stored(*i), "over")).collect();
            let mut v = serde_json::Map::new();
            v.insert("cable".into(), json!(k.hex()));
            rules::put(&mut v, "tag", row.tag.as_ref());
            rules::put(&mut v, "offer", count.offer_text.as_ref());
            rules::put(&mut v, "source", count.source.map(|s| s.word()));
            v.insert("used".into(), json!(count.used()));
            rules::put(&mut v, "spare", count.offer.as_ref().map(|_| count.spare.clone()));
            v.insert("cores".into(), Value::Array(cores));
            v.insert("over".into(), Value::Array(over));
            Ok(Value::Object(v))
        })
    }
}
