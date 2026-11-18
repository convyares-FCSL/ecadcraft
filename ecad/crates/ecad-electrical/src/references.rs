//! Cable references across pages (brief 010, design 010): where the drawing of a cable stops on
//! one page, a reference end shows where it continues, live.
//!
//! # Semantics
//! - A **reference end** is an ordinary drawn representation of the cable with role
//!   [`REFERENCE_ROLE`] and one Point item (design 010 §0). Nothing else is stored for it: no
//!   pairing, no text. A cable has at most two references in this version (one continuation).
//! - Everything a reference **says** (the other end's page number and column, its partner, whether
//!   it is an orphan) is derived on every draw by [`ecad_model::derive::cable_references`] and never
//!   stored. Renaming, reordering or moving pages changes it with no transaction of its own.
//! - [`markers_on_page`] is **the one function** that says what a reference end shows on a page:
//!   its point, its derived text (`/<page>.<column>` of the other end, `/?` for an orphan), its
//!   colour state and where the text sits. The projection draws its minimal marker from it, the
//!   overlay rings it, the hover and `cablerefs` read the same derivation; brief 009's styled
//!   marker calls it too, so both briefs always show the same text.
//! - [`plan_continue`] decides what `cablecontinue` draws (design 010 §5): its JSON form and its
//!   interactive preview both use it, so what is previewed is what is committed.
//!
//! This module has no UI and records nothing: the commands that use it commit through their JSON
//! forms (`ecad_cadcraft::cmd`).

use std::collections::{BTreeMap, BTreeSet};

use crate::CmdError;
use ecad_model::derive::{CableReference, cable_references, cable_references_of};
use ecad_model::{CableId, Document, End, ItemId, LengthUnit, Op, PageId, Pt, Representation, RepresentationId, Subject, TerminalId, WireId};

pub use ecad_model::derive::{MAX_REFERENCES_PER_CABLE, REFERENCE_ROLE};

use crate::text_style::fonts::TextMetrics;

/// `ecad.reference`: ACI 6, magenta (the colour of `ECAD-CABLES`, so a linked text is ByLayer).
pub const REFERENCE_ACI: u8 = 6;
/// `ecad.orphan`: ACI 1, red.
pub const ORPHAN_ACI: u8 = 1;
/// `ecad.ref-stub-spacing` in millimetres: between continued stubs, and from a reference point to
/// its text.
pub const STUB_SPACING_MM: f64 = 5.0;
/// `ecad.ref-size`: the overlay ring on a reference point, in screen pixels (stroke `ecad.bold`).
pub const RING_PX: f32 = 12.0;
/// `ecad.bold`, in screen pixels.
pub const BOLD_PX: f32 = 2.0;

/// Refusal of an edit of the derived text (design 010 §9).
pub const TEXT_DERIVED: &str = "The reference text is derived; move the reference point instead.";

/// Drawing-unit sizes of reference drawing, in the project's length unit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sizes {
    pub text_height: f64,
    pub spacing: f64,
}

/// The sizes for `doc`'s length unit (millimetre tokens divided by 25.4 in an inch project).
pub fn sizes(doc: &Document) -> Sizes {
    let k = match doc.settings.length_unit {
        LengthUnit::Millimetre => 1.0,
        LengthUnit::Inch => 1.0 / 25.4,
    };
    // Brief 066d: reference text is the `Reference` text style, already in drawing units.
    Sizes { text_height: crate::text_style::text_style(doc, crate::text_style::TextKind::Reference).height, spacing: STUB_SPACING_MM * k }
}

/// Whether `r` is a cable reference end.
pub fn is_reference(r: &Representation) -> bool {
    r.role == REFERENCE_ROLE && matches!(r.subject, Subject::Cable(_))
}

/// The reference representations of `cable`, in ID order.
pub fn references_of(doc: &Document, cable: CableId) -> Vec<(RepresentationId, &Representation)> {
    doc.representations_of(Subject::Cable(cable)).filter(|(_, r)| r.role == REFERENCE_ROLE).collect()
}

/// Every reference end of the document, derived (page order, then column, then ID).
pub fn all(doc: &Document) -> Vec<CableReference> {
    cable_references(doc)
}

/// The reference ends on `page`, derived.
pub fn references_on_page(doc: &Document, page: PageId) -> Vec<CableReference> {
    page_cable_references(doc, page).into_iter().filter(|r| r.page == page).collect()
}

/// The references of the cables with a reference end on `page`, derived as [`all`] derives them
/// (a reference's text reads only its own cable's references and routes; brief 014's
/// [`cable_references_of`]). Cost is in proportion to the page's representations and those
/// cables, not to the document (brief 053: `wire` and `joinwires` derived every reference of the
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

/// What one reference end shows on its page (design 010 §6.1, §6.2). Call [`markers_on_page`].
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
    /// The unit direction away from the wires (design 010 §6.2's u): the side brief 009's flag
    /// takes.
    pub text_dir: Pt,
    /// Text height in drawing units.
    pub height: f64,
}

/// What every locatable reference end on `page` shows. **The** function both the minimal marker of
/// this brief and brief 009's styled marker take their text from: one derivation, always live.
/// Ends that cannot be located (a missing or non-finite point, only from a hostile file) are left
/// out; `cablerefs` and the hover still list them with `/?`.
pub fn markers_on_page(doc: &Document, page: PageId) -> Vec<Marker> {
    markers_from(doc, page_cable_references(doc, page), page)
}

/// [`markers_on_page`] from references already derived (`refs`, as [`all`] returns them), so one
/// draw derives them once.
pub fn markers_from(doc: &Document, refs: Vec<CableReference>, page: PageId) -> Vec<Marker> {
    let z = sizes(doc);
    refs.into_iter()
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

/// [`marker_text`] with the direction away from the wires (design 010 §6.2's u), which brief
/// 009's flag points along.
///
/// Derives only the references of that end's cable (all a reference's text depends on), so a page
/// with reference ends costs in proportion to their cables, not to the document (brief 014).
pub fn marker_text_dir(doc: &Document, reference: RepresentationId) -> Option<(String, bool, Pt)> {
    let Some(Subject::Cable(cable)) = doc.representations.get(&reference).map(|r| r.subject) else { return None };
    cable_references_of(doc, &BTreeSet::from([cable])).into_iter().find(|r| r.reference == reference).map(|r| (r.text, r.orphan, r.text_dir))
}

// The marker of a reference end (flag and text) is drawn by brief 009's `look.rs` only, with the
// text from [`marker_text`]; its text is mapped `Mapped::RefText`, so hover, Go to Reference and
// erasing it act on the reference. The ends of member routes that leave for another page (design
// 010 §6.4) show no loose-end square: `look.rs` folds that rule into its `quiet` ends.

/// The reference end on `page` under `p`: the nearest reference point within `radius`, else a
/// reference whose drawn text (its box, grown by `radius`) holds `p`. Used before the generic pick
/// by Go to Reference and the hover (design 010 §4.5, §8), because a continued end's middle stub
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
    let h = crate::words::label_height(doc);
    markers.iter().find(|x| text_box_holds(x, p, radius, h, m)).map(|x| x.reference)
}

/// Whether `p` lies in a marker's box (brief 009's flag) grown by `pad`.
fn text_box_holds(m: &Marker, p: Pt, pad: f64, h: f64, metrics: &dyn TextMetrics) -> bool {
    // Brief 009 draws the marker: its flag around the text, at the reference point, on the side
    // away from the wires.
    let w = crate::words::stroke_label_width(&crate::words::shorten(&m.text, 64), h, metrics);
    let (lo, hi) = crate::words::flag_box(m.at, w, crate::words::FlagSide::away(m.text_dir), h);
    p.x >= lo.x - pad && p.x <= hi.x + pad && p.y >= lo.y - pad && p.y <= hi.y + pad
}

/// The pages (in page order) where `conductor` has a route ([`crate::wires::wire_pages`]).
pub fn route_pages(doc: &Document, conductor: WireId) -> Vec<PageId> {
    let on = crate::wires::wire_pages(doc, conductor);
    doc.page_order.iter().copied().filter(|p| on.contains(p)).collect()
}

/// The number a page is shown with (brief 019): its `number` property, else its position in book
/// order (`1` first), else `?`. Every place that shows a page's number (the Project panel, the
/// page window's title, reference texts' words) asks here.
pub fn page_number(doc: &Document, page: PageId) -> String {
    ecad_model::book::page_number(doc, page)
}

/// `page 2 "Field"` (or `page ? "Field"` for a page in no order).
pub fn page_words(doc: &Document, page: PageId) -> String {
    let n = page_number(doc, page);
    let title = doc.pages.get(&page).map(|p| p.title.as_str()).unwrap_or_default();
    format!("page {n} \"{title}\"")
}

/// `page 2` (or `page ?`).
pub fn page_number_words(doc: &Document, page: PageId) -> String {
    format!("page {}", page_number(doc, page))
}

/// `Cable C1` in messages: its tag, else `8a3f19c0…`.
pub fn cable_words(doc: &Document, cable: CableId) -> String {
    format!("Cable {}", crate::cables::cable_title(doc, cable))
}

// ---------------------------------------------------------------- refusals (design 010 §4.4)

/// R1.
pub fn already_here(doc: &Document, cable: CableId) -> String {
    format!("{} already has a reference on this page.", cable_words(doc, cable))
}
/// R2.
pub fn already_continues(doc: &Document, cable: CableId) -> String {
    format!("{} already continues to another page; a cable can have only two references in this version.", cable_words(doc, cable))
}
/// R3.
pub fn no_wires_here(doc: &Document, cable: CableId) -> String {
    format!("{} has no wires drawn on this page; draw or continue it here first.", cable_words(doc, cable))
}
/// R4.
pub fn no_reference(doc: &Document, cable: CableId) -> String {
    format!("{} has no reference to continue from; place one with Cable Reference on its page first.", cable_words(doc, cable))
}
/// R5.
pub fn drawn_here(doc: &Document, cable: CableId) -> String {
    format!("{} is already drawn on this page; continue it on another page.", cable_words(doc, cable))
}
/// R6.
pub fn none_can_continue(doc: &Document, cable: CableId) -> String {
    format!(
        "None of cable {}'s wires can continue: each is connected at both ends or already drawn on this page.",
        crate::cables::cable_title(doc, cable)
    )
}
/// R9.
pub fn too_many_members() -> String {
    format!("`members` lists more than {} wires", crate::slash::MAX_CABLE_MEMBERS)
}
/// `toward` too close to `at`.
pub const TOWARD_EQUAL: &str = "`toward` must differ from `at`";

/// The checks `cableref` makes before placing a reference for `cable` on `page` (R1 to R3).
///
/// # Errors
/// `no such cable`, or the refusal's words.
pub fn check_reference(doc: &Document, page: PageId, cable: CableId) -> Result<(), CmdError> {
    let c = doc.cables.get(&cable).ok_or(CmdError::refused(crate::msg::NO_SUCH_CABLE))?;
    let refs = references_of(doc, cable);
    if refs.iter().any(|(_, r)| r.page == page) {
        return Err(CmdError::refused(already_here(doc, cable)));
    }
    if refs.len() >= MAX_REFERENCES_PER_CABLE {
        return Err(CmdError::refused(already_continues(doc, cable)));
    }
    if !c.members.iter().any(|m| crate::wires::wire_pages(doc, m.conductor).contains(&page)) {
        return Err(CmdError::refused(no_wires_here(doc, cable)));
    }
    Ok(())
}

// ---------------------------------------------------------------- continuation (design 010 §5)

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

/// One continued member: its stub and which end is free (the outer one).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stub {
    pub conductor: WireId,
    /// The stub's first vertex (End A) and last (End B).
    pub a: Pt,
    pub b: Pt,
    pub free_end: End,
}

/// What `cablecontinue` draws (design 010 §2.2, §5).
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// The cable's existing reference (on the source page), which this end will name.
    pub source: RepresentationId,
    pub source_page: PageId,
    pub stubs: Vec<Stub>,
    pub skipped: Vec<(WireId, Skip)>,
    /// The slash across the stubs.
    pub slash: (Pt, Pt),
}

fn in_range(p: Pt) -> bool {
    p.is_finite() && p.x.abs() <= crate::words::MAX_COORD && p.y.abs() <= crate::words::MAX_COORD
}

/// Plan the continuation of `cable` on `page` from `at` toward `toward` (design 010 §5): which
/// members continue (`members` restricts and orders them), their stubs, and the slash. Pure: reads
/// the document only. Bounded by the cable's members ([`crate::slash::MAX_CABLE_MEMBERS`]).
///
/// # Errors
/// The refusals of design 010 §4.4 and §7.2 in words.
pub fn plan_continue(
    doc: &Document, page: PageId, cable: CableId, members: Option<&[WireId]>, at: Pt, toward: Pt, tol: f64,
) -> Result<Plan, CmdError> {
    let tol = if tol.is_finite() && tol >= 0.0 { tol } else { 0.0 };
    if !(in_range(at) && in_range(toward)) {
        return Err(CmdError::refused(crate::words::OUT_OF_RANGE));
    }
    if at.near(toward, tol) {
        return Err(CmdError::refused(TOWARD_EQUAL));
    }
    let c = doc.cables.get(&cable).ok_or(CmdError::refused(crate::msg::NO_SUCH_CABLE))?;
    // 032 §3.4: cores that end on continuations are continued by a slash there, not here.
    if cores_end_on_continuations(doc, cable, members) {
        return Err(CmdError::refused(cores_on_continuations(doc, cable)));
    }
    let refs = references_of(doc, cable);
    let Some((source, s)) = refs.first().map(|(id, r)| (*id, *r)) else { return Err(CmdError::refused(no_reference(doc, cable))) };
    if refs.len() >= MAX_REFERENCES_PER_CABLE {
        return Err(CmdError::refused(already_continues(doc, cable)));
    }
    if s.page == page {
        return Err(CmdError::refused(drawn_here(doc, cable)));
    }
    let source_at = s.items.first().and_then(|i| doc.items.get(i)).and_then(|i| match &i.kind {
        ecad_model::ItemKind::Primitive(ecad_model::Primitive::Point { at }) if at.is_finite() => Some(*at),
        _ => None,
    });
    let in_cable: BTreeSet<WireId> = c.members.iter().map(|m| m.conductor).collect();
    let candidates: Vec<WireId> = match members {
        None => c.members.iter().map(|m| m.conductor).collect(),
        Some(list) => {
            if list.len() > crate::slash::MAX_CABLE_MEMBERS {
                return Err(CmdError::refused(too_many_members()));
            }
            let mut seen = BTreeSet::new();
            for w in list {
                if !in_cable.contains(w) {
                    return Err(CmdError::refused(format!("conductor {} is not a member of cable {}", w.hex(), cable.hex())));
                }
                if !seen.insert(*w) {
                    return Err(CmdError::refused(format!("conductor {} is listed twice", w.hex())));
                }
            }
            list.to_vec()
        }
    };
    let drawn_on = |w: WireId, pg: PageId| crate::wires::wire_pages(doc, w).contains(&pg);
    if !candidates.is_empty() && candidates.iter().all(|w| drawn_on(*w, page)) {
        return Err(CmdError::refused(drawn_here(doc, cable)));
    }
    let mut going: Vec<(WireId, End)> = Vec::new();
    let mut skipped = Vec::new();
    for w in &candidates {
        if drawn_on(*w, page) {
            skipped.push((*w, Skip::DrawnHere));
            continue;
        }
        let Some(conductor) = doc.wires.get(w) else { continue };
        // The end drawn on the source page is the one already there; the other continues here.
        let on_source = |e: End| crate::wires::drawn_ends_at(doc, conductor.end(e)).iter().any(|(p, _)| *p == s.page);
        let free = match (on_source(End::A), on_source(End::B)) {
            (true, true) => {
                skipped.push((*w, Skip::BothConnected));
                continue;
            }
            (true, false) => End::B,
            (false, true) => End::A,
            (false, false) => {
                let near = |e: End| crate::wires::drawn_ends_at(doc, conductor.end(e)).first().map(|(_, p)| *p);
                match (near(End::A), near(End::B), source_at) {
                    (Some(ra), Some(rb), Some(p)) if ra.dist(p) < rb.dist(p) => End::A,
                    _ => End::B,
                }
            }
        };
        going.push((*w, free));
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
    for (i, (w, free)) in going.iter().enumerate() {
        let off = spacing * (i as f64 - (k - 1.0) / 2.0);
        let s0 = Pt::new(at.x + n.x * off, at.y + n.y * off);
        let o = Pt::new(s0.x + d.x * l, s0.y + d.y * l);
        if !(in_range(s0) && in_range(o)) {
            return Err(CmdError::refused(crate::words::OUT_OF_RANGE));
        }
        let (a, b) = if *free == End::B { (s0, o) } else { (o, s0) };
        stubs.push(Stub { conductor: *w, a, b, free_end: *free });
    }
    let m = Pt::new(at.x + d.x * l / 2.0, at.y + d.y * l / 2.0);
    let h = spacing * (k - 1.0) / 2.0 + spacing / 2.0;
    let half = spacing / 2.0;
    let slash = (Pt::new(m.x - n.x * h - d.x * half, m.y - n.y * h - d.y * half), Pt::new(m.x + n.x * h + d.x * half, m.y + n.y * h + d.y * half));
    if !(in_range(slash.0) && in_range(slash.1)) {
        return Err(CmdError::refused(crate::words::OUT_OF_RANGE));
    }
    Ok(Plan { source, source_page: s.page, stubs, skipped, slash })
}

/// The lines echoed after `cablecontinue` (design 010 §4.3).
pub fn continued_lines(doc: &Document, page: PageId, plan: &Plan, this: RepresentationId) -> Vec<String> {
    let refs = cable_references(doc);
    let here = refs.iter().find(|r| r.reference == this);
    let there = refs.iter().find(|r| r.reference == plan.source);
    let cable = here.map(|r| r.cable).or(there.map(|r| r.cable));
    let title = cable.map(|k| crate::cables::cable_title(doc, k)).unwrap_or_default();
    let mut out = vec![format!(
        "Cable {title} continued on {} with {}; this end shows {} and the end on {} now shows {}.",
        page_number_words(doc, page),
        crate::slash::wires(plan.stubs.len()),
        here.map_or("/?", |r| r.text.as_str()),
        page_number_words(doc, plan.source_page),
        there.map_or("/?", |r| r.text.as_str()),
    )];
    let both: Vec<String> = plan.skipped.iter().filter(|(_, s)| *s == Skip::BothConnected).map(|(w, _)| crate::slash::wire_name(doc, *w)).collect();
    match both.as_slice() {
        [] => {}
        [one] => out.push(format!("1 wire was not continued: {one} is connected at both ends.")),
        many => out.push(format!("{} wires were not continued: {} (connected at both ends).", many.len(), crate::slash::name_list(many))),
    }
    out.push("Connect each wire's free end to its connection point with Connect.".into());
    out
}

/// The line added when a transaction left linked references without their other end (design 010
/// §4.3): `before` are the references that were linked before it.
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

// ---------------------------------------------------------------- per commit (brief 014)

/// Style catalogues no cable reference reads (brief 053): a change of one reaches no reference.
const NO_REFERENCE_STYLES: &[&str] = &["text", "wire", "location"];

/// The cables whose references, or whose drawing at a reference end on any page, `ops` could
/// change; `None` when that could be any cable (op kinds a drawing edit does not produce, such as
/// layer, standard, frame style or settings ops), which means the whole document, as before
/// brief 014. Worked out on the document before the ops.
///
/// # Semantics
/// What a reference says, and how a cable is drawn at its reference ends, depends only on: the
/// cable's reference representations and their points; its members' routes (direction of the
/// text, which pages each member is routed on); its members' end attachments; the cable itself;
/// and the page order and length unit. So a cable is touched when an op changes or removes an item
/// of a representation of the cable or of one of its members, links, changes or unlinks such a
/// representation, or attaches or detaches a member's end. New items, components, terminals,
/// definitions and conductors are not shown or not members yet, so they touch nothing.
pub fn cables_touched(doc: &Document, ops: &[Op]) -> Option<BTreeSet<CableId>> {
    let mut out = BTreeSet::new();
    let mut moved = false;
    let note = |r: &Representation, out: &mut BTreeSet<CableId>| match r.subject {
        Subject::Cable(k) => {
            out.insert(k);
        }
        Subject::Wire(w) => out.extend(doc.cables_of(w)),
        Subject::Device(_) | Subject::Route(_) => {}
    };
    for op in ops {
        // Brief 055 (FP4; review of #110, 1): a frame decides its pages' grids, so a frame choice
        // or a frame's sheet, margins or block height reaches the cables on those pages.
        match crate::title_block::frame_reach(doc, op) {
            crate::title_block::Reach::NotFrame => {}
            crate::title_block::Reach::Unchanged => continue,
            crate::title_block::Reach::Pages(ps) => {
                for p in ps {
                    out.extend(cables_on(doc, p));
                }
                continue;
            }
            crate::title_block::Reach::Wide => return None,
        }
        match op {
            Op::ModifyItem { id, .. } | Op::RemoveItem { id, .. } => {
                if let Some((_, r)) = doc.representation_of_item(*id) {
                    note(r, &mut out);
                }
            }
            Op::AddItem { .. } => {}
            Op::LinkRepresentation { rep, .. } | Op::UnlinkRepresentation { rep, .. } => note(rep, &mut out),
            Op::ModifyRepresentation { before, after, .. } => {
                note(before, &mut out);
                note(after, &mut out);
            }
            Op::CreateWire { id, .. } if !doc.wires.contains_key(id) => {}
            // Brief 032 (a fold deletes the folded wire; numbering writes wire numbers): a
            // conductor's cables only, never every cable.
            Op::DeleteWire { id, .. } | Op::ModifyWire { id, .. } | Op::SetProperty { object: ecad_model::ObjectRef::Wire(id), .. } => {
                out.extend(doc.cables_of(*id))
            }
            Op::CreateDevice { id, .. } if !doc.devices.contains_key(id) => {}
            Op::AddTerminal { id, .. } if !doc.terminals.contains_key(id) => {}
            // Brief 028: a relabelled terminal reaches only the cables of the wires on it.
            Op::AddTerminal { id, .. } | Op::RemoveTerminal { id, .. } => {
                out.extend(doc.ends_at(*id).into_iter().flat_map(|(w, _)| doc.cables_of(w)));
            }
            Op::EmbedDefinition { id, .. } if !doc.definitions.contains_key(id) => {}
            // A device's properties (its tag, brief 018) are drawn beside its symbols only, never
            // in a cable's drawing or reference texts.
            Op::SetProperty { object: ecad_model::ObjectRef::Device(_), .. } => {}
            // Brief 019 (design 019 §9): a page's number or sheet changes what the references on it
            // say about themselves, so their cables (whose other ends show it).
            Op::SetProperty { object: ecad_model::ObjectRef::Page(p), key, .. }
                if key == ecad_model::book::NUMBER_KEY || key == ecad_model::sheet::SHEET_KEY || key == ecad_model::sheet::PAGE_TYPE_KEY =>
            {
                out.extend(cables_on(doc, *p));
            }
            // Brief 054 (design 054 §10.2): a page type's sheet, on its pages only.
            Op::SetStyle { catalogue, .. } if catalogue == crate::styles::page::WORD => {
                if let Some((pages, true)) = crate::styles::page::touched_by(doc, op) {
                    for p in pages {
                        out.extend(cables_on(doc, p));
                    }
                }
            }
            // A page moved between folders, a page added, removed or moved, a folder op: only the
            // pages whose book position changed may read differently (an unnumbered page's
            // number, the order of a cable's partners); a rename in place or an append moves none.
            Op::SetProperty { object: ecad_model::ObjectRef::Page(_), key, .. } if key == ecad_model::book::FOLDER_KEY => {
                moved_cables(doc, ops, &mut moved, &mut out);
            }
            Op::SetProperty { object: ecad_model::ObjectRef::Page(_) | ecad_model::ObjectRef::Project, .. } => {}
            Op::AddPage { .. } | Op::RemovePage { .. } | Op::AddFolder { .. } | Op::RemoveFolder { .. } | Op::ModifyFolder { .. } => {
                moved_cables(doc, ops, &mut moved, &mut out)
            }
            Op::NameVersion { .. } | Op::UnnameVersion { .. } => {}
            // Brief 025 (design 025 §6): a cable op changes that cable's members only, so only its
            // reference ends may be drawn differently (before, every cable).
            Op::CreateCable { id, .. } | Op::DeleteCable { id, .. } => {
                out.insert(*id);
            }
            // Brief 053: a cable's property (its type, tag) is that cable's alone, so only its
            // reference ends may read differently (before, every cable: `spagetype`).
            Op::SetProperty { object: ecad_model::ObjectRef::Cable(k), .. } => {
                out.insert(*k);
            }
            Op::AddCableMember { cable, .. } | Op::RemoveCableMember { cable, .. } => {
                out.insert(*cable);
            }
            Op::SetProjectSettings { before, after }
                if before.length_unit == after.length_unit && before.sheet == after.sheet && before.reference_style == after.reference_style => {}
            // Brief 053: the text, wire and location styles only change how things are drawn;
            // no reference reads them (the derivation reads cables, routes, pages and the sheet),
            // so they reach none (before, every cable: `textstyle` derived every reference twice).
            // Any other catalogue (the frame, one added later) and a standard, which may come to
            // carry sheets, still reach every cable.
            Op::SetStyle { catalogue, .. } if NO_REFERENCE_STYLES.contains(&catalogue.as_str()) => {}
            // Brief 053: a deleted component has no terminals and no representations left (the
            // model's invariants I8 and I10); removing those is its own op above, so the record
            // itself reaches no reference (`toposition` derived every one twice).
            Op::DeleteDevice { .. } => {}
            _ => return None,
        }
    }
    Some(out)
}

/// The cables with a reference on `page` (through the page index).
fn cables_on(doc: &Document, page: PageId) -> impl Iterator<Item = CableId> + '_ {
    doc.representations_on(page).filter(|(_, r)| is_reference(r)).filter_map(|(_, r)| match r.subject {
        Subject::Cable(k) => Some(k),
        _ => None,
    })
}

/// Add the cables with a reference on a page whose book position `ops` change, once per call
/// (`done`).
fn moved_cables(doc: &Document, ops: &[Op], done: &mut bool, out: &mut BTreeSet<CableId>) {
    if std::mem::replace(done, true) {
        return;
    }
    for p in ecad_model::book::positions_changed(doc, ops) {
        out.extend(cables_on(doc, p));
    }
}

/// Per member of a cable: the conductor, the pages it is routed on, and its end attachments.
type MemberState = Vec<(WireId, Vec<PageId>, Option<[TerminalId; 2]>)>;

/// The references of some cables at one moment, and what their reference ends are drawn from, so
/// that one commit derives them once before and once after (brief 014).
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    /// The cables covered; `None` for every cable.
    scope: Option<BTreeSet<CableId>>,
    /// Their references, as [`cable_references`] derives them.
    refs: Vec<CableReference>,
    /// Per covered cable with a reference, its members' state (what brief 009's drawing at a
    /// reference end reads from other pages). Empty for a whole-document snapshot.
    members: BTreeMap<CableId, MemberState>,
}

impl Snapshot {
    /// The references of `scope` (every cable for `None`) in `doc`.
    pub fn take(doc: &Document, scope: Option<&BTreeSet<CableId>>) -> Snapshot {
        let refs = match scope {
            Some(k) => cable_references_of(doc, k),
            None => cable_references(doc),
        };
        let mut members = BTreeMap::new();
        if scope.is_some() {
            let with_refs: BTreeSet<CableId> = refs.iter().map(|r| r.cable).collect();
            for k in with_refs {
                let list = doc.cables.get(&k).map(|c| c.members.as_slice()).unwrap_or_default();
                let state: MemberState = list
                    .iter()
                    .map(|m| {
                        let ends = doc.wires.get(&m.conductor).map(|c| c.ends);
                        (m.conductor, route_pages(doc, m.conductor), ends)
                    })
                    .collect();
                members.insert(k, state);
            }
        }
        Snapshot { scope: scope.cloned(), refs, members }
    }

    /// The covered references that are linked (have another end), as [`linked`] lists them.
    pub fn linked(&self) -> BTreeSet<RepresentationId> {
        self.refs.iter().filter(|r| !r.orphan).map(|r| r.reference).collect()
    }

    /// The line of design 010 §4.3 for references linked in `self` (before a transaction) that
    /// have no other end in `after` (taken with the same scope on `doc`, after it), exactly as
    /// [`orphaned_line`] gives it: references of cables outside the scope did not change.
    pub fn orphaned_line(&self, doc: &Document, after: &Snapshot) -> Option<String> {
        let before = self.linked();
        if before.is_empty() {
            return None;
        }
        orphaned_line_in(doc, &after.refs, &before)
    }

    /// The pages whose reference ends may be drawn differently in `after` than in `self`. `None`
    /// means every page: a whole-document snapshot with any reference before or after (as before
    /// brief 014). Otherwise, the pages of the references of each covered cable whose references
    /// or member state changed.
    pub fn pages_changed(&self, after: &Snapshot) -> Option<BTreeSet<PageId>> {
        let Some(scope) = &self.scope else {
            if self.refs.is_empty() && after.refs.is_empty() {
                return Some(BTreeSet::new());
            }
            return None;
        };
        let mut pages = BTreeSet::new();
        for k in scope {
            fn of(s: &Snapshot, k: CableId) -> Vec<&CableReference> {
                s.refs.iter().filter(|r| r.cable == k).collect()
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

/// Hover lines for a reference end (design 010 §8).
pub fn hover_lines(doc: &Document, reference: RepresentationId) -> Option<Vec<String>> {
    let refs = cable_references(doc);
    let r = refs.iter().find(|r| r.reference == reference)?;
    let title = crate::cables::cable_title(doc, r.cable);
    let place = |x: &CableReference| match (&x.column_label, &x.row_label) {
        (Some(c), Some(row)) => {
            format!("{}, column {c}, row {row}, {}", page_words(doc, x.page), crate::slash::wires(x.wires_here))
        }
        _ => format!("{}, cannot be located, {}", page_words(doc, x.page), crate::slash::wires(x.wires_here)),
    };
    let mut lines =
        vec![format!("Cable reference  {}", r.text), format!("Cable {title}   id {}", reference.hex()), format!("This end: {}", place(r))];
    match r.partner().and_then(|p| refs.iter().find(|x| x.reference == p)) {
        None => lines.push("Other end: none (orphan): continue the cable on another page with Continue Cable".to_string()),
        Some(o) => {
            lines.push(format!("Other end: {}", place(o)));
            // Built once per hover; the lines stop one past the hover's cap, so the caller still
            // says `… and N more`.
            let shown: BTreeSet<ecad_model::TerminalId> =
                ecad_model::derive::terminal_positions(doc, o.page).into_iter().map(|(t, _, _)| t).collect();
            let members = doc.cables.get(&r.cable).map(|c| c.members.as_slice()).unwrap_or_default();
            for m in members.iter().filter(|m| crate::wires::wire_pages(doc, m.conductor).contains(&o.page)) {
                if lines.len() > crate::words::MAX_HOVER_LINES {
                    break;
                }
                let terminal = doc.wires.get(&m.conductor).and_then(|c| terminal_on(doc, &shown, c));
                let to = terminal.unwrap_or_else(|| "(not connected)".into());
                lines.push(format!("  {} -> {to}", crate::slash::wire_name(doc, m.conductor)));
            }
        }
    }
    Some(lines)
}

/// `K2:1`: the terminal among `shown` (those on the other end's page) a conductor attaches to.
fn terminal_on(doc: &Document, shown: &BTreeSet<ecad_model::TerminalId>, c: &ecad_model::Wire) -> Option<String> {
    let t = End::BOTH.into_iter().map(|e| c.end(e)).find(|t| shown.contains(t))?;
    // Brief 044: a connector pin is named by its half (`-X10.S:3`).
    if let Some(name) = crate::connectors::pin_name(doc, t) {
        return Some(name);
    }
    let term = doc.terminals.get(&t)?;
    let comp = doc
        .devices
        .get(&term.component)
        .and_then(|x| x.tag().map(str::to_string))
        .unwrap_or_else(|| format!("{}…", term.component.hex().get(..8).unwrap_or_default()));
    Some(format!("{comp}:{}", crate::terminal_types::terminal_name(doc, t)))
}

/// The debugger's `Cable references` diagnostics (design 010 §8): one line per orphan, one per
/// cable with more references than supported, or `No orphan references.`. Derived, never saved.
pub fn diagnostics(doc: &Document) -> Vec<String> {
    let refs = cable_references(doc);
    let mut out: Vec<String> = refs
        .iter()
        .filter(|r| r.orphan)
        .map(|r| format!("Orphan: cable {} on {} ({})", crate::cables::cable_title(doc, r.cable), page_number_words(doc, r.page), r.text))
        .collect();
    if out.is_empty() {
        out.push("No orphan references.".into());
    }
    // Counted once for every cable (a hostile file may hold many references of many cables).
    let mut counts: BTreeMap<CableId, usize> = BTreeMap::new();
    for r in &refs {
        *counts.entry(r.cable).or_default() += 1;
    }
    for (k, n) in counts.into_iter().filter(|(_, n)| *n > MAX_REFERENCES_PER_CABLE) {
        out.push(format!("{} has {n} references; only two are supported.", cable_words(doc, k)));
    }
    out
}

// ---------------------------------------------------------------- core chains (brief 032 §3)

/// The continuation a conductor end is attached to: (component, name key, its page).
fn continuation_at(doc: &Document, t: TerminalId) -> Option<(ecad_model::DeviceId, String, Option<PageId>)> {
    let c = doc.terminals.get(&t)?.component;
    let key = ecad_model::connectivity::continuation_key(&doc.devices.get(&c)?.properties)?;
    let page = doc.representations_of(Subject::Device(c)).next().map(|(_, r)| r.page);
    Some((c, key, page))
}

/// The members of `cable` attached to continuations, by name key: (member index, continuation,
/// its page). Bounded by the cable's members.
fn continuation_groups(doc: &Document, members: &[WireId]) -> BTreeMap<String, Vec<(usize, ecad_model::DeviceId, Option<PageId>)>> {
    let mut groups: BTreeMap<String, Vec<(usize, ecad_model::DeviceId, Option<PageId>)>> = BTreeMap::new();
    for (i, w) in members.iter().enumerate() {
        let Some(cond) = doc.wires.get(w) else { continue };
        for t in &cond.ends {
            if let Some((c, key, page)) = continuation_at(doc, *t) {
                groups.entry(key).or_default().push((i, c, page));
            }
        }
    }
    groups
}

/// The core chains of `cable` (design 032 §3.2): its members joined end to end through
/// continuation pairs, each chain in order from its first member (the one added first at a free
/// end). Two members are linked when an end of each is attached to a continuation of one name on
/// two different pages and no other member of the cable is attached to that name. Derived, never
/// stored; cost proportional to the cable's members, each visited once (a cycle ends).
pub fn core_chains(doc: &Document, cable: CableId) -> Vec<Vec<WireId>> {
    let Some(c) = doc.cables.get(&cable) else { return Vec::new() };
    let members: Vec<WireId> = c.members.iter().take(crate::slash::MAX_CABLE_MEMBERS).map(|m| m.conductor).collect();
    let n = members.len();
    let mut next: Vec<Vec<usize>> = vec![Vec::new(); n];
    for g in continuation_groups(doc, &members).values() {
        if let [(a, ka, Some(pa)), (b, kb, Some(pb))] = g.as_slice()
            && a != b
            && ka != kb
            && pa != pb
        {
            if let Some(x) = next.get_mut(*a) {
                x.push(*b);
            }
            if let Some(x) = next.get_mut(*b) {
                x.push(*a);
            }
        }
    }
    let mut seen = vec![false; n];
    let mut out = Vec::new();
    for i in 0..n {
        if seen.get(i).copied().unwrap_or(true) {
            continue;
        }
        // The chain's members (each visited once), then its start: the first member in member
        // order with at most one link (a cycle starts at its first member).
        let mut group = vec![i];
        let mut k = 0;
        if let Some(s) = seen.get_mut(i) {
            *s = true;
        }
        while let Some(x) = group.get(k).copied() {
            k += 1;
            for y in next.get(x).into_iter().flatten() {
                if let Some(s) = seen.get_mut(*y)
                    && !*s
                {
                    *s = true;
                    group.push(*y);
                }
            }
        }
        group.sort_unstable();
        let start = group.iter().copied().find(|x| next.get(*x).is_none_or(|l| l.len() <= 1)).unwrap_or(i);
        let mut chain = vec![start];
        let mut walked = BTreeSet::from([start]);
        let mut at = start;
        while let Some(y) = next.get(at).and_then(|l| l.iter().copied().find(|y| !walked.contains(y))) {
            walked.insert(y);
            chain.push(y);
            at = y;
        }
        out.push(chain.into_iter().map(|x| members.get(x).copied()).collect());
        // Members of a branching group (never from links alone) go on as chains of their own.
        for x in group {
            if !walked.contains(&x) {
                out.push(vec![members.get(x).copied()]);
            }
        }
    }
    out.into_iter().map(|c: Vec<Option<WireId>>| c.into_iter().flatten().collect::<Vec<_>>()).filter(|c| !c.is_empty()).collect()
}

/// The cable whose tag is `typed` (normalised as tags are), if any.
pub fn cable_tagged(doc: &Document, typed: &str) -> Option<CableId> {
    let t = ecad_model::tag::normalise(typed, crate::devices::padding(doc)).ok()?;
    doc.cables_tagged(&t).into_iter().next()
}

/// `-W04` in messages: the cable's title.
pub fn title(doc: &Document, cable: CableId) -> String {
    crate::cables::cable_title(doc, cable)
}

/// Most wires a refusal names before `+n`.
const MAX_NAMED: usize = 6;

/// One wire that continues a core on this page (design 032 §3.3).
#[derive(Debug, Clone, PartialEq)]
pub struct Continued {
    pub conductor: WireId,
    /// The chain's core label.
    pub core: Option<String>,
    /// The member it continues.
    pub continues: WireId,
}

/// Plan `cableslash` continuing `cable` on `page` across `crossed` (design 032 §3.3): each crossed
/// wire is joined through a continuation on this page to a core of the cable not drawn here yet,
/// one wire per core.
///
/// # Errors
/// The refusals of §3.3 in words.
pub fn plan_slash_continue(doc: &Document, page: PageId, cable: CableId, crossed: &[WireId]) -> Result<Vec<Continued>, CmdError> {
    let c = doc.cables.get(&cable).ok_or(CmdError::refused(crate::msg::NO_SUCH_CABLE))?;
    let tag = title(doc, cable);
    if doc.representations_of(Subject::Cable(cable)).any(|(_, r)| r.page == page && r.role == "slash") {
        return Err(CmdError::refused(format!("{tag} already has a slash on this page.")));
    }
    if c.members.len().saturating_add(crossed.len()) > crate::slash::MAX_CABLE_MEMBERS {
        return Err(CmdError::refused(crate::slash::too_many()));
    }
    let members: Vec<WireId> = c.members.iter().map(|m| m.conductor).collect();
    let groups = continuation_groups(doc, &members);
    let chains = core_chains(doc, cable);
    let chain_of = |w: WireId| chains.iter().position(|ch| ch.contains(&w));
    let drawn_here = |ch: &Vec<WireId>| ch.iter().any(|w| crate::wires::wire_pages(doc, *w).contains(&page));
    let label_of = |ch: &Vec<WireId>| ch.iter().find_map(|w| c.members.iter().find(|m| m.conductor == *w).and_then(|m| m.core.clone()));
    let mut out: Vec<Continued> = Vec::new();
    let mut used: BTreeMap<usize, WireId> = BTreeMap::new();
    let mut not: Vec<String> = Vec::new();
    for w in crossed {
        let ends = doc.wires.get(w).map(|x| x.ends);
        // The member it continues, and the continuation's name as drawn.
        let found = ends.iter().flatten().find_map(|t| {
            let (k, key, kp) = continuation_at(doc, *t)?;
            if kp != Some(page) {
                return None;
            }
            let name = doc.devices.get(&k).and_then(|x| x.properties.get(ecad_model::connectivity::NAME)).cloned().unwrap_or(key.clone());
            match groups.get(&key).map(Vec::as_slice) {
                Some([(i, other, Some(op))]) if *other != k && *op != page => members.get(*i).copied().map(|m| (m, name)),
                _ => None,
            }
        });
        let chain = found.and_then(|(m, name)| chain_of(m).map(|i| (m, i, name)));
        match chain {
            Some((m, i, name)) if chains.get(i).is_some_and(|ch| !drawn_here(ch)) => {
                if used.insert(i, *w).is_some() {
                    // Fix round (review of #65, tester t13c): plain words, with the way out.
                    let core = chains.get(i).and_then(label_of).map_or_else(|| format!("core at continuation {name}"), |l| format!("core {l}"));
                    return Err(CmdError::refused(format!(
                        "Two of the wires continue {tag}'s {core}. Slash only one wire per core, or give one of the continuations another name."
                    )));
                }
                out.push(Continued { conductor: *w, core: chains.get(i).and_then(label_of), continues: m });
            }
            _ => not.push(format!(
                "Wire {} does not continue a core of {tag}: none of its ends meets {tag} through a continuation.",
                crate::slash::wire_name(doc, *w)
            )),
        }
    }
    if !not.is_empty() {
        let more = not.len().saturating_sub(MAX_NAMED);
        let mut lines: Vec<String> = not.into_iter().take(MAX_NAMED).collect();
        if more > 0 {
            lines.push(format!("+{more}"));
        }
        return Err(CmdError::refused(lines.join("\n")));
    }
    Ok(out)
}

/// 032 §3.4: whether every member of `cable` that `cablecontinue` would take is connected at both
/// ends and one of those ends is a continuation: the cable is continued by slashing its wires
/// where they continue, not by `cablecontinue`.
pub fn cores_end_on_continuations(doc: &Document, cable: CableId, members: Option<&[WireId]>) -> bool {
    let Some(c) = doc.cables.get(&cable) else { return false };
    let all: Vec<WireId> = c.members.iter().map(|m| m.conductor).collect();
    let list = members.unwrap_or(&all);
    !list.is_empty()
        && list.iter().all(|w| doc.wires.contains_key(w))
        && list.iter().any(|w| doc.wires.get(w).is_some_and(|x| x.ends.iter().any(|t| continuation_at(doc, *t).is_some())))
}

/// 032 §3.4: R6 when the cable's cores end on continuations.
pub fn cores_on_continuations(doc: &Document, cable: CableId) -> String {
    let t = title(doc, cable);
    format!("{t}'s cores end on continuations. To show {t} where they continue, slash the wires there with tag {t}.")
}

/// 032 §3.5: what erasing cable slashes adds to a drawing edit. `erased` are the representations
/// the edit unlinks, read on the document before it. Erasing one of a cable's several slashes
/// removes the members routed only on that slash's page; erasing its last slash keeps the cable
/// and its members (design 004 §8).
pub fn slash_erase_followups(doc: &Document, erased: &[(RepresentationId, Representation)]) -> Vec<Op> {
    let gone: BTreeSet<RepresentationId> = erased.iter().map(|(id, _)| *id).collect();
    let mut by_cable: BTreeMap<CableId, BTreeSet<PageId>> = BTreeMap::new();
    for (_, r) in erased {
        if let Subject::Cable(k) = r.subject
            && r.role == "slash"
        {
            by_cable.entry(k).or_default().insert(r.page);
        }
    }
    let mut ops = Vec::new();
    for (k, pages) in by_cable {
        let Some(c) = doc.cables.get(&k) else { continue };
        let kept = doc.representations_of(Subject::Cable(k)).any(|(id, r)| r.role == "slash" && !gone.contains(&id));
        if !kept {
            continue;
        }
        let only_there = |w: WireId| {
            let on = crate::wires::wire_pages(doc, w);
            !on.is_empty() && on.iter().all(|p| pages.contains(p))
        };
        for (index, m) in c.members.iter().enumerate().rev() {
            if only_there(m.conductor) {
                ops.push(Op::RemoveCableMember { cable: k, index, member: m.clone() });
            }
        }
    }
    ops
}
