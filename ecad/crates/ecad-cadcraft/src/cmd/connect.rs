//! `connect` and `disconnect` (brief 004): a wire's end onto a connection point, or off it. The
//! interactive forms ([`crate::interact`]) commit through these.
//!
//! Each command is a thin shim (brief 070): its params struct reads the JSON, the rules in
//! [`ecad_electrical::cmd::connect`] decide, and the shim commits the ops and answers in JSON.

use cadcraft_engine::Session;
use ecad_electrical::cmd::connect as rules;
use ecad_model::{End, TerminalId, WireId};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::cmd::input::{Command, Typed};
use crate::cmd::on_project;

/// The commands of this module with a typed input.
pub const TYPED: &[Typed] = &[Typed::of::<ConnectParams>("connect"), Typed::of::<DisconnectParams>("disconnect")];

/// A wire's end.
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

/// `connect {conductor, end, terminal}`: attach the wire's end to the connection point, moving
/// the route's end onto it.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConnectParams {
    /// The wire.
    conductor: WireId,
    /// Which end of it.
    end: EndParam,
    /// The connection point, shown on the active page.
    terminal: TerminalId,
}

impl Command for ConnectParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        on_project(s, |p, pg, page| {
            let ops = rules::connect(p.ledger.document(), page, self.conductor, self.end.into(), self.terminal)?;
            p.apply_edit(pg, "connect", "Connect", ops)?;
            Ok(json!({"connected": true}))
        })
    }
}

/// `disconnect {conductor, end}`: detach the wire's end.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct DisconnectParams {
    /// The wire.
    conductor: WireId,
    /// Which end of it.
    end: EndParam,
}

impl Command for DisconnectParams {
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value> {
        on_project(s, |p, pg, _| {
            let op = rules::disconnect(p.ledger.document(), self.conductor, self.end.into())?;
            p.apply_edit(pg, "disconnect", "Disconnect", vec![op])?;
            Ok(json!({"disconnected": true}))
        })
    }
}
