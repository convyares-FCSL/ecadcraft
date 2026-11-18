//! The rules of `connect` and `disconnect` (brief 004, design 072 §4.3): land a route's end on a
//! connection point (moving the route's end onto it), or let it go. Each returns the ops to
//! commit; the commit's rewrite makes the wires follow.
//!
//! Excludes: the interactive forms and the JSON.

use ecad_model::{Document, End, Op, PageId, RouteEnd, RouteId, TerminalId};

use crate::error::CmdError;
use crate::wires::{RouteEdits, end_index};

/// No route with the ID given.
const NO_ROUTE: &str = "no such wire";

/// The ops landing `end` of route `r` on connection point `t` shown on the route's page: the
/// route's end (and its drawing) moved onto the point when it is elsewhere, and the end on it.
///
/// # Errors
/// No such route or connection point; the point is not shown on the route's page.
pub fn connect(doc: &Document, r: RouteId, end: End, t: TerminalId) -> Result<Vec<Op>, CmdError> {
    let page: PageId = doc.routes.get(&r).ok_or(CmdError::refused(NO_ROUTE))?.page;
    if !doc.terminals.contains_key(&t) {
        return Err(CmdError::refused("no such connection point"));
    }
    let at = ecad_model::derive::terminal_position(doc, page, t).ok_or(CmdError::refused("the connection point is not shown on this page"))?;
    let mut edits = RouteEdits::new(doc);
    let route = edits.get(r).ok_or(CmdError::refused(NO_ROUTE))?;
    let slot = match end {
        End::A => route.points.first_mut(),
        End::B => route.points.last_mut(),
    };
    if let Some(p) = slot
        && !p.near(at, 1e-12)
    {
        *p = at;
    }
    if let Some(e) = route.ends.get_mut(end_index(end)) {
        *e = RouteEnd::Point(t);
    }
    Ok(edits.ops())
}

/// The ops letting `end` of route `r` go: it ends free where it is.
///
/// # Errors
/// No such route; that end is not on a connection point.
pub fn disconnect(doc: &Document, r: RouteId, end: End) -> Result<Vec<Op>, CmdError> {
    let route = doc.routes.get(&r).ok_or(CmdError::refused(NO_ROUTE))?;
    if route.ends.get(end_index(end)).and_then(|e| e.point()).is_none() {
        return Err(CmdError::refused("that end is not connected"));
    }
    let mut edits = RouteEdits::new(doc);
    edits.set_end(r, end, RouteEnd::Free);
    Ok(edits.ops())
}
