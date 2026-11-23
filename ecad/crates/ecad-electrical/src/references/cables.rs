//! Cable references across pages: where the drawing of a cable stops on
//! one page, a reference end shows where it continues, live.
//!
//! # Semantics
//! - A **reference end** is an ordinary drawn representation of the cable with role
//!   [`ecad_model::Role::Reference`] and one Point item. Nothing else is stored for it: no
//!   pairing, no text. A cable has at most two references in this version (one continuation).
//! - Everything a reference **says** (the other end's page number and column, its partner, whether
//!   it is an orphan) is derived on every draw by [`ecad_model::connectivity::derive::cable_references`] and never
//!   stored. Renaming, reordering or moving pages changes it with no transaction of its own.
//! - [`markers_on_page`] is **the one function** that says what a reference end shows on a page:
//!   its point, its derived text (`/<page>.<column>` of the other end, `/?` for an orphan), its
//!   colour state and where the text sits. The projection draws its minimal marker from it, the
//!   overlay rings it, the hover and `cablereferences` read the same derivation; the styled
//!   marker calls it too, so both briefs always show the same text.
//! - [`plan_continue`] decides what `cablecontinue` draws: its JSON form and its
//!   interactive preview both use it, so what is previewed is what is committed.
//!
//! This module has no UI and records nothing: the commands that use it commit through their JSON
//! forms (`ecad_engine::commands`).

use std::collections::{BTreeMap, BTreeSet};

use crate::CmdError;
use ecad_model::connectivity::derive::{CableReference, cable_references, cable_references_of};
use ecad_model::{
    CableId, ConnectionPointId, CorePosition, Document, End, ItemId, LengthUnit, Op, PageId, Pt, Representation, RepresentationId, RouteEnd, RouteId,
    Subject, WireId,
};
use serde_json::{Value, json};

pub use ecad_model::connectivity::derive::MAX_REFERENCES_PER_CABLE;

use crate::styles::text_style::fonts::TextMetrics;

/// `ecad.reference`: ACI 6, magenta (the colour of `ECAD-CABLES`, so a linked text is ByLayer).
pub const REFERENCE_ACI: u8 = 6;
/// `ecad.orphan`: ACI 1, red.
pub const ORPHAN_ACI: u8 = 1;
/// `ecad.ref-stub-spacing` in millimetres: between continued stubs, and from a reference point to
/// its text.
pub(crate) const STUB_SPACING_MM: f64 = 5.0;
/// `ecad.ref-size`: the overlay ring on a reference point, in screen pixels (stroke `ecad.bold`).
pub const RING_PX: f32 = 12.0;
/// `ecad.bold`, in screen pixels.
pub const BOLD_PX: f32 = 2.0;

/// Refusal of an edit of the derived text.
pub const TEXT_DERIVED: &str = "The reference text is derived; move the reference point instead.";

/// Drawing-unit sizes of reference drawing, in the project's length unit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sizes {
    pub text_height: f64,
    pub spacing: f64,
}

/// The sizes for `doc`'s length unit (millimetre tokens divided by 25.4 in an inch project).
pub fn sizes(doc: &Document) -> Sizes {
    let k = match doc.settings().length_unit {
        LengthUnit::Millimetre => 1.0,
        LengthUnit::Inch => 1.0 / 25.4,
    };
    // Reference text is the `Reference` text style, already in drawing units.
    Sizes {
        text_height: crate::styles::text_style::text_style(doc, crate::styles::text_style::TextKind::Reference).height,
        spacing: STUB_SPACING_MM * k,
    }
}

/// Whether `r` is a cable reference end.
pub fn is_reference(r: &Representation) -> bool {
    r.role == ecad_model::Role::Reference && matches!(r.subject, Subject::Cable(_))
}

/// The reference representations of `cable`, in ID order.
pub(crate) fn references_of(doc: &Document, cable: CableId) -> Vec<(RepresentationId, &Representation)> {
    doc.representations_of(Subject::Cable(cable)).filter(|(_, r)| r.role == ecad_model::Role::Reference).collect()
}

/// Every reference end of the document, derived (page order, then column, then ID).
pub fn all(doc: &Document) -> Vec<CableReference> {
    cable_references(doc)
}

/// The reference `reference` as derived, from its own cable's references only (all a reference's
/// text depends on): the cost is that cable's, not the document's. `None` when it is no cable
/// reference.
pub fn reference_of(doc: &Document, reference: RepresentationId) -> Option<CableReference> {
    let Some(Subject::Cable(cable)) = doc.representations().get(&reference).map(|r| r.subject) else { return None };
    cable_references_of(doc, &BTreeSet::from([cable])).into_iter().find(|r| r.reference == reference)
}

/// The reference ends on `page`, derived.
pub fn references_on_page(doc: &Document, page: PageId) -> Vec<CableReference> {
    page_cable_references(doc, page).into_iter().filter(|r| r.page == page).collect()
}

/// The references of the cables with a reference end on `page`, derived as [`all`] derives them
/// (a reference's text reads only its own cable's references and routes;
/// [`cable_references_of`]). Cost is in proportion to the page's representations and those
/// cables, not to the document (`wire` and `joinwires` derived every reference of the
/// project to find a page's markers).
fn page_cable_references(doc: &Document, page: PageId) -> Vec<CableReference> {
    let cables: BTreeSet<CableId> = doc
        .representations_on(page)
        .filter(|(_, r)| is_reference(r))
        .filter_map(|(_, r)| match r.subject {
            Subject::Cable(k) => Some(k),
            _ => None,
        })
        .collect();
    cable_references_of(doc, &cables)
}

/// What one reference end shows on its page. Call [`markers_on_page`].
#[derive(Debug, Clone, PartialEq)]
pub struct Marker {
    /// The reference representation.
    pub reference: RepresentationId,
    pub cable: CableId,
    /// Its Point item.
    pub item: Option<ItemId>,
    /// The reference point.
    pub at: Pt,
    /// The derived text: `/<page>.<column>` of the other end, `/?` for an orphan (or an end that
    /// cannot be located).
    pub text: String,
    /// Red (`ecad.orphan`) rather than magenta (`ecad.reference`).
    pub orphan: bool,
    /// The other end, when there is one.
    pub partner: Option<RepresentationId>,
    /// Where the text is anchored (middle of its height): `at` moved by one stub spacing away
    /// from the wires.
    pub text_at: Pt,
    /// The text reads leftwards from `text_at` (right-aligned): the wires come from the right.
    pub right_aligned: bool,
    /// The unit direction away from the wires (u): the side the flag
    /// takes.
    pub text_dir: Pt,
    /// Text height in drawing units.
    pub height: f64,
}

/// What every locatable reference end on `page` shows. **The** function both the minimal marker of
/// this and the styled marker take their text from: one derivation, always live.
/// Ends that cannot be located (a missing or non-finite point, only from a hostile file) are left
/// out; `cablereferences` and the hover still list them with `/?`.
pub fn markers_on_page(doc: &Document, page: PageId) -> Vec<Marker> {
    markers_from(doc, page_cable_references(doc, page), page)
}

/// [`markers_on_page`] from references already derived (`references`, as [`all`] returns them), so one
/// draw derives them once.
pub fn markers_from(doc: &Document, references: Vec<CableReference>, page: PageId) -> Vec<Marker> {
    let z = sizes(doc);
    references
        .into_iter()
        .filter(|r| r.page == page)
        .filter_map(|r| {
            let at = r.at?;
            let text_at = Pt::new(at.x + r.text_dir.x * z.spacing, at.y + r.text_dir.y * z.spacing);
            text_at.is_finite().then(|| Marker {
                reference: r.reference,
                cable: r.cable,
                item: r.item,
                at,
                partner: r.partner(),
                orphan: r.orphan,
                text: r.text,
                text_at,
                right_aligned: r.text_dir.x < 0.0,
                text_dir: r.text_dir,
                height: z.text_height,
            })
        })
        .collect()
}

/// The derived text one reference end shows, with whether it is an orphan; `None` when
/// `reference` is no cable reference.
pub fn marker_text(doc: &Document, reference: RepresentationId) -> Option<(String, bool)> {
    marker_text_dir(doc, reference).map(|(text, orphan, _)| (text, orphan))
}

/// [`marker_text`] with the direction away from the wires (u), which the
/// flag points along.
///
/// Derives only the references of that end's cable (all a reference's text depends on), so a page
/// with reference ends costs in proportion to their cables, not to the document.
pub(crate) fn marker_text_dir(doc: &Document, reference: RepresentationId) -> Option<(String, bool, Pt)> {
    let Some(Subject::Cable(cable)) = doc.representations().get(&reference).map(|r| r.subject) else { return None };
    cable_references_of(doc, &BTreeSet::from([cable])).into_iter().find(|r| r.reference == reference).map(|r| (r.text, r.orphan, r.text_dir))
}

// The marker of a reference end (flag and text) is drawn by `draw.rs` only, with the
// text from [`marker_text`]; its text is mapped `Mapped::RefText`, so hover, Go to Reference and
// erasing it act on the reference. The ends of member routes that leave for another page (design
// show no loose-end square: `draw.rs` folds that rule into its `quiet` ends.

/// The reference end on `page` under `p`: the nearest reference point within `radius`, else a
/// reference whose drawn text (its box, grown by `radius`) holds `p`. Used before the generic pick
/// by Go to Reference and the hover, because a continued end's middle stub
/// starts exactly on its reference point and would otherwise win the pick.
///
/// `m` measures the flag's text (the C wrapper `reference_at` passes CADCraft's fonts).
pub fn reference_at_measured(doc: &Document, page: PageId, p: Pt, radius: f64, m: &dyn TextMetrics) -> Option<RepresentationId> {
    if !(p.is_finite() && radius.is_finite() && radius >= 0.0) {
        return None;
    }
    let markers = markers_on_page(doc, page);
    let near =
        markers.iter().map(|m| (m.at.dist(p), m.reference)).filter(|(d, _)| *d <= radius).min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    if let Some((_, r)) = near {
        return Some(r);
    }
    let h = crate::cables::words::label_height(doc);
    markers.iter().find(|x| text_box_holds(x, p, radius, h, m)).map(|x| x.reference)
}

/// Whether `p` lies in a marker's box (the flag) grown by `pad`.
fn text_box_holds(m: &Marker, p: Pt, pad: f64, h: f64, metrics: &dyn TextMetrics) -> bool {
    // Draws the marker: its flag around the text, at the reference point, on the side
    // away from the wires.
    let w = crate::cables::words::stroke_label_width(&crate::cables::words::shorten(&m.text, 64), h, metrics);
    let (lo, hi) = crate::cables::words::flag_box(m.at, w, crate::cables::words::FlagSide::away(m.text_dir), h);
    p.x >= lo.x - pad && p.x <= hi.x + pad && p.y >= lo.y - pad && p.y <= hi.y + pad
}

/// The pages (in page order) where `conductor` has a route ([`crate::wires::routes::wire_pages`]).
pub fn route_pages(doc: &Document, conductor: WireId) -> Vec<PageId> {
    let on = crate::wires::routes::wire_pages(doc, conductor);
    doc.page_order().iter().copied().filter(|p| on.contains(p)).collect()
}

/// The number a page is shown with: its `number` property, else its position in book
/// order (`1` first), else `?`. Every place that shows a page's number (the Project panel, the
/// page window's title, reference texts' words) asks here.
pub fn page_number(doc: &Document, page: PageId) -> String {
    ecad_model::document::book::page_number(doc, page)
}

/// `page 2 "Field"` (or `page ? "Field"` for a page in no order).
pub fn page_words(doc: &Document, page: PageId) -> String {
    let n = page_number(doc, page);
    let title = doc.pages().get(&page).map(|p| p.title.as_str()).unwrap_or_default();
    format!("page {n} \"{title}\"")
}

/// `page 2` (or `page ?`).
pub fn page_number_words(doc: &Document, page: PageId) -> String {
    format!("page {}", page_number(doc, page))
}

/// `Cable C1` in messages: its tag, else `8a3f19c0…`.
pub(crate) fn cable_words(doc: &Document, cable: CableId) -> String {
    format!("Cable {}", crate::cables::cable_title(doc, cable))
}

// ---------------------------------------------------------------- refusals

/// R1.
pub(crate) fn already_here(doc: &Document, cable: CableId) -> String {
    format!("{} already has a reference on this page.", cable_words(doc, cable))
}
/// R2.
pub(crate) fn already_continues(doc: &Document, cable: CableId) -> String {
    format!("{} already continues to another page; a cable can have only two references in this version.", cable_words(doc, cable))
}
/// R3.
pub(crate) fn no_wires_here(doc: &Document, cable: CableId) -> String {
    format!("{} has no wires drawn on this page; draw or continue it here first.", cable_words(doc, cable))
}
/// R4.
pub(crate) fn no_reference(doc: &Document, cable: CableId) -> String {
    format!("{} has no reference to continue from; place one with Cable Reference on its page first.", cable_words(doc, cable))
}
/// R5.
pub(crate) fn drawn_here(doc: &Document, cable: CableId) -> String {
    format!("{} is already drawn on this page; continue it on another page.", cable_words(doc, cable))
}
/// R6.
pub(crate) fn none_can_continue(doc: &Document, cable: CableId) -> String {
    format!(
        "None of cable {}'s wires can continue: each is connected at both ends or already drawn on this page.",
        crate::cables::cable_title(doc, cable)
    )
}
/// R9.
pub(crate) fn too_many_members() -> String {
    format!("`members` lists more than {} wires", crate::cables::slash::MAX_CABLE_MEMBERS)
}
/// `toward` too close to `at`.
pub(crate) const TOWARD_EQUAL: &str = "`toward` must differ from `at`";

/// The checks `cablereference` makes before placing a reference for `cable` on `page` (R1 to R3).
///
/// # Errors
/// `no such cable`, or the refusal's words.
pub fn check_reference(doc: &Document, page: PageId, cable: CableId) -> Result<(), CmdError> {
    let c = doc.cables().get(&cable).ok_or(CmdError::refused(crate::msg::NO_SUCH_CABLE))?;
    let references = references_of(doc, cable);
    if references.iter().any(|(_, r)| r.page == page) {
        return Err(CmdError::refused(already_here(doc, cable)));
    }
    if references.len() >= MAX_REFERENCES_PER_CABLE {
        return Err(CmdError::refused(already_continues(doc, cable)));
    }
    if !c.cores.iter().any(|m| crate::cables::member_pages(doc, m).contains(&page)) {
        return Err(CmdError::refused(no_wires_here(doc, cable)));
    }
    Ok(())
}

// ---------------------------------------------------------------- continuation

/// Why a member does not continue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skip {
    BothConnected,
    DrawnHere,
}

impl Skip {
    /// The JSON `reason`.
    pub fn reason(self) -> &'static str {
        match self {
            Skip::BothConnected => "connected at both ends",
            Skip::DrawnHere => "already drawn on this page",
        }
    }
}

/// One continued core: the source page's loose line it sits on, that line's free end (which lands
/// on the core's named point there), its stub here and which of the stub's ends is free (the outer
/// one; the other lands on the core's named point here).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stub {
    /// The core's index in the cable.
    pub core: usize,
    /// The source page's line, and the index (0 or 1) and position of its free end.
    pub line: RouteId,
    pub line_end: usize,
    pub line_at: Pt,
    /// The stub's first vertex (End A) and last (End B).
    pub a: Pt,
    pub b: Pt,
    pub free_end: End,
}

/// What `cablecontinue` draws.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// The cable's existing reference (on the source page), which this end will name.
    pub source: RepresentationId,
    pub source_page: PageId,
    pub stubs: Vec<Stub>,
    /// The cores that do not continue: the wire (or line) each is on, and why.
    pub skipped: Vec<(String, Skip)>,
    /// The slash across the stubs.
    pub slash: (Pt, Pt),
}

fn in_range(p: Pt) -> bool {
    p.is_finite() && p.x.abs() <= crate::cables::words::MAX_COORD && p.y.abs() <= crate::cables::words::MAX_COORD
}

/// Plan the continuation of `cable` on `page` from `at` toward `toward`: which
/// members continue (`members` restricts and orders them), their stubs, and the slash. Pure: reads
/// the document only. Bounded by the cable's members ([`crate::cables::slash::MAX_CABLE_MEMBERS`]).
///
/// # Errors
/// The refusals in words.
pub fn plan_continue(
    doc: &Document, page: PageId, cable: CableId, members: Option<&[WireId]>, at: Pt, toward: Pt, tol: f64,
) -> Result<Plan, CmdError> {
    let tol = if tol.is_finite() && tol >= 0.0 { tol } else { 0.0 };
    if !(in_range(at) && in_range(toward)) {
        return Err(CmdError::refused(crate::cables::words::OUT_OF_RANGE));
    }
    if at.near(toward, tol) {
        return Err(CmdError::refused(TOWARD_EQUAL));
    }
    let c = doc.cables().get(&cable).ok_or(CmdError::refused(crate::msg::NO_SUCH_CABLE))?;
    let references = references_of(doc, cable);
    let Some((source, s)) = references.first().map(|(id, r)| (*id, *r)) else { return Err(CmdError::refused(no_reference(doc, cable))) };
    if references.len() >= MAX_REFERENCES_PER_CABLE {
        return Err(CmdError::refused(already_continues(doc, cable)));
    }
    if s.page == page {
        return Err(CmdError::refused(drawn_here(doc, cable)));
    }
    let source_at = s.items.first().and_then(|i| doc.items().get(i)).and_then(|i| match &i.kind {
        ecad_model::ItemKind::Primitive(ecad_model::Primitive::Point { at }) if at.is_finite() => Some(*at),
        _ => None,
    });
    // A core is named by its wire, or by its line while it has no wire yet.
    let core_id = |m: &ecad_model::Core| m.wire().map(WireId::hex).or_else(|| m.route().map(RouteId::hex));
    let placed: Vec<(usize, String)> = c.cores.iter().enumerate().filter_map(|(i, m)| Some((i, core_id(m)?))).collect();
    let candidates: Vec<(usize, String)> = match members {
        None => placed.clone(),
        Some(list) => {
            if list.len() > crate::cables::slash::MAX_CABLE_MEMBERS {
                return Err(CmdError::refused(too_many_members()));
            }
            let mut seen = BTreeSet::new();
            let mut out = Vec::with_capacity(list.len());
            for w in list {
                let Some(found) = placed.iter().find(|(_, h)| *h == w.hex()) else {
                    return Err(CmdError::refused(format!("conductor {} is not a member of cable {}", w.hex(), cable.hex())));
                };
                if !seen.insert(w.hex()) {
                    return Err(CmdError::refused(format!("conductor {} is listed twice", w.hex())));
                }
                out.push(found.clone());
            }
            out
        }
    };
    let drawn_on = |i: usize, pg: PageId| match c.cores.get(i).map(|m| m.position) {
        Some(ecad_model::CorePosition::Wire(w)) => crate::wires::routes::wire_pages(doc, w).contains(&pg),
        Some(ecad_model::CorePosition::Route { route, .. }) => doc.routes().get(&route).is_some_and(|r| r.page == pg),
        _ => false,
    };
    if !candidates.is_empty() && candidates.iter().all(|(i, _)| drawn_on(*i, page)) {
        return Err(CmdError::refused(drawn_here(doc, cable)));
    }
    // Only a core on a loose line of the source page continues: its free end goes on through the
    // reference. A wire already runs between two device points.
    let mut going: Vec<(usize, RouteId, usize, Pt)> = Vec::new();
    let mut skipped = Vec::new();
    for (i, h) in &candidates {
        if drawn_on(*i, page) {
            skipped.push((h.clone(), Skip::DrawnHere));
            continue;
        }
        let line = c.cores.get(*i).and_then(|m| m.route()).and_then(|r| Some((r, doc.routes().get(&r)?)));
        let free = line.filter(|(_, r)| r.page == s.page).and_then(|(id, r)| {
            let ends = r.end_points()?;
            let k = (0..2).filter(|k| r.ends[*k] == RouteEnd::Free).min_by(|x, y| {
                let d = |k: usize| source_at.map_or(0.0, |p| ends[k].dist(p));
                d(*x).total_cmp(&d(*y))
            })?;
            Some((id, k, ends[k]))
        });
        match free {
            Some((id, k, at)) => going.push((*i, id, k, at)),
            None => skipped.push((h.clone(), Skip::BothConnected)),
        }
    }
    if going.is_empty() {
        return Err(CmdError::refused(none_can_continue(doc, cable)));
    }
    let spacing = sizes(doc).spacing;
    let (dx, dy) = (toward.x - at.x, toward.y - at.y);
    let l = dx.hypot(dy);
    if !(l.is_finite() && l > tol) {
        return Err(CmdError::refused(TOWARD_EQUAL));
    }
    let d = Pt::new(dx / l, dy / l);
    let n = Pt::new(-d.y, d.x);
    let k = going.len() as f64;
    let mut stubs = Vec::with_capacity(going.len());
    for (i, (core, line, line_end, line_at)) in going.iter().enumerate() {
        let off = spacing * (i as f64 - (k - 1.0) / 2.0);
        let s0 = Pt::new(at.x + n.x * off, at.y + n.y * off);
        let o = Pt::new(s0.x + d.x * l, s0.y + d.y * l);
        if !(in_range(s0) && in_range(o)) {
            return Err(CmdError::refused(crate::cables::words::OUT_OF_RANGE));
        }
        stubs.push(Stub { core: *core, line: *line, line_end: *line_end, line_at: *line_at, a: s0, b: o, free_end: End::B });
    }
    let m = Pt::new(at.x + d.x * l / 2.0, at.y + d.y * l / 2.0);
    let h = spacing * (k - 1.0) / 2.0 + spacing / 2.0;
    let half = spacing / 2.0;
    let slash = (Pt::new(m.x - n.x * h - d.x * half, m.y - n.y * h - d.y * half), Pt::new(m.x + n.x * h + d.x * half, m.y + n.y * h + d.y * half));
    if !(in_range(slash.0) && in_range(slash.1)) {
        return Err(CmdError::refused(crate::cables::words::OUT_OF_RANGE));
    }
    Ok(Plan { source, source_page: s.page, stubs, skipped, slash })
}

/// The lines echoed after `cablecontinue`.
pub(crate) fn continued_lines(doc: &Document, page: PageId, plan: &Plan, this: RepresentationId) -> Vec<String> {
    // Both ends are of one cable, and a reference says only what its own cable's references give,
    // so that cable's are derived, not the document's.
    let (here, there) = (reference_of(doc, this), reference_of(doc, plan.source));
    let (here, there) = (here.as_ref(), there.as_ref());
    let cable = here.map(|r| r.cable).or(there.map(|r| r.cable));
    let title = cable.map(|k| crate::cables::cable_title(doc, k)).unwrap_or_default();
    let mut out = vec![format!(
        "Cable {title} continued on {} with {}; this end shows {} and the end on {} now shows {}.",
        page_number_words(doc, page),
        crate::cables::slash::wires(plan.stubs.len()),
        here.map_or("/?", |r| r.text.as_str()),
        page_number_words(doc, plan.source_page),
        there.map_or("/?", |r| r.text.as_str()),
    )];
    let both: Vec<String> = plan
        .skipped
        .iter()
        .filter(|(_, s)| *s == Skip::BothConnected)
        .map(|(w, _)| WireId::parse(w).map_or_else(|| "a line".to_string(), |w| crate::cables::slash::wire_name(doc, w)))
        .collect();
    match both.as_slice() {
        [] => {}
        [one] => out.push(format!("1 wire was not continued: {one} is connected at both ends.")),
        many => out.push(format!("{} wires were not continued: {} (connected at both ends).", many.len(), crate::cables::slash::name_list(many))),
    }
    out.push("Connect each wire's free end to its connection point with Connect.".into());
    out
}

/// The line added when a transaction left linked references without their other end
/// `before` are the references that were linked before it.
pub fn orphaned_line(doc: &Document, before: &BTreeSet<RepresentationId>) -> Option<String> {
    if before.is_empty() {
        return None;
    }
    orphaned_line_in(doc, &cable_references(doc), before)
}

/// [`orphaned_line`] from references already derived on `doc` (`now`): those of every cable
/// whose references `before` lists, at least.
fn orphaned_line_in(doc: &Document, now: &[CableReference], before: &BTreeSet<RepresentationId>) -> Option<String> {
    let now: Vec<&CableReference> = now.iter().filter(|r| r.orphan && before.contains(&r.reference)).collect();
    match now.as_slice() {
        [] => None,
        [one] => Some(format!(
            "Cable {}'s reference on {} now has no other end.",
            crate::cables::cable_title(doc, one.cable),
            page_number_words(doc, one.page)
        )),
        many => Some(format!("{} cable references now have no other end.", many.len())),
    }
}

/// The references that are linked (have another end) in `doc`.
pub fn linked(doc: &Document) -> BTreeSet<RepresentationId> {
    cable_references(doc).into_iter().filter(|r| !r.orphan).map(|r| r.reference).collect()
}

// ---------------------------------------------------------------- per commit

/// Style catalogues no cable reference reads: a change of one reaches no reference.
const NO_REFERENCE_STYLES: &[ecad_model::StyleCatalogue] =
    &[ecad_model::StyleCatalogue::Text, ecad_model::StyleCatalogue::Wire, ecad_model::StyleCatalogue::Location];

/// What one effect reaches among the cables' references ([`cables_touched_by`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Touch {
    /// The cables it reaches (possibly none) were added to the set it was given.
    Cables,
    /// Every cable, and why: only an effect that can change what every reference reads (the page
    /// grid, the reference style or length unit, a standard's sheets). Each such arm needs its
    /// reason and is counted by `cargo xtask layers` (views guard).
    All(&'static str),
}

/// Adds to `out` the cables whose references, or whose drawing at a reference end on any page, one
/// effect could change, looked up in `doc` before the transaction; [`Touch::All`] when it could
/// reach any cable. The match over [`Effect`] has no wildcard: a new kind of effect does not
/// compile until it says here which cables it reaches.
///
/// # Semantics
/// What a reference says, and how a cable is drawn at its reference ends, depends only on: the
/// cable's reference representations and their points; its members' routes (direction of the
/// text, which pages each member is routed on); its members' end attachments; the cable itself;
/// and the page order and length unit. So a cable is touched when an op changes or removes an item
/// of a representation of the cable or of one of its members, links, changes or unlinks such a
/// representation, adds, changes or removes a line of a member or one landing on the cable's named
/// points, or attaches or detaches a member's end. New items, components, connection points,
/// definitions and conductors are not shown or not members yet, so they touch nothing.
/// The cables of the pages
/// whose book position changes are not added here: `crate::references::devices::watch::Watch` adds them after the
/// commit, comparing the book before with the book after.
pub fn cables_touched_by(doc: &Document, effect: &ecad_model::document::effect::Effect<'_>, out: &mut BTreeSet<CableId>) -> Touch {
    use ecad_model::ObjectRef;
    use ecad_model::document::effect::Effect;
    let note = |r: &Representation, out: &mut BTreeSet<CableId>| match r.subject {
        Subject::Cable(k) => {
            out.insert(k);
        }
        Subject::Wire(w) => out.extend(doc.cables_of(w)),
        Subject::Device(_) | Subject::Route(_) | Subject::Aid(_) => {}
    };
    // A frame decides its pages' grids, so a frame choice
    // or a frame's sheet, margins or block height reaches the cables on those pages.
    match crate::title_blocks::frame_reach_of(doc, effect) {
        crate::title_blocks::Reach::NotFrame => {}
        crate::title_blocks::Reach::Unchanged => return Touch::Cables,
        crate::title_blocks::Reach::Pages(ps) => {
            for p in ps {
                out.extend(cables_on(doc, p));
            }
            return Touch::Cables;
        }
        crate::title_blocks::Reach::Wide => return Touch::All("a frame or standard change moves every page's grid"), // all
    }
    match effect {
        // A new item is not shown yet; a changed or removed one, through its representation.
        Effect::Item { before: None, .. } => {}
        Effect::Item { id, .. } => {
            if let Some((_, r)) = doc.representation_of_item(*id) {
                note(r, out);
            }
        }
        Effect::Rep { before, after, .. } => {
            for r in before.iter().chain(after.iter()) {
                note(r, out);
            }
        }
        // A line reaches the cables whose cores it carries (on the
        // line, or on the wire it draws) and the cables whose named points it lands on.
        Effect::Route { id, before, after } => {
            for r in before.iter().chain(after.iter()) {
                route_cables(doc, *id, r, out);
            }
        }
        Effect::Wire { id, before: None, .. } if !doc.wires().contains_key(id) => {}
        // A wire made again in one transaction: any cable, as before.
        Effect::Wire { before: None, .. } => return Touch::All("a wire made again in one transaction may have been a core of any cable"), // all
        // A
        // conductor's cables only, never every cable.
        Effect::Wire { id, .. } | Effect::Prop { object: ObjectRef::Wire(id), .. } => out.extend(doc.cables_of(*id)),
        // A device record (made, removed or changed: its tag, location, function, definition)
        // reaches no cable reference: a reference says only its own cable's other ends' page and
        // column or row (`derive::references_from`), never a device, strip or connector name. A
        // removed component's connection points and representations go as effects of their own.
        Effect::Device { .. } => {}
        Effect::ConnectionPoint { id, before: None, .. } if !doc.connection_points().contains_key(id) => {}
        // A relabelled connection point reaches only the cables of the wires on it.
        Effect::ConnectionPoint { id, .. } => out.extend(doc.ends_at(*id).into_iter().flat_map(|(w, _)| doc.cables_of(w))),
        // A symbol definition, a device's definition or link and a layer are read by no reference
        // (a reference reads items' points, routes, pages and the settings below).
        Effect::Definition { .. } | Effect::DeviceDefinition { .. } | Effect::DeviceLink { .. } | Effect::Layer { .. } => {}
        // A device's properties (its tag) are drawn beside its symbols only, never
        // in a cable's drawing or reference texts.
        Effect::Prop { object: ObjectRef::Device(_), .. } => {}
        // A page's number or sheet changes what the references on it
        // say about themselves, so their cables (whose other ends show it).
        Effect::Page { id: p, before: Some(b), after: Some(a), index_before: None, index_after: None } => {
            let ch = b.change(a);
            if ch.number || ch.sheet || ch.page_type {
                out.extend(cables_on(doc, *p));
            }
        }
        // A page type's sheet, on its pages only.
        Effect::Style { catalogue: ecad_model::StyleCatalogue::Page, .. } => {
            if let Some((pages, true)) = crate::styles::page::touched_by_effect(doc, effect) {
                for p in pages {
                    out.extend(cables_on(doc, p));
                }
            }
        }
        // A page moved between folders, a page added, removed or moved, a folder effect: only the
        // pages whose book position changed may read differently (an unnumbered page's number,
        // the order of a cable's partners); a rename in place or an append moves none.
        Effect::Prop { object: ObjectRef::Page(_) | ObjectRef::Project, .. } => {}
        Effect::Page { .. } | Effect::Folder { .. } => {}
        Effect::Version { .. } => {}
        // A cable op changes that cable's members only, so only its
        // reference ends may be drawn differently. A cable's fields (its type, tag)
        // are that cable's alone.
        Effect::Cable { id, .. } => {
            out.insert(*id);
        }
        Effect::Settings { before, after }
            if before.length_unit == after.length_unit && before.sheet == after.sheet && before.reference_style == after.reference_style => {}
        // The text, wire and location styles only change how things are drawn; no
        // reference reads them. Any other catalogue (the frame, one added later) and a standard,
        // which may come to carry sheets, still reach every cable.
        Effect::Style { catalogue, .. } if NO_REFERENCE_STYLES.contains(catalogue) => {}
        // A page type's sheet that is not its own pages' (handled above) or any other catalogue
        // (the frame, one added later) and a standard, which may come to carry sheets.
        Effect::Style { .. } => return Touch::All("a style catalogue a reference may read (frame, one added later)"), // all
        Effect::Standard { .. } => return Touch::All("a standard may carry sheets and grids"),                        // all
        // The sheet, length unit or reference style (the guard above passed the other fields).
        Effect::Settings { .. } => return Touch::All("the sheet, length unit or reference style is read by every reference"), // all
        Effect::Prop { object: ObjectRef::Cable(k), .. } => {
            out.insert(*k);
        }
        Effect::Prop { object: ObjectRef::Standard, .. } => return Touch::All("a standard's registry value may be a sheet or grid"), // all
        Effect::Project { .. } => return Touch::All("the project record may carry the reference style or sheet"),                    // all
        Effect::Aid { .. } => return Touch::All("a drawing aid may stand where a reference does (design 072 §6.2)"),                 // all
        // A loose part is drawn on no page and no reference reads it.
        Effect::LoosePart { .. } => {}
    }
    Touch::Cables
}

/// The cables line `id` (as `route`) reaches, through the indexes: those with a core on it or on
/// the wire it draws, and those whose named points it lands on.
fn route_cables(doc: &Document, id: RouteId, route: &ecad_model::Route, out: &mut BTreeSet<CableId>) {
    out.extend(doc.cables_on_route(id));
    out.extend(ecad_model::connectivity::wire_order::wire_of_route(doc, id).into_iter().flat_map(|w| doc.cables_of(w)));
    out.extend(route.ends.iter().filter_map(|e| doc.cable_point(e.point()?)).map(|(k, _)| k));
}

/// The cables with a reference on `page` (through the page index).
pub(crate) fn cables_on(doc: &Document, page: PageId) -> impl Iterator<Item = CableId> + '_ {
    doc.representations_on(page).filter(|(_, r)| is_reference(r)).filter_map(|(_, r)| match r.subject {
        Subject::Cable(k) => Some(k),
        _ => None,
    })
}

/// Per member of a cable: the conductor, the pages it is routed on, and its end attachments.
type MemberState = Vec<(ecad_model::CorePosition, Vec<PageId>, Option<[ConnectionPointId; 2]>)>;

/// The references of some cables at one moment, and what their reference ends are drawn from, so
/// that one commit derives them once before and once after.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    /// The cables covered; `None` for every cable.
    scope: Option<BTreeSet<CableId>>,
    /// Their references, as [`cable_references`] derives them.
    references: Vec<CableReference>,
    /// Per covered cable with a reference, its members' state (what the drawing at a
    /// reference end reads from other pages). Empty for a whole-document snapshot.
    members: BTreeMap<CableId, MemberState>,
}

impl Snapshot {
    /// The references of `scope` (every cable for `None`) in `doc`.
    pub fn take(doc: &Document, scope: Option<&BTreeSet<CableId>>) -> Snapshot {
        let references = match scope {
            Some(k) => cable_references_of(doc, k),
            None => cable_references(doc),
        };
        let mut members = BTreeMap::new();
        if scope.is_some() {
            let with_references: BTreeSet<CableId> = references.iter().map(|r| r.cable).collect();
            for k in with_references {
                let list = doc.cables().get(&k).map(|c| c.cores.as_slice()).unwrap_or_default();
                let state: MemberState = list
                    .iter()
                    .map(|m| {
                        let ends = m.wire().and_then(|w| doc.wires().get(&w)).map(|c| c.ends);
                        (m.position, crate::cables::member_pages(doc, m).into_iter().collect(), ends)
                    })
                    .collect();
                members.insert(k, state);
            }
        }
        Snapshot { scope: scope.cloned(), references, members }
    }

    /// The covered references that are linked (have another end), as [`linked`] lists them.
    pub fn linked(&self) -> BTreeSet<RepresentationId> {
        self.references.iter().filter(|r| !r.orphan).map(|r| r.reference).collect()
    }

    /// The line for references linked in `self` (before a transaction) that
    /// have no other end in `after` (taken with the same scope on `doc`, after it), exactly as
    /// [`orphaned_line`] gives it: references of cables outside the scope did not change.
    pub fn orphaned_line(&self, doc: &Document, after: &Snapshot) -> Option<String> {
        let before = self.linked();
        if before.is_empty() {
            return None;
        }
        orphaned_line_in(doc, &after.references, &before)
    }

    /// The pages whose reference ends may be drawn differently in `after` than in `self`. `None`
    /// means every page: a whole-document snapshot with any reference before or after (as before
    /// Otherwise, the pages of the references of each covered cable whose references
    /// or member state changed.
    pub(crate) fn pages_changed(&self, after: &Snapshot) -> Option<BTreeSet<PageId>> {
        let Some(scope) = &self.scope else {
            if self.references.is_empty() && after.references.is_empty() {
                return Some(BTreeSet::new());
            }
            return None;
        };
        let mut pages = BTreeSet::new();
        for k in scope {
            fn of(s: &Snapshot, k: CableId) -> Vec<&CableReference> {
                s.references.iter().filter(|r| r.cable == k).collect()
            }
            let (old, new) = (of(self, *k), of(after, *k));
            if self.members.get(k) != after.members.get(k) {
                // What every reference end of the cable is drawn from changed.
                pages.extend(old.iter().chain(new.iter()).map(|r| r.page));
                continue;
            }
            // Otherwise only the pages of references that say something else, appeared or went.
            for r in &old {
                if !new.iter().any(|n| n == r) {
                    pages.insert(r.page);
                }
            }
            for n in &new {
                if !old.iter().any(|r| r == n) {
                    pages.insert(n.page);
                }
            }
        }
        Some(pages)
    }
}

/// Hover lines for a reference end.
pub fn hover_lines(doc: &Document, reference: RepresentationId) -> Option<Vec<String>> {
    let references = cable_references(doc);
    let r = references.iter().find(|r| r.reference == reference)?;
    let title = crate::cables::cable_title(doc, r.cable);
    let place = |x: &CableReference| match (&x.column_label, &x.row_label) {
        (Some(c), Some(row)) => {
            format!("{}, column {c}, row {row}, {}", page_words(doc, x.page), crate::cables::slash::wires(x.wires_here))
        }
        _ => format!("{}, cannot be located, {}", page_words(doc, x.page), crate::cables::slash::wires(x.wires_here)),
    };
    let mut lines =
        vec![format!("Cable reference  {}", r.text), format!("Cable {title}   id {}", reference.hex()), format!("This end: {}", place(r))];
    match r.partner().and_then(|p| references.iter().find(|x| x.reference == p)) {
        None => lines.push("Other end: none (orphan): continue the cable on another page with Continue Cable".to_string()),
        Some(o) => {
            lines.push(format!("Other end: {}", place(o)));
            // Built once per hover; the lines stop one past the hover's cap, so the caller still
            // says `… and N more`.
            let shown: BTreeSet<ecad_model::ConnectionPointId> =
                ecad_model::connectivity::derive::connection_point_positions(doc, o.page).into_iter().map(|(t, _, _)| t).collect();
            let members = doc.cables().get(&r.cable).map(|c| c.cores.as_slice()).unwrap_or_default();
            for m in members.iter().filter(|m| crate::cables::member_pages(doc, m).contains(&o.page)) {
                if lines.len() > crate::cables::words::MAX_HOVER_LINES {
                    break;
                }
                let connection_point = m.wire().and_then(|w| doc.wires().get(&w)).and_then(|c| connection_point_on(doc, &shown, c));
                let to = connection_point.unwrap_or_else(|| "(not connected)".into());
                lines.push(format!("  {} -> {to}", crate::cables::slash::position_name(doc, m.position)));
            }
        }
    }
    Some(lines)
}

/// `K2:1`: the connection point among `shown` (those on the other end's page) a conductor attaches to.
fn connection_point_on(doc: &Document, shown: &BTreeSet<ecad_model::ConnectionPointId>, c: &ecad_model::Wire) -> Option<String> {
    let t = End::BOTH.into_iter().map(|e| c.end(e)).find(|t| shown.contains(t))?;
    // A connector pin is named by its half (`-X10.S:3`).
    if let Some(name) = crate::connectors::pin_name(doc, t) {
        return Some(name);
    }
    let term = doc.connection_points().get(&t)?;
    let c = term.device()?;
    let comp = doc.devices().get(&c).and_then(|x| x.tag_shown()).unwrap_or_else(|| format!("{}…", c.hex().get(..8).unwrap_or_default()));
    Some(format!("{comp}:{}", crate::terminal_strips::terminal_blocks::connection_point_name(doc, t)))
}

/// The debugger's `Cable references` diagnostics: one line per orphan, one per
/// cable with more references than supported, or `No orphan references.`. Derived, never saved.
pub fn diagnostics(doc: &Document) -> Vec<String> {
    let references = cable_references(doc);
    let mut out: Vec<String> = references
        .iter()
        .filter(|r| r.orphan)
        .map(|r| format!("Orphan: cable {} on {} ({})", crate::cables::cable_title(doc, r.cable), page_number_words(doc, r.page), r.text))
        .collect();
    if out.is_empty() {
        out.push("No orphan references.".into());
    }
    // Counted once for every cable (a hostile file may hold many references of many cables).
    let mut counts: BTreeMap<CableId, usize> = BTreeMap::new();
    for r in &references {
        *counts.entry(r.cable).or_default() += 1;
    }
    for (k, n) in counts.into_iter().filter(|(_, n)| *n > MAX_REFERENCES_PER_CABLE) {
        out.push(format!("{} has {n} references; only two are supported.", cable_words(doc, k)));
    }
    out
}

// ---------------------------------------------------------------- a cable shown on another page

/// The cable whose tag is `typed` (normalised as tags are), if any.
pub(crate) fn cable_tagged(doc: &Document, typed: &str) -> Option<CableId> {
    let ecad_model::naming::tag::Entry::Tag(given) = ecad_model::naming::tag::enter(typed, crate::devices::padding(doc)).ok()? else { return None };
    doc.cables_with(&given.key()).into_iter().next()
}

/// `-W04` in messages: the cable's title.
pub fn title(doc: &Document, cable: CableId) -> String {
    crate::cables::cable_title(doc, cable)
}

/// Most wires a refusal names before `+n`.
const MAX_NAMED: usize = 6;

/// One core a slash with an existing cable's tag crosses on this page.
#[derive(Debug, Clone, PartialEq)]
pub struct Continued {
    /// Where the core is: its wire, or the loose line it sits on.
    pub position: CorePosition,
    /// The core's label.
    pub core: Option<String>,
}

/// Plan `cableslash` with an existing cable's tag on `page` across `crossed`. Continuations and cable references are
/// named points, so a core through them is one wire on every page it runs on: the slash shows `cable` where its wires
/// already run and adds no member. Each crossed core must be one of `cable`'s.
///
/// # Errors
/// `cable` already has a slash on this page; none of its wires or lines runs on this page; a
/// crossed wire or line is not one of its cores.
pub fn plan_slash_continue(doc: &Document, page: PageId, cable: CableId, crossed: &[CorePosition]) -> Result<Vec<Continued>, CmdError> {
    let c = doc.cables().get(&cable).ok_or(CmdError::refused(crate::msg::NO_SUCH_CABLE))?;
    let tag = title(doc, cable);
    if doc.representations_of(Subject::Cable(cable)).any(|(_, r)| r.page == page && r.role == ecad_model::Role::Slash) {
        return Err(CmdError::refused(format!("{tag} already has a slash on this page.")));
    }
    if !c.cores.iter().any(|m| crate::cables::member_pages(doc, m).contains(&page)) {
        return Err(CmdError::refused(no_wire_here(&tag)));
    }
    let mut out = Vec::new();
    let mut not = Vec::new();
    for at in crossed {
        match c.cores.iter().find(|m| m.position == *at && *at != CorePosition::Unplaced) {
            Some(m) => out.push(Continued { position: *at, core: m.label_str().map(str::to_string) }),
            None => not.push(match at {
                CorePosition::Wire(w) => format!("Wire {} is not a core of {tag}.", crate::cables::slash::wire_name(doc, *w)),
                CorePosition::Route { .. } | CorePosition::Unplaced => format!("A line it crosses is not a core of {tag}."),
            }),
        }
    }
    if !not.is_empty() {
        let more = not.len().saturating_sub(MAX_NAMED);
        let mut lines: Vec<String> = not.into_iter().take(MAX_NAMED).collect();
        if more > 0 {
            lines.push(format!("+{more}"));
        }
        lines.push(format!("A slash with {tag}'s tag shows it where its wires run; slash other wires with a new tag."));
        return Err(CmdError::refused(lines.join(
            "
",
        )));
    }
    Ok(out)
}

/// Refused: a slash with `tag` on a page where none of the cable's wires runs.
pub(crate) fn no_wire_here(tag: &str) -> String {
    format!(
        "{tag} has no wire on this page: a slash with its tag shows it where its wires run. Continue it here with Continue cable, or slash these wires with a new tag."
    )
}

/// What erasing cable slashes adds to a drawing edit. `erased` are the representations
/// the edit unlinks, read on the document before it. Erasing one of a cable's several slashes
/// removes the members routed only on that slash's page; erasing its last slash keeps the cable
/// and its members.
pub fn slash_erase_followups(doc: &Document, erased: &[(RepresentationId, Representation)]) -> Vec<Op> {
    let gone: BTreeSet<RepresentationId> = erased.iter().map(|(id, _)| *id).collect();
    let mut by_cable: BTreeMap<CableId, BTreeSet<PageId>> = BTreeMap::new();
    for (_, r) in erased {
        if let Subject::Cable(k) = r.subject
            && r.role == ecad_model::Role::Slash
        {
            by_cable.entry(k).or_default().insert(r.page);
        }
    }
    let mut ops = Vec::new();
    for (k, pages) in by_cable {
        let Some(c) = doc.cables().get(&k) else { continue };
        let kept = doc.representations_of(Subject::Cable(k)).any(|(id, r)| r.role == ecad_model::Role::Slash && !gone.contains(&id));
        if !kept {
            continue;
        }
        let only_there = |m: &ecad_model::Core| {
            // A continued core runs on through its named points: it stays.
            if !m.through.is_empty() {
                return false;
            }
            let on = crate::cables::member_pages(doc, m);
            !on.is_empty() && on.iter().all(|p| pages.contains(p))
        };
        let cores: Vec<ecad_model::Core> = c.cores.iter().filter(|m| !only_there(m)).cloned().collect();
        Op::edit_cable(doc, &mut ops, k, |c| c.cores = cores);
    }
    reference_erase_followups(doc, erased, &mut ops);
    ops
}

/// The follow-ups of erasing cable references: their named points go, so
/// each core forgets them and a line landing on one is left with a free end there.
fn reference_erase_followups(doc: &Document, erased: &[(RepresentationId, Representation)], ops: &mut Vec<Op>) {
    let removed: BTreeSet<RouteId> = erased
        .iter()
        .filter_map(|(_, r)| match r.subject {
            Subject::Route(x) => Some(x),
            _ => None,
        })
        .collect();
    for (_, r) in erased {
        let Subject::Cable(k) = r.subject else { continue };
        if r.role != ecad_model::Role::Reference || r.connection_points.is_empty() {
            continue;
        }
        let gone: BTreeSet<ConnectionPointId> = r.connection_points.keys().copied().collect();
        Op::edit_cable(doc, ops, k, |c| {
            for core in &mut c.cores {
                core.through.retain(|t| !gone.contains(t));
            }
        });
        let lines: BTreeSet<RouteId> = gone.iter().flat_map(|t| doc.routes_at(*t)).filter(|x| !removed.contains(x)).collect();
        for id in lines {
            let Some(before) = doc.routes().get(&id) else { continue };
            let mut after = before.clone();
            for e in &mut after.ends {
                if e.point().is_some_and(|t| gone.contains(&t)) {
                    *e = RouteEnd::Free;
                }
            }
            ops.push(Op::ModifyRoute { id, before: before.clone(), after });
        }
    }
}

/// Cables with more than [`MAX_REFERENCES_PER_CABLE`] references (possible only from a file), in
/// ID order.
pub fn over_referenced(references: &[CableReference]) -> Vec<ecad_model::CableId> {
    let mut n: std::collections::BTreeMap<ecad_model::CableId, usize> = std::collections::BTreeMap::new();
    for r in references {
        *n.entry(r.cable).or_default() += 1;
    }
    n.into_iter().filter(|(_, c)| *c > MAX_REFERENCES_PER_CABLE).map(|(k, _)| k).collect()
}

/// The cable references of `doc` as `cablereferences` and the snapshot's `checks.cable_references` list
/// them: one object per reference, in page order, then column, then ID.
pub fn references_json(doc: &Document, references: &[CableReference]) -> Value {
    Value::Array(
        references
            .iter()
            .map(|r| {
                json!({
                    "reference": r.reference.hex(),
                    "cable": r.cable.hex(),
                    "cable_tag": doc.cables().get(&r.cable).and_then(|c| c.tag_shown()),
                    "page": r.page.hex(),
                    "page_number": r.page_number,
                    "page_title": doc.pages().get(&r.page).map(|p| p.title.to_string()).unwrap_or_default(),
                    "at": r.at.map(|p| json!([p.x, p.y])),
                    "column": r.column,
                    // The frame's row and labels, and this end's own place.
                    "row": r.row,
                    "column_label": r.column_label,
                    "row_label": r.row_label,
                    "place": r.locator(),
                    "text": r.text,
                    "partner": r.partner().map(|p| p.hex()),
                    "orphan": r.orphan,
                    "wires_here": r.wires_here,
                })
            })
            .collect(),
    )
}
