//! The rules of `connect` and `disconnect` (brief 004): attach a wire's end to a connection point
//! (moving the route's end onto it), or detach it. Each returns the ops to commit.
//!
//! Excludes: the interactive forms and the JSON.

use ecad_model::{Document, End, Item, ItemId, ItemKind, Op, PageId, Pt, TerminalId, WireId};

use crate::error::CmdError;

/// No conductor with the ID given.
const NO_CONDUCTOR: &str = "no such conductor";
/// The route is not a path.
const NOT_A_PATH: &str = "route is not a path";

/// The route of `c` on `page`, its item, and where its `end` lies.
fn route_and_end(doc: &Document, page: PageId, c: WireId, end: End) -> Result<(ItemId, Item, Pt), CmdError> {
    let (id, _) = ecad_model::derive::route_on_page(doc, c, page).ok_or(CmdError::refused("the conductor has no route on this page"))?;
    let item = doc.items.get(&id).cloned().ok_or(CmdError::refused("route missing"))?;
    let ItemKind::Primitive(prim) = &item.kind else { return Err(CmdError::refused(NOT_A_PATH)) };
    let (a, b) = prim.path_ends().ok_or(CmdError::refused(NOT_A_PATH))?;
    Ok((id, item, if end == End::A { a } else { b }))
}

/// The ops attaching `end` of wire `c` to connection point `t` shown on `page`: the route's end
/// moved onto the point when it is elsewhere, then the attachment.
///
/// # Errors
/// No such conductor or connection point; the point is not shown on `page`; the wire has no
/// route there.
pub fn connect(doc: &Document, page: PageId, c: WireId, end: End, t: TerminalId) -> Result<Vec<Op>, CmdError> {
    let conductor = doc.wires.get(&c).ok_or(CmdError::refused(NO_CONDUCTOR))?;
    if !doc.terminals.contains_key(&t) {
        return Err(CmdError::refused("no such connection point"));
    }
    let at = ecad_model::derive::terminal_positions(doc, page)
        .into_iter()
        .find(|(x, _, _)| *x == t)
        .map(|(_, _, p)| p)
        .ok_or(CmdError::refused("the connection point is not shown on this page"))?;
    let (route, item, current) = route_and_end(doc, page, c, end)?;
    let mut ops = Vec::new();
    if !current.near(at, 1e-12) {
        let ItemKind::Primitive(prim) = &item.kind else { return Err(CmdError::refused(NOT_A_PATH)) };
        let moved = prim.with_path_end(end, at).ok_or(CmdError::refused(NOT_A_PATH))?;
        ops.push(Op::ModifyItem { id: route, before: item.clone(), after: Item { kind: ItemKind::Primitive(moved), ..item.clone() } });
    }
    ops.push(Op::AttachEnd { conductor: c, end, before: conductor.end(end), after: Some(t) });
    Ok(ops)
}

/// The op detaching `end` of wire `c`.
///
/// # Errors
/// No such conductor; that end is not connected.
pub fn disconnect(doc: &Document, c: WireId, end: End) -> Result<Op, CmdError> {
    let before = doc.wires.get(&c).ok_or(CmdError::refused(NO_CONDUCTOR))?.end(end);
    if before.is_none() {
        return Err(CmdError::refused("that end is not connected"));
    }
    Ok(Op::AttachEnd { conductor: c, end, before, after: None })
}
