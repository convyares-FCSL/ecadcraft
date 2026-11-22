//! The starter catalogue: the [`STARTERS`] table and its lookups.

use super::*;
use crate::plc::rack::CardType as CT;

const fn def_id(n: u128) -> DefinitionId {
    DefinitionId(0x0000_ecad_0018_5ea1_0000_0000_0001_0000 + n)
}

/// A catalogue entry of the [`Family::Basic`] family with no quiet points; [`Starter::of`] and
/// [`Starter::quiet_at`] set the rest.
pub(super) const fn st(key: StarterKey, name: &'static str, aliases: &'static [&'static str], kind: Kind, id: DefinitionId) -> Starter {
    Starter { key, name, aliases, kind, id, family: Family::Basic, quiet: Quiet::None }
}

impl Starter {
    pub(super) const fn of(self, family: Family) -> Starter {
        Starter { family, ..self }
    }

    pub(super) const fn quiet_at(self, quiet: Quiet) -> Starter {
        Starter { quiet, ..self }
    }
}

const HEAD: Kind = Kind::Fixed { exclusive: true };
const PART: Kind = Kind::Fixed { exclusive: false };

// Brief 034's aliases, shared by several entries.
const FUSE_BANK: &[&str] = &["fuse bank", "fuse block", "distribution", "protection"];
const SOCKET: &[&str] = &["socket", "female", "connector"];
const PLUG: &[&str] = &["plug", "male", "connector"];
const DSUB_S: &[&str] = &["d-sub", "dsub", "sub-d", "socket", "fieldbus", "profibus", "connector"];
const DSUB_P: &[&str] = &["d-sub", "dsub", "sub-d", "plug", "fieldbus", "profibus", "connector"];
const M12_S: &[&str] = &["m12", "fieldbus", "profinet", "io-link", "socket", "connector"];
const M12_P: &[&str] = &["m12", "fieldbus", "profinet", "io-link", "plug", "connector"];
const USB: &[&str] = &["usb", "bulkhead", "connector"];
// Brief 044's aliases.
const PINPAIR: &[&str] = &["pin pair", "mated pin", "plug and socket", "connector pin", "connector"];
const PIN_S: &[&str] = &["socket pin", "female pin", "connector pin", "connector"];
const PIN_P: &[&str] = &["plug pin", "male pin", "connector pin", "connector"];

/// A connector body (stage 3a): a head, every pin quiet (prefix `X`, `defaults::symbols`).
const fn conn(key: StarterKey, name: &'static str, aliases: &'static [&'static str], id: DefinitionId) -> Starter {
    st(key, name, aliases, HEAD, id).of(Family::Connectors).quiet_at(Quiet::All)
}

/// A mating connector half or pin (brief 044, design 044 §3.1): every pin quiet.
const fn half(key: StarterKey, name: &'static str, aliases: &'static [&'static str], side: Side, id: DefinitionId) -> Starter {
    st(key, name, aliases, Kind::ConnectorHalf { side }, id).of(Family::Connectors).quiet_at(Quiet::All)
}

const S: Side = Side::Socket;
const P: Side = Side::Plug;

/// Contacts and protection (brief 034 stage 1): definition IDs `0x100 + n`.
const fn cp(n: u128) -> DefinitionId {
    def_id(0x100 + n)
}

/// Sensors and actuators (stage 2): `0x200 + n`.
const fn sa(n: u128) -> DefinitionId {
    def_id(0x200 + n)
}

/// Connectors (stage 3a): `0x300 + n`.
const fn cn(n: u128) -> DefinitionId {
    def_id(0x300 + n)
}

/// PLC card bodies (stage 3b): `0x400 + n`.
const fn pc(n: u128) -> DefinitionId {
    def_id(0x400 + n)
}

/// A PLC card body (stage 3b): a head (prefix `A`, `defaults::symbols`).
const fn card(key: StarterKey, name: &'static str, aliases: &'static [&'static str], id: DefinitionId) -> Starter {
    st(key, name, aliases, HEAD, id).of(Family::Plc)
}

/// The catalogue, in table order (design 018 §4, then design 034 §3–§6). A definition ID is never
/// reused or changed once it has been on `dev`.
pub const STARTERS: &[Starter] = &[
    st(K::Coil, "Contactor / relay coil", &["coil", "relay", "contactor"], HEAD, def_id(1)),
    st(K::No, "NO contact", &["make", "normally open", "contact"], Kind::Contact { make: true }, def_id(2)),
    st(K::Nc, "NC contact", &["break", "normally closed", "contact"], Kind::Contact { make: false }, def_id(3)),
    st(K::Power3, "Power contacts, 3-pole", &["main contacts", "contactor"], PART, def_id(4)),
    st(K::Mcb1, "MCB 1-pole", &["breaker", "circuit breaker"], HEAD, def_id(5)),
    st(K::Mcb2, "MCB 2-pole", &["breaker", "circuit breaker"], HEAD, def_id(6)),
    st(K::Fuse, "Fuse", &["fuse"], HEAD, def_id(7)),
    st(K::Psu, "Power supply", &["psu", "ac/dc"], HEAD, def_id(8)),
    st(K::Pbno, "Push button, NO", &["push", "button", "start"], Kind::Contact { make: true }, def_id(9)),
    st(K::Pbnc, "Push button, NC", &["push", "button", "stop"], Kind::Contact { make: false }, def_id(10)),
    st(K::Sel, "Selector switch, 2-position", &["selector", "switch"], Kind::Contact { make: true }, def_id(11)),
    st(K::Lamp, "Indicator lamp", &["lamp", "light", "pilot"], HEAD, def_id(12)),
    st(K::Motor3, "Motor, 3-phase", &["motor"], HEAD, def_id(13)),
    st(K::Motor1, "Motor, single-phase", &["motor"], HEAD, def_id(14)),
    st(K::Trafo, "Transformer", &["control transformer"], HEAD, def_id(15)),
    st(K::TerminalBlock, "Terminal", &["terminal", "clamp"], Kind::TerminalBlock, def_id(16)),
    // Brief 028 (design 028 §2): a protective-earth terminal; it joins a strip like `connection_point`.
    st(K::TerminalBlockPe, "PE terminal", &["pe terminal", "earth terminal"], Kind::TerminalBlock, def_id(21)),
    st(K::Di, "PLC digital input", &["plc", "input", "di"], Kind::Channel(crate::plc::rack::CardType::Di), def_id(17)),
    st(K::Do, "PLC digital output", &["plc", "output", "do"], Kind::Channel(crate::plc::rack::CardType::Dq), def_id(18)),
    st(K::Continuation, "Continuation", &["continuation", "arrow", "potential", "link"], Kind::Tagless, def_id(20)),
    // ---- Brief 034 stage 1: contacts and protection (design 034 §3).
    st(K::Co, "Change-over contact", &["change-over", "changeover", "c/o", "spdt", "contact", "relay"], Kind::ChangeOver, cp(1)).of(Family::Contacts),
    st(K::Swco, "Switch, change-over", &["change-over", "switch", "toggle"], Kind::ChangeOver, cp(2)).of(Family::Contacts),
    st(K::Power4, "Power contacts, 4-pole", &["main contacts", "contactor", "4-pole"], PART, cp(3)).of(Family::Contacts),
    st(K::Rcbo2, "RCBO, 1P+N", &["rcbo", "residual current", "breaker", "protection"], HEAD, cp(4)).of(Family::Protection),
    st(K::Rcd2, "RCD, 2-pole", &["rcd", "rccb", "residual current", "protection"], HEAD, cp(5)).of(Family::Protection),
    st(K::Rcd4, "RCD, 4-pole", &["rcd", "rccb", "residual current", "protection"], HEAD, cp(6)).of(Family::Protection),
    st(K::Ssrin, "SSR input", &["ssr", "solid state relay", "relay"], HEAD, cp(7)).of(Family::Contacts),
    st(K::Ssrout, "SSR output", &["ssr", "solid state relay", "contact"], PART, cp(8)).of(Family::Contacts),
    st(K::Fuseterm, "Fuse terminal", &["fuse", "terminal", "fuse holder", "protection"], HEAD, cp(9)).of(Family::Protection),
    st(K::Fusebank4, "Fuse bank, 4-way", FUSE_BANK, HEAD, cp(10)).of(Family::Protection).quiet_at(Quiet::AllBut(&["L+"])),
    st(K::Fusebank6, "Fuse bank, 6-way", FUSE_BANK, HEAD, cp(11)).of(Family::Protection).quiet_at(Quiet::AllBut(&["L+"])),
    st(K::Fusebank8, "Fuse bank, 8-way", FUSE_BANK, HEAD, cp(12)).of(Family::Protection).quiet_at(Quiet::AllBut(&["L+"])),
    st(K::Fusebank10, "Fuse bank, 10-way", FUSE_BANK, HEAD, cp(13)).of(Family::Protection).quiet_at(Quiet::AllBut(&["L+"])),
    st(K::Fusebank12, "Fuse bank, 12-way", FUSE_BANK, HEAD, cp(14)).of(Family::Protection).quiet_at(Quiet::AllBut(&["L+"])),
    // ---- Brief 034 stage 2: sensors and field devices (design 034 §4).
    st(K::Prox3, "Proximity switch, 3-wire PNP", &["proximity", "prox", "inductive", "sensor"], HEAD, sa(1)).of(Family::Sensors),
    st(K::Floatno, "Float switch, NO", &["float", "level", "sensor"], Kind::Contact { make: true }, sa(2)).of(Family::Sensors),
    st(K::Floatnc, "Float switch, NC", &["float", "level", "sensor"], Kind::Contact { make: false }, sa(3)).of(Family::Sensors),
    st(K::Rollerno, "Position switch with roller, NO", &["roller", "limit", "position", "sensor"], Kind::Contact { make: true }, sa(4))
        .of(Family::Sensors),
    st(K::Rollernc, "Position switch with roller, NC", &["roller", "limit", "position", "sensor"], Kind::Contact { make: false }, sa(5))
        .of(Family::Sensors),
    st(K::Interlocknc, "Interlock switch, NC", &["interlock", "guard", "safety", "door switch", "sensor"], Kind::Contact { make: false }, sa(6))
        .of(Family::Sensors),
    st(K::Interlockno, "Interlock switch, NO", &["interlock", "guard", "safety", "sensor"], Kind::Contact { make: true }, sa(7)).of(Family::Sensors),
    st(K::Valve, "Solenoid valve", &["valve", "solenoid", "actuator"], HEAD, sa(8)).of(Family::Actuators),
    st(K::Valve2, "Dual solenoid valve", &["valve", "solenoid", "dual", "actuator"], HEAD, sa(9)).of(Family::Actuators),
    st(K::Tc, "Thermocouple", &["thermocouple", "temperature", "sensor"], HEAD, sa(10)).of(Family::Sensors),
    st(K::Heater, "Heater", &["heater", "heating element", "heat"], HEAD, sa(11)).of(Family::Actuators),
    // ---- Brief 034 stage 3a: connector bodies (design 034 §5); halves only, mating is M1 1.11.
    // Brief 044 (design 044 §3.1): the mating halves are connector halves, version 2.
    half(K::Socket2, "Socket, 2-pin", SOCKET, S, cn(1)),
    half(K::Socket3, "Socket, 3-pin", SOCKET, S, cn(2)),
    half(K::Socket4, "Socket, 4-pin", SOCKET, S, cn(3)),
    half(K::Socket6, "Socket, 6-pin", SOCKET, S, cn(4)),
    half(K::Socket8, "Socket, 8-pin", SOCKET, S, cn(5)),
    half(K::Socket12, "Socket, 12-pin", SOCKET, S, cn(6)),
    half(K::Plug2, "Plug, 2-pin", PLUG, P, cn(7)),
    half(K::Plug3, "Plug, 3-pin", PLUG, P, cn(8)),
    half(K::Plug4, "Plug, 4-pin", PLUG, P, cn(9)),
    half(K::Plug6, "Plug, 6-pin", PLUG, P, cn(10)),
    half(K::Plug8, "Plug, 8-pin", PLUG, P, cn(11)),
    half(K::Plug12, "Plug, 12-pin", PLUG, P, cn(12)),
    half(K::Dsub9s, "D-sub socket, 9-way", DSUB_S, S, cn(13)),
    half(K::Dsub15s, "D-sub socket, 15-way", DSUB_S, S, cn(14)),
    half(K::Dsub25s, "D-sub socket, 25-way", DSUB_S, S, cn(15)),
    half(K::Dsub9p, "D-sub plug, 9-way", DSUB_P, P, cn(16)),
    half(K::Dsub15p, "D-sub plug, 15-way", DSUB_P, P, cn(17)),
    half(K::Dsub25p, "D-sub plug, 25-way", DSUB_P, P, cn(18)),
    half(K::M12s4, "M12 socket, 4-pin", M12_S, S, cn(19)),
    half(K::M12s5, "M12 socket, 5-pin", M12_S, S, cn(20)),
    half(K::M12s8, "M12 socket, 8-pin", M12_S, S, cn(21)),
    half(K::M12p4, "M12 plug, 4-pin", M12_P, P, cn(22)),
    half(K::M12p5, "M12 plug, 5-pin", M12_P, P, cn(23)),
    half(K::M12p8, "M12 plug, 8-pin", M12_P, P, cn(24)),
    conn(K::Rj45, "RJ45 bulkhead", &["rj45", "ethernet", "profinet", "bulkhead", "connector"], cn(25)),
    conn(K::Usba, "USB-A bulkhead", USB, cn(26)),
    conn(K::Usbb, "USB-B bulkhead", USB, cn(27)),
    // ---- Brief 044: connector pins (design 044 §3.2), drawn one by one across wires.
    half(K::Pinpair, "Connector pin pair", PINPAIR, Side::Both, cn(28)),
    half(K::Pins, "Connector socket pin", PIN_S, S, cn(29)),
    half(K::Pinp, "Connector plug pin", PIN_P, P, cn(30)),
    // ---- Brief 034 stage 3b: PLC card bodies (design 034 §6); channels join as `di`/`do`.
    card(K::Plccpu, "PLC CPU", &["plc", "cpu", "controller", "card"], pc(1)),
    card(K::Plcps, "PLC power supply", &["plc", "power", "card"], pc(2)),
    card(K::Plcdi, "PLC digital input card", &["plc", "di", "input", "card"], pc(3)).quiet_at(Quiet::All),
    card(K::Plcdq, "PLC digital output card", &["plc", "dq", "do", "output", "card"], pc(4)).quiet_at(Quiet::All),
    card(K::Plcai, "PLC analogue input card", &["plc", "ai", "analogue", "card"], pc(5)).quiet_at(Quiet::All),
    card(K::Plcao, "PLC analogue output card", &["plc", "ao", "analogue", "card"], pc(6)).quiet_at(Quiet::All),
    card(K::Plcrtd, "PLC RTD card", &["plc", "rtd", "temperature", "card"], pc(7)).quiet_at(Quiet::All),
    card(K::Plciol, "PLC IO-Link master", &["plc", "io-link", "iol", "card"], pc(8)).quiet_at(Quiet::All),
    // ---- Brief 036: multi-level terminals (design 036 §3.1). Geometry, points and bridges come from
    // `terminal_block_types`; `termfuse` shares `fuseterm`'s drawing.
    lvl(K::Term2L1, "Two-level terminal, level 1", TWO, mt(1)),
    lvl(K::Term2L2, "Two-level terminal, level 2", TWO, mt(2)),
    lvl(K::Term2, "Two-level terminal", TWO, mt(3)),
    lvl(K::Term2bL1, "Two-level terminal, bridged, level 1", TWO, mt(4)),
    lvl(K::Term2bL2, "Two-level terminal, bridged, level 2", TWO, mt(5)),
    lvl(K::Term2b, "Two-level terminal, bridged", TWO, mt(6)),
    lvl(K::Term3L1, "Three-level terminal, level 1", THREE, mt(7)),
    lvl(K::Term3L2, "Three-level terminal, level 2", THREE, mt(8)),
    lvl(K::Term3L3, "Three-level terminal, level 3", THREE, mt(9)),
    lvl(K::Term3, "Three-level terminal", THREE, mt(10)),
    lvl(K::TermlnpeL1, "L/N/PE terminal, level L", LNPE, mt(11)),
    lvl(K::TermlnpeL2, "L/N/PE terminal, level N", LNPE, mt(12)),
    lvl(K::TermlnpeL3, "L/N/PE terminal, level PE", LNPE, mt(13)),
    lvl(K::Termlnpe, "L/N/PE terminal", LNPE, mt(14)),
    lvl(K::TermsensorL1, "Sensor terminal, level S", SENSOR, mt(15)),
    lvl(K::TermsensorL2, "Sensor terminal, level +", SENSOR, mt(16)),
    lvl(K::TermsensorL3, "Sensor terminal, level -", SENSOR, mt(17)),
    lvl(K::Termsensor, "Sensor terminal", SENSOR, mt(18)),
    lvl(K::Termdisc, "Disconnect terminal", &["disconnect", "knife", "isolating", "terminal"], mt(19)),
    lvl(K::Termfuse, "Fuse terminal (strip position)", &["fuse terminal", "fuse", "terminal"], mt(20)),
    // ---- Brief 045 (design 045 §3.1): a head module (a rack of its own), a power module and a
    // communication module, as 034's card bodies.
    card(K::Plcim, "PLC head module", &["plc", "im", "interface", "head", "remote", "station", "card"], pc(9)),
    card(K::Plcpm, "PLC power module", &["plc", "pm", "power", "card"], pc(10)),
    card(K::Plccm, "PLC communication module", &["plc", "cm", "communication", "fieldbus", "card"], pc(11)).quiet_at(Quiet::All),
    // ---- Brief 045 (design 045 §4.1): one channel cell per card type with channels; cells side
    // by side read as the card. Every point is quiet (a spare channel is normal).
    cell(K::Chdi, "PLC digital input channel", &["plc", "di", "input", "channel"], CT::Di, pc(12)),
    cell(K::Chdq, "PLC digital output channel", &["plc", "dq", "do", "output", "channel"], CT::Dq, pc(13)),
    cell(K::Chai, "PLC analogue input channel", &["plc", "ai", "analogue", "channel"], CT::Ai, pc(14)),
    cell(K::Chao, "PLC analogue output channel", &["plc", "ao", "analogue", "channel"], CT::Ao, pc(15)),
    cell(K::Chrtd, "PLC RTD channel", &["plc", "rtd", "temperature", "channel"], CT::Rtd, pc(16)),
    cell(K::Chiol, "PLC IO-Link port", &["plc", "io-link", "iol", "port", "channel"], CT::Iol, pc(17)),
];

const TWO: &[&str] = &["two-level", "double-deck", "stacked", "terminal"];
const THREE: &[&str] = &["three-level", "stacked", "terminal"];
const LNPE: &[&str] = &["installation terminal", "l/n/pe", "stacked", "terminal"];
const SENSOR: &[&str] = &["sensor terminal", "three-level", "stacked", "terminal"];

/// Multi-level terminal starters (brief 036): `0x500 + n`.
const fn mt(n: u128) -> DefinitionId {
    def_id(0x500 + n)
}

/// A multi-level terminal starter (brief 036): joins a strip as a level.
const fn lvl(key: StarterKey, name: &'static str, aliases: &'static [&'static str], id: DefinitionId) -> Starter {
    st(key, name, aliases, Kind::TerminalBlock, id)
}

/// The starter symbol with key `key`.
///
/// # Semantics
/// [`STARTERS`] holds one row per [`StarterKey`] in variant order, checked when the crate
/// compiles ([`KEYED_IN_ORDER`]), so the row is the one at the key's index.
pub fn starter(key: StarterKey) -> &'static Starter {
    &STARTERS[key as usize]
}

/// Compile-time check that row `i` of [`STARTERS`] is keyed by variant `i`, and that there is a row
/// for every variant.
const KEYED_IN_ORDER: () = {
    assert!(STARTERS.len() == <StarterKey as strum::VariantArray>::VARIANTS.len(), "one STARTERS row per StarterKey");
    let mut i = 0;
    while i < STARTERS.len() {
        assert!(STARTERS[i].key as usize == i, "STARTERS rows follow StarterKey's order");
        i += 1;
    }
};
const _: () = KEYED_IN_ORDER;

/// The starter symbol a key word names (case-insensitive, trimmed): for a word someone typed or a
/// file holds. Code that knows which starter it means uses [`starter`] with its [`StarterKey`].
pub fn by_key(key: &str) -> Option<&'static Starter> {
    <StarterKey as ecad_model::Word>::parse(key).map(starter)
}

/// The starter symbol an embedded definition was made from, by its origin.
pub fn of_definition(d: &Definition) -> Option<&'static Starter> {
    let o = d.origin.as_ref()?;
    if o.library != STARTER_LIBRARY {
        return None;
    }
    STARTERS.iter().find(|s| s.id == o.definition)
}

/// The project's embedded copy of `s` as this catalogue builds it, found by origin: library,
/// definition **and content hash** (brief 044, FP3), so a project holding an older version of a
/// starter embeds the current one beside it for new placements.
pub fn embedded(doc: &Document, s: &Starter) -> Option<DefinitionId> {
    let hash = s.definition().ok()?.origin.as_ref()?.content_hash;
    doc.definitions
        .iter()
        .find(|(_, d)| d.origin.as_ref().is_some_and(|o| o.library == STARTER_LIBRARY && o.definition == s.id && o.content_hash == hash))
        .map(|(id, _)| *id)
}

/// The kind of a stored definition: its starter's, except that a connector half embedded before
/// brief 044 (no side bridges, version 1) is the exclusive head it was then (design 044 §3.1).
/// A project symbol (brief 048, design §5.2): an edited starter keeps its starter's kind; any
/// other is a device of its own (`join: device`) or part of a named device (`join: part`), with
/// fixed labels.
pub fn kind_of_definition(d: &Definition) -> Option<Kind> {
    if is_project_symbol(d) {
        if let Some(k) = d.derived_from.as_ref().and_then(|o| starter_of_origin(o)).map(|s| s.kind) {
            return Some(k);
        }
        return Some(Kind::Fixed { exclusive: join_of(d) != Join::Part });
    }
    let s = of_definition(d)?;
    Some(match s.kind {
        Kind::ConnectorHalf { .. } if !is_mating(d) => HEAD,
        k @ (Kind::Contact { .. }
        | Kind::TerminalBlock
        | Kind::Channel(_)
        | Kind::ChangeOver
        | Kind::Fixed { .. }
        | Kind::Tagless
        | Kind::ConnectorHalf { .. }) => k,
    })
}
