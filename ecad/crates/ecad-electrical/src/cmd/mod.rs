//! The electrical work of ECAD's commands (brief 070): typed input in, ops and a plain result (or a
//! [`crate::CmdError`]) out. No engine, no JSON and no locking: the engine crate's shims in
//! `ecad_cadcraft::cmd` read the parameters, take the project, call these functions, commit the
//! ops and answer in JSON.

pub mod accessory;
pub mod bom;
pub mod cable;
pub mod connector;
pub mod continuation;
pub mod default;
pub mod designation;
pub mod level;
pub mod location;
pub mod numbering;
pub mod output;
pub mod page_type;
pub mod plc;
pub mod rating;
pub mod sheet;
pub mod style;
pub mod symbol;
pub mod terminal;
pub mod text;
pub mod title_block;
pub mod wire;
pub mod xref;
