//! The rules of brief 044's commands (design 044 §4, §5.3): `connector` (Place ▸ Connector: a row of
//! whole halves, pins on wires, or a bulkhead) and `pinlabel` (rename a pin on both halves, or move
//! one drawn pin to its half's next free pin). Each function reads the document and returns the ops
//! to commit and what to report; the engine crate's shims read the parameters, commit and answer
//! in JSON.

use ecad_model::geometry::units::{Degrees, Radians};
use serde_json::{Value, json};
use std::collections::BTreeSet;

use ecad_model::{DeviceId, Document, IdAllocator, Ledger, Noun, Op, PageId, Placement, Pt, RouteId, Style, ConnectionPoint, ConnectionPointId, WireId};

use crate::connectors::{self as cn, FamilyRules, Names, Shown};
use crate::defaults::{self, Scope};
use crate::devices::TagHigh;
use crate::error::CmdError;
use crate::symbols::{Side, Starter};
use ecad_model::objects::devices::ConnectorFamily;

/// The refusal of a `connector` call with nowhere to place.
pub const NEEDS_AT: &str = "connector needs at for a row, or line or wire for pins.";

/// The connector `device` names: a hex ID or a tag (`None`: none named).
///
/// # Errors
/// A tag that does not read, or names no device.
pub fn device(doc: &Document, device: Option<&str>) -> Result<Option<DeviceId>, CmdError> {
    let Some(t) = device else { return Ok(None) };
    let t = t.trim();
    if let Ok(n) = u128::from_str_radix(t, 16)
        && doc.devices.contains_key(&DeviceId::from(n))
    {
        return Ok(Some(DeviceId::from(n)));
    }
    let tag = ecad_model::naming::tag::normalise(t, crate::defaults::print::tag_style(doc))?;
    crate::devices::holder_of(doc, &tag, None).map(Some).ok_or_else(|| CmdError::refused(format!("No device {tag}.")))
}

/// The page `page` names, else `active`.
///
/// # Errors
/// No such page.
pub fn page(doc: &Document, page: Option<PageId>, active: PageId) -> Result<PageId, CmdError> {
    match page {
        Some(p) if doc.pages.contains_key(&p) => Ok(p),
        Some(_) => Err(CmdError::NotFound(Noun::Page)),
        None => Ok(active),
    }
}

/// How `names` was given: a word (`numbers`, `letters`, or a list written as text) or a list.
#[derive(Debug, Clone, PartialEq)]
pub enum NamesInput {
    /// A list of pin names.
    List(Vec<String>),
    /// `numbers`, `letters`, or the names as text (`L,N,PE`).
    Word(String),
}

/// The pin names a call asks for (absent: the registry's `connector.maker.names`).
///
/// # Errors
/// The registry asks for a list, or the names do not read.
pub fn names(doc: &Document, names: Option<&NamesInput>) -> Result<Names, CmdError> {
    match names {
        None => match defaults::resolve(doc, &defaults::connectors::MAKER_NAMES, Scope::project()).word() {
            "letters" => Ok(Names::Letters),
            "list" => Err(CmdError::refused("Give the pin names as a list: names [\"L\",\"N\",\"PE\"].")),
            _ => Ok(Names::Numbers),
        },
        Some(NamesInput::Word(w)) => match w.trim().to_ascii_lowercase().as_str() {
            "numbers" => Ok(Names::Numbers),
            "letters" => Ok(Names::Letters),
            _ => cn::parse_list(w).map(Names::List),
        },
        Some(NamesInput::List(a)) => {
            if a.len() > cn::MAX_PINS {
                return Err(CmdError::refused(format!("A connector has at most {} pins; got {}.", cn::MAX_PINS, a.len())));
            }
            let v: Vec<String> = a.iter().map(|s| s.trim().to_string()).collect();
            cn::check_list(&v)?;
            Ok(Names::List(v))
        }
    }
}

/// The halves a call asks for (absent: the registry's `connector.maker.halves`).
///
/// # Errors
/// A word that is not socket, plug or both.
pub fn halves(doc: &Document, halves: Option<&str>) -> Result<Side, CmdError> {
    let w = match halves {
        None => defaults::resolve(doc, &defaults::connectors::MAKER_HALVES, Scope::project()).word().to_string(),
        Some(w) => w.trim().to_ascii_lowercase(),
    };
    match w.as_str() {
        "socket" | "s" => Ok(Side::Socket),
        "plug" | "p" => Ok(Side::Plug),
        "both" | "b" => Ok(Side::Both),
        _ => Err(CmdError::input("`halves` is socket, plug or both")),
    }
}

/// The family a call asks for (absent: the registry's `connector.maker.family`).
///
/// # Errors
/// A word that names no family.
pub fn family(doc: &Document, family: Option<&str>) -> Result<ConnectorFamily, CmdError> {
    match family {
        None => Ok(ConnectorFamily::of_typed(defaults::resolve(doc, &defaults::connectors::MAKER_FAMILY, Scope::project()).word())
            .unwrap_or(ConnectorFamily::Plain)),
        Some(w) => ConnectorFamily::of_typed(w).ok_or(CmdError::input("`family` is plain, dsub, m12, rj45, usba or usbb")),
    }
}

/// The pin count a call asks for.
///
/// # Errors
/// A count outside 1 to [`cn::MAX_PINS`].
pub fn pins(pins: Option<u64>) -> Result<Option<usize>, CmdError> {
    match pins {
        None => Ok(None),
        Some(n) => usize::try_from(n)
            .ok()
            .filter(|n| (1..=cn::MAX_PINS).contains(n))
            .map(Some)
            .ok_or_else(|| CmdError::input(format!("`pins` is a whole number from 1 to {}.", cn::MAX_PINS))),
    }
}

/// Whether a call asks for the pins layout (absent: the registry's `connector.maker.layout`).
///
/// # Errors
/// A word that is not row or pins.
pub fn pins_layout(doc: &Document, layout: Option<&str>) -> Result<bool, CmdError> {
    let w = match layout {
        None => defaults::resolve(doc, &defaults::connectors::MAKER_LAYOUT, Scope::project()).word().to_string(),
        Some(w) => w.trim().to_ascii_lowercase(),
    };
    match w.as_str() {
        "row" => Ok(false),
        "pins" => Ok(true),
        _ => Err(CmdError::input("`layout` is row or pins")),
    }
}

/// The registry's first pin count.
fn maker_pins(doc: &Document) -> usize {
    let n = defaults::resolve(doc, &defaults::connectors::MAKER_PINS, Scope::project()).number();
    if n.is_finite() && n >= 1.0 && n <= cn::MAX_PINS as f64 { n as usize } else { 4 }
}

/// `1–4` or `3`.
pub fn span(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [a, .., b] => format!("{a}\u{2013}{b}"),
    }
}

/// What a placement made, for the result and the echo.
#[derive(Debug, Clone)]
pub struct Made {
    pub component: DeviceId,
    pub tag: String,
    pub new: bool,
    pub shown: Vec<Shown>,
    /// The routes split by pins: (route, its new second part).
    pub splits: Vec<(RouteId, RouteId)>,
    /// The transaction's ops.
    pub ops: Vec<Op>,
}

/// What `connector` is asked to place (design 044 §4.3); absent words take the registry's
/// `connector.maker.*` answers.
#[derive(Debug, Clone, Default)]
pub struct ConnectorInput {
    /// The page (absent: the active one).
    pub page: Option<PageId>,
    /// The connector to join (an id or a tag; absent: a new connector).
    pub device: Option<String>,
    pub halves: Option<String>,
    pub family: Option<String>,
    pub pins: Option<u64>,
    pub names: Option<NamesInput>,
    pub layout: Option<String>,
    /// Where a row (or a bulkhead) goes.
    pub at: Option<Pt>,
    /// The line pins are placed along.
    pub line: Option<(Pt, Pt)>,
    /// The wire one pin lands on.
    pub wire: Option<WireId>,
    /// 0, 90, 180 or 270.
    pub degrees: Degrees,
    /// A row's halves swapped.
    pub swap: bool,
    /// The first pin's name.
    pub pin: Option<String>,
}

/// A planned `connector` call.
#[derive(Debug, Clone)]
pub struct Connector {
    pub made: Made,
    /// The transaction's label.
    pub label: String,
    pub family: ConnectorFamily,
    pub halves: Side,
    pub pins_layout: bool,
}

/// Plan `connector` (design 044 §4.3) on `active` (or the page the input names).
///
/// # Errors
/// A word, page or device that does not read, or a refusal of the placement.
#[allow(clippy::too_many_arguments)]
pub fn plan_connector(
    ledger: &Ledger, ids: &mut IdAllocator, high: &mut TagHigh, active: PageId, style: Style, input: ConnectorInput, tol: f64,
) -> Result<Connector, CmdError> {
    let doc = ledger.document();
    let page = page(doc, input.page, active)?;
    let device = device(doc, input.device.as_deref())?;
    if let Some(c) = device
        && !cn::connector_of(doc, c).is_some_and(|x| x.mating)
    {
        return Err(CmdError::refused(cn::not_a_connector(doc, c)));
    }
    let halves = halves(doc, input.halves.as_deref())?;
    let family = match device {
        Some(c) if input.family.is_none() => cn::half_family(doc, c, Side::Socket),
        _ => family(doc, input.family.as_deref())?,
    };
    let pins_given = pins(input.pins)?;
    let names = names(doc, input.names.as_ref())?;
    let pins_layout = pins_layout(doc, input.layout.as_deref())?;
    let (made, label) = if let Some(key) = family.bulkhead() {
        let at = input.at.ok_or(CmdError::refused(NEEDS_AT))?;
        (place_bulkhead(ledger, ids, high, page, style, key, at, input.degrees)?, "Place connector".to_string())
    } else if pins_layout {
        let (from, to) = match (input.line, input.wire) {
            (Some(l), _) => l,
            (None, Some(w)) => {
                let mid = crate::wires::route_of_wire_on(doc, w, page)
                    .and_then(|r| route_point(doc, r))
                    .ok_or(CmdError::refused("That wire is not drawn on this page."))?;
                (mid, mid)
            }
            (None, None) => return Err(CmdError::refused(NEEDS_AT)),
        };
        let route = input.wire.and_then(|w| crate::wires::route_of_wire_on(doc, w, page));
        let pins = Pins { device, halves, pins: pins_given, names: &names, pin: input.pin.as_deref(), from, to, route };
        let made = place_pins(ledger, ids, high, page, style, pins, tol)?;
        let names: Vec<String> = made.shown.iter().filter(|x| x.side != Side::Plug || halves == Side::Plug).map(|x| x.name.clone()).collect();
        let label = format!("Place pins {}:{}", made.tag, span(&names));
        (made, label)
    } else {
        let at = input.at.ok_or(CmdError::refused(NEEDS_AT))?;
        let row = Row { device, halves, family, pins: pins_given, names: &names, at, degrees: input.degrees, swap: input.swap };
        let made = place_row(ledger, ids, high, page, style, row)?;
        let label = format!("Place connector {}", made.tag);
        (made, label)
    };
    Ok(Connector { made, label, family, halves, pins_layout })
}

/// The echo after a commit (design 044 §4.2), read from the document with the placement in.
pub fn echo(doc: &Document, c: &Connector) -> String {
    let m = &c.made;
    let side_names = |side: Side| -> Vec<String> { m.shown.iter().filter(|x| x.side == side).map(|x| x.name.clone()).collect() };
    let mut parts = Vec::new();
    for side in [Side::Socket, Side::Plug] {
        let names = side_names(side);
        if !names.is_empty() {
            parts.push(format!("{}:{}", cn::half_tag(doc, m.component, side), span(&names)));
        }
    }
    if c.pins_layout {
        let n = m.splits.len();
        let what = match c.halves {
            Side::Both => "pin pairs",
            Side::Socket => "socket pins",
            Side::Plug => "plug pins",
        };
        let names: Vec<String> = if c.halves == Side::Plug { side_names(Side::Plug) } else { side_names(Side::Socket) };
        if n > 0 {
            format!("Placed {}:{} on {n} wire{} ({what}).", m.tag, span(&names), if n == 1 { "" } else { "s" })
        } else {
            format!("Placed {}:{} ({what}).", m.tag, span(&names))
        }
    } else if c.family.bulkhead().is_some() {
        format!("Placed {} ({} bulkhead).", m.tag, c.family.name())
    } else {
        let n = cn::pin_count(doc, m.component, &cn::drawn(doc, m.component));
        format!("Placed {} ({}, {n} pins): {}.", m.tag, c.family.name(), parts.join(", "))
    }
}

/// A point on route `r`: the middle of its longest segment.
fn route_point(doc: &Document, r: RouteId) -> Option<Pt> {
    let v = &doc.routes.get(&r)?.points;
    let mut best: Option<(f64, Pt)> = None;
    for pair in v.windows(2) {
        let (Some(a), Some(b)) = (pair.first(), pair.get(1)) else { continue };
        let d = a.dist(*b);
        if d.is_finite() && best.is_none_or(|(bd, _)| d > bd) {
            best = Some((d, Pt::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)));
        }
    }
    best.map(|(_, p)| p)
}

/// A bulkhead (RJ45, USB): 034's one-piece symbol, a new device, as Find Symbol places it.
#[allow(clippy::too_many_arguments)]
fn place_bulkhead(
    ledger: &Ledger, ids: &mut IdAllocator, high: &mut TagHigh, page: PageId, style: Style, key: &str, at: Pt, degrees: Degrees,
) -> Result<Made, CmdError> {
    let starter = crate::devices::starter(key)?;
    let placement = Placement { origin: at, rotation: Radians::from(degrees), mirror: false, scale: 1.0 };
    let plan = crate::devices::plan_place(ledger, ids, high, page, style, starter, placement, crate::devices::Target::New(None))?;
    Ok(Made { component: plan.component, tag: plan.tag, new: true, shown: Vec::new(), splits: Vec::new(), ops: plan.ops })
}

/// The names a whole half of `n` pins takes on connector `c`: its drawn names in order, then the
/// next numbers not used.
pub fn join_names(doc: &Document, c: DeviceId, n: usize) -> Vec<String> {
    let d = cn::drawn(doc, c);
    let mut out: Vec<String> = d.order.iter().take(n).cloned().collect();
    let mut k = 1usize;
    while out.len() < n && k <= cn::MAX_PINS * 2 {
        let name = k.to_string();
        if !d.order.contains(&name) && !out.contains(&name) {
            out.push(name);
        }
        k += 1;
    }
    out
}

/// What a row places.
struct Row<'a> {
    device: Option<DeviceId>,
    halves: Side,
    family: ConnectorFamily,
    pins: Option<usize>,
    names: &'a Names,
    at: Pt,
    degrees: Degrees,
    swap: bool,
}

/// Row: whole halves (design 044 §4.2): Both places the plug at `at` and the socket 5 mm above it
/// in the pair's frame (below with `swap`), so their bodies share an edge.
fn place_row(ledger: &Ledger, ids: &mut IdAllocator, high: &mut TagHigh, page: PageId, style: Style, r: Row<'_>) -> Result<Made, CmdError> {
    let doc = ledger.document();
    let (device, halves, family, names, at, swap) = (r.device, r.halves, r.family, r.names, r.at, r.swap);
    let n = match (r.pins, device) {
        (Some(n), _) => n,
        (None, Some(c)) => cn::pin_count(doc, c, &cn::drawn(doc, c)),
        (None, None) => match names {
            Names::List(v) => v.len(),
            _ => maker_pins(doc),
        },
    };
    if !family.sizes().contains(&n) {
        return Err(CmdError::refused(family.size_refusal()));
    }
    let sides: Vec<Side> = match halves {
        Side::Both => vec![Side::Plug, Side::Socket],
        s => vec![s],
    };
    let taken = BTreeSet::new();
    if let Some(c) = device {
        for side in &sides {
            cn::check_whole_half(doc, c, *side, &taken)?;
        }
    }
    let pin_names = match device {
        Some(c) if !matches!(names, Names::List(_)) => join_names(doc, c, n),
        _ => cn::names_for(names, n)?,
    };
    let rot = Radians::from(r.degrees);
    let (sin, cos) = rot.sin_cos();
    let up = if swap { -5.0 } else { 5.0 };
    let mut ops = Vec::new();
    let mut shown = Vec::new();
    let mut made: Option<(DeviceId, String, bool)> = device.map(|c| (c, crate::devices::tag_words(doc, c), false));
    for side in sides {
        let key = family.half_key(side, n).ok_or_else(|| CmdError::refused(family.size_refusal()))?;
        let starter = crate::devices::starter(&key)?;
        let def = crate::terminal_strips::commands::terminal_strips::definition_for(doc, ids, starter, &mut ops)?;
        let origin = if side == Side::Socket && halves == Side::Both { Pt::new(at.x - sin * up, at.y + cos * up) } else { at };
        let placement = if swap {
            Placement { origin, rotation: Radians(rot.0 + std::f64::consts::PI), mirror: true, scale: 1.0 }
        } else {
            Placement { origin, rotation: rot, mirror: false, scale: 1.0 }
        };
        let c = match &made {
            Some((c, _, _)) => *c,
            None => {
                let list = matches!(names, Names::List(_)).then_some(pin_names.as_slice());
                let (c, tag) = cn::create_ops(ledger, ids, high, starter, def, None, Some(n), list, &mut ops)?;
                made = Some((c, tag, true));
                c
            }
        };
        let (_, _, sh) = cn::symbol_ops(ids, page, style.clone(), def, starter, c, &pin_names, placement, &mut ops)?;
        shown.extend(sh);
    }
    let (component, tag, new) = made.ok_or(CmdError::refused("Nothing to place."))?;
    Ok(Made { component, tag, new, shown, splits: Vec::new(), ops })
}

/// The rotation of a pin symbol on a route segment: its points along the route.
pub fn along_wire(doc: &Document, on: &crate::wires::OnRoute) -> Radians {
    let Some(v) = doc.routes.get(&on.route).map(|r| &r.points) else { return Radians(0.0) };
    match (v.get(on.segment), v.get(on.segment + 1)) {
        (Some(a), Some(b)) if (b.x - a.x).abs() > (b.y - a.y).abs() => Radians(std::f64::consts::FRAC_PI_2),
        _ => Radians(0.0),
    }
}

/// What a pins placement places.
struct Pins<'a> {
    device: Option<DeviceId>,
    halves: Side,
    pins: Option<usize>,
    names: &'a Names,
    pin: Option<&'a str>,
    from: Pt,
    to: Pt,
    route: Option<RouteId>,
}

/// Pins: one pin symbol on each wire the line crosses (design 044 §4.2, §4.4, §4.5).
fn place_pins(
    ledger: &Ledger, ids: &mut IdAllocator, high: &mut TagHigh, page: PageId, style: Style, a: Pins<'_>, tol: f64,
) -> Result<Made, CmdError> {
    let doc = ledger.document();
    let (device, halves, names, from, to) = (a.device, a.halves, a.names, a.from, a.to);
    let crossings: Vec<crate::wires::OnRoute> = if from.near(to, 1e-9) {
        let r = a
            .route
            .or_else(|| crate::wires::route_at(doc, page, from, crate::wires::ON_WIRE, None).map(|o| o.route))
            .ok_or(CmdError::refused("Click on a wire, or draw a line across the wires."))?;
        vec![crate::wires::on_route_of(doc, r, from).ok_or(CmdError::refused("That wire is not drawn here."))?]
    } else {
        let found = crate::cables::slash::crossings(doc, page, from, to, tol);
        if found.members.len() > cn::MAX_PINS {
            return Err(CmdError::refused(format!(
                "That line crosses {} wires; a connector has at most {} pins. Draw a shorter line.",
                found.members.len(),
                cn::MAX_PINS
            )));
        }
        found.members.iter().filter_map(|m| crate::wires::on_route_of(doc, m.route, m.at)).collect()
    };
    if crossings.is_empty() {
        return Err(CmdError::refused("The line crosses no wire. Draw it across the wires."));
    }
    let key = match halves {
        Side::Both => "pinpair",
        Side::Socket => "pins",
        Side::Plug => "pinp",
    };
    let starter: &Starter = crate::devices::starter(key)?;
    let mut ops = Vec::new();
    let def = crate::terminal_strips::commands::terminal_strips::definition_for(doc, ids, starter, &mut ops)?;
    // A new connector: its names first, then numbers past them (Numbers only).
    let (component, tag, new, mut fresh) = match device {
        Some(c) => (c, crate::devices::tag_words(doc, c), false, Vec::new()),
        None => {
            let n = match (a.pins, names) {
                (Some(n), _) => n,
                (None, Names::List(v)) => v.len(),
                (None, _) => maker_pins(doc).max(crossings.len().min(cn::MAX_PINS)),
            };
            let list = cn::names_for(names, n)?;
            let typed = matches!(names, Names::List(_)).then_some(list.as_slice());
            let (c, t) = cn::create_ops(ledger, ids, high, starter, def, None, Some(n), typed, &mut ops)?;
            (c, t, true, list)
        }
    };
    let mut taken: BTreeSet<(Side, String)> = BTreeSet::new();
    let mut named = a.pin.map(str::to_string);
    let mut shown_all = Vec::new();
    let mut splits = Vec::new();
    let take = |side: Side, name: &str, taken: &mut BTreeSet<(Side, String)>| match side {
        Side::Both => {
            taken.insert((Side::Socket, name.to_string()));
            taken.insert((Side::Plug, name.to_string()));
        }
        s => {
            taken.insert((s, name.to_string()));
        }
    };
    for (i, on) in crossings.into_iter().enumerate() {
        let name = match named.take() {
            Some(n) => {
                if !new {
                    cn::check_named_pin(doc, component, halves, &n, &taken)?;
                } else {
                    cn::check_name(&n)?;
                }
                n
            }
            None if new => match fresh.get(i) {
                Some(n) => n.clone(),
                None if matches!(names, Names::Numbers) => {
                    let k = fresh.len() + 1;
                    if k > cn::MAX_PINS {
                        return Err(CmdError::refused(format!("A connector has at most {} pins.", cn::MAX_PINS)));
                    }
                    fresh.push(k.to_string());
                    k.to_string()
                }
                None => {
                    return Err(CmdError::refused(format!(
                        "{tag} has {} named pins; the line crosses more wires. Name more pins, or draw a shorter line.",
                        fresh.len()
                    )));
                }
            },
            None => cn::next_pin(doc, component, halves, &taken)?,
        };
        if new && taken.iter().any(|(_, n)| *n == name) {
            return Err(CmdError::refused(format!("{tag}:{name} is already being drawn. Pick another pin.")));
        }
        take(halves, &name, &mut taken);
        let rotation = along_wire(doc, &on);
        let placement = Placement { origin: on.at, rotation, mirror: false, scale: 1.0 };
        let (_, _, shown) = cn::symbol_ops(ids, page, style.clone(), def, starter, component, std::slice::from_ref(&name), placement, &mut ops)?;
        let land = cn::land_pin(doc, ids, on, &shown, placement).map_err(|e| match crate::wires::wire_of_route(doc, on.route) {
            Some(w) => CmdError::refused(format!("{e} (wire {})", crate::cables::slash::wire_name(doc, w))),
            None => e,
        })?;
        splits.push((land.route, land.new));
        ops.extend(land.ops);
        shown_all.extend(shown);
    }
    Ok(Made { component, tag, new, shown: shown_all, splits, ops })
}

// ---------------------------------------------------------------- pinlabel (§5.3)

/// What `pinlabel` is asked to rename.
#[derive(Debug, Clone, Default)]
pub struct PinlabelInput {
    /// The connector (an id or a tag).
    pub device: Option<String>,
    /// With `device`: the pin's current name.
    pub pin: Option<String>,
    /// One drawn pin.
    pub terminal: Option<ConnectionPointId>,
    /// The new name, or `next`.
    pub to: String,
}

/// A planned `pinlabel` call.
#[derive(Debug, Clone)]
pub enum Pinlabel {
    /// The pin already has that name.
    Unchanged { component: DeviceId, to: String },
    /// One drawn pin moves to its half's next free pin.
    Moved { component: DeviceId, from: String, to: String, ops: Vec<Op>, label: String, message: String },
    /// The pin is renamed on both halves; [`renamed_message`] says so after the commit.
    Renamed { component: DeviceId, tag: String, from: String, to: String, ops: Vec<Op>, label: String },
}

/// Plan `pinlabel` (design 044 §5.3).
///
/// # Errors
/// No such device or connection point, not a connector, a name that does not read or is taken.
pub fn plan_pinlabel(doc: &Document, input: PinlabelInput) -> Result<Pinlabel, CmdError> {
    let PinlabelInput { device: dev, pin, terminal, to } = input;
    let to = to.trim().to_string();
    let (c, old) = match (terminal, device(doc, dev.as_deref())?) {
        (Some(t), _) => {
            let x = doc.terminals.get(&t).ok_or(CmdError::NotFound(Noun::Terminal))?;
            (x.device().ok_or(CmdError::NotFound(Noun::Terminal))?, x.label.clone().ok_or(CmdError::refused("That pin has no name."))?)
        }
        (None, Some(c)) => (c, pin.map(|p| p.trim().to_string()).ok_or(CmdError::input("`pin` (the pin's current name) is required with `device`"))?),
        (None, None) => return Err(CmdError::input("`device` and `pin`, or `terminal`, is required")),
    };
    if !cn::connector_of(doc, c).is_some_and(|x| x.mating) {
        return Err(CmdError::refused(format!(
            "{} is not a connector; rename its connection points in Properties.",
            crate::devices::tag_words(doc, c)
        )));
    }
    let tag = crate::devices::tag_words(doc, c);
    // `to: "next"`: one drawn pin moves to its half's next free pin (the pin-twice fix).
    if to.eq_ignore_ascii_case("next") {
        let t = terminal.ok_or(CmdError::input("`to: \"next\"` moves one drawn pin: give its `terminal`"))?;
        let side = cn::side_of(doc, t).ok_or(CmdError::refused("That is not a connector pin."))?;
        let x = doc.terminals.get(&t).ok_or(CmdError::NotFound(Noun::Terminal))?.clone();
        // The pin itself is not free for itself: drawn once more on this side.
        let next = cn::next_pin(doc, c, side, &BTreeSet::new())?;
        let after = ConnectionPoint { label: Some(next.clone()), ..x.clone() };
        let ops = vec![Op::RemoveConnectionPoint { id: t, terminal: x }, Op::AddConnectionPoint { id: t, terminal: after }];
        let half = cn::half_tag(doc, c, side);
        let message = format!("{half}:{old} is now {half}:{next}.");
        return Ok(Pinlabel::Moved { component: c, label: format!("Move pin {half}:{old}"), from: old, to: next, ops, message });
    }
    cn::check_name(&to)?;
    let targets: Vec<(ConnectionPointId, ConnectionPoint)> =
        doc.connection_points_of(c).filter(|(_, x)| x.label.as_deref() == Some(old.as_str())).map(|(t, x)| (t, x.clone())).collect();
    if targets.is_empty() {
        return Err(CmdError::refused(format!("{tag} has no pin {old}.")));
    }
    if to == old {
        return Ok(Pinlabel::Unchanged { component: c, to });
    }
    if doc.connection_points_of(c).any(|(_, x)| x.label.as_deref() == Some(to.as_str())) || cn::pin_names(doc, c).is_some_and(|l| l.contains(&to)) {
        return Err(CmdError::refused(format!("{tag} already has a pin {to}. Pick another name, or rename that pin first.")));
    }
    let mut ops = Vec::new();
    for (t, x) in &targets {
        ops.push(Op::RemoveConnectionPoint { id: *t, terminal: x.clone() });
    }
    for (t, x) in targets {
        ops.push(Op::AddConnectionPoint { id: t, terminal: ConnectionPoint { label: Some(to.clone()), ..x } });
    }
    // A typed list keeps the pin's place under its new name.
    if let Some(list) = cn::pin_names(doc, c)
        && list.contains(&old)
    {
        let after: Vec<String> = list.iter().map(|n| if *n == old { to.clone() } else { n.clone() }).collect();
        let names = cn::typed_names(&after)?;
        crate::devices::edit_planned(doc, &mut ops, c, |d| {
            if let ecad_model::DeviceType::Connector(x) = &mut d.device_type {
                x.pin_names = names;
            }
        });
    }
    let label = format!("Rename pin {tag}:{old}");
    Ok(Pinlabel::Renamed { component: c, tag, from: old, to, ops, label })
}

/// The echo of a rename, read from the document with it in.
pub fn renamed_message(doc: &Document, c: DeviceId, tag: &str, from: &str, to: &str) -> String {
    let d = cn::drawn(doc, c);
    let both = d.socket.contains_key(to) && d.plug.contains_key(to);
    format!("{tag}:{from} is now {tag}:{to}{}.", if both { " (both halves)" } else { "" })
}

pub fn result_json(doc: &Document, m: &Made, message: &str) -> Value {
    json!({
        "component": m.component.hex(),
        "tag": m.tag,
        "new": m.new,
        "terminals": m.shown.iter().map(|x| json!({
            "terminal": x.terminal.hex(),
            "side": x.side.word(),
            "key": x.side.key(),
            "label": x.name,
            "pin": x.name,
            "name": cn::pin_name(doc, x.terminal).unwrap_or_else(|| format!("{}:{}", m.tag, x.name)),
        })).collect::<Vec<_>>(),
        "split": m.splits.iter().map(|(r, n)| json!({"route": r.hex(), "new": n.hex()})).collect::<Vec<_>>(),
        "echo": message,
        "message": message,
    })
}

/// Whether a point may not be used: not finite, or beyond [`crate::cables::words::MAX_COORD`].
pub fn out_of_range(p: Pt) -> bool {
    !(p.is_finite() && p.x.abs() <= crate::cables::words::MAX_COORD && p.y.abs() <= crate::cables::words::MAX_COORD)
}
