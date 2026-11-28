//! The electrical work of ECAD's commands: typed input in, ops and a typed reply (or a
//! [`crate::CmdError`]) out. No engine and no locking. Every command has one definition
//! ([`request::Request`]: its id, params and reply). A generic command implements
//! [`run::Generic`] here; the engine's one runner reads its params ([`input`]), takes the project,
//! runs it, commits its edit and makes its reply JSON. Commands that need the session keep a shim
//! in `ecad_engine::commands`.

pub mod input;
pub mod request;
pub mod run;
