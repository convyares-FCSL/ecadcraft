//! The rules of brief 028's commands (design 028 §3, §4, §10) and brief 036's levels: Place
//! Terminals (`connection_points`, one terminal or level on each wire a line crosses), Draw Bridge
//! (`bridge`), and the paging of the terminal plan (`stripplan`). Each function reads the document
//! and returns the ops to commit and what to report; the engine crate's shims read the parameters,
//! build the reports, commit and answer in JSON.

use crate::terminal_strips::strip_model::{StripData, TermRow};
use ecad_model::geometry::units::Radians;
use ecad_model::{LevelIo, PointKey};
use serde_json::{Value, json};
use std::collections::BTreeMap;

use ecad_model::{
    ConnectionPoint, ConnectionPointId, DefinitionId, DeviceId, Document, Form, IdAllocator, Item, ItemId, ItemKind, Ledger, Noun, Op, PageId,
    Placement, Pt, Representation, RepresentationId, RouteEnd, RouteId, Style, Subject, SymbolInstance, WireId,
};

use crate::connectors::{Land, plan_land, plan_land_ends, strip_connection_point_at};
use crate::devices::tags::TagHigh;
use crate::error::CmdError;
use crate::terminal_strips::terminal_blocks as tt;
use crate::terminal_strips::{self, connection_point_words};
use crate::wires::placing::OnRoute;

/// Most wires one Place Terminals line lands on (design 028 §3.2).
pub const MAX_FENCE: usize = 200;
/// How far a bridge's staple stands off its terminals, in millimetres (design 028 §4).
pub const BRIDGE_STANDOFF: f64 = 4.0;

/// The definition ID of starter `s` in `doc`, embedding it in `ops` when it is not yet there.
///
/// # Errors
/// The starter's definition could not be built.
pub fn definition_for(doc: &Document, ids: &mut IdAllocator, s: &crate::symbols::Starter, ops: &mut Vec<Op>) -> Result<DefinitionId, CmdError> {
    if let Some(d) = crate::symbols::catalogue::embedded(doc, s) {
        return Ok(d);
    }
    if let Some(d) = ops.iter().find_map(|o| match o {
        Op::EmbedDefinition { id, definition } if crate::symbols::catalogue::of_definition(definition).is_some_and(|x| x.key == s.key) => Some(*id),
        _ => None,
    }) {
        return Ok(d);
    }
    let d: DefinitionId = ids.fresh();
    ops.push(Op::EmbedDefinition { id: d, definition: s.definition()?.clone() });
    Ok(d)
}

/// Plan one terminal symbol of `strip` labelled `label` at `at` (a join: its own connection
/// point keyed by its label, as 018's joins are). `first`: a new strip's first symbol, which shows
/// the definition's own point. Every symbol's role is its starter's part (design 072 §4.4).
#[allow(clippy::too_many_arguments)]
fn terminal_block_symbol(
    ids: &mut IdAllocator, page: PageId, style: Style, def: DefinitionId, strip: DeviceId, label: &str, at: Pt, first: bool, role: ecad_model::Role,
    ops: &mut Vec<Op>,
) -> ConnectionPointId {
    let (t, item, rep): (ConnectionPointId, ItemId, RepresentationId) = (ids.fresh(), ids.fresh(), ids.fresh());
    let key = PointKey::Def(if first { "1".to_string() } else { label.to_string() });
    ops.push(Op::AddConnectionPoint {
        id: t,
        connection_point: ConnectionPoint { owner: strip.into(), label: Some(label.to_string()), key: Some(key), accepts: Default::default() },
    });
    let placement = Placement { origin: at, rotation: Radians(0.0), mirror: false, scale: 1.0 };
    ops.push(Op::AddItem { id: item, item: Item { page, style, kind: ItemKind::Symbol(SymbolInstance { definition: def, placement }) } });
    let mut cps = BTreeMap::new();
    if !first {
        cps.insert(t, Pt::new(0.0, 0.0));
    }
    ops.push(Op::LinkRepresentation {
        id: rep,
        rep: Representation { subject: Subject::Device(strip), page, form: Form::Symbol, role, items: vec![item], connection_points: cps },
    });
    t
}

/// A planned Place Terminals line.
#[derive(Debug, Clone)]
pub struct Fence {
    pub ops: Vec<Op>,
    pub strip: DeviceId,
    pub tag: String,
    /// (terminal, label, route, its new second part) in crossing order.
    pub placed: Vec<(ConnectionPointId, String, RouteId, RouteId)>,
    /// Wires skipped because a terminal of the strip is already there.
    pub skipped: usize,
}

/// Plan Place Terminals (design 028 §3.2) from `from` to `to` on `page`.
///
/// # Errors
/// No wire crossed, more than [`MAX_FENCE`], an unknown strip, or a refusal of one landing.
#[allow(clippy::too_many_arguments)]
pub fn plan_fence(
    ledger: &Ledger, ids: &mut IdAllocator, high: &mut TagHigh, page: PageId, style: Style, strip: Option<DeviceId>, pe: bool, from: Pt, to: Pt,
    tol: f64,
) -> Result<Fence, CmdError> {
    let ty = if pe { tt::by_code("pe") } else { Some(tt::feedthrough()) }.ok_or(CmdError::refused("no PE type"))?;
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
    ledger: &Ledger, ids: &mut IdAllocator, high: &mut TagHigh, page: PageId, style: Style, strip: Option<DeviceId>, kind: FenceKind, from: Pt,
    to: Pt, tol: f64,
) -> Result<Fence, CmdError> {
    let doc = ledger.document();
    let found = crate::cables::slash::crossings(doc, page, from, to, tol);
    if found.members.len() > MAX_FENCE {
        return Err(CmdError::refused(format!("That line crosses {} wires; at most {MAX_FENCE} at once. Draw a shorter line.", found.members.len())));
    }
    let legacy = kind.ty.legacy();
    let starter = crate::symbols::catalogue::starter(kind.ty.starter_key(kind.level));
    if let Some(c) = strip
        && terminal_strips::strip_of(doc, c).is_none()
    {
        return Err(CmdError::refused("That device is not a terminal strip. Press Enter for a new strip, or type the tag of one (-X01)."));
    }
    let mut ops = Vec::new();
    let def = definition_for(doc, ids, starter, &mut ops)?;
    let mut crossings = Vec::new();
    let mut skipped = 0;
    for m in &found.members {
        if let Some(x) = strip_connection_point_at(doc, page, m.at)
            && strip.is_some_and(|c| doc.connection_points.get(&x).is_some_and(|t| t.device() == Some(c)))
        {
            skipped += 1;
            continue;
        }
        crossings.push(*m);
    }
    if crossings.is_empty() {
        return Err(CmdError::refused(if skipped > 0 {
            "Every wire the line crosses already has a terminal of this strip there."
        } else {
            "The line crosses no wire. Draw it across the wires to land."
        }));
    }
    let mut chooser = tt::Chooser::new(doc, strip);
    let existing = strip;
    let (strip, tag, mut next, mut first_main) = match strip {
        Some(c) => (c, crate::devices::tags::tag_words(doc, c), tt::highest_number(doc, c) + 1, false),
        None => {
            let c: DeviceId = ids.fresh();
            // Brief 026: a provisional `-X?n` when provisional tags are on, as Place gives.
            let given = high.next_auto(ledger, &crate::devices::prefix_for(doc, starter))?;
            ops.push(Op::CreateDevice {
                id: c,
                device: crate::devices::with_given(
                    ecad_model::Device {
                        description: crate::devices::description_of("Terminal"),
                        definition: Some(def),
                        device_type: starter.device_type(),
                        ..ecad_model::Device::default()
                    },
                    &given,
                )?,
            });
            (c, given.shown(), 1, legacy)
        }
    };
    let mut placed = Vec::new();
    let mut named = kind.position.clone();
    let on_wire = |e: CmdError, c: ecad_model::CorePosition| CmdError::refused(format!("{e} ({})", crate::cables::slash::position_name(doc, c)));
    for m in crossings {
        let on = crate::wires::placing::on_route_of(doc, m.route, m.at).ok_or(CmdError::refused("A crossed wire has no route here."))?;
        if legacy {
            let label = next.to_string();
            let t = terminal_block_symbol(
                ids,
                page,
                style.clone(),
                def,
                strip,
                &label,
                m.at,
                first_main,
                ecad_model::Role::Part(starter.part_role()),
                &mut ops,
            );
            first_main = false;
            let land = plan_land(doc, ids, on, t).map_err(|e| on_wire(e, m.position))?;
            ops.extend(land.ops);
            placed.push((t, label, land.route, land.new));
            next += 1;
            continue;
        }
        let position = chooser.choose(doc, existing, kind.ty, kind.level, named.take().as_deref())?;
        let placement = Placement { origin: m.at, rotation: Radians(0.0), mirror: false, scale: 1.0 };
        let shown = level_symbol(ids, page, style.clone(), def, starter, strip, &position, placement, &mut ops)?;
        let land = land_level(doc, ids, on, &shown, placement).map_err(|e| on_wire(e, m.position))?;
        ops.extend(land.ops);
        let t = shown.first().map(|(_, t, _)| *t).ok_or(CmdError::refused("The symbol has no connection point."))?;
        placed.push((t, position, land.route, land.new));
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
    ids: &mut IdAllocator, page: PageId, style: Style, def: DefinitionId, starter: &crate::symbols::Starter, strip: DeviceId, position: &str,
    placement: Placement, ops: &mut Vec<Op>,
) -> Result<Vec<(PointKey, ConnectionPointId, Pt)>, CmdError> {
    let d = starter.definition()?;
    let (item, rep): (ItemId, RepresentationId) = (ids.fresh(), ids.fresh());
    let mut cps = BTreeMap::new();
    let mut out = Vec::new();
    for td in &d.connection_points {
        let t: ConnectionPointId = ids.fresh();
        ops.push(Op::AddConnectionPoint {
            id: t,
            connection_point: ConnectionPoint {
                owner: strip.into(),
                label: Some(position.to_string()),
                key: Some(td.key.clone()),
                accepts: td.accepts,
            },
        });
        cps.insert(t, td.at);
        out.push((td.key.clone(), t, td.at));
    }
    ops.push(Op::AddItem { id: item, item: Item { page, style, kind: ItemKind::Symbol(SymbolInstance { definition: def, placement }) } });
    ops.push(Op::LinkRepresentation {
        id: rep,
        rep: Representation {
            subject: Subject::Device(strip),
            page,
            form: Form::Symbol,
            role: ecad_model::Role::Part(starter.part_role()),
            items: vec![item],
            connection_points: cps,
        },
    });
    Ok(out)
}

/// Land a level symbol on its route: a one-point level as 028 does; a two-point level splits the
/// route into two nets (design 036 §3.4): the part towards the in point ends on `.1i`, the other
/// on `.1o`, each drawn to its point. The commit's rewrite makes the wires of both nets.
///
/// # Errors
/// [`plan_land`]'s.
pub fn land_level(
    doc: &Document, ids: &mut IdAllocator, on: OnRoute, shown: &[(PointKey, ConnectionPointId, Pt)], placement: Placement,
) -> Result<Land, CmdError> {
    let first = shown.first().map(|(_, t, _)| *t).ok_or(CmdError::refused("The symbol has no connection point."))?;
    let io = |want: LevelIo| shown.iter().find(|(k, _, _)| matches!(k.level(), Some((_, Some(io))) if io == want));
    let (Some(tin), Some(tout)) = (io(LevelIo::In), io(LevelIo::Out)) else {
        return plan_land(doc, ids, on, first);
    };
    let (pin, pout) = (placement.apply(tin.2), placement.apply(tout.2));
    // Which part comes from the in point's side: the kept part's vertex before the split.
    let before = doc.routes.get(&on.route).and_then(|r| r.points.get(on.segment).copied());
    let keep_in = before.is_none_or(|b| b.dist(pin) <= b.dist(pout));
    let (keep_t, keep_p, rest_t, rest_p) = if keep_in { (tin.1, pin, tout.1, pout) } else { (tout.1, pout, tin.1, pin) };
    plan_land_ends(doc, ids, on, (RouteEnd::Point(keep_t), keep_p), (RouteEnd::Point(rest_t), rest_p))
}

/// The level a prompt or JSON names for type `t` on strip `c`: a level name (`N`), a number, or
/// `All` (`None`: the whole terminal).
///
/// # Errors
/// A word that names no level, with the type's levels.
pub fn level_word(doc: &Document, c: Option<DeviceId>, t: &tt::TermType, word: &str) -> Result<Option<u32>, CmdError> {
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
    Err(CmdError::refused(format!("A {} has levels {}. Type one of them, or All for the whole terminal.", t.name.to_lowercase(), names.join(", "))))
}

// ---------------------------------------------------------------- `connection_points`

/// The terminal type a `connection_points` call names: `type` (Feedthrough, PE, x2, x2B, x3, LNPE, SEN, DIS,
/// FUSE), else PE with `pe`, else feed-through.
///
/// # Errors
/// A word that names no type.
pub fn term_type(word: Option<&str>, pe: bool) -> Result<&'static tt::TermType, CmdError> {
    match word {
        Some(w) => tt::by_word(w).ok_or_else(|| {
            CmdError::refused(format!(
                "No terminal type \"{}\". Types: Feedthrough, PE, x2, x2B, x3, LNPE, SEN, DIS, FUSE.",
                w.chars().take(24).collect::<String>()
            ))
        }),
        None if pe => tt::by_code("pe").ok_or(CmdError::refused("no PE type")),
        None => Ok(tt::feedthrough()),
    }
}

/// A `connection_points` call, as given.
#[derive(Debug, Clone)]
pub struct TerminalBlocksInput {
    /// The strip joined (`None`: a new strip).
    pub strip: Option<DeviceId>,
    pub ty: &'static tt::TermType,
    /// The level word (`None`: level 1).
    pub level: Option<String>,
    /// The position for the first crossing.
    pub position: Option<String>,
    pub from: Pt,
    pub to: Pt,
}

/// What `connection_points` records: the fence and the level it placed.
#[derive(Debug, Clone)]
pub struct TerminalBlocks {
    pub fence: Fence,
    pub level: Option<u32>,
    /// The transaction's label.
    pub label: &'static str,
}

/// `connection_points` (design 028 §3.2, brief 036 §3.2): one terminal, or one level, on each wire the line
/// `from`–`to` crosses on `page`.
///
/// # Errors
/// `from` and `to` the same point, a level word that names no level, or [`plan_fence_typed`]'s.
pub fn plan_terminal_blocks(
    ledger: &Ledger, ids: &mut IdAllocator, high: &mut TagHigh, page: PageId, style: Style, input: TerminalBlocksInput, tol: f64,
) -> Result<TerminalBlocks, CmdError> {
    if input.from.near(input.to, tol) {
        return Err(CmdError::input("`from` and `to` must differ"));
    }
    let level = match &input.level {
        Some(w) => level_word(ledger.document(), input.strip, input.ty, w)?,
        None => Some(1),
    };
    let kind = FenceKind { ty: input.ty, level, position: input.position };
    let fence = plan_fence_typed(ledger, ids, high, page, style, input.strip, kind, input.from, input.to, tol)?;
    let label = if input.ty.legacy() { "Place terminals" } else { "Place terminal level" };
    Ok(TerminalBlocks { fence, level, label })
}

/// The shown labels of `f`'s terminals once committed, and the echo of the line (design 028 §3.2,
/// brief 036 §3.2): the range placed, the wires, positions and level, and any stored wire number
/// an out half did not keep (review M1: said, never dropped silently).
pub fn connection_points_message(doc: &Document, ty: &tt::TermType, level: Option<u32>, f: &Fence) -> (Vec<String>, String) {
    let shown: Vec<String> = f.placed.iter().map(|(t, _, _, _)| tt::connection_point_name(doc, *t)).collect();
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
    (shown, message)
}

// ---------------------------------------------------------------- Draw Bridge (§4)

/// The staple of a bridge from `a` to `b` (design 028 §4): 4 mm towards the inside (up; left when
/// the terminals sit one above the other), then across.
pub fn staple(a: Pt, b: Pt) -> Vec<Pt> {
    let (dx, dy) = ((b.x - a.x).abs(), (b.y - a.y).abs());
    let (ox, oy) = if dy > dx { (-BRIDGE_STANDOFF, 0.0) } else { (0.0, BRIDGE_STANDOFF) };
    vec![a, Pt::new(a.x + ox, a.y + oy), Pt::new(b.x + ox, b.y + oy), b]
}

/// What `bridge` does.
#[derive(Debug, Clone, PartialEq)]
pub enum Bridge {
    /// A wire already joins the two: nothing to record.
    Already { conductor: WireId, same: bool, message: String },
    /// A new route between the two (a bridge when both are of one strip); the commit's rewrite
    /// makes its wire.
    Drawn { ops: Vec<Op>, label: String, route: RouteId, same: bool, message: String },
}

/// `bridge {from, to}` (design 028 §4): a wire between two terminals shown on `page`; of one strip
/// it is a bridge, drawn as a staple.
///
/// # Errors
/// No such terminal, the same terminal twice, or one not on this page.
pub fn plan_bridge(
    doc: &Document, ids: &mut IdAllocator, page: PageId, style: Style, a: ConnectionPointId, b: ConnectionPointId,
) -> Result<Bridge, CmdError> {
    if !doc.connection_points.contains_key(&a) || !doc.connection_points.contains_key(&b) {
        return Err(CmdError::NotFound(Noun::TerminalBlock));
    }
    if a == b {
        return Err(CmdError::refused("Pick two different terminals: a bridge jumper bars one terminal to another."));
    }
    let (wa, wb) = (connection_point_words(doc, a), connection_point_words(doc, b));
    let (sa, sb) = (terminal_strips::strip_of_connection_point(doc, a), terminal_strips::strip_of_connection_point(doc, b));
    let same = sa.is_some() && sa == sb;
    if let Some(w) = crate::wires::routes::wire_between(doc, a, b) {
        return Ok(Bridge::Already { conductor: w, same, message: format!("{wa} and {wb} are already bridged.") });
    }
    let pos = |t: ConnectionPointId| {
        doc.representations_of(doc.connection_points.get(&t)?.owner.subject())
            .find_map(|(_, r)| (r.page == page).then(|| ecad_model::connectivity::derive::connection_point_position(doc, page, t)).flatten())
    };
    let (Some(pa), Some(pb)) = (pos(a), pos(b)) else {
        return Err(CmdError::refused(format!(
            "{wa} and {wb} must both be on this page to draw a bridge; open the page that shows both, or draw a wire."
        )));
    };
    let points = if same { staple(pa, pb) } else { vec![pa, pb] };
    let (route, _, ops) = crate::wires::routes::add_route(ids, page, style, points, [RouteEnd::Point(a), RouteEnd::Point(b)]);
    let label_b = tt::connection_point_name(doc, b);
    let label = if same { format!("Bridge {wa}\u{2013}{label_b}") } else { "Wire".to_string() };
    let message = if same {
        format!("Bridged {wa}\u{2013}{label_b}.")
    } else {
        format!("{wa} and {wb} are on different strips; drawn as a wire, not a bridge.")
    };
    Ok(Bridge::Drawn { ops, label, route, same, message })
}

// ---------------------------------------------------------------- the terminal plan (§10)

/// One page of the terminal plan: for each strip shown (by index), its rows `first..end`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanPage {
    pub parts: Vec<(usize, usize, usize)>,
    /// Rows of every strip shown.
    pub total: usize,
    /// The `after` of the next page, when there is one.
    pub next: Option<usize>,
}

/// `stripplan`'s paging (design 028 §10): strips of `lens` rows each, in order; `after` rows are
/// skipped and at most `limit` (at least 1) shown, a strip split across pages starting part-way.
pub fn plan_page(lens: &[usize], after: usize, limit: usize) -> PlanPage {
    let total: usize = lens.iter().sum();
    let mut parts = Vec::new();
    let (mut skip, mut left) = (after, limit);
    for (i, n) in lens.iter().copied().enumerate() {
        if skip >= n && n > 0 {
            skip -= n;
            continue;
        }
        if left == 0 {
            break;
        }
        let end = (skip + left).min(n);
        parts.push((i, skip, end));
        left -= end.saturating_sub(skip);
        skip = 0;
    }
    let shown_rows = limit - left;
    let next = (after.saturating_add(shown_rows) < total).then(|| after + shown_rows);
    PlanPage { parts, total, next }
}

/// A side cell as JSON: `conductor`, and `target`, `wire` (and outside `cable`, `core`) when known.
pub(crate) fn side_json(c: &terminal_strips::strip_model::SideCell, outside: bool) -> Value {
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

/// [`PLAN_FIELDS`] as JSON: `[{group, key, editable, command?}]`.
pub(crate) fn plan_fields_json() -> Value {
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
    put(&mut o, "connection_point", r.connection_point.map(|t| t.hex()));
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
    let mut editable = vec!["position", "label", "jumper_bars", "marker"];
    if r.breaks() {
        editable.push("rating");
    }
    o.insert("editable".into(), json!(editable));
    Value::Object(o)
}

/// A gap row as JSON (design 030 §7.2).
pub(crate) fn gap_json(s: &StripData, gap: &str, a: crate::terminal_strips::accessories::Acc) -> Value {
    let mut o = serde_json::Map::new();
    o.insert("kind".into(), json!("accessory"));
    o.insert("state".into(), json!("accessory"));
    o.insert("accessory".into(), json!(a.key()));
    o.insert("gap".into(), json!(gap));
    put(&mut o, "part_number", s.acc.gap_part(a, crate::reports::gap_pe(s, gap)));
    put(&mut o, "maker", s.acc.makers.get(a.key()));
    o.insert("connection_point".into(), Value::Null);
    o.insert("label".into(), Value::Null);
    Value::Object(o)
}

/// One strip as JSON with plan rows `first..end` of it (`first_row` when it does not start at
/// 0). With `accessories` (brief 030) every row has `kind`, gap rows are in, terminal rows carry
/// `marker` and `jumper_bars`, and the strip carries `accessories` and `jumper_bars`; without, 028's rows.
pub(crate) fn strip_json(s: &StripData, style: ecad_model::ReferenceStyle, first: usize, end: usize, accessories: bool) -> Value {
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
            terminal_strips::strip_model::PlanItem::Gap { gap, acc } => gap_json(s, gap, *acc),
            terminal_strips::strip_model::PlanItem::Term(i, r) => {
                let mut v = row_json(r, style);
                if accessories {
                    v["kind"] = json!("terminal_block");
                    v["marker"] = json!(s.acc.markers.contains(&r.label));
                    let jumper_bars: Vec<Value> = s
                        .jumper_bars_at(*i)
                        .iter()
                        .filter(|(_, on)| *on)
                        .map(|(l, _)| json!({"jumper_bar": l.n, "lane": l.lane, "level": l.level}))
                        .collect();
                    v["jumper_bars"] = Value::Array(jumper_bars);
                }
                v
            }
        })
        .collect();
    o.insert("rows".into(), Value::Array(rows));
    if accessories {
        let counts: serde_json::Map<String, Value> =
            crate::terminal_strips::accessories::Acc::ALL.iter().map(|a| (a.key().to_string(), json!(s.acc.count(*a)))).collect();
        o.insert("accessories".into(), Value::Object(counts));
        let jumper_bars: Vec<Value> = s
            .acc
            .jumper_bars
            .iter()
            .map(|l| {
                let mut v = json!({"jumper_bar": l.n, "positions": l.text, "span": l.span, "lane": l.lane, "bars": l.bars, "level": l.level});
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
        o.insert("jumper_bars".into(), Value::Array(jumper_bars));
    }
    Value::Object(o)
}

/// Put `v` under `key` in `o` when it is there (absent keys are left out, never null).
pub fn put<T: serde::Serialize>(o: &mut serde_json::Map<String, Value>, key: &str, v: Option<T>) {
    if let Some(v) = v {
        o.insert(key.into(), json!(v));
    }
}

/// The terminal plan's fields (design 036 §9): the row model the terminal manager (2.6, the U3
/// table) and strip pages (1.8) draw. Each is `(group, key, editable, the command that writes it)`;
/// the groups mirror the strip: `inside` | `strip` | `outside`, and `either` for the row's place.
pub(crate) const PLAN_FIELDS: &[(&str, &str, bool, Option<&str>)] = &[
    ("inside", "inside[].target", false, None),
    ("inside", "inside[].wire", false, None),
    ("inside", "inside[].conductor", false, None),
    ("strip", "position", true, Some("terminalblocklabel {position}")),
    ("strip", "level", false, None),
    ("strip", "level_name", false, None),
    ("strip", "label", true, Some("terminalblocklabel {text}")),
    ("strip", "type", false, None),
    ("strip", "type_name", false, None),
    ("strip", "state", false, None),
    ("strip", "rating", true, Some("setprop rating.<position>")),
    ("strip", "jumper_bars", true, Some("jumper")),
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
    ("either", "connection_point", false, None),
];
