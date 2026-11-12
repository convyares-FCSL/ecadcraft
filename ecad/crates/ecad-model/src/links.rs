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
                items.push(match cmp_labels(&a, &b) {
                    Ordering::Less => Item::Range(a, b),
                    Ordering::Equal => Item::One(a),
                    Ordering::Greater => Item::Range(b, a),
                });
            }
            None => items.push(Item::One(label(part, text)?)),
        }
    }
    Ok(Positions(items))
}

impl Positions {
    /// Whether the link names the position labelled `l`.
    pub fn contains(&self, l: &str) -> bool {
        let k = natural_key(l);
        self.0.iter().any(|i| match i {
            Item::One(x) => x == l,
            Item::Range(a, b) => natural_key(a) <= k && k <= natural_key(b),
        })
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

    /// Every label the link names out of `order` (labels in plan order), in that order.
    pub fn select<'a>(&self, order: &'a [String]) -> Vec<&'a String> {
        let keys: Vec<Vec<(u8, u128, String)>> = order.iter().map(|l| natural_key(l)).collect();
        self.select_keyed(order, &keys)
    }

    /// [`Positions::select`] with the natural keys of `order` given (`keys[i]` is `order[i]`'s).
    pub fn select_keyed<'a>(&self, order: &'a [String], keys: &[Vec<(u8, u128, String)>]) -> Vec<&'a String> {
        let ranges: Vec<(Vec<(u8, u128, String)>, Vec<(u8, u128, String)>)> = self
            .0
            .iter()
            .filter_map(|i| match i {
                Item::Range(a, b) => Some((natural_key(a), natural_key(b))),
                Item::One(_) => None,
            })
            .collect();
        order
            .iter()
            .zip(keys)
            .filter(|(l, k)| self.0.iter().any(|i| matches!(i, Item::One(x) if x == *l)) || ranges.iter().any(|(a, b)| a <= *k && *k <= b))
            .map(|(l, _)| l)
            .collect()
    }

    /// The labels the items name that are not in `order` (a single label missing, or a range end
    /// missing).
    pub fn missing(&self, order: &[String]) -> Vec<String> {
        let has = |l: &String| order.iter().any(|o| o == l);
        let mut out = Vec::new();
        for i in &self.0 {
            match i {
                Item::One(x) if !has(x) => out.push(x.clone()),
                Item::Range(a, b) => {
                    for e in [a, b] {
                        if !has(e) && !out.contains(e) {
                            out.push(e.clone());
                        }
                    }
                }
                _ => {}
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

/// A link's positions ready for many lookups: single labels hashed, range ends keyed once
/// (review of PR #57, 2: a lookup costs the link's ranges, never its whole text).
pub struct Matcher {
    ones: std::collections::HashSet<String>,
    ranges: Vec<(NaturalKey, NaturalKey)>,
}

/// A label's natural sort key ([`natural_key`]).
pub type NaturalKey = Vec<(u8, u128, String)>;

impl Matcher {
    pub fn of(p: &Positions) -> Matcher {
        let mut m = Matcher { ones: Default::default(), ranges: Vec::new() };
        for i in &p.0 {
            match i {
                Item::One(x) => {
                    m.ones.insert(x.clone());
                }
                Item::Range(a, b) => m.ranges.push((natural_key(a), natural_key(b))),
            }
        }
        m
    }

    /// Whether the link names the position labelled `l`.
    pub fn matches(&self, l: &str) -> bool {
        if self.ones.contains(l) {
            return true;
        }
        if self.ranges.is_empty() {
            return false;
        }
        let k = natural_key(l);
        self.ranges.iter().any(|(a, b)| *a <= k && k <= *b)
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

/// The terminals of `component` joined to `t` by its links (not `t` itself). One pass over the
/// component's terminals per link that names `t`.
pub fn linked(doc: &Document, t: TerminalId) -> Vec<TerminalId> {
    let Some(term) = doc.terminals.get(&t) else { return Vec::new() };
    let Some(l) = term.label.as_deref() else { return Vec::new() };
    let Some(comp) = doc.components.get(&term.component) else { return Vec::new() };
    let mut out = std::collections::BTreeSet::new();
    for (_, p) in of(&comp.properties) {
        let m = Matcher::of(&p);
        if !m.matches(l) {
            continue;
        }
        for (id, x) in doc.terminals_of(term.component) {
            if id != t && x.label.as_deref().is_some_and(|y| m.matches(y)) {
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
    doc.terminals_of(component).filter(|(_, x)| x.label.as_deref().is_some_and(|l| m.matches(l))).map(|(id, _)| id).collect()
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
