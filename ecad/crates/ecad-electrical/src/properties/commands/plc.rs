//! Lane 072 C's rows of `setprop` (design 072 §3, brief 072 "Lanes"): a PLC station's and a
//! module's keys. Each key that became a [`ecad_model::objects::plc::PlcRack`] or
//! [`ecad_model::objects::plc::PlcModule`] field builds one `ModifyDevice`; the registry's object-level
//! `plc.*` entries stay values of their own (`crate::plc::setprop_rule`).
//!
//! | Key | On | Field |
//! | --- | --- | --- |
//! | `plc.rack`, `plc.slot` | module | its station (a tag or ID) and slot |
//! | `plc.channels` | module with one channel group | the group's count |
//! | `plc.signal` | module with one channel group | the group's signal |
//! | `plc.addr.start`, `plc.addr.size` | module | its pinned block |
//! | `plc.ch.<i>.text`, `.address`, `.tag`, `.safe` | module | channel `i`'s own data |
//! | `plc.safety_rated`, `plc.ex_i`, `plc.calibration` | module | its attributes (yes/no) |
//! | `plc.protocol` | station with a coupler, communication module | its protocol |
//! | `plc.network`, `plc.network.address` | station | its network and address there |
//! | `plc.cpu.family` | station with a CPU | the CPU's family |
//! | `plc.address.style` | station | its own address style (none: the project's) |

use ecad_model::objects::plc::{NetAddress, PlcModule, PlcRack, Protocol, RackHead, Signal};
use ecad_model::{DeviceId, Document, ObjectRef, Op, Word};

use crate::properties::commands::{Build, PropRule};
use crate::error::CmdError;
use crate::plc::{address, channels, edit, rack};

/// The `setprop` key of a module's signal.
pub const SIGNAL: &str = "plc.signal";
/// The `setprop` key of a module's safety rating.
pub const SAFETY_RATED: &str = "plc.safety_rated";
/// The `setprop` key of a module being intrinsically safe (Ex i).
pub const EX_I: &str = "plc.ex_i";
/// The `setprop` key of a module's calibration certificate.
pub const CALIBRATION: &str = "plc.calibration";
/// The `setprop` key of a coupler's or communication module's protocol.
pub const PROTOCOL: &str = "plc.protocol";
/// The `setprop` key of a station's network.
pub const NETWORK: &str = "plc.network";
/// The `setprop` key of a station's address on its network.
pub const NETWORK_ADDRESS: &str = "plc.network.address";
/// The `setprop` key of a CPU's family.
pub const CPU_FAMILY: &str = "plc.cpu.family";
/// The `setprop` key of a station's own address style.
pub const ADDRESS_STYLE: &str = "plc.address.style";

/// The module keys of this lane (besides the channel keys).
const MODULE_KEYS: [&str; 9] =
    [rack::RACK_KEY, rack::SLOT_KEY, rack::CHANNELS_KEY, SIGNAL, address::START_KEY, address::SIZE_KEY, SAFETY_RATED, EX_I, CALIBRATION];
/// The station keys of this lane.
const STATION_KEYS: [&str; 5] = [NETWORK, NETWORK_ADDRESS, CPU_FAMILY, ADDRESS_STYLE, PROTOCOL];

/// This lane's rows, asked before [`crate::properties::commands::PROP_RULES`].
pub static RULES: &[PropRule] = &[
    PropRule {
        what: "PLC module fields (072 C)",
        claims: |doc, o, k| module(doc, o).is_some() && (MODULE_KEYS.contains(&k) || k == PROTOCOL || channels::parse_ch_key(k).is_some()),
        build: Build::Record(plan_module),
    },
    PropRule {
        what: "PLC station fields (072 C)",
        claims: |doc, o, k| station(doc, o).is_some() && STATION_KEYS.contains(&k),
        build: Build::Record(plan_station),
    },
    PropRule {
        what: "PLC field on a device that is not a module or station (072 C)",
        claims: |_, o, k| matches!(o, ObjectRef::Device(_)) && is_field_key(k),
        build: Build::Value(not_plc),
    },
];

/// Whether `k` names a module's or station's field.
fn is_field_key(k: &str) -> bool {
    MODULE_KEYS.contains(&k)
        || STATION_KEYS.contains(&k)
        || channels::parse_ch_key(k).is_some()
        || (k.starts_with("plc.") && ecad_model::naming::keys::is_reserved_on(ecad_model::naming::keys::Holder::Device, k))
}

/// The refusal of a module's or station's key on another device.
fn not_plc(doc: &Document, o: ObjectRef, key: &str, _: Option<&str>) -> Result<Option<String>, CmdError> {
    let (who, is) = match o {
        ObjectRef::Device(c) => (
            crate::devices::tag_words(doc, c),
            match crate::plc::card_type_of(doc, c) {
                Some(t) if t.is_rack() => "a station",
                Some(_) => "a module",
                None => "neither",
            },
        ),
        _ => ("That object".into(), "neither"),
    };
    let whose = if STATION_KEYS.contains(&key) && !MODULE_KEYS.contains(&key) { "a PLC station's" } else { "a PLC module's" };
    Err(CmdError::refused(format!("{key} is {whose} field; {who} is {is}.")))
}

fn module(doc: &Document, o: ObjectRef) -> Option<DeviceId> {
    match o {
        ObjectRef::Device(c) => edit::module_of(doc, c).map(|_| c),
        _ => None,
    }
}

fn station(doc: &Document, o: ObjectRef) -> Option<DeviceId> {
    match o {
        ObjectRef::Device(c) => edit::station_of(doc, c).map(|_| c),
        _ => None,
    }
}

/// The one op `ops` hold, if any.
fn one(mut ops: Vec<Op>) -> Option<Op> {
    ops.pop()
}

fn yes_no(key: &str, v: Option<&str>) -> Result<bool, CmdError> {
    match v {
        None => Ok(false),
        Some(t) => ecad_model::naming::words::flag(t).ok_or_else(|| CmdError::refused(format!("{key} is yes or no."))),
    }
}

fn text<const N: usize>(key: &str, v: Option<&str>) -> Result<Option<ecad_model::naming::text::Text<N>>, CmdError> {
    v.map(ecad_model::naming::text::Text::<N>::new).transpose().map_err(|e| CmdError::refused(format!("{key}: {e}.")))
}

fn word<W: Word>(key: &str, v: Option<&str>) -> Result<Option<W>, CmdError> {
    match v {
        None => Ok(None),
        Some(t) => W::parse(t).map(Some).ok_or_else(|| CmdError::refused(format!("{t} is not a value of {key} ({}).", W::WORDS.join(", ")))),
    }
}

/// A module's single channel group, or the refusal for a module with none or several.
fn single_group<'a>(m: &'a mut PlcModule, who: &str) -> Result<&'a mut ecad_model::objects::plc::ChannelGroup, CmdError> {
    let mut groups = m.function.groups_mut();
    match groups.len() {
        1 => groups.pop().ok_or(CmdError::refused("no channel group")),
        0 => Err(CmdError::refused(format!("{who} has no channels."))),
        _ => Err(CmdError::refused(format!("{who} has several channel groups; set each group's in its module."))),
    }
}

/// The `ModifyDevice` of `setprop` `key` on module `o`.
///
/// # Errors
/// A value that does not read, or a key the module's function has no field for.
fn plan_module(doc: &Document, o: ObjectRef, key: &str, value: Option<&str>) -> Result<Option<Op>, CmdError> {
    let c = module(doc, o).ok_or(CmdError::refused(crate::properties::commands::NO_SUCH_OBJECT))?;
    let who = crate::devices::tag_words(doc, c);
    let v = value.map(str::trim).filter(|v| !v.is_empty());
    let mut ops = Vec::new();
    if let Some((i, what)) = channels::parse_ch_key(key) {
        let checked = match what {
            crate::plc::signals::SAFE => crate::plc::signals::safe_value(crate::plc::card_type_of(doc, c), v)?,
            _ => channels::ch_value(what, v)?,
        };
        channels::set_field(doc, &mut ops, c, i, what, checked.as_deref())?;
        return Ok(one(ops));
    }
    let mut module = edit::module_of(doc, c).cloned().ok_or(CmdError::refused(crate::properties::commands::NO_SUCH_OBJECT))?;
    match key {
        rack::RACK_KEY => {
            module.rack = match v.filter(|t| !t.eq_ignore_ascii_case("none")) {
                None => None,
                Some(t) => match rack::rack_named(doc, t)? {
                    r if r == c => return Err(CmdError::refused("A card cannot be its own rack.")),
                    r => Some(r),
                },
            };
        }
        rack::SLOT_KEY => {
            module.slot = match v {
                None => None,
                Some(t) => Some(
                    rack::slot_number(t).and_then(|s| u16::try_from(s).map_err(|_| CmdError::refused("A slot is a whole number from 0 to 999.")))?,
                ),
            };
        }
        rack::CHANNELS_KEY => {
            let n = match v {
                None => 0,
                Some(t) => rack::parse_channels(t).ok_or_else(|| CmdError::refused(format!("A card has 0 to {} channels.", rack::MAX_CHANNELS)))?,
            };
            single_group(&mut module, &who)?.count = u16::try_from(n).unwrap_or(ecad_model::objects::plc::MAX_CHANNELS);
        }
        SIGNAL => {
            let s: Signal = word(SIGNAL, v)?.ok_or_else(|| CmdError::refused(format!("{SIGNAL} needs a signal ({}).", Signal::WORDS.join(", "))))?;
            single_group(&mut module, &who)?.signal = s;
        }
        address::START_KEY | address::SIZE_KEY => {
            let max = if key == address::START_KEY { address::MAX_BYTE } else { address::MAX_SIZE };
            let n = match v {
                None => None,
                Some(t) => Some(
                    ecad_model::naming::words::uint(t, max)
                        .filter(|n| t.len() <= 5 && (key == address::START_KEY || *n >= 1))
                        .ok_or_else(|| CmdError::refused(format!("{key} is a whole number of bytes from 0 to {max}.")))?,
                ),
            };
            if key == address::START_KEY {
                module.address.start = n;
            } else {
                module.address.size = n;
            }
        }
        SAFETY_RATED => module.attributes.safety_rated = yes_no(key, v)?,
        EX_I => module.attributes.intrinsically_safe = yes_no(key, v)?,
        CALIBRATION => module.attributes.calibration_certificate = yes_no(key, v)?,
        PROTOCOL => match &mut module.function {
            ecad_model::objects::plc::ModuleFunction::Communication(ecad_model::objects::plc::Communication::Link(x)) => {
                x.protocol = word::<Protocol>(key, v)?.ok_or_else(|| CmdError::refused(format!("{key} needs a protocol.")))?;
            }
            _ => return Err(CmdError::refused(format!("{who} is not a communication link; only those have a protocol."))),
        },
        _ => return Err(CmdError::input("not a module key")),
    }
    edit::module(doc, &mut ops, c, |m| *m = module);
    Ok(one(ops))
}

/// The `ModifyDevice` of `setprop` `key` on station `o`.
///
/// # Errors
/// A value that does not read, or a key its head has no field for.
fn plan_station(doc: &Document, o: ObjectRef, key: &str, value: Option<&str>) -> Result<Option<Op>, CmdError> {
    let c = station(doc, o).ok_or(CmdError::refused(crate::properties::commands::NO_SUCH_OBJECT))?;
    let who = crate::devices::tag_words(doc, c);
    let v = value.map(str::trim).filter(|v| !v.is_empty());
    let mut s: PlcRack = edit::station_of(doc, c).cloned().ok_or(CmdError::refused(crate::properties::commands::NO_SUCH_OBJECT))?;
    match key {
        NETWORK => s.network = text::<32>(key, v)?,
        NETWORK_ADDRESS => {
            s.address = match v {
                None => None,
                Some(t) => Some(NetAddress::parse(t).ok_or_else(|| {
                    CmdError::refused(format!(
                        "{t} is not a network address: an IP address (192.168.0.10) or a node number (0 to {}).",
                        ecad_model::objects::plc::MAX_NODE
                    ))
                })?),
            };
        }
        CPU_FAMILY => match &mut s.head {
            RackHead::Cpu { family } => *family = text::<32>(key, v)?,
            RackHead::Coupler { .. } => return Err(CmdError::refused(format!("{who} has a coupler at its head; only a CPU has a family."))),
        },
        PROTOCOL => match &mut s.head {
            RackHead::Coupler { protocol } => {
                *protocol = word::<Protocol>(key, v)?.ok_or_else(|| CmdError::refused(format!("{key} needs a protocol.")))?
            }
            RackHead::Cpu { .. } => return Err(CmdError::refused(format!("{who} has a CPU at its head; only a coupler has a protocol."))),
        },
        ADDRESS_STYLE => {
            s.address_style = match v {
                None => None,
                Some(t) => Some(
                    address::Style::parse(t)
                        .ok_or_else(|| {
                            CmdError::refused(format!("{t} is not a value of {key} ({}).", ecad_model::objects::plc::AddressStyle::WORDS.join(", ")))
                        })?
                        .stored(),
                ),
            };
        }
        _ => return Err(CmdError::input("not a station key")),
    }
    let mut ops = Vec::new();
    edit::station(doc, &mut ops, c, |x| *x = s);
    Ok(one(ops))
}
