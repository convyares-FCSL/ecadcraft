//! What is under the cursor, for the wheel: the object's [`ObjectKind`], its
//! IDs and the name its chip shows. Built from the hover lookups the page already has (the
//! handle map, the terminal capture, the cable reference ends); read only.
//!
//! # Semantics
//! In order: a selection of more than one object that holds the picked one is **mixed**; a cable
//! reference end under the cursor is a **reference**; a wire's end within the aperture is a
//! **wire end**; a terminal strip's terminal within the aperture is a **terminal**; otherwise the
//! picked drawing object decides through the handle map (a device's symbol, a wire, a cable, a
//! tag or label, a cross-reference). Plain geometry is **mixed** (move, copy, turn, delete), free
//! text is **text**, and no object is the **empty page**.

use cadcraft_engine::Session;
use cadcraft_engine::doc::{EntityKind, Handle};
use cadcraft_engine::geom::Vec2;
use ecad_model::{CableId, ConnectionPointId, DeviceId, Document, End, PageId, RepresentationId, RouteId, Subject, WireId};

use crate::interact::{capture, connection_point_name, pick, pixel, pt, read, route_ends};
use crate::project::{Mapped, Project};
use ecad_electrical::devices::kinds::ObjectKind;

/// The object a wheel was opened on.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    pub kind: ObjectKind,
    pub page: Option<PageId>,
    /// The point the wheel opened at (drawing units).
    pub at: [f64; 2],
    /// What the wheel's chip names: `-K01`, `wire 24V-05`, `Page 3`.
    pub name: String,
    pub component: Option<DeviceId>,
    pub conductor: Option<WireId>,
    /// The drawn line (route) clicked, and for a wire end which of its ends.
    pub route: Option<RouteId>,
    pub end: Option<End>,
    pub cable: Option<CableId>,
    pub connection_point: Option<ConnectionPointId>,
    pub representation: Option<RepresentationId>,
    /// The object's drawn handles on the page (what a selection-based action selects).
    pub handles: Vec<Handle>,
    /// A text that is free text (not a tag or label the project generates).
    pub free_text: bool,
    /// A device's symbols on other pages; a cable's references.
    pub others: usize,
    /// `at` is where the person pointed (the wheel's right-click), so a form may take it as its
    /// click; otherwise (a key or Ctrl+K on the selection) the object is given by its handle.
    pub pointed: bool,
    /// A device's representation clicked is a drawing (from Convert
    /// to Device), so its wheel offers Make symbol where a symbol's offers Edit symbol.
    pub drawn: bool,
}

impl Target {
    /// A target of `kind` at `at` with nothing else known.
    pub fn new(kind: ObjectKind, at: [f64; 2], name: impl Into<String>) -> Self {
        Target {
            kind,
            page: None,
            at,
            name: name.into(),
            component: None,
            conductor: None,
            route: None,
            end: None,
            cable: None,
            connection_point: None,
            representation: None,
            handles: Vec::new(),
            free_text: false,
            others: 0,
            pointed: false,
            drawn: false,
        }
    }
}

/// The target under `cursor` on the active page, or why there is none (off an ECAD page, busy).
pub fn under_cursor(s: &Session, cursor: Vec2) -> Result<Target, String> {
    let px = pixel(s).ok_or("the page is not shown")?;
    let aperture = px * s.settings.aperture.max(1.0);
    let picked = pick(s, cursor);
    let free_text = picked.is_some_and(|h| {
        s.doc().ok().and_then(|d| d.entity(h).map(|e| matches!(e.kind, EntityKind::Text(_) | EntityKind::MText(_)))).unwrap_or(false)
    });
    let selection = s.selection();
    let uid = s.state().map_err(|e| e.to_string())?.uid;
    let mut t = read(s, |p, _| Ok(classify(p, uid, cursor, aperture, picked, &selection, free_text)))?;
    t.pointed = true;
    Ok(t)
}

/// Most handles a mixed target carries (a large selection keeps the first).
pub const MAX_HANDLES: usize = 10_000;

/// The target at `cursor` on page drawing `uid` (see the module's semantics). `picked` is the
/// drawing object under the pick box, `free_text` whether it is a text entity. Read only.
pub fn classify(p: &Project, uid: u64, cursor: Vec2, aperture: f64, picked: Option<Handle>, selection: &[Handle], free_text: bool) -> Target {
    let at = [cursor.x, cursor.y];
    let Some(page) = p.page_of(uid) else { return Target::new(ObjectKind::EmptyPage, at, "page") };
    let doc = p.ledger.document();
    let page_name = format!("Page {}", ecad_electrical::references::cables::page_number(doc, page));
    let mut t = Target::new(ObjectKind::EmptyPage, at, page_name);
    t.page = Some(page);
    let Some(view) = p.pages.get(&page) else { return t };
    let map = &view.map;
    if selection.len() > 1 && picked.is_some_and(|h| selection.contains(&h)) {
        t.kind = ObjectKind::Mixed;
        t.name = format!("{} objects", selection.len());
        t.handles = selection.iter().take(MAX_HANDLES).copied().collect();
        return t;
    }
    let c = pt(cursor);
    let ok_aperture = aperture.is_finite() && aperture > 0.0 && crate::interact::in_range(cursor);
    // A cable reference end (its point or text) wins over the wire running into it.
    if ok_aperture && let Some(r) = crate::references::draw::cables::reference_at(doc, page, c, aperture) {
        return reference(doc, map, page, t, r);
    }
    // A wire's end within the aperture.
    if ok_aperture {
        let mut best: Option<(f64, RouteId, End)> = None;
        for (r, a, b) in route_ends(doc, page) {
            for (end, p_) in [(End::A, a), (End::B, b)] {
                let d = p_.dist(c);
                if d <= aperture && best.is_none_or(|(x, _, _)| d < x) {
                    best = Some((d, r, end));
                }
            }
        }
        if let Some((_, r, end)) = best {
            let w = ecad_electrical::wires::routes::wire_of_route(doc, r);
            t.kind = ObjectKind::WireEnd;
            t.conductor = w;
            t.route = Some(r);
            t.end = Some(end);
            t.name = match w {
                Some(w) => format!("end of {}", wire_words(doc, w)),
                None => "end of a wire line".into(),
            };
            t.handles = crate::checks::handles_of(p, page, ecad_electrical::checks::Object::Route(r));
            return t;
        }
    }
    // A terminal strip's terminal within the aperture.
    if ok_aperture
        && let Some(cap) = capture(p, page, cursor, aperture)
        && let Some(strip) = ecad_electrical::terminal_strips::strip_of_connection_point(doc, cap.connection_point)
    {
        return connection_point(p, page, t, strip, Some(cap.connection_point));
    }
    let Some(h) = picked else { return t };
    // A card frame's part stands for the card, through its symbol.
    let h = match map.get(h) {
        Some(Mapped::DeviceReference(r)) if ecad_electrical::plc::draw::is_frame_rep(doc, r) => {
            doc.representations().get(&r).and_then(|x| x.items.first()).and_then(|i| map.item(*i)).unwrap_or(h)
        }
        // A T's dot stands for the line that ends there.
        Some(Mapped::Tee(r)) => ecad_electrical::wires::routes::route_item(doc, r).and_then(|(_, i, _)| map.item(i)).unwrap_or(h),
        _ => h,
    };
    match map.get(h) {
        None if free_text => {
            t.kind = ObjectKind::Text;
            t.free_text = true;
            t.name = "text".into();
            t.handles = vec![h];
        }
        None => {
            t.kind = ObjectKind::Mixed;
            t.name = "drawing".into();
            t.handles = vec![h];
        }
        Some(Mapped::Frame(_) | Mapped::Tee(_)) => {}
        Some(Mapped::RefText(r) | Mapped::DeviceReference(r)) => return reference(doc, map, page, t, r),
        Some(Mapped::Fan(k)) => match map.reference_of(h) {
            Some(r) => return reference(doc, map, page, t, r),
            None => return cable(p, page, t, k),
        },
        Some(Mapped::Tag(r)) => {
            t.kind = ObjectKind::Text;
            t.representation = Some(r);
            t.handles = vec![h];
            if let Some(Subject::Device(c)) = doc.representations().get(&r).map(|x| x.subject) {
                t.component = Some(c);
                t.name = format!("tag of {}", ecad_electrical::devices::tags::tag_words(doc, c));
            } else {
                t.name = "tag".into();
            }
        }
        Some(Mapped::Label(w)) => {
            t.kind = ObjectKind::Text;
            t.conductor = Some(w);
            t.name = format!("label of {}", wire_words(doc, w));
            t.handles = vec![h];
        }
        Some(Mapped::JumperBar(strip, _)) => return connection_point(p, page, t, strip, None),
        Some(Mapped::Marker(rep, term)) => match ecad_electrical::terminal_strips::strip_of_connection_point(doc, term) {
            Some(strip) => return connection_point(p, page, t, strip, Some(term)),
            None => {
                if let Some(Subject::Device(c)) = doc.representations().get(&rep).map(|r| r.subject) {
                    return device(p, page, t, c, Some(rep));
                }
            }
        },
        Some(Mapped::Item(i)) => match doc.representation_of_item(i) {
            Some((rid, r)) if ecad_electrical::references::cables::is_reference(r) => return reference(doc, map, page, t, rid),
            Some((rid, r)) => match r.subject {
                Subject::Device(c) if ecad_electrical::terminal_strips::strip_of(doc, c).is_some() => return connection_point(p, page, t, c, None),
                Subject::Device(c) => return device(p, page, t, c, Some(rid)),
                Subject::Wire(w) => {
                    t.kind = ObjectKind::Wire;
                    t.conductor = Some(w);
                    t.name = wire_words(doc, w);
                    t.handles = crate::checks::handles_of(p, page, ecad_electrical::checks::Object::Wire(w));
                }
                Subject::Cable(k) => return cable(p, page, t, k),
                Subject::Route(r) => {
                    let w = ecad_electrical::wires::routes::wire_of_route(doc, r);
                    t.kind = ObjectKind::Wire;
                    t.conductor = w;
                    t.route = Some(r);
                    t.name = w.map_or_else(|| "wire line".into(), |w| wire_words(doc, w));
                    t.handles = crate::checks::handles_of(p, page, ecad_electrical::checks::Object::Route(r));
                }
                // A drawing aid (a continuation arrow, a location box) is its own
                // kind; its handles are its own.
                Subject::Aid(a) => {
                    t.kind = ObjectKind::Aid;
                    t.representation = Some(rid);
                    t.name = match ecad_electrical::locations::boxes_on(doc, page).into_iter().find(|b| b.aid == a) {
                        Some(b) => format!("location box {}", ecad_electrical::locations::label_of(&b).0),
                        None => format!("continuation {}", ecad_electrical::continuations::display_name(doc, a)),
                    };
                    let map = p.pages.get(&page).map(|v| &v.map);
                    t.handles = r.items.iter().filter_map(|i| map.and_then(|m| m.item(*i))).collect();
                }
            },
            None => {}
        },
    }
    t
}

fn device(p: &Project, page: PageId, mut t: Target, c: DeviceId, rep: Option<RepresentationId>) -> Target {
    let doc = p.ledger.document();
    t.kind = ObjectKind::Device;
    t.component = Some(c);
    t.representation = rep;
    t.name = ecad_electrical::devices::tags::tag_shown_of(doc, c).unwrap_or_else(|| "component".to_string());
    let map = p.pages.get(&page).map(|v| &v.map);
    // The symbol clicked (its outline), so turning or copying acts on that symbol only.
    t.handles = rep
        .and_then(|r| doc.representations().get(&r))
        .map(|r| r.items.iter().filter_map(|i| map.and_then(|m| m.item(*i))).collect())
        .unwrap_or_default();
    t.others = doc.representations_of(Subject::Device(c)).filter(|(_, r)| r.page != page).count();
    t.drawn = rep.and_then(|r| doc.representations().get(&r)).is_some_and(|r| r.form == ecad_model::Form::Drawn);
    t
}

fn connection_point(p: &Project, page: PageId, mut t: Target, strip: DeviceId, term: Option<ConnectionPointId>) -> Target {
    let doc = p.ledger.document();
    t.kind = ObjectKind::TerminalBlock;
    t.component = Some(strip);
    t.connection_point = term;
    let tag = ecad_electrical::devices::tags::tag_words(doc, strip);
    t.name = match term {
        // A connector pin is named by its half (`-X10.S:3`).
        Some(x) => ecad_electrical::connectors::pin_name(doc, x).unwrap_or_else(|| format!("{tag}:{}", connection_point_name(doc, x))),
        None => tag,
    };
    let map = p.pages.get(&page).map(|v| &v.map);
    // A terminal's own symbol (its marker's representation).
    t.handles = term
        .and_then(|x| {
            doc.representations_of(Subject::Device(strip))
                .filter(|(_, r)| r.page == page)
                .find_map(|(rid, r)| map.and_then(|m| m.marker(rid, x)).map(|h| (h, r)))
        })
        .map(|(h, r)| {
            let mut v: Vec<Handle> = r.items.iter().filter_map(|i| map.and_then(|m| m.item(*i))).collect();
            v.push(h);
            v
        })
        .unwrap_or_default();
    t
}

fn cable(p: &Project, page: PageId, mut t: Target, k: CableId) -> Target {
    let doc = p.ledger.document();
    t.kind = ObjectKind::Cable;
    t.cable = Some(k);
    t.name = ecad_electrical::devices::tags::cable_tag_shown_of(doc, k).unwrap_or_else(|| "cable".to_string());
    t.handles = crate::checks::handles_of(p, page, ecad_electrical::checks::Object::Cable(k));
    t.others = doc.representations_of(Subject::Cable(k)).filter(|(_, r)| ecad_electrical::references::cables::is_reference(r)).count();
    t
}

fn reference(doc: &Document, map: &crate::project::HandleMap, page: PageId, mut t: Target, r: RepresentationId) -> Target {
    t.kind = ObjectKind::Reference;
    t.representation = Some(r);
    if let Some(rep) = doc.representations().get(&r) {
        match rep.subject {
            Subject::Cable(k) => t.cable = Some(k),
            Subject::Device(c) => t.component = Some(c),
            Subject::Wire(_) | Subject::Route(_) | Subject::Aid(_) => {}
        }
        if rep.page == page {
            t.handles = rep.items.iter().filter_map(|i| map.item(*i)).collect();
        }
    }
    t.name = ecad_electrical::references::cables::hover_lines(doc, r).and_then(|l| l.into_iter().next()).unwrap_or_else(|| "reference".into());
    t
}

/// A wire as a person names it: `wire 01` (its net's number), else its tag or short ID.
pub fn wire_words(doc: &Document, w: WireId) -> String {
    match ecad_model::connectivity::wiring::wire_number(doc, w) {
        Some(n) => format!("wire {n}"),
        None => ecad_electrical::cables::slash::wire_name(doc, w),
    }
}
