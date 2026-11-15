//! The PLC commands (briefs 045, 051 and 056): racks and slots, channels, addresses, the I/O
//! list, tags and their exchange, and a card's page. The model they edit is [`crate::plc`].

pub mod address;
pub mod channels;
pub mod exchange;
pub mod iolist;
pub mod page;
pub mod rack;
pub mod signals;

use cadcraft_engine::CommandSpec;

/// The commands of brief 045.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("plcslot", "Card Slot", rack::run_plcslot)
            .params(
                "{card: id or tag, rack?: id or tag (null: take the card out of its rack; absent: its own rack, else the only one), \
slot?: number (absent: the next free slot), swap?: bool (swap with the card in that slot)}",
            )
            .noundo(),
        CommandSpec::new("plcracks", "PLC Racks", rack::run_plcracks)
            .params("{rack?: id or tag} (each rack with its slots in order, and the cards in no rack; read only)")
            .noundo(),
        CommandSpec::new("plcchannels", "Place Channels", channels::run_plcchannels)
            .params(
                "{card?: id or tag (absent: a new card), type?: DI|DQ|AI|AO|RTD|IOL (a new card's), first?: channel number, point?: role landed on \
each wire (e.g. C/Q; default the first, C/Q for IO-Link), from: [x,y], to: [x,y]} (one channel cell on each wire the line crosses, in order; \
at most 256; a cell with several points spans its own wires when they are spaced as its points; turned 180° where the wires run down), or {card, rest: true, at?: [x,y]} (every channel not drawn yet, \
as cells in a row, no wires)",
            )
            .noundo(),
        CommandSpec::new("plcchannel", "Channel", channels::run_plcchannel)
            .params(
                "{card: id or tag, channel: number as shown, text?: description (null clears), address?: typed address (null clears), \
pins?: {role: pin}, move_to?: channel number (with terminal?: id; a drawn copy takes that channel)}",
            )
            .noundo(),
        CommandSpec::new("iolist", "I/O List", iolist::run_iolist)
            .params(
                "{rack?: id or tag, card?: id or tag, state?: used|reserved|spare, limit?: rows (200, max 2000), after?: row index} \
(one row per channel of every card, drawn or not; read only)",
            )
            .noundo(),
        CommandSpec::new("plcaddress", "PLC Addresses", address::run_plcaddress)
            .params(
                "{rack: id or tag, style?: byte.bit|iec|percent|slot|none, input_start?: byte, output_start?: byte, fix?: bool (pin every card at its \
current start), clear?: bool (unpin every card)} or {card: id or tag, start?: byte (null unpins), size?: bytes (null: by type)}",
            )
            .noundo(),
    ]
}
