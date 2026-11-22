//! What today's PLC symbols declare (design 072 §3, A19) and how a station's or module's stored
//! type reads as a [`CardType`].
//!
//! # Semantics
//! - Each PLC starter declares its full type with its data: `plccpu` a station with a CPU head,
//!   `plcim` a station with a coupler; `plcdi` → `DigitalIn`, `plcdq` → `DigitalOut`, `plcai` →
//!   `AnalogIn`, `plcao` → `AnalogOut`, `plcrtd` → `AnalogIn` with the RTD signal, `plciol` →
//!   `Communication::IoLinkMaster` (its ports its channels), `plcps` and `plcpm` → `System`
//!   (power supply, power feed), `plccm` → `Communication::Link`. The channel symbols declare their card's.
//! - A new module's channel count is the project's `plc.<type>.channels` at the moment it is made
//!   ([`new_type`]); from then on the count is the module's own.
//! - [`CardType::of_type`] reads the stored type; nothing is worked out from a symbol.

use ecad_model::DeviceType;
use ecad_model::objects::plc::{
    ChannelGroup, CommLink, CommRole, Communication, IoLinkMaster, ModuleFunction, PlcModule, PlcRack, Protocol, RackHead, Signal, SystemKind,
    SystemModule,
};

use crate::defaults::{self, Scope};
use crate::plc::rack::CardType;
use crate::symbols::StarterKey as K;

/// The built-in count of a new module's channels, before the project's registry value.
const fn standard_count(t: CardType) -> u16 {
    match t {
        CardType::Di | CardType::Dq => 16,
        CardType::Ai | CardType::Ao | CardType::Rtd | CardType::Iol => 4,
        CardType::Cpu | CardType::Im | CardType::Ps | CardType::Pm | CardType::Cm | CardType::Module => 0,
    }
}

fn module(function: ModuleFunction) -> DeviceType {
    DeviceType::PlcModule(PlcModule::new(function))
}

fn group(signal: Signal, t: CardType) -> ChannelGroup {
    ChannelGroup::new(signal, standard_count(t))
}

/// The type a PLC module of card type `t` starts with.
pub fn of_card(t: CardType) -> DeviceType {
    match t {
        CardType::Cpu => DeviceType::PlcRack(PlcRack { head: RackHead::Cpu { family: None }, ..PlcRack::default() }),
        CardType::Im => DeviceType::PlcRack(PlcRack { head: RackHead::Coupler { protocol: Protocol::Profinet }, ..PlcRack::default() }),
        CardType::Di => module(ModuleFunction::DigitalIn(group(Signal::Dc24v, t))),
        CardType::Dq => module(ModuleFunction::DigitalOut(group(Signal::Dc24v, t))),
        CardType::Ai => module(ModuleFunction::AnalogIn(group(Signal::Current4To20, t))),
        CardType::Ao => module(ModuleFunction::AnalogOut(group(Signal::Current4To20, t))),
        CardType::Rtd => module(ModuleFunction::AnalogIn(group(Signal::Rtd, t))),
        CardType::Iol => module(ModuleFunction::Communication(Communication::IoLinkMaster(IoLinkMaster { channels: group(Signal::IoLink, t) }))),
        CardType::Ps => module(ModuleFunction::System(SystemModule { kind: SystemKind::PowerSupply })),
        CardType::Pm => module(ModuleFunction::System(SystemModule { kind: SystemKind::PowerFeed })),
        CardType::Cm => module(ModuleFunction::Communication(Communication::Link(CommLink {
            protocol: Protocol::Other,
            role: CommRole::Gateway,
            ports: Vec::new(),
        }))),
        CardType::Module => module(ModuleFunction::System(SystemModule { kind: SystemKind::EndCap })),
    }
}

/// The card type starter `key` draws as a body or a channel, if it is a PLC symbol.
fn card_of_starter(key: K) -> Option<CardType> {
    Some(match key {
        K::Plccpu => CardType::Cpu,
        K::Plcim => CardType::Im,
        K::Plcdi | K::Di | K::Chdi => CardType::Di,
        K::Plcdq | K::Do | K::Chdq => CardType::Dq,
        K::Plcai | K::Chai => CardType::Ai,
        K::Plcao | K::Chao => CardType::Ao,
        K::Plcrtd | K::Chrtd => CardType::Rtd,
        K::Plciol | K::Chiol => CardType::Iol,
        K::Plcps => CardType::Ps,
        K::Plcpm => CardType::Pm,
        K::Plccm => CardType::Cm,
        _ => return None,
    })
}

/// What a device starter `key` begins is, with its data (design 072 §2.3, §3).
pub fn starter_type(key: K) -> DeviceType {
    match card_of_starter(key) {
        Some(t) => of_card(t),
        None => DeviceType::of_group(key.device_group()),
    }
}

/// The type a new device declared `declared` is made with in `doc`: a module's channel count is
/// the project's `plc.<type>.channels` (design 045 §2.3).
pub fn new_type(doc: &ecad_model::Document, declared: &DeviceType) -> DeviceType {
    let mut t = declared.clone();
    let Some(card) = CardType::of_type(&t) else { return t };
    let Some(entry) = card.channels_entry() else { return t };
    let n = defaults::resolve(doc, entry, Scope::project()).number();
    // The registry holds 0–256.
    let n = if n.is_finite() && n >= 0.0 { (n as u16).min(ecad_model::objects::plc::MAX_CHANNELS) } else { standard_count(card) };
    if let DeviceType::PlcModule(m) = &mut t
        && let [g] = m.function.groups_mut().as_mut_slice()
    {
        g.count = n;
    }
    t
}

impl CardType {
    /// The card type a stored type reads as; `None` for a device that is not a station or module.
    pub fn of_type(t: &DeviceType) -> Option<CardType> {
        match t {
            DeviceType::PlcRack(r) => Some(match r.head {
                RackHead::Cpu { .. } => CardType::Cpu,
                RackHead::Coupler { .. } => CardType::Im,
            }),
            DeviceType::PlcModule(m) => Some(match &m.function {
                ModuleFunction::DigitalIn(_) => CardType::Di,
                ModuleFunction::DigitalOut(_) => CardType::Dq,
                ModuleFunction::AnalogIn(g) => match g.signal {
                    Signal::Rtd | Signal::Thermocouple => CardType::Rtd,
                    Signal::Dc24v | Signal::Ac230v | Signal::Relay | Signal::Current4To20 | Signal::Voltage0To10 | Signal::IoLink => CardType::Ai,
                },
                ModuleFunction::AnalogOut(_) => CardType::Ao,
                ModuleFunction::Communication(Communication::IoLinkMaster(_)) => CardType::Iol,
                ModuleFunction::Communication(Communication::Link(_)) => CardType::Cm,
                ModuleFunction::System(s) => match s.kind {
                    SystemKind::PowerSupply => CardType::Ps,
                    SystemKind::PowerFeed | SystemKind::PotentialDistribution | SystemKind::Fusing | SystemKind::EndCap | SystemKind::Shield => {
                        CardType::Pm
                    }
                },
                ModuleFunction::Mixed(_)
                | ModuleFunction::Position(_)
                | ModuleFunction::Drive(_)
                | ModuleFunction::Measurement(_)
                | ModuleFunction::SafetyLogic(_) => CardType::Module,
            }),
            DeviceType::PowerSupply(_)
            | DeviceType::Protection(_)
            | DeviceType::Switching(_)
            | DeviceType::Sensor(_)
            | DeviceType::Motor(_)
            | DeviceType::Load(_)
            | DeviceType::Indicator(_)
            | DeviceType::TerminalStrip(_)
            | DeviceType::Connector(_)
            | DeviceType::General => None,
        }
    }
}
