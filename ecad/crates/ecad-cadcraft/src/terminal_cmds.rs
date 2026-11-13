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
use crate::terminal_types as tt;
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
                "{strip?: id (null or absent: a new strip), pe?: bool, type?: Feedthrough|PE|x2|x2B|x3|LNPE|SEN|DIS|FUSE, level?: name, number \
or All, position?: label, from: [x,y], to: [x,y]} (one terminal, or one level, on each wire the line crosses; a level fills the next \
position of its type that lacks it; at most 200)",
            )
            .interactive(crate::interact::terminals)
            .noundo(),
        CommandSpec::new("bridge", "Draw Bridge", run_bridge)
            .params("{from: terminal id, to: terminal id} (a wire between two terminals; of one strip it is a bridge)")
            .interactive(crate::interact::bridge)
            .noundo(),
        CommandSpec::new("stripplan", "Terminal Plan", run_stripplan)
            .params(
                "{strip?: id, limit?: rows (500, max 5000), after?: row index, accessories?: bool (true: gap rows, markers and links)} \
(every strip's terminals, drawn and spare; read only)",
            )
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
pub(crate) fn definition_for(doc: &Document, ids: &mut IdAllocator, s: &crate::symbols::Starter, ops: &mut Vec<Op>) -> Result<DefinitionId, String> {
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
    let ty = if pe { tt::by_code("pe") } else { Some(tt::feedthrough()) }.ok_or("no PE type")?;
    plan_fence_typed(ledger, ids, high, page, style, strip, FenceKind { ty, level: Some(1), position: None }, from, to, tol)
}

/// What a Place Terminals line places (brief 036 §3.2).
#[derive(Debug, Clone)]
pub struct FenceKind {
    pub ty: &'static tt::TermType,
    /// The level (`None`: whole-terminal symbols).
    pub level: Option<u32>,
    /// The position typed at the prompt, for the first crossing.
    pub position: Option<String>,
}

/// [`plan_fence`] for any terminal type and level (brief 036 §3.2, §3.3).
///
/// # Errors
/// As [`plan_fence`], and a named position that cannot take the level.
#[allow(clippy::too_many_arguments)]
pub fn plan_fence_typed(
    ledger: &ecad_model::Ledger, ids: &mut IdAllocator, high: &mut crate::devices::TagHigh, page: PageId, style: Style, strip: Option<ComponentId>,
    kind: FenceKind, from: Pt, to: Pt, tol: f64,
) -> Result<Fence, String> {
    let doc = ledger.document();
    let found = crate::slash::crossings(doc, page, from, to, tol);
    if found.members.len() > MAX_FENCE {
        return Err(format!("That line crosses {} wires; at most {MAX_FENCE} at once. Draw a shorter line.", found.members.len()));
    }
    let legacy = kind.ty.legacy();
    let starter = crate::devices::starter(&kind.ty.starter_key(kind.level))?;
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
    let mut chooser = tt::Chooser::new(doc, strip);
    let existing = strip;
    let (strip, tag, mut next, mut first_main) = match strip {
        Some(c) => (c, crate::devices::tag_words(doc, c), tt::highest_number(doc, c) + 1, false),
        None => {
            let c: ComponentId = ids.fresh();
            // Brief 026: a provisional `-X?n` when provisional tags are on, as Place gives.
            let tag = high.next_auto(ledger, &crate::devices::prefix_for(doc, starter))?;
            let mut properties = ecad_model::Properties::new();
            properties.insert(ecad_model::tag::TAG_KEY.into(), tag.clone());
            properties.insert("description".into(), "Terminal".into());
            ops.push(Op::CreateComponent { id: c, component: ecad_model::Component { properties, definition: Some(def) } });
            (c, tag, 1, legacy)
        }
    };
    let mut placed = Vec::new();
    let mut named = kind.position.clone();
    for m in crossings {
        let on = on_wire_of(doc, page, m.conductor, m.at).ok_or("A crossed wire has no route here.")?;
        if legacy {
            let label = next.to_string();
            let t = terminal_symbol(ids, page, style.clone(), def, strip, &label, m.at, first_main, starter.key, &mut ops);
            first_main = false;
            let land = plan_land(doc, ids, page, on, t).map_err(|e| format!("{e} (wire {})", crate::slash::wire_name(doc, m.conductor)))?;
            ops.extend(land.ops);
            placed.push((t, label, land.conductor, land.new));
            next += 1;
            continue;
        }
        let position = chooser.choose(doc, existing, kind.ty, kind.level, named.take().as_deref())?;
        let placement = Placement { origin: m.at, rotation: 0.0, mirror: false, scale: 1.0 };
        let shown = level_symbol(ids, page, style.clone(), def, starter, strip, &position, placement, &mut ops)?;
        let land =
            land_level(doc, ids, page, on, &shown, placement).map_err(|e| format!("{e} (wire {})", crate::slash::wire_name(doc, m.conductor)))?;
        ops.extend(land.ops);
        let t = shown.first().map(|(_, t, _)| *t).ok_or("The symbol has no connection point.")?;
        placed.push((t, position, land.conductor, land.new));
    }
    Ok(Fence { ops, strip, tag, placed, skipped })
}

/// Plan one level symbol (or a whole terminal) of `strip` at position `position` (brief 036 §3.3):
/// one terminal per definition point, labelled with the position and keyed by the point, shown
/// through explicit connection points. Returns (key, terminal, local point) in definition order.
///
/// # Errors
/// The starter's definition could not be built.
#[allow(clippy::too_many_arguments)]
pub fn level_symbol(
    ids: &mut IdAllocator, page: PageId, style: Style, def: DefinitionId, starter: &crate::symbols::Starter, strip: ComponentId, position: &str,
    placement: Placement, ops: &mut Vec<Op>,
) -> Result<Vec<(String, TerminalId, Pt)>, String> {
    let d = starter.definition()?;
    let (item, rep): (ItemId, RepresentationId) = (ids.fresh(), ids.fresh());
    let mut cps = BTreeMap::new();
    let mut out = Vec::new();
    for td in &d.terminals {
        let t: TerminalId = ids.fresh();
        ops.push(Op::AddTerminal { id: t, terminal: Terminal { component: strip, label: Some(position.to_string()), key: Some(td.key.clone()) } });
        cps.insert(t, td.at);
        out.push((td.key.clone(), t, td.at));
    }
    ops.push(Op::AddItem { id: item, item: Item { page, style, kind: ItemKind::Symbol(SymbolInstance { definition: def, placement }) } });
    ops.push(Op::LinkRepresentation {
        id: rep,
        rep: Representation {
            subject: Subject::Component(strip),
            page,
            form: Form::Symbol,
            role: starter.key.into(),
            items: vec![item],
            connection_points: cps,
        },
    });
    Ok(out)
}

/// Land a level symbol on its wire: a one-point level as 028 does; a two-point level splits the
/// wire into two nets (design 036 §3.4): the half towards the in point lands on `.1i`, the other
/// on `.1o`, each route ending at its point, and the second half drops its stored wire number.
///
/// # Errors
/// [`plan_land`]'s.
pub fn land_level(
    doc: &Document, ids: &mut IdAllocator, page: PageId, on: OnWire, shown: &[(String, TerminalId, Pt)], placement: Placement,
) -> Result<Land, String> {
    let first = shown.first().map(|(_, t, _)| *t).ok_or("The symbol has no connection point.")?;
    let (Some(tin), Some(tout)) = (shown.iter().find(|(k, _, _)| k.ends_with('i')), shown.iter().find(|(k, _, _)| k.ends_with('o'))) else {
        return plan_land(doc, ids, page, on, first);
    };
    let mut land = plan_land(doc, ids, page, on, tin.1)?;
    let (pin, pout) = (placement.apply(tin.2), placement.apply(tout.2));
    // Which half comes from the in point's side: the kept half's vertex before the split.
    let before = crate::wires::route_vertices(doc, on.conductor, page).and_then(|(_, _, v)| v.get(on.segment).copied());
    let keep_in = before.is_none_or(|b| b.dist(pin) <= b.dist(pout));
    let (keep_t, keep_p, rest_t, rest_p) = if keep_in { (tin.1, pin, tout.1, pout) } else { (tout.1, pout, tin.1, pin) };
    let (c1, c2) = (land.conductor, land.new);
    for op in &mut land.ops {
        match op {
            Op::AttachEnd { conductor, end: End::B, after, .. } if *conductor == c1 => *after = Some(keep_t),
            Op::CreateConductor { id, conductor } if *id == c2 => {
                conductor.ends[0] = Some(rest_t);
                conductor.properties.remove(ecad_model::connectivity::WIRE_NUMBER);
                conductor.properties.remove(ecad_model::connectivity::WIRE_NUMBER_FIXED);
            }
            Op::ModifyItem { after, .. } => set_route_end(after, keep_p, false),
            Op::AddItem { item, .. } if matches!(item.kind, ItemKind::Primitive(_)) => set_route_end(item, rest_p, true),
            _ => {}
        }
    }
    Ok(land)
}

/// Move a route's first (`start`) or last vertex to `p`.
fn set_route_end(item: &mut Item, p: Pt, start: bool) {
    match &mut item.kind {
        ItemKind::Primitive(Primitive::Line { a, b }) => {
            if start {
                *a = p;
            } else {
                *b = p;
            }
        }
        ItemKind::Primitive(Primitive::Polyline { vertices, .. }) => {
            let v = if start { vertices.first_mut() } else { vertices.last_mut() };
            if let Some(v) = v {
                *v = p;
            }
        }
        _ => {}
    }
}

/// The level a prompt or JSON names for type `t` on strip `c`: a level name (`N`), a number, or
/// `All` (`None`: the whole terminal).
///
/// # Errors
/// A word that names no level, with the type's levels.
pub fn level_word(doc: &Document, c: Option<ComponentId>, t: &tt::TermType, word: &str) -> Result<Option<u32>, String> {
    let w = word.trim();
    if !t.multi() {
        return Ok(Some(1));
    }
    if w.eq_ignore_ascii_case("all") || w.eq_ignore_ascii_case("whole") {
        return Ok(None);
    }
    let names: Vec<String> = match c {
        Some(c) => tt::level_names(doc, c, t),
        None => t.levels.iter().map(|x| x.to_string()).collect(),
    };
    if let Some(i) = names.iter().position(|n| n.eq_ignore_ascii_case(w)) {
        return Ok(u32::try_from(i + 1).ok());
    }
    if let Ok(n) = w.parse::<u32>()
        && (1..=t.count()).contains(&n)
    {
        return Ok(Some(n));
    }
    Err(format!("A {} has levels {}. Type one of them, or All for the whole terminal.", t.name.to_lowercase(), names.join(", ")))
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
    // Brief 036: `type` (Feedthrough, PE, x2, x2B, x3, LNPE, SEN, DIS, FUSE), `level`, `position`.
    let ty_word = opt_typed(params, "type", "a terminal type: Feedthrough, PE, x2, x2B, x3, LNPE, SEN, DIS or FUSE", Value::as_str).map_err(err)?;
    let level_in = opt_typed(params, "level", "a level name, a level number, or All", Value::as_str).map_err(err)?.map(str::to_string);
    let position = opt_typed(params, "position", "a position label", Value::as_str).map_err(err)?.map(str::to_string);
    let ty = match ty_word {
        Some(w) => tt::by_word(w).ok_or_else(|| {
            err(format!(
                "No terminal type \"{}\". Types: Feedthrough, PE, x2, x2B, x3, LNPE, SEN, DIS, FUSE.",
                w.chars().take(24).collect::<String>()
            ))
        })?,
        None if pe => tt::by_code("pe").ok_or_else(|| err("no PE type"))?,
        None => tt::feedthrough(),
    };
    let bad = |q: Pt| !(q.x.abs() <= crate::interact::MAX_COORD && q.y.abs() <= crate::interact::MAX_COORD);
    if bad(from) || bad(to) {
        return Err(err(crate::interact::OUT_OF_RANGE));
    }
    let v = with_project(s, |p, pg, page| {
        if from.near(to, p.tolerance) {
            return Err("`from` and `to` must differ".into());
        }
        let style = plain_style(p);
        let level = match &level_in {
            Some(w) => level_word(p.ledger.document(), strip, ty, w)?,
            None => Some(1),
        };
        let kind = FenceKind { ty, level, position: position.clone() };
        let f = plan_fence_typed(&p.ledger, &mut p.ids, &mut p.tag_high, page, style, strip, kind, from, to, p.tolerance)?;
        let tx = if ty.legacy() { "Place terminals" } else { "Place terminal level" };
        p.apply_edit(pg, "terminals", tx, f.ops.clone())?;
        p.last_placed = Some((ty.starter_key(level), f.strip));
        let doc = p.ledger.document();
        let shown: Vec<String> = f.placed.iter().map(|(t, _, _, _)| crate::interact::terminal_name(doc, *t)).collect();
        let range = match shown.as_slice() {
            [one] => one.clone(),
            [a, .., b] => format!("{a}\u{2026}{b}"),
            [] => String::new(),
        };
        let n = f.placed.len();
        let wires = if n == 1 { "1 wire".to_string() } else { format!("{n} wires") };
        let message = if ty.legacy() {
            format!("Placed {}:{range} on {wires}.", f.tag)
        } else {
            let positions: Vec<&str> = f.placed.iter().map(|(_, l, _, _)| l.as_str()).collect();
            let span = match positions.as_slice() {
                [one] => format!("position {one}"),
                [a, .., b] => format!("positions {a}\u{2013}{b}"),
                [] => String::new(),
            };
            let what = match level {
                Some(l) if ty.multi() => format!(", level {}", tt::level_name(doc, f.strip, ty, l)),
                Some(_) => format!(", {}", ty.name.to_lowercase()),
                None => ", whole terminals".to_string(),
            };
            format!("Placed {}:{range} on {wires} ({span}{what}).", f.tag)
        };
        Ok(json!({
            "strip": f.strip.hex(),
            "tag": f.tag,
            "type": ty.code,
            "terminals": f.placed.iter().zip(&shown).map(|((t, l, c, n), sl)| json!({"terminal": t.hex(), "position": l, "label": sl, "conductor": c.hex(), "new": n.hex()})).collect::<Vec<_>>(),
            "skipped": f.skipped,
            "message": message,
        }))
    })?;
    if let Some(m) = v.get("message").and_then(Value::as_str) {
        s.echo(m.to_string());
    }
    Ok(v)
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
pub(crate) fn run_place_level(
    s: &mut Session, params: &Value, starter: &'static crate::symbols::Starter, at: Pt, degrees: f64, mirror: bool,
) -> cadcraft_engine::Result<Value> {
    let (ty, level) = tt::of_starter(starter.key).ok_or_else(|| err("That symbol is not a terminal level."))?;
    let device: Option<ComponentId> = opt_id(params, "device").map_err(err)?;
    let tag = opt_typed(params, "tag", "a tag", Value::as_str).map_err(err)?.map(str::to_string);
    let position = opt_typed(params, "position", "a position label", Value::as_str).map_err(err)?.map(str::to_string);
    if device.is_some() && tag.is_some() {
        return Err(err("`tag` is for a new strip; `device` joins an existing one"));
    }
    let v = with_project(s, |p, pg, page| {
        let style = plain_style(p);
        let doc = p.ledger.document();
        let not_strip = |c: ComponentId| {
            format!("{} is not a terminal strip. Name a strip (-X01), or leave it out for a new one.", crate::devices::tag_words(doc, c))
        };
        let (strip, new_tag) = match (device, &tag) {
            (Some(c), _) => {
                if !doc.components.contains_key(&c) {
                    return Err("No such device.".into());
                }
                if !strips::is_strip(doc, c) {
                    return Err(not_strip(c));
                }
                (Some(c), None)
            }
            (None, Some(t)) => {
                let t = ecad_model::tag::normalise(t, crate::devices::padding(doc))?;
                match crate::devices::holder_of(doc, &t, None) {
                    Some(c) if strips::is_strip(doc, c) => (Some(c), None),
                    Some(c) => return Err(not_strip(c)),
                    None if !doc.cables_tagged(&t).is_empty() => return Err(format!("Tag {t} is a cable's; choose another tag for a strip.")),
                    None => (None, Some(t)),
                }
            }
            (None, None) => (None, None),
        };
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
            "terminals": shown.iter().map(|(k, t, _)| json!({"key": k, "label": crate::interact::terminal_name(doc, *t), "terminal": t.hex()})).collect::<Vec<_>>(),
            "message": message,
        });
        if let Some(l) = &land {
            v["split"] = json!({"conductor": l.conductor.hex(), "new": l.new.hex()});
        }
        Ok(v)
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

/// `stripplan {strip?, limit?, after?}`: the strips in tag order (or one), paged by rows: `after`
/// rows are skipped and at most `limit` returned, a strip split across pages carrying
/// `first_row`; `total` counts the rows, `next` is the next `after`.
fn run_stripplan(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let strip: Option<ComponentId> = opt_id(params, "strip").map_err(err)?;
    let limit = opt_typed(params, "limit", "a whole number", Value::as_u64)
        .map_err(err)?
        .map_or(crate::outputs::DEFAULT_LIMIT, |n| usize::try_from(n).unwrap_or(usize::MAX))
        .clamp(1, crate::outputs::MAX_LIMIT);
    let after = opt_typed(params, "after", "a whole number", Value::as_u64).map_err(err)?.map_or(0, |n| usize::try_from(n).unwrap_or(usize::MAX));
    let accessories = opt_typed(params, "accessories", "true or false", Value::as_bool).map_err(err)?.unwrap_or(true);
    with_any_page(s, |p, _| {
        let style = p.ledger.document().settings.reference_style;
        let o = p.outputs();
        let shown: Vec<&StripData> = match strip {
            Some(c) => vec![o.strips.get(c).ok_or("No such terminal strip.")?],
            None => o.sorted_strips(),
        };
        let len = |d: &StripData| if accessories { d.rows.len() + d.acc.gap_rows() } else { d.rows.len() };
        let total: usize = shown.iter().map(|d| len(d)).sum();
        let mut out = Vec::new();
        let (mut skip, mut left) = (after, limit);
        for d in &shown {
            let n = len(d);
            if skip >= n && n > 0 {
                skip -= n;
                continue;
            }
            if left == 0 {
                break;
            }
            let end = (skip + left).min(n);
            out.push(strip_json(d, style, skip, end, accessories));
            left -= end - skip;
            skip = 0;
        }
        let mut v = json!({"strips": out, "total": total, "fields": plan_fields_json()});
        let shown_rows = limit - left;
        if after.saturating_add(shown_rows) < total {
            v["next"] = json!(after + shown_rows);
        }
        Ok(v)
    })
}

fn run_cablecores(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let k: CableId = id(params, "cable").map_err(err)?;
    with_any_page(s, |p, _| {
        p.outputs();
        let o = p.outputs.as_ref().ok_or("The reports could not be built.")?;
        let row = o.cable_row(k).cloned().ok_or("no such cable")?;
        let wire = |w: ConductorId| o.wire(w).cloned();
        let doc = p.ledger.document();
        let style = doc.settings.reference_style;
        let count = &row.count;
        let member_json = |i: usize, label: Option<String>, state: &str| {
            let w = row.members.get(i).copied();
            let r = w.and_then(wire);
            let mut o = serde_json::Map::new();
            put(&mut o, "label", label);
            o.insert("state".into(), json!(state));
            put(&mut o, "conductor", w.map(|w| w.hex()));
            put(&mut o, "wire", r.as_ref().and_then(|r| r.wire_number.clone()));
            put(&mut o, "from", r.as_ref().and_then(|r| r.from.text(style)));
            put(&mut o, "to", r.as_ref().and_then(|r| r.to.text(style)));
            Value::Object(o)
        };
        let stored = |i: usize| doc.cables.get(&k).and_then(|c| c.members.get(i)).and_then(|m| m.core.clone());
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
