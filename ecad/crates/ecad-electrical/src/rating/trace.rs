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

use ecad_model::{ComponentId, Document, NetId, Node, Subject, TerminalId};

use super::RatedSubject;
use crate::symbols::Kind;

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
    pub devices: Vec<(ComponentId, Option<String>)>,
    pub path: BTreeSet<NetId>,
    pub bounded: bool,
}

/// The overcurrent device starters (design 057 §4.1). `rcd2` and `rcd4` are not: they are crossed.
pub fn is_protective_key(key: &str) -> bool {
    matches!(key, "mcb1" | "mcb2" | "rcbo2" | "fuse" | "fuseterm") || key.strip_prefix("fusebank").is_some_and(ecad_model::words::is_digits)
}

/// The load-side pins of a protective starter (design 057 §4.2), from its pin labels
/// (`symbols.rs`): the even-numbered pins of `mcb*`, `rcbo2`, `fuse`, `fuseterm` (and `rcbo2`'s
/// `N2`); a fuse bank's numbered outputs.
pub fn load_side(key: &str, label: &str) -> bool {
    let n = label.parse::<u32>().ok();
    if key.starts_with("fusebank") {
        return n.is_some_and(|n| n >= 1);
    }
    match key {
        "mcb1" | "mcb2" | "fuse" | "fuseterm" => n.is_some_and(|n| n % 2 == 0 && n > 0),
        "rcbo2" => label == "N2" || n.is_some_and(|n| n % 2 == 0 && n > 0),
        _ => false,
    }
}

/// The starter keys a component's symbols show.
fn starters(doc: &Document, c: ComponentId) -> Vec<&'static crate::symbols::Starter> {
    doc.representations_of(Subject::Component(c)).filter_map(|(_, r)| crate::devices::rep_starter(doc, r)).collect()
}

/// Whether component `c` shows a protective starter.
pub fn is_protective_component(doc: &Document, c: ComponentId) -> bool {
    starters(doc, c).iter().any(|s| is_protective_key(s.key))
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
        if let Some(d) = up.strip_prefix(a).filter(|d| d.len() == 1 && ecad_model::words::is_digits(d)) {
            return Some(format!("{b}{d}"));
        }
    }
    match up.as_str() {
        "N1" => Some("N2".into()),
        "N2" => Some("N1".into()),
        _ => None,
    }
}

/// Whether a starter is crossed by pin pairs: contacts, power contacts, SSR outputs and RCDs.
fn crossed(s: &crate::symbols::Starter) -> bool {
    matches!(s.kind, Kind::Contact { .. }) || matches!(s.key, "power3" | "power4" | "ssrout" | "rcd2" | "rcd4")
}

/// What reaching terminal `t` means.
enum Step {
    /// A protective device's load side (with a bank's fuse or a strip's position).
    Found(ComponentId, Option<String>),
    /// Cross to these terminals.
    Cross(Vec<TerminalId>),
    Stop,
}

fn step(doc: &Document, t: TerminalId) -> Step {
    let Some(term) = doc.terminals.get(&t) else { return Step::Stop };
    let c = term.component;
    let label = term.label.clone().unwrap_or_default();
    // A strip: the other terminals of this position and level; a fuse position protects.
    if crate::strips::is_strip(doc, c) {
        for p in crate::terminal_types::positions(doc, c) {
            if let Some(ts) = p.levels.values().find(|ts| ts.contains(&t)) {
                // Review of #108: a fuse position protects only what is on its out point (`.no`); its
                // in point (supply side) or an old one-point drawing stops the trace, so a supply-side
                // wire is never passed against it.
                if p.ty.code == "fuse" {
                    if crate::terminal_types::level_of(doc, t).1 == Some('o') {
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
    if crate::connectors::is_connector(doc, c) {
        let (Some(side), Some(name)) = (crate::connectors::side_of(doc, t), crate::connectors::pin_name(doc, t)) else { return Step::Stop };
        let d = crate::connectors::drawn(doc, c);
        let other = d.side(side.other()).and_then(|m| m.get(&name)).cloned().unwrap_or_default();
        return Step::Cross(other);
    }
    let ss = starters(doc, c);
    if let Some(p) = ss.iter().find(|s| is_protective_key(s.key)) {
        if load_side(p.key, &label) {
            let fuse = p.key.starts_with("fusebank").then(|| label.clone());
            return Step::Found(c, fuse);
        }
        return Step::Stop;
    }
    if ss.iter().any(|s| crossed(s))
        && let Some(other) = pair(&label)
    {
        return Step::Cross(doc.terminals_of(c).filter(|(_, x)| x.label.as_deref() == Some(other.as_str())).map(|(id, _)| id).collect());
    }
    Step::Stop
}

/// Trace from `start` nets (design 057 §4.2).
pub fn trace_nets(doc: &Document, start: impl IntoIterator<Item = NetId>) -> Found {
    let conn = doc.connectivity();
    let mut found = Found::default();
    let mut level: Vec<NetId> = Vec::new();
    for n in start {
        if found.path.insert(n) {
            level.push(n);
        }
    }
    while !level.is_empty() {
        let mut next = Vec::new();
        let mut here: Vec<(ComponentId, Option<String>)> = Vec::new();
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
        RatedSubject::Conductor(c) => trace_nets(doc, conn.net_of_conductor(c)),
        RatedSubject::Cable(k) => {
            let mut all = Found::default();
            let Some(cable) = doc.cables.get(&k) else { return all };
            for m in &cable.members {
                let f = trace_nets(doc, conn.net_of_conductor(m.conductor));
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
pub fn named(doc: &Document, text: &str) -> Option<(ComponentId, Option<String>)> {
    let t: String = text.trim().chars().take(64).collect();
    let (tag, fuse) = match t.rsplit_once(':') {
        Some((a, b)) if !b.is_empty() => (a.to_string(), Some(b.to_string())),
        _ => (t.clone(), None),
    };
    let c = doc.components_tagged(&tag).into_iter().next()?;
    if let Some(f) = &fuse {
        let has = crate::terminal_types::positions(doc, c).iter().any(|p| &p.label == f && p.ty.code == "fuse")
            || doc.terminals_of(c).any(|(_, x)| x.label.as_deref() == Some(f.as_str()))
                && starters(doc, c).iter().any(|s| s.key.starts_with("fusebank"));
        if !has {
            return None;
        }
    }
    Some((c, fuse))
}

/// The rating text of a device (or its fuse).
pub fn rating_text(doc: &Document, c: ComponentId, fuse: Option<&str>) -> String {
    let props = doc.components.get(&c).map(|x| &x.properties);
    let key = fuse.map_or_else(|| crate::symbols::RATING_KEY.to_string(), |f| format!("{}{f}", crate::symbols::RATING_OF_PREFIX));
    props.and_then(|p| p.get(&key)).map(|v| v.chars().take(64).collect()).unwrap_or_default()
}

/// In of a device (or its fuse), when its rating reads (design 057 §4.1).
pub fn amps_of(doc: &Document, c: ComponentId, fuse: Option<&str>) -> Option<f64> {
    super::parse::amps(&rating_text(doc, c, fuse))
}

/// A device in words: `-Q01`, `-F01:3`.
pub fn device_words(doc: &Document, c: ComponentId, fuse: Option<&str>) -> String {
    let tag = crate::devices::tag_words(doc, c);
    match fuse {
        Some(f) => format!("{tag}:{f}"),
        None => tag,
    }
}
