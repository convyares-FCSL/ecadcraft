//! What the inspector reads about one selected device to lay
//! its Device and Layout tabs out: the header's words, the rows of "Where it is drawn", the
//! connections, the symbol's placement and the size each text slot takes. A plain copy built
//! with the inspector's other rows (the same update map); nothing here writes.

use std::collections::BTreeSet;

use ecad_model::look::TextKind;
use ecad_model::{ConnectionPointId, DeviceId, Document, ObjectRef, PageId, RepresentationId};

/// At most this many rows in "Where it is drawn" and "Connections" (hostile files can hold
/// thousands; the full lists are in Project).
pub(crate) const MAX_ROWS: usize = 64;
/// At most this many names of other connection points on one connection row.
const MAX_OTHERS: usize = 3;

/// One row of "Connections": a connection point, its wire and what else is on that wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnRow {
    pub pin: String,
    /// `wire 24V-04`; `not connected`.
    pub wire: String,
    /// `-S02:14, -K01:14`.
    pub others: String,
}

/// The selected symbol's placement, read from the drawing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    pub x: f64,
    pub y: f64,
    /// Degrees, 0 to 360.
    pub rotation: f64,
    pub mirror: bool,
}

/// What the inspector shows about one device and its selected symbol.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DeviceDetails {
    pub component: Option<DeviceId>,
    pub rep: Option<RepresentationId>,
    pub tag: String,
    pub description: String,
    /// `coil A1–A2`.
    pub symbol_words: String,
    /// `2/D4`.
    pub place: String,
    /// The symbol's name (`Contactor / relay coil`).
    pub symbol_name: String,
    /// The standard's name (`IEC`).
    pub standard: String,
    pub drawn: Vec<crate::references::drawn::DrawnRow>,
    /// How many rows of [`DeviceDetails::drawn`] are drawn.
    pub drawn_count: usize,
    pub connections: Vec<ConnRow>,
    pub placed: Option<Placed>,
    /// The device's connection points (its own checks are about these or the device).
    pub connection_points: BTreeSet<ConnectionPointId>,
    /// The size each text slot takes (`2.5 B`), by kind.
    pub sizes: Vec<(TextKind, String)>,
}

/// A size as the Layout tab shows it: `2.5` and ` B` when bold.
pub(crate) fn size_words(s: &ecad_model::look::TextStyle) -> String {
    let h = (s.height * 100.0).round() / 100.0;
    let bold = matches!(s.weight, ecad_model::objects::styles::text::Weight::Bold);
    format!("{h}{}", if bold { " B" } else { "" })
}

/// Build the details of device `c` with symbol `rep` selected, adding what they read to `deps`.
pub(crate) fn build(doc: &Document, c: DeviceId, rep: RepresentationId, kinds: &[TextKind], deps: &mut BTreeSet<ObjectRef>) -> DeviceDetails {
    let mut d = DeviceDetails { component: Some(c), rep: Some(rep), ..DeviceDetails::default() };
    d.tag = ecad_electrical::devices::tags::tag_words(doc, c);
    d.description = doc.devices().get(&c).and_then(|x| x.description.as_ref()).map(|t| t.to_string()).unwrap_or_default();
    d.standard = doc.standard().map_or_else(|| "IEC".to_string(), |s| s.header.name.to_string());
    let dev = ecad_electrical::references::devices::classify::device(doc, c);
    let mut places = ecad_electrical::references::devices::places::Places::new(doc);
    let place_of = |places: &mut ecad_electrical::references::devices::places::Places<'_>, r: &ecad_model::Representation| {
        places.text(r.page, ecad_electrical::references::devices::classify::anchor(doc, r))
    };
    if let Some(r) = doc.representations().get(&rep) {
        deps.insert(ObjectRef::Page(r.page));
        d.symbol_name =
            ecad_electrical::devices::tags::rep_definition(doc, r).map_or_else(|| "Drawn symbol".to_string(), |(d, _)| d.name.to_string());
        let role = if dev.head == Some(rep) {
            "coil".to_string()
        } else if dev.children.contains(&rep) {
            "contact".to_string()
        } else {
            d.symbol_name.to_lowercase()
        };
        let labels = crate::references::drawn::rep_labels(doc, c, r);
        d.symbol_words = if labels.is_empty() { role } else { format!("{role} {}", crate::references::drawn::pins(&labels)) };
        d.place = place_of(&mut places, r);
        d.placed = r.items.first().and_then(|i| doc.items().get(i)).and_then(|i| match &i.kind {
            ecad_model::ItemKind::Symbol(s) if s.placement.origin.is_finite() && s.placement.rotation.is_finite() => Some(Placed {
                x: s.placement.origin.x,
                y: s.placement.origin.y,
                rotation: ecad_model::geometry::units::Degrees::from(s.placement.rotation).0.rem_euclid(360.0),
                mirror: s.placement.mirror,
            }),
            _ => None,
        });
        let scope = ecad_model::registry::Scope::symbol(doc, rep);
        d.sizes = kinds.iter().map(|k| (*k, size_words(&ecad_drawing_style::text::text_style_at(doc, *k, scope)))).collect();
    }
    let (drawn, drawn_count) = crate::references::drawn::rows(doc, c, rep, &dev, &mut places, deps);
    d.drawn = drawn;
    d.drawn_count = drawn_count;
    // Connections: each connection point, its net's wire number and the other points on it.
    let conn = doc.connectivity();
    let mut points: Vec<(ConnectionPointId, String)> =
        doc.connection_points_of(c).take(MAX_ROWS).map(|(t, x)| (t, x.label.clone().unwrap_or_else(|| "?".into()))).collect();
    points.sort_by_key(|p| ecad_electrical::references::devices::table::natural_key(&p.1));
    for (t, pin) in points {
        d.connection_points.insert(t);
        let net = conn.net_of_connection_point(t).and_then(|n| conn.net(n));
        let Some(net) = net else {
            d.connections.push(ConnRow { pin, wire: "not connected".into(), others: String::new() });
            continue;
        };
        deps.extend(net.wires.iter().take(MAX_ROWS).map(|w| ObjectRef::Wire(*w)));
        let wire = match ecad_model::connectivity::wiring::net_number(doc, net) {
            Some(n) => format!("wire {n}"),
            None if net.wires.is_empty() => "not connected".into(),
            None => "wire".into(),
        };
        let mut others = Vec::new();
        let mut more = 0usize;
        for o in net.connection_points.iter().filter(|o| **o != t) {
            let Some(ot) = doc.connection_points().get(o) else { continue };
            let Some(oc) = ot.device() else { continue };
            deps.insert(ObjectRef::Device(oc));
            // Junctions and other untagged helpers are not named.
            let Some(tag) = ecad_electrical::devices::tags::tag_of(doc, oc) else { continue };
            if others.len() < MAX_OTHERS {
                others.push(format!("{tag}:{}", ot.label.as_deref().unwrap_or("?")));
            } else {
                more += 1;
            }
        }
        let mut others = others.join(", ");
        if more > 0 {
            others.push_str(&format!(" +{more}"));
        }
        d.connections.push(ConnRow { pin, wire, others });
    }
    d
}

/// The page a symbol is on (for the header of a page).
pub(crate) fn page_words(doc: &Document, page: PageId) -> (String, String) {
    let number = ecad_model::document::book::page_number(doc, page);
    let title = doc.pages().get(&page).map(|p| p.title.to_string()).unwrap_or_default();
    (number, title)
}
