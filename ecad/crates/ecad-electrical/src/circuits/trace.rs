//! Which device protects a rated subject (design 057 §4.1–§4.2, Q2).
//!
//! # Semantics
//! - **By hand:** a subject's `protected_by` (`-Q01`, or `-F01:3` for a fuse of a bank or a strip's
//!   fuse position) wins ([`named`]).
//! - **Traced** ([`trace_nets`]): from the subject's net (a cable: each core's net), breadth-first
//!   through the connectivity graph. A net reaching a protective device's **load-side** pin
//!   ([`load_side`]) stops there: that device protects it. A line-side pin stops that branch. A
//!   pass-through device is crossed from pin to paired pin: strip positions (the other terminals
//!   of the same level), connector pins (the same pin on the other half), contacts by IEC 60947 pin
//!   pairs (1–2, 3–4, 13–14, 21–22, `L1`–`T1`, …) and RCDs by pole. Any other device stops.
//! - Bounded: at most [`MAX_TRACE_NETS`] nets per subject; each net once (cycles end).
//! - Devices found at the same depth are all returned; the caller uses the largest In.
//!
//! A strip's fuse position (036) protects only what reaches its out point (`.no`); its in point
//! (the supply side) and a one-point drawing stop the trace there.

use std::collections::BTreeSet;

use ecad_model::{ConnectionPointId, DeviceId, Document, NetKey, Node, Subject};

use crate::circuits::RatedSubject;
use ecad_model::objects::devices::ProtectionKind;

/// Most nets one subject's trace visits (design 057 §4.2).
pub const MAX_TRACE_NETS: usize = 64;

/// How a device was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum How {
    Named,
    Traced,
}

impl How {
    pub fn word(self) -> &'static str {
        match self {
            How::Named => "named",
            How::Traced => "traced",
        }
    }
}

/// A trace's result: the devices at the first depth that has any (with the fuse of a bank or
/// a strip position), the nets it crossed, and whether it stopped at its bound.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Found {
    pub devices: Vec<(DeviceId, Option<String>)>,
    pub path: BTreeSet<NetKey>,
    pub bounded: bool,
}

/// The load-side pins of an overcurrent protective device (design 057 §4.2), by its stored kind
/// (design 072 §2.2) and the pin label: the even-numbered pins of a fuse, a fuse terminal, an MCB
/// and an RCBO (and an RCBO's `N2`); a fuse bank's numbered outputs. An RCD has none: it is crossed.
pub fn load_side(kind: ProtectionKind, label: &str) -> bool {
    let n = label.parse::<u32>().ok();
    let even = n.is_some_and(|n| n % 2 == 0 && n > 0);
    match kind {
        ProtectionKind::FuseBank => n.is_some_and(|n| n >= 1),
        ProtectionKind::Fuse | ProtectionKind::FuseTerminalBlock | ProtectionKind::Mcb => even,
        ProtectionKind::Rcbo => label == "N2" || even,
        ProtectionKind::Rcd => false,
    }
}

/// The kind of device `c` when it is an overcurrent protective device (design 072 §2.4).
fn overcurrent_kind(doc: &Document, c: DeviceId) -> Option<ProtectionKind> {
    use crate::devices::traits::Protects;
    let ecad_model::DeviceType::Protection(p) = &doc.devices.get(&c)?.device_type else { return None };
    p.overcurrent().then(|| p.kind())
}

/// Whether device `c` is an overcurrent protective device: its typed kind (design 072 §2.4), never
/// its symbols.
pub fn is_protective_component(doc: &Document, c: DeviceId) -> bool {
    use crate::devices::traits::DeviceKind;
    doc.devices.get(&c).and_then(|d| d.device_type.as_protects()).is_some_and(|p| p.overcurrent())
}

/// The paired pin of a contact or RCD pole (IEC 60947): 1–2, 3–4, 5–6, 7–8 and their two-digit
/// forms (13–14, 21–22, …), `L1`–`T1`, `N1`–`N2`.
pub fn pair(label: &str) -> Option<String> {
    let t = label.trim();
    if let Ok(n) = t.parse::<u32>() {
        let unit = n % 10;
        let other = match unit {
            1 | 3 | 5 | 7 => n + 1,
            2 | 4 | 6 | 8 => n - 1,
            _ => return None,
        };
        return (n < 100).then(|| other.to_string());
    }
    let up = t.to_ascii_uppercase();
    for (a, b) in [("L", "T"), ("T", "L")] {
        if let Some(d) = up.strip_prefix(a).filter(|d| d.len() == 1 && ecad_model::naming::words::is_digits(d)) {
            return Some(format!("{b}{d}"));
        }
    }
    match up.as_str() {
        "N1" => Some("N2".into()),
        "N2" => Some("N1".into()),
        _ => None,
    }
}

/// Whether device `c` is crossed by pin pairs (design 072 §4.4: its stored parts and its type,
/// never its starters): a device with a contact (contacts, change-overs, power contacts), a
/// solid-state relay with its output, and an RCD.
fn crossed(doc: &Document, c: DeviceId) -> bool {
    use ecad_model::objects::devices::Operator;
    use ecad_model::{DeviceType, PartRole, Role};
    let has = |p: PartRole| doc.representations_of(Subject::Device(c)).any(|(_, r)| r.role == Role::Part(p));
    match doc.devices.get(&c).map(|d| &d.device_type) {
        Some(DeviceType::Protection(p)) if p.kind() == ProtectionKind::Rcd => true,
        Some(DeviceType::Switching(s)) if matches!(s.operator, Operator::SolidState(_)) && has(PartRole::Pole) => true,
        _ => has(PartRole::Contact),
    }
}

/// What reaching terminal `t` means.
enum Step {
    /// A protective device's load side (with a bank's fuse or a strip's position).
    Found(DeviceId, Option<String>),
    /// Cross to these terminals.
    Cross(Vec<ConnectionPointId>),
    Stop,
}

fn step(doc: &Document, t: ConnectionPointId) -> Step {
    let Some(term) = doc.terminals.get(&t) else { return Step::Stop };
    let Some(c) = term.device() else { return Step::Stop };
    let label = term.label.clone().unwrap_or_default();
    // A strip: the other terminals of this position and level; a fuse position protects.
    if crate::terminal_strips::strip_of(doc, c).is_some() {
        for p in crate::terminal_strips::terminal_blocks::positions(doc, c) {
            if let Some(ts) = p.levels.values().find(|ts| ts.contains(&t)) {
                // Review of #108: a fuse position protects only what is on its out point (`.no`); its
                // in point (supply side) or an old one-point drawing stops the trace, so a supply-side
                // wire is never passed against it.
                if p.ty.code == "fuse" {
                    if crate::terminal_strips::terminal_blocks::level_of(doc, t).1 == Some('o') {
                        return Step::Found(c, Some(p.label.clone()));
                    }
                    return Step::Stop;
                }
                return Step::Cross(ts.iter().copied().filter(|x| *x != t).collect());
            }
        }
        return Step::Stop;
    }
    // A connector: the same pin on the other half.
    if crate::connectors::connector_of(doc, c).is_some_and(|x| x.mating) {
        let (Some(side), Some(name)) = (crate::connectors::side_of(doc, t), crate::connectors::pin_name(doc, t)) else { return Step::Stop };
        let d = crate::connectors::drawn(doc, c);
        let other = d.side(side.other()).and_then(|m| m.get(&name)).cloned().unwrap_or_default();
        return Step::Cross(other);
    }
    if let Some(kind) = overcurrent_kind(doc, c) {
        if load_side(kind, &label) {
            let fuse = (kind == ProtectionKind::FuseBank).then(|| label.clone());
            return Step::Found(c, fuse);
        }
        return Step::Stop;
    }
    if crossed(doc, c)
        && let Some(other) = pair(&label)
    {
        return Step::Cross(doc.connection_points_of(c).filter(|(_, x)| x.label.as_deref() == Some(other.as_str())).map(|(id, _)| id).collect());
    }
    Step::Stop
}

/// Trace from `start` nets (design 057 §4.2).
pub fn trace_nets(doc: &Document, start: impl IntoIterator<Item = NetKey>) -> Found {
    let conn = doc.connectivity();
    let mut found = Found::default();
    let mut level: Vec<NetKey> = Vec::new();
    for n in start {
        if found.path.insert(n) {
            level.push(n);
        }
    }
    while !level.is_empty() {
        let mut next = Vec::new();
        let mut here: Vec<(DeviceId, Option<String>)> = Vec::new();
        for n in &level {
            let Some(net) = conn.net(*n) else { continue };
            for t in &net.terminals {
                match step(doc, *t) {
                    Step::Found(c, fuse) => {
                        if !here.contains(&(c, fuse.clone())) {
                            here.push((c, fuse));
                        }
                    }
                    Step::Cross(ts) => {
                        for x in ts {
                            if let Some(m) = conn.net_of(&Node::Terminal(x))
                                && !found.path.contains(&m)
                            {
                                if found.path.len() >= MAX_TRACE_NETS {
                                    found.bounded = true;
                                    continue;
                                }
                                found.path.insert(m);
                                next.push(m);
                            }
                        }
                    }
                    Step::Stop => {}
                }
            }
        }
        if !here.is_empty() {
            found.devices = here;
            found.bounded = false;
            return found;
        }
        if found.bounded {
            return found;
        }
        level = next;
    }
    found
}

/// Trace a subject: a wire from its net; a cable from each core's net, the union of what each
/// core finds.
pub fn trace_subject(doc: &Document, s: RatedSubject) -> Found {
    let conn = doc.connectivity();
    match s {
        RatedSubject::Wire(c) => trace_nets(doc, conn.net_of_conductor(c)),
        RatedSubject::Cable(k) => {
            let mut all = Found::default();
            let Some(cable) = doc.cables.get(&k) else { return all };
            for w in cable.cores.iter().filter_map(|m| m.wire()) {
                let f = trace_nets(doc, conn.net_of_conductor(w));
                all.bounded |= f.bounded;
                all.path.extend(f.path);
                for d in f.devices {
                    if !all.devices.contains(&d) {
                        all.devices.push(d);
                    }
                }
            }
            if !all.devices.is_empty() {
                all.bounded = false;
            }
            all
        }
    }
}

/// The device a `protected_by` value names: `-Q01`, or `-F01:3` (a fuse of a bank, a strip's fuse
/// position).
pub fn named(doc: &Document, text: &str) -> Option<(DeviceId, Option<String>)> {
    let t: String = text.trim().chars().take(64).collect();
    let (tag, fuse) = match t.rsplit_once(':') {
        Some((a, b)) if !b.is_empty() => (a.to_string(), Some(b.to_string())),
        _ => (t.clone(), None),
    };
    let c = doc.components_tagged(&tag).into_iter().next()?;
    if let Some(f) = &fuse {
        let has = crate::terminal_strips::terminal_blocks::positions(doc, c).iter().any(|p| &p.label == f && p.ty.code == "fuse")
            || doc.connection_points_of(c).any(|(_, x)| x.label.as_deref() == Some(f.as_str()))
                && doc.devices.get(&c).is_some_and(
                    |d| matches!(&d.device_type, ecad_model::DeviceType::Protection(p) if p.kind() == ecad_model::objects::devices::ProtectionKind::FuseBank),
                );
        if !has {
            return None;
        }
    }
    Some((c, fuse))
}

/// The device `text` names (`-Q01`, `-F01:3`) as stored: the device and its numbered position.
/// `None` when it names nothing or the position is not a number 1 to 255.
pub fn protected_by(doc: &Document, text: &str) -> Option<ecad_model::objects::wire::ProtectedBy> {
    let (device, fuse) = named(doc, text)?;
    let position = match fuse {
        None => None,
        Some(f) => Some(f.trim().parse::<u8>().ok().filter(|n| *n > 0)?),
    };
    Some(ecad_model::objects::wire::ProtectedBy { device, position })
}

/// How many wires and cables name device `c` as their protection: the ones deleting it clears.
pub fn protected_count(doc: &Document, c: DeviceId) -> (usize, usize) {
    let names = |p: Option<ecad_model::objects::wire::ProtectedBy>| p.is_some_and(|p| p.device == c);
    (doc.wires.values().filter(|w| names(w.protected_by)).count(), doc.cables.values().filter(|k| names(k.protected_by)).count())
}

/// What deleting device `c` says of the `protected_by`s it clears (`protected_by cleared on 2
/// wires and 1 cable`); `None` when none name it.
pub fn cleared_words(doc: &Document, c: DeviceId) -> Option<String> {
    let (w, k) = protected_count(doc, c);
    let n = |n: usize, one: &str| format!("{n} {one}{}", if n == 1 { "" } else { "s" });
    let parts: Vec<String> = [(w, "wire"), (k, "cable")].into_iter().filter(|(c, _)| *c > 0).map(|(c, one)| n(c, one)).collect();
    (!parts.is_empty()).then(|| format!("protected_by cleared on {}", parts.join(" and ")))
}

/// The rating of a device (or its fuse) as shown: its typed rating, else its note (design 072
/// §2.2); a strip's fuse position: its typed fuse rating.
pub fn rating_text(doc: &Document, c: DeviceId, fuse: Option<&str>) -> String {
    let key = fuse
        .map_or_else(|| ecad_model::objects::devices::RATING_KEY.to_string(), |f| format!("{}{f}", ecad_model::objects::devices::RATING_OF_PREFIX));
    doc.devices.get(&c).and_then(|d| d.value(&key)).map(|v| v.chars().take(64).collect()).unwrap_or_default()
}

/// In of a device (or its fuse): a protective device's or a strip's fuse position's typed rated
/// current (design 072 §2.2 and §2.5: checks read only the typed fields).
pub fn amps_of(doc: &Document, c: DeviceId, fuse: Option<&str>) -> Option<f64> {
    let d = doc.devices.get(&c)?;
    match &d.device_type {
        ecad_model::DeviceType::Protection(p) => {
            let output = match fuse {
                Some(f) => Some(f.parse::<u8>().ok()?),
                None => None,
            };
            p.rated_current(output).map(|a| a.0)
        }
        ecad_model::DeviceType::TerminalStrip(s) => s.fuse(fuse?)?.rated_current.map(|a| a.0),
        _ => None,
    }
}

/// A device in words: `-Q01`, `-F01:3`.
pub fn device_words(doc: &Document, c: DeviceId, fuse: Option<&str>) -> String {
    let tag = crate::devices::tag_words(doc, c);
    match fuse {
        Some(f) => format!("{tag}:{f}"),
        None => tag,
    }
}
