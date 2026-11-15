//! UI rev 3 (boards 1.05 and 1.06): what the inspector reads about one selected device to lay
//! its Device and Layout tabs out: the header's words, the rows of "Where it is drawn", the
//! connections, the symbol's placement and the size each text slot takes. A plain copy built
//! with the inspector's other rows (the same update map); nothing here writes.

use std::collections::BTreeSet;

use ecad_cadcraft::text_style::TextKind;
use ecad_model::{ComponentId, Document, ObjectRef, PageId, RepresentationId, Subject, TerminalId};

/// At most this many rows in "Where it is drawn" and "Connections" (hostile files can hold
/// thousands; the full lists are in Project).
pub const MAX_ROWS: usize = 64;
/// At most this many names of other connection points on one connection row.
const MAX_OTHERS: usize = 3;

/// One row of "Where it is drawn": a symbol of the device, or a contact its part offers that is
/// not drawn yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrawnRow {
    /// `A1–A2`, `13–14`.
    pub pins: String,
    /// `this symbol`, `NO`, `NC, not used yet`.
    pub what: String,
    /// `2/D4`; empty when it is not drawn.
    pub place: String,
    /// The symbol that draws it (a go-to).
    pub rep: Option<RepresentationId>,
    /// It is the selected symbol.
    pub this: bool,
}

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
    pub component: Option<ComponentId>,
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
    pub drawn: Vec<DrawnRow>,
    /// How many rows of [`DeviceDetails::drawn`] are drawn.
    pub drawn_count: usize,
    pub connections: Vec<ConnRow>,
    pub placed: Option<Placed>,
    /// The device's connection points (its own checks are about these or the device).
    pub terminals: BTreeSet<TerminalId>,
    /// The size each text slot takes (`2.5 B`), by kind.
    pub sizes: Vec<(TextKind, String)>,
}

/// Pins joined as the boards write them: `A1–A2`, `1–2, 3–4`.
fn pins(labels: &[String]) -> String {
    labels.join("\u{2013}")
}

fn label(doc: &Document, t: TerminalId) -> String {
    doc.terminals.get(&t).and_then(|x| x.label.clone()).unwrap_or_else(|| "?".into())
}

/// The labels a symbol shows: its connection points, in its definition's order.
fn rep_labels(doc: &Document, c: ComponentId, r: &ecad_model::Representation) -> Vec<String> {
    let points = ecad_cadcraft::xref::terminal_points(doc, c, r);
    if points.is_empty() {
        return r.connection_points.keys().take(8).map(|t| label(doc, *t)).collect();
    }
    points.into_iter().take(8).map(|(t, _)| label(doc, t)).collect()
}

/// A size as the Layout tab shows it: `2.5` and ` B` when bold.
pub fn size_words(s: &ecad_cadcraft::text_style::TextStyle) -> String {
    let h = (s.height * 100.0).round() / 100.0;
    let bold = matches!(s.weight, ecad_cadcraft::text_style::Weight::Bold);
    format!("{h}{}", if bold { " B" } else { "" })
}

/// Build the details of device `c` with symbol `rep` selected, adding what they read to `deps`.
pub fn build(doc: &Document, c: ComponentId, rep: RepresentationId, kinds: &[TextKind], deps: &mut BTreeSet<ObjectRef>) -> DeviceDetails {
    use ecad_cadcraft::xref;
    let mut d = DeviceDetails { component: Some(c), rep: Some(rep), ..DeviceDetails::default() };
    let props = doc.components.get(&c).map(|x| &x.properties);
    d.tag = ecad_cadcraft::devices::tag_words(doc, c);
    d.description = props.and_then(|p| p.get("description")).cloned().unwrap_or_default();
    d.standard = doc.standard.as_ref().map_or_else(|| "IEC".to_string(), |s| s.header.name.clone());
    let dev = xref::device(doc, c);
    let mut places = xref::Places::new(doc);
    let place_of = |places: &mut xref::Places<'_>, r: &ecad_model::Representation| places.text(r.page, xref::anchor(doc, r));
    if let Some(r) = doc.representations.get(&rep) {
        deps.insert(ObjectRef::Page(r.page));
        d.symbol_name = ecad_cadcraft::devices::rep_starter(doc, r).map_or_else(|| "Drawn symbol".to_string(), |s| s.name.to_string());
        let role = if dev.head == Some(rep) {
            "coil".to_string()
        } else if dev.children.contains(&rep) {
            "contact".to_string()
        } else {
            d.symbol_name.to_lowercase()
        };
        let labels = rep_labels(doc, c, r);
        d.symbol_words = if labels.is_empty() { role } else { format!("{role} {}", pins(&labels)) };
        d.place = place_of(&mut places, r);
        d.placed = r.items.first().and_then(|i| doc.items.get(i)).and_then(|i| match &i.kind {
            ecad_model::ItemKind::Symbol(s) if s.placement.origin.is_finite() && s.placement.rotation.is_finite() => Some(Placed {
                x: s.placement.origin.x,
                y: s.placement.origin.y,
                rotation: s.placement.rotation.rem_euclid(360.0),
                mirror: s.placement.mirror,
            }),
            _ => None,
        });
        let scope = ecad_cadcraft::defaults::Scope::symbol(doc, rep);
        d.sizes = kinds.iter().map(|k| (*k, size_words(&ecad_cadcraft::text_style::text_style_at(doc, *k, scope)))).collect();
    }
    // Where it is drawn: the coil (or the symbol) first, then each contact the part offers.
    let reps: Vec<(RepresentationId, &ecad_model::Representation)> = doc.representations_of(Subject::Component(c)).take(MAX_ROWS).collect();
    for (_, r) in &reps {
        deps.insert(ObjectRef::Page(r.page));
    }
    let head_first = dev.head.into_iter().chain(reps.iter().map(|(id, _)| *id).filter(|id| Some(*id) != dev.head && !dev.children.contains(id)));
    for id in head_first {
        let Some(r) = doc.representations.get(&id) else { continue };
        let what = if id == rep {
            "this symbol".to_string()
        } else {
            ecad_cadcraft::devices::rep_starter(doc, r).map_or_else(|| "drawn".to_string(), |s| s.name.to_lowercase())
        };
        let labels = rep_labels(doc, c, r);
        let first = if dev.head == Some(id) { format!("Coil {}", pins(&labels)) } else { pins(&labels) };
        d.drawn.push(DrawnRow { pins: first, what, place: place_of(&mut places, r), rep: Some(id), this: id == rep });
    }
    if dev.head.is_some() {
        let offer = xref::offer_of(doc, c);
        for row in xref::rows(&mut places, &dev, offer.as_ref().map(|(_, o)| o.as_slice())).into_iter().take(MAX_ROWS) {
            let kind = if row.power { format!("main, {}", row.kind.word()) } else { row.kind.word().to_string() };
            let (what, place) = match row.state {
                xref::State::Drawn => (kind, row.place.clone()),
                xref::State::Spare => (format!("{kind}, not used yet"), String::new()),
                xref::State::Over => (format!("{kind}, beyond the part"), row.place.clone()),
            };
            d.drawn.push(DrawnRow { pins: pins(&row.labels), what, place, rep: row.rep, this: row.rep == Some(rep) });
        }
    }
    d.drawn.truncate(MAX_ROWS);
    d.drawn_count = d.drawn.iter().filter(|r| !r.place.is_empty()).count();
    // Connections: each connection point, its net's wire number and the other points on it.
    let conn = doc.connectivity();
    let mut points: Vec<(TerminalId, String)> =
        doc.terminals_of(c).take(MAX_ROWS).map(|(t, x)| (t, x.label.clone().unwrap_or_else(|| "?".into()))).collect();
    points.sort_by_key(|p| xref::natural_key(&p.1));
    for (t, pin) in points {
        d.terminals.insert(t);
        let net = conn.net_of_terminal(t).and_then(|n| conn.net(n));
        let Some(net) = net else {
            d.connections.push(ConnRow { pin, wire: "not connected".into(), others: String::new() });
            continue;
        };
        deps.extend(net.conductors.iter().take(MAX_ROWS).map(|w| ObjectRef::Conductor(*w)));
        let wire = match ecad_model::wiring::net_number(doc, net) {
            Some(n) => format!("wire {n}"),
            None if net.conductors.is_empty() => "not connected".into(),
            None => "wire".into(),
        };
        let mut others = Vec::new();
        let mut more = 0usize;
        for o in net.terminals.iter().filter(|o| **o != t) {
            let Some(ot) = doc.terminals.get(o) else { continue };
            deps.insert(ObjectRef::Component(ot.component));
            // Junctions and other untagged helpers are not named.
            let Some(tag) = ecad_cadcraft::devices::tag_of(doc, ot.component) else { continue };
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
pub fn page_words(doc: &Document, page: PageId) -> (String, String) {
    let number = ecad_model::book::page_number(doc, page);
    let title = doc.pages.get(&page).map(|p| p.title.clone()).unwrap_or_default();
    (number, title)
}
