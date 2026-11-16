//! The electrical work of ECAD's commands (brief 070): typed input in, ops and a plain result (or a
//! [`crate::CmdError`]) out. No engine, no JSON and no locking: the engine crate's shims in
//! `ecad_cadcraft::cmd` read the parameters, take the project, call these functions, commit the
//! ops and answer in JSON.

pub mod designation;
pub mod output;
pub mod sheet;
pub mod style;
pub mod symbol;
pub mod text;
pub mod wire;
