//! The rules of the commands: `accessory` (end stops,
//! end plates, partitions, strip labels and markers at places of a strip) and `jumper` (make a jumper bar,
//! or edit one). Both write ordinary strip properties. Each function reads the document and returns
//! the ops to commit and what to report; the engine crate's shims read the parameters, commit and
//! answer in JSON.

use std::collections::BTreeSet;

use ecad_model::connectivity::jumper_bars::{self, Positions};
use ecad_model::{ConnectionPointId, DeviceId, Document, Noun, ObjectRef, Op};

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::commands::input::{Bounded, nullable};
use crate::commands::run::{Cx, Generic, Outcome};
use crate::error::CmdError;
use crate::terminal_strips::accessories::{self as acc, Acc, Spot};
use crate::views::Views;

/// Most places one `accessory` call writes.
pub(crate) const MAX_AT: usize = 999;
/// Most terminals one `jumper` call picks.
pub(crate) const MAX_PICKS: usize = 999;
/// The refusal when a strip has no free jumper bar number.
const JUMPER_BARS_FULL: &str = "This strip has 999 jumper bars; delete one first.";

/// Words given as one text or as a list.
#[derive(Debug, Clone, PartialEq)]
pub enum Words {
    /// A list.
    List(Vec<String>),
    /// One text.
    Text(String),
}

impl Words {
    /// The words as one text, a list joined with commas.
    pub fn text(&self) -> String {
        match self {
            Words::List(v) => v.join(","),
            Words::Text(t) => t.clone(),
        }
    }
}

fn strip_words(doc: &Document, c: DeviceId) -> String {
    crate::devices::tags::tag_words(doc, c)
}

/// The strip a call names (`strip`, or `connection_point`'s), and the terminal's label.
fn strip_of(doc: &Document, strip: Option<DeviceId>, connection_point: Option<ConnectionPointId>) -> Result<(DeviceId, Option<String>), CmdError> {
    let (c, label) = match (strip, connection_point) {
        (Some(c), _) => (c, None),
        (None, Some(t)) => {
            let term = doc.connection_points().get(&t).ok_or(CmdError::NotFound(Noun::TerminalBlock))?;
            (term.device().ok_or(CmdError::NotFound(Noun::TerminalBlock))?, Some(crate::terminal_strips::terminal_blocks::position_label(doc, t)))
        }
        (None, None) => return Err(CmdError::input("`strip` or `connection_point` (hex id) is required")),
    };
    if !doc.devices().contains_key(&c) {
        return Err(CmdError::NotFound(Noun::Strip));
    }
    if crate::terminal_strips::strip_of(doc, c).is_none() {
        return Err(CmdError::refused(format!("{} is not a terminal strip. Pick a terminal of the strip.", strip_words(doc, c))));
    }
    Ok((c, label))
}

/// `-X01 has no position 15; it has 1…12. Pick one of those, or start/end.`
fn no_position(doc: &Document, c: DeviceId, label: &str, order: &[String]) -> CmdError {
    let has = match (order.first(), order.last()) {
        (Some(a), Some(b)) if a != b => format!("{a}\u{2026}{b}"),
        (Some(a), _) => a.clone(),
        _ => "no positions".into(),
    };
    let shown: String = label.chars().take(16).collect();
    CmdError::refused(format!("{} has no position {shown}; it has {has}. Pick one of those, or start/end.", strip_words(doc, c)))
}

/// An item list as given: `None` absent, `Some(None)` null (none), else the words.
fn item_list(given: Option<&Option<Words>>, on: bool) -> Result<Option<Vec<Acc>>, CmdError> {
    match given {
        None => Ok(None),
        Some(None) => Ok(Some(Vec::new())),
        Some(Some(Words::List(a))) => {
            if a.len() > acc::MAX_ITEMS {
                return Err(CmdError::refused(format!("A place holds at most {} accessories.", acc::MAX_ITEMS)));
            }
            acc::parse_items(&a.join(","), on).map(Some)
        }
        Some(Some(Words::Text(t))) => acc::parse_items(t, on).map(Some),
    }
}

fn read_items(doc: &Document, c: DeviceId, key: &str, on: bool) -> Vec<Acc> {
    doc.devices().get(&c).and_then(|x| x.properties.get(key)).and_then(|v| acc::parse_items(v, on).ok()).unwrap_or_default()
}

fn plural(a: Acc, n: usize) -> String {
    let w = a.words();
    let w = if n == 1 { w.to_string() } else { format!("{w}s") };
    let mut ch = w.chars();
    match ch.next() {
        Some(f) => f.to_uppercase().chain(ch).collect(),
        None => w,
    }
}

/// What `accessory` is asked to write.
#[derive(Debug, Clone, Default)]
pub(crate) struct AccessoryInput {
    pub strip: Option<DeviceId>,
    pub connection_point: Option<ConnectionPointId>,
    /// `start`, `end`, `all`, a position label, or a list of them (absent: `connection_point`'s position).
    pub at: Option<Words>,
    /// Markers on positions.
    pub on: bool,
    /// Replace the items (`Some(None)`: none).
    pub items: Option<Option<Words>>,
    /// Add these items (`Some(None)`: none).
    pub add: Option<Option<Words>>,
    /// Remove these items (`Some(None)`: none).
    pub remove: Option<Option<Words>>,
    /// Placement keys emptied in the same transaction.
    pub clear: Option<Vec<String>>,
}

/// A planned `accessory` call.
#[derive(Debug, Clone)]
pub struct Accessory {
    pub strip: DeviceId,
    /// Per place: its key and its items after the call.
    pub keys: Vec<(String, Vec<Acc>)>,
    pub ops: Vec<Op>,
    /// The transaction's label.
    pub label: String,
    pub message: String,
}

/// Plan `accessory`: one `SetProperty` per place, all in one transaction.
///
/// # Errors
/// No such strip or terminal, a place or item that does not read, too many items at a place.
pub(crate) fn plan_accessory(doc: &Document, input: AccessoryInput) -> Result<Accessory, CmdError> {
    let on = input.on;
    let (c, label) = strip_of(doc, input.strip, input.connection_point)?;
    let at: Vec<String> = match input.at {
        Some(Words::Text(t)) => vec![t.trim().to_string()],
        Some(Words::List(a)) => {
            if a.len() > MAX_AT {
                return Err(CmdError::refused(format!("`at` names at most {MAX_AT} places.")));
            }
            a.iter().map(|t| t.trim().to_string()).collect()
        }
        None => vec![label.ok_or(CmdError::input("`at` is required: \"start\", \"end\", a position label or a list of them"))?],
    };
    if at.is_empty() {
        return Err(CmdError::refused("`at` names no place."));
    }
    let (items, add, remove) = (item_list(input.items.as_ref(), on)?, item_list(input.add.as_ref(), on)?, item_list(input.remove.as_ref(), on)?);
    if items.is_none() && add.is_none() && remove.is_none() {
        return Err(CmdError::input("Give `items` (replace), `add` or `remove`."));
    }
    let order = acc::positions(doc, c);
    // `at: "all"` names every position of the strip (the plan's Marker on every position).
    let at: Vec<String> = if at.len() == 1 && at.first().is_some_and(|a| a.eq_ignore_ascii_case("all")) { order.clone() } else { at };
    let mut spots: Vec<Spot> = Vec::new();
    let mut seen = BTreeSet::new();
    for a in &at {
        let spot = match (a.to_ascii_lowercase().as_str(), on) {
            ("start" | "end", true) => return Err(CmdError::refused("Markers go on a position. Pick a terminal, not start or end.")),
            ("start", false) => Spot::Start,
            ("end", false) => Spot::End,
            _ => {
                acc::check_label(a)?;
                if !order.iter().any(|o| o == a) {
                    return Err(no_position(doc, c, a, &order));
                }
                if on { Spot::On(a.clone()) } else { Spot::After(a.clone()) }
            }
        };
        if seen.insert(spot.key()) {
            spots.push(spot);
        }
    }
    let mut ops = Vec::new();
    let mut keys: Vec<(String, Vec<Acc>)> = Vec::new();
    for spot in &spots {
        let key = spot.key();
        let mut list = match &items {
            Some(v) => v.clone(),
            None => read_items(doc, c, &key, on),
        };
        if let Some(a) = &add {
            list.extend(a.iter().copied());
        }
        if let Some(r) = &remove {
            for x in r {
                if let Some(i) = list.iter().rposition(|y| y == x) {
                    list.remove(i);
                }
            }
        }
        if list.len() > acc::MAX_ITEMS {
            return Err(CmdError::refused(format!("A place holds at most {} accessories.", acc::MAX_ITEMS)));
        }
        let after = (!list.is_empty()).then(|| acc::items_text(&list));
        let before = doc.devices().get(&c).and_then(|x| x.properties.get(&key)).cloned();
        keys.push((key.clone(), list));
        if before != after {
            ops.push(Op::SetProperty { object: ObjectRef::Device(c), key, before, after });
        }
    }
    // `clear`: placement keys emptied in the same transaction (the `accessory-orphan` fix,
    // Move to end).
    for k in input.clear.iter().flatten().take(MAX_AT) {
        if !k.starts_with(acc::ACC_PREFIX) {
            return Err(CmdError::refused(format!("`clear` takes placement keys (acc.…), not \"{k}\".")));
        }
        if keys.iter().any(|(x, _)| x == k) {
            continue;
        }
        if let Some(before) = doc.devices().get(&c).and_then(|x| x.properties.get(k)).cloned() {
            ops.push(Op::SetProperty { object: ObjectRef::Device(c), key: k.clone(), before: Some(before), after: None });
        }
    }
    let tag = strip_words(doc, c);
    let place = |s: &Spot| match s {
        Spot::Start => format!("at the start of {tag}"),
        Spot::End => format!("at the end of {tag}"),
        Spot::After(l) => format!("after {tag}:{l}"),
        Spot::On(l) => format!("on {tag}:{l}"),
    };
    let where_ = match (spots.first(), spots.last()) {
        (Some(a), Some(b)) if spots.len() > 1 => match (a.label(), b.label()) {
            (Some(x), Some(y)) => format!("{} {tag}:{x}\u{2026}{y}", if on { "on" } else { "after" }),
            _ => format!("at {} places of {tag}", spots.len()),
        },
        (Some(a), _) => place(a),
        _ => tag.clone(),
    };
    let what = match (&items, &add, &remove) {
        (Some(v), _, _) if v.is_empty() => "Cleared".to_string(),
        (_, Some(v), _) | (Some(v), _, _) if !v.is_empty() => v.iter().map(|a| plural(*a, spots.len())).collect::<Vec<_>>().join(", "),
        (_, _, Some(v)) if !v.is_empty() => format!("Removed {}", v.iter().map(|a| a.words()).collect::<Vec<_>>().join(", ")),
        _ => "Accessories".to_string(),
    };
    let message = format!("{what} {where_}.");
    let label = match spots.as_slice() {
        [one] => format!(
            "Accessory {tag} {}",
            match one {
                Spot::Start => "start".to_string(),
                Spot::End => "end".to_string(),
                Spot::After(l) => format!("after {l}"),
                Spot::On(l) => format!("on {l}"),
            }
        ),
        many => format!("Accessories {tag} ({} places)", many.len()),
    };
    Ok(Accessory { strip: c, keys, ops, label, message })
}

// ---------------------------------------------------------------- jumper

/// The positions a jumper bar names in plan order, with the labels it names that the strip lacks.
fn jumper_bar_labels(p: &Positions, order: &[String]) -> Vec<String> {
    let mut v: Vec<String> = p.select(order).into_iter().cloned().collect();
    for m in p.missing(order) {
        if !v.contains(&m) {
            v.push(m);
        }
    }
    v
}

/// Canonical jumper bar text on `level` (`1.2-8.2`; on level 1 the plain labels, `1-8`).
fn canonical_of(labels: &[String], order: &[String], level: u32, point: Option<char>) -> String {
    let mut sorted: Vec<&String> = labels.iter().collect();
    sorted.sort_by_key(|l| (order.iter().position(|o| o == *l).unwrap_or(usize::MAX), jumper_bars::natural_key(l)));
    sorted.dedup();
    crate::terminal_strips::terminal_blocks::jumper_bar_text(&sorted, order, level, point)
}

/// Labels from a text (`5,6` or `5-8`).
fn labels_of(text: &str, order: &[String]) -> Result<Vec<String>, CmdError> {
    Ok(jumper_bar_labels(&jumper_bars::parse(text)?, order))
}

/// The next free jumper bar number on a strip.
fn next_jumper_bar(doc: &Document, c: DeviceId) -> Option<u32> {
    let props = &doc.devices().get(&c)?.properties;
    let high = props
        .range(jumper_bars::PREFIX.to_string()..)
        .take_while(|(k, _)| k.starts_with(jumper_bars::PREFIX))
        .filter_map(|(k, _)| {
            let rest = k.strip_prefix(jumper_bars::PREFIX)?;
            rest.split('.').next()?.parse::<u32>().ok()
        })
        .filter(|n| *n <= jumper_bars::MAX_JUMPER_BAR)
        .max()
        .unwrap_or(0);
    (high < jumper_bars::MAX_JUMPER_BAR).then_some(high + 1)
}

fn set_op(doc: &Document, c: DeviceId, key: String, after: Option<String>) -> Option<Op> {
    let before = doc.devices().get(&c).and_then(|x| x.properties.get(&key)).cloned();
    (before != after).then_some(Op::SetProperty { object: ObjectRef::Device(c), key, before, after })
}

/// The ops clearing a jumper bar and its own part keys.
fn delete_ops(doc: &Document, c: DeviceId, n: u32) -> Vec<Op> {
    let mut ops: Vec<Op> = set_op(doc, c, jumper_bars::key(n), None).into_iter().collect();
    for f in acc::FIELDS {
        ops.extend(set_op(doc, c, format!("{}.{f}", jumper_bars::key(n)), None));
    }
    ops
}

/// The positions from `a` to `b` in plan order (either way round).
fn range_between(order: &[String], a: &str, b: &str) -> Vec<String> {
    let (i, j) = (order.iter().position(|o| o == a), order.iter().position(|o| o == b));
    match (i, j) {
        (Some(i), Some(j)) => order.get(i.min(j)..=i.max(j)).unwrap_or_default().to_vec(),
        _ => [a, b].iter().map(|x| x.to_string()).collect(),
    }
}

/// The jumper bar a `jumper` edit names.
#[derive(Debug, Clone, Default)]
pub(crate) struct JumperBarRef {
    pub strip: Option<DeviceId>,
    pub n: Option<u64>,
}

/// What `jumper` is asked to do: make a jumper bar (`jumper_bar` absent), or edit one.
#[derive(Debug, Clone, Default)]
pub(crate) struct JumperInput {
    /// The jumper bar to edit.
    pub jumper_bar: Option<JumperBarRef>,
    /// Make: the strip.
    pub strip: Option<DeviceId>,
    /// Make: the picked terminals.
    pub connection_points: Vec<ConnectionPointId>,
    /// Make: no ranges between consecutive picks.
    pub skip: bool,
    /// The positions as text (`1-4, 7`).
    pub positions: Option<String>,
    /// Edit: positions to add, as text.
    pub add: Option<String>,
    /// Edit: positions to remove, as text.
    pub remove: Option<String>,
    /// Edit: split the jumper bar after this position.
    pub split_at: Option<String>,
    /// Edit: drop the positions the strip lacks.
    pub drop_missing: bool,
    /// Edit: delete the jumper bar.
    pub delete: bool,
}

/// A planned `jumper` call.
#[derive(Debug, Clone)]
pub struct Jumper {
    pub strip: DeviceId,
    /// The jumper bar's number.
    pub n: u32,
    pub ops: Vec<Op>,
    /// The transaction's label.
    pub label: String,
    pub message: String,
    /// A split's new jumper bar: its number and positions.
    pub split: Option<(u32, String)>,
}

/// Plan `jumper`: make a jumper bar, or edit one.
///
/// # Errors
/// No such strip or terminal, positions that do not read, a jumper bar the rules refuse.
pub(crate) fn plan_jumper(doc: &Document, input: JumperInput) -> Result<Jumper, CmdError> {
    // The level the jumper bar runs on, or a cross-level jumper bar's text.
    let mut level: u32 = 1;
    let mut point: Option<char> = None;
    let mut cross: Option<String> = None;
    let (c, n, before_text, labels, label, extra) = if let Some(l) = &input.jumper_bar {
        let c = l.strip.ok_or(CmdError::input("`strip` (hex id) is required"))?;
        let (c, _) = strip_of(doc, Some(c), None)?;
        let n = l.n.and_then(|n| u32::try_from(n).ok()).filter(|n| (1..=jumper_bars::MAX_JUMPER_BAR).contains(n));
        let n = n.ok_or(CmdError::input("`jumper_bar.n` must be a whole number from 1 to 999"))?;
        let key = jumper_bars::key(n);
        let cur_text = doc.devices().get(&c).and_then(|x| x.properties.get(&key)).cloned();
        let cur_text = cur_text.ok_or_else(|| CmdError::refused(format!("{} has no jumper bar {n}.", strip_words(doc, c))))?;
        let order = acc::positions(doc, c);
        let parsed = jumper_bars::parse(&cur_text).unwrap_or_default();
        level = parsed.level().unwrap_or(1);
        point = parsed.point().flatten();
        let cur = jumper_bar_labels(&parsed, &order);
        let tag = strip_words(doc, c);
        if parsed.crosses_levels() && !input.delete {
            return Err(CmdError::refused(format!(
                "Jumper bar {tag} {cur_text} joins levels of one position; delete it and draw it again, or set jumper_bar.{n}."
            )));
        }
        if input.delete {
            let ops = delete_ops(doc, c, n);
            return Ok(Jumper {
                strip: c,
                n,
                ops,
                label: format!("Delete jumper bar {tag} {cur_text}"),
                message: format!("Deleted jumper bar {tag} {cur_text}."),
                split: None,
            });
        }
        let mut next = cur.clone();
        if let Some(v) = &input.positions {
            next = labels_of(v, &order)?;
        }
        if let Some(v) = &input.add {
            for l in labels_of(v, &order)? {
                if !next.contains(&l) {
                    next.push(l);
                }
            }
        }
        if let Some(v) = &input.remove {
            let r = labels_of(v, &order)?;
            next.retain(|l| !r.contains(l));
        }
        if input.drop_missing {
            next.retain(|l| order.contains(l));
        }
        let mut extra: Option<(u32, Vec<String>)> = None;
        if let Some(at) = &input.split_at {
            let i = order.iter().position(|o| o == at).ok_or_else(|| no_position(doc, c, at, &order))?;
            let (first, rest): (Vec<String>, Vec<String>) =
                next.iter().cloned().partition(|l| order.iter().position(|o| o == l).is_some_and(|j| j <= i));
            if first.is_empty() || rest.is_empty() {
                return Err(CmdError::refused(format!("Jumper bar {cur_text} has no positions on both sides of {at}; pick a position inside it.")));
            }
            // A side of one position would be no jumper bar; never dropped.
            if first.len() < 2 || rest.len() < 2 {
                let one = if first.len() < 2 { &first } else { &rest };
                return Err(CmdError::refused(format!(
                    "Splitting jumper bar {cur_text} after {at} would leave {} alone. Shorten the jumper bar instead (remove {}), or split it at another position.",
                    one.join(", "),
                    one.join(", ")
                )));
            }
            let m = next_jumper_bar(doc, c).ok_or(CmdError::refused(JUMPER_BARS_FULL))?;
            next = first;
            extra = Some((m, rest));
        }
        let verb = if extra.is_some() {
            "Split"
        } else if next.len() > cur.len() {
            "Extend"
        } else if next.len() < cur.len() {
            "Shorten"
        } else {
            "Change"
        };
        (c, n, Some(cur_text), next, format!("{verb} jumper bar {tag}"), extra)
    } else {
        // Make a new jumper bar.
        let picks = &input.connection_points;
        let c = match (input.strip, picks.first()) {
            (Some(c), _) => c,
            (None, Some(t)) => doc.connection_points().get(t).and_then(|x| x.device()).ok_or(CmdError::NotFound(Noun::TerminalBlock))?,
            (None, None) => return Err(CmdError::input("`strip` with `positions`, or `connection_points`, is required")),
        };
        let (c, _) = strip_of(doc, Some(c), None)?;
        let order = acc::positions(doc, c);
        let mut labels: Vec<String> = Vec::new();
        let mut prev: Option<String> = None;
        // The first pick fixes the level; another level of the same position makes a
        // cross-level jumper bar; another level elsewhere is refused.
        if let Some(t) = picks.first() {
            level = crate::terminal_strips::terminal_blocks::level_of(doc, *t).0;
        }
        let mut cross_levels: Vec<u32> = Vec::new();
        for t in picks {
            let (lv, _) = crate::terminal_strips::terminal_blocks::level_of(doc, *t);
            let pos = crate::terminal_strips::terminal_blocks::position_label(doc, *t);
            if lv != level {
                if prev.as_deref() == Some(pos.as_str()) && labels.len() == 1 {
                    if cross_levels.is_empty() {
                        cross_levels.push(level);
                    }
                    cross_levels.push(lv);
                    continue;
                }
                let ty = crate::terminal_strips::terminal_blocks::type_of_connection_point(doc, *t);
                return Err(CmdError::refused(format!(
                    "{} is on level {}; this jumper bar is on level {}. Pick level {} terminals, or Enter to finish.",
                    crate::terminal_strips::connection_point_words(doc, *t),
                    crate::terminal_strips::terminal_blocks::level_name(doc, c, ty, lv),
                    crate::terminal_strips::terminal_blocks::level_name(doc, c, ty, level),
                    crate::terminal_strips::terminal_blocks::level_name(doc, c, ty, level)
                )));
            }
            if !cross_levels.is_empty() {
                return Err(CmdError::refused("A jumper bar between levels of one position ends there; Enter to finish."));
            }
            let term = doc.connection_points().get(t).ok_or(CmdError::NotFound(Noun::TerminalBlock))?;
            if term.device() != Some(c) {
                return Err(CmdError::refused(format!(
                    "{} is on another strip; a jumper bar stays on one strip. Pick a terminal of {}.",
                    crate::terminal_strips::connection_point_words(doc, *t),
                    strip_words(doc, c)
                )));
            }
            let l = crate::terminal_strips::terminal_blocks::position_label(doc, *t);
            let add = match (&prev, input.skip) {
                (Some(a), false) => range_between(&order, a, &l),
                _ => vec![l.clone()],
            };
            for x in add {
                if !labels.contains(&x) {
                    labels.push(x);
                }
            }
            prev = Some(l);
        }
        if let Some(v) = &input.positions {
            // Typed positions keep their level and point (`1.3-4.3`); a typed
            // cross-level jumper bar is kept as written.
            let typed = jumper_bars::parse(v)?;
            if picks.is_empty() {
                if typed.crosses_levels() {
                    cross = Some(typed.text());
                }
                level = typed.level().unwrap_or(1);
                point = typed.point().flatten();
            } else if typed.level().unwrap_or(1) != level {
                return Err(CmdError::refused(
                    "The typed positions are on another level than the picked terminals; type them on that level, or pick only.",
                ));
            }
            for l in labels_of(v, &order)? {
                if !labels.contains(&l) {
                    labels.push(l);
                }
            }
        }
        if let (Some(lo), Some(hi), Some(pos)) = (cross_levels.iter().min(), cross_levels.iter().max(), labels.first()) {
            // A cross-level jumper bar names both levels (`1.1-1.3`).
            cross = Some(format!("{pos}.{lo}-{pos}.{hi}"));
        }
        let n = next_jumper_bar(doc, c).ok_or(CmdError::refused(JUMPER_BARS_FULL))?;
        (c, n, None, labels, format!("Jumper bar {}", strip_words(doc, c)), None)
    };
    let order = acc::positions(doc, c);
    let tag = strip_words(doc, c);
    let mut ops = Vec::new();
    let text = if let Some(t) = cross.clone() {
        ops.extend(set_op(doc, c, jumper_bars::key(n), Some(t.clone())));
        Some(t)
    } else if labels.len() < 2 {
        if before_text.is_none() {
            return Err(CmdError::refused("A jumper bar joins at least 2 positions. Pick another terminal, or type positions like 1-4."));
        }
        ops.extend(delete_ops(doc, c, n));
        None
    } else {
        let t = canonical_of(&labels, &order, level, point);
        ops.extend(set_op(doc, c, jumper_bars::key(n), Some(t.clone())));
        Some(t)
    };
    let mut extra_text = None;
    if let Some((m, rest)) = &extra
        && rest.len() >= 2
    {
        let t = canonical_of(rest, &order, level, point);
        ops.extend(set_op(doc, c, jumper_bars::key(*m), Some(t.clone())));
        // Both halves keep the jumper bar's own part.
        for f in acc::FIELDS {
            let own = doc.devices().get(&c).and_then(|x| x.properties.get(&format!("{}.{f}", jumper_bars::key(n)))).cloned();
            if own.is_some() {
                ops.extend(set_op(doc, c, format!("{}.{f}", jumper_bars::key(*m)), own));
            }
        }
        extra_text = Some((*m, t));
    }
    let shown = text.clone().unwrap_or_else(|| "(removed)".into());
    let tx_label = match (&before_text, &extra_text) {
        (Some(b), Some((_, e))) => format!("{label} {b} \u{2192} {shown} + {e}"),
        (Some(b), None) => format!("{label} {b} \u{2192} {shown}"),
        (None, _) => format!("{label} {shown}"),
    };
    let message = match (&before_text, &text) {
        (Some(b), None) => format!("Removed jumper bar {tag} {b} (fewer than 2 positions)."),
        (Some(_), Some(_)) => format!("{tx_label}."),
        (None, _) => format!("Jumper bar {tag} {shown}."),
    };
    Ok(Jumper { strip: c, n, ops, label: tx_label, message, split: extra_text })
}

// ---------------------------------------------------------------------------------------------
// The commands (generic: crate::commands::run)

/// Words written as one text or as a list.
#[derive(Deserialize, JsonSchema)]
#[serde(untagged)]
pub(crate) enum WordsParam {
    /// A list.
    List(Vec<String>),
    /// One text (items and positions: comma-separated).
    Text(String),
}

/// Puts accessories on a strip's places (`strip` or a terminal, and `at`): replaces them (`items`), adds or
/// removes some, or empties placement keys (`clear`). All in one transaction.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AccessoryParams {
    /// The strip.
    #[serde(default)]
    strip: Option<DeviceId>,
    /// A terminal of the strip (its position is the place when `at` is absent).
    #[serde(default)]
    connection_point: Option<ConnectionPointId>,
    /// "start", "end", "all", a position label, or a list of them (at most 999).
    #[serde(default)]
    at: Option<WordsParam>,
    /// Markers on positions.
    #[serde(default)]
    on: Option<bool>,
    /// The items to put there (replace; null: none): end_stop, end_plate, partition, strip_label;
    /// with `on`: marker.
    #[serde(default, deserialize_with = "nullable")]
    items: Option<Option<WordsParam>>,
    /// Items to add.
    #[serde(default, deserialize_with = "nullable")]
    add: Option<Option<WordsParam>>,
    /// Items to remove.
    #[serde(default, deserialize_with = "nullable")]
    remove: Option<Option<WordsParam>>,
    /// Placement keys (acc.…) emptied in the same transaction.
    #[serde(default)]
    clear: Option<Vec<String>>,
}

/// The jumper bar a `jumper` edit names.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct JumperBarParam {
    /// The strip.
    #[serde(default)]
    strip: Option<DeviceId>,
    /// The jumper bar's number, 1 to 999.
    #[serde(default)]
    n: Option<u64>,
}

/// Makes a jumper bar (`{strip | connection_points, positions?, skip?}`: ranges between consecutive picks unless
/// `skip`), or edits one (`{jumper_bar, add?, remove?, positions?, split_at?, drop_missing?, delete?}`). One
/// transaction.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JumperParams {
    /// Edit: the jumper bar.
    #[serde(default)]
    jumper_bar: Option<JumperBarParam>,
    /// Make: the strip.
    #[serde(default)]
    strip: Option<DeviceId>,
    /// Make: the picked terminals (at most 999).
    #[serde(default)]
    connection_points: Option<Bounded<ConnectionPointId, 0, MAX_PICKS>>,
    /// Make: no ranges between consecutive picks.
    #[serde(default)]
    skip: Option<bool>,
    /// The positions: text like "1-4, 7" or a list of labels.
    #[serde(default)]
    positions: Option<WordsParam>,
    /// Edit: positions to add ("5,6" or a list).
    #[serde(default)]
    add: Option<WordsParam>,
    /// Edit: positions to remove ("5" or a list).
    #[serde(default)]
    remove: Option<WordsParam>,
    /// Edit: split the jumper bar after this position.
    #[serde(default)]
    split_at: Option<String>,
    /// Edit: drop the positions the strip lacks.
    #[serde(default)]
    drop_missing: Option<bool>,
    /// Edit: delete the jumper bar.
    #[serde(default)]
    delete: Option<bool>,
}

impl WordsParam {
    fn words(self) -> Words {
        match self {
            WordsParam::List(v) => Words::List(v),
            WordsParam::Text(t) => Words::Text(t),
        }
    }
}

impl Generic for AccessoryParams {
    const ID: &'static str = "accessory";
    const TITLE: &'static str = "Strip Accessory";

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let words = |w: Option<Option<WordsParam>>| w.map(|w| w.map(WordsParam::words));
        let input = AccessoryInput {
            strip: self.strip,
            connection_point: self.connection_point,
            at: self.at.map(WordsParam::words),
            on: self.on.unwrap_or(false),
            items: words(self.items),
            add: words(self.add),
            remove: words(self.remove),
            clear: self.clear,
        };
        let done = plan_accessory(cx.doc, input)?;
        let changed = !done.ops.is_empty();
        let keys: Vec<Value> =
            done.keys.iter().map(|(k, list)| json!({"key": k, "items": list.iter().map(|a| a.key()).collect::<Vec<_>>()})).collect();
        let answer = json!({"strip": done.strip.hex(), "keys": keys, "changed": changed, "message": done.message});
        Ok(Outcome::edit_if_any(done.label, done.ops, answer).echo_message())
    }
}

impl Generic for JumperParams {
    const ID: &'static str = "jumper";
    const TITLE: &'static str = "Jumper";

    fn run(self, cx: &mut Cx<'_>) -> Result<Outcome, CmdError> {
        let text = |w: Option<WordsParam>| w.map(|w| w.words().text());
        let input = JumperInput {
            jumper_bar: self.jumper_bar.map(|l| JumperBarRef { strip: l.strip, n: l.n }),
            strip: self.strip,
            connection_points: self.connection_points.map(|t| t.0).unwrap_or_default(),
            skip: self.skip.unwrap_or(false),
            positions: text(self.positions),
            add: text(self.add),
            remove: text(self.remove),
            split_at: self.split_at,
            drop_missing: self.drop_missing.unwrap_or(false),
            delete: self.delete.unwrap_or(false),
        };
        let done = plan_jumper(cx.doc, input)?;
        let (c, n) = (done.strip, done.n);
        let changed = !done.ops.is_empty();
        let (message, split) = (done.message, done.split);
        let answer = move |doc: &Document, views: &mut Views| {
            let positions = doc.devices().get(&c).and_then(|x| x.properties.get(&jumper_bars::key(n))).cloned();
            let bars = views.reports(doc).strips.get(c).and_then(|d| d.acc.jumper_bars.iter().find(|l| l.n == n)).map_or(0, |l| l.bars);
            let mut v = json!({"strip": c.hex(), "jumper_bar": n, "positions": positions, "bars": bars, "changed": changed, "message": message});
            if let Some((m, t)) = split {
                v["split"] = json!({"jumper_bar": m, "positions": t});
            }
            v
        };
        Ok(Outcome::edit_then_derived(done.label, done.ops, answer).echo_message())
    }
}
