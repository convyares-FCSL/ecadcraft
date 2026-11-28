//! The rules of `connect` and `disconnect`: land a route's end on a
//! connection point (moving the route's end onto it), or let it go. Each returns the ops to
//! commit; the commit's rewrite makes the wires follow.
//!
//! Excludes: the interactive forms.

use ecad_model::{ConnectionPointId, Document, End, Op, PageId, RouteEnd, RouteId};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::commands::run::{Cx, Generic, Outcome, Reply, Request};
use crate::error::CmdError;
use crate::wires::joining::end_index;
use crate::wires::routes::RouteEdits;

/// No route with the ID given.
const NO_ROUTE: &str = "no such wire";

/// The ops landing `end` of route `r` on connection point `t` shown on the route's page: the
/// route's end (and its drawing) moved onto the point when it is elsewhere, and the end on it.
///
/// # Errors
/// No such route or connection point; the point is not shown on the route's page.
pub fn connect(doc: &Document, r: RouteId, end: End, t: ConnectionPointId) -> Result<Vec<Op>, CmdError> {
    let page: PageId = doc.routes().get(&r).ok_or(CmdError::refused(NO_ROUTE))?.page;
    if !doc.connection_points().contains_key(&t) {
        return Err(CmdError::refused("no such connection point"));
    }
    let at = ecad_model::connectivity::derive::connection_point_position(doc, page, t)
        .ok_or(CmdError::refused("the connection point is not shown on this page"))?;
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
    let route = doc.routes().get(&r).ok_or(CmdError::refused(NO_ROUTE))?;
    if route.ends.get(end_index(end)).and_then(|e| e.point()).is_none() {
        return Err(CmdError::refused("that end is not connected"));
    }
    let mut edits = RouteEdits::new(doc);
    edits.set_end(r, end, RouteEnd::Free);
    Ok(edits.ops())
}

// ---------------------------------------------------------------------------------------------
// The commands (generic: crate::commands::run)

/// A route's end.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, JsonSchema)]
pub enum EndParam {
    /// The start.
    #[serde(rename = "A", alias = "a")]
    A,
    /// The other end.
    #[serde(rename = "B", alias = "b")]
    B,
}

impl From<EndParam> for End {
    fn from(e: EndParam) -> Self {
        match e {
            EndParam::A => End::A,
            EndParam::B => End::B,
        }
    }
}

impl From<End> for EndParam {
    fn from(e: End) -> Self {
        match e {
            End::A => EndParam::A,
            End::B => EndParam::B,
        }
    }
}

/// Lands a route's end on a connection point, moving the end onto it.
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConnectParams {
    /// The route (a drawn wire line).
    pub route: RouteId,
    /// Which end of it.
    pub end: EndParam,
    /// The connection point, shown on the active page.
    pub connection_point: ConnectionPointId,
}

/// What `connect` did.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub struct ConnectReply {
    /// Always true: the end is on the connection point.
    pub connected: bool,
}
impl Reply for ConnectReply {}

/// `connect`: land a route's end on a connection point.
pub enum Connect {}
impl Request for Connect {
    const ID: &'static str = "connect";
    type Params = ConnectParams;
    type Reply = ConnectReply;
}

impl Generic for Connect {
    const TITLE: &'static str = "Connect";

    fn run(p: ConnectParams, cx: &mut Cx<'_>) -> Result<Outcome<ConnectReply>, CmdError> {
        let ops = connect(cx.doc, p.route, p.end.into(), p.connection_point)?;
        Ok(Outcome::edit("Connect", ops, ConnectReply { connected: true }))
    }
}

/// Lets a route's end go; it ends free where it is.
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DisconnectParams {
    /// The route (a drawn wire line).
    pub route: RouteId,
    /// Which end of it.
    pub end: EndParam,
}

/// What `disconnect` did.
#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub struct DisconnectReply {
    /// Always true: the end is free.
    pub disconnected: bool,
}
impl Reply for DisconnectReply {}

/// `disconnect`: let a route's end go.
pub enum Disconnect {}
impl Request for Disconnect {
    const ID: &'static str = "disconnect";
    type Params = DisconnectParams;
    type Reply = DisconnectReply;
}

impl Generic for Disconnect {
    const TITLE: &'static str = "Disconnect";

    fn run(p: DisconnectParams, cx: &mut Cx<'_>) -> Result<Outcome<DisconnectReply>, CmdError> {
        let ops = disconnect(cx.doc, p.route, p.end.into())?;
        Ok(Outcome::edit("Disconnect", ops, DisconnectReply { disconnected: true }))
    }
}
