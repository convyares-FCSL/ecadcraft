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
    BRIDGE_STANDOFF, Fence, FenceKind, MAX_FENCE, definition_for, land_level, level_symbol, on_wire_of, plan_fence, plan_fence_typed, staple,
};
use ecad_model::{CableId, ComponentId, ConductorId, Document, End, Noun, Op, PageId, Placement, Pt, Style, TerminalId};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::cmd::input::{Command, Point, Typed};
use crate::cmd::{err, on_any_page, on_project, plain_style};
pub use crate::connectors::strip_terminal_at;
pub use crate::connectors::{Land, plan_land};
pub use crate::strips::terminal_words;
use crate::strips::{self, StripData, TermRow};
use crate::terminal_types as tt;
use crate::wires::{ON_WIRE, OnWire};

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
    let conn = doc.connectivity();
    let wire = conn
        .net_of_conductor(land.conductor)
        .and_then(|n| conn.net(n))
        .and_then(|n| ecad_model::wiring::net_number(doc, n))
        .unwrap_or_else(|| crate::slash::wire_name(doc, land.conductor));
    let mut line = format!("Placed {} on wire {wire} (split in two).", terminal_words(doc, t));
    if let Some((k, keeper, decided)) = land.cable {
        let core = doc
            .cables
            .get(&k)
            .and_then(|x| x.members.iter().find(|m| m.conductor == keeper))
            .and_then(|m| m.core.clone())
            .map_or_else(String::new, |c| format!(" core {c}"));
        let half = if keeper == land.conductor { "first" } else { "second" };
        if !decided {
            line.push_str(&format!(
                " {}{core} stays with the {half} half; move it with Cable Cores if that is wrong.",
                crate::slash::cable_name(doc, k)
            ));
        }
    }
    line
}

/// Where a terminal starter placed at `at` lands on a wire (018's `place`), or `None` when `at` is
/// on no wire (or on a loose wire end): it is then placed as before.
pub(crate) fn landing_point(doc: &Document, page: PageId, at: Pt) -> Option<OnWire> {
    let on = crate::wires::wire_at(doc, page, at, ON_WIRE, None)?;
    let (_, _, v) = crate::wires::route_vertices(doc, on.conductor, page)?;
    let cond = doc.conductors.get(&on.conductor)?;
    for (end, p) in [(End::A, v.first()), (End::B, v.last())] {
        if p.is_some_and(|p| p.near(on.at, ON_WIRE)) && cond.end(end).is_none() {
            return None;
        }
    }
    Some(on)
}

/// The JSON fields and echo of a landed place.
pub(crate) fn land_result(doc: &Document, t: TerminalId, land: &Land) -> (Value, String) {
    let mut v = json!({"split": {"conductor": land.conductor.hex(), "new": land.new.hex()}});
    if let Some((k, keeper, _)) = land.cable {
        v["cable_kept_by"] = json!({"cable": k.hex(), "conductor": keeper.hex()});
    }
    (v, land_words(doc, t, land))
}

// ---------------------------------------------------------------- Place Terminals (§3.2)

/// The level a prompt or JSON names for type `t` on strip `c` ([`rules::level_word`]), for the
/// prompt.
///
/// # Errors
/// A word that names no level, with the type's levels.
pub fn level_word(doc: &Document, c: Option<ComponentId>, t: &tt::TermType, word: &str) -> Result<Option<u32>, CmdError> {
    rules::level_word(doc, c, t, word)
}

/// [`plan_fence`] on project `p`, with its own IDs and tag numbers (tools such as the bench, which
/// commit the plan themselves).
///
/// # Errors
/// As [`plan_fence`].
pub fn plan_fence_in(
    p: &mut crate::project::Project, page: PageId, style: Style, strip: Option<ComponentId>, pe: bool, from: Pt, to: Pt,
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
    strip: Option<ComponentId>,
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
            p.apply_edit(pg, "terminals", done.label, f.ops.clone())?;
            p.last_placed = Some((ty.starter_key(done.level), f.strip));
            let (shown, message) = rules::terminals_message(p.ledger.document(), ty, done.level, &f);
            Ok(json!({
                "strip": f.strip.hex(),
                "tag": f.tag,
                "type": ty.code,
                "terminals": f.placed.iter().zip(&shown).map(|((t, l, c, n), sl)| json!({"terminal": t.hex(), "position": l, "label": sl, "conductor": c.hex(), "new": n.hex()})).collect::<Vec<_>>(),
                "skipped": f.skipped,
                "dropped_wire_numbers": f.dropped.iter().map(|(c, n, fixed)| json!({"conductor": c.hex(), "number": n, "fixed": fixed})).collect::<Vec<_>>(),
                "message": message,
            }))
        })?;
        if let Some(m) = v.get("message").and_then(Value::as_str) {
            s.echo(m.to_string());
        }
        Ok(v)
    }
}

/// The net number of conductor `w`, else its name.
fn wire_words(doc: &Document, w: ConductorId) -> String {
    let conn = doc.connectivity();
    conn.net_of_conductor(w)
        .and_then(|n| conn.net(n))
        .and_then(|n| ecad_model::wiring::net_number(doc, n))
        .unwrap_or_else(|| crate::slash::wire_name(doc, w))
}

/// `place {symbol: <level starter>, at, rotation?, mirror?, device?, tag?, position?}` (brief 036
/// §3.3, §3.4): a level symbol (or whole terminal, disconnect or fuse terminal) joins strip `device`
/// (or the strip tagged `tag`, or a new strip) at the next position of its type that lacks its
/// level, or at `position`; on a wire it lands there. One transaction.
pub(crate) fn place_level(
    s: &mut Session, params: &crate::cmd::device::PlaceParams, starter: &'static crate::symbols::Starter, at: Pt, degrees: f64, mirror: bool,
) -> cadcraft_engine::Result<Value> {
    let (ty, level) = tt::of_starter(ecad_model::Word::word(starter.key)).ok_or_else(|| err("That symbol is not a terminal level."))?;
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
                let c: ComponentId = p.ids.fresh();
                let t = match new_tag {
                    Some(t) => t,
                    None => p.tag_high.next_auto(&p.ledger, &crate::devices::prefix_for(doc, starter))?,
                };
                let mut properties = ecad_model::Properties::new();
                properties.insert(ecad_model::tag::TAG_KEY.into(), t.clone());
                properties.insert("description".into(), "Terminal".into());
                ops.push(Op::CreateComponent { id: c, component: ecad_model::Component { properties, definition: Some(def) } });
                (c, t)
            }
        };
        let landing = landing_point(doc, page, at);
        let origin = landing.map_or(at, |on| on.at);
        let placement = Placement { origin, rotation: degrees.to_radians(), mirror, scale: 1.0 };
        let shown = level_symbol(&mut p.ids, page, style, def, starter, c, &position, placement, &mut ops)?;
        let land = match landing {
            Some(on) => Some(land_level(doc, &mut p.ids, page, on, &shown, placement)?),
            None => None,
        };
        if let Some(l) = &land {
            ops.extend(l.ops.clone());
        }
        // Brief 052d (design 052 §5.3): the place proposes for the strip.
        let proposed = crate::draw::proposals::plan_place(p, modes, c, page, origin, None).map_err(CmdError::refused)?;
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
                wire_words(doc, l.conductor),
                wire_words(doc, l.conductor),
                wire_words(doc, l.new)
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
            v["split"] = json!({"conductor": l.conductor.hex(), "new": l.new.hex()});
            if let Some((n, fixed)) = &l.dropped_number {
                v["dropped_wire_number"] = json!({"conductor": l.new.hex(), "number": n, "fixed": fixed});
                let words = format!(" The out half no longer carries {}wire number {n}; it gets its own.", if *fixed { "the fixed " } else { "" });
                if let Some(m) = v["message"].as_str() {
                    v["message"] = json!(format!("{m}{words}"));
                }
            }
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
                Bridge::Drawn { ops, label, conductor, same, message } => {
                    p.apply_edit(pg, "bridge", &label, ops)?;
                    Ok(json!({"conductor": conductor.hex(), "bridge": same, "changed": true, "message": message}))
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

/// Put `v` under `key` in `o` when it is there (absent keys are left out, never null).
fn put<T: serde::Serialize>(o: &mut serde_json::Map<String, Value>, key: &str, v: Option<T>) {
    if let Some(v) = v {
        o.insert(key.into(), json!(v));
    }
}

/// A side cell as JSON: `conductor`, and `target`, `wire` (and outside `cable`, `core`) when known.
fn side_json(c: &strips::SideCell, outside: bool) -> Value {
    let mut o = serde_json::Map::new();
    put(&mut o, "target", c.target.as_ref());
    put(&mut o, "wire", c.wire.as_ref());
    if outside {
        put(&mut o, "cable", c.cable.as_ref().map(|(_, t)| t.clone().unwrap_or_else(|| "?".into())));
        put(&mut o, "core", c.core.as_ref());
    }
    o.insert("conductor".into(), json!(c.conductor.hex()));
    Value::Object(o)
}

/// The terminal plan's fields (design 036 §9): the row model the terminal manager (2.6, the U3
/// table) and strip pages (1.8) draw. Each is `(group, key, editable, the command that writes it)`;
/// the groups mirror the strip: `inside` | `strip` | `outside`, and `either` for the row's place.
pub const PLAN_FIELDS: &[(&str, &str, bool, Option<&str>)] = &[
    ("inside", "inside[].target", false, None),
    ("inside", "inside[].wire", false, None),
    ("inside", "inside[].conductor", false, None),
    ("strip", "position", true, Some("termlabel {position}")),
    ("strip", "level", false, None),
    ("strip", "level_name", false, None),
    ("strip", "label", true, Some("termlabel {text}")),
    ("strip", "type", false, None),
    ("strip", "type_name", false, None),
    ("strip", "state", false, None),
    ("strip", "rating", true, Some("setprop rating.<position>")),
    ("strip", "links", true, Some("jumper")),
    ("strip", "internal_bridge", false, None),
    ("strip", "breaks", false, None),
    ("strip", "bridge_group", false, None),
    ("strip", "marker", true, Some("accessory")),
    ("outside", "outside[].target", false, None),
    ("outside", "outside[].wire", false, None),
    ("outside", "outside[].cable", false, None),
    ("outside", "outside[].core", false, None),
    ("outside", "outside[].conductor", false, None),
    ("either", "place", false, None),
    ("either", "terminal", false, None),
];

/// [`PLAN_FIELDS`] as JSON: `[{group, key, editable, command?}]`.
pub fn plan_fields_json() -> Value {
    Value::Array(
        PLAN_FIELDS
            .iter()
            .map(|(g, k, e, c)| {
                let mut v = json!({"group": g, "key": k, "editable": e});
                if let Some(c) = c {
                    v["command"] = json!(c);
                }
                v
            })
            .collect(),
    )
}

/// One plan row as JSON (the keys 027 exports; design 028 §10, brief 036 §9): `label` is the shown
/// label (the position for a one-level position, as before); `type` keeps 028's `feed-through` and
/// `pe` and is the type code otherwise.
pub fn row_json(r: &TermRow, style: ecad_model::ReferenceStyle) -> Value {
    let mut o = serde_json::Map::new();
    put(&mut o, "terminal", r.terminal.map(|t| t.hex()));
    put(&mut o, "out", r.out.map(|t| t.hex()));
    o.insert("label".into(), json!(r.shown));
    o.insert("position".into(), json!(r.label));
    o.insert("level".into(), json!(r.level));
    o.insert("level_name".into(), json!(r.level_name));
    let ty = match r.ty.code {
        "ft" => "feed-through",
        "pe" => "pe",
        c => c,
    };
    o.insert("type".into(), json!(ty));
    o.insert("type_name".into(), json!(r.ty.name));
    o.insert("state".into(), json!(r.state()));
    o.insert("label_overridden".into(), json!(r.label_overridden));
    o.insert("breaks".into(), json!(r.breaks()));
    put(&mut o, "rating", r.rating.as_ref());
    put(&mut o, "internal_bridge", r.internal_bridge);
    o.insert("inside".into(), Value::Array(r.inside.iter().map(|c| side_json(c, false)).collect()));
    o.insert("outside".into(), Value::Array(r.outside.iter().map(|c| side_json(c, true)).collect()));
    put(&mut o, "bridge_group", r.bridge_group);
    put(&mut o, "place", r.place.as_ref().map(|p| p.text(style)));
    let mut editable = vec!["position", "label", "links", "marker"];
    if r.breaks() {
        editable.push("rating");
    }
    o.insert("editable".into(), json!(editable));
    Value::Object(o)
}

/// A gap row as JSON (design 030 §7.2).
fn gap_json(s: &StripData, gap: &str, a: crate::accessories::Acc) -> Value {
    let mut o = serde_json::Map::new();
    o.insert("kind".into(), json!("accessory"));
    o.insert("state".into(), json!("accessory"));
    o.insert("accessory".into(), json!(a.key()));
    o.insert("gap".into(), json!(gap));
    put(&mut o, "part_number", s.acc.gap_part(a, crate::outputs::gap_pe(s, gap)));
    put(&mut o, "maker", s.acc.makers.get(a.key()));
    o.insert("terminal".into(), Value::Null);
    o.insert("label".into(), Value::Null);
    Value::Object(o)
}

/// One strip as JSON with plan rows `first..end` of it (`first_row` when it does not start at
/// 0). With `accessories` (brief 030) every row has `kind`, gap rows are in, terminal rows carry
/// `marker` and `links`, and the strip carries `accessories` and `links`; without, 028's rows.
pub fn strip_json(s: &StripData, style: ecad_model::ReferenceStyle, first: usize, end: usize, accessories: bool) -> Value {
    let mut o = serde_json::Map::new();
    o.insert("strip".into(), json!(s.component.hex()));
    o.insert("tag".into(), json!(s.tag));
    put(&mut o, "location", s.location.as_ref());
    put(&mut o, "offered", s.offer.map(|x| x.text()));
    o.insert("drawn".into(), json!(s.drawn));
    o.insert("spare".into(), json!(s.spare));
    // Brief 036: positions per type, with their BOM part.
    let types: Vec<Value> = s
        .types
        .iter()
        .map(|t| {
            let mut v = json!({"code": t.ty.code, "name": t.ty.name, "drawn": t.drawn, "spare": t.spare, "description": t.description});
            if let Some(pn) = &t.part_number {
                v["part_number"] = json!(pn);
            }
            v
        })
        .collect();
    o.insert("types".into(), Value::Array(types));
    if first > 0 {
        o.insert("first_row".into(), json!(first));
    }
    let plan = s.plan(accessories);
    let rows: Vec<Value> = plan
        .get(first..end)
        .unwrap_or_default()
        .iter()
        .map(|item| match item {
            strips::PlanItem::Gap { gap, acc } => gap_json(s, gap, *acc),
            strips::PlanItem::Term(i, r) => {
                let mut v = row_json(r, style);
                if accessories {
                    v["kind"] = json!("terminal");
                    v["marker"] = json!(s.acc.markers.contains(&r.label));
                    let links: Vec<Value> =
                        s.links_at(*i).iter().filter(|(_, on)| *on).map(|(l, _)| json!({"link": l.n, "lane": l.lane, "level": l.level})).collect();
                    v["links"] = Value::Array(links);
                }
                v
            }
        })
        .collect();
    o.insert("rows".into(), Value::Array(rows));
    if accessories {
        let counts: serde_json::Map<String, Value> =
            crate::accessories::Acc::ALL.iter().map(|a| (a.key().to_string(), json!(s.acc.count(*a)))).collect();
        o.insert("accessories".into(), Value::Object(counts));
        let links: Vec<Value> = s
            .acc
            .links
            .iter()
            .map(|l| {
                let mut v = json!({"link": l.n, "positions": l.text, "span": l.span, "lane": l.lane, "bars": l.bars, "level": l.level});
                if let Some((lo, hi)) = l.cross {
                    v["levels"] = json!([lo, hi]);
                }
                if let Some(pn) = &l.part_number {
                    v["part_number"] = json!(pn);
                }
                if let Some(m) = &l.maker {
                    v["maker"] = json!(m);
                }
                v
            })
            .collect();
        o.insert("links".into(), Value::Array(links));
    }
    Value::Object(o)
}

/// `stripplan {strip?, limit?, after?, accessories?}` (read only): the strips in tag order (or
/// one), paged by rows: `after` rows are skipped and at most `limit` returned, a strip split across
/// pages carrying `first_row`; `total` counts the rows, `next` is the next `after`.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct StripplanParams {
    /// Only this strip.
    #[serde(default)]
    strip: Option<ComponentId>,
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
            let out: Vec<Value> =
                page.parts.iter().filter_map(|(i, first, end)| shown.get(*i).map(|d| strip_json(d, style, *first, *end, accessories))).collect();
            let mut v = json!({"strips": out, "total": page.total, "fields": plan_fields_json()});
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
            let o = p.outputs.as_ref().ok_or(CmdError::refused("The reports could not be built."))?;
            let row = o.cable_row(k).cloned().ok_or(CmdError::refused("no such cable"))?;
            let wire = |w: ConductorId| o.wire(w).cloned();
            let doc = p.ledger.document();
            let style = doc.settings.reference_style();
            let o_chain = |i: usize| o.chain_ends(&row, i, style);
            let count = &row.count;
            let member_json = |i: usize, label: Option<String>, state: &str| {
                let w = row.members.get(i).copied();
                let r = w.and_then(wire);
                let mut o = serde_json::Map::new();
                put(&mut o, "label", label);
                o.insert("state".into(), json!(state));
                put(&mut o, "conductor", w.map(|w| w.hex()));
                put(&mut o, "wire", r.as_ref().and_then(|r| r.wire_number.clone()));
                // Brief 032: a core is a chain of members through continuations, From to To.
                let (from, to) = o_chain(i);
                put(&mut o, "from", from);
                put(&mut o, "to", to);
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
            put(&mut v, "tag", row.tag.as_ref());
            put(&mut v, "offer", count.offer_text.as_ref());
            put(&mut v, "source", count.source.map(|s| s.word()));
            v.insert("used".into(), json!(count.used()));
            put(&mut v, "spare", count.offer.as_ref().map(|_| count.spare.clone()));
            v.insert("cores".into(), Value::Array(cores));
            v.insert("over".into(), Value::Array(over));
            Ok(Value::Object(v))
        })
    }
}
