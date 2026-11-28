//! One command's definition, shared by the engine, the app and MCP: its id, its params type and its
//! reply type ([`Request`]), as in an RPC protocol. JSON is only how they travel: the engine makes a
//! reply JSON at its edge (the generic runner, the session command's run, MCP and the agent bridge),
//! and the app's `call` reads it back as the type.
//!
//! Excludes: running a command ([`crate::commands::run::Generic`], the engine's session commands).

use schemars::JsonSchema;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// A command's definition: its id, what it takes and what it answers. Implemented on a type that
/// names the command (`pub enum Net {}`), by both kinds of command: a generic command
/// ([`crate::commands::run::Generic`]) and a session command (the engine's).
///
/// # Semantics
/// The params type is read from the JSON with the strict reader ([`crate::commands::input::parse`]);
/// its doc comments are the command's help. The reply type is what the command answers; its doc
/// comments describe the reply in MCP's description of the command.
pub trait Request: 'static {
    /// The command's id (`execute`'s name, the command line's word).
    const ID: &'static str;
    /// What it takes.
    type Params: Serialize + DeserializeOwned + JsonSchema;
    /// What it answers.
    type Reply: Reply;
}

/// What a command answers: a documented struct or enum. Implemented for each reply type by name,
/// never for a JSON value, so a command's answer always has a shape its schema states.
///
/// # Example
/// A JSON value is not a reply:
/// ```compile_fail
/// fn reply<R: ecad_electrical::commands::request::Reply>() {}
/// reply::<serde_json::Value>();
/// ```
pub trait Reply: Serialize + DeserializeOwned + JsonSchema + 'static {}
