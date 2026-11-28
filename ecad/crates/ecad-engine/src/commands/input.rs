//! A session command's typed input and reply: a command that runs with the session ([`Command`]),
//! its params read with [`ecad_electrical::commands::input::parse`] and its reply made JSON here, at
//! the edge.
//!
//! [`JsonCommand`] is the older form, a params struct answering with a JSON value; commands still
//! written that way register with [`Typed::json`].
//!
//! Excludes: reading the parameters ([`ecad_electrical::commands::input`]) and generic commands
//! ([`crate::generic`]).

use cadcraft_engine::Session;
use cadcraft_engine::cmd::Run;
use ecad_electrical::CmdError;
use ecad_electrical::commands::input::{parse, schema};
use ecad_electrical::commands::request::Request;
use ecad_electrical::commands::run::{Access, Io};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::commands::err;

/// A command that needs the session: its definition ([`Request`]: its id, params and reply) and its
/// run. Implemented on a type of its own (`pub enum Wire {}`), as a generic command is.
pub(crate) trait Command: Request {
    /// Run the command with its params read.
    ///
    /// # Errors
    /// The command's refusal.
    fn run(params: Self::Params, s: &mut Session) -> Result<Self::Reply, CmdError>;
}

/// The run function of command `C`: read its params, run it, and make its reply JSON.
///
/// # Errors
/// Parameters that do not read ([`parse`]), or the command's own refusal, in words.
pub(crate) fn typed<C: Command>(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let p = parse::<C::Params>(params).map_err(|e| err(e.to_string()))?;
    let reply = C::run(p, s).map_err(|e| err(e.to_string()))?;
    serde_json::to_value(reply).map_err(|e| err(format!("{}: the reply could not be written: {e}", C::ID)))
}

/// The older form of a session command: its params struct, answering with a JSON value.
pub(crate) trait JsonCommand: DeserializeOwned + JsonSchema {
    /// Run the command with its input read.
    ///
    /// # Errors
    /// The command's refusal, in words.
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value>;
}

/// The run function of the older command `P`: read the parameters, then run it.
///
/// # Errors
/// Parameters that do not read ([`parse`]), or the command's own refusal.
pub(crate) fn run_json<P: JsonCommand>(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let p = parse::<P>(params).map_err(|e| err(e.to_string()))?;
    p.run(s)
}

/// A command with a typed input, as registered: its id, its run function, its params' schema and,
/// once its reply is typed, its reply's; for a generic command, what the agent gate reads from its
/// declaration.
#[derive(Clone, Copy)]
pub struct Typed {
    pub id: &'static str,
    pub run: Run,
    pub schema: fn() -> Value,
    /// Its reply's schema (none: it still answers with a JSON value).
    pub reply: Option<fn() -> Value>,
    /// A generic command's access (a session command's is in the gate's own lists).
    pub access: Option<Access>,
    /// The parameters that name a file.
    pub files: Io,
}

impl Typed {
    /// Session command `C`.
    pub(crate) const fn of<C: Command>() -> Typed {
        Typed { id: C::ID, run: typed::<C>, schema: schema::<C::Params>, reply: Some(schema::<C::Reply>), access: None, files: Io::NONE }
    }

    /// The older session command `id` reading its parameters as `P`.
    pub(crate) const fn json<P: JsonCommand>(id: &'static str) -> Typed {
        Typed { id, run: run_json::<P>, schema: schema::<P>, reply: None, access: None, files: Io::NONE }
    }
}
