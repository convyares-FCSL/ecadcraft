//! Links (brief 030, design 030 §5, FP5): positions of one component joined electrically by a
//! component property `link.<n>`, never a conductor.
//!
//! # Semantics
//! - A **link key** is `link.<n>` with `n` a whole number 1…[`MAX_LINK`] written without leading
//!   zeros. `link.<n>.part_number` and the like are not link keys.
//! - Its value names **positions** by terminal label: ranges `a-b` (every label from `a` to `b` in
//!   natural order, [`natural_key`]) and single labels, comma-separated (`1-4`, `7, 9, 11`).
//! - [`crate::connectivity`] joins the terminals of the component whose labels a link names
//!   (code, not format). ECAD's commands write link keys only on terminal strips; this crate does
//!   not know what a strip is and reads them on any component.
//! - Unreadable values join nothing; they are never an error here.
//! - **Levels (brief 036, FP3, FP8b).** An endpoint whose text ends in `.` + digits (+ an optional
//!   `i` or `o`) names a level of a position: `4.2` is position 4, level 2; `7.1o` the out point of
//!   a two-point level. Without a level an endpoint means level 1 (roadmap question 5). A range whose
//!   two ends have the same position is a **level range** (`1.1-1.3`, a cross-level link); any other
//!   range is a **position range** whose ends name the same level (`1.2-8.2`). A terminal's level
//!   comes from its key ([`level_of_key`]): `.n`, `.ni`, `.no`; any other key is level 1.
//!
//! This module reads text; it holds no I/O and does not depend on any CADCraft crate.

use std::cmp::Ordering;

use crate::doc::{Document, Properties};
use crate::id::{ComponentId, TerminalId};

/// What every link key starts with.
pub const PREFIX: &str = "link.";
/// The highest link number.
pub const MAX_LINK: u32 = 999;
/// Most positions (items) one link names.
pub const MAX_ITEMS: usize = 999;
/// Most characters of one position label.
pub const MAX_LABEL_CHARS: usize = 16;
/// Most characters of a link value read.
pub const MAX_TEXT_CHARS: usize = 20_000;

/// The highest level number a link or a level key may name.
pub const MAX_LEVEL: u32 = 99;

/// A terminal's level from its key (brief 036, FP2): `.2` → (2, none), `.1i` → (1, `i`). Any key
/// that does not start with `.` is level 1 of a one-level position; a dotted key that does not
/// read (`.0`, `.-1`, `.x`) is `None`.
pub fn level_of_key(key: Option<&str>) -> Option<(u32, Option<char>)> {
    let Some(k) = key.and_then(|k| k.strip_prefix('.')) else { return Some((1, None)) };
    level_text(k)
}

/// `2`, `1i`, `1o` → (level, point); `None` when it does not read or is out of range.
fn level_text(k: &str) -> Option<(u32, Option<char>)> {
    let (digits, point) = match k.chars().last() {
        Some(c @ ('i' | 'o')) => (k.get(..k.len() - 1)?, Some(c)),
        _ => (k, None),
    };
    if digits.is_empty() || digits.len() > 2 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n: u32 = digits.parse().ok()?;
    (1..=MAX_LEVEL).contains(&n).then_some((n, point))
}

/// An endpoint whose tail looks like a level but is none (`1.0`, `1.999`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BadLevel;

/// An endpoint split into its position and level (brief 036 §5.1): the last `.digits` (+ `i`/`o`)
/// is the level. `Err` when that tail looks like a level but is not one (`1.0`, `1.999`).
pub fn split_level(end: &str) -> Result<(&str, Option<(u32, Option<char>)>), BadLevel> {
    let Some((pos, tail)) = end.rsplit_once('.') else { return Ok((end, None)) };
    let core = tail.strip_suffix(['i', 'o']).unwrap_or(tail);
    if pos.is_empty() || core.is_empty() || !core.bytes().all(|b| b.is_ascii_digit()) {
        return Ok((end, None));
    }
    level_text(tail).map(|l| (pos, Some(l))).ok_or(BadLevel)
}

/// The natural sort key of a label: digit runs compare as numbers (`2` before `10`).
pub fn natural_key(s: &str) -> Vec<(u8, u128, String)> {
    let mut out = Vec::new();
    let mut digits = String::new();
    let mut other = String::new();
    for ch in s.chars() {
        if ch.is_ascii_digit() {
            if !other.is_empty() {
                out.push((1, 0, std::mem::take(&mut other)));
            }
            digits.push(ch);
        } else {
            if !digits.is_empty() {
                out.push((0, digits.parse().unwrap_or(u128::MAX), std::mem::take(&mut digits)));
            }
            other.push(ch);
        }
    }
    if !digits.is_empty() {
        out.push((0, digits.parse().unwrap_or(u128::MAX), digits));
    }
    if !other.is_empty() {
        out.push((1, 0, other));
    }
    out
}

fn cmp_labels(a: &str, b: &str) -> Ordering {
    natural_key(a).cmp(&natural_key(b))
}

/// The link number of `key` when it is a link key (`link.3` → 3).
pub fn link_number(key: &str) -> Option<u32> {
    let n = key.strip_prefix(PREFIX)?;
    if n.is_empty() || n.len() > 3 || !n.bytes().all(|b| b.is_ascii_digit()) || n.starts_with('0') {
        return None;
    }
    n.parse::<u32>().ok().filter(|n| (1..=MAX_LINK).contains(n))
}

/// The link key of number `n`.
pub fn key(n: u32) -> String {
    format!("{PREFIX}{n}")
}

/// One item of a link's value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    One(String),
    /// From the first to the second label, both included, the first not after the second.
    Range(String, String),
}

/// A link's positions as written.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Positions(pub Vec<Item>);

fn refusal(text: &str) -> String {
    let shown: String = text.chars().take(24).collect();
    let more = if text.chars().count() > 24 { "\u{2026}" } else { "" };
    format!("\"{shown}{more}\" is not a link. Write positions like 1-4 or 7, 9, 11.")
}

fn label(s: &str, text: &str) -> Result<String, String> {
    let l = s.trim();
    if l.is_empty() || l.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(refusal(text));
    }
    if l.chars().count() > MAX_LABEL_CHARS {
        return Err(format!("A position label has at most {MAX_LABEL_CHARS} characters."));
    }
    Ok(l.to_string())
}

fn level_refusal(end: &str) -> String {
    format!("\"{end}\" names no level. Levels are 1 to {MAX_LEVEL}: write 4.2 for position 4, level 2.")
}

/// Read a link's positions (spaces ignored).
///
/// # Errors
/// Unreadable text, an empty item, a label longer than [`MAX_LABEL_CHARS`], more than
/// [`MAX_ITEMS`] items, in words with the way forward.
pub fn parse(text: &str) -> Result<Positions, String> {
    if text.chars().count() > MAX_TEXT_CHARS {
        return Err(format!("A link names at most {MAX_ITEMS} positions."));
    }
    let mut items = Vec::new();
    for part in text.split(',') {
        if items.len() >= MAX_ITEMS {
            return Err(format!("A link names at most {MAX_ITEMS} positions."));
        }
        let part = part.trim();
        match part.split_once('-') {
            Some((a, b)) => {
                let (a, b) = (label(a, text)?, label(b, text)?);
                if b.contains('-') {
                    return Err(refusal(text));
                }
                let (pa, la) = split_level(&a).map_err(|_| level_refusal(&a))?;
                let (pb, lb) = split_level(&b).map_err(|_| level_refusal(&b))?;
                if pa != pb && la.unwrap_or((1, None)).0 != lb.unwrap_or((1, None)).0 {
                    return Err(format!(
                        "\"{a}-{b}\" joins different levels at different positions. Write {pa}.1-{pb}.1, or {pa}.1-{pa}.2 for levels of one position."
                    ));
                }
                let ord = if pa == pb { la.unwrap_or((1, None)).cmp(&lb.unwrap_or((1, None))) } else { cmp_labels(pa, pb) };
                items.push(match ord {
                    Ordering::Less => Item::Range(a, b),
                    Ordering::Equal => Item::One(a),
                    Ordering::Greater => Item::Range(b, a),
                });
            }
            None => {
                let l = label(part, text)?;
                split_level(&l).map_err(|_| level_refusal(&l))?;
                items.push(Item::One(l));
            }
        }
    }
    Ok(Positions(items))
}

/// One item of a link, read for matching (brief 036 §5.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Span {
    /// Positions `a` to `b` (natural order; equal for one position) on one level and point.
    Positions { a: String, b: String, level: u32, point: Option<char> },
    /// Levels `lo` to `hi` of one position: a cross-level link.
    Levels { position: String, lo: u32, hi: u32 },
}

impl Item {
    /// The item read for matching; `None` when an end does not read (never for a parsed item).
    pub fn span(&self) -> Option<Span> {
        let (a, b) = match self {
            Item::One(x) => (x.as_str(), x.as_str()),
            Item::Range(a, b) => (a.as_str(), b.as_str()),
        };
        let (pa, la) = split_level(a).ok()?;
        let (pb, lb) = split_level(b).ok()?;
        let (la, lb) = (la.unwrap_or((1, None)), lb.unwrap_or((1, None)));
        if pa == pb && la.0 != lb.0 {
            return Some(Span::Levels { position: pa.to_string(), lo: la.0.min(lb.0), hi: la.0.max(lb.0) });
        }
        Some(Span::Positions { a: pa.to_string(), b: pb.to_string(), level: la.0, point: la.1.or(lb.1) })
    }
}

impl Positions {
    /// Whether the link names the position labelled `l` (on any level).
    pub fn contains(&self, l: &str) -> bool {
        Matcher::of(self).matches(l)
    }

    /// As stored: `1-4`, `7, 9, 11`.
    pub fn text(&self) -> String {
        self.0
            .iter()
            .map(|i| match i {
                Item::One(x) => x.clone(),
                Item::Range(a, b) => format!("{a}-{b}"),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The items read for matching.
    pub fn spans(&self) -> Vec<Span> {
        self.0.iter().filter_map(Item::span).collect()
    }

    /// The one level every item names, when the link is on one level and crosses none (`1-4`: 1;
    /// `1.2-8.2`: 2; `1.1-1.3`: `None`).
    pub fn level(&self) -> Option<u32> {
        let mut out = None;
        for sp in self.spans() {
            match sp {
                Span::Positions { level, .. } if out.is_none_or(|l| l == level) => out = Some(level),
                _ => return None,
            }
        }
        out
    }

    /// Whether the link joins levels of one position (`1.1-1.3`).
    pub fn crosses_levels(&self) -> bool {
        self.spans().iter().any(|s| matches!(s, Span::Levels { .. }))
    }

    /// Every label the link names out of `order` (labels in plan order), in that order.
    pub fn select<'a>(&self, order: &'a [String]) -> Vec<&'a String> {
        let keys: Vec<Vec<(u8, u128, String)>> = order.iter().map(|l| natural_key(l)).collect();
        self.select_keyed(order, &keys)
    }

    /// [`Positions::select`] with the natural keys of `order` given (`keys[i]` is `order[i]`'s).
    pub fn select_keyed<'a>(&self, order: &'a [String], keys: &[Vec<(u8, u128, String)>]) -> Vec<&'a String> {
        let m = Matcher::of(self);
        order.iter().zip(keys).filter(|(l, k)| m.matches_keyed(l, k)).map(|(l, _)| l).collect()
    }

    /// The positions the items name that are not in `order` (a single position missing, or a
    /// range end missing).
    pub fn missing(&self, order: &[String]) -> Vec<String> {
        let has = |l: &str| order.iter().any(|o| o == l);
        let mut out: Vec<String> = Vec::new();
        for sp in self.spans() {
            let ends = match &sp {
                Span::Positions { a, b, .. } => vec![a.clone(), b.clone()],
                Span::Levels { position, .. } => vec![position.clone()],
            };
            for e in ends {
                if !has(&e) && !out.contains(&e) {
                    out.push(e);
                }
            }
        }
        out
    }
}

/// The positions `labels` (plan order) as canonical text: runs of consecutive positions in
/// `order` become ranges (`1, 2, 3, 4` → `1-4`); a run of two stays a range.
pub fn canonical(labels: &[&String], order: &[String]) -> String {
    let idx = |l: &String| order.iter().position(|o| o == l);
    let mut out: Vec<String> = Vec::new();
    let mut run: Vec<(usize, &String)> = Vec::new();
    let flush = |run: &mut Vec<(usize, &String)>, out: &mut Vec<String>| {
        match (run.first(), run.last()) {
            (Some((_, a)), Some((_, b))) if run.len() >= 2 => out.push(format!("{a}-{b}")),
            (Some((_, a)), _) => out.push((*a).clone()),
            _ => {}
        }
        run.clear();
    };
    for l in labels {
        match idx(l) {
            Some(i) => {
                if run.last().is_some_and(|(j, _)| j + 1 != i) {
                    flush(&mut run, &mut out);
                }
                run.push((i, l));
            }
            None => {
                flush(&mut run, &mut out);
                out.push((*l).clone());
            }
        }
    }
    flush(&mut run, &mut out);
    out.join(", ")
}

/// The link keys of `props` with their readable positions, by number.
pub fn of(props: &Properties) -> Vec<(u32, Positions)> {
    props
        .range(PREFIX.to_string()..)
        .take_while(|(k, _)| k.starts_with(PREFIX))
        .filter_map(|(k, v)| Some((link_number(k)?, parse(v).ok()?)))
        .collect()
}

/// A link's positions ready for many lookups: single positions hashed, range ends keyed once
/// (review of PR #57, 2: a lookup costs the link's ranges, never its whole text).
pub struct Matcher {
    /// Single positions: position → (level, point) items.
    ones: std::collections::HashMap<String, Vec<(u32, Option<char>)>>,
    ranges: Vec<(NaturalKey, NaturalKey, u32, Option<char>)>,
    /// Cross-level items: position → level ranges.
    levels: std::collections::HashMap<String, Vec<(u32, u32)>>,
}

/// A label's natural sort key ([`natural_key`]).
pub type NaturalKey = Vec<(u8, u128, String)>;

/// Whether an item's point names a terminal's point: an item without a point names every one-point
/// level and the in point of a two-point level.
fn point_ok(item: Option<char>, term: Option<char>) -> bool {
    match (item, term) {
        (None, None | Some('i')) => true,
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

impl Matcher {
    pub fn of(p: &Positions) -> Matcher {
        let mut m = Matcher { ones: Default::default(), ranges: Vec::new(), levels: Default::default() };
        for sp in p.spans() {
            match sp {
                Span::Positions { a, b, level, point } if a == b => m.ones.entry(a).or_default().push((level, point)),
                Span::Positions { a, b, level, point } => m.ranges.push((natural_key(&a), natural_key(&b), level, point)),
                Span::Levels { position, lo, hi } => m.levels.entry(position).or_default().push((lo, hi)),
            }
        }
        m
    }

    /// Whether the link names the position labelled `l` (on any level).
    pub fn matches(&self, l: &str) -> bool {
        if self.ones.contains_key(l) || self.levels.contains_key(l) {
            return true;
        }
        if self.ranges.is_empty() {
            return false;
        }
        self.matches_keyed(l, &natural_key(l))
    }

    fn matches_keyed(&self, l: &str, k: &NaturalKey) -> bool {
        self.ones.contains_key(l) || self.levels.contains_key(l) || self.ranges.iter().any(|(a, b, _, _)| a <= k && k <= b)
    }

    /// Whether the link names the terminal at position `l`, level `level`, point `point` (brief
    /// 036, FP8b).
    pub fn matches_level(&self, l: &str, level: u32, point: Option<char>) -> bool {
        if self.ones.get(l).is_some_and(|v| v.iter().any(|(n, p)| *n == level && point_ok(*p, point))) {
            return true;
        }
        if self.levels.get(l).is_some_and(|v| v.iter().any(|(lo, hi)| *lo <= level && level <= *hi && point_ok(None, point))) {
            return true;
        }
        if self.ranges.is_empty() {
            return false;
        }
        let k = natural_key(l);
        self.ranges.iter().any(|(a, b, n, p)| *n == level && point_ok(*p, point) && *a <= k && k <= *b)
    }

    /// [`Matcher::matches_level`] for a terminal: its label and its key's level.
    pub fn matches_terminal(&self, t: &crate::doc::Terminal) -> bool {
        let Some(l) = t.label.as_deref() else { return false };
        let Some((n, p)) = level_of_key(t.key.as_deref()) else { return false };
        self.matches_level(l, n, p)
    }
}

/// Labels in plan order: natural order ([`natural_key`]), each once. The one ordering of a strip's
/// positions, used by ECAD's strips, accessories and links.
pub fn plan_order(labels: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut v: Vec<(NaturalKey, String)> = labels.into_iter().map(|l| (natural_key(&l), l)).collect();
    v.sort();
    v.dedup_by(|a, b| a.1 == b.1);
    v.into_iter().map(|(_, l)| l).collect()
}

/// The terminals of `component` joined to `t` by its links (not `t` itself), level by level
/// (brief 036). One pass over the
/// component's terminals per link that names `t`.
pub fn linked(doc: &Document, t: TerminalId) -> Vec<TerminalId> {
    let Some(term) = doc.terminals.get(&t) else { return Vec::new() };
    let Some(comp) = doc.components.get(&term.component) else { return Vec::new() };
    let mut out = std::collections::BTreeSet::new();
    for (_, p) in of(&comp.properties) {
        let m = Matcher::of(&p);
        if !m.matches_terminal(term) {
            continue;
        }
        for (id, x) in doc.terminals_of(term.component) {
            if id != t && m.matches_terminal(x) {
                out.insert(id);
            }
        }
    }
    out.into_iter().collect()
}

/// The terminals of `component` a link value names (unreadable: none).
pub fn terminals_named(doc: &Document, component: ComponentId, value: Option<&str>) -> Vec<TerminalId> {
    let Some(p) = value.and_then(|v| parse(v).ok()) else { return Vec::new() };
    let m = Matcher::of(&p);
    doc.terminals_of(component).filter(|(_, x)| m.matches_terminal(x)).map(|(id, _)| id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_and_values() {
        assert_eq!(link_number("link.1"), Some(1));
        assert_eq!(link_number("link.999"), Some(999));
        assert_eq!(link_number("link.0"), None);
        assert_eq!(link_number("link.1000"), None);
        assert_eq!(link_number("link.01"), None);
        assert_eq!(link_number("link.1.part_number"), None);
        let p = parse(" 1 - 4 ,7").unwrap_or_default();
        assert_eq!(p.text(), "1-4, 7");
        assert!(p.contains("3") && p.contains("7") && !p.contains("5") && !p.contains("10"));
        assert_eq!(parse("4-1").map(|p| p.text()), Ok("1-4".into()));
        assert!(parse("1-x-2").is_err());
        assert!(parse("1,,2").is_err());
        assert!(parse(&"1,".repeat(5_000)).is_err());
        assert!(parse(&"9".repeat(20)).is_err());
        let order: Vec<String> = (1..=12).map(|n| n.to_string()).collect();
        let pick: Vec<&String> = ["1", "2", "3", "4", "7", "9", "10"].iter().filter_map(|l| order.iter().find(|o| o == l)).collect();
        assert_eq!(canonical(&pick, &order), "1-4, 7, 9-10");
        assert_eq!(parse("3-15").map(|p| p.missing(&order)), Ok(vec!["15".to_string()]));
    }
}
