//! The cable checks: references, cores and the stale cores of a wire.

use super::places::*;
use super::*;

/// The checks of cable `k`'s reference ends (brief 027, design 027 §4.2): a cable with exactly one
/// reference end has an orphan (`/?`). Reads that cable's representations only.
pub fn reference_checks(doc: &Document, k: CableId) -> Vec<Check> {
    let references = crate::references::cables::references_of(doc, k);
    let [(rid, r)] = references.as_slice() else { return Vec::new() };
    let at = r.items.first().and_then(|i| doc.items().get(i)).and_then(|i| match &i.kind {
        ecad_model::ItemKind::Primitive(ecad_model::Primitive::Point { at }) if at.is_finite() => Some(*at),
        _ => None,
    });
    let which = format!("{} /?", crate::cables::cable_title(doc, k));
    vec![Check { kind: Kind::OrphanReference, page: Some(r.page), at, object: Object::Reference(*rid), which, fix: None }]
}

/// The checks of cable `k` (brief 026): a provisional tag; a tag another cable or a device holds.
pub fn cable_checks(doc: &Document, k: ecad_model::CableId) -> Vec<Check> {
    cable_checks_in(&mut Places::new(doc), k)
}

/// As [`cable_checks`], reading the book and grids through `places`.
pub(crate) fn cable_checks_in(places: &mut Places<'_>, k: ecad_model::CableId) -> Vec<Check> {
    let doc = places.doc();
    let mut out = Vec::new();
    // Brief 059 (design 059 §6): a cable not drawn yet has one Info row and no core rows; its
    // planned ends and glands are not checked until it is drawn.
    let drawn = crate::cables::is_drawn(doc, k);
    if !drawn && doc.cables().contains_key(&k) {
        let name = crate::cables::cable_title(doc, k);
        out.push(Check {
            kind: Kind::CableNotDrawn,
            page: None,
            at: None,
            object: Object::Cable(k),
            which: format!("{name} is not in the drawing yet."),
            fix: Some(Fix::AssignCable(k)),
        });
    }
    out.extend(crate::cables::value_checks(doc, k, drawn));
    core_place_checks(doc, k, &mut out);
    // Brief 057 (owner rule, board 2.06): every core is connected at both ends or ticked Not used.
    let open = if drawn { crate::styles::cable::open_cores(doc, k) } else { Vec::new() };
    if !open.is_empty() {
        let (page, at) = ecad_model::naming::numbering::rep_anchor(doc, Subject::Cable(k)).map_or((None, None), |(p, a)| (Some(p), Some(a)));
        let name = crate::cables::cable_title(doc, k);
        for (i, o) in open.into_iter().enumerate() {
            // Brief 059 (design 059 §6): an unwired core is "not assigned".
            let which = format!("{name} core {} is not assigned: slash or add a wire, or tick Not used.", o.label);
            out.push(Check {
                kind: Kind::CoreOpen,
                page,
                at,
                object: Object::Cable(k),
                which,
                fix: Some(Fix::CoreOpen(k, u16::try_from(i).unwrap_or(u16::MAX))),
            });
        }
    }
    let Some(cable) = doc.cables().get(&k) else { return out };
    let (Some(key), Some(t)) = (cable.tag_key(), cable.tag_shown()) else { return out };
    if !cable.is_provisional() && doc.cables_with(&key).len() < 2 && doc.components_with(&key).is_empty() {
        return out;
    }
    let (page, at) = ecad_model::naming::numbering::rep_anchor_in(places, Subject::Cable(k)).map_or((None, None), |(p, a)| (Some(p), Some(a)));
    if cable.is_provisional() {
        out.push(Check { kind: Kind::ProvisionalTag, page, at, object: Object::Cable(k), which: t.clone(), fix: Some(Fix::Finalise) });
    }
    if doc.cables_with(&key).len() > 1 {
        out.push(Check { kind: Kind::DuplicateTag, page, at, object: Object::Cable(k), which: format!("Two cables share {t}"), fix: None });
    } else if !doc.components_with(&key).is_empty() {
        out.push(Check { kind: Kind::DuplicateTag, page, at, object: Object::Cable(k), which: format!("A cable and a device share {t}"), fix: None });
    }
    out
}

/// The rows of cable `k`'s cores on lines (owner rule, 2026-11-18): a core whose line has one end
/// landed, and a core whose stored place is not the one its line now gives
/// ([`ecad_model::connectivity::wire_order::core_position`]). Reads only the cable's own cores and their lines.
pub(super) fn core_place_checks(doc: &Document, k: ecad_model::CableId, out: &mut Vec<Check>) {
    let Some(cable) = doc.cables().get(&k) else { return };
    let name = crate::cables::cable_title(doc, k);
    for (i, m) in cable.cores.iter().enumerate() {
        let core = m.label_str().map(str::to_string).unwrap_or_else(|| (i + 1).to_string());
        if let ecad_model::CorePosition::Wire(w) = m.position {
            if let Some(now) = wire_core_stale(doc, w) {
                let (page, at) = if doc.wires().contains_key(&w) { conductor_place(doc, w, None) } else { (None, None) };
                let which = format!("{name} core {core}: stored on a wire, but {now}.");
                out.push(Check { kind: Kind::CorePlaceStale, page, at, object: Object::Cable(k), which, fix: None });
            }
            continue;
        }
        let ecad_model::CorePosition::Route { route, landed } = m.position else { continue };
        let (page, at) = route_place(doc, route, None);
        let found = ecad_model::connectivity::wire_order::core_position(doc, route);
        if found != m.position {
            let now = match found {
                ecad_model::CorePosition::Wire(_) => "its line now carries a wire".to_string(),
                ecad_model::CorePosition::Route { landed: None, .. } => "its line has no end landed".to_string(),
                ecad_model::CorePosition::Route { landed: Some(t), .. } => format!("its line is landed at {}", wires::words::point_words(doc, t)),
                ecad_model::CorePosition::Unplaced => "its line is gone".to_string(),
            };
            let which = format!("{name} core {core}: stored on a line, but {now}.");
            out.push(Check { kind: Kind::CorePlaceStale, page, at, object: Object::Cable(k), which, fix: None });
        } else if let Some(t) = landed {
            let which = format!("{name} core {core} is landed at {} only.", wires::words::point_words(doc, t));
            out.push(Check { kind: Kind::CoreOneEnd, page, at, object: Object::Cable(k), which, fix: None });
        }
    }
}

/// Why a core stored on wire `w` is stale, in words: the wire is gone, or lines land at its ends
/// but none of them carries it. `None` when it holds (a wire with no line at its ends, made by a
/// command that draws none, is not judged).
pub(super) fn wire_core_stale(doc: &Document, w: WireId) -> Option<&'static str> {
    let Some(wire) = doc.wires().get(&w) else { return Some("its wire is gone") };
    let mut lines = wire.ends.iter().flat_map(|t| doc.routes_at(*t)).peekable();
    lines.peek()?;
    let carried = lines.any(|r| ecad_model::connectivity::wire_order::wire_of_route(doc, r) == Some(w));
    (!carried).then_some("no line carries that wire now")
}

/// The connection points `route` lands on, and those of each route it ends on (a T, one level).
pub(super) fn route_points(doc: &Document, route: &ecad_model::Route) -> Vec<ConnectionPointId> {
    let mut out: Vec<ConnectionPointId> = route.points_landed().collect();
    for on in route.ends.iter().filter_map(|e| e.route()) {
        if let Some(r) = doc.routes().get(&on) {
            out.extend(r.points_landed());
        }
    }
    out
}
