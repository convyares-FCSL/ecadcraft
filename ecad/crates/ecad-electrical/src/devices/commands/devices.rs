//! The rules of the device commands (design 018 §6–§8; experiments 004 and 008): tag settings
//! (`tagsettings`), deleting a wire, device or cable (`ecaddelete`, `devicedelete`), and the ops of
//! `convertcomponent` and `addterminal`. Typed input in, ops and a plain result (or a
//! [`CmdError`]) out.
//!
//! Excludes: what a handle on the page is (the engine crate's page map), committing and the JSON
//! answer (the engine crate's shims).

use serde_json::Value;
use std::collections::BTreeMap;

use ecad_model::{
    CableId, Device, DeviceId, Document, Form, IdAllocator, ItemId, Op, PageId, ProjectSettings, Pt, Representation, RepresentationId, RouteEnd,
    RouteId, Subject, Terminal, ConnectionPointId, WireId,
};

use crate::devices;
use crate::error::CmdError;

// ---------------------------------------------------------------- tagsettings

/// Tag settings to write: the padding, and per starter symbol key the prefix (`None`: back to the
/// built-in one).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TagSettings {
    pub padding: Option<u8>,
    pub prefixes: Vec<(String, Option<String>)>,
}

/// The tag settings `tagsettings` is given (brief 018, owner decision 2026-11-08): `padding` digits,
/// and per starter symbol the project's own prefix (`None`: the built-in one).
///
/// # Errors
/// A padding out of range, an unknown symbol, a prefix that is not 1 to
/// [`devices::MAX_PREFIX_LETTERS`] letters.
pub fn tag_settings(padding: Option<u64>, prefixes: impl IntoIterator<Item = (String, Option<String>)>) -> Result<TagSettings, CmdError> {
    let max = ecad_model::tag::MAX_PADDING;
    let padding = match padding {
        None => None,
        Some(n) => Some(
            Some(n)
                .filter(|n| (1..=u64::from(max)).contains(n))
                .and_then(|n| u8::try_from(n).ok())
                .ok_or_else(|| CmdError::refused(format!("`padding` must be a whole number from 1 to {max}")))?,
        ),
    };
    let mut changes = Vec::new();
    for (k, v) in prefixes {
        let symbol = devices::starter(&k)?;
        let prefix = match v {
            None => None,
            Some(t) => Some(
                devices::valid_prefix(&t)
                    .ok_or_else(|| CmdError::refused(format!("A prefix is 1 to {} letters, for example K or SR.", devices::MAX_PREFIX_LETTERS)))?,
            ),
        };
        changes.push((symbol.key.to_string(), prefix));
    }
    Ok(TagSettings { padding, prefixes: changes })
}

/// Apply tag settings read by [`tag_settings`] to `after`: a prefix equal to the built-in one is
/// stored as none.
pub fn apply_tag_settings(after: &mut ProjectSettings, settings: TagSettings) {
    if let Some(n) = settings.padding {
        after.tag_padding = Some(n);
    }
    for (k, v) in settings.prefixes {
        crate::devices::set_symbol_prefix(after, ecad_model::prefix::PrefixOf::Starter(k.into()), v.as_deref());
    }
}

// ---------------------------------------------------------------- ecaddelete

/// What `ecaddelete` deletes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Doomed {
    /// A wire: the routes drawn straight between its two connection points.
    Wire(WireId),
    /// A drawn route.
    Route(RouteId),
    Device(DeviceId),
    Cable(CableId),
}

/// A planned delete.
#[derive(Debug, Clone, PartialEq)]
pub struct Deleted {
    pub ops: Vec<Op>,
    /// A rack's cards that left it (brief 045).
    pub released: usize,
    /// A cable's wires, which stay (design 004 §8).
    pub kept: Option<usize>,
    /// What to say, when there is something to say.
    pub message: Option<String>,
}

/// Ops removing every representation of `subject` with its items.
fn remove_representations(doc: &Document, subject: Subject) -> Vec<Op> {
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

fn kept_words(n: usize) -> String {
    match n {
        0 => "0 wires are".into(),
        1 => "1 wire is".into(),
        n => format!("{n} wires are"),
    }
}

/// The ops removing routes `routes` with their drawings; routes landing on them end free.
fn remove_routes(doc: &Document, routes: &[RouteId]) -> Vec<Op> {
    let mut edits = crate::wires::RouteEdits::new(doc);
    for r in routes {
        edits.free_tees(*r);
    }
    for r in routes {
        edits.forget(*r);
    }
    let mut ops = edits.ops();
    for r in routes {
        ops.extend(crate::wires::remove_route_ops(doc, *r));
    }
    ops
}

/// `ecaddelete {conductor} | {route} | {component} | {cable}`: the object with its drawings, in
/// one transaction. A wire goes with the routes drawn straight between its two connection points
/// (the commit's rewrite takes it away) and leaves its cables; a route goes with its drawing, and
/// routes landing on it end free; a device's connection points let go of their routes and a
/// rack's cards leave it (design 045 §2.2, FP1); a cable's wires stay (design 004 §8, owner
/// answer O3).
///
/// # Errors
/// No such conductor, route, component or cable; a wire with no route drawn straight between its
/// points.
pub fn plan_delete(doc: &Document, doomed: Doomed) -> Result<Deleted, CmdError> {
    let mut ops = Vec::new();
    match doomed {
        Doomed::Wire(c) => {
            let wire = doc.wires.get(&c).ok_or(CmdError::refused("no such conductor"))?;
            let [a, b] = wire.ends;
            let direct: Vec<RouteId> = doc
                .routes_at(a)
                .filter(|r| doc.routes.get(r).is_some_and(|x| x.ends.contains(&RouteEnd::Point(a)) && x.ends.contains(&RouteEnd::Point(b))))
                .collect();
            if direct.is_empty() {
                return Err(CmdError::refused("This wire is not drawn as one line between its two points; erase the lines it runs along instead."));
            }
            for cable in doc.cables_of(c) {
                if let Some(k) = doc.cables.get(&cable) {
                    let after = ecad_model::Cable { cores: k.cores.iter().filter(|m| m.wire() != Some(c)).cloned().collect(), ..k.clone() };
                    ops.extend(Op::modify_cable(cable, k, after));
                }
            }
            ops.extend(remove_routes(doc, &direct));
            Ok(Deleted { ops, released: 0, kept: None, message: None })
        }
        Doomed::Route(r) => {
            if !doc.routes.contains_key(&r) {
                return Err(CmdError::refused("no such wire"));
            }
            Ok(Deleted { ops: remove_routes(doc, &[r]), released: 0, kept: None, message: None })
        }
        Doomed::Device(c) => {
            let component = doc.devices.get(&c).ok_or(CmdError::refused("no such component"))?;
            ops.extend(crate::wires::free_points_ops(doc, doc.connection_points_of(c).map(|(t, _)| t)));
            ops.extend(remove_representations(doc, Subject::Device(c)));
            for (t, term) in doc.connection_points_of(c) {
                ops.push(Op::RemoveConnectionPoint { id: t, terminal: term.clone() });
            }
            // Brief 045 (design 045 §2.2, FP1): a rack's cards leave it in the same transaction.
            let (released_ops, released) =
                if crate::plc::edit::station_of(doc, c).is_some() { crate::plc::rack::release_cards(doc, c) } else { (Vec::new(), 0) };
            ops.extend(released_ops);
            ops.extend(super::device_link::unlink_ops(doc, c));
            ops.push(Op::DeleteDevice { id: c, device: component.clone() });
            let mut message =
                (released > 0).then(|| format!("Deleted {}; {}", devices::tag_words(doc, c), crate::plc::rack::released_words(released)));
            if let Some(cleared) = crate::rating::trace::cleared_words(doc, c) {
                message = Some(format!("{}; {cleared}", message.unwrap_or_else(|| format!("Deleted {}", devices::tag_words(doc, c)))));
            }
            Ok(Deleted { ops, released, kept: None, message })
        }
        Doomed::Cable(k) => {
            let cable = doc.cables.get(&k).ok_or(CmdError::refused("no such cable"))?;
            // Its named points go with it (design 072 §4.4, F4): the lines on them end free.
            ops.extend(crate::wires::free_points_ops(doc, cable.cores.iter().flat_map(|c| c.through.iter().copied())));
            ops.extend(remove_representations(doc, Subject::Cable(k)));
            ops.push(Op::DeleteCable { id: k, cable: cable.clone() });
            let kept = cable.cores.iter().filter(|m| m.position != ecad_model::CorePosition::Unplaced).count();
            let message = format!("Cable {} deleted; its {} kept.", k.hex(), kept_words(kept));
            Ok(Deleted { ops, released: 0, kept: Some(kept), message: Some(message) })
        }
    }
}

// ---------------------------------------------------------------- convertcomponent, addterminal

/// A component made of drawn geometry ([`plan_convert`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Converted {
    pub component: DeviceId,
    pub representation: RepresentationId,
    pub ops: Vec<Op>,
}

/// `convertcomponent` (experiment 004): plain geometry `items` on `page` become one new component,
/// drawn.
pub fn plan_convert(ids: &mut IdAllocator, page: PageId, items: Vec<ItemId>) -> Converted {
    let (component, representation): (DeviceId, RepresentationId) = (ids.fresh(), ids.fresh());
    let ops = vec![
        Op::CreateDevice { id: component, device: Device::default() },
        Op::LinkRepresentation {
            id: representation,
            rep: Representation {
                subject: Subject::Device(component),
                page,
                form: Form::Drawn,
                role: ecad_model::Role::Drawn,
                items,
                connection_points: BTreeMap::new(),
            },
        },
    ];
    Converted { component, representation, ops }
}

/// The refusal of an item that belongs to no component.
pub const NOT_A_COMPONENTS_ITEM: &str = "that item is not part of a component";

/// `addterminal` (experiment 008): a connection point labelled `label` at `at` on the drawn
/// component that `item` is part of.
///
/// # Errors
/// An item of no component; a placed symbol (its connection points come from its definition).
pub fn plan_addterminal(
    doc: &Document, ids: &mut IdAllocator, item: ItemId, at: Pt, label: Option<String>,
) -> Result<(ConnectionPointId, Vec<Op>), CmdError> {
    let (rid, r) = doc.representation_of_item(item).ok_or(CmdError::refused(NOT_A_COMPONENTS_ITEM))?;
    let Subject::Device(c) = r.subject else { return Err(CmdError::refused(NOT_A_COMPONENTS_ITEM)) };
    if r.form != Form::Drawn {
        return Err(CmdError::refused("connection points of placed symbols come from their definition"));
    }
    let t: ConnectionPointId = ids.fresh();
    let mut after = r.clone();
    after.connection_points.insert(t, at);
    let ops = vec![
        Op::AddConnectionPoint { id: t, terminal: Terminal { owner: c.into(), label, key: None, accepts: Default::default() } },
        Op::ModifyRepresentation { id: rid, before: r.clone(), after },
    ];
    Ok((t, ops))
}

/// Every starter symbol's prefix as this project uses it (`-K`).
pub fn prefixes_json(doc: &ecad_model::Document) -> serde_json::Map<String, Value> {
    crate::symbols::STARTERS
        .iter()
        // Junctions and continuations have no tag (brief 022).
        .filter(|s| s.kind != crate::symbols::Kind::Tagless)
        .map(|s| (s.key.to_string(), Value::String(format!("-{}", devices::prefix_for(doc, s)))))
        .collect()
}
