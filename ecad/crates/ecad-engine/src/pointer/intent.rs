//! The electrical intent of the running tool at the cursor (the principle:
//! "the click says what it will do, electrically, before you make it"). The intent line shows
//! it beside the prompt. Read only; `None` where nothing is known beyond the prompt.
//!
//! # Semantics
//! - Wire, ending on a connection point: `joins A1 of -K01`, and when a wire already lands
//!   there, `… to <that wire>`; ending on another wire: `joins <wire> (a T)`.
//! - Connect, on a connection point: `connects to A1 of -K01`.
//! - Every other tool: `None` (the line shows its prompt).

use cadcraft_engine::Session;
use cadcraft_engine::geom::Vec2;
use ecad_model::Subject;

use crate::pointer::pick::{capture, pick, pixel, read};
use crate::project::{Mapped, Project};

/// The sentence for tool `command` with the cursor at `cursor` on the active page.
pub fn at_cursor(s: &Session, command: &str, cursor: Vec2) -> Option<String> {
    if !matches!(command, "wire" | "connect") {
        return None;
    }
    let aperture = pixel(s)? * s.settings.aperture.max(1.0);
    let picked = pick(s, cursor);
    let uid = s.state().ok()?.uid;
    read(s, |p, _| Ok(sentence(p, uid, command, cursor, aperture, picked))).ok().flatten()
}

/// [`at_cursor`] on a project already read: `picked` is the drawing object under the pick box.
pub fn sentence(p: &Project, uid: u64, command: &str, cursor: Vec2, aperture: f64, picked: Option<cadcraft_engine::doc::Handle>) -> Option<String> {
    let page = p.page_of(uid)?;
    let doc = p.ledger.document();
    let point = capture(p, page, cursor, aperture);
    match command {
        "wire" => match point {
            Some(c) => {
                let mut s = format!("joins {}", ecad_electrical::terminal_strips::connection_point_phrase(doc, c.connection_point));
                if let Some(w) = ecad_electrical::wires::routes::wire_ending_at(doc, page, c.at, aperture) {
                    s.push_str(&format!(" to {}", ecad_electrical::wires::words::wire_called(doc, w)));
                }
                Some(s)
            }
            None => {
                let h = picked?;
                let Some(Mapped::Item(i)) = p.pages.get(&page)?.map.get(h) else { return None };
                match doc.representation_of_item(i)?.1.subject {
                    Subject::Route(r) => {
                        let words = ecad_electrical::wires::routes::wire_of_route(doc, r)
                            .map_or_else(|| "the wire line".into(), |w| ecad_electrical::wires::words::wire_called(doc, w));
                        Some(format!("joins {words} (a T)"))
                    }
                    _ => None,
                }
            }
        },
        "connect" => point.map(|c| format!("connects to {}", ecad_electrical::terminal_strips::connection_point_phrase(doc, c.connection_point))),
        _ => None,
    }
}
