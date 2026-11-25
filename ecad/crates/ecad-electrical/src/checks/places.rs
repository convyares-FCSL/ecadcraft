//! Where a check's subject is on the drawing, and how it is named in a row.

use super::*;
/// Words for a net: its number, or its first device point.
pub(super) fn net_words(doc: &Document, net: &NetData) -> String {
    wiring::net_number(doc, net).unwrap_or_else(|| wires::words::net_points(doc, net).first().cloned().unwrap_or_default())
}

/// Where a conductor is: the first page in book order of the routes drawing it
/// ([`wires::routes::routes_of_wire`]) and the midpoint of that route's longest segment, or where a given
/// end is drawn.
pub(crate) fn conductor_place(doc: &Document, c: WireId, end: Option<End>) -> (Option<PageId>, Option<Pt>) {
    let order: HashMap<PageId, usize> = doc.page_order().iter().enumerate().map(|(i, p)| (*p, i)).collect();
    let pos = |p: &PageId| order.get(p).copied().unwrap_or(usize::MAX);
    if let (Some(e), Some(w)) = (end, doc.wires().get(&c)) {
        return wires::routes::drawn_ends_at(doc, w.end(e)).into_iter().min_by_key(|(p, _)| pos(p)).map_or((None, None), |(p, a)| (Some(p), Some(a)));
    }
    let routes: Vec<RouteId> = wires::routes::routes_of_wire(doc, c);
    let Some(r) = routes.iter().filter_map(|r| doc.routes().get(r)).min_by_key(|r| pos(&r.page)) else {
        return (None, None);
    };
    (Some(r.page), wires::labels::longest_segment(&r.points).map(|(a, b)| Pt::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)))
}

/// Where a route is: its page and the midpoint of its longest segment, or a given end.
pub(crate) fn route_place(doc: &Document, r: RouteId, end: Option<End>) -> (Option<PageId>, Option<Pt>) {
    let Some(route) = doc.routes().get(&r) else { return (None, None) };
    let v = &route.points;
    let at = match end {
        Some(End::A) => v.first().copied(),
        Some(End::B) => v.last().copied(),
        None => wires::labels::longest_segment(v).map(|(a, b)| Pt::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)),
    };
    (Some(route.page), at)
}

/// A route's words: the words of the wire it stands for, else `a wire with no connection point`.
pub(super) fn route_words(doc: &Document, r: RouteId) -> String {
    wires::routes::wire_of_route(doc, r).map_or_else(|| "a wire with no connection point".into(), |w| conductor_words(doc, w))
}

/// Where a connection point is shown first (book order).
pub(super) fn connection_point_place(doc: &Document, t: ConnectionPointId) -> (Option<PageId>, Option<Pt>) {
    // A device's point or a drawing aid's (a continuation's).
    let Some(owner) = doc.connection_points().get(&t).map(|x| x.owner.subject()) else { return (None, None) };
    for page in doc.page_order() {
        let shown = doc.representations_of(owner).any(|(_, r)| r.page == *page);
        if shown && let Some(at) = ecad_model::connectivity::derive::connection_point_position(doc, *page, t) {
            return (Some(*page), Some(at));
        }
    }
    (None, None)
}

/// A conductor's words: its cable and core, or its net's number.
pub(super) fn conductor_words(doc: &Document, c: WireId) -> String {
    if let Some(k) = doc.cables_of(c).first() {
        let core = doc
            .cables()
            .get(k)
            .and_then(|x| x.cores.iter().find(|m| m.wire() == Some(c)))
            .and_then(|m| m.label_str().map(str::to_string))
            .unwrap_or_default();
        return format!("{} {core}", crate::references::cables::cable_words(doc, *k)).trim().to_string();
    }
    let conn = doc.connectivity();
    conn.net_of_conductor(c).and_then(|id| conn.net(id)).map(|n| net_words(doc, n)).unwrap_or_default()
}
