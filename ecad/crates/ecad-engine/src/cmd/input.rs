//! A session command's typed input (brief 070): a params struct read with
//! [`ecad_electrical::cmd::input::parse`] that runs with the session ([`Command`]).
//!
//! Excludes: reading the parameters ([`ecad_electrical::cmd::input`]) and generic commands
//! ([`crate::cmd::generic`]).

use cadcraft_engine::Session;
use cadcraft_engine::cmd::Run;
use ecad_electrical::cmd::input::{parse, schema};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::cmd::err;

/// A command with a typed input that needs the session.
pub(crate) trait Command: DeserializeOwned + JsonSchema {
    /// Run the command with its input read.
    ///
    /// # Errors
    /// The command's refusal, in words.
    fn run(self, s: &mut Session) -> cadcraft_engine::Result<Value>;
}

/// The run function of command `P`: read the parameters, then run it.
///
/// # Errors
/// Parameters that do not read ([`parse`]), or the command's own refusal.
pub(crate) fn typed<P: Command>(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let p = parse::<P>(params).map_err(|e| err(e.to_string()))?;
    p.run(s)
}

/// A command with a typed input, as registered: its id, its run function and its schema.
#[derive(Clone, Copy)]
pub struct Typed {
    pub id: &'static str,
    pub run: Run,
    pub schema: fn() -> Value,
}

impl Typed {
    /// Session command `id` reading its parameters as `P`.
    pub(crate) const fn of<P: Command>(id: &'static str) -> Typed {
        Typed { id, run: typed::<P>, schema: schema::<P> }
    }
}
