//! Connector devices (brief 044, design 044): a plug and its socket drawn as one device that mates.
//!
//! # Semantics
//! - A **connector** is one component (`-X10`) whose definition is a brief-044 connector half or
//!   pin ([`crate::symbols::is_mating`]). Each pin of each half is one terminal: label = the pin
//!   name, key = the side key `.s` (socket) or `.p` (plug) (FP1). Nothing new is stored.
//! - **Mating** is 036's per-position bridge rule (FP8a): every half definition bridges `.s` with
//!   `.p`, so pin `n` of the socket half and pin `n` of the plug half are one net (FP2).
//! - A **half** is shown and typed as the connector's tag, a separator and a side name (`-X10.S`),
//!   all registry defaults ([`crate::defaults::connectors`]); a pin as `-X10.S:3`.
//! - The per-half BOM fields and location are component properties `half.<s|p>.<field>` (FP4);
//!   `pins` says how many pins the part has.
//! - A connector half embedded before brief 044 (no side bridges) is not a connector: it stays the
//!   device it was (design 044 §3.1).
//!
//! Everything here reads the document; the planners return ops and change nothing. Excludes: the
//! commands themselves ([`crate::connector_cmds`]), BOM rows (`outputs.rs` reads [`half_rows`]),
//! and reference texts (`xref.rs` reads [`ref_classes`]).

use std::collections::{BTreeMap, BTreeSet};

use ecad_model::{
    Component, ComponentId, DefinitionId, Document, End, Form, IdAllocator, Item, ItemId, ItemKind, Ledger, Op, PageId, Placement, Primitive, Pt,
    Representation, RepresentationId, Style, Subject, SymbolInstance, Terminal, TerminalId,
};

use crate::checks::{Check, Fix, Kind as CheckKind, Object};
use crate::defaults::{self, Scope};
use crate::locations::{BoxCache, Located};
use crate::symbols::{self, Kind, PLUG_KEY, Side, Starter};

/// Most pins a connector has (design 044 §2.3): a guard, not a preference.
pub const MAX_PINS: usize = 64;
/// Most characters of a pin name.
pub const MAX_NAME_CHARS: usize = 8;
/// Most characters of a typed pin-name list that are read.
pub const MAX_LIST_CHARS: usize = 1_000;
/// The component property holding the pin count (FP4).
pub const PINS_KEY: &str = "pins";
/// The component property holding a typed list of pin names, in pin order (`L,N,PE`; overseer,
/// fix round 1: names not drawn yet are kept, never lost).
pub const PIN_NAMES_KEY: &str = "pin_names";
/// The start of the per-half property keys (FP4).
pub const HALF_PREFIX: &str = "half.";
/// The per-half fields the BOM and locations read (FP4).
pub const HALF_FIELDS: [&str; 4] = ["description", "maker", "part_number", "location"];
/// The start of a connector's ignored-warning keys (`ignore.connector-family`; overseer, fix round
/// 1: the Checks rows' Ignore button).
pub const IGNORE_PREFIX: &str = "ignore.";

/// Whether connector `c` ignores Checks rows of `kind`.
pub fn ignores(doc: &Document, c: ComponentId, kind: CheckKind) -> bool {
    doc.components.get(&c).is_some_and(|x| x.properties.contains_key(&format!("{IGNORE_PREFIX}{}", kind.name())))
}

/// The keys of the pin starters.
pub const PIN_KEYS: [&str; 3] = ["pinpair", "pins", "pinp"];

// ---------------------------------------------------------------- families (design 044 §1)

/// A connector family: both halves of one connector have one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Family {
    Plain,
    Dsub,
    M12,
    Rj45,
    Usba,
    Usbb,
}

impl Family {
    pub const ALL: [Family; 6] = [Family::Plain, Family::Dsub, Family::M12, Family::Rj45, Family::Usba, Family::Usbb];

    /// The JSON and registry word: `plain`, `dsub`, `m12`, `rj45`, `usba`, `usbb`.
    pub fn word(self) -> &'static str {
        match self {
            Family::Plain => "plain",
            Family::Dsub => "dsub",
            Family::M12 => "m12",
            Family::Rj45 => "rj45",
            Family::Usba => "usba",
            Family::Usbb => "usbb",
        }
    }

    /// The prompt keyword: `Plain`, `Dsub`, `M12`, `RJ45`, `USBA`, `USBB`.
    pub fn keyword(self) -> &'static str {
        match self {
            Family::Plain => "Plain",
            Family::Dsub => "Dsub",
            Family::M12 => "M12",
            Family::Rj45 => "RJ45",
            Family::Usba => "USBA",
            Family::Usbb => "USBB",
        }
    }

    /// The words a person reads: `D-sub`, `M12`…
    pub fn name(self) -> &'static str {
        match self {
            Family::Plain => "Plain",
            Family::Dsub => "D-sub",
            Family::M12 => "M12",
            Family::Rj45 => "RJ45",
            Family::Usba => "USB-A",
            Family::Usbb => "USB-B",
        }
    }

    /// The family a word names (case-insensitive; `d-sub`, `sub-d` and `usb-a` too).
    pub fn parse(text: &str) -> Option<Family> {
        let t: String = text.trim().chars().take(16).filter(|c| *c != '-').collect::<String>().to_ascii_lowercase();
        match t.as_str() {
            "plain" => Some(Family::Plain),
            "dsub" | "subd" => Some(Family::Dsub),
            "m12" => Some(Family::M12),
            "rj45" => Some(Family::Rj45),
            "usba" => Some(Family::Usba),
            "usbb" => Some(Family::Usbb),
            _ => None,
        }
    }

    /// The sizes its whole halves come in; empty for a bulkhead.
    pub fn sizes(self) -> &'static [usize] {
        match self {
            Family::Plain => &[2, 3, 4, 6, 8, 12],
            Family::Dsub => &[9, 15, 25],
            Family::M12 => &[4, 5, 8],
            Family::Rj45 | Family::Usba | Family::Usbb => &[],
        }
    }

    /// The bulkhead starter of a one-piece family.
    pub fn bulkhead(self) -> Option<&'static str> {
        match self {
            Family::Rj45 => Some("rj45"),
            Family::Usba => Some("usba"),
            Family::Usbb => Some("usbb"),
            _ => None,
        }
    }

    /// The starter key of its whole half of `side` with `n` pins (`socket4`, `dsub9p`, `m12s5`).
    pub fn half_key(self, side: Side, n: usize) -> Option<String> {
        if !self.sizes().contains(&n) {
            return None;
        }
        Some(match (self, side) {
            (Family::Plain, Side::Socket) => format!("socket{n}"),
            (Family::Plain, Side::Plug) => format!("plug{n}"),
            (Family::Dsub, Side::Socket) => format!("dsub{n}s"),
            (Family::Dsub, Side::Plug) => format!("dsub{n}p"),
            (Family::M12, Side::Socket) => format!("m12s{n}"),
            (Family::M12, Side::Plug) => format!("m12p{n}"),
            _ => return None,
        })
    }

    /// The refusal for a pin count that is not one of its sizes (design 044 §6.1).
    pub fn size_refusal(self) -> String {
        let sizes: Vec<String> = self.sizes().iter().map(ToString::to_string).collect();
        let list = match sizes.as_slice() {
            [] => String::new(),
            [one] => one.clone(),
            [rest @ .., last] => format!("{} or {last}", rest.join(", ")),
        };
        let article = if self == Family::M12 { "An" } else { "A" };
        format!("{article} {} has {list} pins. Pick one, or draw pin by pin with Pins.", self.name())
    }
}

/// The family of a connector starter (`None` for a pin, which has none, and for other symbols).
pub fn family_of(s: &Starter) -> Option<Family> {
    let k = s.key;
    if PIN_KEYS.contains(&k) || s.family != symbols::Family::Connectors {
        return None;
    }
    Some(if k.starts_with("dsub") {
        Family::Dsub
    } else if k.starts_with("m12") {
        Family::M12
    } else if k == "rj45" {
        Family::Rj45
    } else if k == "usba" {
        Family::Usba
    } else if k == "usbb" {
        Family::Usbb
    } else {
        Family::Plain
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
    s.definition().ok().map(|d| d.terminals.len())
}

// ---------------------------------------------------------------- what is a connector

/// Whether component `c` is a connector: its definition is a brief-044 half or pin.
pub fn is_connector(doc: &Document, c: ComponentId) -> bool {
    doc.components.get(&c).and_then(|x| x.definition).and_then(|d| doc.definitions.get(&d)).is_some_and(symbols::is_mating)
}

/// The side of terminal `t` when it is a connector pin.
pub fn side_of(doc: &Document, t: TerminalId) -> Option<Side> {
    let term = doc.terminals.get(&t)?;
    let side = Side::of_key(term.key.as_deref()?)?;
    is_connector(doc, term.component).then_some(side)
}

/// The starter a representation draws when it is a brief-044 half or pin.
pub fn rep_half(doc: &Document, r: &Representation) -> Option<&'static Starter> {
    let (d, _) = crate::devices::rep_definition(doc, r)?;
    if !symbols::is_mating(d) {
        return None;
    }
    symbols::of_definition(d)
}

/// The side a representation draws (`Both` for a pin pair).
pub fn rep_side(doc: &Document, r: &Representation) -> Option<Side> {
    rep_half(doc, r).and_then(Starter::side)
}

// ---------------------------------------------------------------- naming (design 044 §5)

/// The separator and the socket and plug side names connector `c` (or the project) uses; two
/// equal side names count as absent (the built-ins).
pub fn side_names(doc: &Document, c: Option<ComponentId>) -> (String, String, String) {
    use crate::defaults::connectors as dc;
    let scope = c.map_or_else(Scope::project, Scope::component);
    let sep = defaults::resolve(doc, &dc::SEPARATOR, scope).text();
    let s = defaults::resolve(doc, &dc::SIDE_SOCKET, scope).text();
    let p = defaults::resolve(doc, &dc::SIDE_PLUG, scope).text();
    if s.eq_ignore_ascii_case(&p) {
        let b = |e: &defaults::Entry| defaults::Resolved { value: e.builtin, from: defaults::Source::Builtin }.text();
        return (sep, b(&dc::SIDE_SOCKET), b(&dc::SIDE_PLUG));
    }
    (sep, s, p)
}

/// The tag of half `side` of connector `c` (`-X10.S`); the connector's tag for `Both`.
pub fn half_tag(doc: &Document, c: ComponentId, side: Side) -> String {
    let tag = crate::devices::tag_words(doc, c);
    let (sep, s, p) = side_names(doc, Some(c));
    match side {
        Side::Socket => format!("{tag}{sep}{s}"),
        Side::Plug => format!("{tag}{sep}{p}"),
        Side::Both => tag,
    }
}

/// The half tag a connector pin belongs to (`-X10.S`); `None` for any other terminal.
pub fn half_tag_of(doc: &Document, t: TerminalId) -> Option<String> {
    let side = side_of(doc, t)?;
    let c = doc.terminals.get(&t)?.component;
    Some(half_tag(doc, c, side))
}

/// A connector pin's full name, `-X10.S:3`; `None` for any other terminal.
pub fn pin_name(doc: &Document, t: TerminalId) -> Option<String> {
    let half = half_tag_of(doc, t)?;
    let label = doc.terminals.get(&t)?.label.clone()?;
    Some(format!("{half}:{label}"))
}

/// The tag a connector symbol shows in its tag block: its half's tag (a pin pair: the connector's).
pub fn rep_tag(doc: &Document, c: ComponentId, r: &Representation) -> Option<String> {
    let side = rep_side(doc, r)?;
    Some(half_tag(doc, c, side))
}

/// Whether a pin pair's plug-side label is hidden (design 044 §5.2: a pin pair shows its pin name
/// once, beside the socket point).
pub fn hides_label(doc: &Document, r: &Representation, t: TerminalId) -> bool {
    rep_half(doc, r).is_some_and(|s| s.key == "pinpair") && doc.terminals.get(&t).and_then(|x| x.key.as_deref()) == Some(PLUG_KEY)
}

/// The run class of a connector symbol for the "once per run" tag rule (design 044 §5.2): one
/// class per half, so `-X10.S` and `-X10.P` each show once per run.
pub fn run_class(doc: &Document, r: &Representation) -> Option<u64> {
    rep_side(doc, r).map(|s| match s {
        Side::Socket => u64::MAX - 1,
        Side::Plug => u64::MAX - 2,
        Side::Both => u64::MAX - 3,
    })
}

/// A typed connector name (design 044 §5.1): `-X10.S`, `-X10.P:3`, or `-X10:3` on a connector.
/// `None` when `text` names no connector; `Some(Err)` when it names one ambiguously.
///
/// # Errors
/// `-X10:3` on a connector with pin 3 on both halves, and a pin the half does not have.
pub fn resolve(doc: &Document, text: &str) -> Option<Result<(ComponentId, Option<Side>, Option<TerminalId>), String>> {
    let text: String = text.trim().chars().take(64).collect();
    let (head, pin) = match text.split_once(':') {
        Some((h, p)) => (h.to_string(), Some(p.to_string())),
        None => (text.clone(), None),
    };
    // The connector and side: a tag as is, else a tag followed by the separator and a side name.
    let found = doc.components.keys().copied().find_map(|c| {
        if !is_connector(doc, c) {
            return None;
        }
        let t = crate::devices::tag_of(doc, c)?;
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
        |want: Side| doc.terminals_of(c).find(|(_, x)| x.label.as_deref() == Some(pin.as_str()) && x.key.as_deref() == want.key()).map(|(t, _)| t);
    Some(match side {
        Some(s) => with(s).map(|t| (c, Some(s), Some(t))).ok_or_else(|| format!("{} has no pin {pin}.", half_tag(doc, c, s))),
        None => match (with(Side::Socket), with(Side::Plug)) {
            (Some(_), Some(_)) => {
                let tag = crate::devices::tag_words(doc, c);
                Err(format!(
                    "{tag} has pin {pin} on both halves. Type {}:{pin} or {}:{pin}.",
                    half_tag(doc, c, Side::Socket),
                    half_tag(doc, c, Side::Plug)
                ))
            }
            (Some(t), None) => Ok((c, Some(Side::Socket), Some(t))),
            (None, Some(t)) => Ok((c, Some(Side::Plug), Some(t))),
            (None, None) => Err(format!("{} has no pin {pin}.", crate::devices::tag_words(doc, c))),
        },
    })
}

// ---------------------------------------------------------------- pins (design 044 §2.3, §4.4)

/// The pins a connector has drawn: per side, pin name → its terminals (more than one: drawn twice).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Drawn {
    pub socket: BTreeMap<String, Vec<TerminalId>>,
    pub plug: BTreeMap<String, Vec<TerminalId>>,
    /// Every pin name, in name order (natural order for numbers, else the order drawn).
    pub order: Vec<String>,
}

impl Drawn {
    /// The pins of `side` (`Both`: none).
    pub fn side(&self, side: Side) -> Option<&BTreeMap<String, Vec<TerminalId>>> {
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
pub fn drawn(doc: &Document, c: ComponentId) -> Drawn {
    let mut d = Drawn::default();
    let mut first: BTreeMap<String, TerminalId> = BTreeMap::new();
    for (t, x) in doc.terminals_of(c) {
        let (Some(label), Some(side)) = (x.label.clone(), x.key.as_deref().and_then(Side::of_key)) else { continue };
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
    let mut order: Vec<(String, TerminalId)> = first.into_iter().collect();
    if order.iter().all(|(n, _)| n.parse::<u64>().is_ok()) {
        order.sort_by_key(|(n, _)| n.parse::<u64>().unwrap_or(u64::MAX));
    } else {
        order.sort_by_key(|(_, t)| *t);
    }
    // A typed list gives the pin order (tester D7).
    if let Some(list) = pin_names(doc, c) {
        order.sort_by_key(|(n, _)| list.iter().position(|x| x == n).unwrap_or(usize::MAX));
    }
    d.order = order.into_iter().map(|(n, _)| n).collect();
    d
}

/// Connector `c`'s typed list of pin names, when it holds a valid one.
pub fn pin_names(doc: &Document, c: ComponentId) -> Option<Vec<String>> {
    parse_list(doc.components.get(&c)?.properties.get(PIN_NAMES_KEY)?).ok().filter(|v| !v.is_empty())
}

/// Connector `c`'s `pins` property, when it holds a count from 1 to [`MAX_PINS`].
pub fn pins_prop(doc: &Document, c: ComponentId) -> Option<usize> {
    doc.components.get(&c)?.properties.get(PINS_KEY)?.trim().parse::<usize>().ok().filter(|n| (1..=MAX_PINS).contains(n))
}

/// The whole-half symbols of connector `c`: (representation, side, starter), in ID order.
pub fn whole_halves(doc: &Document, c: ComponentId) -> Vec<(RepresentationId, Side, &'static Starter)> {
    doc.representations_of(Subject::Component(c))
        .filter_map(|(rid, r)| {
            let s = rep_half(doc, r).filter(|s| !is_pin_starter(s))?;
            Some((rid, s.side()?, s))
        })
        .collect()
}

/// How many pins connector `c` has (design 044 §2.3): `pins`, else the largest whole half drawn,
/// else the number of distinct pins drawn.
pub fn pin_count(doc: &Document, c: ComponentId, d: &Drawn) -> usize {
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

/// Whether `name` is a valid pin name (design 044 §2.3).
///
/// # Errors
/// Empty, too long, a space, comma, colon or control character.
pub fn check_name(name: &str) -> Result<(), String> {
    let shown: String = name.chars().take(16).filter(|c| !c.is_control()).collect();
    if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
        return Err(format!("A pin name is 1 to {MAX_NAME_CHARS} characters: \"{shown}\"."));
    }
    if name.chars().any(|c| c.is_whitespace() || c == ',' || c == ':' || c.is_control()) {
        return Err(format!("Pin names cannot contain spaces, commas or colons: \"{shown}\"."));
    }
    Ok(())
}

/// Parse a typed list of pin names (`L,N,PE`).
///
/// # Errors
/// Too long, a bad name, a repeated name, more than [`MAX_PINS`].
pub fn parse_list(text: &str) -> Result<Vec<String>, String> {
    if text.chars().count() > MAX_LIST_CHARS {
        return Err(format!("A pin name list is at most {MAX_LIST_CHARS} characters."));
    }
    let names: Vec<String> = text.split(',').map(|n| n.trim().to_string()).collect();
    check_list(&names)?;
    Ok(names)
}

/// Check a list of pin names: each valid, all different, at most [`MAX_PINS`].
///
/// # Errors
/// As [`parse_list`].
pub fn check_list(names: &[String]) -> Result<(), String> {
    if names.len() > MAX_PINS {
        return Err(format!("A connector has at most {MAX_PINS} pins; got {}.", names.len()));
    }
    let mut seen = BTreeSet::new();
    for n in names {
        check_name(n)?;
        if !seen.insert(n.to_ascii_lowercase()) {
            return Err(format!("Pin {n} is named twice. Give each pin its own name."));
        }
    }
    Ok(())
}

/// The names of `n` pins named `names`.
///
/// # Errors
/// Letters past 26, a list whose count is not `n`.
pub fn names_for(names: &Names, n: usize) -> Result<Vec<String>, String> {
    if n == 0 || n > MAX_PINS {
        return Err(format!("A connector has 1 to {MAX_PINS} pins."));
    }
    match names {
        Names::Numbers => Ok((1..=n).map(|i| i.to_string()).collect()),
        Names::Letters => {
            if n > 26 {
                return Err("Letters name at most 26 pins. Use Numbers or a list.".into());
            }
            Ok((0..n).filter_map(|i| u8::try_from(i).ok().map(|i| char::from(b'A' + i).to_string())).collect())
        }
        Names::List(v) => {
            if v.len() != n {
                return Err(format!("Give {n} pin names; got {}.", v.len()));
            }
            check_list(v)?;
            Ok(v.clone())
        }
    }
}

/// The pins connector `c` offers, in name order: those drawn, then the ones its pin count and
/// naming imply (numbers 1…n, or letters A…).
fn candidates(doc: &Document, c: ComponentId, d: &Drawn) -> Vec<String> {
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

/// The pin a pin symbol of `side` takes on connector `c` (design 044 §4.4); `taken` are pins this
/// transaction already gave, per side.
///
/// # Errors
/// A connector named by letters or a list whose pins are all drawn on that side; more than
/// [`MAX_PINS`] pins.
pub fn next_pin(doc: &Document, c: ComponentId, side: Side, taken: &BTreeSet<(Side, String)>) -> Result<String, String> {
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
            return Err(format!("{} has {MAX_PINS} pins, the most a connector has.", crate::devices::tag_words(doc, c)));
        }
        return Ok(next.to_string());
    }
    let tag = crate::devices::tag_words(doc, c);
    let where_ = match side {
        Side::Both => "both halves".to_string(),
        s => format!("the {} half", s.word()),
    };
    Err(format!(
        "{tag} has {} named pins ({}), all drawn on {where_}. Add a name with pinlabel, or set pins.",
        all.len(),
        all.iter().take(8).cloned().collect::<Vec<_>>().join(", ")
    ))
}

// ---------------------------------------------------------------- halves and their symbols

/// Where a representation is, in words (`101 B3`, the zone's letter first), for refusals.
pub fn where_words(doc: &Document, rid: RepresentationId) -> String {
    let Some(r) = doc.representations.get(&rid) else { return String::new() };
    let page = ecad_model::book::page_number(doc, r.page);
    match crate::xref::anchor(doc, r).and_then(|a| ecad_model::wiring::place(doc, r.page, a)) {
        Some(p) => format!("{page} {}", ecad_model::derive::zone_text(&p.column_label, &p.row_label)),
        None => page,
    }
}

/// The representations drawing side `side` of connector `c` (a pin pair draws both), in book
/// order (page, then ID).
pub fn side_symbols(doc: &Document, c: ComponentId, side: Side) -> Vec<RepresentationId> {
    let order: BTreeMap<PageId, usize> = doc.page_order.iter().enumerate().map(|(i, p)| (*p, i)).collect();
    let mut v: Vec<(usize, RepresentationId)> = doc
        .representations_of(Subject::Component(c))
        .filter(|(_, r)| rep_side(doc, r).is_some_and(|s| s == side || s == Side::Both))
        .map(|(rid, r)| (order.get(&r.page).copied().unwrap_or(usize::MAX), rid))
        .collect();
    v.sort();
    v.into_iter().map(|(_, r)| r).collect()
}

/// Where go-to lands for half `side` ([`crate::reps::goto`], design 052 §4): its whole-half
/// symbol, else its first pin symbol in book order, then top-left; `None` when the half is not
/// drawn. Display only: it decides nothing about the half.
pub fn half_goto(doc: &Document, c: ComponentId, side: Side) -> Option<RepresentationId> {
    crate::reps::goto(doc, c, Some(side))
}

/// The family of half `side`: its whole-half symbol's, else the connector's definition's (a
/// pin's: none, then `Plain`).
pub fn half_family(doc: &Document, c: ComponentId, side: Side) -> Family {
    if let Some(f) = whole_halves(doc, c).into_iter().find(|(_, s, _)| *s == side).and_then(|(_, _, s)| family_of(s)) {
        return f;
    }
    doc.components
        .get(&c)
        .and_then(|x| x.definition)
        .and_then(|d| doc.definitions.get(&d))
        .and_then(symbols::of_definition)
        .and_then(family_of)
        .unwrap_or(Family::Plain)
}

// ---------------------------------------------------------------- planning (design 044 §4)

/// One connection point a planned symbol shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Shown {
    pub side: Side,
    pub name: String,
    pub terminal: TerminalId,
    /// Its point in the definition's frame.
    pub local: Pt,
}

/// Plan one connector symbol of `starter` on connector `c`, its pins named `names` (a whole half:
/// one name per point; a pin: one name): one terminal per point keyed by its side, shown through
/// explicit connection points (design 044 §3.1, §4.6). Its role is the starter key, whichever
/// symbol of the connector it is (design 052 §4).
///
/// # Errors
/// Fewer names than points; the definition could not be built.
#[allow(clippy::too_many_arguments)]
pub fn symbol_ops(
    ids: &mut IdAllocator, page: PageId, style: Style, def: DefinitionId, starter: &Starter, c: ComponentId, names: &[String], placement: Placement,
    ops: &mut Vec<Op>,
) -> Result<(RepresentationId, ItemId, Vec<Shown>), String> {
    let d = starter.definition()?;
    let (item, rep): (ItemId, RepresentationId) = (ids.fresh(), ids.fresh());
    let mut cps = BTreeMap::new();
    let mut shown = Vec::new();
    let pin = is_pin_starter(starter);
    for (i, td) in d.terminals.iter().enumerate() {
        let side = match starter.side() {
            Some(Side::Both) | None => Side::of_key(&td.key).ok_or("A connector symbol's point has no side.")?,
            Some(s) => s,
        };
        let name = if pin { names.first() } else { names.get(i) }.ok_or("Not enough pin names for this symbol.")?;
        let t: TerminalId = ids.fresh();
        let key = side.key().ok_or("A connector symbol's point has no side.")?;
        ops.push(Op::AddTerminal { id: t, terminal: Terminal { component: c, label: Some(name.clone()), key: Some(key.to_string()) } });
        cps.insert(t, td.at);
        shown.push(Shown { side, name: name.clone(), terminal: t, local: td.at });
    }
    ops.push(Op::AddItem { id: item, item: Item { page, style, kind: ItemKind::Symbol(SymbolInstance { definition: def, placement }) } });
    ops.push(Op::LinkRepresentation {
        id: rep,
        rep: Representation {
            subject: Subject::Component(c),
            page,
            form: Form::Symbol,
            role: starter.key.into(),
            items: vec![item],
            connection_points: cps,
        },
    });
    Ok((rep, item, shown))
}

/// Plan a new connector made by `starter` (its definition `def`): `CreateComponent` with its tag
/// (`tag`, else the next automatic one), the registry description and `pins`.
///
/// # Errors
/// No tag number left.
pub fn create_ops(
    ledger: &Ledger, ids: &mut IdAllocator, high: &mut crate::devices::TagHigh, starter: &Starter, def: DefinitionId, tag: Option<String>,
    pins: Option<usize>, names: Option<&[String]>, ops: &mut Vec<Op>,
) -> Result<(ComponentId, String), String> {
    let doc = ledger.document();
    let tag = match tag {
        Some(t) if ecad_model::tag::provisional(&t).is_some_and(|(_, n)| n == 0) => {
            high.catch_up(ledger);
            high.next_provisional_in(doc, &ecad_model::tag::provisional(&t).map(|x| x.0).unwrap_or_default())
        }
        Some(t) => t,
        None => high.next_auto(ledger, &crate::devices::prefix_for(doc, starter))?,
    };
    let c: ComponentId = ids.fresh();
    let mut properties = ecad_model::Properties::new();
    properties.insert(ecad_model::tag::TAG_KEY.into(), tag.clone());
    properties.insert("description".into(), defaults::resolve(doc, &defaults::connectors::DESCRIPTION, Scope::project()).text());
    if let Some(n) = pins {
        properties.insert(PINS_KEY.into(), n.to_string());
    }
    if let Some(list) = names.filter(|l| !l.is_empty()) {
        properties.insert(PIN_NAMES_KEY.into(), list.join(","));
    }
    ops.push(Op::CreateComponent { id: c, component: Component { properties, definition: Some(def) } });
    Ok((c, tag))
}

/// The refusal for joining a device that is not a connector (design 044 §6.1).
pub fn not_a_connector(doc: &Document, c: ComponentId) -> String {
    format!("{} is not a connector. Pick a connector, or Enter for a new one.", crate::devices::tag_words(doc, c))
}

/// Check that a whole half of `side` may join connector `c` (design 044 §6.1).
///
/// # Errors
/// The half already has pins.
pub fn check_whole_half(doc: &Document, c: ComponentId, side: Side, taken: &BTreeSet<(Side, String)>) -> Result<(), String> {
    let d = drawn(doc, c);
    let has = d.side(side).is_some_and(|m| !m.is_empty()) || taken.iter().any(|(s, _)| *s == side);
    if !has {
        return Ok(());
    }
    let at = half_goto(doc, c, side).map(|r| format!(" ({})", where_words(doc, r))).unwrap_or_default();
    Err(format!(
        "{} already has a {} half{at}. Place a {} half, or Enter for a new connector.",
        crate::devices::tag_words(doc, c),
        side.word(),
        side.other().word()
    ))
}

/// Check that pin `name` may be drawn on `side` of connector `c` (design 044 §6.1).
///
/// # Errors
/// The pin is drawn on that side (either side for a pin pair).
pub fn check_named_pin(doc: &Document, c: ComponentId, side: Side, name: &str, taken: &BTreeSet<(Side, String)>) -> Result<(), String> {
    check_name(name)?;
    let d = drawn(doc, c);
    let sides: &[Side] = match side {
        Side::Both => &[Side::Socket, Side::Plug],
        Side::Socket => &[Side::Socket],
        Side::Plug => &[Side::Plug],
    };
    for s in sides {
        if taken.contains(&(*s, name.to_string())) {
            return Err(format!("{}:{name} is already being drawn. Pick another pin.", half_tag(doc, c, *s)));
        }
        if let Some(t) = d.side(*s).and_then(|m| m.get(name)).and_then(|v| v.first()) {
            let at = doc.representations_showing(*t).next().map(|(rid, _)| where_words(doc, rid)).unwrap_or_default();
            return Err(format!("{}:{name} is drawn on {at}. Pick another pin, or Enter for the next free one.", half_tag(doc, c, *s)));
        }
    }
    Ok(())
}

/// Plan landing a connector pin symbol, placed at `on.at`, on its wire (design 044 §4.5): a pin
/// pair splits the wire, the part towards `.s` landing on `.s` and the other on `.p`; both are
/// one net through the mate, so the wire number stays on both. A one-side pin takes the part
/// towards its point; the other part is left open, without a stored number of its own.
///
/// # Errors
/// [`crate::terminal_cmds::plan_land`]'s refusals.
pub fn land_pin(
    doc: &Document, ids: &mut IdAllocator, page: PageId, on: crate::wires::OnWire, shown: &[Shown], placement: Placement,
) -> Result<crate::terminal_cmds::Land, String> {
    let first = shown.first().ok_or("The symbol has no connection point.")?;
    let before = crate::wires::route_vertices(doc, on.conductor, page).and_then(|(_, _, v)| v.get(on.segment).copied());
    let mut land = crate::terminal_cmds::plan_land(doc, ids, page, on, first.terminal)?;
    let (c1, c2) = (land.conductor, land.new);
    match (shown.iter().find(|x| x.side == Side::Socket), shown.iter().find(|x| x.side == Side::Plug)) {
        (Some(s), Some(p)) if shown.len() == 2 => {
            let (ps, pp) = (placement.apply(s.local), placement.apply(p.local));
            let keep_s = before.is_none_or(|b| b.dist(ps) <= b.dist(pp));
            let (keep_t, keep_p, rest_t, rest_p) = if keep_s { (s.terminal, ps, p.terminal, pp) } else { (p.terminal, pp, s.terminal, ps) };
            for op in &mut land.ops {
                match op {
                    Op::AttachEnd { conductor, end: End::B, after, .. } if *conductor == c1 => *after = Some(keep_t),
                    Op::CreateConductor { id, conductor } if *id == c2 => conductor.ends[0] = Some(rest_t),
                    Op::ModifyItem { after, .. } => set_route_end(after, keep_p, false),
                    Op::AddItem { item, .. } if matches!(item.kind, ItemKind::Primitive(_)) => set_route_end(item, rest_p, true),
                    _ => {}
                }
            }
        }
        _ => {
            let pt = placement.apply(first.local);
            let away = Pt::new(2.0 * placement.origin.x - pt.x, 2.0 * placement.origin.y - pt.y);
            let keep_attached = before.is_none_or(|b| b.dist(pt) <= b.dist(away));
            let mut dropped = None;
            for op in &mut land.ops {
                match op {
                    Op::AttachEnd { conductor, end: End::B, after, .. } if *conductor == c1 && !keep_attached => *after = None,
                    Op::CreateConductor { id, conductor } if *id == c2 => {
                        if keep_attached {
                            conductor.ends[0] = None;
                        }
                        let number = conductor.properties.remove(ecad_model::connectivity::WIRE_NUMBER);
                        let fixed = conductor.properties.remove(ecad_model::connectivity::WIRE_NUMBER_FIXED).is_some();
                        dropped = number.map(|n| (n, fixed));
                    }
                    Op::ModifyItem { after, .. } if keep_attached => set_route_end(after, pt, false),
                    Op::AddItem { item, .. } if !keep_attached && matches!(item.kind, ItemKind::Primitive(_)) => set_route_end(item, pt, true),
                    _ => {}
                }
            }
            land.dropped_number = dropped;
        }
    }
    Ok(land)
}

/// Move a route's first (`start`) or last vertex to `p`.
fn set_route_end(item: &mut Item, p: Pt, start: bool) {
    match &mut item.kind {
        ItemKind::Primitive(Primitive::Line { a, b }) => {
            if start {
                *a = p;
            } else {
                *b = p;
            }
        }
        ItemKind::Primitive(Primitive::Polyline { vertices, .. }) => {
            let v = if start { vertices.first_mut() } else { vertices.last_mut() };
            if let Some(v) = v {
                *v = p;
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------- properties (FP4)

/// The value `setprop` stores for a connector key on component `c` (`pins`, `half.<s|p>.<field>`,
/// the naming defaults), or `None` when `c` is not a connector or the key is not one of these.
///
/// # Errors
/// A pin count out of range, an unknown side letter, a value the rules refuse.
pub fn setprop_value(doc: &Document, c: ComponentId, key: &str, value: Option<&str>) -> Option<Result<Option<String>, String>> {
    if key == PINS_KEY {
        if !is_connector(doc, c) {
            return None;
        }
        return Some(match value.map(str::trim).filter(|v| !v.is_empty()) {
            None => Ok(None),
            Some(v) => v
                .parse::<usize>()
                .ok()
                .filter(|n| (1..=MAX_PINS).contains(n))
                .map(|n| Some(n.to_string()))
                .ok_or_else(|| format!("`pins` is a whole number from 1 to {MAX_PINS}.")),
        });
    }
    if key == PIN_NAMES_KEY {
        if !is_connector(doc, c) {
            return None;
        }
        return Some(match value.map(str::trim).filter(|v| !v.is_empty()) {
            None => Ok(None),
            Some(v) => parse_list(v).map(|l| Some(l.join(","))),
        });
    }
    if let Some(rest) = key.strip_prefix(HALF_PREFIX) {
        if !is_connector(doc, c) {
            return None;
        }
        let tag = crate::devices::tag_words(doc, c);
        let (letter, field) = rest.split_once('.').unwrap_or((rest, ""));
        if letter != "s" && letter != "p" {
            let shown: String = letter.chars().take(8).filter(|c| !c.is_control()).collect();
            return Some(Err(format!("{tag} has no half \"{shown}\". Use half.s.* or half.p.*.")));
        }
        if field.is_empty() || field.chars().count() > 32 || !field.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
            return Some(Err(format!("A half's field is a word such as {}: half.{letter}.description.", HALF_FIELDS.join(", "))));
        }
        return Some(match field {
            "location" => match value.map(str::trim).filter(|v| !v.is_empty()) {
                None => Ok(None),
                Some(v) => crate::locations::location_value(doc, v).map(Some),
            },
            _ => crate::devices::property_value(doc, "description", value),
        });
    }
    if crate::defaults::connectors::is_naming_key(key) {
        if !is_connector(doc, c) {
            return None;
        }
        return crate::defaults::connectors::setprop_value(key, value);
    }
    None
}

/// The key of a half's field (`half.s.maker`).
pub fn half_key(side: Side, field: &str) -> Option<String> {
    Some(format!("{HALF_PREFIX}{}.{field}", side.letter()?))
}

/// A half's own value of `field`.
pub fn half_prop(doc: &Document, c: ComponentId, side: Side, field: &str) -> Option<String> {
    let k = half_key(side, field)?;
    doc.components.get(&c)?.properties.get(&k).map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

// ---------------------------------------------------------------- BOM (design 044 §8)

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
    pub main: Option<(RepresentationId, PageId)>,
}

/// The pin count of half `side`: its whole-half size, else the connector's pin count.
pub fn half_size(doc: &Document, c: ComponentId, side: Side, d: &Drawn) -> usize {
    whole_halves(doc, c).into_iter().find(|(_, s, _)| *s == side).and_then(|(_, _, s)| size_of(s)).unwrap_or_else(|| pin_count(doc, c, d))
}

/// The BOM description a half gets from the registry (`D-sub socket, 9-way`).
pub fn default_description(doc: &Document, family: Family, side: Side, n: usize) -> String {
    let fam = match family {
        Family::Dsub | Family::M12 => family,
        _ => Family::Plain,
    };
    let Some(e) = crate::defaults::connectors::description_entry(fam.word(), side.word()) else { return String::new() };
    defaults::resolve(doc, e, Scope::project()).text().replace("{n}", &n.to_string())
}

/// One row per drawn half of connector `c` (design 044 §8.1): its half tag, own fields or the
/// registry's description, and its location (§8.2). Empty for a connector with nothing drawn.
pub fn half_rows(doc: &Document, c: ComponentId, cache: &mut BoxCache) -> Vec<HalfRow> {
    if !is_connector(doc, c) {
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
        let location = match half_prop(doc, c, side, "location") {
            Some(v) => Located { value: Some(v), source: crate::locations::Source::Device },
            None => match doc.components.get(&c).and_then(|x| x.properties.get(ecad_model::book::LOCATION_KEY)) {
                Some(v) => Located { value: Some(v.clone()), source: crate::locations::Source::Device },
                None => match doc.representations.get(&main) {
                    Some(r) => crate::locations::rep_context(doc, r, cache),
                    None => Located { value: None, source: crate::locations::Source::None },
                },
            },
        };
        out.push(HalfRow {
            side,
            tag: half_tag(doc, c, side),
            description,
            maker: half_prop(doc, c, side, "maker"),
            part_number: half_prop(doc, c, side, "part_number"),
            location,
            main: doc.representations.get(&main).map(|r| (main, r.page)),
        });
    }
    out
}

// ---------------------------------------------------------------- checks (design 044 §6.3)

/// Where terminal `t` is drawn: the page and anchor of the first symbol showing it.
fn terminal_at(doc: &Document, t: TerminalId) -> (Option<PageId>, Option<Pt>, Option<RepresentationId>) {
    match doc.representations_showing(t).next() {
        Some((rid, r)) => (Some(r.page), crate::xref::anchor(doc, r), Some(rid)),
        None => (None, None, None),
    }
}

fn rep_at(doc: &Document, rid: Option<RepresentationId>) -> (Option<PageId>, Option<Pt>) {
    match rid.and_then(|r| doc.representations.get(&r)) {
        Some(r) => (Some(r.page), crate::xref::anchor(doc, r)),
        None => (None, None),
    }
}

/// The Checks rows of connector `c` (design 044 §6.3); none for any other component.
pub fn checks(doc: &Document, c: ComponentId) -> Vec<Check> {
    if !is_connector(doc, c) {
        return Vec::new();
    }
    let d = drawn(doc, c);
    let tag = crate::devices::tag_words(doc, c);
    let mut out = Vec::new();
    // A pin drawn twice on one side.
    for side in [Side::Socket, Side::Plug] {
        let Some(m) = d.side(side) else { continue };
        for (name, ts) in m {
            let mut ts = ts.clone();
            ts.sort();
            if let Some(second) = ts.get(1).copied() {
                let (page, at, _) = terminal_at(doc, second);
                out.push(Check {
                    kind: CheckKind::ConnectorPinTwice,
                    page,
                    at,
                    object: Object::Terminal(second),
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
            let words = |f: Option<Family>, side: Side| format!("{} {}", f.map_or("Plain", Family::name), side.word());
            out.push(Check {
                kind: CheckKind::ConnectorFamily,
                page,
                at,
                object: Object::Component(c),
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
                object: Object::Component(c),
                which: format!("{} has {ns} pins, {} has {np}", half_tag(doc, c, Side::Socket), half_tag(doc, c, Side::Plug)),
                fix: Some(Fix::IgnoreConnector(c, CheckKind::ConnectorSize)),
            });
        }
    }
    // Lone pins, only when both halves have pins.
    if !d.socket.is_empty() && !d.plug.is_empty() {
        let (_, s, p) = side_names(doc, Some(c));
        let mut lone: Vec<(String, TerminalId)> = Vec::new();
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
            let (page, at, _) = terminal_at(doc, *first);
            let n = lone.len();
            let names: Vec<String> = lone.iter().take(8).map(|(w, _)| w.clone()).collect();
            let more = if n > 8 { ", …" } else { "" };
            out.push(Check {
                kind: CheckKind::ConnectorLonePin,
                page,
                at,
                object: Object::Component(c),
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
            let (page, at, _) = t.map_or((None, None, None), |t| terminal_at(doc, t));
            out.push(Check {
                kind: CheckKind::ConnectorOver,
                page,
                at,
                object: Object::Component(c),
                which: format!("{tag} has pin {v} beyond its {n} pins"),
                fix: u32::try_from(v).ok().map(|v| Fix::SetPins(c, v)),
            });
        }
    }
    out
}

// ---------------------------------------------------------------- references (design 044 §7)

/// The reference members of connector `c` (design 044 §7, design 052 §3.4): each half's
/// whole-half symbol ([`crate::reps::half_anchor`]) is a half of that side, every other connector
/// symbol a pin of its side (a pin pair: `Both`). A pin-only half has no half member: its pins
/// keep their mate edges.
pub fn ref_classes(doc: &Document, c: ComponentId) -> Vec<(RepresentationId, crate::xref::Class)> {
    use crate::xref::Class;
    if !is_connector(doc, c) {
        return Vec::new();
    }
    let halves: BTreeMap<RepresentationId, Side> =
        [Side::Socket, Side::Plug].into_iter().filter_map(|s| crate::reps::half_anchor(doc, c, s).map(|r| (r, s))).collect();
    doc.representations_of(Subject::Component(c))
        .filter_map(|(rid, r)| {
            let side = rep_side(doc, r)?;
            Some(match halves.get(&rid) {
                Some(s) => (rid, Class::Half(*s)),
                None => (rid, Class::Pin(side)),
            })
        })
        .collect()
}

/// The edges between connector `c`'s members (design 044 §7): each half's whole-half symbol shows
/// the other half's, and a pin whose mate is drawn only on other pages shows those.
pub fn ref_edges(doc: &Document, c: ComponentId, list: &[(RepresentationId, crate::xref::Class)]) -> Vec<(RepresentationId, RepresentationId)> {
    use crate::xref::Class;
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
        let Some(r) = doc.representations.get(rid) else { continue };
        let mut mates: BTreeSet<RepresentationId> = BTreeSet::new();
        let mut here = false;
        for t in r.connection_points.keys() {
            let Some(x) = doc.terminals.get(t) else { continue };
            let other = side.other().key();
            for (m, mx) in doc.terminals_of(c) {
                if mx.label != x.label || mx.key.as_deref() != other {
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

/// The hover line of a connector half (design 044 §7): `-X10.S (socket, 4 pins) mates with -X10.P
/// on 102 C4.`, or `-X10.S has no plug half.`
pub fn hover_words(doc: &Document, c: ComponentId, side: Side) -> Option<String> {
    if !is_connector(doc, c) || side == Side::Both {
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

// ---------------------------------------------------------------- the index (design 044 §11.1)

/// The connector index (design 044 §11.1): which connectors have a symbol on each page, and which
/// read each naming key (separator, side names) from the project or the built-in. Built on open,
/// reload and branch switch ([`Index::build`]); kept per commit, undo, redo and merge from the
/// ops ([`Index::touched`], [`Index::update`]): only the connectors an op names are recomputed.
///
/// # Update map
/// | Op (kind, target) | Recomputed |
/// | --- | --- |
/// | `AddTerminal` / `RemoveTerminal` | that terminal's component |
/// | `LinkRepresentation` / `UnlinkRepresentation` / `ModifyRepresentation` | the component shown |
/// | `AddItem` of a symbol | the component its representation shows |
/// | `CreateComponent` / `DeleteComponent` | that component (added / dropped) |
/// | `SetProperty` of a component | that component (its naming keys) |
/// | `EmbedDefinition` / `RemoveDefinition` | the components using it |
/// | anything else | nothing |
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Index {
    /// Page → the connectors with a symbol there.
    pub by_page: BTreeMap<PageId, BTreeSet<ComponentId>>,
    /// Naming key → the connectors that inherit it (no own value).
    pub inherits: BTreeMap<&'static str, BTreeSet<ComponentId>>,
    /// Each connector's pages, to take it out again.
    pages_of: BTreeMap<ComponentId, BTreeSet<PageId>>,
}

/// The naming keys a connector may hold (design 044 §10).
fn naming_keys() -> [&'static str; 3] {
    use crate::defaults::connectors as dc;
    [dc::SEPARATOR.key, dc::SIDE_SOCKET.key, dc::SIDE_PLUG.key]
}

/// How many connectors the last [`Index::update`] on this thread recomputed (tests and bench:
/// never the whole project on an ordinary edit).
pub fn index_work() -> usize {
    INDEX_WORK.with(std::cell::Cell::get)
}

thread_local! {
    static INDEX_WORK: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

impl Index {
    /// The whole index of `doc`: O(components).
    pub fn build(doc: &Document) -> Index {
        let mut idx = Index::default();
        for c in doc.components.keys() {
            idx.set(doc, *c);
        }
        idx
    }

    /// The components whose entries `ops` (just applied to `doc`) could change.
    pub fn touched(doc: &Document, ops: &[Op]) -> BTreeSet<ComponentId> {
        let mut out = BTreeSet::new();
        let subject = |r: &Representation, out: &mut BTreeSet<ComponentId>| {
            if let Subject::Component(c) = r.subject {
                out.insert(c);
            }
        };
        for op in ops {
            match op {
                Op::AddTerminal { terminal, .. } | Op::RemoveTerminal { terminal, .. } => {
                    out.insert(terminal.component);
                }
                Op::LinkRepresentation { rep, .. } | Op::UnlinkRepresentation { rep, .. } => subject(rep, &mut out),
                Op::ModifyRepresentation { before, after, .. } => {
                    subject(before, &mut out);
                    subject(after, &mut out);
                }
                Op::AddItem { id, .. } => {
                    if let Some((_, r)) = doc.representation_of_item(*id) {
                        subject(r, &mut out);
                    }
                }
                Op::CreateComponent { id, .. } | Op::DeleteComponent { id, .. } | Op::SetComponentDefinition { id, .. } => {
                    out.insert(*id);
                }
                Op::SetProperty { object: ecad_model::ObjectRef::Component(c), .. } => {
                    out.insert(*c);
                }
                Op::EmbedDefinition { id, .. } | Op::RemoveDefinition { id, .. } => out.extend(doc.components_using(*id)),
                _ => {}
            }
        }
        out
    }

    /// Recompute `touched`'s entries from `doc`.
    pub fn update(&mut self, doc: &Document, touched: &BTreeSet<ComponentId>) {
        INDEX_WORK.with(|w| w.set(touched.len()));
        for c in touched {
            self.set(doc, *c);
        }
    }

    /// Take connector `c` out, then put it back as `doc` has it.
    fn set(&mut self, doc: &Document, c: ComponentId) {
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
        if !is_connector(doc, c) {
            return;
        }
        let pages: BTreeSet<PageId> = doc.representations_of(Subject::Component(c)).map(|(_, r)| r.page).collect();
        for p in &pages {
            self.by_page.entry(*p).or_default().insert(c);
        }
        self.pages_of.insert(c, pages);
        let props = doc.components.get(&c).map(|x| &x.properties);
        for k in naming_keys() {
            if !props.is_some_and(|p| p.contains_key(k)) {
                self.inherits.entry(k).or_default().insert(c);
            }
        }
    }

    /// The connectors that inherit `key` from the project: a naming key's inheritors; for any
    /// other `connector.` key (a description), every connector.
    pub fn inheriting(&self, key: &str) -> BTreeSet<ComponentId> {
        match self.inherits.get(key) {
            Some(s) => s.clone(),
            None if naming_keys().contains(&key) => BTreeSet::new(),
            None => self.pages_of.keys().copied().collect(),
        }
    }

    /// The pages showing the connectors that inherit `key` (the one wide case of §11.1).
    pub fn pages_inheriting(&self, key: &str) -> BTreeSet<PageId> {
        self.inheriting(key).iter().flat_map(|c| self.pages_of.get(c).into_iter().flatten().copied()).collect()
    }

    /// The pages a project `connector.` naming or description key in `ops` redraws.
    pub fn pages_for(&self, ops: &[Op]) -> BTreeSet<PageId> {
        let mut out = BTreeSet::new();
        for op in ops {
            if let Op::SetProperty { object: ecad_model::ObjectRef::Project, key, .. } = op
                && key.starts_with("connector.")
                && (crate::defaults::connectors::is_naming_key(key) || key.ends_with(".description"))
            {
                out.extend(self.pages_inheriting(key));
            }
        }
        out
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn families_name_their_halves() {
        assert_eq!(Family::Dsub.half_key(Side::Plug, 9).as_deref(), Some("dsub9p"));
        assert_eq!(Family::M12.half_key(Side::Socket, 5).as_deref(), Some("m12s5"));
        assert_eq!(Family::Plain.half_key(Side::Socket, 5), None);
        assert_eq!(Family::parse("D-sub"), Some(Family::Dsub));
        assert_eq!(Family::Dsub.size_refusal(), "A D-sub has 9, 15 or 25 pins. Pick one, or draw pin by pin with Pins.");
        for f in Family::ALL {
            for n in f.sizes() {
                for side in [Side::Socket, Side::Plug] {
                    let key = f.half_key(side, *n).unwrap_or_default();
                    let s = symbols::by_key(&key).unwrap_or_else(|| panic!("{key}"));
                    assert_eq!((family_of(s), size_of(s), s.side()), (Some(f), Some(*n), Some(side)), "{key}");
                }
            }
        }
    }

    #[test]
    fn names_are_checked() {
        assert_eq!(names_for(&Names::Letters, 3), Ok(vec!["A".into(), "B".into(), "C".into()]));
        assert!(names_for(&Names::Letters, 27).is_err());
        assert_eq!(parse_list("L, N ,PE"), Ok(vec!["L".into(), "N".into(), "PE".into()]));
        assert!(parse_list("P E").is_err_and(|e| e.contains("spaces")));
        assert!(parse_list("L,L").is_err());
        assert!(parse_list(&"x,".repeat(5_000)).is_err());
        assert!(check_name("a\u{1}").is_err());
        assert!(check_name("123456789").is_err());
        assert!(names_for(&Names::List(vec!["L".into()]), 3).is_err_and(|e| e == "Give 3 pin names; got 1."));
    }
}
