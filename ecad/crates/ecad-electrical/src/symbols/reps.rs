//! Representation types and the relations between a device's representations.
//!
//! # Semantics
//! - A device is the master and is never drawn; every representation (symbol) of it is equal.
//! - A representation's **type** ([`PartRole`]) is its stored role's part ([`rep_type`]), set from
//!   its definition when it is placed: never from placement order or ID.
//! - Its **view** ([`View`]) is [`View::Schematic`] for every symbol today. Rev 4's panel layout
//!   adds a variant and its source; [`views`] is ready for it.
//! - Relations are derived within one device, never stored: [`parent`], [`children`],
//!   [`siblings`], [`views`]. Each is O(the device's representations).
//! - Ties go by **drawing order**, the one numbering uses: book position, zone column and row,
//!   then top-left (greatest y, then least x), then ID, which is stable under random IDs
//!   ([`in_book_order`]). The book is built only when the candidates lie on more
//!   than one page.
//!
//! Excludes: locations (`crate::locations`), the reference rule's poles and texts
//! (`crate::references::devices`), and anything stored. This module only reads.

use ecad_model::naming::numbering::{Order, Places};
use ecad_model::{DeviceId, Document, Pt, Representation, RepresentationId, Subject};

use crate::symbols::{Kind, Side};

/// What a representation is for its device: its stored part.
use ecad_model::PartRole;

/// Which kind of drawing a representation belongs to. Only the schematic
/// exists today; rev 4's panel layout adds `Panel` and its source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum View {
    Schematic,
}

impl View {
    /// Every view.
    pub const ALL: [View; 1] = [View::Schematic];

    /// The word a person sees.
    pub fn word(self) -> &'static str {
        match self {
            View::Schematic => "schematic",
        }
    }
}

/// The starter kind a representation shows: its definition's kind (a project symbol's by its
/// own rule). A drawn form has none.
pub fn kind_of(doc: &Document, r: &Representation) -> Option<Kind> {
    crate::devices::tags::rep_definition(doc, r).and_then(|(d, _)| crate::symbols::catalogue::kind_of_definition(d))
}

/// The part representation `r` plays in its device: its stored role's part,
/// set from its definition when it was placed. Anything not a device's
/// symbol is [`PartRole::Other`].
pub fn rep_type(_doc: &Document, r: &Representation) -> PartRole {
    match (r.subject, r.role.part()) {
        (Subject::Device(_), Some(p)) => p,
        _ => PartRole::Other,
    }
}

/// The view of representation `r`: the schematic for every symbol today.
pub fn view(_doc: &Document, _r: &Representation) -> View {
    View::Schematic
}

/// [`PartRole::numbering_rank`] of `r`: the rank hook `ecad_model::naming::numbering::plan_ranked` takes.
pub fn numbering_rank(doc: &Document, r: &Representation) -> u8 {
    rep_type(doc, r).numbering_rank()
}

// ---------------------------------------------------------------- order

/// As [`in_book_order`], reading the book and grids through `places`, shared across a pass (a
/// Checks build) so the book is built at most once.
pub(crate) fn in_book_order_in(places: &mut Places<'_>, reps: Vec<(RepresentationId, &Representation)>) -> Vec<RepresentationId> {
    if reps.len() < 2 {
        return reps.into_iter().map(|(id, _)| id).collect();
    }
    let one_page = reps.iter().all(|(_, r)| reps.first().is_some_and(|(_, f)| f.page == r.page));
    let mut keyed: Vec<(Order, RepresentationId)> = reps
        .into_iter()
        .map(|(id, r)| {
            let o = match places.rep_point(r) {
                Some(at) if one_page => places.order_on_one_page(r.page, at),
                Some(at) => places.order(r.page, at),
                None => Order::unplaced(),
            };
            (o, id)
        })
        .collect();
    keyed.sort();
    keyed.into_iter().map(|(_, id)| id).collect()
}

/// Device `c`'s representations in `view` that pass `keep`, in drawing order.
fn of_device(doc: &Document, c: DeviceId, v: View, keep: impl Fn(&Representation) -> bool) -> Vec<RepresentationId> {
    of_device_in(&mut Places::new(doc), c, v, keep)
}

fn of_device_in(places: &mut Places<'_>, c: DeviceId, v: View, keep: impl Fn(&Representation) -> bool) -> Vec<RepresentationId> {
    let doc = places.doc();
    let reps: Vec<(RepresentationId, &Representation)> =
        doc.representations_of(Subject::Device(c)).filter(|(_, r)| view(doc, r) == v && keep(r)).collect();
    in_book_order_in(places, reps)
}

fn device_of(r: &Representation) -> Option<DeviceId> {
    match r.subject {
        Subject::Device(c) => Some(c),
        Subject::Wire(_) | Subject::Cable(_) | Subject::Route(_) | Subject::Aid(_) => None,
    }
}

// ---------------------------------------------------------------- relations

/// Device `c`'s heads in the schematic, in book order. More than one only after a merge (Checks
/// lists it); the reference rule uses the first.
pub fn heads(doc: &Document, c: DeviceId) -> Vec<RepresentationId> {
    of_device(doc, c, View::Schematic, |r| rep_type(doc, r) == PartRole::Head)
}

/// Device `c`'s head in the schematic: its first head in book order. A coil
/// placed after its contacts is the head as soon as it exists.
pub fn head(doc: &Document, c: DeviceId) -> Option<RepresentationId> {
    heads(doc, c).into_iter().next()
}

/// The parent of `rid`: its device's head in the same view, when `rid` is a contact, pole or
/// channel.
pub fn parent(doc: &Document, rid: RepresentationId) -> Option<RepresentationId> {
    let r = doc.representations().get(&rid)?;
    if !rep_type(doc, r).is_child() {
        return None;
    }
    let v = view(doc, r);
    of_device(doc, device_of(r)?, v, |x| rep_type(doc, x) == PartRole::Head).into_iter().next()
}

/// The children of head `rid`: its device's contacts, poles and channels in the same view, in book
/// order. Empty when `rid` is not its device's head.
pub fn children(doc: &Document, rid: RepresentationId) -> Vec<RepresentationId> {
    let Some(r) = doc.representations().get(&rid) else { return Vec::new() };
    let Some(c) = device_of(r) else { return Vec::new() };
    let v = view(doc, r);
    if rep_type(doc, r) != PartRole::Head || of_device(doc, c, v, |x| rep_type(doc, x) == PartRole::Head).first() != Some(&rid) {
        return Vec::new();
    }
    of_device(doc, c, v, |x| rep_type(doc, x).is_child())
}

/// The siblings of `rid`: its device's other representations of the same type and view, in book
/// order. For a pin, its mates (the same pin name on the other side) come first.
pub fn siblings(doc: &Document, rid: RepresentationId) -> Vec<RepresentationId> {
    let Some(r) = doc.representations().get(&rid) else { return Vec::new() };
    let Some(c) = device_of(r) else { return Vec::new() };
    let t = rep_type(doc, r);
    let v = view(doc, r);
    let mut out: Vec<RepresentationId> = of_device(doc, c, v, |x| rep_type(doc, x) == t);
    out.retain(|x| *x != rid);
    if t == PartRole::Pin {
        let mine = pin_names(doc, r);
        let (mates, rest): (Vec<_>, Vec<_>) = out.into_iter().partition(|x| {
            doc.representations().get(x).is_some_and(|o| pin_names(doc, o).iter().any(|(n, s)| mine.iter().any(|(m, ms)| m == n && ms != s)))
        });
        out = mates.into_iter().chain(rest).collect();
    }
    out
}

/// The (pin name, side key) pairs a pin representation shows.
fn pin_names(doc: &Document, r: &Representation) -> Vec<(String, String)> {
    r.connection_points
        .keys()
        .filter_map(|t| doc.connection_points().get(t))
        .filter_map(|t| Some((t.label.clone()?, t.key.as_ref()?.to_string())))
        .collect()
}

/// The representations of `rid`'s device of the same type in other views: the
/// click-through schematic ↔ panel ↔ one-line. Empty until a second view exists.
pub fn views(doc: &Document, rid: RepresentationId) -> Vec<RepresentationId> {
    let Some(r) = doc.representations().get(&rid) else { return Vec::new() };
    let Some(c) = device_of(r) else { return Vec::new() };
    let (t, v) = (rep_type(doc, r), view(doc, r));
    View::ALL.into_iter().filter(|w| *w != v).flat_map(|w| of_device(doc, c, w, |x| rep_type(doc, x) == t)).collect()
}

// ---------------------------------------------------------------- go-to

/// Where go-to lands for device `c` (display only; it decides nothing). With no side: its head,
/// else its first representation in book order. For a connector half: its whole-half symbol,
/// else its first pin of that side in book order.
pub fn goto(doc: &Document, c: DeviceId, side: Option<Side>) -> Option<RepresentationId> {
    goto_in(&mut Places::new(doc), c, side)
}

/// As [`goto`], reading the book and grids through `places`.
pub fn goto_in(places: &mut Places<'_>, c: DeviceId, side: Option<Side>) -> Option<RepresentationId> {
    let doc = places.doc();
    match side {
        None => of_device_in(places, c, View::Schematic, |r| rep_type(doc, r) == PartRole::Head)
            .into_iter()
            .next()
            .or_else(|| of_device_in(places, c, View::Schematic, |_| true).into_iter().next()),
        Some(side) => {
            let on_side = |r: &Representation| crate::connectors::rep_side(doc, r).is_some_and(|s| s == side || s == Side::Both);
            let whole = of_device_in(places, c, View::Schematic, |r| rep_type(doc, r) == PartRole::Half && on_side(r));
            whole
                .into_iter()
                .next()
                .or_else(|| of_device_in(places, c, View::Schematic, |r| rep_type(doc, r) == PartRole::Pin && on_side(r)).into_iter().next())
        }
    }
}

/// The page and anchor go-to lands on for device `c` ([`goto`]), for Checks rows.
pub fn goto_place(doc: &Document, c: DeviceId) -> Option<(ecad_model::PageId, Option<Pt>)> {
    goto_place_in(&mut Places::new(doc), c)
}

/// As [`goto_place`], reading the book and grids through `places`, shared across a Checks build.
pub(crate) fn goto_place_in(places: &mut Places<'_>, c: DeviceId) -> Option<(ecad_model::PageId, Option<Pt>)> {
    let doc = places.doc();
    let r = doc.representations().get(&goto_in(places, c, None)?)?;
    Some((r.page, crate::references::devices::classify::anchor(doc, r)))
}

/// The whole-half symbol of connector `c`'s `side`, first in book order: the half's head-like
/// anchor. A pin-only half has none.
pub fn half_anchor(doc: &Document, c: DeviceId, side: Side) -> Option<RepresentationId> {
    of_device(doc, c, View::Schematic, |r| rep_type(doc, r) == PartRole::Half && crate::connectors::rep_side(doc, r) == Some(side)).into_iter().next()
}
