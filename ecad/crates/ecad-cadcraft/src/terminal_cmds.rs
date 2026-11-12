//! The commands of brief 028 (design 028 §3, §4, §10): a terminal placed on a wire splits it
//! (through `place`), `terminals` (Place Terminals: one per wire a fence line crosses), `bridge`
//! (Draw Bridge), and the reads `stripplan` and `cablecores`.
//!
//! Every write is one transaction, committed through [`Project::apply_edit`]; the reads record
//! nothing.

use std::collections::{BTreeMap, BTreeSet};

use cadcraft_engine::{CommandSpec, Session};
use ecad_model::{
    CableId, CableMember, ComponentId, Conductor, ConductorId, DefinitionId, Document, End, Form, IdAllocator, Item, ItemId, ItemKind, Op, PageId,
    Placement, Primitive, Pt, Representation, RepresentationId, Style, Subject, SymbolInstance, Terminal, TerminalId,
};
use serde_json::{Value, json};

use crate::commands::{err, id, opt_id, opt_typed, plain_style, point, with_any_page, with_project};
use crate::strips::{self, StripData, TermRow};
use crate::wires::{ON_WIRE, OnWire};

/// Most wires one Place Terminals line lands on (design 028 §3.2).
pub const MAX_FENCE: usize = 200;
/// How far a bridge's staple stands off its terminals, in millimetres (design 028 §4).
pub const BRIDGE_STANDOFF: f64 = 4.0;

/// The commands this module adds.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("terminals", "Place Terminals", run_terminals)
            .params(
                "{strip?: id (null or absent: a new strip), pe?: bool, from: [x,y], to: [x,y]} (one terminal on each wire the line crosses, \
numbered on from the strip's highest label; at most 200)",
            )
            .interactive(crate::interact::terminals)
            .noundo(),
        CommandSpec::new("bridge", "Draw Bridge", run_bridge)
            .params("{from: terminal id, to: terminal id} (a wire between two terminals; of one strip it is a bridge)")
            .interactive(crate::interact::bridge)
            .noundo(),
        CommandSpec::new("stripplan", "Terminal Plan", run_stripplan)
            .params("{strip?: id, limit?: rows (500, max 5000), after?: strip index} (every strip's terminals, drawn and spare; read only)")
            .noundo(),
        CommandSpec::new("cablecores", "Cable Cores", run_cablecores)
            .params("{cable: id} (the cable's offered cores, used, spare and over; read only)")
            .noundo(),
    ]
}

// ---------------------------------------------------------------- landing on a wire (§3.1)

/// A wire split at a terminal.
#[derive(Debug, Clone)]
pub struct Land {
    pub ops: Vec<Op>,
    /// The wire, which keeps end A and the route up to the terminal.
    pub conductor: ConductorId,
    /// The new second half, from the terminal to the old end B.
    pub new: ConductorId,
    /// The cable whose membership was decided, and which half keeps it.
    pub cable: Option<(CableId, ConductorId, bool)>,
}

fn segments(v: &[Pt]) -> impl Iterator<Item = (Pt, Pt)> + '_ {
    v.windows(2).filter_map(|w| Some((*w.first()?, *w.get(1)?)))
}

/// Whether segments `p`-`q` and `r`-`s` cross or touch.
fn meets(p: Pt, q: Pt, r: Pt, s: Pt) -> bool {
    let d = (q.x - p.x) * (s.y - r.y) - (q.y - p.y) * (s.x - r.x);
    if d.abs() < 1e-12 {
        return false;
    }
    let t = ((r.x - p.x) * (s.y - r.y) - (r.y - p.y) * (s.x - r.x)) / d;
    let u = ((r.x - p.x) * (q.y - p.y) - (r.y - p.y) * (q.x - p.x)) / d;
    (-1e-9..=1.0 + 1e-9).contains(&t) && (-1e-9..=1.0 + 1e-9).contains(&u)
}

/// The terminal of any strip drawn at `at` on `page`.
pub fn strip_terminal_at(doc: &Document, page: PageId, at: Pt) -> Option<TerminalId> {
    // Each terminal symbol shows its own connection point: O(representations on the page).
    doc.representations_on(page).find_map(|(_, r)| {
        let Subject::Component(c) = r.subject else { return None };
        if !strips::is_strip(doc, c) {
            return None;
        }
        let placement = crate::devices::rep_definition(doc, r).map(|(_, pl)| pl);
        crate::devices::shown_terminals(doc, c, r).into_iter().find_map(|(t, p)| {
            let p = match (r.form, placement) {
                (Form::Symbol, Some(pl)) => pl.apply(p),
                _ => p,
            };
            p.near(at, ON_WIRE).then_some(t)
        })
    })
}

/// `-X01:3`.
pub fn terminal_words(doc: &Document, t: TerminalId) -> String {
    let c = doc.terminals.get(&t).map(|x| x.component);
    let tag = c.and_then(|c| crate::devices::tag_of(doc, c)).unwrap_or("?");
    format!("{tag}:{}", crate::interact::terminal_name(doc, t))
}

/// The wire `c` met at `at` on `page`: its segment there.
pub fn on_wire_of(doc: &Document, page: PageId, c: ConductorId, at: Pt) -> Option<OnWire> {
    let (_, _, v) = crate::wires::route_vertices(doc, c, page)?;
    let mut best: Option<(f64, usize)> = None;
    for (i, (a, b)) in segments(&v).enumerate() {
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let len2 = dx * dx + dy * dy;
        let t = if len2 > 0.0 { (((at.x - a.x) * dx + (at.y - a.y) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
        let d = Pt::new(a.x + t * dx, a.y + t * dy).dist(at);
        if d.is_finite() && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, i));
        }
    }
    let (d, segment) = best?;
    (d <= ON_WIRE).then_some(OnWire { conductor: c, segment, at })
}

/// Plan landing terminal `t` (being placed at `on.at`) on its wire (design 028 §3.1): the wire is
/// split there and both halves attach to `t`; cable membership stays with the half the cable's
/// slash crosses on this page.
///
/// # Errors
/// The point at a route end, a terminal already there, a wire drawn on several pages; each says
/// what to do instead.
pub fn plan_land(doc: &Document, ids: &mut IdAllocator, page: PageId, on: OnWire, t: TerminalId) -> Result<Land, String> {
    let c = on.conductor;
    let cond = doc.conductors.get(&c).ok_or("No such wire.")?;
    let pages: BTreeSet<PageId> = doc.representations_of(Subject::Conductor(c)).map(|(_, r)| r.page).collect();
    if pages.len() > 1 {
        return Err(format!(
            "This wire is drawn on {} pages; place the terminal where the wire is drawn on one page, or on empty space and wire it.",
            pages.len()
        ));
    }
    if let Some(x) = strip_terminal_at(doc, page, on.at) {
        return Err(format!("{} is already there. Click elsewhere on the wire.", terminal_words(doc, x)));
    }
    let (item_id, item, v) = crate::wires::route_vertices(doc, c, page).ok_or("The wire has no route on this page.")?;
    let (first, last) = (v.first().copied().ok_or("empty route")?, v.last().copied().ok_or("empty route")?);
    for (end, p) in [(End::A, first), (End::B, last)] {
        if on.at.near(p, ON_WIRE) {
            return Err(match cond.end(end) {
                Some(x) => {
                    format!("That is the end of the wire at {}. Click along the wire, away from its ends, or on empty space.", terminal_words(doc, x))
                }
                None => "That is the end of the wire. Click along the wire, away from its ends, or on empty space.".into(),
            });
        }
    }
    let seg = on.segment;
    let mut keep: Vec<Pt> = v.get(..=seg).unwrap_or_default().to_vec();
    let mut rest: Vec<Pt> = v.get(seg + 1..).unwrap_or_default().to_vec();
    if keep.last().is_some_and(|p| p.near(on.at, 1e-9)) {
        keep.pop();
    }
    if rest.first().is_some_and(|p| p.near(on.at, 1e-9)) {
        rest.remove(0);
    }
    keep.push(on.at);
    rest.insert(0, on.at);
    let path = |pts: &[Pt]| match pts {
        [a, b] => Primitive::Line { a: *a, b: *b },
        _ => Primitive::Polyline { vertices: pts.to_vec(), closed: false },
    };
    let mut ops = Vec::new();
    let mut shortened = item.clone();
    shortened.kind = ItemKind::Primitive(path(&keep));
    ops.push(Op::ModifyItem { id: item_id, before: item.clone(), after: shortened });
    let old_b = cond.end(End::B);
    ops.push(Op::AttachEnd { conductor: c, end: End::B, before: old_b, after: Some(t) });
    let (c2, i2, r2): (ConductorId, ItemId, RepresentationId) = (ids.fresh(), ids.fresh(), ids.fresh());
    ops.push(Op::CreateConductor { id: c2, conductor: Conductor { properties: cond.properties.clone(), ends: [Some(t), old_b] } });
    let mut second = item;
    second.kind = ItemKind::Primitive(path(&rest));
    ops.push(Op::AddItem { id: i2, item: second });
    let role =
        doc.representations_of(Subject::Conductor(c)).find(|(_, r)| r.page == page).map_or_else(|| "route".to_string(), |(_, r)| r.role.clone());
    ops.push(Op::LinkRepresentation {
        id: r2,
        rep: Representation { subject: Subject::Conductor(c2), page, form: Form::Drawn, role, items: vec![i2], connection_points: BTreeMap::new() },
    });
    // Cable membership (§3.1 step 3).
    let mut cable = None;
    if let Some(k) = doc.cables_of(c).first().copied()
        && let Some((index, member)) = doc.cables.get(&k).and_then(|x| x.members.iter().enumerate().find(|(_, m)| m.conductor == c))
    {
        let slashes = crate::slash::slashes_of(doc, page, k);
        let crosses = |pts: &[Pt]| slashes.iter().any(|(_, a, b)| segments(pts).any(|(p, q)| meets(p, q, *a, *b)));
        let (in_keep, in_rest) = (crosses(&keep), crosses(&rest));
        let moved = in_rest && !in_keep;
        let decided = in_keep != in_rest;
        if moved {
            ops.push(Op::RemoveCableMember { cable: k, index, member: member.clone() });
            ops.push(Op::AddCableMember { cable: k, index, member: CableMember { conductor: c2, core: member.core.clone() } });
        }
        cable = Some((k, if moved { c2 } else { c }, decided));
    }
    Ok(Land { ops, conductor: c, new: c2, cable })
}

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

/// The definition ID of starter `s` in `doc`, embedding it in `ops` when it is not yet there.
fn definition_for(doc: &Document, ids: &mut IdAllocator, s: &crate::symbols::Starter, ops: &mut Vec<Op>) -> Result<DefinitionId, String> {
    if let Some(d) = crate::symbols::embedded(doc, s) {
        return Ok(d);
    }
    if let Some(d) = ops.iter().find_map(|o| match o {
        Op::EmbedDefinition { id, definition } if crate::symbols::of_definition(definition).is_some_and(|x| x.key == s.key) => Some(*id),
        _ => None,
    }) {
        return Ok(d);
    }
    let d: DefinitionId = ids.fresh();
    ops.push(Op::EmbedDefinition { id: d, definition: s.definition()?.clone() });
    Ok(d)
}

/// Plan one terminal symbol of `strip` labelled `label` at `at` (a join: its own connection
/// point keyed by its label, as 018's joins are).
#[allow(clippy::too_many_arguments)]
fn terminal_symbol(
    ids: &mut IdAllocator, page: PageId, style: Style, def: DefinitionId, strip: ComponentId, label: &str, at: Pt, main: bool, role: &str,
    ops: &mut Vec<Op>,
) -> TerminalId {
    let (t, item, rep): (TerminalId, ItemId, RepresentationId) = (ids.fresh(), ids.fresh(), ids.fresh());
    let key = if main { "1".to_string() } else { label.to_string() };
    ops.push(Op::AddTerminal { id: t, terminal: Terminal { component: strip, label: Some(label.to_string()), key: Some(key) } });
    let placement = Placement { origin: at, rotation: 0.0, mirror: false, scale: 1.0 };
    ops.push(Op::AddItem { id: item, item: Item { page, style, kind: ItemKind::Symbol(SymbolInstance { definition: def, placement }) } });
    let mut cps = BTreeMap::new();
    if !main {
        cps.insert(t, Pt::new(0.0, 0.0));
    }
    ops.push(Op::LinkRepresentation {
        id: rep,
        rep: Representation {
            subject: Subject::Component(strip),
            page,
            form: Form::Symbol,
            role: if main { "main".into() } else { role.into() },
            items: vec![item],
            connection_points: cps,
        },
    });
    t
}

/// A planned Place Terminals line.
#[derive(Debug, Clone)]
pub struct Fence {
    pub ops: Vec<Op>,
    pub strip: ComponentId,
    pub tag: String,
    /// (terminal, label, wire, new half) in crossing order.
    pub placed: Vec<(TerminalId, String, ConductorId, ConductorId)>,
    /// Wires skipped because a terminal of the strip is already there.
    pub skipped: usize,
}

/// Plan Place Terminals (design 028 §3.2) from `from` to `to` on `page`.
///
/// # Errors
/// No wire crossed, more than [`MAX_FENCE`], an unknown strip, or a refusal of one landing.
#[allow(clippy::too_many_arguments)]
pub fn plan_fence(
    ledger: &ecad_model::Ledger, ids: &mut IdAllocator, high: &mut crate::devices::TagHigh, page: PageId, style: Style, strip: Option<ComponentId>,
    pe: bool, from: Pt, to: Pt, tol: f64,
) -> Result<Fence, String> {
    let doc = ledger.document();
    let found = crate::slash::crossings(doc, page, from, to, tol);
    if found.members.len() > MAX_FENCE {
        return Err(format!("That line crosses {} wires; at most {MAX_FENCE} at once. Draw a shorter line.", found.members.len()));
    }
    let starter = crate::devices::starter(if pe { strips::PE_KEY } else { "terminal" })?;
    if let Some(c) = strip
        && !strips::is_strip(doc, c)
    {
        return Err("That device is not a terminal strip. Press Enter for a new strip, or type the tag of one (-X01).".into());
    }
    let mut ops = Vec::new();
    let def = definition_for(doc, ids, starter, &mut ops)?;
    let mut crossings = Vec::new();
    let mut skipped = 0;
    for m in &found.members {
        if let Some(x) = strip_terminal_at(doc, page, m.at)
            && strip.is_some_and(|c| doc.terminals.get(&x).is_some_and(|t| t.component == c))
        {
            skipped += 1;
            continue;
        }
        crossings.push(*m);
    }
    if crossings.is_empty() {
        return Err(if skipped > 0 {
            "Every wire the line crosses already has a terminal of this strip there.".into()
        } else {
            "The line crosses no wire. Draw it across the wires to land.".into()
        });
    }
    let (strip, tag, mut next, mut first_main) = match strip {
        Some(c) => {
            let highest =
                crate::devices::labels_of(doc, c).iter().filter_map(|l| l.parse::<u64>().ok()).filter(|n| *n < 1_000_000).max().unwrap_or(0);
            (c, crate::devices::tag_words(doc, c), highest + 1, false)
        }
        None => {
            let c: ComponentId = ids.fresh();
            let tag = high.next(ledger, &crate::devices::prefix_for(doc, starter))?;
            let mut properties = ecad_model::Properties::new();
            properties.insert(ecad_model::tag::TAG_KEY.into(), tag.clone());
            properties.insert("description".into(), "Terminal".into());
            ops.push(Op::CreateComponent { id: c, component: ecad_model::Component { properties, definition: Some(def) } });
            (c, tag, 1, true)
        }
    };
    let mut placed = Vec::new();
    for m in crossings {
        let on = on_wire_of(doc, page, m.conductor, m.at).ok_or("A crossed wire has no route here.")?;
        let label = next.to_string();
        let t = terminal_symbol(ids, page, style.clone(), def, strip, &label, m.at, first_main, starter.key, &mut ops);
        first_main = false;
        let land = plan_land(doc, ids, page, on, t).map_err(|e| format!("{e} (wire {})", crate::slash::wire_name(doc, m.conductor)))?;
        ops.extend(land.ops);
        placed.push((t, label, land.conductor, land.new));
        next += 1;
    }
    Ok(Fence { ops, strip, tag, placed, skipped })
}

/// [`plan_fence`] on project `p`, with its own IDs and tag numbers (tools such as the bench, which
/// commit the plan themselves).
///
/// # Errors
/// As [`plan_fence`].
pub fn plan_fence_in(
    p: &mut crate::project::Project, page: PageId, style: Style, strip: Option<ComponentId>, pe: bool, from: Pt, to: Pt,
) -> Result<Fence, String> {
    let tol = p.tolerance;
    plan_fence(&p.ledger, &mut p.ids, &mut p.tag_high, page, style, strip, pe, from, to, tol)
}

fn run_terminals(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let from = params.get("from").and_then(point).ok_or_else(|| err("`from` [x,y] is required"))?;
    let to = params.get("to").and_then(point).ok_or_else(|| err("`to` [x,y] is required"))?;
    let strip: Option<ComponentId> = opt_id(params, "strip").map_err(err)?;
    let pe = opt_typed(params, "pe", "true or false", Value::as_bool).map_err(err)?.unwrap_or(false);
    let bad = |q: Pt| !(q.x.abs() <= crate::interact::MAX_COORD && q.y.abs() <= crate::interact::MAX_COORD);
    if bad(from) || bad(to) {
        return Err(err(crate::interact::OUT_OF_RANGE));
    }
    let v = with_project(s, |p, pg, page| {
        if from.near(to, p.tolerance) {
            return Err("`from` and `to` must differ".into());
        }
        let style = plain_style(p);
        let f = plan_fence(&p.ledger, &mut p.ids, &mut p.tag_high, page, style, strip, pe, from, to, p.tolerance)?;
        p.apply_edit(pg, "terminals", "Place terminals", f.ops.clone())?;
        p.last_placed = Some((if pe { strips::PE_KEY } else { "terminal" }.to_string(), f.strip));
        let labels: Vec<&str> = f.placed.iter().map(|(_, l, _, _)| l.as_str()).collect();
        let range = match labels.as_slice() {
            [one] => (*one).to_string(),
            [a, .., b] => format!("{a}\u{2026}{b}"),
            [] => String::new(),
        };
        let n = f.placed.len();
        let message = format!("Placed {}:{range} on {}.", f.tag, if n == 1 { "1 wire".to_string() } else { format!("{n} wires") });
        Ok(json!({
            "strip": f.strip.hex(),
            "tag": f.tag,
            "terminals": f.placed.iter().map(|(t, l, c, n)| json!({"terminal": t.hex(), "label": l, "conductor": c.hex(), "new": n.hex()})).collect::<Vec<_>>(),
            "skipped": f.skipped,
            "message": message,
        }))
    })?;
    if let Some(m) = v.get("message").and_then(Value::as_str) {
        s.echo(m.to_string());
    }
    Ok(v)
}

// ---------------------------------------------------------------- Draw Bridge (§4)

/// The staple of a bridge from `a` to `b` (design 028 §4): 4 mm towards the inside (up; left when
/// the terminals sit one above the other), then across.
pub fn staple(a: Pt, b: Pt) -> Vec<Pt> {
    let (dx, dy) = ((b.x - a.x).abs(), (b.y - a.y).abs());
    let (ox, oy) = if dy > dx { (-BRIDGE_STANDOFF, 0.0) } else { (0.0, BRIDGE_STANDOFF) };
    vec![a, Pt::new(a.x + ox, a.y + oy), Pt::new(b.x + ox, b.y + oy), b]
}

fn run_bridge(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let a: TerminalId = id(params, "from").map_err(err)?;
    let b: TerminalId = id(params, "to").map_err(err)?;
    let v = with_project(s, |p, pg, page| {
        let doc = p.ledger.document();
        if !doc.terminals.contains_key(&a) || !doc.terminals.contains_key(&b) {
            return Err("No such terminal.".into());
        }
        if a == b {
            return Err("Pick two different terminals: a bridge links one terminal to another.".into());
        }
        let (wa, wb) = (terminal_words(doc, a), terminal_words(doc, b));
        let (sa, sb) = (strips::strip_of_terminal(doc, a), strips::strip_of_terminal(doc, b));
        let same = sa.is_some() && sa == sb;
        if let Some((w, _)) =
            doc.ends_at(a).into_iter().find(|(w, _)| doc.conductors.get(w).is_some_and(|c| c.ends.contains(&Some(b)) && c.ends.contains(&Some(a))))
        {
            let message = format!("{wa} and {wb} are already bridged.");
            return Ok(json!({"conductor": w.hex(), "bridge": same, "changed": false, "message": message}));
        }
        let pos = |t: TerminalId| {
            doc.representations_of(Subject::Component(doc.terminals.get(&t)?.component))
                .find_map(|(_, r)| (r.page == page).then(|| ecad_model::derive::terminal_position(doc, page, t)).flatten())
        };
        let (Some(pa), Some(pb)) = (pos(a), pos(b)) else {
            return Err(format!("{wa} and {wb} must both be on this page to draw a bridge; open the page that shows both, or draw a wire."));
        };
        let route = if same { staple(pa, pb) } else { vec![pa, pb] };
        let (w, i, r): (ConductorId, ItemId, RepresentationId) = (p.ids.fresh(), p.ids.fresh(), p.ids.fresh());
        let item = Item { page, style: plain_style(p), kind: ItemKind::Primitive(Primitive::Polyline { vertices: route, closed: false }) };
        let ops = vec![
            Op::CreateConductor { id: w, conductor: Conductor::default() },
            Op::AddItem { id: i, item },
            Op::LinkRepresentation {
                id: r,
                rep: Representation {
                    subject: Subject::Conductor(w),
                    page,
                    form: Form::Drawn,
                    role: "route".into(),
                    items: vec![i],
                    connection_points: BTreeMap::new(),
                },
            },
            Op::AttachEnd { conductor: w, end: End::A, before: None, after: Some(a) },
            Op::AttachEnd { conductor: w, end: End::B, before: None, after: Some(b) },
        ];
        let label_b = crate::interact::terminal_name(doc, b);
        let label = if same { format!("Bridge {wa}\u{2013}{label_b}") } else { "Wire".to_string() };
        p.apply_edit(pg, "bridge", &label, ops)?;
        let message = if same {
            format!("Bridged {wa}\u{2013}{label_b}.")
        } else {
            format!("{wa} and {wb} are on different strips; drawn as a wire, not a bridge.")
        };
        Ok(json!({"conductor": w.hex(), "bridge": same, "changed": true, "message": message}))
    })?;
    if let Some(m) = v.get("message").and_then(Value::as_str) {
        s.echo(m.to_string());
    }
    Ok(v)
}

// ---------------------------------------------------------------- reads (§10)

/// A side cell as JSON.
fn side_json(c: &strips::SideCell, outside: bool) -> Value {
    let mut v = json!({"target": c.target, "wire": c.wire, "conductor": c.conductor.hex()});
    if outside {
        v["cable"] = json!(c.cable.as_ref().map(|(_, t)| t.clone().unwrap_or_else(|| "?".into())));
        v["core"] = json!(c.core);
    }
    v
}

/// One plan row as JSON (the keys 027 exports).
pub fn row_json(r: &TermRow, style: ecad_model::ReferenceStyle) -> Value {
    json!({
        "terminal": r.terminal.map(|t| t.hex()),
        "label": r.label,
        "type": if r.pe { "pe" } else { "feed-through" },
        "state": if r.spare() { "spare" } else { "drawn" },
        "inside": r.inside.iter().map(|c| side_json(c, false)).collect::<Vec<_>>(),
        "outside": r.outside.iter().map(|c| side_json(c, true)).collect::<Vec<_>>(),
        "bridge_group": r.bridge_group,
        "place": r.place.as_ref().map(|p| p.text(style)),
    })
}

/// One strip as JSON.
pub fn strip_json(s: &StripData, style: ecad_model::ReferenceStyle) -> Value {
    json!({
        "strip": s.component.hex(),
        "tag": s.tag,
        "location": s.location,
        "offered": s.offer.map(|o| o.text()),
        "drawn": s.drawn,
        "spare": s.spare,
        "rows": s.rows.iter().map(|r| row_json(r, style)).collect::<Vec<_>>(),
    })
}

fn run_stripplan(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let strip: Option<ComponentId> = opt_id(params, "strip").map_err(err)?;
    let limit = opt_typed(params, "limit", "a whole number", Value::as_u64)
        .map_err(err)?
        .map_or(crate::outputs::DEFAULT_LIMIT, |n| n as usize)
        .clamp(1, crate::outputs::MAX_LIMIT);
    let after = opt_typed(params, "after", "a whole number", Value::as_u64).map_err(err)?.unwrap_or(0) as usize;
    with_any_page(s, |p, _| {
        let style = p.ledger.document().settings.reference_style;
        let o = p.outputs();
        if let Some(c) = strip {
            let d = o.strips.get(c).ok_or("No such terminal strip.")?;
            return Ok(json!({"strips": [strip_json(d, style)]}));
        }
        let all = o.strips.sorted();
        let mut out = Vec::new();
        let mut rows = 0;
        let mut next = None;
        for (i, d) in all.iter().enumerate().skip(after) {
            if !out.is_empty() && rows + d.rows.len() > limit {
                next = Some(i);
                break;
            }
            rows += d.rows.len();
            out.push(strip_json(d, style));
        }
        let mut v = json!({"strips": out, "total": all.len()});
        if let Some(n) = next {
            v["next"] = json!(n);
        }
        Ok(v)
    })
}

fn run_cablecores(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let k: CableId = id(params, "cable").map_err(err)?;
    with_any_page(s, |p, _| {
        p.outputs();
        let o = p.outputs.as_ref().ok_or("The reports could not be built.")?;
        let row = o.cables().into_iter().find(|r| r.cable == k).cloned().ok_or("no such cable")?;
        let wire = |w: ConductorId| o.wire(w).cloned();
        let doc = p.ledger.document();
        let style = doc.settings.reference_style;
        let count = &row.count;
        let member_json = |i: usize, label: Option<String>, state: &str| {
            let w = row.members.get(i).copied();
            let r = w.and_then(wire);
            json!({
                "label": label,
                "state": state,
                "conductor": w.map(|w| w.hex()),
                "wire": r.as_ref().and_then(|r| r.wire_number.clone()),
                "from": r.as_ref().and_then(|r| r.from.text(style)),
                "to": r.as_ref().and_then(|r| r.to.text(style)),
            })
        };
        let stored = |i: usize| doc.cables.get(&k).and_then(|c| c.members.get(i)).and_then(|m| m.core.clone());
        let mut cores = Vec::new();
        match &count.offer {
            Some(offer) => {
                for label in offer {
                    match count.matched.iter().position(|m| m.as_deref() == Some(label.as_str())) {
                        Some(i) => cores.push(member_json(i, Some(label.clone()), "used")),
                        None => cores.push(json!({"label": label, "state": "spare", "from": "spare"})),
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
        Ok(json!({
            "cable": k.hex(),
            "tag": row.tag,
            "offer": count.offer_text,
            "source": count.source.map(|s| s.word()),
            "used": count.used(),
            "spare": count.offer.as_ref().map(|_| count.spare.clone()),
            "cores": cores,
            "over": over,
        }))
    })
}
