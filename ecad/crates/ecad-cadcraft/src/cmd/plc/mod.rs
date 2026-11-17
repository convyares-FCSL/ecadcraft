//! The PLC commands (briefs 045, 051 and 056): racks and slots, channels, addresses, the I/O
//! list, tags and their exchange, and a card's page. The model they edit is [`crate::plc`].
//!
//! Each command is a thin shim (brief 070): its params struct reads the JSON, the rules in
//! [`ecad_electrical::cmd::plc`] decide, and the shim commits the ops and answers in JSON.

pub mod address;
pub mod channels;
pub mod exchange;
pub mod iolist;
pub mod page;
pub mod rack;
pub mod signals;

use cadcraft_engine::CommandSpec;

use crate::cmd::input::{Typed, typed};

/// The PLC commands with a typed input.
pub const TYPED: &[Typed] = &[
    Typed::of::<rack::PlcslotParams>("plcslot"),
    Typed::of::<rack::PlcracksParams>("plcracks"),
    Typed::of::<channels::PlcchannelsParams>("plcchannels"),
    Typed::of::<channels::PlcchannelParams>("plcchannel"),
    Typed::of::<iolist::IolistParams>("iolist"),
    Typed::of::<address::PlcaddressParams>("plcaddress"),
    Typed::of::<page::PlcpageParams>("plcpage"),
    Typed::of::<signals::PlcmaketagsParams>("plcmaketags"),
    Typed::of::<exchange::PlcexportParams>("plcexport"),
    Typed::of::<exchange::PlcimportParams>("plcimport"),
    Typed::of::<rack::PlccardtypeParams>("plccardtype"),
];

/// The commands of brief 045.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("plcslot", "Card Slot", typed::<rack::PlcslotParams>)
            .params(
                "{card: id or tag, rack?: id or tag (null: take the card out of its rack; absent: its own rack, else the only one), \
slot?: number (absent: the next free slot), swap?: bool (swap with the card in that slot)}",
            )
            .noundo(),
        CommandSpec::new("plcracks", "PLC Racks", typed::<rack::PlcracksParams>)
            .params("{rack?: id or tag} (each rack with its slots in order, and the cards in no rack; read only)")
            .noundo(),
        CommandSpec::new("plcchannels", "Place Channels", typed::<channels::PlcchannelsParams>)
            .params(
                "{card?: id or tag (absent: a new card), type?: DI|DQ|AI|AO|RTD|IOL (a new card's), first?: channel number, point?: role landed on \
each wire (e.g. C/Q; default the first, C/Q for IO-Link), from: [x,y], to: [x,y]} (one channel cell on each wire the line crosses, in order; \
at most 256; a cell with several points spans its own wires when they are spaced as its points; turned 180° where the wires run down), or {card, rest: true, at?: [x,y]} (every channel not drawn yet, \
as cells in a row, no wires)",
            )
            .noundo(),
        CommandSpec::new("plcchannel", "Channel", typed::<channels::PlcchannelParams>)
            .params(
                "{card: id or tag, channel: number as shown, text?: description (null clears), address?: typed address (null clears), \
pins?: {role: pin}, move_to?: channel number (with terminal?: id; a drawn copy takes that channel)}",
            )
            .noundo(),
        CommandSpec::new("iolist", "I/O List", typed::<iolist::IolistParams>)
            .params(
                "{rack?: id or tag, card?: id or tag, state?: used|reserved|spare, limit?: rows (200, max 2000), after?: row index} \
(one row per channel of every card, drawn or not; read only)",
            )
            .noundo(),
        CommandSpec::new("plcaddress", "PLC Addresses", typed::<address::PlcaddressParams>)
            .params(
                "{rack: id or tag, style?: byte.bit|iec|percent|slot|none, input_start?: byte, output_start?: byte, fix?: bool (pin every card at its \
current start), clear?: bool (unpin every card)} or {card: id or tag, start?: byte (null unpins), size?: bytes (null: by type)}",
            )
            .noundo(),
    ]
}
