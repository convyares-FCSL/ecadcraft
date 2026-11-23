//! Connector devices: a plug and its socket drawn as one device that mates.
//!
//! # Semantics
//! - A **connector** is one component (`-X10`) whose stored type is a Connector with `mating` set
//!   (its first half's or pin's starter sets it, [`Connector::mating`]). Each pin of each half is one
//!   terminal: label = the pin name, key = the side key `.s` (socket) or `.p` (plug).
//! - **Mating** is the per-position bridge rule: every half definition bridges `.s` with
//!   `.p`, so pin `n` of the socket half and pin `n` of the plug half are one net.
//! - A **half** is shown and typed as the connector's tag, a separator and a side name (`-X10.S`),
//!   all registry defaults ([`crate::defaults::connectors`]); a pin as `-X10.S:3`.
//! - The per-half BOM fields and location are component properties `half.<s|p>.<field>`;
//!   `pins` says how many pins the part has.
//! - A connector half embedded before the side bridges existed (no side bridges) is not a connector: it stays the
//!   device it was.
//!
//! Everything here reads the document; the planners return ops and change nothing. Excludes: the
//! commands themselves (`ecad_engine::connectors::commands`), BOM rows (`reports.rs` reads [`half_rows`]),
//! and reference texts (`references.rs` reads [`ref_classes`]).

pub mod commands;
pub(crate) mod view;
use std::collections::{BTreeMap, BTreeSet};

use ecad_model::{
    ConnectionPoint, ConnectionPointId, DefinitionId, Device, DeviceId, Document, Form, IdAllocator, Item, ItemId, ItemKind, Ledger, Op, PageId,
    Placement, Pt, Representation, RepresentationId, RouteEnd, RouteId, Style, Subject, SymbolInstance,
};

use ecad_model::Noun;
use ecad_model::Word;
use ecad_model::objects::devices::{Connector, ConnectorCheck, DeviceType, HalfPart};

use crate::CmdError;
use crate::checks::{Check, Fix, Kind as CheckKind, Object};
use crate::defaults::{self};
use crate::locations::{BoxCache, Located};
use crate::symbols::{self, Kind, PLUG_KEY, Side, Starter, StarterKey};
use crate::terminal_strips::{self, connection_point_words};
use crate::wires::ON_WIRE;
use crate::wires::placing::OnRoute;
use ecad_model::registry::Scope;

/// Most pins a connector has: a guard, not a preference.
pub const MAX_PINS: usize = 64;
/// Most characters of a pin name.
pub const MAX_NAME_CHARS: usize = 8;
/// Most characters of a typed pin-name list that are read.
pub(crate) const MAX_LIST_CHARS: usize = 1_000;
/// The component property holding the pin count.
pub const PINS_KEY: &str = "pins";
/// The component property holding a typed list of pin names, in pin order (`L,N,PE`;
/// names not drawn yet are kept, never lost).
pub(crate) const PIN_NAMES_KEY: &str = "pin_names";
/// The start of the per-half property keys.
pub(crate) const HALF_PREFIX: &str = "half.";
/// The per-half fields the BOM reads. A half's location is the connector's override
/// `location.socket` / `location.plug` (`half.<s>.location` is retired).
pub(crate) const HALF_FIELDS: [&str; 3] = ["description", "maker", "part_number"];
/// The start of a connector's ignored-warning keys (`ignore.connector-family`;
/// the Checks rows' Ignore button).
pub(crate) const IGNORE_PREFIX: &str = "ignore.";

/// Connector `c`'s own data (its type's), when it is a connector.
pub fn connector_of(doc: &Document, c: DeviceId) -> Option<&Connector> {
    doc.devices().get(&c)?.device_type.as_connector()
}

/// The ignorable Checks row of `kind`, if it is one.
pub(crate) fn ignorable(kind: CheckKind) -> Option<ConnectorCheck> {
    match kind {
        CheckKind::ConnectorFamily => Some(ConnectorCheck::Family),
        CheckKind::ConnectorSize => Some(ConnectorCheck::Size),
        _ => None,
    }
}

/// Whether connector `c` ignores Checks rows of `kind`.
pub fn ignores(doc: &Document, c: DeviceId, kind: CheckKind) -> bool {
    ignorable(kind).is_some_and(|k| connector_of(doc, c).is_some_and(|x| x.ignored.contains(&k)))
}

/// The keys of the pin starters.
pub(crate) const PIN_KEYS: [StarterKey; 3] = [StarterKey::Pinpair, StarterKey::Pins, StarterKey::Pinp];

// ---------------------------------------------------------------- families

use ecad_model::objects::devices::ConnectorFamily;

/// What a family's halves come in: its sizes, its bulkhead and its whole-half
/// starters.
pub trait FamilyRules: Sized {
    /// The sizes its whole halves come in; empty for a bulkhead.
    fn sizes(self) -> &'static [usize];
    /// The bulkhead starter of a one-piece family.
    fn bulkhead(self) -> Option<&'static str>;
    /// The starter key of its whole half of `side` with `n` pins (`socket4`, `dsub9p`, `m12s5`).
    fn half_key(self, side: Side, n: usize) -> Option<String>;
    /// The refusal for a pin count that is not one of its sizes.
    fn size_refusal(self) -> String;
}

impl FamilyRules for ConnectorFamily {
    /// The sizes its whole halves come in; empty for a bulkhead.
    fn sizes(self) -> &'static [usize] {
        match self {
            ConnectorFamily::Plain => &[2, 3, 4, 6, 8, 12],
            ConnectorFamily::Dsub => &[9, 15, 25],
            ConnectorFamily::M12 => &[4, 5, 8],
            ConnectorFamily::Rj45 | ConnectorFamily::Usba | ConnectorFamily::Usbb => &[],
        }
    }

    /// The bulkhead starter of a one-piece family.
    fn bulkhead(self) -> Option<&'static str> {
        match self {
            ConnectorFamily::Rj45 => Some("rj45"),
            ConnectorFamily::Usba => Some("usba"),
            ConnectorFamily::Usbb => Some("usbb"),
            _ => None,
        }
    }

    /// The starter key of its whole half of `side` with `n` pins (`socket4`, `dsub9p`, `m12s5`).
    fn half_key(self, side: Side, n: usize) -> Option<String> {
        if !self.sizes().contains(&n) {
            return None;
        }
        Some(match (self, side) {
            (ConnectorFamily::Plain, Side::Socket) => format!("socket{n}"),
            (ConnectorFamily::Plain, Side::Plug) => format!("plug{n}"),
            (ConnectorFamily::Dsub, Side::Socket) => format!("dsub{n}s"),
            (ConnectorFamily::Dsub, Side::Plug) => format!("dsub{n}p"),
            (ConnectorFamily::M12, Side::Socket) => format!("m12s{n}"),
            (ConnectorFamily::M12, Side::Plug) => format!("m12p{n}"),
            _ => return None,
        })
    }

    /// The refusal for a pin count that is not one of its sizes.
    fn size_refusal(self) -> String {
        let sizes: Vec<String> = self.sizes().iter().map(ToString::to_string).collect();
        let list = match sizes.as_slice() {
            [] => String::new(),
            [one] => one.clone(),
            [rest @ .., last] => format!("{} or {last}", rest.join(", ")),
        };
        let article = if self == ConnectorFamily::M12 { "An" } else { "A" };
        format!("{article} {} has {list} pins. Pick one, or draw pin by pin with Pins.", self.name())
    }
}

/// The family of a connector starter (`None` for a pin, which has none, and for other symbols).
pub fn family_of(s: &Starter) -> Option<ConnectorFamily> {
    let k = s.key;
    if PIN_KEYS.contains(&k) || s.family != symbols::Family::Connectors {
        return None;
    }
    use StarterKey as K;
    Some(match k {
        K::Dsub9s | K::Dsub15s | K::Dsub25s | K::Dsub9p | K::Dsub15p | K::Dsub25p => ConnectorFamily::Dsub,
        K::M12s4 | K::M12s5 | K::M12s8 | K::M12p4 | K::M12p5 | K::M12p8 => ConnectorFamily::M12,
        K::Rj45 => ConnectorFamily::Rj45,
        K::Usba => ConnectorFamily::Usba,
        K::Usbb => ConnectorFamily::Usbb,
        _ => ConnectorFamily::Plain,
    })
}

/// Whether `s` is one of the pin starters (`pinpair`, `pins`, `pinp`).
pub fn is_pin_starter(s: &Starter) -> bool {
    PIN_KEYS.contains(&s.key)
}

/// The pin count of a whole-half starter (`None` for a pin).
pub fn size_of(s: &Starter) -> Option<usize> {
    if is_pin_starter(s) || !matches!(s.kind, Kind::ConnectorHalf { .. }) {
        return None;
    }
    s.definition().ok().map(|d| d.connection_points.len())
}

// ---------------------------------------------------------------- what is a connector

/// The side of terminal `t` when it is a connector pin.
pub fn side_of(doc: &Document, t: ConnectionPointId) -> Option<Side> {
    let term = doc.connection_points().get(&t)?;
    let side = Side::of_key(term.key.as_ref()?)?;
    connector_of(doc, term.device()?).is_some_and(|x| x.mating).then_some(side)
}

/// The starter a representation draws when it is a connector half or pin.
pub fn rep_half(doc: &Document, r: &Representation) -> Option<&'static Starter> {
    let (d, _) = crate::devices::tags::rep_definition(doc, r)?;
    // A half or a pin is what the representation's stored part says.
    if !matches!(crate::symbols::reps::rep_type(doc, r), ecad_model::PartRole::Half | ecad_model::PartRole::Pin) {
        return None;
    }
    symbols::catalogue::of_definition(d)
}

/// The side a representation draws (`Both` for a pin pair).
pub(crate) fn rep_side(doc: &Document, r: &Representation) -> Option<Side> {
    rep_half(doc, r).and_then(Starter::side)
}

// ---------------------------------------------------------------- naming

/// The separator and the socket and plug side names connector `c` (or the project) uses; two
/// equal side names count as absent (the built-ins).
pub(crate) fn side_names(doc: &Document, c: Option<DeviceId>) -> (String, String, String) {
    use crate::defaults::connectors as dc;
    let scope = c.map_or_else(Scope::project, Scope::component);
    let sep = ecad_model::registry::resolve(doc, &dc::SEPARATOR, scope).text();
    let s = ecad_model::registry::resolve(doc, &dc::SIDE_SOCKET, scope).text();
    let p = ecad_model::registry::resolve(doc, &dc::SIDE_PLUG, scope).text();
    if s.eq_ignore_ascii_case(&p) {
        let b = |e: &ecad_model::registry::Entry| {
            ecad_model::registry::Resolved { value: e.builtin(), from: ecad_model::registry::Source::Builtin }.text()
        };
        return (sep, b(&dc::SIDE_SOCKET), b(&dc::SIDE_PLUG));
    }
    (sep, s, p)
}

/// The tag of half `side` of connector `c` (`-X10.S`); the connector's tag for `Both`.
pub fn half_tag(doc: &Document, c: DeviceId, side: Side) -> String {
    let tag = crate::devices::tags::tag_words(doc, c);
    let (sep, s, p) = side_names(doc, Some(c));
    match side {
        Side::Socket => format!("{tag}{sep}{s}"),
        Side::Plug => format!("{tag}{sep}{p}"),
        Side::Both => tag,
    }
}

/// The half tag a connector pin belongs to (`-X10.S`); `None` for any other terminal.
pub(crate) fn half_tag_of(doc: &Document, t: ConnectionPointId) -> Option<String> {
    let side = side_of(doc, t)?;
    let c = doc.connection_points().get(&t)?.device()?;
    Some(half_tag(doc, c, side))
}

/// A connector pin's full name, `-X10.S:3`; `None` for any other terminal.
pub fn pin_name(doc: &Document, t: ConnectionPointId) -> Option<String> {
    let half = half_tag_of(doc, t)?;
    let label = doc.connection_points().get(&t)?.label.clone()?;
    Some(format!("{half}:{label}"))
}

/// Whether a pin pair's plug-side label is hidden (a pin pair shows its pin name
/// once, beside the socket point).
pub(crate) fn hides_label(doc: &Document, r: &Representation, t: ConnectionPointId) -> bool {
    rep_half(doc, r).is_some_and(|s| s.key == StarterKey::Pinpair) && doc.connection_points().get(&t).and_then(|x| x.key.as_ref()) == Some(&PLUG_KEY)
}

/// The run class of a connector symbol for the "once per run" tag rule: one
/// class per half, so `-X10.S` and `-X10.P` each show once per run.
pub(crate) fn run_class(doc: &Document, r: &Representation) -> Option<u64> {
    rep_side(doc, r).map(|s| match s {
        Side::Socket => u64::MAX - 1,
        Side::Plug => u64::MAX - 2,
        Side::Both => u64::MAX - 3,
    })
}

/// A typed connector name: `-X10.S`, `-X10.P:3`, or `-X10:3` on a connector.
/// `None` when `text` names no connector; `Some(Err)` when it names one ambiguously.
///
/// # Errors
/// `-X10:3` on a connector with pin 3 on both halves, and a pin the half does not have.
pub fn resolve(doc: &Document, text: &str) -> Option<Result<(DeviceId, Option<Side>, Option<ConnectionPointId>), CmdError>> {
    let text: String = text.trim().chars().take(64).collect();
    let (head, pin) = match text.split_once(':') {
        Some((h, p)) => (h.to_string(), Some(p.to_string())),
        None => (text.clone(), None),
    };
    // The connector and side: a tag as is, else a tag followed by the separator and a side name.
    let found = doc.devices().keys().copied().find_map(|c| {
        if !connector_of(doc, c).is_some_and(|x| x.mating) {
            return None;
        }
        // A provisional connector is named with its `?`, which no half name carries.
        let t = crate::devices::tags::tag_of(doc, c).filter(|_| doc.devices().get(&c).is_some_and(|d| !d.is_provisional()))?;
        if t.eq_ignore_ascii_case(&head) {
            return Some((c, None));
        }
        let rest = head.get(..t.len()).filter(|h| h.eq_ignore_ascii_case(t)).and_then(|_| head.get(t.len()..))?;
        let (sep, s, p) = side_names(doc, Some(c));
        let name = rest.strip_prefix(sep.as_str())?;
        if name.eq_ignore_ascii_case(&s) {
            Some((c, Some(Side::Socket)))
        } else if name.eq_ignore_ascii_case(&p) {
            Some((c, Some(Side::Plug)))
        } else {
            None
        }
    });
    let (c, side) = found?;
    let Some(pin) = pin else { return Some(Ok((c, side, None))) };
    let pin = pin.trim().to_string();
    let with =
        |want: Side| doc.connection_points_of(c).find(|(_, x)| x.label.as_deref() == Some(pin.as_str()) && x.key == want.key()).map(|(t, _)| t);
    Some(match side {
        Some(s) => with(s).map(|t| (c, Some(s), Some(t))).ok_or_else(|| CmdError::refused(format!("{} has no pin {pin}.", half_tag(doc, c, s)))),
        None => match (with(Side::Socket), with(Side::Plug)) {
            (Some(_), Some(_)) => {
                let tag = crate::devices::tags::tag_words(doc, c);
                Err(CmdError::refused(format!(
                    "{tag} has pin {pin} on both halves. Type {}:{pin} or {}:{pin}.",
                    half_tag(doc, c, Side::Socket),
                    half_tag(doc, c, Side::Plug)
                )))
            }
            (Some(t), None) => Ok((c, Some(Side::Socket), Some(t))),
            (None, Some(t)) => Ok((c, Some(Side::Plug), Some(t))),
            (None, None) => Err(CmdError::refused(format!("{} has no pin {pin}.", crate::devices::tags::tag_words(doc, c)))),
        },
    })
}

// ---------------------------------------------------------------- pins

/// The pins a connector has drawn: per side, pin name → its terminals (more than one: drawn twice).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Drawn {
    pub socket: BTreeMap<String, Vec<ConnectionPointId>>,
    pub plug: BTreeMap<String, Vec<ConnectionPointId>>,
    /// Every pin name, in name order (natural order for numbers, else the order drawn).
    pub order: Vec<String>,
}

impl Drawn {
    /// The pins of `side` (`Both`: none).
    pub fn side(&self, side: Side) -> Option<&BTreeMap<String, Vec<ConnectionPointId>>> {
        match side {
            Side::Socket => Some(&self.socket),
            Side::Plug => Some(&self.plug),
            Side::Both => None,
        }
    }

    /// Whether pin `name` is drawn on `side`.
    pub fn has(&self, side: Side, name: &str) -> bool {
        self.side(side).is_some_and(|m| m.contains_key(name))
    }
}

/// The pins drawn on connector `c`: O(its terminals).
pub fn drawn(doc: &Document, c: DeviceId) -> Drawn {
    let mut d = Drawn::default();
    let mut first: BTreeMap<String, ConnectionPointId> = BTreeMap::new();
    for (t, x) in doc.connection_points_of(c) {
        let (Some(label), Some(side)) = (x.label.clone(), x.key.as_ref().and_then(Side::of_key)) else { continue };
        let e = first.entry(label.clone()).or_insert(t);
        if t < *e {
            *e = t;
        }
        match side {
            Side::Socket => d.socket.entry(label).or_default().push(t),
            Side::Plug => d.plug.entry(label).or_default().push(t),
            Side::Both => {}
        }
    }
    let mut order: Vec<(String, ConnectionPointId)> = first.into_iter().collect();
    if order.iter().all(|(n, _)| n.parse::<u64>().is_ok()) {
        order.sort_by_key(|(n, _)| n.parse::<u64>().unwrap_or(u64::MAX));
    } else {
        order.sort_by_key(|(_, t)| *t);
    }
    // A typed list gives the pin order.
    if let Some(list) = pin_names(doc, c) {
        order.sort_by_key(|(n, _)| list.iter().position(|x| x == n).unwrap_or(usize::MAX));
    }
    d.order = order.into_iter().map(|(n, _)| n).collect();
    d
}

/// Connector `c`'s typed list of pin names, when it holds a valid one.
pub fn pin_names(doc: &Document, c: DeviceId) -> Option<Vec<String>> {
    let names: Vec<String> = connector_of(doc, c)?.pin_names.iter().map(ToString::to_string).collect();
    (!names.is_empty()).then_some(names)
}

/// Connector `c`'s `pins` property, when it holds a count from 1 to [`MAX_PINS`].
pub fn pins_prop(doc: &Document, c: DeviceId) -> Option<usize> {
    connector_of(doc, c)?.pins.map(|n| usize::from(n.get()))
}

/// The whole-half symbols of connector `c`: (representation, side, starter), in ID order.
pub(crate) fn whole_halves(doc: &Document, c: DeviceId) -> Vec<(RepresentationId, Side, &'static Starter)> {
    doc.representations_of(Subject::Device(c))
        .filter_map(|(rid, r)| {
            let s = rep_half(doc, r).filter(|s| !is_pin_starter(s))?;
            Some((rid, s.side()?, s))
        })
        .collect()
}

/// How many pins connector `c` has: `pins`, else the largest whole half drawn,
/// else the number of distinct pins drawn.
pub fn pin_count(doc: &Document, c: DeviceId, d: &Drawn) -> usize {
    if let Some(n) = pins_prop(doc, c) {
        return n;
    }
    whole_halves(doc, c).iter().filter_map(|(_, _, s)| size_of(s)).max().unwrap_or(d.order.len())
}

/// How pins are named.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Names {
    Numbers,
    Letters,
    List(Vec<String>),
}

/// Whether `name` is a valid pin name.
///
/// # Errors
/// Empty, too long, a space, comma, colon or control character.
pub fn check_name(name: &str) -> Result<(), CmdError> {
    let shown: String = name.chars().take(16).filter(|c| !c.is_control()).collect();
    if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
        return Err(CmdError::refused(format!("A pin name is 1 to {MAX_NAME_CHARS} characters: \"{shown}\".")));
    }
    if name.chars().any(|c| c.is_whitespace() || c == ',' || c == ':' || c.is_control()) {
        return Err(CmdError::refused(format!("Pin names cannot contain spaces, commas or colons: \"{shown}\".")));
    }
    Ok(())
}

/// Parse a typed list of pin names (`L,N,PE`).
///
/// # Errors
/// Too long, a bad name, a repeated name, more than [`MAX_PINS`].
pub fn parse_list(text: &str) -> Result<Vec<String>, CmdError> {
    if text.chars().count() > MAX_LIST_CHARS {
        return Err(CmdError::refused(format!("A pin name list is at most {MAX_LIST_CHARS} characters.")));
    }
    let names: Vec<String> = text.split(',').map(|n| n.trim().to_string()).collect();
    check_list(&names)?;
    Ok(names)
}

/// Check a list of pin names: each valid, all different, at most [`MAX_PINS`].
///
/// # Errors
/// As [`parse_list`].
pub(crate) fn check_list(names: &[String]) -> Result<(), CmdError> {
    if names.len() > MAX_PINS {
        return Err(CmdError::refused(format!("A connector has at most {MAX_PINS} pins; got {}.", names.len())));
    }
    let mut seen = BTreeSet::new();
    for n in names {
        check_name(n)?;
        if !seen.insert(n.to_ascii_lowercase()) {
            return Err(CmdError::refused(format!("Pin {n} is named twice. Give each pin its own name.")));
        }
    }
    Ok(())
}

/// The names of `n` pins named `names`.
///
/// # Errors
/// Letters past 26, a list whose count is not `n`.
pub fn names_for(names: &Names, n: usize) -> Result<Vec<String>, CmdError> {
    if n == 0 || n > MAX_PINS {
        return Err(CmdError::refused(format!("A connector has 1 to {MAX_PINS} pins.")));
    }
    match names {
        Names::Numbers => Ok((1..=n).map(|i| i.to_string()).collect()),
        Names::Letters => {
            if n > 26 {
                return Err(CmdError::refused("Letters name at most 26 pins. Use Numbers or a list."));
            }
            Ok((0..n).filter_map(|i| u8::try_from(i).ok().map(|i| char::from(b'A' + i).to_string())).collect())
        }
        Names::List(v) => {
            if v.len() != n {
                return Err(CmdError::refused(format!("Give {n} pin names; got {}.", v.len())));
            }
            check_list(v)?;
            Ok(v.clone())
        }
    }
}

/// The pins connector `c` offers, in name order: those drawn, then the ones its pin count and
/// naming imply (numbers 1…n, or letters A…).
fn candidates(doc: &Document, c: DeviceId, d: &Drawn) -> Vec<String> {
    // A typed list: its names in order, then any other pin drawn.
    if let Some(mut list) = pin_names(doc, c) {
        for x in &d.order {
            if !list.contains(x) {
                list.push(x.clone());
            }
        }
        return list;
    }
    let n = pin_count(doc, c, d).min(MAX_PINS);
    let numbers = d.order.iter().all(|x| x.parse::<u64>().is_ok());
    let letters = !d.order.is_empty() && d.order.iter().all(|x| x.len() == 1 && x.bytes().all(|b| b.is_ascii_uppercase()));
    let mut out = d.order.clone();
    let implied: Vec<String> = if numbers {
        (1..=n).map(|i| i.to_string()).collect()
    } else if letters {
        names_for(&Names::Letters, n.min(26)).unwrap_or_default()
    } else {
        Vec::new()
    };
    for x in implied {
        if !out.contains(&x) {
            out.push(x);
        }
    }
    if numbers {
        out.sort_by_key(|x| x.parse::<u64>().unwrap_or(u64::MAX));
    } else if letters {
        out.sort();
    }
    out
}

/// The pin a pin symbol of `side` takes on connector `c`; `taken` are pins this
/// transaction already gave, per side.
///
/// # Errors
/// A connector named by letters or a list whose pins are all drawn on that side; more than
/// [`MAX_PINS`] pins.
pub fn next_pin(doc: &Document, c: DeviceId, side: Side, taken: &BTreeSet<(Side, String)>) -> Result<String, CmdError> {
    let d = drawn(doc, c);
    let on = |s: Side, n: &str| d.has(s, n) || taken.contains(&(s, n.to_string()));
    let free = |n: &str| match side {
        Side::Both => !on(Side::Socket, n) && !on(Side::Plug, n),
        s => !on(s, n),
    };
    let cands = candidates(doc, c, &d);
    let mut all: Vec<String> = cands.clone();
    for (_, n) in taken {
        if !all.contains(n) {
            all.push(n.clone());
        }
    }
    // 1. Mate first.
    if side != Side::Both
        && let Some(n) = all.iter().find(|n| free(n) && on(side.other(), n))
    {
        return Ok(n.clone());
    }
    // 2. The next free pin.
    if let Some(n) = cands.iter().find(|n| free(n)) {
        return Ok(n.clone());
    }
    // 3. A new pin.
    let numbers = all.iter().all(|x| x.parse::<u64>().is_ok());
    if numbers {
        let next = all.iter().filter_map(|x| x.parse::<u64>().ok()).max().unwrap_or(0) + 1;
        if next > MAX_PINS as u64 {
            return Err(CmdError::refused(format!("{} has {MAX_PINS} pins, the most a connector has.", crate::devices::tags::tag_words(doc, c))));
        }
        return Ok(next.to_string());
    }
    let tag = crate::devices::tags::tag_words(doc, c);
    let where_ = match side {
        Side::Both => "both halves".to_string(),
        s => format!("the {} half", s.word()),
    };
    Err(CmdError::refused(format!(
        "{tag} has {} named pins ({}), all drawn on {where_}. Add a name with pinlabel, or set pins.",
        all.len(),
        all.iter().take(8).cloned().collect::<Vec<_>>().join(", ")
    )))
}

// ---------------------------------------------------------------- halves and their symbols

/// Where a representation is, in words (`101 B3`, the zone's letter first), for refusals.
pub fn where_words(doc: &Document, rid: RepresentationId) -> String {
    let Some(r) = doc.representations().get(&rid) else { return String::new() };
    let page = ecad_model::document::book::page_number(doc, r.page);
    match crate::references::devices::classify::anchor(doc, r).and_then(|a| ecad_model::connectivity::wiring::place(doc, r.page, a)) {
        Some(p) => format!("{page} {}", ecad_model::connectivity::derive::zone_text(&p.column_label, &p.row_label)),
        None => page,
    }
}

/// Where go-to lands for half `side` ([`crate::symbols::reps::goto`]): its whole-half
/// symbol, else its first pin symbol in book order, then top-left; `None` when the half is not
/// drawn. Display only: it decides nothing about the half.
pub fn half_goto(doc: &Document, c: DeviceId, side: Side) -> Option<RepresentationId> {
    crate::symbols::reps::goto(doc, c, Some(side))
}

/// The family of half `side`: its whole-half symbol's, else the connector's definition's (a
/// pin's: none, then `Plain`).
pub fn half_family(doc: &Document, c: DeviceId, side: Side) -> ConnectorFamily {
    if let Some(f) = whole_halves(doc, c).into_iter().find(|(_, s, _)| *s == side).and_then(|(_, _, s)| family_of(s)) {
        return f;
    }
    connector_of(doc, c).map_or(ConnectorFamily::Plain, |x| x.family)
}

// ---------------------------------------------------------------- planning

/// One connection point a planned symbol shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Shown {
    pub side: Side,
    pub name: String,
    pub connection_point: ConnectionPointId,
    /// Its point in the definition's frame.
    pub local: Pt,
}

/// Plan one connector symbol of `starter` on connector `c`, its pins named `names` (a whole half:
/// one name per point; a pin: one name): one terminal per point keyed by its side, shown through
/// explicit connection points. Its role is the part its starter
/// declares.
///
/// # Errors
/// Fewer names than points; the definition could not be built.
#[allow(clippy::too_many_arguments)]
pub fn symbol_ops(
    ids: &mut IdAllocator, page: PageId, style: Style, def: DefinitionId, starter: &Starter, c: DeviceId, names: &[String], placement: Placement,
    ops: &mut Vec<Op>,
) -> Result<(RepresentationId, ItemId, Vec<Shown>), CmdError> {
    let d = starter.definition()?;
    let (item, rep): (ItemId, RepresentationId) = (ids.fresh(), ids.fresh());
    let mut cps = BTreeMap::new();
    let mut shown = Vec::new();
    let pin = is_pin_starter(starter);
    for (i, td) in d.connection_points.iter().enumerate() {
        let side = match starter.side() {
            Some(Side::Both) | None => Side::of_key(&td.key).ok_or(CmdError::refused("A connector symbol's point has no side."))?,
            Some(s) => s,
        };
        let name = if pin { names.first() } else { names.get(i) }.ok_or(CmdError::refused("Not enough pin names for this symbol."))?;
        let t: ConnectionPointId = ids.fresh();
        let key = side.key().ok_or(CmdError::refused("A connector symbol's point has no side."))?;
        ops.push(Op::AddConnectionPoint {
            id: t,
            connection_point: ConnectionPoint { owner: c.into(), label: Some(name.clone()), key: Some(key), accepts: td.accepts },
        });
        cps.insert(t, td.at);
        shown.push(Shown { side, name: name.clone(), connection_point: t, local: td.at });
    }
    ops.push(Op::AddItem { id: item, item: Item { page, style, kind: ItemKind::Symbol(SymbolInstance { definition: def, placement }) } });
    ops.push(Op::LinkRepresentation {
        id: rep,
        rep: Representation {
            subject: Subject::Device(c),
            page,
            form: Form::Symbol,
            role: ecad_model::Role::Part(starter.part_role()),
            items: vec![item],
            connection_points: cps,
        },
    });
    Ok((rep, item, shown))
}

/// Plan a new connector made by `starter` (its definition `def`): `CreateDevice` with its tag
/// (`tag`, else the next automatic one), the registry description and `pins`.
///
/// # Errors
/// No tag number left.
pub fn create_ops(
    ledger: &Ledger, ids: &mut IdAllocator, high: &mut crate::devices::tags::TagHigh, starter: &Starter, def: DefinitionId,
    tag: Option<ecad_model::naming::tag::Entry>, pins: Option<usize>, names: Option<&[String]>, ops: &mut Vec<Op>,
) -> Result<(DeviceId, String), CmdError> {
    let doc = ledger.document();
    let tag = high.given(ledger, tag, &crate::devices::prefix_for(doc, starter))?;
    let c: DeviceId = ids.fresh();
    let description = ecad_model::registry::resolve(doc, &defaults::connectors::DESCRIPTION, Scope::project()).text();
    let mut device_type = starter.device_type();
    if let DeviceType::Connector(x) = &mut device_type {
        x.pins = pins.and_then(|n| u8::try_from(n).ok()).and_then(ecad_model::objects::devices::PinCount::new);
        x.pin_names = typed_names(names.unwrap_or_default())?;
    }
    ops.push(Op::CreateDevice {
        id: c,
        device: crate::devices::with_given(
            Device { description: crate::devices::description_of(&description), definition: Some(def), device_type, ..Device::default() },
            &tag,
        )?,
    });
    Ok((c, tag.shown()))
}

/// The refusal for joining a device that is not a connector.
pub fn not_a_connector(doc: &Document, c: DeviceId) -> String {
    format!("{} is not a connector. Pick a connector, or Enter for a new one.", crate::devices::tags::tag_words(doc, c))
}

/// Check that a whole half of `side` may join connector `c`.
///
/// # Errors
/// The half already has pins.
pub fn check_whole_half(doc: &Document, c: DeviceId, side: Side, taken: &BTreeSet<(Side, String)>) -> Result<(), CmdError> {
    let d = drawn(doc, c);
    let has = d.side(side).is_some_and(|m| !m.is_empty()) || taken.iter().any(|(s, _)| *s == side);
    if !has {
        return Ok(());
    }
    let at = half_goto(doc, c, side).map(|r| format!(" ({})", where_words(doc, r))).unwrap_or_default();
    Err(CmdError::refused(format!(
        "{} already has a {} half{at}. Place a {} half, or Enter for a new connector.",
        crate::devices::tags::tag_words(doc, c),
        side.word(),
        side.other().word()
    )))
}

/// Check that pin `name` may be drawn on `side` of connector `c`.
///
/// # Errors
/// The pin is drawn on that side (either side for a pin pair).
pub fn check_named_pin(doc: &Document, c: DeviceId, side: Side, name: &str, taken: &BTreeSet<(Side, String)>) -> Result<(), CmdError> {
    check_name(name)?;
    let d = drawn(doc, c);
    let sides: &[Side] = match side {
        Side::Both => &[Side::Socket, Side::Plug],
        Side::Socket => &[Side::Socket],
        Side::Plug => &[Side::Plug],
    };
    for s in sides {
        if taken.contains(&(*s, name.to_string())) {
            return Err(CmdError::refused(format!("{}:{name} is already being drawn. Pick another pin.", half_tag(doc, c, *s))));
        }
        if let Some(t) = d.side(*s).and_then(|m| m.get(name)).and_then(|v| v.first()) {
            let at = doc.representations_showing(*t).next().map(|(rid, _)| where_words(doc, rid)).unwrap_or_default();
            return Err(CmdError::refused(format!(
                "{}:{name} is drawn on {at}. Pick another pin, or Enter for the next free one.",
                half_tag(doc, c, *s)
            )));
        }
    }
    Ok(())
}

/// Plan landing a connector pin symbol, placed at `on.at`, on its route: a pin pair splits the route, the part towards
/// `.s` ending on `.s` and the other on `.p`. A one-side pin takes the part towards its point; the other part ends
/// free. The commit's rewrite makes the wires.
///
/// # Errors
/// [`crate::connectors::plan_land`]'s refusals.
pub fn land_pin(
    doc: &Document, ids: &mut IdAllocator, on: crate::wires::placing::OnRoute, shown: &[Shown], placement: Placement,
) -> Result<crate::connectors::Land, CmdError> {
    let first = shown.first().ok_or(CmdError::refused("The symbol has no connection point."))?;
    let before = doc.routes().get(&on.route).and_then(|r| r.points.get(on.segment).copied());
    let (a, b) = match (shown.iter().find(|x| x.side == Side::Socket), shown.iter().find(|x| x.side == Side::Plug)) {
        (Some(s), Some(p)) if shown.len() == 2 => {
            let (ps, pp) = (placement.apply(s.local), placement.apply(p.local));
            let keep_s = before.is_none_or(|b| b.dist(ps) <= b.dist(pp));
            let (keep_t, keep_p, rest_t, rest_p) =
                if keep_s { (s.connection_point, ps, p.connection_point, pp) } else { (p.connection_point, pp, s.connection_point, ps) };
            ((RouteEnd::Point(keep_t), keep_p), (RouteEnd::Point(rest_t), rest_p))
        }
        _ => {
            let pt = placement.apply(first.local);
            let away = Pt::new(2.0 * placement.origin.x - pt.x, 2.0 * placement.origin.y - pt.y);
            let keep_attached = before.is_none_or(|b| b.dist(pt) <= b.dist(away));
            if keep_attached {
                ((RouteEnd::Point(first.connection_point), pt), (RouteEnd::Free, on.at))
            } else {
                ((RouteEnd::Free, on.at), (RouteEnd::Point(first.connection_point), pt))
            }
        }
    };
    crate::connectors::plan_land_ends(doc, ids, on, a, b)
}

// ---------------------------------------------------------------- properties

/// The value `setprop` stores for a connector's naming default on device `c`, or `None` when `c` is
/// not a connector or the key is not one (its pins, pin names, halves and ignored rows are fields:
/// [`with_field`]).
///
/// # Errors
/// A pin count out of range, an unknown side letter, a value the rules refuse.
pub(crate) fn setprop_value(doc: &Document, c: DeviceId, key: &str, value: Option<&str>) -> Option<Result<Option<String>, CmdError>> {
    if crate::defaults::connectors::is_naming_key(key) {
        if !connector_of(doc, c).is_some_and(|x| x.mating) {
            return None;
        }
        return crate::defaults::connectors::setprop_value(key, value);
    }
    None
}

/// A half's own value of `field` (one of [`HALF_FIELDS`]).
pub fn half_prop(doc: &Document, c: DeviceId, side: Side, field: &str) -> Option<String> {
    let plug = match side {
        Side::Socket => false,
        Side::Plug => true,
        Side::Both => return None,
    };
    half_field(connector_of(doc, c)?.half(plug), field).map(str::to_string)
}

/// The value of `field` (`description`, `maker`, `part_number`) of half part `h`.
pub(crate) fn half_field<'a>(h: &'a HalfPart, field: &str) -> Option<&'a str> {
    match field {
        "description" => h.description.as_ref().map(|t| t.as_str()),
        "maker" => h.manufacturer.as_ref().map(|t| t.as_str()),
        "part_number" => h.part_number.as_ref().map(|t| t.as_str()),
        _ => None,
    }
}

/// Pin names as stored: each checked ([`check_list`]) and validated text.
///
/// # Errors
/// As [`check_list`].
pub(crate) fn typed_names(names: &[String]) -> Result<Vec<ecad_model::objects::devices::PinName>, CmdError> {
    check_list(names)?;
    names.iter().map(|n| ecad_model::objects::devices::PinName::new(n).map_err(|e| CmdError::refused(format!("Pin name {n}: {e}")))).collect()
}

/// Whether `key` is one of a connector's own fields' `setprop` keys: `pins`, `pin_names`,
/// `half.<s|p>.<field>`, `ignore.<row>`. (The retired `half.<s|p>.location` is the location
/// override `location.socket` / `location.plug`: [`crate::locations::commands::locations::aspect_property`].)
pub(crate) fn is_field_key(key: &str) -> bool {
    key == PINS_KEY || key == PIN_NAMES_KEY || (key.starts_with(HALF_PREFIX) && !key.ends_with(".location")) || key.starts_with(IGNORE_PREFIX)
}

/// Connector `d` (named `tag` in refusals) with the field `setprop`'s `key` names set to `value`
/// (`None`, or blank, clears it).
///
/// # Errors
/// Not a connector; a pin count out of range; a bad pin name list; an unknown side, half field or
/// row; text a field refuses.
pub fn with_field(d: &Device, tag: &str, key: &str, value: Option<&str>) -> Result<Device, CmdError> {
    let mut out = d.clone();
    let DeviceType::Connector(x) = &mut out.device_type else {
        return Err(CmdError::refused(format!("{tag} is not a connector: `{key}` is a connector's.")));
    };
    let value = value.map(str::trim).filter(|v| !v.is_empty());
    if key == PINS_KEY {
        x.pins = match value {
            None => None,
            Some(v) => Some(
                v.parse::<u8>()
                    .ok()
                    .filter(|n| (1..=ecad_model::objects::devices::MAX_PINS).contains(n))
                    .and_then(ecad_model::objects::devices::PinCount::new)
                    .ok_or_else(|| CmdError::refused(format!("`pins` is a whole number from 1 to {MAX_PINS}.")))?,
            ),
        };
    } else if key == PIN_NAMES_KEY {
        x.pin_names = match value {
            None => Vec::new(),
            Some(v) => typed_names(&parse_list(v)?)?,
        };
    } else if let Some(rest) = key.strip_prefix(HALF_PREFIX) {
        let (letter, field) = rest.split_once('.').unwrap_or((rest, ""));
        let plug = match letter {
            "s" => false,
            "p" => true,
            _ => {
                let shown: String = letter.chars().take(8).filter(|c| !c.is_control()).collect();
                return Err(CmdError::refused(format!("{tag} has no half \"{shown}\". Use half.s.* or half.p.*.")));
            }
        };
        let h = x.half_mut(plug);
        let text = |max: usize| CmdError::refused(format!("`{key}` is one line of at most {max} characters, with no control or hidden characters."));
        match field {
            "description" => h.description = value.map(ecad_model::objects::devices::Description::new).transpose().map_err(|_| text(256))?,
            "maker" => h.manufacturer = value.map(ecad_model::objects::devices::PartText::new).transpose().map_err(|_| text(64))?,
            "part_number" => h.part_number = value.map(ecad_model::objects::devices::PartText::new).transpose().map_err(|_| text(64))?,
            _ => {
                return Err(CmdError::refused(format!("A half's field is one of {}: half.{letter}.description.", HALF_FIELDS.join(", "))));
            }
        }
    } else if let Some(row) = key.strip_prefix(IGNORE_PREFIX) {
        let words = <ConnectorCheck as ecad_model::Word>::WORDS.join(" or ");
        let k = <ConnectorCheck as ecad_model::Word>::parse(row).ok_or_else(|| {
            let shown: String = row.chars().take(32).filter(|c| !c.is_control()).collect();
            CmdError::refused(format!("A connector ignores {words}, not \"{shown}\"."))
        })?;
        let on = match value {
            None => false,
            Some(v) => ecad_model::naming::words::flag(v).ok_or_else(|| CmdError::refused(format!("`{key}` is yes or no.")))?,
        };
        if on {
            x.ignored.insert(k);
        } else {
            x.ignored.remove(&k);
        }
    } else {
        return Err(CmdError::refused(format!("`{key}` is not a connector's field.")));
    }
    Ok(out)
}

// ---------------------------------------------------------------- BOM

/// One drawn half as the BOM sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct HalfRow {
    pub side: Side,
    pub tag: String,
    pub description: Option<String>,
    pub maker: Option<String>,
    pub part_number: Option<String>,
    pub location: Located,
    /// Where its go-to lands ([`half_goto`]) and that symbol's page.
    pub goto: Option<(RepresentationId, PageId)>,
}

/// The pin count of half `side`: its whole-half size, else the connector's pin count.
pub(crate) fn half_size(doc: &Document, c: DeviceId, side: Side, d: &Drawn) -> usize {
    whole_halves(doc, c).into_iter().find(|(_, s, _)| *s == side).and_then(|(_, _, s)| size_of(s)).unwrap_or_else(|| pin_count(doc, c, d))
}

/// The BOM description a half gets from the registry (`D-sub socket, 9-way`).
pub(crate) fn default_description(doc: &Document, family: ConnectorFamily, side: Side, n: usize) -> String {
    let fam = match family {
        ConnectorFamily::Dsub | ConnectorFamily::M12 => family,
        _ => ConnectorFamily::Plain,
    };
    let Some(e) = crate::defaults::connectors::description_entry(fam.word(), side.word()) else { return String::new() };
    ecad_model::registry::resolve(doc, e, Scope::project()).text().replace("{n}", &n.to_string())
}

/// One row per drawn half of connector `c`: its half tag, own fields or the
/// registry's description, and its location. Empty for a connector with nothing drawn.
pub fn half_rows(doc: &Document, c: DeviceId, _cache: &mut BoxCache) -> Vec<HalfRow> {
    if !connector_of(doc, c).is_some_and(|x| x.mating) {
        return Vec::new();
    }
    let d = drawn(doc, c);
    let mut out = Vec::new();
    for side in [Side::Socket, Side::Plug] {
        let Some(main) = half_goto(doc, c, side) else { continue };
        let description = half_prop(doc, c, side, "description").or_else(|| {
            let v = default_description(doc, half_family(doc, c, side), side, half_size(doc, c, side, &d));
            (!v.is_empty()).then_some(v)
        });
        // The half's override, else the connector's stored location.
        let location = crate::locations::half_location(doc, c, side);
        out.push(HalfRow {
            side,
            tag: half_tag(doc, c, side),
            description,
            maker: half_prop(doc, c, side, "maker"),
            part_number: half_prop(doc, c, side, "part_number"),
            location,
            goto: doc.representations().get(&main).map(|r| (main, r.page)),
        });
    }
    out
}

// ---------------------------------------------------------------- checks

/// Where terminal `t` is drawn: the page and anchor of the first symbol showing it.
fn connection_point_at(doc: &Document, t: ConnectionPointId) -> (Option<PageId>, Option<Pt>, Option<RepresentationId>) {
    match doc.representations_showing(t).next() {
        Some((rid, r)) => (Some(r.page), crate::references::devices::classify::anchor(doc, r), Some(rid)),
        None => (None, None, None),
    }
}

fn rep_at(doc: &Document, rid: Option<RepresentationId>) -> (Option<PageId>, Option<Pt>) {
    match rid.and_then(|r| doc.representations().get(&r)) {
        Some(r) => (Some(r.page), crate::references::devices::classify::anchor(doc, r)),
        None => (None, None),
    }
}

/// The Checks rows of connector `c`; none for any other component.
pub fn checks(doc: &Document, c: DeviceId) -> Vec<Check> {
    if !connector_of(doc, c).is_some_and(|x| x.mating) {
        return Vec::new();
    }
    let d = drawn(doc, c);
    let tag = crate::devices::tags::tag_words(doc, c);
    let mut out = Vec::new();
    // A pin drawn twice on one side.
    for side in [Side::Socket, Side::Plug] {
        let Some(m) = d.side(side) else { continue };
        for (name, ts) in m {
            let mut ts = ts.clone();
            ts.sort();
            if let Some(second) = ts.get(1).copied() {
                let (page, at, _) = connection_point_at(doc, second);
                out.push(Check {
                    kind: CheckKind::ConnectorPinTwice,
                    page,
                    at,
                    object: Object::ConnectionPoint(second),
                    which: format!("{}:{name} is drawn twice", half_tag(doc, c, side)),
                    fix: Some(Fix::ConnectorNextPin(second)),
                });
            }
        }
    }
    // Family and size of the whole halves.
    let wholes = whole_halves(doc, c);
    let half = |side: Side| wholes.iter().find(|(_, s, _)| *s == side);
    if let (Some((_, _, s)), Some((rp, _, p))) = (half(Side::Socket), half(Side::Plug)) {
        let (fs, fp) = (family_of(s), family_of(p));
        let (page, at) = rep_at(doc, Some(*rp));
        if fs != fp && !ignores(doc, c, CheckKind::ConnectorFamily) {
            let words = |f: Option<ConnectorFamily>, side: Side| format!("{} {}", f.map_or("Plain", ConnectorFamily::name), side.word());
            out.push(Check {
                kind: CheckKind::ConnectorFamily,
                page,
                at,
                object: Object::Device(c),
                which: format!("{tag}'s halves differ: {}, {}", words(fs, Side::Socket), words(fp, Side::Plug)),
                fix: Some(Fix::IgnoreConnector(c, CheckKind::ConnectorFamily)),
            });
        }
        if let (Some(ns), Some(np)) = (size_of(s), size_of(p))
            && ns != np
            && !ignores(doc, c, CheckKind::ConnectorSize)
        {
            out.push(Check {
                kind: CheckKind::ConnectorSize,
                page,
                at,
                object: Object::Device(c),
                which: format!("{} has {ns} pins, {} has {np}", half_tag(doc, c, Side::Socket), half_tag(doc, c, Side::Plug)),
                fix: Some(Fix::IgnoreConnector(c, CheckKind::ConnectorSize)),
            });
        }
    }
    // Lone pins, only when both halves have pins.
    if !d.socket.is_empty() && !d.plug.is_empty() {
        let (_, s, p) = side_names(doc, Some(c));
        let mut lone: Vec<(String, ConnectionPointId)> = Vec::new();
        let mut lacking = None;
        for name in &d.order {
            match (d.socket.get(name), d.plug.get(name)) {
                (Some(ts), None) => {
                    lone.extend(ts.first().map(|t| (format!("{s}:{name}"), *t)));
                    lacking.get_or_insert(Side::Plug);
                }
                (None, Some(ts)) => {
                    lone.extend(ts.first().map(|t| (format!("{p}:{name}"), *t)));
                    lacking.get_or_insert(Side::Socket);
                }
                _ => {}
            }
        }
        if let Some((_, first)) = lone.first() {
            let (page, at, _) = connection_point_at(doc, *first);
            let n = lone.len();
            let names: Vec<String> = lone.iter().take(8).map(|(w, _)| w.clone()).collect();
            let more = if n > 8 { ", …" } else { "" };
            out.push(Check {
                kind: CheckKind::ConnectorLonePin,
                page,
                at,
                object: Object::Device(c),
                which: format!(
                    "{tag}: {n} pin{} {} no mate ({}{more})",
                    if n == 1 { "" } else { "s" },
                    if n == 1 { "has" } else { "have" },
                    names.join(", ")
                ),
                fix: lacking.map(|side| Fix::PlaceMates(c, side)),
            });
        }
    }
    // A numbered pin beyond `pins`.
    if let Some(n) = pins_prop(doc, c) {
        let over = d.order.iter().filter_map(|x| x.parse::<usize>().ok().map(|v| (v, x))).filter(|(v, _)| *v > n).max_by_key(|(v, _)| *v);
        if let Some((v, name)) = over {
            let t = d.socket.get(name).or_else(|| d.plug.get(name)).and_then(|ts| ts.first()).copied();
            let (page, at, _) = t.map_or((None, None, None), |t| connection_point_at(doc, t));
            out.push(Check {
                kind: CheckKind::ConnectorOver,
                page,
                at,
                object: Object::Device(c),
                which: format!("{tag} has pin {v} beyond its {n} pins"),
                fix: u32::try_from(v).ok().map(|v| Fix::SetPins(c, v)),
            });
        }
    }
    out
}

// ---------------------------------------------------------------- references

/// The reference members of connector `c`: each half's
/// whole-half symbol ([`crate::symbols::reps::half_anchor`]) is a half of that side, every other connector
/// symbol a pin of its side (a pin pair: `Both`). A pin-only half has no half member: its pins
/// keep their mate edges.
pub fn ref_classes(doc: &Document, c: DeviceId) -> Vec<(RepresentationId, crate::references::devices::classify::Class)> {
    use crate::references::devices::classify::Class;
    if !connector_of(doc, c).is_some_and(|x| x.mating) {
        return Vec::new();
    }
    let halves: BTreeMap<RepresentationId, Side> =
        [Side::Socket, Side::Plug].into_iter().filter_map(|s| crate::symbols::reps::half_anchor(doc, c, s).map(|r| (r, s))).collect();
    doc.representations_of(Subject::Device(c))
        .filter_map(|(rid, r)| {
            let side = rep_side(doc, r)?;
            Some(match halves.get(&rid) {
                Some(s) => (rid, Class::Half(*s)),
                None => (rid, Class::Pin(side)),
            })
        })
        .collect()
}

/// The edges between connector `c`'s members: each half's whole-half symbol shows
/// the other half's, and a pin whose mate is drawn only on other pages shows those.
pub fn ref_edges(
    doc: &Document, c: DeviceId, list: &[(RepresentationId, crate::references::devices::classify::Class)],
) -> Vec<(RepresentationId, RepresentationId)> {
    use crate::references::devices::classify::Class;
    let mut out = Vec::new();
    let half = |want: Side| list.iter().find(|(_, k)| *k == Class::Half(want)).map(|(r, _)| *r);
    if let (Some(a), Some(b)) = (half(Side::Socket), half(Side::Plug))
        && a != b
    {
        out.push((a, b));
        out.push((b, a));
    }
    for (rid, k) in list {
        let Class::Pin(side) = k else { continue };
        if *side == Side::Both {
            continue;
        }
        let Some(r) = doc.representations().get(rid) else { continue };
        let mut mates: BTreeSet<RepresentationId> = BTreeSet::new();
        let mut here = false;
        for t in r.connection_points.keys() {
            let Some(x) = doc.connection_points().get(t) else { continue };
            let other = side.other().key();
            for (m, mx) in doc.connection_points_of(c) {
                if mx.label != x.label || mx.key != other {
                    continue;
                }
                for (mr, mrep) in doc.representations_showing(m) {
                    if mrep.page == r.page {
                        here = true;
                    } else {
                        mates.insert(mr);
                    }
                }
            }
        }
        if !here {
            out.extend(mates.into_iter().filter(|m| m != rid).map(|m| (*rid, m)));
        }
    }
    out
}

/// The hover line of a connector half: `-X10.S (socket, 4 pins) mates with -X10.P
/// on 102 C4.`, or `-X10.S has no plug half.`
pub fn hover_words(doc: &Document, c: DeviceId, side: Side) -> Option<String> {
    if !connector_of(doc, c).is_some_and(|x| x.mating) || side == Side::Both {
        return None;
    }
    let d = drawn(doc, c);
    let n = d.side(side).map_or(0, BTreeMap::len);
    let me = half_tag(doc, c, side);
    Some(match half_goto(doc, c, side.other()) {
        Some(r) => format!(
            "{me} ({}, {n} pin{}) mates with {} on {}.",
            side.word(),
            if n == 1 { "" } else { "s" },
            half_tag(doc, c, side.other()),
            where_words(doc, r)
        ),
        None => format!("{me} has no {} half.", side.other().word()),
    })
}

// ---------------------------------------------------------------- the index

/// The connector index: which connectors have a symbol on each page, and which
/// read each naming key (separator, side names) from the project or the built-in. Built on open,
/// reload and branch switch, and again after every commit ([`Index::build`]; 0.4 ms on the large
/// project, so no update map: the build stays under ~5 ms).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Index {
    /// Page → the connectors with a symbol there.
    pub by_page: BTreeMap<PageId, BTreeSet<DeviceId>>,
    /// Naming key → the connectors that inherit it (no own value).
    pub inherits: BTreeMap<&'static str, BTreeSet<DeviceId>>,
    /// Each connector's pages, to take it out again.
    pages_of: BTreeMap<DeviceId, BTreeSet<PageId>>,
}

/// The naming keys a connector may hold.
fn naming_keys() -> [&'static str; 3] {
    use crate::defaults::connectors as dc;
    [dc::SEPARATOR.key, dc::SIDE_SOCKET.key, dc::SIDE_PLUG.key]
}

impl Index {
    /// The whole index of `doc`: O(components).
    pub fn build(doc: &Document) -> Index {
        let mut idx = Index::default();
        for c in doc.devices().keys() {
            idx.set(doc, *c);
        }
        idx
    }

    /// Put connector `c` in as `doc` has it.
    fn set(&mut self, doc: &Document, c: DeviceId) {
        for p in self.pages_of.remove(&c).unwrap_or_default() {
            if let Some(set) = self.by_page.get_mut(&p) {
                set.remove(&c);
                if set.is_empty() {
                    self.by_page.remove(&p);
                }
            }
        }
        for set in self.inherits.values_mut() {
            set.remove(&c);
        }
        self.inherits.retain(|_, s| !s.is_empty());
        if !connector_of(doc, c).is_some_and(|x| x.mating) {
            return;
        }
        let pages: BTreeSet<PageId> = doc.representations_of(Subject::Device(c)).map(|(_, r)| r.page).collect();
        for p in &pages {
            self.by_page.entry(*p).or_default().insert(c);
        }
        self.pages_of.insert(c, pages);
        let props = doc.devices().get(&c).map(|x| &x.properties);
        for k in naming_keys() {
            if !props.is_some_and(|p| p.contains_key(k)) {
                self.inherits.entry(k).or_default().insert(c);
            }
        }
    }

    /// The connectors that inherit `key` from the project: a naming key's inheritors; for any
    /// other `connector.` key (a description), every connector.
    pub fn inheriting(&self, key: &str) -> BTreeSet<DeviceId> {
        match self.inherits.get(key) {
            Some(s) => s.clone(),
            None if naming_keys().contains(&key) => BTreeSet::new(),
            None => self.pages_of.keys().copied().collect(),
        }
    }

    /// The pages showing the connectors that inherit `key` (the one wide case).
    pub fn pages_inheriting(&self, key: &str) -> BTreeSet<PageId> {
        self.inheriting(key).iter().flat_map(|c| self.pages_of.get(c).into_iter().flatten().copied()).collect()
    }

    /// How many connectors are indexed.
    pub fn len(&self) -> usize {
        self.pages_of.len()
    }

    /// Whether no connector is indexed.
    pub fn is_empty(&self) -> bool {
        self.pages_of.is_empty()
    }
}

/// A route split at a terminal.
#[derive(Debug, Clone)]
pub struct Land {
    pub ops: Vec<Op>,
    /// The route, which keeps its first end and its points up to the terminal.
    pub route: RouteId,
    /// The new second part, from the terminal to the old last end.
    pub new: RouteId,
}

/// Plan landing terminal `t` (being placed at `on.at`) on its route
/// The route is split there and both parts end on `t`; the commit's rewrite makes the
/// wires (cable membership stays with the wires it keeps).
///
/// # Errors
/// As [`plan_land_ends`].
pub fn plan_land(doc: &Document, ids: &mut IdAllocator, on: OnRoute, t: ConnectionPointId) -> Result<Land, CmdError> {
    plan_land_ends(doc, ids, on, (RouteEnd::Point(t), on.at), (RouteEnd::Point(t), on.at))
}

/// Plan splitting `on.route` at `on.at` where a symbol lands: the first part ends on `first` (an
/// end and the point drawn to), the second starts on `second`.
///
/// # Errors
/// The point at a route end, or a strip terminal already there; each says what to do instead.
pub(crate) fn plan_land_ends(
    doc: &Document, ids: &mut IdAllocator, on: OnRoute, first: (RouteEnd, Pt), second: (RouteEnd, Pt),
) -> Result<Land, CmdError> {
    let route = doc.routes().get(&on.route).ok_or(CmdError::NotFound(Noun::Wire))?;
    if let Some(x) = strip_connection_point_at(doc, route.page, on.at) {
        return Err(CmdError::refused(format!("{} is already there. Click elsewhere on the wire.", connection_point_words(doc, x))));
    }
    if let Some([a, b]) = route.end_points() {
        for (p, e) in [a, b].into_iter().zip(route.ends) {
            if on.at.near(p, ON_WIRE) {
                return Err(CmdError::refused(match e.point() {
                    Some(x) => format!(
                        "That is the end of the wire at {}. Click along the wire, away from its ends, or on empty space.",
                        connection_point_words(doc, x)
                    ),
                    None => "That is the end of the wire. Click along the wire, away from its ends, or on empty space.".into(),
                }));
            }
        }
    }
    let split = crate::wires::placing::plan_split(doc, ids, &on, first, second)?;
    Ok(Land { ops: split.ops, route: split.route, new: split.new })
}

pub fn segments(v: &[Pt]) -> impl Iterator<Item = (Pt, Pt)> + '_ {
    v.windows(2).filter_map(|w| Some((*w.first()?, *w.get(1)?)))
}

/// The terminal of any strip drawn at `at` on `page`.
pub(crate) fn strip_connection_point_at(doc: &Document, page: PageId, at: Pt) -> Option<ConnectionPointId> {
    // Each terminal symbol shows its own connection point: O(representations on the page).
    doc.representations_on(page).find_map(|(_, r)| {
        let Subject::Device(c) = r.subject else { return None };
        terminal_strips::strip_of(doc, c)?;
        let placement = crate::devices::tags::rep_definition(doc, r).map(|(_, pl)| pl);
        crate::devices::merge::shown_connection_points(doc, c, r).into_iter().find_map(|(t, p)| {
            let p = match (r.form, placement) {
                (Form::Symbol, Some(pl)) => pl.apply(p),
                _ => p,
            };
            p.near(at, ON_WIRE).then_some(t)
        })
    })
}
