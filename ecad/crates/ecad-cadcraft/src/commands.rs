//! ECAD commands (Path A). Their JSON forms build transactions from resolved inputs (IDs,
//! coordinates) and commit them through [`Project::apply_edit`].
//!
//! They reach the command line, scripts, the control channel and MCP through the engine's
//! extension command registry. `convertcomponent`, `addterminal`, `place`, `wire`, `connect`,
//! `disconnect`, `cableslash`, `cableview` and `cablecore` also have click-based interactive forms
//! ([`crate::interact`], briefs 008, 004, 005 and 009), started by a bare command name, a menu or a
//! tool button; they commit through these JSON forms. `cableview` records nothing: a cable's
//! view is presentation only ([`crate::views`]).
//! Brief 010 adds `cableref`, `cablecontinue` and `cablegoto` (with interactive forms),
//! `cablerefs` (read only), `renamepage` and `movepage` ([`crate::references`]).
//! Programmatic calls (`Session::execute`, `name {json}`) always run the JSON form and never
//! prompt.

use std::collections::BTreeMap;

use cadcraft_engine::doc::Handle;
use cadcraft_engine::{CommandSpec, EngineError, Session};
use ecad_model::{
    CableId, CableMember, Color, Component, ComponentId, Conductor, ConductorId, Definition, DefinitionId, End, Form, Item, ItemId, ItemKind,
    ObjectRef, Op, PageId, Placement, Primitive, Pt, Representation, RepresentationId, Style, Subject, SymbolInstance, Terminal, TerminalDef,
    TerminalId,
};
use serde_json::{Value, json};

use crate::access::{Pages, owner_of_active, try_lock};
use crate::interact;
use crate::project::{Mapped, Project};
use crate::slash::{self, MAX_CABLE_MEMBERS};
use crate::views::{self, View};

/// The commands this crate adds. Lanes in flight register theirs in [`crate::lanes`], not here;
/// their lines move here at the phase-end tidy-up.
pub fn specs() -> Vec<CommandSpec> {
    let mut v = base_specs();
    v.extend(crate::sheet_cmds::specs());
    v.extend(crate::history::specs());
    v.extend(crate::merging::specs());
    v.extend(crate::numbering_cmds::specs());
    v.extend(crate::xref_cmds::specs());
    v.extend(crate::output_cmds::specs());
    v.extend(crate::terminal_cmds::specs());
    v.extend(crate::accessory_cmds::specs());
    v.extend(crate::release::specs());
    v.extend(crate::text_cmds::specs());
    #[cfg(not(target_arch = "wasm32"))]
    v.extend(crate::agent_access::specs());
    v.extend(crate::level_cmds::specs());
    for (_, lane) in crate::lanes::LANES {
        v.extend(lane());
    }
    v
}

fn base_specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("convertcomponent", "Convert to Component", run_convert)
            .params("{handles?: [hex]} (default: selection)")
            .interactive(interact::convert)
            .noundo(),
        CommandSpec::new("addterminal", "Add Connection Point", run_addterminal)
            .params("{handle: hex (an item of the component), at: [x,y], label?}")
            .interactive(interact::addterminal)
            .noundo(),
        CommandSpec::new("wire", "Wire", crate::wire_cmds::run_wire)
            .params(
                "{points: [[x,y], …], potential?: string, join?: bool (true), junction?: bool (old name of join)} (ends on connection points attach; an end on another wire's open end joins it into one wire, on its interior adds a junction; open ends lying on the new wire join it with a junction)",
            )
            .interactive(interact::wire)
            .noundo(),
        CommandSpec::new("connect", "Connect", run_connect)
            .params("{conductor: id, end: \"A\"|\"B\", terminal: id}")
            .interactive(interact::connect)
            .noundo(),
        CommandSpec::new("disconnect", "Disconnect", run_disconnect)
            .params("{conductor: id, end: \"A\"|\"B\"}")
            .interactive(interact::disconnect)
            .noundo(),
        CommandSpec::new("cableslash", "Cable Slash", run_cableslash)
            .params("{from: [x,y], to: [x,y], tag?} (the wires the slash crosses become one cable)")
            .interactive(interact::cableslash)
            .noundo(),
        CommandSpec::new("cableview", "Cable View", run_cableview)
            .params("{cable?: id, view?: \"traditional\"|\"fan\"} (no cable: every cable's view; presentation only)")
            .interactive(interact::cableview)
            .noundo(),
        CommandSpec::new("cablecore", "Core Label", run_cablecore)
            .params(
                "{conductor: id, core: string|null} | {cable: id, cores: [string|null, …]} (null, empty or spaces clear)",
            )
            .interactive(interact::cablecore)
            .noundo(),
        CommandSpec::new("cablefan", "Cable Join", run_cablefan)
            .params(
                "{cable: id, end?: \"A\"|\"B\", at: [x,y] | fraction 0..1 | null} (where the trunk joins a fanned \
end's spine; null: the middle)",
            )
            .interactive(interact::cablefan)
            .noundo(),
        CommandSpec::new("cableref", "Cable Reference", run_cableref)
            .params("{cable: id, at: [x,y]} (a reference end of the cable on this page)")
            .interactive(interact::cableref)
            .noundo(),
        CommandSpec::new("cablecontinue", "Continue Cable", run_cablecontinue)
            .params("{cable: id, at: [x,y], toward: [x,y], members?: [conductor id]} (the cable's second drawing)")
            .interactive(interact::cablecontinue)
            .noundo(),
        CommandSpec::new("cablegoto", "Go to Reference", run_cablegoto)
            .params("{reference: id} (opens the other end's page; records nothing)")
            .interactive(interact::cablegoto)
            .noundo(),
        CommandSpec::new("cablerefs", "List Cable References", run_cablerefs)
            .params("{cable?: id} (read only)")
            .noundo(),
        CommandSpec::new("ecaddelete", "Delete Electrical Object", run_delete)
            .params("{conductor: id} | {component: id} | {cable: id}")
            .noundo(),
        CommandSpec::new("setprop", "Set Property", run_setprop)
            .params(
                "{component|conductor|cable: id, key, value: string|null, conflict?: \"swap\"|\"merge\"|\"next\", merge?: bool} | {components: [id], key, value} (several devices in one transaction; not `tag`) (a device's or cable's tag: typing fixes it; `-K?` is the next provisional; a tag another device or cable holds needs `conflict`; key `tag_fixed`: \"yes\" or null)",
            )
            .noundo(),
        CommandSpec::new("cableadd", "Add to Cable", run_cableadd)
            .params("{cable?: id, conductors: [id], tag?}")
            .noundo(),
        CommandSpec::new("place", "Place Symbol", run_place)
            .params(
                "{at: [x,y], rotation?: degrees, mirror?: bool, symbol?: key, definition?: id, device?: id, tag?} (`symbol`: a starter symbol, see `symbols`; `device` adds it to that device; `tag` names a new one, or adds it to the device that has that tag)",
            )
            .interactive(interact::place)
            .noundo(),
        CommandSpec::new("symbols", "Find Symbol", crate::device_cmds::run_symbols)
            .params("{query?} (starter symbols and devices matching it; read only)")
            .noundo(),
        CommandSpec::new("devices", "List Devices", crate::device_cmds::run_devices)
            .params("{tag?} (devices whose tag starts with it; read only)")
            .noundo(),
        CommandSpec::new("tagsettings", "Tag Settings", crate::device_cmds::run_tagsettings)
            .params(
                "{padding?: 1..6, prefixes?: {symbol: prefix|null}} (automatic tags: digits, and this project's prefix per starter symbol; no parameters lists them)",
            )
            .noundo(),
        CommandSpec::new("deviceprops", "Device Properties", crate::device_cmds::run_deviceprops)
            .params("{component: id} (a device's properties; read only; interactive: change them)")
            .interactive(interact::deviceprops)
            .noundo(),
        CommandSpec::new("devicedelete", "Delete Device", crate::device_cmds::run_devicedelete)
            .params("{component: id} (deletes the device with every symbol, as `ecaddelete`)")
            .interactive(interact::devicedelete)
            .noundo(),
        CommandSpec::new("nets", "List Nets", crate::wire_cmds::run_nets)
            .params("{page?: id, number?, conductor?: id, terminal?: id, all?: bool, limit?: n (200, max 2000), after?: cursor} (read only)")
            .noundo(),
        CommandSpec::new("net", "Show Net", crate::wire_cmds::run_net)
            .params("{conductor: id} | {terminal: id} | {number} (read only)")
            .noundo(),
        CommandSpec::new("wirenumber", "Fix Wire Number", crate::wire_cmds::run_wirenumber)
            .params(
                "{conductor: id | number, value: string|null, conflict?: \"swap\"|\"another\"|\"merge\"} | {conductor: id, fixed: bool} (fixes the net's number; null or fixed false releases it; a number another net uses needs `conflict`)",
            )
            .interactive(interact::wirenumber)
            .noundo(),
        CommandSpec::new("potential", "Set Potential", crate::wire_cmds::run_potential)
            .params("{conductor: id, value: string|null} (the potential of the whole net)")
            .noundo(),
        CommandSpec::new("wiresettings", "Wire Settings", crate::wire_cmds::run_wiresettings)
            .params(
                "{scheme?: \"potential\"|\"page\"|\"page-column\", digits?: 1..6, unclassified_prefix?, potentials?: [{name, prefix, numbered, colour, cross_section}], colour_style?: \"iec\"|\"three-letter\", label_height?: mm} (no parameters reads; colours are IEC 60757 codes by default, free text allowed)",
            )
            .noundo(),
        CommandSpec::new("renumberwires", "Renumber Wires", crate::wire_cmds::run_renumberwires)
            .params("{scope?: \"unnumbered\"|\"all\"} (fixed numbers never change)")
            .interactive(interact::renumberwires)
            .noundo(),
        CommandSpec::new("junction", "Junction", crate::wire_cmds::run_junction)
            .params("{at: [x,y]} (a junction on the wire there, which is split at it)")
            .interactive(interact::junction)
            .noundo(),
        CommandSpec::new("checks", "Checks", crate::wire_cmds::run_checks)
            .params("{kinds?: [kind], limit?: n (200, max 2000), after?: n} (read only)")
            .noundo(),
        CommandSpec::new("newpage", "New Page", run_newpage)
            .params(
                "{title, number?, folder?: id, generated?: kind, strip?: id} (number: default the next after the page before it; generated: \"cover\", \"contents\", \"revisions\", \"bom-location\", \"terminals\" (strip: one strip) or \"cables\")",
            )
            .noundo(),
        CommandSpec::new("openpage", "Open Page", run_openpage).params("{page: id}").noundo(),
        CommandSpec::new("renamepage", "Rename Page", run_renamepage).params("{page: id, title}").noundo(),
        CommandSpec::new("movepage", "Move Page", run_movepage)
            .params("{page: id, index: n} (1 = first) | {page: id, folder: id|null, index?: n} (into a folder, 1 = first of its pages)")
            .noundo(),
        CommandSpec::new("ecadinspect", "Inspect Project", run_inspect)
            .params("{seq?: n} (read-only snapshot)")
            .noundo(),
        CommandSpec::new("joinwires", "Join Wires", crate::wire_cmds::run_joinwires)
            .params("{at: [x,y]} (two open ends there become one wire; an open end on a wire, or three or more ends, get a junction)")
            .interactive(interact::joinwires)
            .noundo(),
    ]
}

pub(crate) fn err(m: impl Into<String>) -> EngineError {
    EngineError::Other(m.into())
}

/// Run `f` with the project owning the active document. The closure gets [`Pages`], not the
/// session, so it cannot call an owner-aware session method while the project is locked; the lock
/// is taken with `try_lock`, so a re-entrant command is refused instead of deadlocking.
pub(crate) fn with_project<R>(
    s: &mut Session, f: impl FnOnce(&mut Project, &mut Pages<'_>, PageId) -> Result<R, String>,
) -> cadcraft_engine::Result<R> {
    let owner = owner_of_active(s).map_err(err)?;
    let uid = s.state()?.uid;
    let mut guard = try_lock(&owner).map_err(err)?;
    let p = guard.as_any_mut().downcast_mut::<Project>().ok_or_else(|| err("this page is not owned by an ECAD project"))?;
    let page = p.page_of(uid).ok_or_else(|| err("page view not found"))?;
    if p.pages.get(&page).is_some_and(|v| v.unavailable) {
        return Err(err(UNAVAILABLE));
    }
    f(p, &mut Pages(s), page).map_err(EngineError::Other)
}

/// Like [`with_project`] for commands that also work on an unavailable page (open, inspect).
pub(crate) fn with_any_page<R>(s: &mut Session, f: impl FnOnce(&mut Project, &mut Pages<'_>) -> Result<R, String>) -> cadcraft_engine::Result<R> {
    let owner = owner_of_active(s).map_err(err)?;
    let mut guard = try_lock(&owner).map_err(err)?;
    let p = guard.as_any_mut().downcast_mut::<Project>().ok_or_else(|| err("this page is not owned by an ECAD project"))?;
    f(p, &mut Pages(s)).map_err(EngineError::Other)
}

/// Refusal for edits on a page whose drawing could not be rebuilt (PROJ-02).
pub const UNAVAILABLE: &str = "this page is unavailable (its drawing could not be rebuilt); close and reopen it";

pub(crate) fn id<T: From<u128>>(p: &Value, key: &str) -> Result<T, String> {
    p.get(key)
        .and_then(Value::as_str)
        .and_then(|t| u128::from_str_radix(t.trim(), 16).ok())
        .map(T::from)
        .ok_or_else(|| format!("`{key}` (hex id) is required"))
}

/// An optional ID parameter: absent or `null` is `None`; anything else must be a valid hex ID, so a
/// typo is an error rather than "not given".
pub(crate) fn opt_id<T: From<u128>>(p: &Value, key: &str) -> Result<Option<T>, String> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(_) => id(p, key).map(Some),
    }
}

/// A list of IDs: every entry must parse, and the list may hold at most `max`; checked before
/// anything is collected.
pub(crate) fn id_list<T: From<u128>>(p: &Value, key: &str, max: usize) -> Result<Vec<T>, String> {
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
pub(crate) fn opt_typed<'a, T>(p: &'a Value, key: &str, what: &str, get: impl Fn(&'a Value) -> Option<T>) -> Result<Option<T>, String> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => get(v).map(Some).ok_or_else(|| format!("`{key}` must be {what}")),
    }
}

/// Most points one WIRE accepts.
pub(crate) const MAX_WIRE_POINTS: usize = 10_000;

pub(crate) fn point(v: &Value) -> Option<Pt> {
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
pub(crate) const MAX_HANDLES: usize = 100_000;

/// The `handles` parameter, or the selection when it is absent. Every entry must be a valid hex
/// handle: a typo is an error, never a silently smaller set.
fn handles(s: &Session, p: &Value) -> Result<Vec<Handle>, String> {
    let Some(v) = p.get("handles") else { return Ok(s.selection()) };
    let a = v.as_array().ok_or("`handles` must be an array of hex handles")?;
    if a.len() > MAX_HANDLES {
        return Err(format!("at most {MAX_HANDLES} handles per call"));
    }
    a.iter().map(|h| h.as_str().and_then(Handle::parse_hex).ok_or_else(|| format!("`{h}` is not a valid handle"))).collect()
}

pub(crate) fn plain_style(p: &Project) -> Style {
    Style { layer: p.default_layer, color: Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 }
}

fn run_convert(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let hs = handles(s, params).map_err(err)?;
    with_project(s, |p, pg, page| {
        let map = &p.pages.get(&page).ok_or("page view not found")?.map;
        let mut items = Vec::new();
        // Computed parts of a traditional-view cable are left out of a selection with other
        // objects (brief 009 review S2); on their own they are refused below.
        let mut drawn = None;
        for h in hs {
            match map.get(h) {
                Some(Mapped::Item(i)) if p.ledger.document().representation_of_item(i).is_none() => items.push(i),
                Some(Mapped::Fan(k)) if !map.is_fan(k) => drawn = Some(k),
                Some(Mapped::Fan(k)) => return Err(views::refusal(map, k).into()),
                Some(_) => {
                    return Err("only plain geometry can be converted; part of the selection already has a meaning".into());
                }
                None => return Err("the selection holds an object this page does not know".into()),
            }
        }
        if items.is_empty() {
            return Err(match drawn {
                Some(k) => views::refusal(map, k).into(),
                None => "select the geometry to convert".into(),
            });
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
    let h = params.get("handle").and_then(Value::as_str).and_then(Handle::parse_hex).ok_or_else(|| err("`handle` is required"))?;
    let at = params.get("at").and_then(point).ok_or_else(|| err("`at` [x,y] is required"))?;
    let label = params.get("label").and_then(Value::as_str).map(str::to_string);
    with_project(s, |p, pg, page| {
        let map = &p.pages.get(&page).ok_or("page view not found")?.map;
        let item = match map.get(h) {
            Some(Mapped::Item(item)) => item,
            Some(Mapped::Fan(k)) => return Err(views::refusal(map, k).into()),
            _ => return Err("pick an item of a component".into()),
        };
        let doc = p.ledger.document();
        let (rid, r) = doc.representation_of_item(item).ok_or("that item is not part of a component")?;
        let Subject::Component(c) = r.subject else { return Err("that item is not part of a component".into()) };
        if r.form != Form::Drawn {
            return Err("connection points of placed symbols come from their definition".into());
        }
        let t: TerminalId = p.ids.fresh();
        let mut after = r.clone();
        after.connection_points.insert(t, at);
        let ops = vec![
            Op::AddTerminal { id: t, terminal: Terminal { component: c, label, key: None } },
            Op::ModifyRepresentation { id: rid, before: r.clone(), after },
        ];
        p.apply_edit(pg, "addterminal", "Add connection point", ops)?;
        Ok(json!({"terminal": t.hex()}))
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
            return Err("no such connection point".into());
        }
        let at = ecad_model::derive::terminal_positions(doc, page)
            .into_iter()
            .find(|(x, _, _)| *x == t)
            .map(|(_, _, p)| p)
            .ok_or("the connection point is not shown on this page")?;
        let (route, item, current) = route_and_end(p, page, c, end)?;
        let mut ops = Vec::new();
        if !current.near(at, 1e-12) {
            let ItemKind::Primitive(prim) = &item.kind else { return Err("route is not a path".into()) };
            let moved = prim.with_path_end(end, at).ok_or("route is not a path")?;
            ops.push(Op::ModifyItem { id: route, before: item.clone(), after: Item { kind: ItemKind::Primitive(moved), ..item.clone() } });
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

pub(crate) fn run_delete(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
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
                if let Some((i, m)) = doc.cables.get(&cable).and_then(|k| k.members.iter().enumerate().find(|(_, m)| m.conductor == c)) {
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
        } else if let Ok(k) = id::<CableId>(params, "cable") {
            // Design 004 §8 (owner answer O3): the members leave, the slashes go, every conductor
            // stays.
            let cable = doc.cables.get(&k).ok_or("no such cable")?;
            for (index, member) in cable.members.iter().enumerate().rev() {
                ops.push(Op::RemoveCableMember { cable: k, index, member: member.clone() });
            }
            ops.extend(remove_representations(doc, Subject::Cable(k)));
            ops.push(Op::DeleteCable { id: k, cable: ecad_model::Cable { members: Vec::new(), ..cable.clone() } });
            let kept = cable.members.len();
            p.apply_edit(pg, "ecaddelete", "Delete", ops)?;
            let message = format!("Cable {} deleted; its {} kept.", k.hex(), kept_words(kept));
            return Ok(json!({"deleted": true, "kept": kept, "message": message}));
        } else {
            return Err("`conductor`, `component` or `cable` (hex id) is required".into());
        }
        p.apply_edit(pg, "ecaddelete", "Delete", ops)?;
        Ok(json!({"deleted": true}))
    })
}

fn kept_words(n: usize) -> String {
    match n {
        0 => "0 wires are".into(),
        1 => "1 wire is".into(),
        n => format!("{n} wires are"),
    }
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
    // Brief 031 (§10.6): `text.` keys are written by the text commands only.
    if crate::text_style::is_reserved_key(&key) {
        return Err(err(crate::text_style::RESERVED_KEY));
    }
    // Several devices at once (brief 019, the Properties panel): one transaction for all.
    if params.get("components").is_some() {
        let cs: Vec<ComponentId> = id_list(params, "components", MAX_HANDLES).map_err(err)?;
        return run_setprop_many(s, cs, key, value);
    }
    let object = if let Some(c) = opt_id(params, "component").map_err(err)? {
        ObjectRef::Component(c)
    } else if let Some(c) = opt_id(params, "conductor").map_err(err)? {
        ObjectRef::Conductor(c)
    } else if let Some(c) = opt_id(params, "cable").map_err(err)? {
        ObjectRef::Cable(c)
    } else {
        return Err(err("`component`, `conductor` or `cable` (hex id) is required"));
    };
    // Brief 026: tags of devices and cables share one namespace and one set of rules.
    if matches!(object, ObjectRef::Component(_) | ObjectRef::Cable(_)) {
        let tagless = matches!(object, ObjectRef::Component(c) if with_any_page(s, |p, _| Ok(crate::wires::kind_of(p.ledger.document(), c).is_some())).unwrap_or(false));
        if key == ecad_model::tag::TAG_KEY && !tagless {
            return crate::numbering_cmds::run_set_tag(s, params, object);
        }
        if key == ecad_model::tag::FIXED_KEY && !tagless {
            return crate::numbering_cmds::run_set_fixed(s, params, object);
        }
    }
    // Brief 018: device properties follow design 018 §6.1 (tags are identities).
    if let ObjectRef::Component(c) = object
        && crate::device_cmds::is_device_property(object, &key)
    {
        return crate::device_cmds::run_setprop(s, params, c, key);
    }
    let v = with_project(s, |p, pg, _page| {
        // The value as typed (the rules below may store it canonically).
        let typed = value.clone();
        let doc = p.ledger.document();
        // Brief 022: numbering keys go through their commands; label values are checked.
        let value = match crate::wire_cmds::setprop_rule(doc, object, &key, value.as_deref())? {
            Some(v) => v,
            None => value,
        };
        // Brief 025 (FP1, FP2): a part's contacts and its table's placement are checked.
        let value = match (object, key.as_str()) {
            (ObjectRef::Component(_), crate::xref::CONTACTS_KEY) => crate::xref::offer_value(value.as_deref())?,
            (ObjectRef::Component(_), crate::xref::MIRROR_KEY) => crate::xref::mirror_value(value.as_deref())?,
            // Brief 028 (FP1, FP2, FP4): a strip's offer, a cable's cores, the PE row's part.
            // A strip's offer; on another device `terminals` stays free text (review of PR #55, 5).
            (ObjectRef::Component(c), crate::strips::OFFER_KEY) if crate::strips::is_strip(doc, c) => crate::strips::offer_value(value.as_deref())?,
            (ObjectRef::Cable(_), crate::strips::CORES_KEY) => crate::strips::cores_value(value.as_deref())?,
            (ObjectRef::Component(_), crate::strips::PE_PART_KEY | crate::strips::PE_MAKER_KEY) => {
                crate::devices::property_value(doc, &key, value.as_deref())?
            }
            // Brief 036 (§12, FP4): the registry's terminal defaults on a strip, and a level's label.
            (ObjectRef::Component(c), k) if k.starts_with("terminal.") && crate::strips::is_strip(doc, c) => {
                match crate::defaults::terminals::setprop_value(k, value.as_deref()) {
                    Some(r) => r?,
                    None => value,
                }
            }
            (ObjectRef::Component(c), k) if k.starts_with(crate::terminal_types::LABEL_PREFIX) && crate::strips::is_strip(doc, c) => {
                match value.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
                    None => None,
                    Some(v) if v.chars().count() > crate::terminal_types::MAX_LABEL_CHARS || v.chars().any(char::is_control) => {
                        return Err(format!(
                            "A level's label is 1 to {} characters, with no control characters.",
                            crate::terminal_types::MAX_LABEL_CHARS
                        ));
                    }
                    Some(v) => Some(v.to_string()),
                }
            }
            // Brief 034 (design 034 §7.3): `rating` and a fuse bank's `rating.<n>`.
            (ObjectRef::Component(_), k) if crate::symbols::is_rating_key(k) => crate::symbols::setprop_rule(doc, object, &key, value.as_deref())?,
            // Brief 030 (FP1, FP2, FP5, FP6): placements, links and accessory parts of a strip.
            (ObjectRef::Component(c), _) => match crate::accessories::setprop_rule(doc, c, &key, value.as_deref())? {
                Some(v) => v,
                None => value,
            },
            _ => value,
        };
        let props = match object {
            ObjectRef::Component(c) => doc.components.get(&c).map(|x| &x.properties),
            ObjectRef::Conductor(c) => doc.conductors.get(&c).map(|x| &x.properties),
            ObjectRef::Cable(c) => doc.cables.get(&c).map(|x| &x.properties),
            ObjectRef::Page(_) | ObjectRef::Project => None,
        }
        .ok_or("no such object")?;
        let before = props.get(&key).cloned();
        if before == value {
            return Ok(json!({"changed": false}));
        }
        // Brief 036 (§5.1): a typed `2.1` is position 2, level 1, also when a position `2.1` exists.
        let note = match (object, &value) {
            (ObjectRef::Component(c), Some(_)) if ecad_model::links::link_number(&key).is_some() => {
                typed.as_deref().and_then(|v| crate::terminal_types::dotted_position_words(doc, c, v))
            }
            _ => None,
        };
        p.apply_edit(pg, "setprop", "Set property", vec![Op::SetProperty { object, key, before, after: value }])?;
        Ok(match note {
            Some(m) => json!({"changed": true, "message": m}),
            None => json!({"changed": true}),
        })
    })?;
    if let Some(m) = v.get("message").and_then(Value::as_str) {
        s.echo(m.to_string());
    }
    Ok(v)
}

/// `setprop {components: [id…], key, value}`: a device property written to several devices in
/// one transaction (design 019 §8.2); tags are one per device, so `tag` is refused.
fn run_setprop_many(s: &mut Session, cs: Vec<ComponentId>, key: String, value: Option<String>) -> cadcraft_engine::Result<Value> {
    if key == ecad_model::tag::TAG_KEY {
        return Err(err("Tags are one per device."));
    }
    with_project(s, |p, pg, _page| {
        let doc = p.ledger.document();
        let after = match key.as_str() {
            crate::xref::CONTACTS_KEY => crate::xref::offer_value(value.as_deref())?,
            crate::xref::MIRROR_KEY => crate::xref::mirror_value(value.as_deref())?,
            crate::strips::OFFER_KEY if cs.iter().any(|c| crate::strips::is_strip(doc, *c)) => crate::strips::offer_value(value.as_deref())?,
            // Brief 030: the same rule for every strip (a link's text depends on its strip).
            k if k.starts_with(crate::accessories::ACC_PREFIX) || k.starts_with(ecad_model::links::PREFIX) || crate::accessories::is_part_key(k) => {
                let mut out = None;
                for c in &cs {
                    if let Some(v) = crate::accessories::setprop_rule(doc, *c, k, value.as_deref())? {
                        out = v;
                    }
                }
                out
            }
            // Brief 034: `rating.<n>` only on fuse banks that have fuse `<n>`.
            k if crate::symbols::is_rating_key(k) => {
                let mut out = None;
                for c in &cs {
                    out = crate::symbols::setprop_rule(doc, ObjectRef::Component(*c), k, value.as_deref())?;
                }
                out
            }
            _ => crate::devices::property_value(doc, &key, value.as_deref())?,
        };
        let mut ops = Vec::new();
        for c in &cs {
            let comp = doc.components.get(c).ok_or("no such object")?;
            let before = comp.properties.get(&key).cloned();
            if before != after {
                ops.push(Op::SetProperty { object: ObjectRef::Component(*c), key: key.clone(), before, after: after.clone() });
            }
        }
        if ops.is_empty() {
            return Ok(json!({"changed": false}));
        }
        let n = ops.len();
        p.apply_edit(pg, "setprop", "Set property", ops)?;
        Ok(json!({"changed": true, "devices": n}))
    })
}

/// A core label as given (design 009 §5.4): trimmed; empty or only spaces clears it (`None`).
///
/// # Errors
/// Longer than [`crate::look::MAX_CORE_CHARS`] characters, or a control character, in words.
pub fn core_label(text: Option<&str>) -> Result<Option<String>, String> {
    let Some(t) = text.map(str::trim).filter(|t| !t.is_empty()) else { return Ok(None) };
    if t.chars().any(char::is_control) {
        return Err("A core label cannot contain control characters.".into());
    }
    if t.chars().count() > crate::look::MAX_CORE_CHARS {
        return Err(format!("A core label can be at most {} characters.", crate::look::MAX_CORE_CHARS));
    }
    Ok(Some(t.to_string()))
}

/// The `core` of one entry: a string or `null`.
fn core_value(v: Option<&Value>) -> Result<Option<String>, String> {
    match v {
        Some(Value::Null) => core_label(None),
        Some(Value::String(t)) => core_label(Some(t)),
        _ => Err("`core` must be a string, or null to clear it".into()),
    }
}

/// The ops changing member `index` of `cable` from `old` to `new` (design 009 §5.3).
fn core_ops(cable: CableId, index: usize, conductor: ConductorId, old: Option<String>, new: Option<String>) -> [Op; 2] {
    [
        Op::RemoveCableMember { cable, index, member: CableMember { conductor, core: old } },
        Op::AddCableMember { cable, index, member: CableMember { conductor, core: new } },
    ]
}

/// The other members of `cable` with core label `core` (design 009 §5.4, duplicates allowed).
fn duplicates(doc: &ecad_model::Document, cable: CableId, conductor: ConductorId, core: &str) -> Vec<ConductorId> {
    doc.cables
        .get(&cable)
        .map(|c| c.members.iter().filter(|m| m.conductor != conductor && m.core.as_deref() == Some(core)).map(|m| m.conductor).collect())
        .unwrap_or_default()
}

/// `cablecore` (design 009 §5.2): set or clear the core label of one member, or of every member of
/// a cable at once. One transaction; nothing changed, no transaction.
fn run_cablecore(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    if params.get("cores").is_some() {
        return run_cablecores(s, params);
    }
    let conductor: ConductorId = id(params, "conductor").map_err(err)?;
    let core = core_value(params.get("core")).map_err(err)?;
    with_project(s, |p, pg, _page| {
        let doc = p.ledger.document();
        if !doc.conductors.contains_key(&conductor) {
            return Err("no such conductor".into());
        }
        let Some(cable) = doc.cables_of(conductor).first().copied() else {
            return Err(format!("Wire {} is not in a cable.", slash::wire_name(doc, conductor)));
        };
        let index = doc.cables.get(&cable).and_then(|c| c.members.iter().position(|m| m.conductor == conductor)).ok_or("no such conductor")?;
        let old = doc.cables.get(&cable).and_then(|c| c.members.get(index)).and_then(|m| m.core.clone());
        let dups = core.as_deref().map(|c| duplicates(doc, cable, conductor, c)).unwrap_or_default();
        let mut r = json!({
            "cable": cable.hex(), "conductor": conductor.hex(), "index": index,
            "core": core, "changed": old != core,
        });
        if !dups.is_empty() {
            r["duplicates"] = Value::Array(dups.iter().map(|w| Value::String(w.hex())).collect());
        }
        if old == core {
            return Ok(r);
        }
        p.apply_edit(pg, "cablecore", "Core label", core_ops(cable, index, conductor, old, core).to_vec())?;
        Ok(r)
    })
}

/// `cablecore {cable, cores}`: one label (or null) per member, in member order.
fn run_cablecores(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let cable: CableId = id(params, "cable").map_err(err)?;
    with_project(s, |p, pg, _page| {
        let doc = p.ledger.document();
        let members = doc.cables.get(&cable).ok_or("no such cable")?.members.clone();
        let wrong =
            || format!("`cores` must list one label (or null) per member: cable {} has {} members", views::cable_title(doc, cable), members.len());
        let list = params.get("cores").and_then(Value::as_array).ok_or_else(wrong)?;
        if list.len() != members.len() {
            return Err(wrong());
        }
        let mut cores = Vec::with_capacity(list.len());
        for (i, v) in list.iter().enumerate() {
            cores.push(core_value(Some(v)).map_err(|e| format!("Core {}: {e}", i + 1))?);
        }
        let mut ops = Vec::new();
        for (index, (m, new)) in members.iter().zip(&cores).enumerate() {
            if m.core != *new {
                ops.extend(core_ops(cable, index, m.conductor, m.core.clone(), new.clone()));
            }
        }
        let changed = !ops.is_empty();
        if changed {
            p.apply_edit(pg, "cablecore", "Core labels", ops)?;
        }
        Ok(json!({"cable": cable.hex(), "cores": cores, "changed": changed}))
    })
}

/// What `cablefan`'s `at` asks for.
enum JoinAt {
    Point(Pt),
    Fraction(f64),
    Middle,
}

/// `cablefan` (owner request on brief 009, 2026-11-07): where the trunk joins one fanned end of a
/// cable on the active page. The join is stored as a `fan-anchor` representation of the cable
/// holding one Point item (existing ops, no format change), snapped onto the spine; `null` removes
/// it and the trunk joins the middle again. Works whatever the cable's view (programmatic calls
/// ignore views).
fn run_cablefan(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let cable: CableId = id(params, "cable").map_err(err)?;
    let end = match params.get("end") {
        None | Some(Value::Null) => None,
        Some(Value::String(e)) if e == "A" => Some(crate::look::MarkerEnd::A),
        Some(Value::String(e)) if e == "B" => Some(crate::look::MarkerEnd::B),
        Some(_) => return Err(err("`end` must be \"A\" or \"B\"")),
    };
    let at = match params.get("at") {
        Some(Value::Null) => JoinAt::Middle,
        Some(Value::Number(n)) => match n.as_f64().filter(|f| f.is_finite() && (0.0..=1.0).contains(f)) {
            Some(f) => JoinAt::Fraction(f),
            None => return Err(err("`at` as a fraction must be from 0 to 1")),
        },
        Some(v @ Value::Array(_)) => match point(v) {
            Some(p) if p.x.abs() <= 1e12 && p.y.abs() <= 1e12 => JoinAt::Point(p),
            Some(_) => return Err(err("Point out of range.")),
            None => return Err(err("`at` must be a point [x,y], a fraction from 0 to 1, or null")),
        },
        _ => return Err(err("`at` must be a point [x,y], a fraction from 0 to 1, or null")),
    };
    with_project(s, |p, pg, page| {
        let doc = p.ledger.document();
        if !doc.cables.contains_key(&cable) {
            return Err("no such cable".into());
        }
        let title = views::cable_title(doc, cable);
        let ends = crate::look::fan_ends(doc, page, cable, p.tolerance, &crate::look::font_width);
        if ends.is_empty() {
            return Err(format!("Cable {title} has no fanned end on this page."));
        }
        let name = |e: crate::look::MarkerEnd| if e == crate::look::MarkerEnd::A { "A" } else { "B" };
        let fe = match (end, &at) {
            (Some(e), _) => {
                ends.iter().find(|f| f.end == e).ok_or_else(|| format!("End {} of cable {title} stops at a reference; it has no spine.", name(e)))?
            }
            (None, JoinAt::Point(q)) => ends
                .iter()
                .min_by(|a, b| {
                    let d = |f: &crate::look::FanEnd| f.spine.join_for(*q).map_or(f64::INFINITY, |j| j.dist(*q));
                    d(a).total_cmp(&d(b))
                })
                .ok_or("no fanned end")?,
            (None, _) => return Err("`end` (\"A\" or \"B\") is required unless `at` is a point".into()),
        };
        let sp = fe.spine;
        let join = match at {
            JoinAt::Point(q) => Some(sp.join_for(q).ok_or("Point out of range.")?),
            JoinAt::Fraction(f) => Some(sp.point(sp.tmin + f * (sp.tmax - sp.tmin))),
            JoinAt::Middle => None,
        };
        let mut r = json!({
            "cable": cable.hex(), "end": name(fe.end),
            "at": join.map(|j| json!([j.x, j.y])), "changed": true,
        });
        let ops = match (fe.anchor, join) {
            (None, None) => {
                r["changed"] = Value::Bool(false);
                return Ok(r);
            }
            (Some((_, _, old)), Some(j)) if old.near(j, 1e-9) => {
                r["changed"] = Value::Bool(false);
                return Ok(r);
            }
            (Some((rid, item, _)), None) => {
                let before = doc.items.get(&item).cloned().ok_or("anchor item missing")?;
                let rep = doc.representations.get(&rid).cloned().ok_or("anchor missing")?;
                vec![Op::UnlinkRepresentation { id: rid, rep }, Op::RemoveItem { id: item, item: before }]
            }
            (Some((_, item, _)), Some(j)) => {
                let before = doc.items.get(&item).cloned().ok_or("anchor item missing")?;
                let after = Item { kind: ItemKind::Primitive(Primitive::Point { at: j }), ..before.clone() };
                vec![Op::ModifyItem { id: item, before, after }]
            }
            (None, Some(j)) => {
                let (item, rid): (ItemId, RepresentationId) = (p.ids.fresh(), p.ids.fresh());
                let style = plain_style(p);
                vec![
                    Op::AddItem { id: item, item: Item { page, style, kind: ItemKind::Primitive(Primitive::Point { at: j }) } },
                    Op::LinkRepresentation {
                        id: rid,
                        rep: Representation {
                            subject: Subject::Cable(cable),
                            page,
                            form: Form::Drawn,
                            role: crate::look::ANCHOR_ROLE.into(),
                            items: vec![item],
                            connection_points: BTreeMap::new(),
                        },
                    },
                ]
            }
        };
        p.apply_edit(pg, "cablefan", "Cable join", ops)?;
        Ok(r)
    })
}

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
        // A conductor is in at most one cable (design 004, owner answer O5; §18.4 defers several).
        let mut blocked = Vec::new();
        for w in &conductors {
            if let Some(k) = doc.cables_of(*w).first() {
                blocked.push((*w, *k));
            }
        }
        if !blocked.is_empty() {
            return Err(slash::already_in_cable(doc, &blocked, "Nothing was changed."));
        }
        let mut ops = Vec::new();
        // Each wire once per call, and the cap holds for the whole cable, not only for one call.
        let mut seen = std::collections::BTreeSet::new();
        if let Some(w) = conductors.iter().find(|w| !seen.insert(**w)) {
            return Err(format!("`conductors` lists wire {} twice; nothing was changed", w.hex()));
        }
        let had = match existing {
            Some(c) => doc.cables.get(&c).ok_or("no such cable")?.members.len(),
            None => 0,
        };
        if had.saturating_add(conductors.len()) > MAX_CABLE_MEMBERS {
            return Err(format!("a cable holds at most {MAX_CABLE_MEMBERS} wires; nothing was changed"));
        }
        let (cable, mut index) = match existing {
            Some(c) => (c, had),
            None => {
                let c: CableId = p.ids.fresh();
                let mut cable = ecad_model::Cable::default();
                // Brief 026: every new cable gets a tag (`-W01`, or `-W?1` with provisional tags).
                let t = crate::devices::new_cable_tag(&p.ledger, &mut p.tag_high, tag.as_deref())?;
                cable.properties.insert(ecad_model::tag::TAG_KEY.into(), t);
                ops.push(Op::CreateCable { id: c, cable });
                (c, 0)
            }
        };
        // Brief 028 (design 028 §6.2, Q10): a cable with a core offer gives each new member the
        // next free core, in the order given.
        let mut spare: std::collections::VecDeque<String> =
            existing.and_then(|k| crate::strips::cable_cores(doc, k)).map(|c| c.spare.into_iter().collect()).unwrap_or_default();
        for w in conductors {
            ops.push(Op::AddCableMember { cable, index, member: CableMember { conductor: w, core: spare.pop_front() } });
            index += 1;
        }
        p.apply_edit(pg, "cableadd", "Add to cable", ops)?;
        Ok(json!({"cable": cable.hex()}))
    })
}

/// A slash end: a finite `[x,y]` within ±[`interact::MAX_COORD`].
fn slash_point(p: &Value, key: &str) -> Result<Pt, String> {
    let v = p.get(key).filter(|v| !v.is_null()).ok_or_else(|| format!("`{key}` [x,y] is required"))?;
    point(v)
        .filter(|q| q.x.abs() <= interact::MAX_COORD && q.y.abs() <= interact::MAX_COORD)
        .ok_or_else(|| format!("`{v}` is not a finite [x,y] point"))
}

/// `cableslash` (brief 004, design 004 §2, §4, §6): the conductors the slash `from → to` crosses
/// on the active page become one new cable, in crossing order, with the slash as its drawn
/// representation; one transaction. Refused when it crosses no wire, more than
/// [`MAX_CABLE_MEMBERS`], or a wire already in a cable.
fn run_cableslash(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let from = slash_point(params, "from").map_err(err)?;
    let to = slash_point(params, "to").map_err(err)?;
    let tag = opt_typed(params, "tag", "a string", |v| v.as_str().map(str::to_string)).map_err(err)?;
    let existing: Option<CableId> = opt_id(params, "cable").map_err(err)?;
    with_project(s, |p, pg, page| {
        if from.near(to, p.tolerance) {
            return Err("`from` and `to` must differ".into());
        }
        let doc = p.ledger.document();
        let found = slash::crossings(doc, page, from, to, p.tolerance);
        if found.members.len() > MAX_CABLE_MEMBERS {
            return Err(slash::too_many());
        }
        if found.members.is_empty() {
            return Err(slash::no_wires(found.ignored.len()));
        }
        // Brief 032 §3.3: an existing cable's ID or tag continues that cable on this page.
        let existing = match existing {
            Some(k) if doc.cables.contains_key(&k) => Some(k),
            Some(_) => return Err("no such cable".into()),
            None => tag.as_deref().and_then(|t| crate::references::cable_tagged(doc, t)),
        };
        if let Some(k) = existing {
            return crate::references::run_slash_continue(p, pg, page, k, from, to, &found);
        }
        let blocked: Vec<(ConductorId, CableId)> = slash::blocked(doc, &found.members).into_iter().map(|(m, k)| (m.conductor, k)).collect();
        if !blocked.is_empty() {
            return Err(slash::already_in_cable(doc, &blocked, "Nothing was created."));
        }
        let (cable, item, rep): (CableId, ItemId, RepresentationId) = (p.ids.fresh(), p.ids.fresh(), p.ids.fresh());
        let mut c = ecad_model::Cable::default();
        // Brief 026: every new cable gets a tag (`-W01`, or `-W?1` with provisional tags).
        let t = crate::devices::new_cable_tag(&p.ledger, &mut p.tag_high, tag.as_deref())?;
        c.properties.insert(ecad_model::tag::TAG_KEY.into(), t);
        let mut ops = vec![Op::CreateCable { id: cable, cable: c }];
        for (index, m) in found.members.iter().enumerate() {
            ops.push(Op::AddCableMember { cable, index, member: CableMember { conductor: m.conductor, core: None } });
        }
        ops.push(Op::AddItem { id: item, item: Item { page, style: plain_style(p), kind: ItemKind::Primitive(Primitive::Line { a: from, b: to }) } });
        ops.push(Op::LinkRepresentation {
            id: rep,
            rep: Representation {
                subject: Subject::Cable(cable),
                page,
                form: Form::Drawn,
                role: "slash".into(),
                items: vec![item],
                connection_points: BTreeMap::new(),
            },
        });
        p.apply_edit(pg, "cableslash", "Cable slash", ops)?;
        Ok(json!({
            "cable": cable.hex(),
            "item": item.hex(),
            "representation": rep.hex(),
            "conductors": found.members.iter().map(|m| m.conductor.hex()).collect::<Vec<_>>(),
            "ignored": found.ignored.iter().map(|(i, _)| i.hex()).collect::<Vec<_>>(),
            "touched": found.touched.iter().map(|c| c.hex()).collect::<Vec<_>>(),
        }))
    })
}

/// `cableview` (brief 005, design 005 §3.4): show a cable traditional or fan. Presentation only:
/// no transaction, no head move, no unsaved change, never refused because of the project's state
/// (read-only, an unavailable page). Without `cable` it changes nothing and lists every cable's
/// view.
fn run_cableview(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let cable: Option<CableId> = opt_id(params, "cable").map_err(err)?;
    let bad_view = || err("`view` must be \"traditional\" or \"fan\"");
    let view = match params.get("view") {
        None | Some(Value::Null) => None,
        Some(Value::String(t)) => Some(View::parse(t).ok_or_else(bad_view)?),
        Some(_) => return Err(bad_view()),
    };
    if cable.is_none() && view.is_some() {
        return Err(err("`view` needs `cable`"));
    }
    with_any_page(s, |p, pg| {
        let doc = p.ledger.document();
        let Some(k) = cable else {
            let views: serde_json::Map<String, Value> = doc.cables.keys().map(|k| (k.hex(), Value::String(p.view_of(*k).name().into()))).collect();
            return Ok(json!({ "views": views }));
        };
        if !doc.cables.contains_key(&k) {
            return Err("no such cable".into());
        }
        if p.faults.fail_view_switches > 0 {
            p.faults.fail_view_switches -= 1;
            return Err("injected view switch failure".into());
        }
        let current = p.view_of(k);
        let target = view.unwrap_or_else(|| current.toggled());
        if target == View::Fan && current != View::Fan && !views::offers_fan(doc, k) {
            return Err(format!("Cable {} {}", views::cable_title(doc, k), views::ONE_WIRE));
        }
        let (changed, stored) = p.set_view(pg, k, target);
        let mut r = json!({"cable": k.hex(), "view": target.name()});
        let remembered = match (&stored, p.settings.dir()) {
            (Ok(()), Some(_)) => true,
            (Err(e), Some(_)) => {
                r["reason"] = Value::String(e.clone());
                false
            }
            (_, None) => {
                if changed && !p.said_not_remembered {
                    p.said_not_remembered = true;
                    r["notice"] = Value::String(views::NOT_REMEMBERED.into());
                }
                false
            }
        };
        r["remembered"] = Value::Bool(remembered);
        if !changed {
            r["changed"] = Value::Bool(false);
        }
        Ok(r)
    })
}

/// The experiment's built-in definition: a two-terminal coil, 10 × 20, terminals A1 (top) and A2
/// (bottom).
pub fn builtin_coil() -> Definition {
    let r = [(-5.0, -10.0), (5.0, -10.0), (5.0, 10.0), (-5.0, 10.0)];
    Definition {
        bridges: Vec::new(),
        name: "Coil (experiment)".into(),
        origin: None,
        graphics: vec![Primitive::Polyline { vertices: r.iter().map(|(x, y)| Pt::new(*x, *y)).collect(), closed: true }],
        terminals: vec![
            TerminalDef { key: "A1".into(), label: Some("A1".into()), at: Pt::new(0.0, 10.0) },
            TerminalDef { key: "A2".into(), label: Some("A2".into()), at: Pt::new(0.0, -10.0) },
        ],
    }
}

fn run_place(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    // Brief 018: a starter symbol, as a new device or another symbol of one.
    if crate::device_cmds::is_symbol_place(params) {
        return crate::device_cmds::run_place(s, params);
    }
    let at = params.get("at").and_then(point).ok_or_else(|| err("`at` [x,y] is required"))?;
    let degrees = opt_typed(params, "rotation", "a finite number of degrees", |v| v.as_f64().filter(|d| d.is_finite())).map_err(err)?.unwrap_or(0.0);
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
        ops.push(Op::CreateComponent { id: c, component: Component { definition: Some(def_id), ..Component::default() } });
        let mut terminals = Vec::new();
        for td in &def.terminals {
            let t: TerminalId = p.ids.fresh();
            ops.push(Op::AddTerminal { id: t, terminal: Terminal { component: c, label: td.label.clone(), key: Some(td.key.clone()) } });
            terminals.push(json!({"key": td.key, "terminal": t.hex()}));
        }
        let placement = Placement { origin: at, rotation: degrees.to_radians(), mirror, scale: 1.0 };
        ops.push(Op::AddItem {
            id: item,
            item: Item { page, style: plain_style(p), kind: ItemKind::Symbol(SymbolInstance { definition: def_id, placement }) },
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
    // Brief 024 (FP1): a generated page's kind, set only here.
    let generated = match params.get("generated") {
        None | Some(Value::Null) => None,
        Some(Value::String(t)) => Some(
            crate::generated::GenKind::parse(t)
                .ok_or_else(|| err("`generated` is \"cover\", \"contents\", \"revisions\", \"bom-location\", \"terminals\" or \"cables\""))?,
        ),
        Some(_) => return Err(err("`generated` must be a string")),
    };
    let title = params.get("title").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| generated.map_or("new", |g| g.title()).to_string());
    let number = match params.get("number") {
        None | Some(Value::Null) => None,
        Some(Value::String(t)) => Some(t.clone()),
        Some(_) => return Err(err("`number` must be a string such as \"105\", or null for the next number")),
    };
    let folder: Option<ecad_model::FolderId> = opt_id(params, "folder").map_err(err)?;
    // Brief 028 (FP3): a terminal plan page may show one strip.
    let strip: Option<ComponentId> = opt_id(params, "strip").map_err(err)?;
    if strip.is_some() && generated != Some(crate::generated::GenKind::Terminals) {
        return Err(err("`strip` is for a terminal plan page (`generated: \"terminals\"`)"));
    }
    let (page, number) = with_project(s, |p, pg, _| {
        if let Some(c) = strip
            && !crate::strips::is_strip(p.ledger.document(), c)
        {
            return Err("That device is not a terminal strip; leave `strip` out to show every strip.".into());
        }
        let (id, number, mut ops) = crate::sheet_cmds::new_page_ops(p, title, number, folder)?;
        if let Some(g) = generated {
            for op in &mut ops {
                if let Op::AddPage { page, .. } = op {
                    page.properties.insert(crate::generated::KEY.into(), g.value().into());
                    if let Some(c) = strip {
                        page.properties.insert(crate::generated::STRIP_KEY.into(), c.hex());
                    }
                }
            }
        }
        p.apply_edit(pg, "newpage", "New page", ops)?;
        p.pages.entry(id).or_default();
        Ok((id, number))
    })?;
    let mut v = open(s, page)?;
    v["number"] = json!(number);
    Ok(v)
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

// ---------------------------------------------------------------- cable references (brief 010)

/// The result fields every reference command reports about one end, derived after the commit.
fn reference_fields(doc: &ecad_model::Document, reference: RepresentationId) -> (Value, bool, Value) {
    match crate::references::all(doc).into_iter().find(|r| r.reference == reference) {
        Some(r) => (Value::String(r.text.clone()), r.orphan, r.partner().map_or(Value::Null, |p| Value::String(p.hex()))),
        None => (Value::String("/?".into()), true, Value::Null),
    }
}

/// `cableref` (design 010 §2.1, §7.1): a reference end of `cable` on the active page, at `at`.
/// One transaction: the Point item and its `reference` representation. Refused (R1 to R3) when
/// the cable already has a reference here, already has two, or has no wire drawn here.
fn run_cableref(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let cable: CableId = id(params, "cable").map_err(err)?;
    let at = slash_point(params, "at").map_err(err)?;
    with_project(s, |p, pg, page| {
        let doc = p.ledger.document();
        crate::references::check_reference(doc, page, cable)?;
        let (item, rep): (ItemId, RepresentationId) = (p.ids.fresh(), p.ids.fresh());
        let ops = vec![
            Op::AddItem { id: item, item: Item { page, style: plain_style(p), kind: ItemKind::Primitive(Primitive::Point { at }) } },
            Op::LinkRepresentation {
                id: rep,
                rep: Representation {
                    subject: Subject::Cable(cable),
                    page,
                    form: Form::Drawn,
                    role: crate::references::REFERENCE_ROLE.into(),
                    items: vec![item],
                    connection_points: BTreeMap::new(),
                },
            },
        ];
        p.apply_edit(pg, "cableref", "Cable reference", ops)?;
        let (text, orphan, partner) = reference_fields(p.ledger.document(), rep);
        Ok(json!({
            "reference": rep.hex(),
            "item": item.hex(),
            "page": page.hex(),
            "text": text,
            "orphan": orphan,
            "partner": partner,
        }))
    })
}

/// The optional `members` list of `cablecontinue`: absent or `null` is every member; otherwise a
/// list of conductor ids, at most [`MAX_CABLE_MEMBERS`] (checked before anything is parsed).
fn member_list(p: &Value) -> Result<Option<Vec<ConductorId>>, String> {
    const BAD: &str = "`members` must be a list of conductor ids";
    match p.get("members") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(a)) => {
            if a.len() > MAX_CABLE_MEMBERS {
                return Err(crate::references::too_many_members());
            }
            a.iter()
                .map(|v| v.as_str().and_then(|t| u128::from_str_radix(t.trim(), 16).ok()).map(ConductorId::from).ok_or_else(|| BAD.to_string()))
                .collect::<Result<Vec<_>, _>>()
                .map(Some)
        }
        Some(_) => Err(BAD.into()),
    }
}

/// `cablecontinue` (design 010 §2.2, §5, §7.2): the second drawing of `cable` on the active page.
/// One transaction: a reference end at `at`, then for each continued member a stub that is a new
/// route of the **same** conductor, then a slash across the stubs. No conductor, attachment or
/// membership changes, so the nets are unchanged.
fn run_cablecontinue(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let cable: CableId = id(params, "cable").map_err(err)?;
    let at = slash_point(params, "at").map_err(err)?;
    let toward = slash_point(params, "toward").map_err(err)?;
    let members = member_list(params).map_err(err)?;
    with_project(s, |p, pg, page| {
        let doc = p.ledger.document();
        let plan = crate::references::plan_continue(doc, page, cable, members.as_deref(), at, toward, p.tolerance)?;
        let (item, rep): (ItemId, RepresentationId) = (p.ids.fresh(), p.ids.fresh());
        let style = plain_style(p);
        let mut ops = vec![
            Op::AddItem { id: item, item: Item { page, style: style.clone(), kind: ItemKind::Primitive(Primitive::Point { at }) } },
            Op::LinkRepresentation {
                id: rep,
                rep: Representation {
                    subject: Subject::Cable(cable),
                    page,
                    form: Form::Drawn,
                    role: crate::references::REFERENCE_ROLE.into(),
                    items: vec![item],
                    connection_points: BTreeMap::new(),
                },
            },
        ];
        let mut routes = Vec::with_capacity(plan.stubs.len());
        for stub in &plan.stubs {
            let (i, r): (ItemId, RepresentationId) = (p.ids.fresh(), p.ids.fresh());
            ops.push(Op::AddItem {
                id: i,
                item: Item { page, style: style.clone(), kind: ItemKind::Primitive(Primitive::Line { a: stub.a, b: stub.b }) },
            });
            ops.push(Op::LinkRepresentation {
                id: r,
                rep: Representation {
                    subject: Subject::Conductor(stub.conductor),
                    page,
                    form: Form::Drawn,
                    role: "route".into(),
                    items: vec![i],
                    connection_points: BTreeMap::new(),
                },
            });
            routes.push(json!({
                "conductor": stub.conductor.hex(),
                "representation": r.hex(),
                "item": i.hex(),
                "free_end": if stub.free_end == End::A { "A" } else { "B" },
            }));
        }
        let (slash_item, slash_rep): (ItemId, RepresentationId) = (p.ids.fresh(), p.ids.fresh());
        ops.push(Op::AddItem {
            id: slash_item,
            item: Item { page, style, kind: ItemKind::Primitive(Primitive::Line { a: plan.slash.0, b: plan.slash.1 }) },
        });
        ops.push(Op::LinkRepresentation {
            id: slash_rep,
            rep: Representation {
                subject: Subject::Cable(cable),
                page,
                form: Form::Drawn,
                role: "slash".into(),
                items: vec![slash_item],
                connection_points: BTreeMap::new(),
            },
        });
        p.apply_edit(pg, "cablecontinue", "Continue cable", ops)?;
        let doc = p.ledger.document();
        let (text, _, partner) = reference_fields(doc, rep);
        let skipped: Vec<Value> = plan.skipped.iter().map(|(w, why)| json!({"conductor": w.hex(), "reason": why.reason()})).collect();
        Ok(json!({
            "reference": rep.hex(),
            "item": item.hex(),
            "slash": slash_rep.hex(),
            "slash_item": slash_item.hex(),
            "routes": routes,
            "skipped": skipped,
            "text": text,
            "partner": partner,
            "messages": crate::references::continued_lines(doc, page, &plan, rep),
        }))
    })
}

/// `cablegoto` (design 010 §4.5, §7.3): open (or front) the page of the other end of
/// `reference`, centre the view on it and select its point. Records nothing; works on a read-only
/// project.
fn run_cablegoto(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let reference: RepresentationId = id(params, "reference").map_err(err)?;
    let (uid, page, other, item, at, opened, message) = with_any_page(s, |p, pg| {
        let doc = p.ledger.document();
        let refs = crate::references::all(doc);
        let here = refs.iter().find(|r| r.reference == reference).ok_or("no such reference")?;
        let other = here.partner().ok_or("this reference has no other end")?;
        let there = refs.iter().find(|r| r.reference == other).ok_or("this reference has no other end")?;
        let item = there.item.ok_or("the other end cannot be located")?;
        let at = there.at.ok_or("the other end cannot be located")?;
        let message = format!(
            "{}, at the other end of cable {} ({}).",
            crate::references::page_words(doc, there.page).replacen("page", "Page", 1),
            crate::views::cable_title(doc, there.cable),
            there.locator()
        );
        let page = there.page;
        let opened = p.pages.get(&page).and_then(|v| v.uid).is_none();
        let uid = p.open_page(pg, page)?;
        Ok((uid, page, other, item, at, opened, message))
    })?;
    if opened {
        let _ = s.zoom_extents();
    }
    if let Ok(st) = s.state_mut() {
        let height = st.view().height;
        let _ = st.set_view(cadcraft_engine::View { center: cadcraft_engine::geom::Vec2::new(at.x, at.y), height, twist: 0.0 });
    }
    let handle = with_any_page(s, |p, _| Ok(p.pages.get(&page).and_then(|v| v.map.item(item))))?;
    if let Some(h) = handle {
        s.set_selection(vec![h]);
    }
    Ok(json!({
        "page": page.hex(),
        "document": uid,
        "reference": other.hex(),
        "item": item.hex(),
        "message": message,
    }))
}

/// `cablerefs` (design 010 §7.4): every cable reference (or `cable`'s), derived; read only.
fn run_cablerefs(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let cable: Option<CableId> = opt_id(params, "cable").map_err(err)?;
    with_any_page(s, |p, _| {
        let doc = p.ledger.document();
        if let Some(k) = cable
            && !doc.cables.contains_key(&k)
        {
            return Err("no such cable".into());
        }
        let refs: Vec<_> = crate::references::all(doc).into_iter().filter(|r| cable.is_none_or(|k| r.cable == k)).collect();
        Ok(json!({ "references": ecad_inspect::references_json(doc, &refs) }))
    })
}

/// Most characters in a page title.
pub const MAX_PAGE_TITLE: usize = 200;

/// A page title: 1 to [`MAX_PAGE_TITLE`] characters after trimming, no control characters.
pub(crate) fn page_title(p: &Value) -> Result<String, String> {
    const BAD: &str = "`title` must be a non-empty string of at most 200 characters";
    let t = p.get("title").and_then(Value::as_str).ok_or(BAD)?.trim();
    let n = t.chars().count();
    if !(1..=MAX_PAGE_TITLE).contains(&n) || t.chars().any(char::is_control) {
        return Err(BAD.into());
    }
    Ok(t.to_string())
}

/// `renamepage` (design 010 §2.3, §7.5): one transaction, `RemovePage` + `AddPage` with the same
/// ID at the same position. The same title changes nothing.
fn run_renamepage(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let page: PageId = id(params, "page").map_err(err)?;
    let title = page_title(params).map_err(err)?;
    with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let old = doc.pages.get(&page).cloned().ok_or("no such page")?;
        let index = doc.page_order.iter().position(|x| *x == page).ok_or("no such page")?;
        if old.title == title {
            return Ok(json!({"page": page.hex(), "title": title, "index": index + 1, "changed": false}));
        }
        // The page as it is with only its title changed, so any other field of a page is kept.
        let mut renamed = old.clone();
        renamed.title = title.clone();
        let ops = vec![Op::RemovePage { id: page, page: old, index }, Op::AddPage { id: page, page: renamed, index }];
        p.apply_edit(pg, "renamepage", "Rename page", ops)?;
        Ok(json!({"page": page.hex(), "title": title, "index": index + 1, "changed": true}))
    })
}

/// `movepage` (design 010 §2.3, §7.5): one transaction, `RemovePage` at the old position +
/// `AddPage` at `index` (1 = first). The same position changes nothing.
fn run_movepage(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    if params.get("folder").is_some() {
        return crate::sheet_cmds::run_movepage_folder(s, params);
    }
    let page: PageId = id(params, "page").map_err(err)?;
    let wanted = params.get("index").and_then(Value::as_u64);
    with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let n = doc.page_order.len();
        let bad = || format!("`index` must be a whole number from 1 to {n}");
        let to = wanted.and_then(|i| usize::try_from(i).ok()).filter(|i| (1..=n).contains(i)).ok_or_else(bad)?;
        let old = doc.pages.get(&page).cloned().ok_or("no such page")?;
        let from = doc.page_order.iter().position(|x| *x == page).ok_or("no such page")?;
        if from + 1 == to {
            return Ok(json!({"page": page.hex(), "title": old.title, "index": to, "changed": false}));
        }
        let title = old.title.clone();
        let ops = vec![Op::RemovePage { id: page, page: old.clone(), index: from }, Op::AddPage { id: page, page: old, index: to - 1 }];
        p.apply_edit(pg, "movepage", "Move page", ops)?;
        Ok(json!({"page": page.hex(), "title": title, "index": to, "changed": true}))
    })
}

#[cfg(test)]
mod lane_tests {
    /// Lanes register in brief order, once each. A union merge of `lanes.rs` (see that module) can
    /// leave two lanes' lines swapped; this says which to swap.
    #[test]
    fn lanes_are_in_brief_order() {
        let briefs: Vec<&str> = crate::lanes::LANES.iter().map(|(b, _)| *b).collect();
        for pair in briefs.windows(2) {
            assert!(pair[0] < pair[1], "crate::lanes::LANES: {} must come after {}, once each", pair[0], pair[1]);
        }
    }

    /// Every ECAD command id is registered once, so the order of the lists in [`super::specs`]
    /// never decides which command a name finds (lookups take the first match).
    #[test]
    fn command_ids_are_unique() {
        let mut seen = std::collections::BTreeSet::new();
        let twice: Vec<&str> = super::specs().iter().map(|c| c.id).filter(|id| !seen.insert(*id)).collect();
        assert!(twice.is_empty(), "registered more than once: {twice:?}");
    }
}
