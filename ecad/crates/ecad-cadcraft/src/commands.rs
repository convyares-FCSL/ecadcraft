//! ECAD commands (Path A). JSON forms only in the experiment: they build transactions from
//! resolved inputs (IDs, coordinates) and commit them through [`Project::apply_edit`].
//!
//! They reach the command line, scripts, the control channel and MCP through the engine's
//! extension command registry. Interactive prompting versions are not part of the experiment.

use std::collections::BTreeMap;

use cadcraft_engine::doc::Handle;
use cadcraft_engine::{CommandSpec, EngineError, Session};
use ecad_model::{
    CableId, CableMember, Color, Component, ComponentId, Conductor, ConductorId, Definition, DefinitionId, End, Form,
    Item, ItemId, ItemKind, ObjectRef, Op, Page, PageId, Placement, Primitive, Pt, Representation, RepresentationId,
    Style, Subject, SymbolInstance, Terminal, TerminalDef, TerminalId,
};
use serde_json::{Value, json};

use crate::access::{Pages, owner_of_active, try_lock};
use crate::project::{Mapped, Project};
use crate::translate::terminal_at;

/// The commands this crate adds.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("convertcomponent", "Convert to Component", run_convert)
            .params("{handles?: [hex]} (default: selection)")
            .noundo(),
        CommandSpec::new("addterminal", "Add Terminal", run_addterminal)
            .params("{handle: hex (an item of the component), at: [x,y], label?}")
            .noundo(),
        CommandSpec::new("wire", "Wire", run_wire).params("{points: [[x,y], …]} (ends on terminals attach)").noundo(),
        CommandSpec::new("connect", "Connect", run_connect)
            .params("{conductor: id, end: \"A\"|\"B\", terminal: id}")
            .noundo(),
        CommandSpec::new("disconnect", "Disconnect", run_disconnect)
            .params("{conductor: id, end: \"A\"|\"B\"}")
            .noundo(),
        CommandSpec::new("ecaddelete", "Delete Electrical Object", run_delete)
            .params("{conductor: id} | {component: id}")
            .noundo(),
        CommandSpec::new("setprop", "Set Property", run_setprop)
            .params("{component|conductor|cable: id, key, value: string|null}")
            .noundo(),
        CommandSpec::new("cableadd", "Add to Cable", run_cableadd)
            .params("{cable?: id, conductors: [id], tag?}")
            .noundo(),
        CommandSpec::new("place", "Place Symbol", run_place)
            .params("{at: [x,y], rotation?: degrees, mirror?: bool, definition?: id}")
            .noundo(),
        CommandSpec::new("newpage", "New Page", run_newpage).params("{title}").noundo(),
        CommandSpec::new("openpage", "Open Page", run_openpage).params("{page: id}").noundo(),
        CommandSpec::new("ecadinspect", "Inspect Project", run_inspect)
            .params("{seq?: n} (read-only snapshot)")
            .noundo(),
    ]
}

fn err(m: impl Into<String>) -> EngineError {
    EngineError::Other(m.into())
}

/// Run `f` with the project owning the active document. The closure gets [`Pages`], not the
/// session, so it cannot call an owner-aware session method while the project is locked; the lock
/// is taken with `try_lock`, so a re-entrant command is refused instead of deadlocking.
pub(crate) fn with_project<R>(
    s: &mut Session,
    f: impl FnOnce(&mut Project, &mut Pages<'_>, PageId) -> Result<R, String>,
) -> cadcraft_engine::Result<R> {
    let owner = owner_of_active(s).map_err(err)?;
    let uid = s.state()?.uid;
    let mut guard = try_lock(&owner).map_err(err)?;
    let p =
        guard.as_any_mut().downcast_mut::<Project>().ok_or_else(|| err("this page is not owned by an ECAD project"))?;
    let page = p.page_of(uid).ok_or_else(|| err("page view not found"))?;
    if p.pages.get(&page).is_some_and(|v| v.unavailable) {
        return Err(err(UNAVAILABLE));
    }
    f(p, &mut Pages(s), page).map_err(EngineError::Other)
}

/// Like [`with_project`] for commands that also work on an unavailable page (open, inspect).
fn with_any_page<R>(
    s: &mut Session,
    f: impl FnOnce(&mut Project, &mut Pages<'_>) -> Result<R, String>,
) -> cadcraft_engine::Result<R> {
    let owner = owner_of_active(s).map_err(err)?;
    let mut guard = try_lock(&owner).map_err(err)?;
    let p =
        guard.as_any_mut().downcast_mut::<Project>().ok_or_else(|| err("this page is not owned by an ECAD project"))?;
    f(p, &mut Pages(s)).map_err(EngineError::Other)
}

/// Refusal for edits on a page whose drawing could not be rebuilt (PROJ-02).
pub const UNAVAILABLE: &str = "this page is unavailable (its drawing could not be rebuilt); close and reopen it";

fn id<T: From<u128>>(p: &Value, key: &str) -> Result<T, String> {
    p.get(key)
        .and_then(Value::as_str)
        .and_then(|t| u128::from_str_radix(t.trim(), 16).ok())
        .map(T::from)
        .ok_or_else(|| format!("`{key}` (hex id) is required"))
}

/// An optional ID parameter: absent or `null` is `None`; anything else must be a valid hex ID, so a
/// typo is an error rather than "not given".
fn opt_id<T: From<u128>>(p: &Value, key: &str) -> Result<Option<T>, String> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(_) => id(p, key).map(Some),
    }
}

/// A list of IDs: every entry must parse, and the list may hold at most `max`; checked before
/// anything is collected.
fn id_list<T: From<u128>>(p: &Value, key: &str, max: usize) -> Result<Vec<T>, String> {
    let a = p.get(key).and_then(Value::as_array).ok_or_else(|| format!("`{key}` (array of hex ids) is required"))?;
    if a.len() > max {
        return Err(format!("at most {max} entries in `{key}` per call"));
    }
    a.iter()
        .map(|v| {
            v.as_str()
                .and_then(|t| u128::from_str_radix(t.trim(), 16).ok())
                .map(T::from)
                .ok_or_else(|| format!("`{v}` in `{key}` is not a valid hex id"))
        })
        .collect()
}

/// An optional parameter of a given JSON type: absent or `null` is `None`, a wrong type is an error.
fn opt_typed<'a, T>(
    p: &'a Value,
    key: &str,
    what: &str,
    get: impl Fn(&'a Value) -> Option<T>,
) -> Result<Option<T>, String> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => get(v).map(Some).ok_or_else(|| format!("`{key}` must be {what}")),
    }
}

/// Most points one WIRE accepts.
const MAX_WIRE_POINTS: usize = 10_000;

fn point(v: &Value) -> Option<Pt> {
    let a = v.as_array()?;
    let (x, y) = (a.first()?.as_f64()?, a.get(1)?.as_f64()?);
    (x.is_finite() && y.is_finite()).then_some(Pt::new(x, y))
}

fn end_param(p: &Value) -> Result<End, String> {
    match p.get("end").and_then(Value::as_str) {
        Some("A" | "a") => Ok(End::A),
        Some("B" | "b") => Ok(End::B),
        _ => Err("`end` must be \"A\" or \"B\"".into()),
    }
}

/// Most handles one call may name.
const MAX_HANDLES: usize = 100_000;

/// The `handles` parameter, or the selection when it is absent. Every entry must be a valid hex
/// handle: a typo is an error, never a silently smaller set.
fn handles(s: &Session, p: &Value) -> Result<Vec<Handle>, String> {
    let Some(v) = p.get("handles") else { return Ok(s.selection()) };
    let a = v.as_array().ok_or("`handles` must be an array of hex handles")?;
    if a.len() > MAX_HANDLES {
        return Err(format!("at most {MAX_HANDLES} handles per call"));
    }
    a.iter()
        .map(|h| h.as_str().and_then(Handle::parse_hex).ok_or_else(|| format!("`{h}` is not a valid handle")))
        .collect()
}

fn plain_style(p: &Project) -> Style {
    Style { layer: p.default_layer, color: Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 }
}

fn run_convert(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let hs = handles(s, params).map_err(err)?;
    with_project(s, |p, pg, page| {
        let map = &p.pages.get(&page).ok_or("page view not found")?.map;
        let mut items = Vec::new();
        for h in hs {
            match map.get(h) {
                Some(Mapped::Item(i)) if p.ledger.document().representation_of_item(i).is_none() => items.push(i),
                Some(_) => {
                    return Err(
                        "only plain geometry can be converted; part of the selection already has a meaning".into()
                    );
                }
                None => return Err("the selection holds an object this page does not know".into()),
            }
        }
        if items.is_empty() {
            return Err("select the geometry to convert".into());
        }
        let (c, r): (ComponentId, RepresentationId) = (p.ids.fresh(), p.ids.fresh());
        let ops = vec![
            Op::CreateComponent { id: c, component: Component::default() },
            Op::LinkRepresentation {
                id: r,
                rep: Representation {
                    subject: Subject::Component(c),
                    page,
                    form: Form::Drawn,
                    role: "main".into(),
                    items,
                    connection_points: BTreeMap::new(),
                },
            },
        ];
        p.apply_edit(pg, "convertcomponent", "Convert to component", ops)?;
        Ok(json!({"component": c.hex(), "representation": r.hex()}))
    })
}

fn run_addterminal(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let h = params
        .get("handle")
        .and_then(Value::as_str)
        .and_then(Handle::parse_hex)
        .ok_or_else(|| err("`handle` is required"))?;
    let at = params.get("at").and_then(point).ok_or_else(|| err("`at` [x,y] is required"))?;
    let label = params.get("label").and_then(Value::as_str).map(str::to_string);
    with_project(s, |p, pg, page| {
        let map = &p.pages.get(&page).ok_or("page view not found")?.map;
        let Some(Mapped::Item(item)) = map.get(h) else { return Err("pick an item of a component".into()) };
        let doc = p.ledger.document();
        let (rid, r) = doc.representation_of_item(item).ok_or("that item is not part of a component")?;
        let Subject::Component(c) = r.subject else { return Err("that item is not part of a component".into()) };
        if r.form != Form::Drawn {
            return Err("terminals of placed symbols come from their definition".into());
        }
        let t: TerminalId = p.ids.fresh();
        let mut after = r.clone();
        after.connection_points.insert(t, at);
        let ops = vec![
            Op::AddTerminal { id: t, terminal: Terminal { component: c, label, key: None } },
            Op::ModifyRepresentation { id: rid, before: r.clone(), after },
        ];
        p.apply_edit(pg, "addterminal", "Add terminal", ops)?;
        Ok(json!({"terminal": t.hex()}))
    })
}

fn run_wire(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let raw = params.get("points").and_then(Value::as_array).ok_or_else(|| err("`points` [[x,y], …] is required"))?;
    if raw.len() > MAX_WIRE_POINTS {
        return Err(err(format!("at most {MAX_WIRE_POINTS} points per wire")));
    }
    let pts: Vec<Pt> = raw
        .iter()
        .map(|v| point(v).ok_or_else(|| err(format!("`{v}` is not a finite [x,y] point"))))
        .collect::<cadcraft_engine::Result<_>>()?;
    if pts.len() < 2 {
        return Err(err("`points` needs at least two [x,y] points"));
    }
    with_project(s, |p, pg, page| {
        let (w, i, r): (ConductorId, ItemId, RepresentationId) = (p.ids.fresh(), p.ids.fresh(), p.ids.fresh());
        let item = Item {
            page,
            style: plain_style(p),
            kind: ItemKind::Primitive(Primitive::Polyline { vertices: pts.clone(), closed: false }),
        };
        let mut ops = vec![
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
        ];
        // Ends that land on a terminal attach, resolved now and recorded (TX-01).
        let doc = p.ledger.document();
        let mut attached = Vec::new();
        for (end, at) in [(End::A, pts.first().copied()), (End::B, pts.last().copied())] {
            if let Some(t) = at.and_then(|at| terminal_at(doc, page, at, p.tolerance)) {
                ops.push(Op::AttachEnd { conductor: w, end, before: None, after: Some(t) });
                attached.push(json!({"end": format!("{end:?}"), "terminal": t.hex()}));
            }
        }
        p.apply_edit(pg, "wire", "Wire", ops)?;
        Ok(json!({"conductor": w.hex(), "item": i.hex(), "attached": attached}))
    })
}

fn route_and_end(p: &Project, page: PageId, c: ConductorId, end: End) -> Result<(ItemId, Item, Pt), String> {
    let doc = p.ledger.document();
    let (id, _) = ecad_model::derive::route_on_page(doc, c, page).ok_or("the conductor has no route on this page")?;
    let item = doc.items.get(&id).cloned().ok_or("route missing")?;
    let ItemKind::Primitive(prim) = &item.kind else { return Err("route is not a path".into()) };
    let (a, b) = prim.path_ends().ok_or("route is not a path")?;
    Ok((id, item, if end == End::A { a } else { b }))
}

fn run_connect(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let c: ConductorId = id(params, "conductor").map_err(err)?;
    let t: TerminalId = id(params, "terminal").map_err(err)?;
    let end = end_param(params).map_err(err)?;
    with_project(s, |p, pg, page| {
        let doc = p.ledger.document();
        let conductor = doc.conductors.get(&c).ok_or("no such conductor")?;
        if !doc.terminals.contains_key(&t) {
            return Err("no such terminal".into());
        }
        let at = ecad_model::derive::terminal_positions(doc, page)
            .into_iter()
            .find(|(x, _, _)| *x == t)
            .map(|(_, _, p)| p)
            .ok_or("the terminal is not shown on this page")?;
        let (route, item, current) = route_and_end(p, page, c, end)?;
        let mut ops = Vec::new();
        if !current.near(at, 1e-12) {
            let ItemKind::Primitive(prim) = &item.kind else { return Err("route is not a path".into()) };
            let moved = prim.with_path_end(end, at).ok_or("route is not a path")?;
            ops.push(Op::ModifyItem {
                id: route,
                before: item.clone(),
                after: Item { kind: ItemKind::Primitive(moved), ..item.clone() },
            });
        }
        ops.push(Op::AttachEnd { conductor: c, end, before: conductor.end(end), after: Some(t) });
        p.apply_edit(pg, "connect", "Connect", ops)?;
        Ok(json!({"connected": true}))
    })
}

fn run_disconnect(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let c: ConductorId = id(params, "conductor").map_err(err)?;
    let end = end_param(params).map_err(err)?;
    with_project(s, |p, pg, _page| {
        let before = p.ledger.document().conductors.get(&c).ok_or("no such conductor")?.end(end);
        if before.is_none() {
            return Err("that end is not connected".into());
        }
        p.apply_edit(pg, "disconnect", "Disconnect", vec![Op::AttachEnd { conductor: c, end, before, after: None }])?;
        Ok(json!({"disconnected": true}))
    })
}

/// Ops removing every representation of `subject` with its items.
fn remove_representations(doc: &ecad_model::Document, subject: Subject) -> Vec<Op> {
    let mut ops = Vec::new();
    for (rid, r) in doc.representations_of(subject) {
        ops.push(Op::UnlinkRepresentation { id: rid, rep: r.clone() });
        for i in &r.items {
            if let Some(item) = doc.items.get(i) {
                ops.push(Op::RemoveItem { id: *i, item: item.clone() });
            }
        }
    }
    ops
}

fn run_delete(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    with_project(s, |p, pg, _page| {
        let doc = p.ledger.document();
        let mut ops = Vec::new();
        if let Ok(c) = id::<ConductorId>(params, "conductor") {
            let conductor = doc.conductors.get(&c).ok_or("no such conductor")?;
            for end in End::BOTH {
                if let Some(t) = conductor.end(end) {
                    ops.push(Op::AttachEnd { conductor: c, end, before: Some(t), after: None });
                }
            }
            for cable in doc.cables_of(c) {
                if let Some((i, m)) =
                    doc.cables.get(&cable).and_then(|k| k.members.iter().enumerate().find(|(_, m)| m.conductor == c))
                {
                    ops.push(Op::RemoveCableMember { cable, index: i, member: m.clone() });
                }
            }
            ops.extend(remove_representations(doc, Subject::Conductor(c)));
            ops.push(Op::DeleteConductor { id: c, conductor: Conductor { ends: [None, None], ..conductor.clone() } });
        } else if let Ok(c) = id::<ComponentId>(params, "component") {
            let component = doc.components.get(&c).ok_or("no such component")?;
            for (t, term) in doc.terminals_of(c) {
                for (w, end) in doc.ends_at(t) {
                    ops.push(Op::AttachEnd { conductor: w, end, before: Some(t), after: None });
                }
                let _ = term;
            }
            ops.extend(remove_representations(doc, Subject::Component(c)));
            for (t, term) in doc.terminals_of(c) {
                ops.push(Op::RemoveTerminal { id: t, terminal: term.clone() });
            }
            ops.push(Op::DeleteComponent { id: c, component: component.clone() });
        } else {
            return Err("`conductor` or `component` (hex id) is required".into());
        }
        p.apply_edit(pg, "ecaddelete", "Delete", ops)?;
        Ok(json!({"deleted": true}))
    })
}

fn run_setprop(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let key = params.get("key").and_then(Value::as_str).ok_or_else(|| err("`key` is required"))?.to_string();
    // `null` clears the property; a missing value or another type is an error, never a clear.
    let value = match params.get("value") {
        Some(Value::Null) => None,
        Some(Value::String(v)) => Some(v.clone()),
        Some(_) => return Err(err("`value` must be a string, or null to clear the property")),
        None => return Err(err("`value` is required (a string, or null to clear the property)")),
    };
    let object = if let Some(c) = opt_id(params, "component").map_err(err)? {
        ObjectRef::Component(c)
    } else if let Some(c) = opt_id(params, "conductor").map_err(err)? {
        ObjectRef::Conductor(c)
    } else if let Some(c) = opt_id(params, "cable").map_err(err)? {
        ObjectRef::Cable(c)
    } else {
        return Err(err("`component`, `conductor` or `cable` (hex id) is required"));
    };
    with_project(s, |p, pg, _page| {
        let doc = p.ledger.document();
        let props = match object {
            ObjectRef::Component(c) => doc.components.get(&c).map(|x| &x.properties),
            ObjectRef::Conductor(c) => doc.conductors.get(&c).map(|x| &x.properties),
            ObjectRef::Cable(c) => doc.cables.get(&c).map(|x| &x.properties),
        }
        .ok_or("no such object")?;
        let before = props.get(&key).cloned();
        if before == value {
            return Ok(json!({"changed": false}));
        }
        p.apply_edit(pg, "setprop", "Set property", vec![Op::SetProperty { object, key, before, after: value }])?;
        Ok(json!({"changed": true}))
    })
}

/// Most conductors one CABLEADD accepts (input-derived sizes are capped, never trusted).
const MAX_CABLE_MEMBERS: usize = 10_000;

fn run_cableadd(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let conductors: Vec<ConductorId> = id_list(params, "conductors", MAX_CABLE_MEMBERS).map_err(err)?;
    if conductors.is_empty() {
        return Err(err("`conductors` (hex ids) is required"));
    }
    // Only an absent `cable` creates one; a malformed ID is an error, never a new cable.
    let existing: Option<CableId> = opt_id(params, "cable").map_err(err)?;
    let tag = opt_typed(params, "tag", "a string", |v| v.as_str().map(str::to_string)).map_err(err)?;
    with_project(s, |p, pg, _page| {
        let doc = p.ledger.document();
        let mut ops = Vec::new();
        let (cable, mut index) = match existing {
            Some(c) => (c, doc.cables.get(&c).ok_or("no such cable")?.members.len()),
            None => {
                let c: CableId = p.ids.fresh();
                let mut cable = ecad_model::Cable::default();
                if let Some(t) = tag {
                    cable.properties.insert("tag".into(), t);
                }
                ops.push(Op::CreateCable { id: c, cable });
                (c, 0)
            }
        };
        for w in conductors {
            ops.push(Op::AddCableMember { cable, index, member: CableMember { conductor: w, core: None } });
            index += 1;
        }
        p.apply_edit(pg, "cableadd", "Add to cable", ops)?;
        Ok(json!({"cable": cable.hex()}))
    })
}

/// The experiment's built-in definition: a two-terminal coil, 10 × 20, terminals A1 (top) and A2
/// (bottom).
pub fn builtin_coil() -> Definition {
    let r = [(-5.0, -10.0), (5.0, -10.0), (5.0, 10.0), (-5.0, 10.0)];
    Definition {
        name: "Coil (experiment)".into(),
        origin: None,
        graphics: vec![Primitive::Polyline {
            vertices: r.iter().map(|(x, y)| Pt::new(*x, *y)).collect(),
            closed: true,
        }],
        terminals: vec![
            TerminalDef { key: "A1".into(), label: Some("A1".into()), at: Pt::new(0.0, 10.0) },
            TerminalDef { key: "A2".into(), label: Some("A2".into()), at: Pt::new(0.0, -10.0) },
        ],
    }
}

fn run_place(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let at = params.get("at").and_then(point).ok_or_else(|| err("`at` [x,y] is required"))?;
    let degrees = opt_typed(params, "rotation", "a finite number of degrees", |v| v.as_f64().filter(|d| d.is_finite()))
        .map_err(err)?
        .unwrap_or(0.0);
    let mirror = opt_typed(params, "mirror", "true or false", Value::as_bool).map_err(err)?.unwrap_or(false);
    let wanted: Option<DefinitionId> = opt_id(params, "definition").map_err(err)?;
    with_project(s, |p, pg, page| {
        let doc = p.ledger.document();
        let mut ops = Vec::new();
        let (def_id, def) = match wanted {
            Some(d) => (d, doc.definitions.get(&d).cloned().ok_or("no such definition")?),
            None => match doc.definitions.iter().find(|(_, d)| d.name == builtin_coil().name) {
                Some((id, d)) => (*id, d.clone()),
                None => {
                    let id: DefinitionId = p.ids.fresh();
                    ops.push(Op::EmbedDefinition { id, definition: builtin_coil() });
                    (id, builtin_coil())
                }
            },
        };
        let (c, item, r): (ComponentId, ItemId, RepresentationId) = (p.ids.fresh(), p.ids.fresh(), p.ids.fresh());
        ops.push(Op::CreateComponent {
            id: c,
            component: Component { definition: Some(def_id), ..Component::default() },
        });
        let mut terminals = Vec::new();
        for td in &def.terminals {
            let t: TerminalId = p.ids.fresh();
            ops.push(Op::AddTerminal {
                id: t,
                terminal: Terminal { component: c, label: td.label.clone(), key: Some(td.key.clone()) },
            });
            terminals.push(json!({"key": td.key, "terminal": t.hex()}));
        }
        let placement = Placement { origin: at, rotation: degrees.to_radians(), mirror, scale: 1.0 };
        ops.push(Op::AddItem {
            id: item,
            item: Item {
                page,
                style: plain_style(p),
                kind: ItemKind::Symbol(SymbolInstance { definition: def_id, placement }),
            },
        });
        ops.push(Op::LinkRepresentation {
            id: r,
            rep: Representation {
                subject: Subject::Component(c),
                page,
                form: Form::Symbol,
                role: "main".into(),
                items: vec![item],
                connection_points: BTreeMap::new(),
            },
        });
        p.apply_edit(pg, "place", "Place symbol", ops)?;
        Ok(json!({"component": c.hex(), "item": item.hex(), "definition": def_id.hex(), "terminals": terminals}))
    })
}

fn run_newpage(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let title = params.get("title").and_then(Value::as_str).unwrap_or("new").to_string();
    let page = with_project(s, |p, pg, _| {
        let id: PageId = p.ids.fresh();
        let index = p.ledger.document().page_order.len();
        p.apply_edit(pg, "newpage", "New page", vec![Op::AddPage { id, page: Page { title }, index }])?;
        p.pages.entry(id).or_default();
        Ok(id)
    })?;
    open(s, page)
}

fn open(s: &mut Session, page: PageId) -> cadcraft_engine::Result<Value> {
    let uid = with_any_page(s, |p, pg| p.open_page(pg, page))?;
    Ok(json!({"page": page.hex(), "document": uid}))
}

fn run_openpage(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let page: PageId = id(params, "page").map_err(err)?;
    open(s, page)
}

fn run_inspect(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let at = params.get("seq").and_then(Value::as_u64);
    with_any_page(s, |p, pg| p.snapshot(&pg.as_ref(), at))
}
