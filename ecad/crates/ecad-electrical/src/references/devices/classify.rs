//! Which objects a representation belongs to and what it is in them (design 025 §3): groups, classes, devices and their drawn poles.

use crate::symbols::{Kind, Starter, StarterKey as K};
use ecad_model::objects::devices::PoleKind;
use ecad_model::{CableId, DeviceId, Document, Form, PageId, Pt, Representation, RepresentationId, Subject};

// ---------------------------------------------------------------- classes (design 025 §3)

/// The objects whose representations may point at each other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Group {
    Cable(CableId),
    Device(DeviceId),
    /// Brief 049 (FP4): the continuation arrows of one name, keyed by the lowest component ID
    /// among them. Its edges are implicit: every member shows every other.
    Continuation(ecad_model::AidId),
}

/// What a member of a group is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Class {
    /// A cable reference end.
    End,
    /// A device's head: its exclusive symbol.
    Head,
    /// A symbol with contact poles.
    Child,
    /// Brief 044 (design 044 §7, design 052 §3.4): a connector half's whole-half symbol. A
    /// pin-only half has none.
    Half(crate::symbols::Side),
    /// Brief 044: any other connector symbol, by the side it draws.
    Pin(crate::symbols::Side),
    /// Brief 049: a continuation arrow's symbol.
    Arrow,
    /// Anything else (no edges).
    Other,
}

/// The poles a starter symbol carries (code, not format: design 025 §2).
pub(crate) fn poles_of(s: &Starter) -> Vec<(PoleKind, bool)> {
    match (s.key, s.kind) {
        (K::Power3, _) => vec![(PoleKind::No, true); 3],
        // Pack re-run, bug 11: the 4-pole set is a contactor's power contacts too.
        (K::Power4, _) => vec![(PoleKind::No, true); 4],
        (_, Kind::Contact { make }) => vec![(if make { PoleKind::No } else { PoleKind::Nc }, false)],
        _ => Vec::new(),
    }
}

/// The class of a component representation by its symbol alone (a second head is made `Other`
/// by [`device`]): a head by its type ([`crate::symbols::reps::rep_type`]), a child when its symbol carries
/// poles.
pub(super) fn raw_class(doc: &Document, r: &Representation) -> Class {
    if crate::symbols::reps::rep_type(doc, r) == ecad_model::PartRole::Head {
        return Class::Head;
    }
    match crate::devices::rep_starter(doc, r) {
        Some(s) if !poles_of(s).is_empty() => Class::Child,
        _ => Class::Other,
    }
}

// ---------------------------------------------------------------- a device's structure

/// One drawn pole.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawnPole {
    pub rep: RepresentationId,
    pub labels: [String; 2],
    pub kind: PoleKind,
    pub power: bool,
    pub page: PageId,
    /// Its symbol's anchor, when it can be located.
    pub at: Option<Pt>,
}

/// A device as the reference rule sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct Device {
    pub component: DeviceId,
    /// Its head ([`crate::symbols::reps::head`]: the first head symbol in book order, then top-left; never
    /// by ID or placement order, design 052 §3.4).
    pub head: Option<RepresentationId>,
    /// Its children, in ID order.
    pub children: Vec<RepresentationId>,
    /// Its drawn poles, children in ID order, each child's poles in its connection point order.
    pub poles: Vec<DrawnPole>,
}

/// The anchor of a representation (design 025 §3.1): a symbol's insertion point, else the
/// middle of its connection points.
pub fn anchor(doc: &Document, r: &Representation) -> Option<Pt> {
    if r.form == Form::Symbol {
        let (_, pl) = crate::devices::rep_definition(doc, r)?;
        return (pl.is_valid() && pl.origin.is_finite()).then_some(pl.origin);
    }
    let pts: Vec<Pt> = r.connection_points.values().copied().filter(|p| p.is_finite()).collect();
    if pts.is_empty() {
        return None;
    }
    let (lo, hi) = bounds(&pts)?;
    Some(Pt::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0))
}

pub(super) fn bounds(pts: &[Pt]) -> Option<(Pt, Pt)> {
    let first = *pts.first()?;
    let (mut lo, mut hi) = (first, first);
    for p in pts {
        lo = Pt::new(lo.x.min(p.x), lo.y.min(p.y));
        hi = Pt::new(hi.x.max(p.x), hi.y.max(p.y));
    }
    (lo.is_finite() && hi.is_finite()).then_some((lo, hi))
}

/// The page positions of a representation's connection points, in its definition's connection point
/// order.
pub fn connection_point_points(doc: &Document, c: DeviceId, r: &Representation) -> Vec<(ecad_model::ConnectionPointId, Pt)> {
    let shown = crate::devices::shown_connection_points(doc, c, r);
    match (r.form, crate::devices::rep_definition(doc, r)) {
        (Form::Symbol, Some((_, pl))) if pl.is_valid() => shown.into_iter().map(|(t, p)| (t, pl.apply(p))).filter(|(_, p)| p.is_finite()).collect(),
        (Form::Symbol, _) => Vec::new(),
        (Form::Drawn, _) => shown,
    }
}

/// Device `c` as the reference rule sees it: O(its representations).
pub fn device(doc: &Document, c: DeviceId) -> Device {
    // Brief 054 (design 054 §3.4): the first head on a page with checks and references on.
    let head = crate::symbols::reps::heads(doc, c)
        .into_iter()
        .find(|h| doc.representations.get(h).is_some_and(|r| ecad_model::document::sheet::page_checks(doc, r.page)));
    let mut children = Vec::new();
    let mut poles = Vec::new();
    for (rid, r) in doc.representations_of(Subject::Device(c)) {
        // Brief 054 (design 054 §3.4): a symbol on a Plain page has no references.
        if !ecad_model::document::sheet::page_checks(doc, r.page) {
            continue;
        }
        if raw_class(doc, r) != Class::Child {
            continue;
        }
        children.push(rid);
        let Some(s) = crate::devices::rep_starter(doc, r) else { continue };
        let kinds = poles_of(s);
        let pts = crate::devices::shown_connection_points(doc, c, r);
        let at = anchor(doc, r);
        for (i, (kind, power)) in kinds.iter().enumerate() {
            let label = |j: usize| pts.get(2 * i + j).map_or_else(|| "?".to_string(), |(t, _)| crate::devices::label_of(doc, *t));
            poles.push(DrawnPole { rep: rid, labels: [label(0), label(1)], kind: *kind, power: *power, page: r.page, at });
        }
    }
    Device { component: c, head, children, poles }
}

/// The class of member `rid` of its group, as the index keeps it.
pub fn class_in(doc: &Document, rid: RepresentationId) -> Option<(Group, Class)> {
    let r = doc.representations.get(&rid)?;
    // Brief 049: a continuation arrow's symbol.
    if let Some(g) = crate::continuations::group_of_rep(doc, r) {
        return Some((g, Class::Arrow));
    }
    // Brief 044: a connector's members.
    if let Subject::Device(c) = r.subject
        && crate::connectors::connector_of(doc, c).is_some_and(|x| x.mating)
    {
        return crate::connectors::ref_classes(doc, c).into_iter().find(|(x, _)| *x == rid).map(|(_, k)| (Group::Device(c), k));
    }
    // Brief 054 (design 054 §3.4): a symbol on a Plain page is in no device's group.
    if matches!(r.subject, Subject::Device(_)) && !ecad_model::document::sheet::page_checks(doc, r.page) {
        return None;
    }
    match r.subject {
        Subject::Cable(k) if crate::references::cables::is_reference(r) => Some((Group::Cable(k), Class::End)),
        Subject::Device(c) => match raw_class(doc, r) {
            Class::Head => Some((Group::Device(c), if crate::symbols::reps::head(doc, c) == Some(rid) { Class::Head } else { Class::Other })),
            Class::Child => Some((Group::Device(c), Class::Child)),
            _ => None,
        },
        _ => None,
    }
}
