//! The rules of brief 010's cable reference commands (design 010 §2, §7): `cableref` (a reference
//! end), `cablecontinue` (the cable's second drawing) and `cablegoto` (the other end). Each reads
//! the document and returns the ops to commit and what to report; the derivations themselves are
//! [`crate::references`].
//!
//! Excludes: opening pages and moving the view (`cablegoto`'s engine half), the interactive forms
//! and the JSON.

use std::collections::BTreeMap;

use ecad_model::{
    CableId, Document, End, Form, IdAllocator, Item, ItemId, ItemKind, Op, PageId, Primitive, Pt, Representation, RepresentationId, RouteEnd,
    RouteId, Style, Subject, WireId,
};

use crate::error::CmdError;
use crate::references::{self, Plan, REFERENCE_ROLE};

/// A new reference end: its Point item and its representation.
#[derive(Debug, Clone, PartialEq)]
pub struct Referenced {
    pub item: ItemId,
    pub rep: RepresentationId,
    pub ops: Vec<Op>,
}

/// The reference end's Point at `at` on `page` and its `reference` representation of `cable`.
fn reference_ops(ids: &mut IdAllocator, page: PageId, style: Style, cable: CableId, at: Pt) -> Referenced {
    let (item, rep): (ItemId, RepresentationId) = (ids.fresh(), ids.fresh());
    let ops = vec![
        Op::AddItem { id: item, item: Item { page, style, kind: ItemKind::Primitive(Primitive::Point { at }) } },
        Op::LinkRepresentation {
            id: rep,
            rep: Representation {
                subject: Subject::Cable(cable),
                page,
                form: Form::Drawn,
                role: REFERENCE_ROLE.into(),
                items: vec![item],
                connection_points: BTreeMap::new(),
            },
        },
    ];
    Referenced { item, rep, ops }
}

/// `cableref` (design 010 §2.1, §7.1): the ops of a reference end of `cable` on `page` at `at`.
///
/// # Errors
/// No such cable; the cable already has a reference here, already has two, or has no wire drawn
/// here (R1 to R3).
pub fn reference(doc: &Document, ids: &mut IdAllocator, page: PageId, style: Style, cable: CableId, at: Pt) -> Result<Referenced, CmdError> {
    references::check_reference(doc, page, cable)?;
    Ok(reference_ops(ids, page, style, cable, at))
}

/// One continued member's stub: a route with both ends free (design 072 §4.3), drawn for the
/// wire it continues.
#[derive(Debug, Clone, PartialEq)]
pub struct StubRoute {
    pub conductor: WireId,
    pub route: RouteId,
    pub item: ItemId,
    pub free_end: End,
}

/// What `cablecontinue` adds.
#[derive(Debug, Clone, PartialEq)]
pub struct Continued {
    /// The new reference end.
    pub reference: Referenced,
    pub routes: Vec<StubRoute>,
    pub slash_item: ItemId,
    pub slash_rep: RepresentationId,
    /// What was planned (the skipped members, for the report).
    pub plan: Plan,
    /// Every op, in order: the reference, the stubs, the slash.
    pub ops: Vec<Op>,
}

/// `cablecontinue` (design 010 §2.2, §5, §7.2): the second drawing of `cable` on `page`. A
/// reference end at `at`, then for each continued member (`members` restricts and orders them) a
/// stub, a new route with free ends, then a slash across the stubs. No wire or membership
/// changes, so the nets are unchanged.
///
/// # Errors
/// `members` longer than a cable holds (R9); the refusals of design 010 §4.4 and §7.2.
#[allow(clippy::too_many_arguments)]
pub fn continue_cable(
    doc: &Document, ids: &mut IdAllocator, page: PageId, style: Style, cable: CableId, members: Option<&[WireId]>, at: Pt, toward: Pt, tol: f64,
) -> Result<Continued, CmdError> {
    if members.is_some_and(|m| m.len() > crate::slash::MAX_CABLE_MEMBERS) {
        return Err(CmdError::refused(references::too_many_members()));
    }
    let plan = references::plan_continue(doc, page, cable, members, at, toward, tol)?;
    let reference = reference_ops(ids, page, style.clone(), cable, at);
    let mut ops = reference.ops.clone();
    let mut routes = Vec::with_capacity(plan.stubs.len());
    for stub in &plan.stubs {
        let (r, i, add) = crate::wires::add_route(ids, page, style.clone(), vec![stub.a, stub.b], [RouteEnd::Free, RouteEnd::Free]);
        ops.extend(add);
        routes.push(StubRoute { conductor: stub.conductor, route: r, item: i, free_end: stub.free_end });
    }
    let (slash_item, slash_rep): (ItemId, RepresentationId) = (ids.fresh(), ids.fresh());
    ops.push(Op::AddItem {
        id: slash_item,
        item: Item { page, style, kind: ItemKind::Primitive(Primitive::Line { a: plan.slash.0, b: plan.slash.1 }) },
    });
    ops.push(Op::LinkRepresentation {
        id: slash_rep,
        rep: Representation {
            subject: Subject::Cable(cable),
            page,
            form: Form::Drawn,
            role: "slash".into(),
            items: vec![slash_item],
            connection_points: BTreeMap::new(),
        },
    });
    Ok(Continued { reference, routes, slash_item, slash_rep, plan, ops })
}

/// The other end of a reference, as `cablegoto` goes to it.
#[derive(Debug, Clone, PartialEq)]
pub struct OtherEnd {
    /// Its page.
    pub page: PageId,
    /// Its reference representation.
    pub reference: RepresentationId,
    /// Its Point item.
    pub item: ItemId,
    pub at: Pt,
    /// `Page 3, at the other end of cable -W01 (…).`
    pub message: String,
}

/// `cablegoto` (design 010 §4.5, §7.3): the other end of `reference`.
///
/// # Errors
/// No such reference; it has no other end, or the other end cannot be located.
pub fn other_end(doc: &Document, reference: RepresentationId) -> Result<OtherEnd, CmdError> {
    const NO_OTHER: &str = "this reference has no other end";
    const NOT_LOCATED: &str = "the other end cannot be located";
    let refs = references::all(doc);
    let here = refs.iter().find(|r| r.reference == reference).ok_or(CmdError::refused("no such reference"))?;
    let other = here.partner().ok_or(CmdError::refused(NO_OTHER))?;
    let there = refs.iter().find(|r| r.reference == other).ok_or(CmdError::refused(NO_OTHER))?;
    let item = there.item.ok_or(CmdError::refused(NOT_LOCATED))?;
    let at = there.at.ok_or(CmdError::refused(NOT_LOCATED))?;
    let message = format!(
        "{}, at the other end of cable {} ({}).",
        references::page_words(doc, there.page).replacen("page", "Page", 1),
        crate::cables::cable_title(doc, there.cable),
        there.locator()
    );
    Ok(OtherEnd { page: there.page, reference: other, item, at, message })
}
