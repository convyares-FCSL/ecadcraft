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
    RouteId, Style, Subject, TerminalId, WireId,
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

/// One continued core's stub: a route from the core's named point on the new reference to a free
/// end (design 072 §4.4, F4).
#[derive(Debug, Clone, PartialEq)]
pub struct StubRoute {
    /// The core's index in the cable.
    pub core: usize,
    /// The core's named points: on the source page's reference, and on the new one.
    pub points: [TerminalId; 2],
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

/// `cablecontinue` (design 010 §2.2, §5, §7.2; design 072 §4.4, F4): the second drawing of
/// `cable` on `page`. A reference end at `at`; then, for each continued core (`members`, its wire
/// or its loose line, restricts and orders them), one named point on each of the cable's two
/// references: the source page's loose line lands on the one there, and a stub, a new route from
/// the one here to a free end, is drawn; then a slash across the stubs. Cross-page joins go only
/// through these points: once the stub lands on a device point, the rewrite makes one wire from
/// device point to device point across the pages.
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
    let source = doc.representations.get(&plan.source).ok_or(CmdError::refused(references::no_reference(doc, cable)))?;
    let c = doc.cables.get(&cable).ok_or(CmdError::refused(crate::msg::NO_SUCH_CABLE))?;
    let mut reference = reference_ops(ids, page, style.clone(), cable, at);
    let mut source_after = source.clone();
    let mut cable_after = c.clone();
    let mut lines = Vec::new();
    let mut routes = Vec::with_capacity(plan.stubs.len());
    let mut stub_ops = Vec::new();
    for stub in &plan.stubs {
        let points: [TerminalId; 2] = [ids.fresh(), ids.fresh()];
        source_after.connection_points.insert(points[0], stub.line_at);
        if let Some(Op::LinkRepresentation { rep, .. }) = reference.ops.get_mut(1) {
            rep.connection_points.insert(points[1], stub.a);
        }
        if let Some(core) = cable_after.cores.get_mut(stub.core) {
            core.through = points.to_vec();
        }
        if let Some(line) = doc.routes.get(&stub.line) {
            let mut after = line.clone();
            if let Some(e) = after.ends.get_mut(stub.line_end) {
                *e = RouteEnd::Point(points[0]);
            }
            lines.push(Op::ModifyRoute { id: stub.line, before: line.clone(), after });
        }
        let (r, i, add) = crate::wires::add_route(ids, page, style.clone(), vec![stub.a, stub.b], [RouteEnd::Point(points[1]), RouteEnd::Free]);
        stub_ops.extend(add);
        routes.push(StubRoute { core: stub.core, points, route: r, item: i, free_end: stub.free_end });
    }
    let mut ops = reference.ops.clone();
    ops.extend(Op::modify_cable(cable, c, cable_after));
    ops.push(Op::ModifyRepresentation { id: plan.source, before: source.clone(), after: source_after });
    ops.extend(lines);
    ops.extend(stub_ops);
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
