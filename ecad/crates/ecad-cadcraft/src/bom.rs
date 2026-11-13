//! Brief 047 (design 047): part lines, items and article numbers on the BOM, and the commands
//! `partline`, `bomitem` and `article`.
//!
//! # Semantics
//! - A **part line** is one more part of a device or strip, stored as the component properties
//!   `part.<n>.part_number | maker | description | article_number | qty | unit | kit | for`, `<n>`
//!   1…99 with no leading zero (design §2.1, FP2). A line exists when any of its keys is set. The
//!   device's own `part_number`, `maker`, `description` and `article_number` are line 0, its main
//!   part (§2.2).
//! - An **item** is a tagless component with `kind: "bom_item"` and no representation: material
//!   that belongs to no device (§3, FP1). It has one part, a quantity, a unit and a location.
//! - `for` names the sub-items a line serves: a fuse bank's fuses, a strip's positions (§2.3).
//! - Every value goes through [`setprop_rule`], also when written by `setprop`; a refusal is in
//!   words and writes nothing. No new op and no format change: everything is `SetProperty`,
//!   `CreateComponent` and `DeleteComponent`.
//!
//! Excluded: the parts catalogue and kits from it (2.3); counts derived from the drawing; the
//! BOM model itself, which is [`crate::outputs`].

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use cadcraft_engine::{CommandSpec, Session};
use ecad_model::{Component, ComponentId, Document, ObjectRef, Op, Properties, Subject};
use serde_json::{Value, json};

use crate::commands::{MAX_HANDLES, id_list, opt_id, with_project};
use crate::defaults::{self, Scope};

/// The `kind` of an item (FP1).
pub const ITEM_KIND: &str = "bom_item";
/// The prefix of a part line's keys.
pub const PART_PREFIX: &str = "part.";
/// The article number of a device's main part, an item, or (with a prefix) a line or strip part.
pub const ARTICLE_KEY: &str = "article_number";
/// An item's quantity and unit (a line's are `part.<n>.qty`, `part.<n>.unit`).
pub const QTY_KEY: &str = "qty";
pub const UNIT_KEY: &str = "unit";
/// The stored unit of a length; pieces are stored as no unit.
pub const METRES: &str = "m";

/// Most part lines per component: line numbers are 1…99 (a guard: a script cannot give a device
/// thousands of keys, and the key stays two digits).
pub const MAX_LINES: u8 = 99;
/// Most lines one `partline` call adds (a kit): a guard against a script filling a device.
pub const MAX_ADD_LINES: usize = 20;
/// Most sub-items one `for` names after expanding ranges: a guard against `1-99999`.
pub const MAX_SUBS: usize = 999;
/// Most characters of a `for` value read: longer is refused before it is parsed.
pub const MAX_FOR_CHARS: usize = 2000;
/// Most items in a project (house rule 12: it stops a script from filling the project).
pub const MAX_ITEMS: usize = 10_000;
/// The largest quantity, in thousandths (9 999): a guard that keeps sums far from overflow.
pub const MAX_QTY_MILLI: u64 = 9_999_000;
/// Most characters of an article number or a kit name.
pub const MAX_ARTICLE_CHARS: usize = 64;
pub const MAX_KIT_CHARS: usize = 64;
/// Most characters of one sub-item name in `for` (a strip label or a fuse number).
pub const MAX_SUB_CHARS: usize = 12;
/// Most components `partline {components}` and `article` touch in one call.
pub const MAX_COMPONENTS: usize = MAX_HANDLES;

/// Refusals, in words.
pub const ITEM_NO_TAG: &str = "An item has no tag. Add a device instead if it is drawn.";
pub const ITEM_NO_LINES: &str = "An item has no part lines: it is one part. Set its own part_number, maker and qty.";
pub const ITEM_NEEDS_DESCRIPTION: &str = "An item needs a description.";
pub const PARTLINE_FOR_DEVICES: &str = "partline is for devices and strips.";

/// One field of a part line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Field {
    PartNumber,
    Maker,
    Description,
    ArticleNumber,
    Qty,
    Unit,
    Kit,
    For,
}

impl Field {
    pub const ALL: [Field; 8] =
        [Field::PartNumber, Field::Maker, Field::Description, Field::ArticleNumber, Field::Qty, Field::Unit, Field::Kit, Field::For];

    /// The key's last part (`part_number`).
    pub fn word(self) -> &'static str {
        match self {
            Field::PartNumber => "part_number",
            Field::Maker => "maker",
            Field::Description => "description",
            Field::ArticleNumber => ARTICLE_KEY,
            Field::Qty => QTY_KEY,
            Field::Unit => UNIT_KEY,
            Field::Kit => "kit",
            Field::For => "for",
        }
    }

    pub fn parse(word: &str) -> Option<Field> {
        Field::ALL.into_iter().find(|f| f.word() == word)
    }
}

/// `part.<n>.<field>`.
pub fn part_key(n: u8, f: Field) -> String {
    format!("{PART_PREFIX}{n}.{}", f.word())
}

/// What a `part.` key names: `None` when `key` is not a part key at all, else the line and field
/// or why the key is not one ECAD reads (`<n>` outside 1…99, a leading zero, an unknown field).
pub fn parse_part_key(key: &str) -> Option<Result<(u8, Field), String>> {
    let rest = key.strip_prefix(PART_PREFIX)?;
    let shown: String = key.chars().take(40).collect();
    let bad = || format!("\"{shown}\" is not a part line key. Write part.<n>.<field>, n 1 to {MAX_LINES}, field one of {}.", field_words());
    let Some((n, f)) = rest.split_once('.') else { return Some(Err(bad())) };
    let ok_n = !n.is_empty() && n.len() <= 2 && n.bytes().all(|b| b.is_ascii_digit()) && !n.starts_with('0');
    let n = if ok_n { n.parse::<u8>().ok().filter(|n| (1..=MAX_LINES).contains(n)) } else { None };
    match (n, Field::parse(f)) {
        (Some(n), Some(f)) => Some(Ok((n, f))),
        _ => Some(Err(bad())),
    }
}

fn field_words() -> String {
    Field::ALL.iter().map(|f| f.word()).collect::<Vec<_>>().join(", ")
}

// ---------------------------------------------------------------- quantities (§4.4)

/// A quantity in thousandths: a count of pieces, or metres.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Qty(pub u64);

impl Qty {
    pub const ONE: Qty = Qty(1000);

    /// A whole count.
    pub fn count(n: usize) -> Qty {
        Qty(u64::try_from(n).unwrap_or(u64::MAX / 1000).saturating_mul(1000))
    }

    /// Whether it is a whole number.
    pub fn whole(self) -> bool {
        self.0 % 1000 == 0
    }

    /// The sum, or `None` on overflow (never a panic).
    pub fn checked_add(self, o: Qty) -> Option<Qty> {
        self.0.checked_add(o.0).map(Qty)
    }

    /// The product with a count, or `None` on overflow.
    pub fn checked_mul(self, n: usize) -> Option<Qty> {
        u64::try_from(n).ok().and_then(|n| self.0.checked_mul(n)).map(Qty)
    }

    /// As a JSON number: whole numbers as integers, others with their decimals.
    pub fn json(self) -> Value {
        if self.whole() { json!(self.0 / 1000) } else { json!(self.0 as f64 / 1000.0) }
    }
}

impl fmt::Display for Qty {
    /// `4`, `5.6`, `0.75`: at most three decimals, trailing zeros trimmed.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (w, r) = (self.0 / 1000, self.0 % 1000);
        if r == 0 {
            return write!(f, "{w}");
        }
        let frac = format!("{r:03}");
        write!(f, "{w}.{}", frac.trim_end_matches('0'))
    }
}

/// A quantity as typed: a number greater than 0 and at most 9 999, with at most three decimals,
/// and whole unless `metres`. Plain digits only (no sign, no exponent, so never NaN or infinity).
///
/// # Errors
/// Anything else, in words.
pub fn parse_qty(text: &str, metres: bool) -> Result<Qty, String> {
    let t = text.trim();
    let shown: String = t.chars().take(20).collect();
    let more = if t.chars().count() > 20 { "…" } else { "" };
    let bad = || format!("\"{shown}{more}\" is not a quantity. Write a number greater than 0, at most 9999, like 2 or 3.6.");
    let (w, frac) = t.split_once('.').unwrap_or((t, ""));
    if w.is_empty() && frac.is_empty() || w.len() > 6 || !w.bytes().all(|b| b.is_ascii_digit()) || !frac.bytes().all(|b| b.is_ascii_digit()) {
        return Err(bad());
    }
    if frac.len() > 3 {
        return Err(format!("\"{shown}{more}\" has more than three decimals."));
    }
    let whole: u64 = if w.is_empty() { 0 } else { w.parse().map_err(|_| bad())? };
    let thousandths: u64 = if frac.is_empty() { 0 } else { format!("{frac:0<3}").parse().map_err(|_| bad())? };
    let q = whole.checked_mul(1000).and_then(|x| x.checked_add(thousandths)).ok_or_else(bad)?;
    if q == 0 || q > MAX_QTY_MILLI {
        return Err(bad());
    }
    if !metres && q % 1000 != 0 {
        return Err(format!("\"{shown}\" pieces: a count of pieces is whole. Set the unit to m for a length."));
    }
    Ok(Qty(q))
}

/// A unit as typed: `m` (metres) or `pcs` (pieces, stored as no unit).
///
/// # Errors
/// Any other word.
pub fn parse_unit(text: &str) -> Result<bool, String> {
    match text.trim().to_ascii_lowercase().as_str() {
        "m" | "metre" | "metres" | "meter" | "meters" => Ok(true),
        "pcs" | "pc" | "piece" | "pieces" | "" => Ok(false),
        _ => Err("A unit is m (metres) or pcs (pieces).".into()),
    }
}

// ---------------------------------------------------------------- text values

/// 1 to `max` characters, trimmed, no control characters; empty clears.
fn plain(key: &str, value: Option<&str>, max: usize) -> Result<Option<String>, String> {
    let Some(v) = value.map(str::trim).filter(|v| !v.is_empty()) else { return Ok(None) };
    if v.chars().count() > max || v.chars().any(char::is_control) {
        return Err(format!("`{key}` can be at most {max} characters, with no control characters."));
    }
    Ok(Some(v.to_string()))
}

// ---------------------------------------------------------------- sub-items (§2.3)

/// What a device's `for` may name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Subs {
    /// Nothing: `for` is refused.
    None,
    /// A fuse bank's fuses, `1`…`n`.
    Fuses(Vec<String>),
    /// A strip's positions by label, and which of them are fuse positions.
    Positions { all: Vec<String>, fuses: Vec<String> },
}

impl Subs {
    fn names(&self) -> &[String] {
        match self {
            Subs::None => &[],
            Subs::Fuses(v) => v,
            Subs::Positions { all, .. } => all,
        }
    }

    /// The fuses a fuse insert would serve (a bank's fuses, a strip's fuse positions).
    pub fn fuses(&self) -> &[String] {
        match self {
            Subs::None => &[],
            Subs::Fuses(v) => v,
            Subs::Positions { fuses, .. } => fuses,
        }
    }
}

/// The head symbol of `c` (its one exclusive symbol), when it is a starter.
fn head(doc: &Document, c: ComponentId) -> Option<&'static crate::symbols::Starter> {
    doc.representations_of(Subject::Component(c))
        .filter_map(|(_, r)| crate::devices::rep_starter(doc, r))
        .find(|s| s.kind == crate::symbols::Kind::Fixed { exclusive: true })
}

/// What `for` may name on `c`.
pub fn sub_items(doc: &Document, c: ComponentId) -> Subs {
    if crate::strips::is_strip(doc, c) {
        let ps = crate::terminal_types::positions(doc, c);
        let fuses = ps.iter().filter(|p| p.ty.code == "fuse").map(|p| p.label.clone()).collect();
        return Subs::Positions { all: ps.into_iter().map(|p| p.label).collect(), fuses };
    }
    match head(doc, c).filter(|s| crate::symbols::is_fuse_bank(s)) {
        Some(bank) => Subs::Fuses(bank.labels().into_iter().filter(|l| !l.is_empty() && l.bytes().all(|b| b.is_ascii_digit())).collect()),
        None => Subs::None,
    }
}

/// Whether `c` is a fuse terminal device (034's `fuseterm`): covered by a part line with no `for`.
pub fn is_fuse_terminal(doc: &Document, c: ComponentId) -> bool {
    head(doc, c).is_some_and(|s| s.key == "fuseterm")
}

/// The names a `for` value lists, ranges of whole numbers expanded (`1-4, 7`), in order, each
/// once.
///
/// # Errors
/// Too long, a range the wrong way round, more than [`MAX_SUBS`] names, or a name that is too long
/// or holds control characters.
pub fn expand_for(text: &str) -> Result<Vec<String>, String> {
    if text.chars().count() > MAX_FOR_CHARS {
        return Err(format!("`for` names at most {MAX_SUBS} positions or fuses."));
    }
    let mut out: Vec<String> = Vec::new();
    let mut seen = BTreeSet::new();
    let mut push = |name: String, out: &mut Vec<String>| -> Result<(), String> {
        if seen.insert(name.clone()) {
            out.push(name);
        }
        if out.len() > MAX_SUBS { Err(format!("`for` names at most {MAX_SUBS} positions or fuses.")) } else { Ok(()) }
    };
    for part in text.split(',') {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        let range = p.split_once('-').and_then(|(a, b)| Some((a.trim().parse::<u64>().ok()?, b.trim().parse::<u64>().ok()?)));
        if let Some((a, b)) = range {
            if a > b {
                return Err(format!("\"{p}\" goes the wrong way round. Write the smaller number first."));
            }
            if b - a >= MAX_SUBS as u64 {
                return Err(format!("`for` names at most {MAX_SUBS} positions or fuses."));
            }
            for i in a..=b {
                push(i.to_string(), &mut out)?;
            }
            continue;
        }
        if p.chars().count() > MAX_SUB_CHARS || p.chars().any(char::is_control) {
            let shown: String = p.chars().take(MAX_SUB_CHARS).collect();
            return Err(format!("\"{shown}…\" is not a position or fuse: names are at most {MAX_SUB_CHARS} characters."));
        }
        push(p.to_string(), &mut out)?;
    }
    Ok(out)
}

/// `1-8`, `1, 2, 5-6`: names as ranges of consecutive whole numbers.
pub fn names_words(names: &[String]) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while let Some(n) = names.get(i) {
        let mut j = i;
        if let Ok(start) = n.parse::<u64>() {
            while names.get(j + 1).and_then(|x| x.parse::<u64>().ok()) == Some(start + (j + 1 - i) as u64) {
                j += 1;
            }
        }
        out.push(if j > i { format!("{n}-{}", names.get(j).map_or("", String::as_str)) } else { n.clone() });
        i = j + 1;
    }
    out.join(", ")
}

/// Check a `for` value on `c`: every name a sub-item of `c` (§2.3).
fn check_for(doc: &Document, c: ComponentId, text: &str) -> Result<Option<String>, String> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(None);
    }
    let who = crate::devices::tag_words(doc, c);
    let subs = sub_items(doc, c);
    if subs == Subs::None {
        return Err(format!("{who} has nothing to name with for: leave it empty and set qty."));
    }
    let names = expand_for(t)?;
    for n in &names {
        if !subs.names().contains(n) {
            return Err(match &subs {
                Subs::Fuses(f) => format!("{who} has no fuse {n}. Its fuses are {}.", names_words(f)),
                _ => format!("{who} has no position {n}."),
            });
        }
    }
    Ok(Some(t.to_string()))
}

// ---------------------------------------------------------------- lines

/// One part line as read from a component.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Line {
    pub n: u8,
    pub part_number: Option<String>,
    pub maker: Option<String>,
    pub description: Option<String>,
    pub article_number: Option<String>,
    pub qty: Qty,
    pub metres: bool,
    pub kit: Option<String>,
    /// The `for` value as stored, and the names it lists (empty: the device as a whole).
    pub for_text: Option<String>,
    pub subs: Vec<String>,
}

fn clean(v: Option<&String>) -> Option<String> {
    v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// The part lines of a component, in line order, and the `part.` keys ECAD does not read (a file
/// made elsewhere; at most 20 kept). A stored value that does not parse reads as its default and
/// its key is listed too.
pub fn lines(props: &Properties) -> (Vec<Line>, Vec<String>) {
    let mut by_n: BTreeMap<u8, Line> = BTreeMap::new();
    let mut bad = Vec::new();
    let note = |k: &str, bad: &mut Vec<String>| {
        if bad.len() < 20 {
            bad.push(k.chars().take(60).collect());
        }
    };
    for (k, v) in props.range(PART_PREFIX.to_string()..) {
        if !k.starts_with(PART_PREFIX) {
            break;
        }
        match parse_part_key(k) {
            Some(Ok((n, f))) => {
                let line = by_n.entry(n).or_insert_with(|| Line { n, qty: Qty::ONE, ..Line::default() });
                match f {
                    Field::PartNumber => line.part_number = clean(Some(v)),
                    Field::Maker => line.maker = clean(Some(v)),
                    Field::Description => line.description = clean(Some(v)),
                    Field::ArticleNumber => line.article_number = clean(Some(v)),
                    Field::Kit => line.kit = clean(Some(v)),
                    Field::For => line.for_text = clean(Some(v)),
                    Field::Unit => match parse_unit(v) {
                        Ok(m) => line.metres = m,
                        Err(_) => note(k, &mut bad),
                    },
                    Field::Qty => {}
                }
            }
            _ => note(k, &mut bad),
        }
    }
    // Quantities after units (a line's qty is whole unless its unit is metres).
    for line in by_n.values_mut() {
        if let Some(q) = props.get(&part_key(line.n, Field::Qty)) {
            match parse_qty(q, line.metres) {
                Ok(q) => line.qty = q,
                Err(_) => note(&part_key(line.n, Field::Qty), &mut bad),
            }
        }
        if let Some(t) = &line.for_text {
            match expand_for(t) {
                Ok(v) => line.subs = v,
                Err(_) => note(&part_key(line.n, Field::For), &mut bad),
            }
        }
    }
    (by_n.into_values().collect(), bad)
}

/// The line numbers in use on `props`.
fn used_lines(props: &Properties) -> BTreeSet<u8> {
    props
        .range(PART_PREFIX.to_string()..)
        .take_while(|(k, _)| k.starts_with(PART_PREFIX))
        .filter_map(|(k, _)| parse_part_key(k).and_then(Result::ok).map(|(n, _)| n))
        .collect()
}

/// The next free line number: one above the highest, else the lowest gap.
fn next_line(used: &BTreeSet<u8>) -> Option<u8> {
    match used.last() {
        None => Some(1),
        Some(m) if *m < MAX_LINES => Some(m + 1),
        Some(_) => (1..=MAX_LINES).find(|n| !used.contains(n)),
    }
}

// ---------------------------------------------------------------- items (§3)

/// Whether `c` is an item.
pub fn is_item(doc: &Document, c: ComponentId) -> bool {
    crate::wires::kind_of(doc, c) == Some(ITEM_KIND)
}

/// Whether a component's properties make it an item.
pub fn is_item_component(c: &Component) -> bool {
    c.properties.get(ecad_model::connectivity::KIND).map(String::as_str) == Some(ITEM_KIND)
}

/// Whether `c` takes part lines: a device or strip (a tag and no `kind`).
fn takes_lines(doc: &Document, c: ComponentId) -> bool {
    crate::locations::is_device(doc, c)
}

/// Refuse keys an item cannot take, before any other rule runs (`tag`, `part.*`, clearing its
/// description).
///
/// # Errors
/// [`ITEM_NO_TAG`], [`ITEM_NO_LINES`], [`ITEM_NEEDS_DESCRIPTION`].
pub fn refuse_on_item(doc: &Document, c: ComponentId, key: &str, value: Option<&str>) -> Result<(), String> {
    if !is_item(doc, c) {
        return Ok(());
    }
    if key == ecad_model::tag::TAG_KEY || key == ecad_model::tag::FIXED_KEY {
        return Err(ITEM_NO_TAG.into());
    }
    if key.starts_with(PART_PREFIX) {
        return Err(ITEM_NO_LINES.into());
    }
    if key == "description" && value.map(str::trim).is_none_or(str::is_empty) {
        return Err(ITEM_NEEDS_DESCRIPTION.into());
    }
    Ok(())
}

/// Whether [`setprop_rule`] has an opinion on `key` of `c`.
pub fn is_bom_key(doc: &Document, c: ComponentId, key: &str) -> bool {
    key == ARTICLE_KEY || key.starts_with(PART_PREFIX) || (is_item(doc, c) && matches!(key, QTY_KEY | UNIT_KEY))
}

/// The `setprop` rule for 047's keys on component `c` (design §2.4): `article_number`,
/// `part.<n>.*`, and an item's `qty` and `unit`. Returns the value to store.
///
/// # Errors
/// Every refusal of §2.1, §2.3 and §3, in words. Clearing a part line key is always allowed.
pub fn setprop_rule(doc: &Document, c: ComponentId, key: &str, value: Option<&str>) -> Result<Option<String>, String> {
    refuse_on_item(doc, c, key, value)?;
    let props = doc.components.get(&c).map(|x| &x.properties);
    let clearing = value.map(str::trim).is_none_or(str::is_empty);
    if key == ARTICLE_KEY {
        return plain(key, value, MAX_ARTICLE_CHARS);
    }
    if is_item(doc, c) && key == QTY_KEY {
        if clearing {
            return Ok(None);
        }
        let metres = props.and_then(|p| p.get(UNIT_KEY)).is_some_and(|u| parse_unit(u).unwrap_or(false));
        return parse_qty(value.unwrap_or(""), metres).map(|_| value.map(|v| v.trim().to_string()));
    }
    if is_item(doc, c) && key == UNIT_KEY {
        return unit_value(props.and_then(|p| p.get(QTY_KEY)).map(String::as_str), value);
    }
    let Some(parsed) = parse_part_key(key) else { return Ok(value.map(str::to_string)) };
    if clearing {
        return Ok(None);
    }
    let (n, f) = parsed?;
    if !takes_lines(doc, c) {
        return Err(PARTLINE_FOR_DEVICES.into());
    }
    let metres = props.and_then(|p| p.get(&part_key(n, Field::Unit))).is_some_and(|u| parse_unit(u).unwrap_or(false));
    let qty = props.and_then(|p| p.get(&part_key(n, Field::Qty))).map(String::as_str);
    field_value(doc, c, f, value, metres, qty)
}

/// A unit value, checked against the quantity it will apply to.
fn unit_value(qty: Option<&str>, value: Option<&str>) -> Result<Option<String>, String> {
    let metres = parse_unit(value.unwrap_or(""))?;
    if !metres && let Some(q) = qty.and_then(|q| parse_qty(q, true).ok()).filter(|q| !q.whole()) {
        return Err(format!("The quantity is {q}: a count of pieces is whole. Set a whole quantity first."));
    }
    Ok(metres.then(|| METRES.to_string()))
}

/// One field's value for a line of `c` (with the line's unit `metres` and stored `qty`).
fn field_value(doc: &Document, c: ComponentId, f: Field, value: Option<&str>, metres: bool, qty: Option<&str>) -> Result<Option<String>, String> {
    let key = f.word();
    match f {
        Field::PartNumber | Field::Maker | Field::Description => crate::devices::property_value(doc, key, value),
        Field::ArticleNumber => plain(key, value, MAX_ARTICLE_CHARS),
        Field::Kit => plain(key, value, MAX_KIT_CHARS),
        Field::Qty => match value.map(str::trim).filter(|v| !v.is_empty()) {
            None => Ok(None),
            Some(v) => parse_qty(v, metres).map(|_| Some(v.to_string())),
        },
        Field::Unit => unit_value(qty, value),
        Field::For => check_for(doc, c, value.unwrap_or("")),
    }
}

// ---------------------------------------------------------------- defaults (§8)

/// Whether the BOM shows its Article number column (`bom.column.article`).
pub fn article_column(doc: &Document) -> bool {
    defaults::resolve(doc, &defaults::bom::COLUMN_ARTICLE, Scope::project()).word() == "on"
}

/// Whether part lines are listed under their device (`bom.lines = nested`).
pub fn lines_nested(doc: &Document) -> bool {
    defaults::resolve(doc, &defaults::bom::LINES, Scope::project()).word() == "nested"
}

/// Whether a new item's unit is metres (`bom.item.unit = m`).
pub fn item_unit_metres(doc: &Document) -> bool {
    defaults::resolve(doc, &defaults::bom::ITEM_UNIT, Scope::project()).word() == METRES
}

// ---------------------------------------------------------------- commands (§7)

/// The commands of this module, registered through [`crate::lanes`].
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("partline", "Part Line", run_partline)
            .params(
                "{component: id | components: [id…], add: {part_number?, maker?, description?, article_number?, qty?, unit?: \"pcs\"|\"m\", for?: \"1,2,4\"} \
| add: [{…}, …] (at most 20), kit?: name} | {component, line: n, set: {field: value|null}} | {component, line: n, remove: true} (one transaction)",
            )
            .noundo(),
        CommandSpec::new("bomitem", "BOM Item", run_bomitem)
            .params(
                "{description, qty?, unit?: \"pcs\"|\"m\", location?: \"+CP1\", part_number?, maker?, article_number?} (adds an item) \
| {item: id, set: {field: value|null}} | {item: id, delete: true} (one transaction)",
            )
            .interactive(crate::interact::bomitem)
            .noundo(),
        CommandSpec::new("article", "Article Number", run_article)
            .params("{part_number, maker?, article_number: text|null} (sets it on every part with that part number; one transaction)")
            .noundo(),
    ]
}

/// A field value from JSON: a string, a number (written as typed), or `null` (clear).
fn json_text(v: &Value, what: &str) -> Result<Option<String>, String> {
    match v {
        Value::Null => Ok(None),
        Value::String(s) => Ok(Some(s.clone())),
        Value::Number(n) => Ok(Some(n.to_string())),
        _ => Err(format!("`{what}` must be text, a number, or null")),
    }
}

/// The fields of one `add` or `set` object, in field order.
fn fields_of(v: &Value, set: bool) -> Result<Vec<(Field, Option<String>)>, String> {
    let o = v.as_object().ok_or("a part line is an object of fields: {part_number, maker, description, qty…}")?;
    let mut out = Vec::new();
    for (k, x) in o {
        let f =
            Field::parse(k).filter(|f| set || *f != Field::Kit).ok_or_else(|| format!("`{k}` is not a part line field. Use {}.", field_words()))?;
        out.push((f, json_text(x, k)?));
    }
    // Unit before qty, so the qty is checked against the line's new unit.
    out.sort_by_key(|(f, _)| *f);
    Ok(out)
}

/// The ops writing `fields` on line `n` of `c`, every value checked.
fn line_ops(doc: &Document, c: ComponentId, n: u8, fields: &[(Field, Option<String>)]) -> Result<Vec<Op>, String> {
    let props = doc.components.get(&c).map(|x| &x.properties).ok_or("no such component")?;
    let given = |f: Field| fields.iter().find(|(g, _)| *g == f).map(|(_, v)| v.clone());
    let metres = match given(Field::Unit) {
        Some(u) => parse_unit(u.as_deref().unwrap_or(""))?,
        None => props.get(&part_key(n, Field::Unit)).is_some_and(|u| parse_unit(u).unwrap_or(false)),
    };
    let qty = match given(Field::Qty) {
        Some(q) => q,
        None => props.get(&part_key(n, Field::Qty)).cloned(),
    };
    let mut ops = Vec::new();
    for (f, v) in fields {
        let after = match f {
            Field::Unit => unit_value(qty.as_deref(), v.as_deref())?,
            _ => field_value(doc, c, *f, v.as_deref(), metres, qty.as_deref())?,
        };
        let key = part_key(n, *f);
        let before = props.get(&key).cloned();
        if before != after {
            ops.push(Op::SetProperty { object: ObjectRef::Component(c), key, before, after });
        }
    }
    Ok(ops)
}

/// The components a `partline` call names, each one a device or strip.
fn partline_targets(doc: &Document, params: &Value) -> Result<Vec<ComponentId>, String> {
    let cs: Vec<ComponentId> = if params.get("components").is_some() {
        id_list(params, "components", MAX_COMPONENTS)?
    } else {
        vec![opt_id(params, "component")?.ok_or("`component` or `components` (hex id) is required")?]
    };
    if cs.is_empty() {
        return Err("`components` is empty".into());
    }
    for c in &cs {
        if !doc.components.contains_key(c) {
            return Err(format!("No component {}.", c.hex()));
        }
        if !takes_lines(doc, *c) {
            return Err(PARTLINE_FOR_DEVICES.into());
        }
    }
    Ok(cs)
}

fn run_partline(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let v = with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let cs = partline_targets(doc, params)?;
        let mut ops = Vec::new();
        let mut result = Vec::new();
        let (label, echo) = if let Some(add) = params.get("add") {
            let kit = match params.get("kit") {
                None | Some(Value::Null) => None,
                Some(Value::String(k)) => plain("kit", Some(k), MAX_KIT_CHARS)?,
                Some(_) => return Err("`kit` must be text".into()),
            };
            let adds: Vec<&Value> = match add {
                Value::Array(a) => a.iter().collect(),
                o => vec![o],
            };
            if adds.is_empty() {
                return Err("`add` names no line".into());
            }
            if adds.len() > MAX_ADD_LINES {
                return Err(format!("At most {MAX_ADD_LINES} lines per call."));
            }
            let lines: Vec<Vec<(Field, Option<String>)>> = adds.iter().map(|a| fields_of(a, false)).collect::<Result<_, _>>()?;
            for c in &cs {
                let props = doc.components.get(c).map(|x| &x.properties).ok_or("no such component")?;
                let mut used = used_lines(props);
                let mut ns = Vec::new();
                for fields in &lines {
                    let n = next_line(&used)
                        .ok_or_else(|| format!("{} has {MAX_LINES} part lines; remove one first.", crate::devices::tag_words(doc, *c)))?;
                    used.insert(n);
                    let mut fields = fields.clone();
                    if let Some(k) = &kit {
                        fields.push((Field::Kit, Some(k.clone())));
                    }
                    if fields.iter().all(|(_, v)| v.as_deref().is_none_or(|t| t.trim().is_empty())) {
                        return Err("A part line needs at least one field: part_number, maker or description.".into());
                    }
                    let line = line_ops(doc, *c, n, &fields)?;
                    ops.extend(line);
                    ns.push(n);
                }
                result.push(json!({"component": c.hex(), "lines": ns}));
            }
            let lines_words = if lines.len() == 1 { "a part line".to_string() } else { format!("{} part lines", lines.len()) };
            let to = if cs.len() == 1 { crate::devices::tag_words(doc, cs[0]) } else { format!("{} devices", cs.len()) };
            match (&kit, lines.len()) {
                (Some(k), _) => (format!("Add kit {k}"), format!("Added kit {k} ({lines_words}) to {to}.")),
                (None, 1) => ("Add part line".to_string(), format!("Added {lines_words} to {to}.")),
                (None, _) => ("Add part lines".to_string(), format!("Added {lines_words} to {to}.")),
            }
        } else {
            let n = params.get("line").and_then(Value::as_u64).and_then(|n| u8::try_from(n).ok()).filter(|n| (1..=MAX_LINES).contains(n));
            let n = n.ok_or_else(|| format!("`line` (1 to {MAX_LINES}) is required with `set` or `remove`, or give `add`"))?;
            for c in &cs {
                let props = doc.components.get(c).map(|x| &x.properties).ok_or("no such component")?;
                if !used_lines(props).contains(&n) {
                    return Err(format!("{} has no part line {n}.", crate::devices::tag_words(doc, *c)));
                }
                if params.get("remove").and_then(Value::as_bool) == Some(true) {
                    let prefix = format!("{PART_PREFIX}{n}.");
                    for (k, v) in props.range(prefix.clone()..).take_while(|(k, _)| k.starts_with(&prefix)) {
                        ops.push(Op::SetProperty { object: ObjectRef::Component(*c), key: k.clone(), before: Some(v.clone()), after: None });
                    }
                } else if let Some(set) = params.get("set") {
                    let fields = fields_of(set, true)?;
                    ops.extend(line_ops(doc, *c, n, &fields)?);
                } else {
                    return Err("Give `add`, `set` or `remove: true`.".into());
                }
                result.push(json!({"component": c.hex(), "lines": [n]}));
            }
            if params.get("remove").and_then(Value::as_bool) == Some(true) {
                ("Remove part line".to_string(), format!("Removed part line {n}."))
            } else {
                ("Edit part line".to_string(), format!("Edited part line {n}."))
            }
        };
        if ops.is_empty() {
            return Ok(json!({"changed": false, "components": result}));
        }
        p.apply_edit(pg, "partline", &label, ops)?;
        let mut v = match result.as_slice() {
            [one] => one.clone(),
            _ => json!({"components": result}),
        };
        v["changed"] = json!(true);
        v["message"] = json!(echo);
        Ok(v)
    })?;
    if let Some(m) = v.get("message").and_then(Value::as_str) {
        s.echo(m.to_string());
    }
    Ok(v)
}

/// The item fields `bomitem` takes, in the order they are checked (unit before qty).
const ITEM_FIELDS: [&str; 7] = ["description", "unit", "qty", "location", "part_number", "maker", ARTICLE_KEY];

/// An item property's value, checked (with the item's unit `metres` and stored `qty`).
fn item_value(doc: &Document, key: &str, value: Option<&str>, metres: bool, qty: Option<&str>) -> Result<Option<String>, String> {
    match key {
        "description" => match crate::devices::property_value(doc, key, value)? {
            None => Err(ITEM_NEEDS_DESCRIPTION.into()),
            v => Ok(v),
        },
        UNIT_KEY => unit_value(qty, value),
        QTY_KEY => match value.map(str::trim).filter(|v| !v.is_empty()) {
            None => Ok(None),
            Some(v) => parse_qty(v, metres).map(|_| Some(v.to_string())),
        },
        ARTICLE_KEY => plain(key, value, MAX_ARTICLE_CHARS),
        _ => crate::devices::property_value(doc, key, value),
    }
}

/// The item fields of a `bomitem` object.
fn item_fields(v: &Value) -> Result<BTreeMap<&'static str, Option<String>>, String> {
    let o = v.as_object().ok_or("an item is an object of fields")?;
    let mut out = BTreeMap::new();
    for (k, x) in o {
        let f =
            ITEM_FIELDS.iter().find(|f| **f == k.as_str()).ok_or_else(|| format!("`{k}` is not an item field. Use {}.", ITEM_FIELDS.join(", ")))?;
        out.insert(*f, json_text(x, k)?);
    }
    Ok(out)
}

/// `Slotted trunking 50x75, 3.6 m in +CP1`.
pub fn item_words(props: &Properties) -> String {
    let d = props.get("description").map_or("?", String::as_str);
    let metres = props.get(UNIT_KEY).is_some_and(|u| parse_unit(u).unwrap_or(false));
    let q = props.get(QTY_KEY).and_then(|q| parse_qty(q, metres).ok()).unwrap_or(Qty::ONE);
    let unit = if metres { " m" } else { "" };
    match props.get("location") {
        Some(l) => format!("{d}, {q}{unit} in {l}"),
        None => format!("{d}, {q}{unit}"),
    }
}

fn run_bomitem(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let v = with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        if let Some(item) = opt_id::<ComponentId>(params, "item")? {
            let comp = doc.components.get(&item).filter(|c| is_item_component(c)).ok_or_else(|| format!("{} is not an item.", item.hex()))?;
            if params.get("delete").and_then(Value::as_bool) == Some(true) {
                let words = item_words(&comp.properties);
                let ops = vec![Op::DeleteComponent { id: item, component: comp.clone() }];
                p.apply_edit(pg, "bomitem", "Delete item", ops)?;
                return Ok(json!({"changed": true, "item": item.hex(), "message": format!("Deleted item {words}.")}));
            }
            let set = params.get("set").ok_or("Give `set: {field: value}` or `delete: true` with `item`.")?;
            let fields = item_fields(set)?;
            let props = &comp.properties;
            let metres = match fields.get(UNIT_KEY) {
                Some(u) => parse_unit(u.as_deref().unwrap_or(""))?,
                None => props.get(UNIT_KEY).is_some_and(|u| parse_unit(u).unwrap_or(false)),
            };
            let qty = match fields.get(QTY_KEY) {
                Some(q) => q.clone(),
                None => props.get(QTY_KEY).cloned(),
            };
            let mut ops = Vec::new();
            for key in ITEM_FIELDS {
                let Some(v) = fields.get(key) else { continue };
                let after = item_value(doc, key, v.as_deref(), metres, qty.as_deref())?;
                let before = props.get(key).cloned();
                if before != after {
                    ops.push(Op::SetProperty { object: ObjectRef::Component(item), key: key.to_string(), before, after });
                }
            }
            if ops.is_empty() {
                return Ok(json!({"changed": false, "item": item.hex()}));
            }
            p.apply_edit(pg, "bomitem", "Edit item", ops)?;
            return Ok(json!({"changed": true, "item": item.hex(), "message": "Edited the item."}));
        }
        let fields = item_fields(params)?;
        if !fields.contains_key("description") {
            return Err(ITEM_NEEDS_DESCRIPTION.into());
        }
        let items = doc.components.values().filter(|c| is_item_component(c)).count();
        if items >= MAX_ITEMS {
            return Err(format!("This project has {items} items, the most a project holds."));
        }
        let metres = match fields.get(UNIT_KEY) {
            Some(u) => parse_unit(u.as_deref().unwrap_or(""))?,
            None => item_unit_metres(doc),
        };
        let qty = fields.get(QTY_KEY).cloned().flatten();
        let mut props = Properties::new();
        props.insert(ecad_model::connectivity::KIND.into(), ITEM_KIND.into());
        if metres {
            props.insert(UNIT_KEY.into(), METRES.into());
        }
        for key in ITEM_FIELDS {
            if key == UNIT_KEY {
                continue;
            }
            let Some(v) = fields.get(key) else { continue };
            if let Some(x) = item_value(doc, key, v.as_deref(), metres, qty.as_deref())? {
                props.insert(key.to_string(), x);
            }
        }
        let words = item_words(&props);
        let id: ComponentId = p.ids.fresh();
        p.apply_edit(pg, "bomitem", "Add item", vec![Op::CreateComponent { id, component: Component { properties: props, definition: None } }])?;
        Ok(json!({"changed": true, "item": id.hex(), "message": format!("Added item {words}.")}))
    })?;
    if let Some(m) = v.get("message").and_then(Value::as_str) {
        s.echo(m.to_string());
    }
    Ok(v)
}

/// Whether a stored value equals `want` (trimmed, case-insensitive).
fn same(v: Option<&String>, want: &str) -> bool {
    v.is_some_and(|x| x.trim().eq_ignore_ascii_case(want))
}

/// The article keys `article` writes for part number `pn` (and maker `maker`) on `doc`, with what
/// each is (`device`, `line`, `item`, `strip`).
pub fn article_targets(doc: &Document, pn: &str, maker: Option<&str>) -> Vec<(ComponentId, String, &'static str)> {
    let mut out = Vec::new();
    for (c, comp) in &doc.components {
        let props = &comp.properties;
        let item = is_item_component(comp);
        let device = !item && props.contains_key(ecad_model::tag::TAG_KEY) && !props.contains_key(ecad_model::connectivity::KIND);
        if !(item || device) {
            continue;
        }
        let maker_ok = |k: &str| maker.is_none_or(|m| same(props.get(k), m));
        if same(props.get("part_number"), pn) && maker_ok("maker") && !crate::strips::is_strip(doc, *c) {
            out.push((*c, ARTICLE_KEY.to_string(), if item { "item" } else { "device" }));
        }
        if device {
            for n in used_lines(props) {
                if same(props.get(&part_key(n, Field::PartNumber)), pn) && maker_ok(&part_key(n, Field::Maker)) {
                    out.push((*c, part_key(n, Field::ArticleNumber), "line"));
                }
            }
        }
    }
    out
}

fn run_article(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let v = with_project(s, |p, pg, _| {
        let doc = p.ledger.document();
        let pn = params.get("part_number").and_then(Value::as_str).map(str::trim).filter(|t| !t.is_empty());
        let pn = pn.ok_or("`part_number` is required")?;
        let maker = match params.get("maker") {
            None | Some(Value::Null) => None,
            Some(Value::String(m)) => Some(m.trim()).filter(|m| !m.is_empty()),
            Some(_) => return Err("`maker` must be text".into()),
        };
        let value = match params.get("article_number") {
            Some(v) => json_text(v, "article_number")?,
            None => return Err("`article_number` is required (text, or null to clear)".into()),
        };
        let after = plain(ARTICLE_KEY, value.as_deref(), MAX_ARTICLE_CHARS)?;
        let targets = article_targets(doc, pn, maker);
        if targets.is_empty() {
            return Err(format!("No part {pn} in this project."));
        }
        if targets.len() > MAX_COMPONENTS * 4 {
            return Err(format!("{pn} is used {} times; at most {} per call.", targets.len(), MAX_COMPONENTS * 4));
        }
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        let mut ops = Vec::new();
        for (c, key, what) in &targets {
            *counts.entry(what).or_default() += 1;
            let before = doc.components.get(c).and_then(|x| x.properties.get(key)).cloned();
            if before != after {
                ops.push(Op::SetProperty { object: ObjectRef::Component(*c), key: key.clone(), before, after: after.clone() });
            }
        }
        let parts: Vec<String> = [("line", "line", "lines"), ("device", "device", "devices"), ("item", "item", "items")]
            .iter()
            .filter_map(|(k, one, many)| counts.get(k).map(|n| format!("{n} {}", if *n == 1 { one } else { many })))
            .collect();
        let n = targets.len();
        let what = match &after {
            Some(a) => format!("Set {a} on {n} part{} ({}).", if n == 1 { "" } else { "s" }, parts.join(", ")),
            None => format!("Cleared the article number of {n} part{} ({}).", if n == 1 { "" } else { "s" }, parts.join(", ")),
        };
        if ops.is_empty() {
            return Ok(json!({"changed": false, "parts": n, "message": what}));
        }
        p.apply_edit(pg, "article", "Set article number", ops)?;
        Ok(json!({"changed": true, "parts": n, "message": what}))
    })?;
    if let Some(m) = v.get("message").and_then(Value::as_str) {
        s.echo(m.to_string());
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantities_parse_and_print() {
        assert_eq!(parse_qty("3.6", true), Ok(Qty(3600)));
        assert_eq!(parse_qty("1.50", true), Ok(Qty(1500)));
        assert_eq!(parse_qty("4", false), Ok(Qty(4000)));
        for bad in ["NaN", "-1", "1e308", "0.0001", "0", "10000", "", ".", "1.5.5", "inf"] {
            assert!(parse_qty(bad, true).is_err(), "{bad}");
        }
        assert!(parse_qty("1.5", false).is_err());
        assert!(parse_qty(&"9".repeat(10_000), true).is_err());
        assert_eq!(Qty(5600).to_string(), "5.6");
        assert_eq!(Qty(750).to_string(), "0.75");
        assert_eq!(Qty(4000).to_string(), "4");
    }

    #[test]
    fn part_keys_parse() {
        assert_eq!(parse_part_key("part.1.maker"), Some(Ok((1, Field::Maker))));
        assert_eq!(parse_part_key("part.99.for"), Some(Ok((99, Field::For))));
        for bad in ["part.0.maker", "part.007.maker", "part.100.maker", "part.1.colour", "part.1", "part..maker"] {
            assert!(matches!(parse_part_key(bad), Some(Err(_))), "{bad}");
        }
        assert_eq!(parse_part_key("maker"), None);
    }

    #[test]
    fn for_expands_and_is_bounded() {
        assert_eq!(expand_for("1, 5-6, 1").unwrap(), ["1", "5", "6"]);
        assert!(expand_for("1-99999").is_err());
        assert!(expand_for("4-1").is_err());
        assert_eq!(names_words(&["1".into(), "2".into(), "3".into(), "5".into(), "PE".into()]), "1-3, 5, PE");
    }

    #[test]
    fn next_line_fills_after_the_highest() {
        assert_eq!(next_line(&BTreeSet::new()), Some(1));
        assert_eq!(next_line(&[1, 3].into_iter().collect()), Some(4));
        assert_eq!(next_line(&[2, 99].into_iter().collect()), Some(1));
        assert_eq!(next_line(&(1..=99).collect()), None);
    }
}
