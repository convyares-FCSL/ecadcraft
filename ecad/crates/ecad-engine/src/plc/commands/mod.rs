//! The PLC commands that place drawing: `plcchannels` here, `plcpage` in
//! [`page`]. The other PLC commands are generic and declared in `ecad_electrical::plc::commands`.
//! The model they edit is [`crate::plc`].
//!
//! Each command is a thin shim: its params struct reads the JSON, the rules in
//! [`ecad_electrical::plc::commands`] decide, and the shim commits the ops and answers in JSON.

pub(crate) mod channels;
pub(crate) mod page;

use cadcraft_engine::CommandSpec;

use crate::commands::input::{Typed, typed};

/// The PLC commands with a typed input.
pub(crate) const TYPED: &[Typed] = &[Typed::of::<channels::PlcchannelsParams>("plcchannels"), Typed::of::<page::PlcpageParams>("plcpage")];

/// The PLC commands that place drawing.
pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec::new("plcchannels", "Place Channels", typed::<channels::PlcchannelsParams>).noundo()]
}
