//! The electrical work of ECAD's commands (brief 070): typed input in, ops and a plain result (or a
//! [`crate::CmdError`]) out, and the JSON answers built from the document (`*_json`). No engine
//! and no locking. A generic command is a params struct here that implements [`run::Generic`];
//! the engine's one runner reads it ([`input`]), takes the project, runs it and commits its edit.
//! Commands that need the session keep a shim in `ecad_engine::commands`.

pub mod input;
pub mod run;
