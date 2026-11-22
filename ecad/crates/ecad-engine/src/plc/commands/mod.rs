//! The PLC commands that place drawing (briefs 045 and 051): `plcchannels` here, `plcpage` in
//! [`page`]. The other PLC commands are generic and declared in `ecad_electrical::plc::commands`.
//! The model they edit is [`crate::plc`].
//!
//! Each command is a thin shim (brief 070): its params struct reads the JSON, the rules in
//! [`ecad_electrical::plc::commands`] decide, and the shim commits the ops and answers in JSON.

pub mod channels;
pub mod page;

use cadcraft_engine::CommandSpec;

use crate::commands::input::{Typed, typed};

/// The PLC commands with a typed input.
pub const TYPED: &[Typed] = &[Typed::of::<channels::PlcchannelsParams>("plcchannels"), Typed::of::<page::PlcpageParams>("plcpage")];

/// The commands of brief 045.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("plcchannels", "Place Channels", typed::<channels::PlcchannelsParams>)
            .params(
                "{card?: id or tag (absent: a new card), type?: DI|DQ|AI|AO|RTD|IOL (a new card's), first?: channel number, point?: role landed on \
each wire (e.g. C/Q; default the first, C/Q for IO-Link), from: [x,y], to: [x,y]} (one channel cell on each wire the line crosses, in order; \
at most 256; a cell with several points spans its own wires when they are spaced as its points; turned 180° where the wires run down), or {card, rest: true, at?: [x,y]} (every channel not drawn yet, \
as cells in a row, no wires)",
            )
            .noundo(),
    ]
}
