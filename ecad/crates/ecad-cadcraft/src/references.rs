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
//! forms (`crate::commands`).

use std::collections::{BTreeMap, BTreeSet};

use ecad_model::derive::{CableReference, cable_references, cable_references_of, route_ends_by_page};
use ecad_model::{
    CableId, ConductorId, Document, End, ItemId, LengthUnit, Op, PageId, Pt, Representation, RepresentationId, Subject,
    TerminalId,
};

pub use ecad_model::derive::{MAX_REFERENCES_PER_CABLE, REFERENCE_ROLE};

/// `ecad.reference`: ACI 6, magenta (the colour of `ECAD-CABLES`, so a linked text is ByLayer).
pub const REFERENCE_ACI: u8 = 6;
/// `ecad.orphan`: ACI 1, red.
pub const ORPHAN_ACI: u8 = 1;
/// `ecad.ref-text-height` in millimetres.
pub const TEXT_HEIGHT_MM: f64 = 2.5;
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
    Sizes { text_height: TEXT_HEIGHT_MM * k, spacing: STUB_SPACING_MM * k }
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
    cable_references(doc).into_iter().filter(|r| r.page == page).collect()
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
    markers_from(doc, cable_references(doc), page)
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
    cable_references_of(doc, &BTreeSet::from([cable]))
        .into_iter()
        .find(|r| r.reference == reference)
        .map(|r| (r.text, r.orphan, r.text_dir))
}

// The marker of a reference end (flag and text) is drawn by brief 009's `look.rs` only, with the
// text from [`marker_text`]; its text is mapped `Mapped::RefText`, so hover, Go to Reference and
// erasing it act on the reference. The ends of member routes that leave for another page (design
// 010 §6.4) show no loose-end square: `look.rs` folds that rule into its `quiet` ends.

/// The reference end on `page` under `p`: the nearest reference point within `radius`, else a
/// reference whose drawn text (its box, grown by `radius`) holds `p`. Used before the generic pick
/// by Go to Reference and the hover (design 010 §4.5, §8), because a continued end's middle stub
/// starts exactly on its reference point and would otherwise win the pick.
pub fn reference_at(doc: &Document, page: PageId, p: Pt, radius: f64) -> Option<RepresentationId> {
    if !(p.is_finite() && radius.is_finite() && radius >= 0.0) {
        return None;
    }
    let markers = markers_on_page(doc, page);
    let near = markers
        .iter()
        .map(|m| (m.at.dist(p), m.reference))
        .filter(|(d, _)| *d <= radius)
        .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    if let Some((_, r)) = near {
        return Some(r);
    }
    markers.iter().find(|m| text_box_holds(m, p, radius)).map(|m| m.reference)
}

/// Whether `p` lies in a marker's box (brief 009's flag) grown by `pad`.
fn text_box_holds(m: &Marker, p: Pt, pad: f64) -> bool {
    // Brief 009 draws the marker: its flag around the text, at the reference point, on the side
    // away from the wires.
    let (lo, hi) = crate::look::marker_box(m.at, &m.text, crate::look::FlagSide::away(m.text_dir));
    p.x >= lo.x - pad && p.x <= hi.x + pad && p.y >= lo.y - pad && p.y <= hi.y + pad
}

/// The pages (in page order) where `conductor` has a route.
pub fn route_pages(doc: &Document, conductor: ConductorId) -> Vec<PageId> {
    let on: BTreeSet<PageId> = doc
        .representations_of(Subject::Conductor(conductor))
        .filter(|(_, r)| !r.items.is_empty())
        .map(|(_, r)| r.page)
        .collect();
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
    format!("Cable {}", crate::views::cable_title(doc, cable))
}

// ---------------------------------------------------------------- refusals (design 010 §4.4)

/// R1.
pub fn already_here(doc: &Document, cable: CableId) -> String {
    format!("{} already has a reference on this page.", cable_words(doc, cable))
}
/// R2.
pub fn already_continues(doc: &Document, cable: CableId) -> String {
    format!(
        "{} already continues to another page; a cable can have only two references in this version.",
        cable_words(doc, cable)
    )
}
/// R3.
pub fn no_wires_here(doc: &Document, cable: CableId) -> String {
    format!("{} has no wires drawn on this page; draw or continue it here first.", cable_words(doc, cable))
}
/// R4.
pub fn no_reference(doc: &Document, cable: CableId) -> String {
    format!(
        "{} has no reference to continue from; place one with Cable Reference on its page first.",
        cable_words(doc, cable)
    )
}
/// R5.
pub fn drawn_here(doc: &Document, cable: CableId) -> String {
    format!("{} is already drawn on this page; continue it on another page.", cable_words(doc, cable))
}
/// R6.
pub fn none_can_continue(doc: &Document, cable: CableId) -> String {
    format!(
        "None of cable {}'s wires can continue: each is connected at both ends or already drawn on this page.",
        crate::views::cable_title(doc, cable)
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
pub fn check_reference(doc: &Document, page: PageId, cable: CableId) -> Result<(), String> {
    let c = doc.cables.get(&cable).ok_or("no such cable")?;
    let refs = references_of(doc, cable);
    if refs.iter().any(|(_, r)| r.page == page) {
        return Err(already_here(doc, cable));
    }
    if refs.len() >= MAX_REFERENCES_PER_CABLE {
        return Err(already_continues(doc, cable));
    }
    let routes = route_ends_by_page(doc);
    if !c.members.iter().any(|m| routes.contains_key(&(m.conductor, page))) {
        return Err(no_wires_here(doc, cable));
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
    pub conductor: ConductorId,
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
    pub skipped: Vec<(ConductorId, Skip)>,
    /// The slash across the stubs.
    pub slash: (Pt, Pt),
}

fn in_range(p: Pt) -> bool {
    p.is_finite() && p.x.abs() <= crate::interact::MAX_COORD && p.y.abs() <= crate::interact::MAX_COORD
}

/// Plan the continuation of `cable` on `page` from `at` toward `toward` (design 010 §5): which
/// members continue (`members` restricts and orders them), their stubs, and the slash. Pure: reads
/// the document only. Bounded by the cable's members ([`crate::slash::MAX_CABLE_MEMBERS`]).
///
/// # Errors
/// The refusals of design 010 §4.4 and §7.2 in words.
pub fn plan_continue(
    doc: &Document,
    page: PageId,
    cable: CableId,
    members: Option<&[ConductorId]>,
    at: Pt,
    toward: Pt,
    tol: f64,
) -> Result<Plan, String> {
    let tol = if tol.is_finite() && tol >= 0.0 { tol } else { 0.0 };
    if !(in_range(at) && in_range(toward)) {
        return Err(crate::interact::OUT_OF_RANGE.into());
    }
    if at.near(toward, tol) {
        return Err(TOWARD_EQUAL.into());
    }
    let c = doc.cables.get(&cable).ok_or("no such cable")?;
    let refs = references_of(doc, cable);
    let Some((source, s)) = refs.first().map(|(id, r)| (*id, *r)) else { return Err(no_reference(doc, cable)) };
    if refs.len() >= MAX_REFERENCES_PER_CABLE {
        return Err(already_continues(doc, cable));
    }
    if s.page == page {
        return Err(drawn_here(doc, cable));
    }
    let source_at = s.items.first().and_then(|i| doc.items.get(i)).and_then(|i| match &i.kind {
        ecad_model::ItemKind::Primitive(ecad_model::Primitive::Point { at }) if at.is_finite() => Some(*at),
        _ => None,
    });
    let in_cable: BTreeSet<ConductorId> = c.members.iter().map(|m| m.conductor).collect();
    let candidates: Vec<ConductorId> = match members {
        None => c.members.iter().map(|m| m.conductor).collect(),
        Some(list) => {
            if list.len() > crate::slash::MAX_CABLE_MEMBERS {
                return Err(too_many_members());
            }
            let mut seen = BTreeSet::new();
            for w in list {
                if !in_cable.contains(w) {
                    return Err(format!("conductor {} is not a member of cable {}", w.hex(), cable.hex()));
                }
                if !seen.insert(*w) {
                    return Err(format!("conductor {} is listed twice", w.hex()));
                }
            }
            list.to_vec()
        }
    };
    let routes = route_ends_by_page(doc);
    if !candidates.is_empty() && candidates.iter().all(|w| routes.contains_key(&(*w, page))) {
        return Err(drawn_here(doc, cable));
    }
    let mut going: Vec<(ConductorId, End)> = Vec::new();
    let mut skipped = Vec::new();
    for w in &candidates {
        if routes.contains_key(&(*w, page)) {
            skipped.push((*w, Skip::DrawnHere));
            continue;
        }
        let Some(conductor) = doc.conductors.get(w) else { continue };
        let free = match (conductor.end(End::A), conductor.end(End::B)) {
            (Some(_), Some(_)) => {
                skipped.push((*w, Skip::BothConnected));
                continue;
            }
            (Some(_), None) => End::B,
            (None, Some(_)) => End::A,
            (None, None) => match (routes.get(&(*w, s.page)), source_at) {
                (Some((ra, rb)), Some(p)) if ra.dist(p) < rb.dist(p) => End::A,
                _ => End::B,
            },
        };
        going.push((*w, free));
    }
    if going.is_empty() {
        return Err(none_can_continue(doc, cable));
    }
    let spacing = sizes(doc).spacing;
    let (dx, dy) = (toward.x - at.x, toward.y - at.y);
    let l = dx.hypot(dy);
    if !(l.is_finite() && l > tol) {
        return Err(TOWARD_EQUAL.into());
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
            return Err(crate::interact::OUT_OF_RANGE.into());
        }
        let (a, b) = if *free == End::B { (s0, o) } else { (o, s0) };
        stubs.push(Stub { conductor: *w, a, b, free_end: *free });
    }
    let m = Pt::new(at.x + d.x * l / 2.0, at.y + d.y * l / 2.0);
    let h = spacing * (k - 1.0) / 2.0 + spacing / 2.0;
    let half = spacing / 2.0;
    let slash = (
        Pt::new(m.x - n.x * h - d.x * half, m.y - n.y * h - d.y * half),
        Pt::new(m.x + n.x * h + d.x * half, m.y + n.y * h + d.y * half),
    );
    if !(in_range(slash.0) && in_range(slash.1)) {
        return Err(crate::interact::OUT_OF_RANGE.into());
    }
    Ok(Plan { source, source_page: s.page, stubs, skipped, slash })
}

/// The lines echoed after `cablecontinue` (design 010 §4.3).
pub fn continued_lines(doc: &Document, page: PageId, plan: &Plan, this: RepresentationId) -> Vec<String> {
    let refs = cable_references(doc);
    let here = refs.iter().find(|r| r.reference == this);
    let there = refs.iter().find(|r| r.reference == plan.source);
    let cable = here.map(|r| r.cable).or(there.map(|r| r.cable));
    let title = cable.map(|k| crate::views::cable_title(doc, k)).unwrap_or_default();
    let mut out = vec![format!(
        "Cable {title} continued on {} with {}; this end shows {} and the end on {} now shows {}.",
        page_number_words(doc, page),
        crate::slash::wires(plan.stubs.len()),
        here.map_or("/?", |r| r.text.as_str()),
        page_number_words(doc, plan.source_page),
        there.map_or("/?", |r| r.text.as_str()),
    )];
    let both: Vec<String> = plan
        .skipped
        .iter()
        .filter(|(_, s)| *s == Skip::BothConnected)
        .map(|(w, _)| crate::slash::wire_name(doc, *w))
        .collect();
    match both.as_slice() {
        [] => {}
        [one] => out.push(format!("1 wire was not continued: {one} is connected at both ends.")),
        many => out.push(format!(
            "{} wires were not continued: {} (connected at both ends).",
            many.len(),
            crate::slash::name_list(many)
        )),
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
            crate::views::cable_title(doc, one.cable),
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

/// The cables whose references, or whose drawing at a reference end on any page, `ops` could
/// change; `None` when that could be any cable (op kinds a drawing edit does not produce, such as
/// page, layer, cable, property or settings ops), which means the whole document, as before
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
        Subject::Conductor(w) => out.extend(doc.cables_of(w)),
        Subject::Component(_) => {}
    };
    for op in ops {
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
            Op::AttachEnd { conductor, .. } => out.extend(doc.cables_of(*conductor)),
            Op::CreateConductor { id, .. } if !doc.conductors.contains_key(id) => {}
            Op::CreateComponent { id, .. } if !doc.components.contains_key(id) => {}
            Op::AddTerminal { id, .. } if !doc.terminals.contains_key(id) => {}
            Op::EmbedDefinition { id, .. } if !doc.definitions.contains_key(id) => {}
            // A device's properties (its tag, brief 018) are drawn beside its symbols only, never
            // in a cable's drawing or reference texts.
            Op::SetProperty { object: ecad_model::ObjectRef::Component(_), .. } => {}
            // Brief 019 (design 019 §9): a page's number or sheet changes what the references on it
            // say about themselves, so their cables (whose other ends show it).
            Op::SetProperty { object: ecad_model::ObjectRef::Page(p), key, .. }
                if key == ecad_model::book::NUMBER_KEY || key == ecad_model::sheet::SHEET_KEY =>
            {
                out.extend(cables_on(doc, *p));
            }
            // A page moved between folders, a page added, removed or moved, a folder op: only the
            // pages whose book position changed may read differently (an unnumbered page's
            // number, the order of a cable's partners); a rename in place or an append moves none.
            Op::SetProperty { object: ecad_model::ObjectRef::Page(_), key, .. }
                if key == ecad_model::book::FOLDER_KEY =>
            {
                moved_cables(doc, ops, &mut moved, &mut out);
            }
            Op::SetProperty { object: ecad_model::ObjectRef::Page(_) | ecad_model::ObjectRef::Project, .. } => {}
            Op::AddPage { .. }
            | Op::RemovePage { .. }
            | Op::AddFolder { .. }
            | Op::RemoveFolder { .. }
            | Op::ModifyFolder { .. } => moved_cables(doc, ops, &mut moved, &mut out),
            Op::NameVersion { .. } | Op::UnnameVersion { .. } => {}
            Op::SetProjectSettings { before, after }
                if before.length_unit == after.length_unit
                    && before.sheet == after.sheet
                    && before.reference_style == after.reference_style => {}
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
type MemberState = Vec<(ConductorId, Vec<PageId>, [Option<TerminalId>; 2])>;

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
                        let ends = doc.conductors.get(&m.conductor).map(|c| c.ends).unwrap_or([None, None]);
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
    let title = crate::views::cable_title(doc, r.cable);
    let place = |x: &CableReference| match (&x.column_label, &x.row_label) {
        (Some(c), Some(row)) => {
            format!("{}, column {c}, row {row}, {}", page_words(doc, x.page), crate::slash::wires(x.wires_here))
        }
        _ => format!("{}, cannot be located, {}", page_words(doc, x.page), crate::slash::wires(x.wires_here)),
    };
    let mut lines = vec![
        format!("Cable reference  {}", r.text),
        format!("Cable {title}   id {}", reference.hex()),
        format!("This end: {}", place(r)),
    ];
    match r.partner().and_then(|p| refs.iter().find(|x| x.reference == p)) {
        None => {
            lines.push("Other end: none (orphan): continue the cable on another page with Continue Cable".to_string())
        }
        Some(o) => {
            lines.push(format!("Other end: {}", place(o)));
            // Built once per hover; the lines stop one past the hover's cap, so the caller still
            // says `… and N more`.
            let routes = route_ends_by_page(doc);
            let shown: BTreeSet<ecad_model::TerminalId> =
                ecad_model::derive::terminal_positions(doc, o.page).into_iter().map(|(t, _, _)| t).collect();
            let members = doc.cables.get(&r.cable).map(|c| c.members.as_slice()).unwrap_or_default();
            for m in members.iter().filter(|m| routes.contains_key(&(m.conductor, o.page))) {
                if lines.len() > crate::interact::MAX_HOVER_LINES {
                    break;
                }
                let terminal = doc.conductors.get(&m.conductor).and_then(|c| terminal_on(doc, &shown, c));
                let to = terminal.unwrap_or_else(|| "(not connected)".into());
                lines.push(format!("  {} -> {to}", crate::slash::wire_name(doc, m.conductor)));
            }
        }
    }
    Some(lines)
}

/// `K2:1`: the terminal among `shown` (those on the other end's page) a conductor attaches to.
fn terminal_on(doc: &Document, shown: &BTreeSet<ecad_model::TerminalId>, c: &ecad_model::Conductor) -> Option<String> {
    let t = End::BOTH.into_iter().filter_map(|e| c.end(e)).find(|t| shown.contains(t))?;
    let term = doc.terminals.get(&t)?;
    let comp = doc
        .components
        .get(&term.component)
        .and_then(|x| x.properties.get("tag").cloned())
        .unwrap_or_else(|| format!("{}…", term.component.hex().get(..8).unwrap_or_default()));
    Some(format!("{comp}:{}", crate::interact::terminal_name(doc, t)))
}

/// The debugger's `Cable references` diagnostics (design 010 §8): one line per orphan, one per
/// cable with more references than supported, or `No orphan references.`. Derived, never saved.
pub fn diagnostics(doc: &Document) -> Vec<String> {
    let refs = cable_references(doc);
    let mut out: Vec<String> = refs
        .iter()
        .filter(|r| r.orphan)
        .map(|r| {
            format!(
                "Orphan: cable {} on {} ({})",
                crate::views::cable_title(doc, r.cable),
                page_number_words(doc, r.page),
                r.text
            )
        })
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
