//! Terminal strip accessories and links (brief 030, design 030 §2, §4, §5, §7): end stops, end
//! plates, partitions, strip labels and markers placed by the user, links (jumpers) owned by the
//! strip, and their part data. Everything is an ordinary property (FP1–FP3, FP5, FP6); this
//! module reads and checks them and derives a strip's places, links, BOM rows and checks.
//!
//! # Semantics
//! - Places (FP1): `acc.start`, `acc.end`, `acc.after.<label>` hold gap items (`end_stop`,
//!   `end_plate`, `partition`, `strip_label`), `acc.on.<label>` holds `marker`; a comma list,
//!   stored canonical (`end_plate,end_stop`), at most [`MAX_ITEMS`]. Nothing is placed by a rule.
//! - Part data (FP2, FP3, FP6): the strip's `<item>.<field>` first, then the project's
//!   `strip_<item>_<field>` ([`project_key`]), then unknown.
//! - Links (FP5): `link.<n>` ([`ecad_model::links`]); bars per link `ceil(span / bar)`, bar from
//!   `jumper.bar`, else `strip_jumper_bar`, else [`DEFAULT_BAR`].
//! - Nothing here refuses because of a check; orphans and crossings are Checks rows (§8).
//!
//! This module excludes drawing (`link_lines`) and commands (`terminal_cmds`).

use std::collections::{BTreeMap, BTreeSet};

use ecad_model::links::{self, Positions};
use ecad_model::{ComponentId, Document, ObjectRef, Op, Properties};

use crate::strips::TermRow;

/// What every placement key starts with.
pub const ACC_PREFIX: &str = "acc.";
pub const START_KEY: &str = "acc.start";
pub const END_KEY: &str = "acc.end";
pub const AFTER_PREFIX: &str = "acc.after.";
pub const ON_PREFIX: &str = "acc.on.";
/// Most items one place holds.
pub const MAX_ITEMS: usize = 8;
/// Most characters of a position label in a key.
pub const MAX_LABEL_CHARS: usize = 16;
/// The marker card size key and its range.
pub const PER_CARD_KEY: &str = "marker.per_card";
pub const MAX_PER_CARD: u32 = 100;
/// The jumper bar size key, its range and its default.
pub const BAR_KEY: &str = "jumper.bar";
pub const MIN_BAR: u32 = 2;
pub const MAX_BAR: u32 = 50;
pub const DEFAULT_BAR: usize = 10;
/// The items with part data (FP2, FP6).
pub const PART_ITEMS: [&str; 7] = ["end_stop", "end_plate", "end_plate_pe", "partition", "marker", "strip_label", "jumper"];
/// A part's fields.
pub const FIELDS: [&str; 3] = ["part_number", "maker", "description"];

/// An accessory placed on a strip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Acc {
    EndStop,
    EndPlate,
    Partition,
    StripLabel,
    Marker,
}

impl Acc {
    pub const GAP: [Acc; 4] = [Acc::EndStop, Acc::EndPlate, Acc::Partition, Acc::StripLabel];
    pub const ALL: [Acc; 5] = [Acc::EndStop, Acc::EndPlate, Acc::Partition, Acc::StripLabel, Acc::Marker];

    /// As stored and in JSON: `end_stop`.
    pub fn key(self) -> &'static str {
        match self {
            Acc::EndStop => "end_stop",
            Acc::EndPlate => "end_plate",
            Acc::Partition => "partition",
            Acc::StripLabel => "strip_label",
            Acc::Marker => "marker",
        }
    }

    /// In words: `end stop`.
    pub fn words(self) -> &'static str {
        match self {
            Acc::EndStop => "end stop",
            Acc::EndPlate => "end plate",
            Acc::Partition => "partition",
            Acc::StripLabel => "strip label",
            Acc::Marker => "marker",
        }
    }

    /// The BOM description when none is given (§4).
    pub fn description(self) -> &'static str {
        match self {
            Acc::EndStop => "End stop",
            Acc::EndPlate => "End plate",
            Acc::Partition => "Partition plate",
            Acc::StripLabel => "Strip label",
            Acc::Marker => "Marker",
        }
    }

    /// The mark in a plan's Terminal cell (design 030 §7.3, as text).
    pub fn mark(self) -> &'static str {
        match self {
            Acc::EndStop => "\u{2016}",
            Acc::EndPlate => "\u{2502}",
            Acc::Partition => "\u{2501}",
            Acc::StripLabel => "\u{25a1}",
            Acc::Marker => "\u{25ad}",
        }
    }

    /// Read one word (case-insensitive, spaces and `-` as `_`).
    pub fn parse(word: &str) -> Option<Acc> {
        let w: String = word.trim().to_ascii_lowercase().chars().map(|c| if c == ' ' || c == '-' { '_' } else { c }).collect();
        Acc::ALL.into_iter().find(|a| a.key() == w)
    }
}

/// Where a placement key puts its items.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Spot {
    Start,
    End,
    After(String),
    On(String),
}

impl Spot {
    /// Its key: `acc.after.4`.
    pub fn key(&self) -> String {
        match self {
            Spot::Start => START_KEY.into(),
            Spot::End => END_KEY.into(),
            Spot::After(l) => format!("{AFTER_PREFIX}{l}"),
            Spot::On(l) => format!("{ON_PREFIX}{l}"),
        }
    }

    /// The `gap` value in JSON: `start`, `end` or the label.
    pub fn gap(&self) -> String {
        match self {
            Spot::Start => "start".into(),
            Spot::End => "end".into(),
            Spot::After(l) | Spot::On(l) => l.clone(),
        }
    }

    pub fn label(&self) -> Option<&str> {
        match self {
            Spot::After(l) | Spot::On(l) => Some(l),
            _ => None,
        }
    }

    pub fn is_on(&self) -> bool {
        matches!(self, Spot::On(_))
    }
}

/// The place a key names, when it is a placement key.
pub fn spot_of(key: &str) -> Option<Spot> {
    match key {
        START_KEY => Some(Spot::Start),
        END_KEY => Some(Spot::End),
        _ => {
            if let Some(l) = key.strip_prefix(AFTER_PREFIX) {
                return (!l.is_empty()).then(|| Spot::After(l.to_string()));
            }
            key.strip_prefix(ON_PREFIX).filter(|l| !l.is_empty()).map(|l| Spot::On(l.to_string()))
        }
    }
}

fn shown(text: &str) -> String {
    let s: String = text.chars().take(24).collect();
    if text.chars().count() > 24 { format!("{s}\u{2026}") } else { s }
}

/// Check a position label used in a key or an `at`.
///
/// # Errors
/// Empty, longer than [`MAX_LABEL_CHARS`], or with spaces, control characters, `,` or `-`.
pub fn check_label(l: &str) -> Result<(), String> {
    if l.is_empty() || l.chars().count() > MAX_LABEL_CHARS || l.chars().any(|c| c.is_whitespace() || c.is_control() || c == ',') {
        return Err(format!("\"{}\" is not a position label. A label has 1 to {MAX_LABEL_CHARS} characters, no spaces or commas.", shown(l)));
    }
    Ok(())
}

/// Read the items of a place (`on`: a position's, else a gap's).
///
/// # Errors
/// More than [`MAX_ITEMS`], or a word not allowed at that place, in words with the way forward.
pub fn parse_items(text: &str, on: bool) -> Result<Vec<Acc>, String> {
    if text.chars().count() > MAX_ITEMS * 16 {
        return Err(format!("A place holds at most {MAX_ITEMS} accessories."));
    }
    let mut out = Vec::new();
    for w in text.split(',').map(str::trim).filter(|w| !w.is_empty()) {
        if out.len() >= MAX_ITEMS {
            return Err(format!("A place holds at most {MAX_ITEMS} accessories."));
        }
        let a = Acc::parse(w);
        match (a, on) {
            (Some(Acc::Marker), true) => out.push(Acc::Marker),
            (Some(Acc::Marker), false) => {
                return Err("A marker goes on a position, not in a gap. Write end_stop, end_plate, partition or strip_label.".into());
            }
            (Some(a), true) => return Err(format!("{} goes in a gap, not on a position. Only marker goes on a position.", a.description())),
            (Some(a), false) => out.push(a),
            (None, false) => return Err(format!("\"{}\" is not an accessory. Write end_stop, end_plate, partition or strip_label.", shown(w))),
            (None, true) => return Err(format!("\"{}\" is not an accessory. Only marker goes on a position.", shown(w))),
        }
    }
    Ok(out)
}

/// Items as stored: `end_plate,end_stop`.
pub fn items_text(items: &[Acc]) -> String {
    items.iter().map(|a| a.key()).collect::<Vec<_>>().join(",")
}

/// A placement value as stored (canonical); empty clears it.
///
/// # Errors
/// [`parse_items`]'s.
pub fn acc_value(spot: &Spot, value: Option<&str>) -> Result<Option<String>, String> {
    let Some(v) = value.map(str::trim).filter(|v| !v.is_empty()) else { return Ok(None) };
    let items = parse_items(v, spot.is_on())?;
    Ok((!items.is_empty()).then(|| items_text(&items)))
}

/// The project key standing for a strip key (FP3): `end_stop.part_number` →
/// `strip_end_stop_part_number`.
pub fn project_key(strip_key: &str) -> String {
    format!("strip_{}", strip_key.replace('.', "_"))
}

/// Every strip part key (FP2, FP6), in table order.
pub fn part_keys() -> Vec<String> {
    let mut v: Vec<String> = PART_ITEMS.iter().flat_map(|i| FIELDS.iter().map(move |f| format!("{i}.{f}"))).collect();
    v.push(PER_CARD_KEY.into());
    v.push(BAR_KEY.into());
    v
}

/// Whether `key` is a strip part key.
pub fn is_part_key(key: &str) -> bool {
    if key == PER_CARD_KEY || key == BAR_KEY {
        return true;
    }
    key.split_once('.').is_some_and(|(i, f)| PART_ITEMS.contains(&i) && FIELDS.contains(&f))
}

/// `link.<n>.<field>`: the link and the field.
pub fn link_part_key(key: &str) -> Option<(u32, &str)> {
    let rest = key.strip_prefix(links::PREFIX)?;
    let (n, f) = rest.split_once('.')?;
    let n = links::link_number(&links::key(n.parse().ok()?)).filter(|x| x.to_string() == n)?;
    FIELDS.contains(&f).then_some((n, f))
}

/// The strip key a project key stands for (`strip_jumper_bar` → `jumper.bar`).
pub fn strip_key_of_project(key: &str) -> Option<String> {
    part_keys().into_iter().find(|k| project_key(k) == key)
}

fn whole(key: &str, value: &str, lo: u32, hi: u32, what: &str) -> Result<String, String> {
    match value.trim().parse::<u32>() {
        Ok(n) if (lo..=hi).contains(&n) => Ok(n.to_string()),
        _ => Err(format!("\"{}\" is not {what}. Write {lo} to {hi}. ({key})", shown(value))),
    }
}

/// A part key's value as stored (strip or project key); empty clears it.
///
/// # Errors
/// A card size outside 1…100, a bar size outside 2…50, or text that is too long.
pub fn part_value(doc: &Document, strip_key: &str, value: Option<&str>) -> Result<Option<String>, String> {
    let Some(v) = value.map(str::trim).filter(|v| !v.is_empty()) else { return Ok(None) };
    match strip_key {
        PER_CARD_KEY => whole(strip_key, v, 1, MAX_PER_CARD, "a card size").map(Some),
        BAR_KEY => whole(strip_key, v, MIN_BAR, MAX_BAR, "a bar size").map(Some),
        _ => crate::devices::property_value(doc, "part_number", Some(v)),
    }
}

/// The rule for a project property `strip_*` (FP3, FP6): `None` when `key` is not one.
pub fn project_rule(doc: &Document, key: &str, value: Option<&str>) -> Option<Result<Option<String>, String>> {
    let sk = strip_key_of_project(key)?;
    Some(part_value(doc, &sk, value))
}

/// Every position label of strip `c` in plan order: drawn labels and the offer's positions.
pub fn positions(doc: &Document, c: ComponentId) -> Vec<String> {
    let mut labels: BTreeSet<String> =
        crate::strips::drawn_terminals(doc, c).into_iter().map(|(t, _, _)| crate::interact::terminal_name(doc, t)).collect();
    if let Some(o) = crate::strips::offer_of(doc, c) {
        labels.extend((1..=o.positions).map(|n| n.to_string()));
    }
    let mut v: Vec<String> = labels.into_iter().collect();
    v.sort_by_key(|l| links::natural_key(l));
    v
}

/// A link value as stored: canonical (`1, 2, 3, 4` → `1-4`) when every label it names is a
/// position of the strip, else as written; empty clears it.
///
/// # Errors
/// Unreadable text, or fewer than 2 positions.
pub fn link_value(doc: &Document, c: ComponentId, value: Option<&str>) -> Result<Option<String>, String> {
    let Some(v) = value.map(str::trim).filter(|v| !v.is_empty()) else { return Ok(None) };
    let p = links::parse(v)?;
    let order = positions(doc, c);
    Ok(Some(link_text(&p, &order)?))
}

/// [`link_value`] on parsed positions and a plan order.
///
/// # Errors
/// Fewer than 2 positions.
pub fn link_text(p: &Positions, order: &[String]) -> Result<String, String> {
    let sel = p.select(order);
    let missing = p.missing(order);
    if sel.len() + missing.len() < 2 {
        return Err("A link joins at least 2 positions. Write positions like 1-4 or 7, 9, 11.".into());
    }
    Ok(if missing.is_empty() { links::canonical(&sel, order) } else { p.text() })
}

/// The `setprop` rule for strip keys (FP1, FP2, FP5, FP6): `Ok(None)` when `key` is not one of
/// them (or a part key on another device, which stays free text), else the value to store.
///
/// # Errors
/// A placement or link key on a device that is not a strip, and every refusal of the value.
pub fn setprop_rule(doc: &Document, c: ComponentId, key: &str, value: Option<&str>) -> Result<Option<Option<String>>, String> {
    let strip = crate::strips::is_strip(doc, c);
    let not_strip = || format!("{} is not a terminal strip. Pick a terminal of the strip.", crate::devices::tag_words(doc, c));
    if key.starts_with(ACC_PREFIX) {
        if !strip {
            return Err(not_strip());
        }
        let spot = spot_of(key)
            .ok_or_else(|| format!("\"{}\" is not a place. Write acc.start, acc.end, acc.after.<label> or acc.on.<label>.", shown(key)))?;
        if let Some(l) = spot.label() {
            check_label(l)?;
        }
        return acc_value(&spot, value).map(Some);
    }
    if links::link_number(key).is_some() {
        if !strip {
            return Err(not_strip());
        }
        return link_value(doc, c, value).map(Some);
    }
    if key.starts_with(links::PREFIX) {
        if !strip {
            return Err(not_strip());
        }
        if link_part_key(key).is_none() {
            return Err(format!("\"{}\" is not a link key. Write link.<n> (1 to 999) or link.<n>.part_number.", shown(key)));
        }
        return crate::devices::property_value(doc, "part_number", value).map(Some);
    }
    if strip && is_part_key(key) {
        return part_value(doc, key, value).map(Some);
    }
    Ok(None)
}

// ---------------------------------------------------------------- derived (§7, §11)

/// One link of a strip, resolved against its positions.
#[derive(Debug, Clone, PartialEq)]
pub struct LinkInfo {
    pub n: u32,
    /// As shown: `1-4`, `7, 9, 11`.
    pub text: String,
    /// The strip's positions it names, in plan order.
    pub labels: Vec<String>,
    /// Plan indexes of its first and last position (positions only, not gap rows).
    pub first: usize,
    pub last: usize,
    pub span: usize,
    pub lane: usize,
    pub bars: usize,
    pub part_number: Option<String>,
    pub maker: Option<String>,
    pub description: Option<String>,
    /// Labels it names that the strip does not have (`link-orphan`).
    pub missing: Vec<String>,
    /// Partitions inside its span, by the label they follow (`link-across-partition`).
    pub crosses: Vec<String>,
}

/// One accessory BOM row of a strip.
#[derive(Debug, Clone, PartialEq)]
pub struct AccBom {
    /// `end_stop`, `end_plate`, `partition`, `strip_label`, `marker`, `jumper`.
    pub accessory: &'static str,
    pub description: String,
    pub part_number: Option<String>,
    pub maker: Option<String>,
    pub qty: usize,
}

/// A strip's accessories and links (design 030 §7, §11.1).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StripAcc {
    pub start: Vec<Acc>,
    pub end: Vec<Acc>,
    /// Gap items after positions the strip has, by label.
    pub after: BTreeMap<String, Vec<Acc>>,
    /// Positions with a marker.
    pub markers: BTreeSet<String>,
    /// Keys whose label names no position: (key, items).
    pub orphans: Vec<(String, Vec<Acc>)>,
    pub links: Vec<LinkInfo>,
    pub bom: Vec<AccBom>,
    /// Placed items with no part number (`no-accessory-part`).
    pub no_part: usize,
    /// Project keys this strip reads because it has no value of its own (the `inherits` index).
    pub inherits: BTreeSet<String>,
    /// Bar size in use.
    pub bar: usize,
    /// The part number each placed gap item shows (`end_plate_pe` when an end plate follows a PE
    /// position and that part is known).
    pub parts: BTreeMap<&'static str, String>,
}

impl StripAcc {
    /// The part number of a gap row: `acc` in a gap, `pe_before` when the position before it is
    /// PE.
    pub fn gap_part(&self, acc: Acc, pe_before: bool) -> Option<&String> {
        if acc == Acc::EndPlate
            && pe_before
            && let Some(p) = self.parts.get("end_plate_pe")
        {
            return Some(p);
        }
        self.parts.get(acc.key())
    }
}

impl StripAcc {
    /// How many gap rows the plan shows.
    pub fn gap_rows(&self) -> usize {
        self.start.len() + self.end.len() + self.after.values().map(Vec::len).sum::<usize>()
    }

    /// How many of `a` are placed (markers: positions with one).
    pub fn count(&self, a: Acc) -> usize {
        if a == Acc::Marker {
            return self.markers.len();
        }
        let n = |v: &Vec<Acc>| v.iter().filter(|x| **x == a).count();
        n(&self.start) + n(&self.end) + self.after.values().map(n).sum::<usize>()
    }

    /// Its Checks warning rows: orphans, link orphans, crossings.
    pub fn warnings(&self) -> usize {
        self.orphans.len() + self.links.iter().map(|l| usize::from(!l.missing.is_empty()) + l.crosses.len()).sum::<usize>()
    }

    /// The bars of every link.
    pub fn bars(&self) -> usize {
        self.links.iter().map(|l| l.bars).sum()
    }

    /// `2 end stops, 1 end plate, 1 partition, 12 markers, 2 links (3 bars)`; empty with nothing.
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        for a in Acc::ALL {
            let n = self.count(a);
            if n > 0 {
                parts.push(format!("{n} {}{}", a.words(), if n == 1 { "" } else { "s" }));
            }
        }
        if !self.links.is_empty() {
            let n = self.links.len();
            let b = self.bars();
            parts.push(format!("{n} link{} ({b} bar{})", if n == 1 { "" } else { "s" }, if b == 1 { "" } else { "s" }));
        }
        parts.join(", ")
    }
}

/// Part data lookup: the strip's own value, else the project's (recorded in `inherits`).
struct Parts<'a> {
    strip: &'a Properties,
    project: &'a Properties,
    inherits: BTreeSet<String>,
}

impl Parts<'_> {
    fn get(&mut self, key: &str) -> Option<String> {
        if let Some(v) = self.strip.get(key).map(|v| v.trim()).filter(|v| !v.is_empty()) {
            return Some(v.to_string());
        }
        let pk = project_key(key);
        self.inherits.insert(pk.clone());
        self.project.get(&pk).map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
    }

    fn part(&mut self, item: &str) -> (Option<String>, Option<String>, Option<String>) {
        (self.get(&format!("{item}.part_number")), self.get(&format!("{item}.maker")), self.get(&format!("{item}.description")))
    }
}

/// A strip's accessories on `doc`, from its properties and its plan rows (label order).
pub fn strip_acc(doc: &Document, props: &Properties, rows: &[TermRow]) -> StripAcc {
    let mut a = StripAcc::default();
    let order: Vec<String> = rows.iter().map(|r| r.label.clone()).collect();
    let index: BTreeMap<&str, usize> = order.iter().enumerate().map(|(i, l)| (l.as_str(), i)).collect();
    // Places.
    for (k, v) in props.range(ACC_PREFIX.to_string()..).take_while(|(k, _)| k.starts_with(ACC_PREFIX)) {
        let Some(spot) = spot_of(k) else { continue };
        let Ok(items) = parse_items(v, spot.is_on()) else { continue };
        if items.is_empty() {
            continue;
        }
        match &spot {
            Spot::Start => a.start = items,
            Spot::End => a.end = items,
            Spot::After(l) if index.contains_key(l.as_str()) => {
                a.after.insert(l.clone(), items);
            }
            Spot::On(l) if index.contains_key(l.as_str()) => {
                a.markers.insert(l.clone());
            }
            _ => a.orphans.push((k.clone(), items)),
        }
    }
    let mut parts = Parts { strip: props, project: &doc.properties, inherits: BTreeSet::new() };
    for g in Acc::GAP {
        if a.count(g) > 0 {
            if let Some(pn) = parts.get(&format!("{}.part_number", g.key())) {
                a.parts.insert(g.key(), pn);
            }
            if g == Acc::EndPlate
                && let Some(pn) = parts.get("end_plate_pe.part_number")
            {
                a.parts.insert("end_plate_pe", pn);
            }
        }
    }
    // Links, lanes and bars.
    a.bar =
        parts.get(BAR_KEY).and_then(|v| v.parse::<usize>().ok()).filter(|n| (MIN_BAR as usize..=MAX_BAR as usize).contains(n)).unwrap_or(DEFAULT_BAR);
    let partitions: Vec<usize> =
        a.after.iter().filter(|(_, v)| v.contains(&Acc::Partition)).filter_map(|(l, _)| index.get(l.as_str()).copied()).collect();
    let mut lanes: Vec<usize> = Vec::new();
    let mut found: Vec<(u32, Positions, String)> = Vec::new();
    for (k, v) in props.range(links::PREFIX.to_string()..).take_while(|(k, _)| k.starts_with(links::PREFIX)) {
        let Some(n) = links::link_number(k) else { continue };
        if let Ok(p) = links::parse(v) {
            found.push((n, p, v.clone()));
        }
    }
    let mut infos: Vec<LinkInfo> = Vec::new();
    let keys: Vec<Vec<(u8, u128, String)>> = if found.is_empty() { Vec::new() } else { order.iter().map(|l| links::natural_key(l)).collect() };
    for (n, p, text) in found {
        let labels: Vec<String> = p.select_keyed(&order, &keys).into_iter().cloned().collect();
        let missing = p.missing(&order);
        let idx: Vec<usize> = labels.iter().filter_map(|l| index.get(l.as_str()).copied()).collect();
        let (first, last) = (idx.first().copied().unwrap_or(0), idx.last().copied().unwrap_or(0));
        let span = if idx.is_empty() { 0 } else { last - first + 1 };
        let crosses = partitions.iter().filter(|i| **i >= first && **i < last).filter_map(|i| order.get(*i).cloned()).collect();
        let own = |f: &str| props.get(&format!("{}.{f}", links::key(n))).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        let (pn, mk, ds) = (own("part_number"), own("maker"), own("description"));
        let (part_number, maker, description) = match pn {
            Some(pn) => (Some(pn), mk, ds),
            None => parts.part("jumper"),
        };
        infos.push(LinkInfo {
            n,
            text: text.trim().to_string(),
            labels,
            first,
            last,
            span,
            lane: 0,
            bars: span.div_ceil(a.bar),
            part_number,
            maker,
            description,
            missing,
            crosses,
        });
    }
    // Lanes: greedy by first position, then link number (stable).
    let mut by_first: Vec<usize> = (0..infos.len()).collect();
    by_first.sort_by_key(|i| infos.get(*i).map(|l| (l.first, l.n)));
    for i in by_first {
        let Some(l) = infos.get_mut(i) else { continue };
        if l.span == 0 {
            continue;
        }
        match lanes.iter().position(|end| *end < l.first) {
            Some(k) => {
                l.lane = k;
                if let Some(e) = lanes.get_mut(k) {
                    *e = l.last;
                }
            }
            None => {
                l.lane = lanes.len();
                lanes.push(l.last);
            }
        }
    }
    a.links = infos;
    // BOM rows: per accessory part, only what is placed (§7.4).
    let pe_at = |label: Option<&str>| label.and_then(|l| index.get(l)).and_then(|i| rows.get(*i)).is_some_and(|r| r.pe);
    let mut plates: [usize; 2] = [0, 0];
    let mut count_plates = |items: &[Acc], pe: bool| {
        let n = items.iter().filter(|x| **x == Acc::EndPlate).count();
        if let Some(slot) = plates.get_mut(usize::from(pe)) {
            *slot += n;
        }
    };
    count_plates(&a.start, false);
    for (l, v) in &a.after {
        count_plates(v, pe_at(Some(l)));
    }
    count_plates(&a.end, rows.last().is_some_and(|r| r.pe));
    let mut bom: Vec<AccBom> = Vec::new();
    let mut no_part = 0usize;
    let push = |bom: &mut Vec<AccBom>,
                no_part: &mut usize,
                accessory: &'static str,
                qty: usize,
                part: (Option<String>, Option<String>, Option<String>),
                words: &str| {
        if qty == 0 {
            return;
        }
        if part.0.is_none() {
            *no_part += qty;
        }
        bom.push(AccBom { accessory, description: part.2.unwrap_or_else(|| words.to_string()), part_number: part.0, maker: part.1, qty });
    };
    for acc in [Acc::EndStop, Acc::EndPlate, Acc::Partition, Acc::StripLabel] {
        if acc == Acc::EndPlate {
            if plates.iter().sum::<usize>() == 0 {
                continue;
            }
            let (plain, pe) = (plates.first().copied().unwrap_or(0), plates.get(1).copied().unwrap_or(0));
            let pe_part = parts.part("end_plate_pe");
            let (plain, pe) = if pe_part.0.is_some() { (plain, pe) } else { (plain + pe, 0) };
            let p = parts.part("end_plate");
            push(&mut bom, &mut no_part, "end_plate", plain, p, "End plate");
            push(&mut bom, &mut no_part, "end_plate", pe, pe_part, "End plate (PE)");
            continue;
        }
        let n = a.count(acc);
        if n > 0 {
            let p = parts.part(acc.key());
            push(&mut bom, &mut no_part, acc.key(), n, p, acc.description());
        }
    }
    let markers = a.markers.len();
    if markers > 0 {
        let p = parts.part("marker");
        match p.0 {
            None => no_part += markers,
            Some(_) => {
                let per = parts.get(PER_CARD_KEY).and_then(|v| v.parse::<usize>().ok()).filter(|n| (1..=MAX_PER_CARD as usize).contains(n));
                match per {
                    Some(per) => {
                        let words = format!("Marker card \u{d7}{per}");
                        push(&mut bom, &mut no_part, "marker", markers.div_ceil(per), p, &words);
                    }
                    None => push(&mut bom, &mut no_part, "marker", markers, p, "Marker"),
                }
            }
        }
    }
    // Jumper bars, summed per part (§5.6).
    let mut jumpers: Vec<AccBom> = Vec::new();
    for l in &a.links {
        if l.bars == 0 {
            continue;
        }
        if l.part_number.is_none() {
            no_part += l.bars;
        }
        match jumpers.iter_mut().find(|j| j.part_number == l.part_number && j.maker == l.maker) {
            Some(j) => j.qty += l.bars,
            None => jumpers.push(AccBom {
                accessory: "jumper",
                description: l.description.clone().unwrap_or_else(|| format!("Jumper bar \u{d7}{}", a.bar)),
                part_number: l.part_number.clone(),
                maker: l.maker.clone(),
                qty: l.bars,
            }),
        }
    }
    bom.extend(jumpers);
    a.bom = bom;
    a.no_part = no_part;
    a.inherits = parts.inherits;
    a
}

// ---------------------------------------------------------------- renumbering (§6, FP4, FP7)

/// The ops that keep a strip's accessories and links on their terminals when `ops` (a renumber
/// plan: `RemoveTerminal` + `AddTerminal` of one ID) relabel them, and the preview lines
/// (`-X01: partition after 4 → after 6`, `-X01: link 1-4 → 3-6`). Moves are worked out on the end
/// state, so `4 ↔ 5` swaps cleanly; keys on labels no terminal had stay.
pub fn renumber_ops(doc: &Document, ops: &[Op]) -> (Vec<Op>, Vec<String>) {
    let mut removed: BTreeMap<ecad_model::TerminalId, (ComponentId, Option<String>)> = BTreeMap::new();
    let mut maps: BTreeMap<ComponentId, BTreeMap<String, String>> = BTreeMap::new();
    for op in ops {
        match op {
            Op::RemoveTerminal { id, terminal } => {
                removed.insert(*id, (terminal.component, terminal.label.clone()));
            }
            Op::AddTerminal { id, terminal } => {
                if let Some((c, Some(old))) = removed.get(id)
                    && *c == terminal.component
                    && let Some(new) = &terminal.label
                    && old != new
                {
                    maps.entry(*c).or_default().insert(old.clone(), new.clone());
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    let mut lines = Vec::new();
    for (c, map) in maps {
        if !crate::strips::is_strip(doc, c) {
            continue;
        }
        let Some(props) = doc.components.get(&c).map(|x| &x.properties) else { continue };
        let tag = crate::devices::tag_words(doc, c);
        let moved = |l: &str| map.get(l).cloned().unwrap_or_else(|| l.to_string());
        // Placement keys: the end state, then the difference.
        let mut now: BTreeMap<String, String> = BTreeMap::new();
        let mut was: BTreeMap<String, String> = BTreeMap::new();
        for (k, v) in props.range(ACC_PREFIX.to_string()..).take_while(|(k, _)| k.starts_with(ACC_PREFIX)) {
            was.insert(k.clone(), v.clone());
            let nk = match spot_of(k) {
                Some(Spot::After(l)) => Spot::After(moved(&l)).key(),
                Some(Spot::On(l)) => Spot::On(moved(&l)).key(),
                _ => k.clone(),
            };
            if nk != *k {
                let how = if k.starts_with(ON_PREFIX) { "on" } else { "after" };
                let (a, b) = (spot_of(k).and_then(|x| x.label().map(str::to_string)), spot_of(&nk).and_then(|x| x.label().map(str::to_string)));
                lines.push(format!(
                    "{tag}: {} {how} {} \u{2192} {how} {}",
                    v.replace(',', ", ").replace('_', " "),
                    a.unwrap_or_default(),
                    b.unwrap_or_default()
                ));
            }
            now.insert(nk, v.clone());
        }
        for k in was.keys().chain(now.keys()).collect::<BTreeSet<_>>() {
            let (b, a) = (was.get(k).cloned(), now.get(k).cloned());
            if b != a {
                out.push(Op::SetProperty { object: ObjectRef::Component(c), key: k.clone(), before: b, after: a });
            }
        }
        // Links: labels rewritten, re-collapsed in the new plan order.
        let old_order = positions(doc, c);
        let mut new_order: Vec<String> = old_order.iter().map(|l| moved(l)).collect();
        new_order.sort_by_key(|l| links::natural_key(l));
        new_order.dedup();
        for (n, p) in links::of(props) {
            let key = links::key(n);
            let Some(before) = props.get(&key).cloned() else { continue };
            let mut labels: Vec<String> = p.select(&old_order).iter().map(|l| moved(l)).collect();
            labels.extend(p.missing(&old_order));
            let mut refs: Vec<&String> = labels.iter().collect();
            refs.sort_by_key(|l| (new_order.iter().position(|o| o == *l).unwrap_or(usize::MAX), links::natural_key(l)));
            refs.dedup();
            let after = links::canonical(&refs, &new_order);
            if after != before {
                lines.push(format!("{tag}: link {before} \u{2192} {after}"));
                out.push(Op::SetProperty { object: ObjectRef::Component(c), key, before: Some(before), after: Some(after) });
            }
        }
    }
    (out, lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_and_values() {
        assert_eq!(spot_of("acc.after.4"), Some(Spot::After("4".into())));
        assert_eq!(spot_of("acc.on.PE1"), Some(Spot::On("PE1".into())));
        assert_eq!(spot_of("acc.middle"), None);
        assert_eq!(acc_value(&Spot::End, Some(" End plate , end-stop ")), Ok(Some("end_plate,end_stop".into())));
        assert_eq!(acc_value(&Spot::Start, Some(" ")), Ok(None));
        assert!(acc_value(&Spot::Start, Some("plate")).unwrap_err().contains("Write end_stop, end_plate, partition or strip_label"));
        assert!(acc_value(&Spot::On("4".into()), Some("partition")).unwrap_err().contains("Only marker goes on a position"));
        assert!(acc_value(&Spot::Start, Some("marker")).is_err());
        assert!(acc_value(&Spot::Start, Some(&"end_stop,".repeat(9))).is_err());
        assert!(acc_value(&Spot::Start, Some(&"end_stop,".repeat(1000))).is_err());
        assert_eq!(project_key("end_stop.part_number"), "strip_end_stop_part_number");
        assert_eq!(strip_key_of_project("strip_jumper_bar").as_deref(), Some("jumper.bar"));
        assert_eq!(strip_key_of_project("strip_marker_per_card").as_deref(), Some("marker.per_card"));
        assert!(part_keys().iter().all(|k| project_key(k).len() <= 40));
        assert_eq!(link_part_key("link.2.part_number"), Some((2, "part_number")));
        assert_eq!(link_part_key("link.02.part_number"), None);
        assert_eq!(link_part_key("link.2.colour"), None);
        assert!(check_label(&"x".repeat(17)).is_err());
    }
}
