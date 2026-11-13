//! The electrical intent of the running tool at the cursor (brief 041 step 4, spec principle 4:
//! "the click says what it will do, electrically, before you make it"). The intent line shows
//! it beside the prompt. Read only; `None` where nothing is known beyond the prompt.
//!
//! # Semantics
//! - Wire, ending on a connection point: `joins A1 of -K01`, and when a wire already lands
//!   there, `… to <that wire>`; ending on another wire: `joins <wire> with a junction`.
//! - Connect, on a connection point: `connects to A1 of -K01`.
//! - Every other tool: `None` (the line shows its prompt).

use cadcraft_engine::Session;
use cadcraft_engine::geom::Vec2;
use ecad_model::{Document, PageId, Subject, TerminalId};

use super::{capture, pick, pixel, read, route_ends};
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
                let mut s = format!("joins {}", point_words(doc, c.terminal));
                if let Some(w) = wire_ending_at(doc, page, c.at, aperture) {
                    s.push_str(&format!(" to {}", super::kind::wire_words(doc, w)));
                }
                Some(s)
            }
            None => {
                let h = picked?;
                let Some(Mapped::Item(i)) = p.pages.get(&page)?.map.get(h) else { return None };
                match doc.representation_of_item(i)?.1.subject {
                    Subject::Conductor(w) => Some(format!("joins {} with a junction", super::kind::wire_words(doc, w))),
                    _ => None,
                }
            }
        },
        "connect" => point.map(|c| format!("connects to {}", point_words(doc, c.terminal))),
        _ => None,
    }
}

/// `A1 of -K01` (a device's connection point), or the terminal's name (`-X01:3`).
fn point_words(doc: &Document, t: TerminalId) -> String {
    let name = super::terminal_name(doc, t);
    match doc.terminals.get(&t).map(|x| x.component) {
        Some(c) if crate::strips::is_strip(doc, c) => format!("{}:{name}", crate::devices::tag_words(doc, c)),
        Some(c) => match crate::devices::tag_of(doc, c) {
            Some(tag) => format!("{name} of {tag}"),
            None => name,
        },
        None => name,
    }
}

/// A wire on `page` with an end within `aperture` of `at`.
fn wire_ending_at(doc: &Document, page: PageId, at: ecad_model::Pt, aperture: f64) -> Option<ecad_model::ConductorId> {
    route_ends(doc, page).into_iter().find(|(_, a, b)| a.dist(at) <= aperture || b.dist(at) <= aperture).map(|(w, _, _)| w)
}
