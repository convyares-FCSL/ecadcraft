//! The electrical work of ECAD's commands (brief 070): typed input in, ops and a plain result (or a
//! [`crate::CmdError`]) out, and the JSON answers built from the document (`*_json`). No engine
//! and no locking. A generic command is a params struct here that implements [`run::Generic`];
//! the engine's one runner reads it ([`input`]), takes the project, runs it and commits its edit.
//! Commands that need the session keep a shim in `ecad_engine::cmd`.

pub mod accessory;
pub mod bom;
pub mod cable;
pub mod cable_draw;
pub mod connect;
pub mod connector;
pub mod continuation;
pub mod default;
pub mod designation;
pub mod device;
pub mod device_link;
pub mod input;
pub mod level;
pub mod location;
pub mod merge;
pub mod numbering;
pub mod output;
pub mod page_type;
pub mod place;
pub mod plc;
pub mod prop;
pub mod props;
pub mod rating;
pub mod reference;
pub mod run;
pub mod sheet;
pub mod style;
pub mod symbol;
pub mod terminal;
pub mod text;
pub mod title_block;
pub mod wire;
pub mod xref;
