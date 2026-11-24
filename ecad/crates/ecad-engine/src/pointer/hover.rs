//! What hovering shows: the lines for the object, connection point or cable reference end under
//! the pointer. The words come from ecad-electrical; the engine adds what only the session knows
//! (a cable's view on this page) and caps the lines.

use cadcraft_engine::doc::Handle;
use cadcraft_engine::geom::Vec2;
use ecad_electrical::cables::hover as cable_hover;
use ecad_electrical::devices::hover::with_device_references;
use ecad_electrical::references::cables::MAX_HOVER_LINES;
use ecad_model::{ConnectionPointId, Subject};

use super::pick::{capture, in_range, pt};
use crate::cables::views::{FanOnPage, View, fan_on_page};
use crate::project::{Mapped, Project};

/// What hovering drawing object `h` on page drawing `uid` shows: its name or ID and
/// its connections, or `None` for plain geometry and objects ECAD does not know. Read only.
pub fn hover_lines(p: &Project, uid: u64, h: Handle) -> Option<Vec<String>> {
    let page = p.page_of(uid)?;
    let view = p.pages.get(&page)?;
    let doc = p.ledger.document();
    let mut lines = match view.map.get(h)? {
        // A reference end, by its text, its point or its flag: one hover for all of them.
        Mapped::RefText(r) => ecad_electrical::references::cables::hover_lines(doc, r)?,
        Mapped::Fan(_) if view.map.reference_of(h).is_some() => ecad_electrical::references::cables::hover_lines(doc, view.map.reference_of(h)?)?,
        // The sheet frame and title block: where to change it.
        Mapped::Frame(_) => {
            vec![format!("Page {}: change it in the Properties panel", ecad_electrical::references::cables::page_number(doc, page))]
        }
        // A device's tag text or terminal label: the device's lens.
        Mapped::Tag(r) => match doc.representations().get(&r)?.subject {
            Subject::Device(c) => with_device_references(doc, c, r, crate::devices::draw::hover_lines(doc, c, r)),
            _ => return None,
        },
        // A device cross-reference: a table row's pole, the table's contacts, or the
        // contact's lens with its coil.
        Mapped::DeviceReference(r) => {
            // An arrow's reference text, bracket or caption.
            if let Subject::Aid(a) = doc.representations().get(&r)?.subject {
                ecad_electrical::continuations::arrow(doc, r)?;
                return Some(ecad_electrical::continuations::hover_lines(doc, a, r));
            }
            let Subject::Device(c) = doc.representations().get(&r)?.subject else { return None };
            let row = view.map.device_reference_part(h).and_then(|(_, i)| {
                let dev = ecad_electrical::references::devices::classify::device(doc, c);
                let mut places = ecad_electrical::references::devices::places::Places::new(doc);
                let t = crate::references::draw::devices::table(doc, &mut places, &dev)?;
                t.parts.get(usize::try_from(i).ok()?)?.row
            });
            match row {
                Some(i) => ecad_electrical::references::devices::read::row_lines(doc, c, i, &crate::styles::draw::LOOK)?,
                None if ecad_electrical::references::devices::classify::device(doc, c).head == Some(r) => {
                    ecad_electrical::references::devices::read::head_lines(doc, c, r)
                }
                None => with_device_references(doc, c, r, crate::devices::draw::hover_lines(doc, c, r)),
            }
        }
        // A conductor label: its wire's lens.
        Mapped::Label(w) => {
            ecad_electrical::wires::hover::wire_lines(doc, Some(w), ecad_electrical::wires::routes::route_of_wire_on(doc, w, page), page)
        }
        // A T's dot: the lens of the line that ends there.
        Mapped::Tee(r) => ecad_electrical::wires::hover::wire_lines(doc, ecad_electrical::wires::routes::wire_of_route(doc, r), Some(r), page),
        // A link line.
        Mapped::JumperBar(c, n) => {
            vec![ecad_electrical::terminal_strips::jumper_bars::hover(doc, p.kept_reports().and_then(|o| o.strips.get(c)), c, n)]
        }
        Mapped::Item(i) => match doc.representation_of_item(i)? {
            (rid, r) if ecad_electrical::references::cables::is_reference(r) => ecad_electrical::references::cables::hover_lines(doc, rid)?,
            (rid, r) => match r.subject {
                // A device (a tagged component): its lens.
                Subject::Device(c) if ecad_electrical::devices::tags::tag_of(doc, c).is_some() => {
                    with_device_references(doc, c, rid, crate::devices::draw::hover_lines(doc, c, rid))
                }
                // A continuation arrow's other ends.
                Subject::Device(c) => ecad_electrical::devices::hover::component_lines(doc, c),
                Subject::Wire(w) => ecad_electrical::wires::hover::wire_lines(doc, Some(w), None, page),
                Subject::Route(x) => {
                    ecad_electrical::wires::hover::wire_lines(doc, ecad_electrical::wires::routes::wire_of_route(doc, x), Some(x), page)
                }
                // A continuation arrow's other ends.
                Subject::Aid(a) if ecad_electrical::continuations::arrow(doc, rid).is_some() => {
                    ecad_electrical::continuations::hover_lines(doc, a, rid)
                }
                Subject::Aid(_) => Vec::new(),
                // A drawn slash: traditional, or a fan that cannot be drawn on this page.
                Subject::Cable(k) => {
                    let shown = match p.view_of(k) {
                        View::Traditional => "View: traditional",
                        View::Fan => match fan_on_page(doc, page, k, p.tolerance) {
                            FanOnPage::Failed => "View: fan (cannot be drawn on this page)",
                            FanOnPage::TooFew => "View: fan (needs two wires on this page)",
                            _ => "View: fan",
                        },
                    };
                    cable_hover::with_drawings(doc, k, cable_hover::with_view(cable_hover::cable_lines(doc, page, k), shown))
                }
            },
        },
        // A cable reference's named point is the reference's: its lens.
        Mapped::Marker(rid, _) if doc.representations().get(&rid).is_some_and(ecad_electrical::references::cables::is_reference) => {
            ecad_electrical::references::cables::hover_lines(doc, rid)?
        }
        Mapped::Marker(_, t) => ecad_electrical::devices::hover::connection_point_lines(doc, t),
        Mapped::Fan(k) => {
            let fan = view.map.is_fan(k);
            let mut lines = cable_hover::with_view(cable_hover::cable_lines(doc, page, k), if fan { "View: fan" } else { "View: traditional" });
            if fan {
                let d = ecad_electrical::cables::layout::cable_on_page(
                    doc,
                    page,
                    k,
                    true,
                    p.tolerance,
                    &crate::cables::draw::font_width,
                    &crate::styles::draw::LOOK,
                );
                lines.extend(
                    d.at_reference
                        .iter()
                        .map(|w| format!("{} is attached at the reference end.", ecad_electrical::cables::slash::wire_name(doc, *w))),
                );
            }
            // A tail, bend or label names its core second.
            if let Some(w) = view.map.core_of(h) {
                let core = cable_hover::core_of(doc, k, w).unwrap_or_else(|| "none".into());
                lines.insert(1.min(lines.len()), format!("Core: {core} ({})", ecad_electrical::cables::slash::wire_name(doc, w)));
            }
            cable_hover::with_drawings(doc, k, lines)
        }
    };
    capped(&mut lines);
    Some(lines)
}

/// The terminal shown on page drawing `uid` within `aperture` drawing units of `cursor` (the
/// nearest, as a capture picks it), with what hovering it shows: drawn and symbol terminals
/// alike, connected or free; never a cable reference's named point. A terminal under the pointer wins over the objects around it (the
/// wire ending on it, the symbol it belongs to). Read only.
pub fn hover_connection_point(p: &Project, uid: u64, cursor: Vec2, aperture: f64) -> Option<(ConnectionPointId, Vec<String>)> {
    let page = p.page_of(uid)?;
    if p.pages.get(&page).is_none_or(|v| v.unavailable) {
        return None;
    }
    let c = capture(p, page, cursor, aperture)?;
    // A cable reference's named point is the reference's: its own lens.
    if p.ledger.document().cable_point(c.connection_point).is_some() {
        return None;
    }
    let mut lines = ecad_electrical::devices::hover::connection_point_lines(p.ledger.document(), c.connection_point);
    capped(&mut lines);
    Some((c.connection_point, lines))
}

/// The cable reference end on page drawing `uid` under `cursor` (its point within `radius`
/// drawing units, or its text), with what hovering it shows. It wins over the
/// objects around it: a continued end's middle wire starts exactly on its point. Read only.
pub fn hover_reference(p: &Project, uid: u64, cursor: Vec2, radius: f64) -> Option<(ecad_model::RepresentationId, Vec<String>)> {
    let page = p.page_of(uid)?;
    if p.pages.get(&page).is_none_or(|v| v.unavailable) || !in_range(cursor) {
        return None;
    }
    let doc = p.ledger.document();
    let r = crate::references::draw::cables::reference_at(doc, page, pt(cursor), radius)?;
    let mut lines = ecad_electrical::references::cables::hover_lines(doc, r)?;
    capped(&mut lines);
    Some((r, lines))
}

fn capped(lines: &mut Vec<String>) {
    if lines.len() > MAX_HOVER_LINES {
        let more = lines.len() - (MAX_HOVER_LINES - 1);
        lines.truncate(MAX_HOVER_LINES - 1);
        lines.push(format!("… and {more} more"));
    }
}
