//! The commands of brief 044 (design 044 §4, §5.3): `connector` (Place ▸ Connector) and `pinlabel`,
//! and `place` for a connector half or pin (Find Symbol).
//!
//! Every write is one transaction, committed through [`Project::apply_edit`]; nothing here opens a
//! dialog. The interactive form of `connector` ([`crate::interact::connector`]) commits through
//! this JSON form.

use std::collections::BTreeSet;

use cadcraft_engine::{CommandSpec, Session};
use ecad_model::{ComponentId, ConductorId, Document, Op, PageId, Placement, Pt, Style, Terminal, TerminalId};
use serde_json::{Value, json};

use crate::commands::{err, opt_id, opt_typed, plain_style, point, with_project};
use crate::connectors::{self as cn, Family, Names, Shown};
use crate::defaults::{self, Scope};
use crate::project::Project;
use crate::symbols::{Side, Starter};

/// The commands this module adds (registered in `lanes.rs`).
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("connector", "Connector", run_connector)
            .params(
                "{device?: id | tag (absent: a new connector), halves?: socket|plug|both, family?: plain|dsub|m12|rj45|usba|usbb, \
pins?: 1..64, names?: numbers|letters|[\"L\",\"N\",\"PE\"], layout?: row|pins, at?: [x,y], rotation?: 0|90|180|270, swap?: bool (row), \
line?: [[x,y],[x,y]] | wire?: id (pins), pin?: name (the first pin), page?: id} (design 044 §4.3; absent fields take the registry's \
connector.maker.* answers; one transaction)",
            )
            .interactive(crate::interact::connector)
            .noundo(),
        CommandSpec::new("pinlabel", "Rename Pin", run_pinlabel)
            .params(
                "{device?: id | tag, pin?: name, terminal?: id, to: new name | \"next\"} (renames a connector pin on both halves in one \
transaction; with terminal and to \"next\", moves that one drawn pin to its half's next free pin)",
            )
            .noundo(),
    ]
}

// ---------------------------------------------------------------- reading parameters

/// The connector `device` names: a hex ID or a tag.
fn device_param(doc: &Document, params: &Value) -> Result<Option<ComponentId>, String> {
    match params.get("device") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(t)) => {
            let t = t.trim();
            if let Ok(n) = u128::from_str_radix(t, 16)
                && doc.components.contains_key(&ComponentId::from(n))
            {
                return Ok(Some(ComponentId::from(n)));
            }
            let tag = ecad_model::tag::normalise(t, crate::defaults::print::tag_style(doc))?;
            crate::devices::holder_of(doc, &tag, None).map(Some).ok_or_else(|| format!("No device {tag}."))
        }
        Some(_) => Err("`device` is a connector's id or tag".into()),
    }
}

/// The page `page` names, else the active page.
fn page_param(doc: &Document, params: &Value, active: PageId) -> Result<PageId, String> {
    match opt_id::<PageId>(params, "page")? {
        Some(p) if doc.pages.contains_key(&p) => Ok(p),
        Some(_) => Err("No such page.".into()),
        None => Ok(active),
    }
}

fn names_param(doc: &Document, params: &Value) -> Result<Names, String> {
    match params.get("names") {
        None | Some(Value::Null) => match defaults::resolve(doc, &defaults::connectors::MAKER_NAMES, Scope::project()).word() {
            "letters" => Ok(Names::Letters),
            "list" => Err("Give the pin names as a list: names [\"L\",\"N\",\"PE\"].".into()),
            _ => Ok(Names::Numbers),
        },
        Some(Value::String(w)) => match w.trim().to_ascii_lowercase().as_str() {
            "numbers" => Ok(Names::Numbers),
            "letters" => Ok(Names::Letters),
            _ => cn::parse_list(w).map(Names::List),
        },
        Some(Value::Array(a)) => {
            if a.len() > cn::MAX_PINS {
                return Err(format!("A connector has at most {} pins; got {}.", cn::MAX_PINS, a.len()));
            }
            let v: Vec<String> =
                a.iter().map(|x| x.as_str().map(|s| s.trim().to_string()).ok_or("each pin name must be a string")).collect::<Result<_, _>>()?;
            cn::check_list(&v)?;
            Ok(Names::List(v))
        }
        Some(_) => Err("`names` is numbers, letters or a list of pin names".into()),
    }
}

fn halves_param(doc: &Document, params: &Value) -> Result<Side, String> {
    let w = match params.get("halves") {
        None | Some(Value::Null) => defaults::resolve(doc, &defaults::connectors::MAKER_HALVES, Scope::project()).word().to_string(),
        Some(Value::String(w)) => w.trim().to_ascii_lowercase(),
        Some(_) => return Err("`halves` is socket, plug or both".into()),
    };
    match w.as_str() {
        "socket" | "s" => Ok(Side::Socket),
        "plug" | "p" => Ok(Side::Plug),
        "both" | "b" => Ok(Side::Both),
        _ => Err("`halves` is socket, plug or both".into()),
    }
}

fn family_param(doc: &Document, params: &Value) -> Result<Family, String> {
    match params.get("family") {
        None | Some(Value::Null) => {
            Ok(Family::parse(defaults::resolve(doc, &defaults::connectors::MAKER_FAMILY, Scope::project()).word()).unwrap_or(Family::Plain))
        }
        Some(Value::String(w)) => Family::parse(w).ok_or_else(|| "`family` is plain, dsub, m12, rj45, usba or usbb".to_string()),
        Some(_) => Err("`family` is plain, dsub, m12, rj45, usba or usbb".into()),
    }
}

fn pins_param(params: &Value) -> Result<Option<usize>, String> {
    match params.get("pins") {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .filter(|n| (1..=cn::MAX_PINS).contains(n))
            .map(Some)
            .ok_or_else(|| format!("`pins` is a whole number from 1 to {}.", cn::MAX_PINS)),
    }
}

fn layout_param(doc: &Document, params: &Value) -> Result<bool, String> {
    let w = match params.get("layout") {
        None | Some(Value::Null) => defaults::resolve(doc, &defaults::connectors::MAKER_LAYOUT, Scope::project()).word().to_string(),
        Some(Value::String(w)) => w.trim().to_ascii_lowercase(),
        Some(_) => return Err("`layout` is row or pins".into()),
    };
    match w.as_str() {
        "row" => Ok(false),
        "pins" => Ok(true),
        _ => Err("`layout` is row or pins".into()),
    }
}

/// The registry's first pin count.
fn maker_pins(doc: &Document) -> usize {
    let n = defaults::resolve(doc, &defaults::connectors::MAKER_PINS, Scope::project()).number();
    if n.is_finite() && n >= 1.0 && n <= cn::MAX_PINS as f64 { n as usize } else { 4 }
}

fn out_of_range(p: Pt) -> bool {
    !(p.is_finite() && p.x.abs() <= crate::interact::MAX_COORD && p.y.abs() <= crate::interact::MAX_COORD)
}

/// `1–4` or `3`.
fn span(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [a, .., b] => format!("{a}\u{2013}{b}"),
    }
}

// ---------------------------------------------------------------- connector (§4)

/// What a placement made, for the result and the echo.
struct Made {
    component: ComponentId,
    tag: String,
    new: bool,
    shown: Vec<Shown>,
    splits: Vec<(ConductorId, ConductorId)>,
    dropped: Vec<(ConductorId, String)>,
    /// The transaction's ops.
    ops: Vec<Op>,
}

fn result_json(doc: &Document, m: &Made, message: &str) -> Value {
    json!({
        "component": m.component.hex(),
        "tag": m.tag,
        "new": m.new,
        "terminals": m.shown.iter().map(|x| json!({
            "terminal": x.terminal.hex(),
            "side": x.side.word(),
            "key": x.side.key(),
            "label": x.name,
            "pin": x.name,
            "name": cn::pin_name(doc, x.terminal).unwrap_or_else(|| format!("{}:{}", m.tag, x.name)),
        })).collect::<Vec<_>>(),
        "split": m.splits.iter().map(|(c, n)| json!({"conductor": c.hex(), "new": n.hex()})).collect::<Vec<_>>(),
        "dropped_wire_numbers": m.dropped.iter().map(|(c, n)| json!({"conductor": c.hex(), "number": n})).collect::<Vec<_>>(),
        "echo": message,
        "message": message,
    })
}

/// `connector {…}` (design 044 §4.3).
fn run_connector(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let at = match params.get("at") {
        None | Some(Value::Null) => None,
        Some(v) => Some(point(v).ok_or_else(|| err("`at` is [x, y]"))?),
    };
    let line = match params.get("line") {
        None | Some(Value::Null) => None,
        Some(v) => {
            let a = v.as_array().ok_or_else(|| err("`line` is [[x, y], [x, y]]"))?;
            match (a.first().and_then(point), a.get(1).and_then(point), a.len()) {
                (Some(p), Some(q), 2) => Some((p, q)),
                _ => return Err(err("`line` is [[x, y], [x, y]]")),
            }
        }
    };
    let wire: Option<ConductorId> = opt_id(params, "wire").map_err(err)?;
    let degrees = opt_typed(params, "rotation", "0, 90, 180 or 270", Value::as_f64).map_err(err)?.unwrap_or(0.0);
    if ![0.0, 90.0, 180.0, 270.0].contains(&degrees) {
        return Err(err("`rotation` is 0, 90, 180 or 270"));
    }
    let swap = opt_typed(params, "swap", "true or false", Value::as_bool).map_err(err)?.unwrap_or(false);
    let pin = opt_typed(params, "pin", "a pin name", Value::as_str).map_err(err)?.map(|p| p.trim().to_string());
    for p in at.iter().chain(line.iter().flat_map(|(a, b)| [a, b])) {
        if out_of_range(*p) {
            return Err(err(crate::interact::OUT_OF_RANGE));
        }
    }
    let v = with_project(s, |p, pg, active| {
        let doc = p.ledger.document();
        let page = page_param(doc, params, active)?;
        let device = device_param(doc, params)?;
        if let Some(c) = device
            && !cn::is_connector(doc, c)
        {
            return Err(cn::not_a_connector(doc, c));
        }
        let halves = halves_param(doc, params)?;
        let explicit_family = params.get("family").is_some_and(|v| !v.is_null());
        let family = match device {
            Some(c) if !explicit_family => cn::half_family(doc, c, Side::Socket),
            _ => family_param(doc, params)?,
        };
        let pins_given = pins_param(params)?;
        let names = names_param(doc, params)?;
        let pins_layout = layout_param(doc, params)?;
        let style = plain_style(p);
        let (made, label) = if let Some(key) = family.bulkhead() {
            let at = at.ok_or("connector needs at for a row, or line or wire for pins.")?;
            (place_bulkhead(p, page, style, key, at, degrees)?, "Place connector".to_string())
        } else if pins_layout {
            let (from, to) = match (line, wire) {
                (Some(l), _) => l,
                (None, Some(w)) => {
                    let mid = wire_point(p.ledger.document(), page, w).ok_or("That wire is not drawn on this page.")?;
                    (mid, mid)
                }
                (None, None) => return Err("connector needs at for a row, or line or wire for pins.".into()),
            };
            let made = place_pins(p, page, style, device, halves, pins_given, &names, pin.as_deref(), from, to, wire)?;
            let names: Vec<String> = made.shown.iter().filter(|x| x.side != Side::Plug || halves == Side::Plug).map(|x| x.name.clone()).collect();
            let label = format!("Place pins {}:{}", made.tag, span(&names));
            (made, label)
        } else {
            let at = at.ok_or("connector needs at for a row, or line or wire for pins.")?;
            let made = place_row(p, page, style, device, halves, family, pins_given, &names, at, degrees, swap)?;
            let label = format!("Place connector {}", made.tag);
            (made, label)
        };
        // One transaction.
        p.apply_edit(pg, "connector", &label, made.ops.clone())?;
        p.last_placed = Some(("connector".to_string(), made.component));
        let doc = p.ledger.document();
        let message = echo(doc, &made, family, halves, pins_layout);
        Ok(result_json(doc, &made, &message))
    })?;
    if let Some(m) = v.get("message").and_then(Value::as_str) {
        s.echo(m.to_string());
    }
    Ok(v)
}

/// The echo after a commit (design 044 §4.2).
fn echo(doc: &Document, m: &Made, family: Family, halves: Side, pins_layout: bool) -> String {
    let side_names = |side: Side| -> Vec<String> { m.shown.iter().filter(|x| x.side == side).map(|x| x.name.clone()).collect() };
    let mut parts = Vec::new();
    for side in [Side::Socket, Side::Plug] {
        let names = side_names(side);
        if !names.is_empty() {
            parts.push(format!("{}:{}", cn::half_tag(doc, m.component, side), span(&names)));
        }
    }
    let mut out = if pins_layout {
        let n = m.splits.len();
        let what = match halves {
            Side::Both => "pin pairs",
            Side::Socket => "socket pins",
            Side::Plug => "plug pins",
        };
        let names: Vec<String> = if halves == Side::Plug { side_names(Side::Plug) } else { side_names(Side::Socket) };
        if n > 0 {
            format!("Placed {}:{} on {n} wire{} ({what}).", m.tag, span(&names), if n == 1 { "" } else { "s" })
        } else {
            format!("Placed {}:{} ({what}).", m.tag, span(&names))
        }
    } else if family.bulkhead().is_some() {
        format!("Placed {} ({} bulkhead).", m.tag, family.name())
    } else {
        let n = cn::pin_count(doc, m.component, &cn::drawn(doc, m.component));
        format!("Placed {} ({}, {n} pins): {}.", m.tag, family.name(), parts.join(", "))
    };
    if !m.dropped.is_empty() {
        let list: Vec<&str> = m.dropped.iter().map(|(_, n)| n.as_str()).collect();
        out.push_str(&format!(
            " The open parts of the cut wires no longer carry wire number{} {}.",
            if list.len() == 1 { "" } else { "s" },
            list.join(", ")
        ));
    }
    out
}

/// A point on wire `w`'s route on `page`: the middle of its longest segment.
fn wire_point(doc: &Document, page: PageId, w: ConductorId) -> Option<Pt> {
    let (_, _, v) = crate::wires::route_vertices(doc, w, page)?;
    let mut best: Option<(f64, Pt)> = None;
    for pair in v.windows(2) {
        let (Some(a), Some(b)) = (pair.first(), pair.get(1)) else { continue };
        let d = a.dist(*b);
        if d.is_finite() && best.is_none_or(|(bd, _)| d > bd) {
            best = Some((d, Pt::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)));
        }
    }
    best.map(|(_, p)| p)
}

/// A bulkhead (RJ45, USB): 034's one-piece symbol, a new device, as Find Symbol places it.
fn place_bulkhead(p: &mut Project, page: PageId, style: Style, key: &str, at: Pt, degrees: f64) -> Result<Made, String> {
    let starter = crate::devices::starter(key)?;
    let placement = Placement { origin: at, rotation: degrees.to_radians(), mirror: false, scale: 1.0 };
    let plan =
        crate::devices::plan_place(&p.ledger, &mut p.ids, &mut p.tag_high, page, style, starter, placement, crate::devices::Target::New(None))?;
    Ok(Made { component: plan.component, tag: plan.tag, new: true, shown: Vec::new(), splits: Vec::new(), dropped: Vec::new(), ops: plan.ops })
}

/// The names a whole half of `n` pins takes on connector `c`: its drawn names in order, then the
/// next numbers not used.
fn join_names(doc: &Document, c: ComponentId, n: usize) -> Vec<String> {
    let d = cn::drawn(doc, c);
    let mut out: Vec<String> = d.order.iter().take(n).cloned().collect();
    let mut k = 1usize;
    while out.len() < n && k <= cn::MAX_PINS * 2 {
        let name = k.to_string();
        if !d.order.contains(&name) && !out.contains(&name) {
            out.push(name);
        }
        k += 1;
    }
    out
}

/// Row: whole halves (design 044 §4.2): Both places the plug at `at` and the socket 5 mm above it
/// in the pair's frame (below with `swap`), so their bodies share an edge.
#[allow(clippy::too_many_arguments)]
fn place_row(
    p: &mut Project, page: PageId, style: Style, device: Option<ComponentId>, halves: Side, family: Family, pins: Option<usize>, names: &Names,
    at: Pt, degrees: f64, swap: bool,
) -> Result<Made, String> {
    let doc = p.ledger.document();
    let n = match (pins, device) {
        (Some(n), _) => n,
        (None, Some(c)) => cn::pin_count(doc, c, &cn::drawn(doc, c)),
        (None, None) => match names {
            Names::List(v) => v.len(),
            _ => maker_pins(doc),
        },
    };
    if !family.sizes().contains(&n) {
        return Err(family.size_refusal());
    }
    let sides: Vec<Side> = match halves {
        Side::Both => vec![Side::Plug, Side::Socket],
        s => vec![s],
    };
    let taken = BTreeSet::new();
    if let Some(c) = device {
        for side in &sides {
            cn::check_whole_half(doc, c, *side, &taken)?;
        }
    }
    let pin_names = match device {
        Some(c) if !matches!(names, Names::List(_)) => join_names(doc, c, n),
        _ => cn::names_for(names, n)?,
    };
    let rot = degrees.to_radians();
    let (sin, cos) = rot.sin_cos();
    let up = if swap { -5.0 } else { 5.0 };
    let mut ops = Vec::new();
    let mut shown = Vec::new();
    let mut made: Option<(ComponentId, String, bool)> = device.map(|c| (c, crate::devices::tag_words(doc, c), false));
    for side in sides {
        let key = family.half_key(side, n).ok_or_else(|| family.size_refusal())?;
        let starter = crate::devices::starter(&key)?;
        let def = crate::terminal_cmds::definition_for(doc, &mut p.ids, starter, &mut ops)?;
        let origin = if side == Side::Socket && halves == Side::Both { Pt::new(at.x - sin * up, at.y + cos * up) } else { at };
        let placement = if swap {
            Placement { origin, rotation: rot + std::f64::consts::PI, mirror: true, scale: 1.0 }
        } else {
            Placement { origin, rotation: rot, mirror: false, scale: 1.0 }
        };
        let (c, main) = match &made {
            Some((c, _, _)) => (*c, false),
            None => {
                let pins_prop = Some(n);
                let list = matches!(names, Names::List(_)).then_some(pin_names.as_slice());
                let (c, tag) = cn::create_ops(&p.ledger, &mut p.ids, &mut p.tag_high, starter, def, None, pins_prop, list, &mut ops)?;
                made = Some((c, tag, true));
                (c, true)
            }
        };
        let (_, _, sh) = cn::symbol_ops(&mut p.ids, page, style.clone(), def, starter, c, &pin_names, placement, main, &mut ops)?;
        shown.extend(sh);
    }
    let (component, tag, new) = made.ok_or("Nothing to place.")?;
    Ok(Made { component, tag, new, shown, splits: Vec::new(), dropped: Vec::new(), ops })
}

/// The rotation of a pin symbol on a wire segment: its points along the wire.
fn along_wire(doc: &Document, page: PageId, on: &crate::wires::OnWire) -> f64 {
    let Some((_, _, v)) = crate::wires::route_vertices(doc, on.conductor, page) else { return 0.0 };
    match (v.get(on.segment), v.get(on.segment + 1)) {
        (Some(a), Some(b)) if (b.x - a.x).abs() > (b.y - a.y).abs() => std::f64::consts::FRAC_PI_2,
        _ => 0.0,
    }
}

/// Pins: one pin symbol on each wire the line crosses (design 044 §4.2, §4.4, §4.5).
#[allow(clippy::too_many_arguments)]
fn place_pins(
    p: &mut Project, page: PageId, style: Style, device: Option<ComponentId>, halves: Side, pins: Option<usize>, names: &Names, pin: Option<&str>,
    from: Pt, to: Pt, wire: Option<ConductorId>,
) -> Result<Made, String> {
    let doc = p.ledger.document();
    let tol = p.tolerance;
    let crossings: Vec<crate::wires::OnWire> = if from.near(to, 1e-9) {
        let w = wire
            .or_else(|| crate::wires::wire_at(doc, page, from, crate::wires::ON_WIRE, None).map(|o| o.conductor))
            .ok_or("Click on a wire, or draw a line across the wires.")?;
        vec![crate::terminal_cmds::on_wire_of(doc, page, w, from).ok_or("That wire is not drawn here.")?]
    } else {
        let found = crate::slash::crossings(doc, page, from, to, tol);
        if found.members.len() > cn::MAX_PINS {
            return Err(format!(
                "That line crosses {} wires; a connector has at most {} pins. Draw a shorter line.",
                found.members.len(),
                cn::MAX_PINS
            ));
        }
        found.members.iter().filter_map(|m| crate::terminal_cmds::on_wire_of(doc, page, m.conductor, m.at)).collect()
    };
    if crossings.is_empty() {
        return Err("The line crosses no wire. Draw it across the wires.".into());
    }
    let key = match halves {
        Side::Both => "pinpair",
        Side::Socket => "pins",
        Side::Plug => "pinp",
    };
    let starter: &Starter = crate::devices::starter(key)?;
    let mut ops = Vec::new();
    let def = crate::terminal_cmds::definition_for(doc, &mut p.ids, starter, &mut ops)?;
    // A new connector: its names first, then numbers past them (Numbers only).
    let (component, tag, new, mut fresh) = match device {
        Some(c) => (c, crate::devices::tag_words(doc, c), false, Vec::new()),
        None => {
            let n = match (pins, names) {
                (Some(n), _) => n,
                (None, Names::List(v)) => v.len(),
                (None, _) => maker_pins(doc).max(crossings.len().min(cn::MAX_PINS)),
            };
            let list = cn::names_for(names, n)?;
            let typed = matches!(names, Names::List(_)).then_some(list.as_slice());
            let (c, t) = cn::create_ops(&p.ledger, &mut p.ids, &mut p.tag_high, starter, def, None, Some(n), typed, &mut ops)?;
            (c, t, true, list)
        }
    };
    let mut taken: BTreeSet<(Side, String)> = BTreeSet::new();
    let mut named = pin.map(str::to_string);
    let mut shown_all = Vec::new();
    let mut splits = Vec::new();
    let mut dropped = Vec::new();
    let take = |side: Side, name: &str, taken: &mut BTreeSet<(Side, String)>| match side {
        Side::Both => {
            taken.insert((Side::Socket, name.to_string()));
            taken.insert((Side::Plug, name.to_string()));
        }
        s => {
            taken.insert((s, name.to_string()));
        }
    };
    for (i, on) in crossings.into_iter().enumerate() {
        let name = match named.take() {
            Some(n) => {
                if !new {
                    cn::check_named_pin(doc, component, halves, &n, &taken)?;
                } else {
                    cn::check_name(&n)?;
                }
                n
            }
            None if new => match fresh.get(i) {
                Some(n) => n.clone(),
                None if matches!(names, Names::Numbers) => {
                    let k = fresh.len() + 1;
                    if k > cn::MAX_PINS {
                        return Err(format!("A connector has at most {} pins.", cn::MAX_PINS));
                    }
                    fresh.push(k.to_string());
                    k.to_string()
                }
                None => {
                    return Err(format!(
                        "{tag} has {} named pins; the line crosses more wires. Name more pins, or draw a shorter line.",
                        fresh.len()
                    ));
                }
            },
            None => cn::next_pin(doc, component, halves, &taken)?,
        };
        if new && taken.iter().any(|(_, n)| *n == name) {
            return Err(format!("{tag}:{name} is already being drawn. Pick another pin."));
        }
        take(halves, &name, &mut taken);
        let rotation = along_wire(doc, page, &on);
        let placement = Placement { origin: on.at, rotation, mirror: false, scale: 1.0 };
        let (_, _, shown) = cn::symbol_ops(
            &mut p.ids,
            page,
            style.clone(),
            def,
            starter,
            component,
            std::slice::from_ref(&name),
            placement,
            new && i == 0,
            &mut ops,
        )?;
        let land = cn::land_pin(doc, &mut p.ids, page, on, &shown, placement)
            .map_err(|e| format!("{e} (wire {})", crate::slash::wire_name(doc, on.conductor)))?;
        if let Some((n, _)) = &land.dropped_number {
            dropped.push((land.new, n.clone()));
        }
        splits.push((land.conductor, land.new));
        ops.extend(land.ops);
        shown_all.extend(shown);
    }
    Ok(Made { component, tag, new, shown: shown_all, splits, dropped, ops })
}

// ---------------------------------------------------------------- place (Find Symbol)

/// `place {symbol: <connector half or pin>, at, rotation?, mirror?, device?, tag?, pin?}` (design
/// 044 §3.3): a new connector, or joining the connector named by `device` (or `tag`); a pin takes
/// the next pin (§4.4) or `pin`, and on a wire it lands there. One transaction.
pub(crate) fn run_place_half(
    s: &mut Session, params: &Value, starter: &'static Starter, at: Pt, degrees: f64, mirror: bool,
) -> cadcraft_engine::Result<Value> {
    let tag = opt_typed(params, "tag", "a tag", Value::as_str).map_err(err)?.map(str::to_string);
    let pin = opt_typed(params, "pin", "a pin name", Value::as_str).map_err(err)?.map(|p| p.trim().to_string());
    let side = starter.side().ok_or_else(|| err("That symbol is not a connector half."))?;
    let v = with_project(s, |p, pg, page| {
        let doc = p.ledger.document();
        let mut device = device_param(doc, params)?;
        if params.get("device").is_some_and(|v| !v.is_null()) && tag.is_some() {
            return Err("`tag` is for a new connector; `device` joins an existing one".into());
        }
        let mut new_tag = None;
        if let Some(t) = &tag
            && let Some(found) = crate::connectors::resolve(doc, t)
        {
            // Brief 044 (tester D5): a half's tag (`-X01.P`) names its connector and that half.
            let (c, half, _) = found?;
            if let Some(h) = half
                && side != Side::Both
                && h != side
            {
                return Err(format!(
                    "{} is the {} half of {}; this symbol is a {} half. Name -{}'s {} half, or Enter for a new connector.",
                    t.trim(),
                    h.word(),
                    crate::devices::tag_words(doc, c),
                    side.word(),
                    crate::devices::tag_words(doc, c).trim_start_matches('-'),
                    side.word()
                ));
            }
            device = Some(c);
        } else if let Some(t) = &tag {
            let t = ecad_model::tag::normalise(t, crate::defaults::print::tag_style(doc))?;
            match crate::devices::holder_of(doc, &t, None) {
                Some(c) => device = Some(c),
                None if !doc.cables_tagged(&t).is_empty() => return Err(format!("Tag {t} is a cable's; choose another tag for a device.")),
                None => new_tag = Some(t),
            }
        }
        if let Some(c) = device
            && !cn::is_connector(doc, c)
        {
            return Err(cn::not_a_connector(doc, c));
        }
        let style = plain_style(p);
        let mut ops = Vec::new();
        let def = crate::terminal_cmds::definition_for(doc, &mut p.ids, starter, &mut ops)?;
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
                join_names(doc, c, cn::size_of(starter).unwrap_or(0))
            }
            (None, false) => cn::names_for(&Names::Numbers, cn::size_of(starter).unwrap_or(0))?,
        };
        let (c, tag_text, new) = match device {
            Some(c) => (c, crate::devices::tag_words(doc, c), false),
            None => {
                let pins = cn::size_of(starter);
                let (c, t) = cn::create_ops(&p.ledger, &mut p.ids, &mut p.tag_high, starter, def, new_tag, pins, None, &mut ops)?;
                (c, t, true)
            }
        };
        let landing = if pin_starter { crate::terminal_cmds::landing_point(doc, page, at) } else { None };
        let origin = landing.map_or(at, |on| on.at);
        let rotation = match landing {
            Some(on) if degrees == 0.0 => along_wire(doc, page, &on),
            _ => degrees.to_radians(),
        };
        let placement = Placement { origin, rotation, mirror, scale: 1.0 };
        let (rep, _, shown) = cn::symbol_ops(&mut p.ids, page, style, def, starter, c, &names, placement, new, &mut ops)?;
        let land = match landing {
            Some(on) => Some(cn::land_pin(doc, &mut p.ids, page, on, &shown, placement)?),
            None => None,
        };
        if let Some(l) = &land {
            ops.extend(l.ops.clone());
        }
        let label = if new { format!("Place connector {tag_text}") } else { format!("Add symbol to {tag_text}") };
        p.apply_edit(pg, "place", &label, ops)?;
        p.last_placed = Some((starter.key.to_string(), c));
        let doc = p.ledger.document();
        let made = Made {
            component: c,
            tag: tag_text.clone(),
            new,
            shown: shown.clone(),
            splits: land.iter().map(|l| (l.conductor, l.new)).collect(),
            dropped: land.iter().filter_map(|l| l.dropped_number.as_ref().map(|(n, _)| (l.new, n.clone()))).collect(),
            ops: Vec::new(),
        };
        let names: Vec<String> = shown.iter().filter_map(|x| cn::pin_name(doc, x.terminal)).collect();
        let message = format!(
            "Placed {}.",
            if names.len() > 2 {
                format!("{}…{}", names.first().cloned().unwrap_or_default(), names.last().cloned().unwrap_or_default())
            } else {
                names.join(" and ")
            }
        );
        let mut v = result_json(doc, &made, &message);
        v["symbol"] = json!(starter.key);
        v["joined"] = json!(!new);
        v["representation"] = json!(rep.hex());
        v["definition"] = json!(def.hex());
        Ok(v)
    })?;
    if let Some(m) = v.get("message").and_then(Value::as_str) {
        s.echo(m.to_string());
    }
    Ok(v)
}

// ---------------------------------------------------------------- pinlabel (§5.3)

/// `pinlabel {device | terminal, pin?, to}` (design 044 §5.3).
fn run_pinlabel(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let terminal: Option<TerminalId> = opt_id(params, "terminal").map_err(err)?;
    let pin = opt_typed(params, "pin", "a pin name", Value::as_str).map_err(err)?.map(|p| p.trim().to_string());
    let to = params
        .get("to")
        .and_then(Value::as_str)
        .map(|t| t.trim().to_string())
        .ok_or_else(|| err("`to` (the new pin name, or \"next\") is required"))?;
    let v = with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let (c, old) = match (terminal, device_param(doc, params)?) {
            (Some(t), _) => {
                let x = doc.terminals.get(&t).ok_or("No such connection point.")?;
                (x.component, x.label.clone().ok_or("That pin has no name.")?)
            }
            (None, Some(c)) => (c, pin.clone().ok_or("`pin` (the pin's current name) is required with `device`")?),
            (None, None) => return Err("`device` and `pin`, or `terminal`, is required".into()),
        };
        if !cn::is_connector(doc, c) {
            return Err(format!("{} is not a connector; rename its connection points in Properties.", crate::devices::tag_words(doc, c)));
        }
        let tag = crate::devices::tag_words(doc, c);
        // `to: "next"`: one drawn pin moves to its half's next free pin (the pin-twice fix).
        if to.eq_ignore_ascii_case("next") {
            let t = terminal.ok_or("`to: \"next\"` moves one drawn pin: give its `terminal`")?;
            let side = cn::side_of(doc, t).ok_or("That is not a connector pin.")?;
            let x = doc.terminals.get(&t).ok_or("No such connection point.")?.clone();
            // The pin itself is not free for itself: drawn once more on this side.
            let next = cn::next_pin(doc, c, side, &BTreeSet::new())?;
            let after = Terminal { label: Some(next.clone()), ..x.clone() };
            let ops = vec![Op::RemoveTerminal { id: t, terminal: x }, Op::AddTerminal { id: t, terminal: after }];
            let half = cn::half_tag(doc, c, side);
            p.apply_edit(pg, "pinlabel", &format!("Move pin {half}:{old}"), ops)?;
            let message = format!("{half}:{old} is now {half}:{next}.");
            return Ok(json!({"changed": true, "component": c.hex(), "from": old, "to": next, "message": message}));
        }
        cn::check_name(&to)?;
        let targets: Vec<(TerminalId, Terminal)> =
            doc.terminals_of(c).filter(|(_, x)| x.label.as_deref() == Some(old.as_str())).map(|(t, x)| (t, x.clone())).collect();
        if targets.is_empty() {
            return Err(format!("{tag} has no pin {old}."));
        }
        if to == old {
            return Ok(json!({"changed": false, "component": c.hex(), "to": to}));
        }
        if doc.terminals_of(c).any(|(_, x)| x.label.as_deref() == Some(to.as_str())) || cn::pin_names(doc, c).is_some_and(|l| l.contains(&to)) {
            return Err(format!("{tag} already has a pin {to}. Pick another name, or rename that pin first."));
        }
        let mut ops = Vec::new();
        for (t, x) in &targets {
            ops.push(Op::RemoveTerminal { id: *t, terminal: x.clone() });
        }
        for (t, x) in targets {
            ops.push(Op::AddTerminal { id: t, terminal: Terminal { label: Some(to.clone()), ..x } });
        }
        // A typed list keeps the pin's place under its new name.
        if let Some(list) = cn::pin_names(doc, c)
            && list.contains(&old)
        {
            let after: Vec<String> = list.iter().map(|n| if *n == old { to.clone() } else { n.clone() }).collect();
            let before = doc.components.get(&c).and_then(|x| x.properties.get(cn::PIN_NAMES_KEY)).cloned();
            ops.push(Op::SetProperty {
                object: ecad_model::ObjectRef::Component(c),
                key: cn::PIN_NAMES_KEY.into(),
                before,
                after: Some(after.join(",")),
            });
        }
        p.apply_edit(pg, "pinlabel", &format!("Rename pin {tag}:{old}"), ops)?;
        let doc = p.ledger.document();
        let d = cn::drawn(doc, c);
        let both = d.socket.contains_key(&to) && d.plug.contains_key(&to);
        let message = format!("{tag}:{old} is now {tag}:{to}{}.", if both { " (both halves)" } else { "" });
        Ok(json!({"changed": true, "component": c.hex(), "from": old, "to": to, "message": message}))
    })?;
    if let Some(m) = v.get("message").and_then(Value::as_str) {
        s.echo(m.to_string());
    }
    Ok(v)
}
