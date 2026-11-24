//! What hovering a wire shows: its lens lines.

use ecad_model::{Document, End, PageId, RouteEnd, RouteId, WireId};

use crate::terminal_strips::terminal_blocks::{connection_point_name, short};

/// A wire's lens: wire `w` (the one a clicked line stands for, when it has one) and the open ends
/// of route `route` (the line clicked, or the wire's line on `page`).
pub fn wire_lines(doc: &Document, w: Option<WireId>, route: Option<RouteId>, page: PageId) -> Vec<String> {
    let conductor = w.and_then(|w| doc.wires().get(&w));
    let mut lines = match w {
        Some(w) => vec![format!("Wire {}", conductor.and_then(|x| x.number_text()).unwrap_or("(no number)")), format!("id {}", w.hex())],
        None => vec!["Wire line (no connection points yet)".to_string()],
    };
    if let Some((w, k)) = w.and_then(|w| doc.cables_of(w).first().map(|k| (w, *k))) {
        let name = match doc.cables().get(&k).and_then(|x| x.tag_shown()) {
            Some(t) => t,
            None => format!("{}\u{2026}", short(&k.hex())),
        };
        match crate::cables::hover::core_of(doc, k, w) {
            Some(c) => lines.push(format!("In cable {name}, core {c}")),
            None => lines.push(format!("In cable {name}")),
        }
    }
    if let Some(c) = conductor {
        for end in End::BOTH {
            let t = c.end(end);
            lines.push(match doc.connection_points().get(&t) {
                // A continuation is named by what it is.
                Some(x) if x.owner.aid().is_some() => {
                    let words = super::words::point_words(doc, t);
                    match words.strip_prefix('\u{2192}') {
                        Some(name) => format!("End {end:?}: continuation {name}"),
                        None => format!("End {end:?}: {words}"),
                    }
                }
                Some(x) => {
                    let of = x
                        .device()
                        .and_then(|c| doc.devices().get(&c))
                        .and_then(|c| c.tag_shown())
                        .unwrap_or_else(|| short(&x.owner.subject_hex()).to_string());
                    format!("End {end:?}: connection point {} of {of}", connection_point_name(doc, t))
                }
                None => format!("End {end:?}: not connected"),
            });
        }
    }
    // An open end of the line lying on another line says so, and how to join them.
    if let Some(r) = route
        && let Some(line) = doc.routes().get(&r)
    {
        for ((at, landed), end) in line.end_points().into_iter().flatten().zip(line.ends).zip(End::BOTH) {
            match landed {
                // A T is joined, not open: it names the line it lands on.
                RouteEnd::On { route: on, .. } => {
                    let joined = match super::routes::wire_of_route(doc, on) {
                        Some(w) => format!("wire {}", ecad_model::connectivity::wiring::wire_number(doc, w).unwrap_or_else(|| "(no number)".into())),
                        None => "a line with no wire yet".into(),
                    };
                    lines.push(format!("End {end:?} joins {joined} (a T)."));
                }
                RouteEnd::Free => {
                    if let Some(on) = super::placing::route_at(doc, page, at, super::ON_WIRE, Some(r)) {
                        let number = super::routes::wire_of_route(doc, on.route)
                            .and_then(|w| ecad_model::connectivity::wiring::wire_number(doc, w))
                            .unwrap_or_else(|| "?".into());
                        lines.push(format!("Open end: looks connected to {number}; Join Wires joins them."));
                    }
                }
                RouteEnd::Point(_) => {}
            }
        }
    }
    let Some(w) = w else { return lines };
    // A wire continued on other pages says where.
    let others: Vec<String> = crate::references::cables::route_pages(doc, w)
        .into_iter()
        .filter(|p| *p != page)
        .map(|p| crate::references::cables::page_number_words(doc, p))
        .collect();
    if !others.is_empty() {
        lines.push(format!("Also drawn on {}", others.join(", ")));
    }
    // The wire's number, potential, label and net (the Properties panel shows the same facts).
    lines.extend(super::words::hover_lines(doc, w));
    lines
}
