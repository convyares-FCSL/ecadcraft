//! `connect` and `disconnect` (brief 004, design 072 §4.3): a route's end onto a connection point,
//! or off it. The
//! interactive forms ([`crate::interact`]) commit through these.
//!
//! Each command is a thin shim (brief 070): its params struct reads the JSON, the rules in
//! [`ecad_electrical::cmd::connect`] decide, and the shim commits the ops and answers in JSON.

use cadcraft_engine::Session;
use ecad_electrical::cmd::connect as rules;
use ecad_model::{End, RouteId, TerminalId};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::cmd::input::{Command, Typed};
use crate::cmd::on_project;

/// The commands of this module with a typed input.
pub const TYPED: &[Typed] = &[Typed::of::<ConnectParams>("connect"), Typed::of::<DisconnectParams>("disconnect")];

/// A route's end.
#[derive(Deserialize, JsonSchema, Clone, Copy)]
pub(crate) enum EndParam {
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

/// `connect {route, end, terminal}`: land the route's end on the connection point, moving it
/// onto the point.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConnectParams {
    /// The route (a drawn wire line).
    route: RouteId,
    /// Which end of it.
    end: EndParam,
    /// The connection point, shown on the active page.
    terminal: TerminalId,
}

impl Command for ConnectParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        on_project(s, |p, pg, _| {
            let ops = rules::connect(p.ledger.document(), self.route, self.end.into(), self.terminal)?;
            p.apply_edit(pg, "connect", "Connect", ops)?;
            Ok(json!({"connected": true}))
        })
    }
}

/// `disconnect {route, end}`: let the route's end go; it ends free where it is.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct DisconnectParams {
    /// The route (a drawn wire line).
    route: RouteId,
    /// Which end of it.
    end: EndParam,
}

impl Command for DisconnectParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        on_project(s, |p, pg, _| {
            let ops = rules::disconnect(p.ledger.document(), self.route, self.end.into())?;
            p.apply_edit(pg, "disconnect", "Disconnect", ops)?;
            Ok(json!({"disconnected": true}))
        })
    }
}
