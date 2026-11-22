//! References, as the inspector's "Where it is drawn" shows them: every symbol that draws a
//! device, and each contact its part offers that is not drawn yet, with the page and zone each
//! sits at. The rows come from `ecad_electrical::references`; this module only words them.

use std::collections::BTreeSet;

use ecad_model::{ConnectionPointId, DeviceId, Document, ObjectRef, RepresentationId, Subject};

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

/// Pins joined as the boards write them: `A1–A2`, `1–2, 3–4`.
pub(crate) fn pins(labels: &[String]) -> String {
    labels.join("\u{2013}")
}

fn label(doc: &Document, t: ConnectionPointId) -> String {
    doc.connection_points().get(&t).and_then(|x| x.label.clone()).unwrap_or_else(|| "?".into())
}

/// The labels a symbol shows: its connection points, in its definition's order.
pub(crate) fn rep_labels(doc: &Document, c: DeviceId, r: &ecad_model::Representation) -> Vec<String> {
    let points = ecad_electrical::references::devices::classify::connection_point_points(doc, c, r);
    if points.is_empty() {
        return r.connection_points.keys().take(8).map(|t| label(doc, *t)).collect();
    }
    points.into_iter().take(8).map(|(t, _)| label(doc, t)).collect()
}

/// The rows of "Where it is drawn" for device `c` with symbol `rep` selected, and how many are drawn;
/// what they read is added to `deps`.
pub fn rows(
    doc: &Document, c: DeviceId, rep: RepresentationId, dev: &ecad_electrical::references::devices::classify::Device,
    places: &mut ecad_electrical::references::devices::places::Places<'_>, deps: &mut BTreeSet<ObjectRef>,
) -> (Vec<DrawnRow>, usize) {
    let mut out: Vec<DrawnRow> = Vec::new();
    let place_of = |places: &mut ecad_electrical::references::devices::places::Places<'_>, r: &ecad_model::Representation| {
        places.text(r.page, ecad_electrical::references::devices::classify::anchor(doc, r))
    };
    // Where it is drawn: the coil (or the symbol) first, then each contact the part offers.
    let reps: Vec<(RepresentationId, &ecad_model::Representation)> =
        doc.representations_of(Subject::Device(c)).take(crate::inspector::details::MAX_ROWS).collect();
    for (_, r) in &reps {
        deps.insert(ObjectRef::Page(r.page));
    }
    let head_first = dev.head.into_iter().chain(reps.iter().map(|(id, _)| *id).filter(|id| Some(*id) != dev.head && !dev.children.contains(id)));
    for id in head_first {
        let Some(r) = doc.representations().get(&id) else { continue };
        let what = if id == rep {
            "this symbol".to_string()
        } else {
            ecad_electrical::devices::tags::rep_definition(doc, r).map_or_else(|| "drawn".to_string(), |(d, _)| d.name.to_lowercase())
        };
        let labels = rep_labels(doc, c, r);
        let first = if dev.head == Some(id) { format!("Coil {}", pins(&labels)) } else { pins(&labels) };
        out.push(DrawnRow { pins: first, what, place: place_of(places, r), rep: Some(id), this: id == rep });
    }
    if dev.head.is_some() {
        let offer = ecad_electrical::references::devices::parse::offer_of(doc, c);
        for row in ecad_electrical::references::devices::table::rows(places, dev, offer.as_ref().map(|(_, o)| o.as_slice()))
            .into_iter()
            .take(crate::inspector::details::MAX_ROWS)
        {
            let kind = if row.power { format!("main, {}", row.kind.upper()) } else { row.kind.upper().to_string() };
            let (what, place) = match row.state {
                ecad_electrical::references::devices::table::State::Drawn => (kind, row.place.clone()),
                ecad_electrical::references::devices::table::State::Spare => (format!("{kind}, not used yet"), String::new()),
                ecad_electrical::references::devices::table::State::Over => (format!("{kind}, beyond the part"), row.place.clone()),
            };
            out.push(DrawnRow { pins: pins(&row.labels), what, place, rep: row.rep, this: row.rep == Some(rep) });
        }
    }
    out.truncate(crate::inspector::details::MAX_ROWS);
    let count = out.iter().filter(|r| !r.place.is_empty()).count();
    (out, count)
}
