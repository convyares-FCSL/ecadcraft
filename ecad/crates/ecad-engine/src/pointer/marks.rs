//! The marks drawn over a page: connection point rings, wire end dots and squares, cable slash
//! crossings, cable reference rings and the Checks tab's go-to ring. Display only; none prints.

use cadcraft_engine::ext::{Mark, MarkShape};
use ecad_model::RouteEnd;
use ecad_model::connectivity::derive::connection_point_positions;

use super::pick::v2;
use crate::project::Project;

/// Crossing dot of a cable slash on a member wire (`ecad.crossing-size`).
const CROSSING_PX: f32 = 5.0;

/// The marks shown over page drawing `uid`: a ring on every terminal shown, a
/// dot on every attached wire end, a square on every loose one; a magenta dot
/// where a cable slash crosses one of its own members; and a bold ring on
/// every cable reference point, magenta when linked and red for an orphan, with no end mark on a
/// wire end that leaves toward a reference. Display only.
pub fn overlay_marks(p: &Project, uid: u64) -> Vec<Mark> {
    // Colours and sizes from `ecad-tokens` by meaning, on the sheet in use. An orphan
    // reference (error) and an end that looks connected (warning) are painted by the app in
    // shapes these marks lack (`draw::painted_marks`); none of them is an entity, so none prints.
    use ecad_model::palette::marker::{CONNECTION_POINT_PX, LOOSE_END_PX, WIRE_END_ATTACHED_PX};
    let Some(page) = p.page_of(uid) else { return Vec::new() };
    if p.pages.get(&page).is_none_or(|v| v.unavailable) {
        return Vec::new();
    }
    let sheet = crate::cables::draw::sheet();
    let colour = |m: ecad_model::palette::Meaning| crate::cables::draw::rgb(sheet.color(m));
    let doc = p.ledger.document();
    let positions = connection_point_positions(doc, page);
    let ring = colour(ecad_model::palette::Meaning::ConnectionPoint);
    // A cable reference's named points are drawn by its marker, never as rings or dots.
    let mut out: Vec<Mark> = positions
        .iter()
        .filter(|(t, _, _)| doc.named_points(*t).is_none())
        .map(|(_, _, at)| Mark { at: v2(*at), shape: MarkShape::Ring, size: CONNECTION_POINT_PX, stroke: 1.0, color: ring })
        .collect();
    // Rings at the reference ends; loose-end squares are left out where a cable stops
    // at a reference or a wire leaves for another page
    // (the `quiet` ends).
    let markers = ecad_electrical::references::cables::markers_on_page(doc, page, &crate::styles::draw::LOOK);
    let quiet = p.pages.get(&page).map(|v| v.map.quiet_ends()).unwrap_or_default();
    for (r, a, b) in ecad_electrical::wires::routes::route_ends(doc, page) {
        let Some(route) = doc.routes().get(&r) else { continue };
        for (at, landed) in [a, b].into_iter().zip(route.ends) {
            match landed {
                // An end on a connection point shown on this page: a dot on the point.
                RouteEnd::Point(t) => {
                    if let Some((_, _, q)) = positions.iter().find(|(x, _, _)| *x == t && doc.named_points(t).is_none()) {
                        let color = colour(ecad_model::palette::Meaning::WireEndAttached);
                        out.push(Mark { at: v2(*q), shape: MarkShape::Dot, size: WIRE_END_ATTACHED_PX, stroke: 0.0, color });
                    }
                }
                // A T: its dot is drawn with the page (`wires::routes::tees_on_page`).
                RouteEnd::On { .. } => {}
                RouteEnd::Free if quiet.iter().any(|q| q.near(at, p.tolerance)) => {}
                RouteEnd::Free => {
                    let color = colour(ecad_model::palette::Meaning::LooseEnd);
                    out.push(Mark { at: v2(at), shape: MarkShape::Square, size: LOOSE_END_PX, stroke: 1.0, color });
                }
            }
        }
    }
    let cable = colour(ecad_model::palette::Meaning::Cable);
    let map = p.pages.get(&page).map(|v| &v.map);
    let shown = |i| map.is_some_and(|m| m.item(i).is_some());
    out.extend(ecad_electrical::cables::slash::member_crossing_points_where(doc, page, p.tolerance, shown).into_iter().map(|at| Mark {
        at: v2(at),
        shape: MarkShape::Dot,
        size: CROSSING_PX,
        stroke: 0.0,
        color: cable,
    }));
    out.extend(markers.into_iter().filter(|m| !m.orphan).map(|m| Mark {
        at: v2(m.at),
        shape: MarkShape::Ring,
        size: ecad_electrical::references::cables::RING_PX,
        stroke: ecad_electrical::references::cables::BOLD_PX,
        color: cable,
    }));
    // The Checks tab's go-to ring, in the row's severity colour.
    if let Some((_, at, sev)) = p.display.goto_ring.filter(|(pg, _, _)| *pg == page) {
        let color = colour(match sev {
            ecad_electrical::checks::Severity::Error => ecad_model::palette::Meaning::Error,
            ecad_electrical::checks::Severity::Warning => ecad_model::palette::Meaning::Warning,
            ecad_electrical::checks::Severity::Info => ecad_model::palette::Meaning::Ink2,
        });
        out.push(Mark { at: v2(at), shape: MarkShape::Ring, size: 24.0, stroke: 2.0, color });
    }
    out
}
