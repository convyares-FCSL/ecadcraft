//! The wiring checks: nets, routes, conductors, pages, connection points and aids (boxes).

use super::*;
use super::{grid::*, places::*};

/// The net checks of net `id`.
pub(crate) fn net_checks(doc: &Document, id: NetKey, net: &NetData) -> Vec<Check> {
    let mut out = Vec::new();
    let first = net.first_conductor();
    let (page, at) = first.map_or((None, None), |c| conductor_place(doc, c, None));
    let object = first.map_or(Object::None, Object::Wire);
    let joined = wiring::joined_potentials(doc, net);
    if !joined.is_empty() {
        // Point at a continuation in it, when it has one.
        let cont = net.connection_points.iter().find(|t| doc.connection_points.get(t).is_some_and(|x| x.owner.aid().is_some()));
        let (page, at, object) = match cont {
            Some(t) => {
                let (p, a) = connection_point_place(doc, *t);
                (p, a, Object::ConnectionPoint(*t))
            }
            None => (page, at, object),
        };
        out.push(Check { kind: Kind::JoinsPotentials, page, at, object, which: joined.join(" and "), fix: None });
    }
    if net.fixed.len() > 1 {
        out.push(Check { kind: Kind::TwoFixed, page, at, object, which: net.fixed.keys().cloned().collect::<Vec<_>>().join(", "), fix: None });
    }
    if let Some(number) = wiring::net_number(doc, net)
        && wiring::sharing(doc, &number).contains(&id)
    {
        out.push(Check { kind: Kind::SharedNumber, page, at, object, which: number, fix: None });
    }
    // A net with conductors, one connection point and no continuation.
    if !net.wires.is_empty() && net.connection_points.len() == 1 && net.names.is_empty() {
        let t = net.connection_points.first().copied();
        let on_aid = t.and_then(|t| doc.connection_points.get(&t)).is_some_and(|x| x.owner.aid().is_some());
        if !on_aid && let Some(t) = t {
            let (p, a) = connection_point_place(doc, t);
            out.push(Check {
                kind: Kind::SinglePoint,
                page: p,
                at: a,
                object: Object::ConnectionPoint(t),
                which: wires::words::point_words(doc, t),
                fix: None,
            });
        }
    }
    if net.potentials.len() > 1 {
        let main = wiring::net_potential(doc, net).name;
        let odd = net.wires.iter().find(|c| doc.wires.get(c).and_then(|x| x.potential_name()).is_some_and(|p| Some(p) != main.as_deref()));
        let (p, a) = odd.map_or((page, at), |c| conductor_place(doc, *c, None));
        out.push(Check {
            kind: Kind::MixedPotentials,
            page: p,
            at: a,
            object: odd.map_or(object, |c| Object::Wire(*c)),
            which: net.potentials.keys().cloned().collect::<Vec<_>>().join(", "),
            fix: None,
        });
    }
    // A continuation name that is neither in the potentials table nor the net's own number (a
    // pair placed by Merge the nets): a warning, never a refusal (owner, 2026-11-09). Only names
    // in the table give a net its potential (design 022 §6 rule 2).
    for key in &net.names {
        let numbered = net.numbers.keys().any(|n| name_key(n) == *key);
        if doc.settings.wiring.potential(key).is_some() || numbered {
            continue;
        }
        let cont = net.connection_points.iter().find_map(|t| {
            let a = doc.connection_points.get(t)?.owner.aid()?;
            (crate::continuations::name_of(doc, a).as_ref() == Some(key)).then_some((*t, a))
        });
        let (p, a, o, which) = match cont {
            Some((t, c)) => {
                let (p, a) = connection_point_place(doc, t);
                (p, a, Object::ConnectionPoint(t), crate::continuations::display_name(doc, c))
            }
            None => (page, at, object, key.clone()),
        };
        out.push(Check { kind: Kind::UnknownName, page: p, at: a, object: o, which, fix: None });
    }
    out
}

// 032: the Looks connected row's words and fix (design 032 §2.7): free route ends that meet are
// joined (`joinwires`): merged, or landed on the route (a T).
pub(super) fn looks_connected_words() -> &'static str {
    "Looks connected"
}

// 032: see `looks_connected_words`.
pub(super) fn looks_connected_fix(at: Pt) -> Fix {
    Fix::Join(at)
}

/// The checks of route `r` (design 072 §4.3): an open end (one free end), a loose route (no end
/// on a connection point or another route), a line with no wire yet (any free end), a free end
/// that looks connected, an end drawn away from its connection point.
pub(crate) fn route_checks(doc: &Document, r: RouteId, grid: &Grid, tol: f64) -> Vec<Check> {
    let mut out = Vec::new();
    let Some(route) = doc.routes.get(&r) else { return out };
    let which = route_words(doc, r);
    let page = route.page;
    let v = &route.points;
    let loose: Vec<End> = End::BOTH.into_iter().zip(route.ends).filter(|(_, e)| *e == RouteEnd::Free).map(|(end, _)| end).collect();
    match loose.len() {
        2 => {
            let (page, at) = route_place(doc, r, None);
            out.push(Check { kind: Kind::LooseWire, page, at, object: Object::Route(r), which: which.clone(), fix: None });
        }
        1 => {
            let (page, at) = route_place(doc, r, loose.first().copied());
            out.push(Check { kind: Kind::OpenEnd, page, at, object: Object::Route(r), which: which.clone(), fix: None });
        }
        _ => {}
    }
    // A line with a free end carries no wire yet (owner, 2026-11-18): at its first free end.
    if let Some(end) = loose.first().copied() {
        let (page, at) = route_place(doc, r, Some(end));
        out.push(Check { kind: Kind::NoWireYet, page, at, object: Object::Route(r), which: which.clone(), fix: None });
    }
    // Looks connected: a free end on another route's interior, or on another free end (reported
    // by the lower route ID only, so the pair is one row).
    for end in &loose {
        let at = if *end == End::A { v.first() } else { v.last() };
        let Some(at) = at.copied() else { continue };
        if let Some((other, is_end)) = grid.under(page, at, r) {
            if is_end && other < r {
                continue;
            }
            out.push(Check {
                kind: Kind::LooksConnected,
                page: Some(page),
                at: Some(at),
                object: Object::Route(r),
                which: which.clone(),
                fix: Some(looks_connected_fix(at)),
            });
        }
    }
    // Drawn away: an end on a connection point that is not where the point is on this page.
    for (at, e) in [v.first(), v.last()].into_iter().zip(route.ends) {
        let (Some(t), Some(at)) = (e.point(), at) else { continue };
        if let Some(tp) = ecad_model::connectivity::derive::connection_point_position(doc, page, t)
            && !tp.near(*at, tol.max(1e-6))
        {
            out.push(Check { kind: Kind::DrawnAway, page: Some(page), at: Some(*at), object: Object::Route(r), which: which.clone(), fix: None });
        }
    }
    out
}

/// The checks of conductor `c`: a colour code the app cannot draw.
pub(crate) fn conductor_checks(doc: &Document, c: WireId) -> Vec<Check> {
    let mut out = Vec::new();
    if !doc.wires.contains_key(&c) {
        return out;
    }
    let which = conductor_words(doc, c);
    // Brief 040: a colour code the app cannot draw is drawn in ink and listed as information.
    if let Some(code) = crate::cables::words::conductor_colour(doc, c).filter(|code| code.list().len() > 2) {
        let (page, at) = conductor_place(doc, c, None);
        out.push(Check { kind: Kind::UnknownColour, page, at, object: Object::Wire(c), which: format!("{which} {code}"), fix: None });
    }
    out
}

/// The checks of `page` as a whole: brief 040's dash budget ([`crate::cables::words::dash_budget`]).
pub fn page_checks(doc: &Document, page: PageId) -> Vec<Check> {
    let left = crate::cables::words::dash_budget(doc, page).left_out;
    if left.is_empty() {
        return Vec::new();
    }
    let n = left.len();
    let which = if n == 1 { "1 wire".to_string() } else { format!("{n} wires") };
    vec![Check { kind: Kind::DashesLeftOut, page: Some(page), at: None, object: Object::None, which, fix: None }]
}

/// Whether connection point `t` is one its symbol marks quiet (brief 034, design 034 §11): a
/// spare connector pin, fuse output or card supply point may stay unwired without an
/// `Unconnected` check. Every other check on it still runs.
pub(super) fn is_quiet_point(doc: &Document, t: ConnectionPointId) -> bool {
    let Some(c) = doc.connection_points.get(&t).and_then(|x| x.device()) else { return false };
    doc.representations_of(Subject::Device(c)).any(|(_, r)| {
        let Some(s) = crate::devices::tags::rep_starter(doc, r) else { return false };
        let Some((def, _)) = crate::devices::tags::rep_definition(doc, r) else { return false };
        crate::devices::merge::shown_connection_points(doc, c, r)
            .iter()
            .filter(|(x, _)| *x == t)
            .filter_map(|(_, at)| def.connection_points.iter().find(|td| td.at.near(*at, 1e-9)))
            .any(|td| crate::symbols::rules::is_quiet(s, &td.key.to_string()))
    })
}

/// The checks of connection point `t`: more wires than it accepts (design 072 §4.4: `-X1:3 takes
/// 2 conductors; 3 are connected`); a device point with nothing attached.
pub fn connection_point_checks(doc: &Document, t: ConnectionPointId) -> Vec<Check> {
    let Some(term) = doc.connection_points.get(&t) else { return Vec::new() };
    let mut out = Vec::new();
    let (wired, takes) = (doc.ends_at(t).len(), ecad_model::connectivity::wire_order::accepts(doc, t));
    if wired > takes {
        let (page, at) = connection_point_place(doc, t);
        let which = format!("{} takes {takes} conductors; {wired} are connected", wires::words::point_words(doc, t));
        out.push(Check { kind: Kind::OverCount, page, at, object: Object::ConnectionPoint(t), which, fix: None });
    }
    if term.owner.aid().is_some() || doc.has_ends(t) || doc.routes_at(t).next().is_some() {
        return out;
    }
    // Bridged inside its device to an attached point: connected.
    let conn = doc.connectivity();
    if conn.net_of_connection_point(t).and_then(|id| conn.net(id)).is_some_and(|n| !n.wires.is_empty()) {
        return out;
    }
    if is_quiet_point(doc, t) {
        return out;
    }
    let (page, at) = connection_point_place(doc, t);
    out.push(Check { kind: Kind::Unconnected, page, at, object: Object::ConnectionPoint(t), which: wires::words::point_words(doc, t), fix: None });
    out
}

/// The checks of drawing aid `a` (design 072 §6.2): a continuation with no partner (the row
/// points at its connection point); a location box whose function or location is no longer in
/// the list (design 052 §6).
pub(crate) fn aid_checks(doc: &Document, a: ecad_model::AidId) -> Vec<Check> {
    if let Some(b) = crate::locations::box_values(doc, a) {
        return box_checks(doc, a, b);
    }
    let Some(_) = crate::continuations::continuation(doc, a) else { return Vec::new() };
    let partners = crate::continuations::name_of(doc, a).map_or(0, |k| doc.continuations_named(&k).len());
    if partners >= 2 {
        return Vec::new();
    }
    let t = doc.connection_points_of(a).next().map(|(t, _)| t);
    let (page, at) = t.map_or((None, None), |t| connection_point_place(doc, t));
    let object = t.map_or(Object::None, Object::ConnectionPoint);
    vec![Check { kind: Kind::LonelyContinuation, page, at, object, which: crate::continuations::display_name(doc, a), fix: None }]
}

/// Box `a`'s rows: a stored function or location naming no entry. The row points at its outline.
pub(super) fn box_checks(doc: &Document, a: ecad_model::AidId, b: &ecad_model::LocationBox) -> Vec<Check> {
    use crate::locations::designations::{self as d, Aspect};
    let (page, at) = doc.representations_of(Subject::Aid(a)).next().map_or((None, None), |(_, r)| (Some(r.page), crate::locations::anchor(doc, r)));
    Aspect::ALL
        .into_iter()
        .filter(|x| d::box_key(b, *x).is_some_and(|k| d::code_of(doc, *x, &k).is_none()))
        .map(|x| Check {
            kind: Kind::DesignationMissing,
            page,
            at,
            object: Object::Aid(a),
            which: format!("A box's {} is no longer in the list", x.word()),
            fix: Some(Fix::ShowDesignations),
        })
        .collect()
}
