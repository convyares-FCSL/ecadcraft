//! Brief 047 (design 047): part lines, items and article numbers on the BOM, and the commands
//! `partline`, `bomitem` and `article`.
//!
//! # Semantics
//! - A **part line** is one more part of a device or strip, stored as the component properties
//!   `part.<n>.part_number | maker | description | article_number | qty | unit | kit | for`, `<n>`
//!   1…99 with no leading zero (design §2.1, FP2). A line exists when any of its keys is set. The
//!   device's own `part_number`, `maker`, `description` and `article_number` are line 0, its main
//!   part (§2.2).
//! - An **item** is a loose part (`Document.loose_parts`, design 072 §6.4): material that belongs
//!   to no device and is not drawn (§3, FP1). It has one part, a quantity, a unit and a location.
//! - `for` names the sub-items a line serves: a fuse bank's fuses, a strip's positions (§2.3).
//! - Every value goes through [`setprop_rule`], also when written by `setprop`; a refusal is in
//!   words and writes nothing. Part lines are `SetProperty`; items are `CreateLoosePart`,
//!   `ModifyLoosePart` and `DeleteLoosePart`.
//!
//! Excluded: the parts catalogue and kits from it (2.3); counts derived from the drawing; the
//! BOM model itself, which is [`crate::reports`].

use crate::CmdError;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use ecad_model::objects::loose_part::LoosePart;
use ecad_model::{DeviceId, Document, LoosePartId, ObjectRef, Op, Properties};
use serde_json::{Value, json};

use crate::defaults::{self, Scope};

/// The prefix of a part line's keys.
pub(crate) const PART_PREFIX: &str = "part.";
/// The article number of a device's main part, an item, or (with a prefix) a line or strip part.
pub const ARTICLE_KEY: &str = ecad_model::objects::devices::ARTICLE_KEY;
/// An item's quantity and unit (a line's are `part.<n>.qty`, `part.<n>.unit`).
pub(crate) const QTY_KEY: &str = "qty";
pub(crate) const UNIT_KEY: &str = "unit";
/// The stored unit of a length; pieces are stored as no unit.
pub(crate) const METRES: &str = "m";

/// Most part lines per component: line numbers are 1…99 (a guard: a script cannot give a device
/// thousands of keys, and the key stays two digits).
pub(crate) const MAX_LINES: u8 = 99;
/// Most lines one `partline` call adds (a kit): a guard against a script filling a device.
pub(crate) const MAX_ADD_LINES: usize = 20;
/// Most sub-items one `for` names after expanding ranges: a guard against `1-99999`.
pub(crate) const MAX_SUBS: usize = 999;
/// Most characters of a `for` value read: longer is refused before it is parsed.
pub(crate) const MAX_FOR_CHARS: usize = 2000;
/// Most items in a project (house rule 12: it stops a script from filling the project).
pub const MAX_ITEMS: usize = 10_000;
/// The largest quantity, in thousandths (9 999): a guard that keeps sums far from overflow.
pub const MAX_QTY_MILLI: u64 = 9_999_000;
/// Most characters of an article number or a kit name.
pub(crate) const MAX_ARTICLE_CHARS: usize = 64;
pub(crate) const MAX_KIT_CHARS: usize = 64;
/// Most characters of one sub-item name in `for` (a strip label or a fuse number).
pub(crate) const MAX_SUB_CHARS: usize = 12;
/// Most components `partline {components}` and `article` touch in one call.
pub(crate) const MAX_COMPONENTS: usize = crate::cables::words::MAX_HANDLES;

/// Refusals, in words.
pub const ITEM_NEEDS_DESCRIPTION: &str = "An item needs a description.";
pub(crate) const PARTLINE_FOR_DEVICES: &str = "partline is for devices and strips.";

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
    /// Brief 044 (design 044 §8.3): on a connector, the half the line belongs to (`s` or `p`).
    Half,
}

impl Field {
    pub const ALL: [Field; 9] =
        [Field::PartNumber, Field::Maker, Field::Description, Field::ArticleNumber, Field::Qty, Field::Unit, Field::Kit, Field::For, Field::Half];

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
            Field::Half => "half",
        }
    }

    pub fn parse(word: &str) -> Option<Field> {
        Field::ALL.into_iter().find(|f| f.word() == word)
    }
}

/// `part.<n>.<field>`.
pub(crate) fn part_key(n: u8, f: Field) -> String {
    format!("{PART_PREFIX}{n}.{}", f.word())
}

/// What a `part.` key names: `None` when `key` is not a part key at all, else the line and field
/// or why the key is not one ECAD reads (`<n>` outside 1…99, a leading zero, an unknown field).
pub fn parse_part_key(key: &str) -> Option<Result<(u8, Field), CmdError>> {
    let rest = key.strip_prefix(PART_PREFIX)?;
    let shown: String = key.chars().take(40).collect();
    let bad = || {
        CmdError::refused(format!("\"{shown}\" is not a part line key. Write part.<n>.<field>, n 1 to {MAX_LINES}, field one of {}.", field_words()))
    };
    let Some((n, f)) = rest.split_once('.') else { return Some(Err(bad())) };
    let n = ecad_model::naming::words::uint_canonical(n, u32::from(MAX_LINES)).filter(|n| *n >= 1).and_then(|n| u8::try_from(n).ok());
    match (n, Field::parse(f)) {
        (Some(n), Some(f)) => Some(Ok((n, f))),
        _ => Some(Err(bad())),
    }
}

pub(crate) fn field_words() -> String {
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
        self.0.is_multiple_of(1000)
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
pub fn parse_qty(text: &str, metres: bool) -> Result<Qty, CmdError> {
    let t = text.trim();
    let shown: String = t.chars().take(20).collect();
    let more = if t.chars().count() > 20 { "…" } else { "" };
    let bad = || CmdError::refused(format!("\"{shown}{more}\" is not a quantity. Write a number greater than 0, at most 9999, like 2 or 3.6."));
    let (w, frac) = t.split_once('.').unwrap_or((t, ""));
    let digits_or_none = |t: &str| t.is_empty() || ecad_model::naming::words::is_digits(t);
    if w.is_empty() && frac.is_empty() || w.len() > 6 || !digits_or_none(w) || !digits_or_none(frac) {
        return Err(bad());
    }
    if frac.len() > 3 {
        return Err(CmdError::refused(format!("\"{shown}{more}\" has more than three decimals.")));
    }
    let whole: u64 = if w.is_empty() { 0 } else { w.parse().map_err(|_| bad())? };
    let thousandths: u64 = if frac.is_empty() { 0 } else { format!("{frac:0<3}").parse().map_err(|_| bad())? };
    let q = whole.checked_mul(1000).and_then(|x| x.checked_add(thousandths)).ok_or_else(bad)?;
    if q == 0 || q > MAX_QTY_MILLI {
        return Err(bad());
    }
    if !metres && !q.is_multiple_of(1000) {
        return Err(CmdError::refused(format!("\"{shown}\" pieces: a count of pieces is whole. Set the unit to m for a length.")));
    }
    Ok(Qty(q))
}

/// A unit as typed: `m` (metres) or `pcs` (pieces, stored as no unit).
///
/// # Errors
/// Any other word.
pub(crate) fn parse_unit(text: &str) -> Result<bool, CmdError> {
    match text.trim().to_ascii_lowercase().as_str() {
        "m" | "metre" | "metres" | "meter" | "meters" => Ok(true),
        "pcs" | "pc" | "piece" | "pieces" | "" => Ok(false),
        _ => Err(CmdError::refused("A unit is m (metres) or pcs (pieces).")),
    }
}

// ---------------------------------------------------------------- text values

/// 1 to `max` characters, trimmed, no control characters; empty clears.
/// An article number as stored (trimmed; empty clears it).
///
/// # Errors
/// Longer than [`MAX_ARTICLE_CHARS`], or control characters.
pub(crate) fn article_value(value: Option<&str>) -> Result<Option<String>, CmdError> {
    plain(ARTICLE_KEY, value, MAX_ARTICLE_CHARS)
}

pub fn plain(key: &str, value: Option<&str>, max: usize) -> Result<Option<String>, CmdError> {
    let Some(v) = value.map(str::trim).filter(|v| !v.is_empty()) else { return Ok(None) };
    if v.chars().count() > max || v.chars().any(char::is_control) {
        return Err(CmdError::refused(format!("`{key}` can be at most {max} characters, with no control characters.")));
    }
    Ok(Some(v.to_string()))
}

// ---------------------------------------------------------------- sub-items (§2.3)

/// What a device's `for` may name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Subs {
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

/// What `for` may name on `c`.
pub(crate) fn sub_items(doc: &Document, c: DeviceId) -> Subs {
    if crate::terminal_strips::strip_of(doc, c).is_some() {
        let ps = crate::terminal_strips::terminal_blocks::positions(doc, c);
        let fuses = ps.iter().filter(|p| p.ty.code == "fuse").map(|p| p.label.clone()).collect();
        return Subs::Positions { all: ps.into_iter().map(|p| p.label).collect(), fuses };
    }
    let fuses = crate::symbols::rules::bank_labels(doc, c);
    if fuses.is_empty() { Subs::None } else { Subs::Fuses(fuses) }
}

/// The names a `for` value lists, ranges of whole numbers expanded (`1-4, 7`), in order, each
/// once.
///
/// # Errors
/// Too long, a range the wrong way round, more than [`MAX_SUBS`] names, or a name that is too long
/// or holds control characters.
pub fn expand_for(text: &str) -> Result<Vec<String>, CmdError> {
    if text.chars().count() > MAX_FOR_CHARS {
        return Err(CmdError::refused(format!("`for` names at most {MAX_SUBS} positions or fuses.")));
    }
    let mut out: Vec<String> = Vec::new();
    let mut seen = BTreeSet::new();
    let mut push = |name: String, out: &mut Vec<String>| -> Result<(), CmdError> {
        if seen.insert(name.clone()) {
            out.push(name);
        }
        if out.len() > MAX_SUBS { Err(CmdError::refused(format!("`for` names at most {MAX_SUBS} positions or fuses."))) } else { Ok(()) }
    };
    for part in text.split(',') {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        let range = p.split_once('-').and_then(|(a, b)| Some((a.trim().parse::<u64>().ok()?, b.trim().parse::<u64>().ok()?)));
        if let Some((a, b)) = range {
            if a > b {
                return Err(CmdError::refused(format!("\"{p}\" goes the wrong way round. Write the smaller number first.")));
            }
            if b - a >= MAX_SUBS as u64 {
                return Err(CmdError::refused(format!("`for` names at most {MAX_SUBS} positions or fuses.")));
            }
            for i in a..=b {
                push(i.to_string(), &mut out)?;
            }
            continue;
        }
        if p.chars().count() > MAX_SUB_CHARS || p.chars().any(char::is_control) {
            let shown: String = p.chars().take(MAX_SUB_CHARS).collect();
            return Err(CmdError::refused(format!("\"{shown}…\" is not a position or fuse: names are at most {MAX_SUB_CHARS} characters.")));
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
fn check_for(doc: &Document, c: DeviceId, text: &str) -> Result<Option<String>, CmdError> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(None);
    }
    let who = crate::devices::tags::tag_words(doc, c);
    let subs = sub_items(doc, c);
    if subs == Subs::None {
        return Err(CmdError::refused(format!("{who} has nothing to name with for: leave it empty and set qty.")));
    }
    let names = expand_for(t)?;
    for n in &names {
        if !subs.names().contains(n) {
            return Err(CmdError::refused(match &subs {
                Subs::Fuses(f) => format!("{who} has no fuse {n}. Its fuses are {}.", names_words(f)),
                _ => format!("{who} has no position {n}."),
            }));
        }
    }
    Ok(Some(t.to_string()))
}

/// The ops that keep strips' `part.<n>.for` lists on their positions when a renumber plan `ops`
/// relabels their terminals (design 047 §2.3, as 030 FP4 keeps `jumper_bar.<n>`), and the preview lines
/// (`-X05: part line 1 for 1, 5-6 → 1-2, 6`). A list naming no relabelled position is left as
/// typed; names no terminal had stay.
pub(crate) fn renumber_for_ops(doc: &Document, ops: &[Op]) -> (Vec<Op>, Vec<String>) {
    let mut out = Vec::new();
    let mut lines = Vec::new();
    for (c, map) in crate::terminal_strips::accessories::relabels(ops) {
        if crate::terminal_strips::strip_of(doc, c).is_none() {
            continue;
        }
        let Some(props) = doc.devices.get(&c).map(|x| &x.properties) else { continue };
        let tag = crate::devices::tags::tag_words(doc, c);
        for n in used_lines(props) {
            let key = part_key(n, Field::For);
            let Some(before) = props.get(&key) else { continue };
            let Ok(names) = expand_for(before) else { continue };
            if !names.iter().any(|x| map.contains_key(x)) {
                continue;
            }
            let mut moved: Vec<String> = Vec::with_capacity(names.len());
            for name in names {
                let m = map.get(&name).cloned().unwrap_or(name);
                if !moved.contains(&m) {
                    moved.push(m);
                }
            }
            let after = names_words(&moved);
            if after != before.trim() {
                lines.push(format!("{tag}: part line {n} for {} \u{2192} {after}", before.trim()));
                out.push(Op::SetProperty { object: ObjectRef::Device(c), key, before: Some(before.clone()), after: Some(after) });
            }
        }
    }
    (out, lines)
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
    /// Brief 044: the connector half the line belongs to (`part.<n>.half`).
    pub half: Option<crate::symbols::Side>,
}

fn clean(v: Option<impl AsRef<str>>) -> Option<String> {
    v.map(|s| s.as_ref().trim().to_string()).filter(|s| !s.is_empty())
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
                    Field::Half => match v.trim() {
                        "s" | "S" => line.half = Some(crate::symbols::Side::Socket),
                        "p" | "P" => line.half = Some(crate::symbols::Side::Plug),
                        _ => note(k, &mut bad),
                    },
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
pub(crate) fn used_lines(props: &Properties) -> BTreeSet<u8> {
    props
        .range(PART_PREFIX.to_string()..)
        .take_while(|(k, _)| k.starts_with(PART_PREFIX))
        .filter_map(|(k, _)| parse_part_key(k).and_then(Result::ok).map(|(n, _)| n))
        .collect()
}

/// The next free line number: one above the highest, else the lowest gap.
pub fn next_line(used: &BTreeSet<u8>) -> Option<u8> {
    match used.last() {
        None => Some(1),
        Some(m) if *m < MAX_LINES => Some(m + 1),
        Some(_) => (1..=MAX_LINES).find(|n| !used.contains(n)),
    }
}

/// One row of a device's part set as the inspector shows it (design 047 §6.2): line 0 the main
/// part, then the part lines.
#[derive(Debug, Clone, PartialEq)]
pub struct PartSetRow {
    pub n: u8,
    /// `1`, `3`, `2.5 m`; a `for` line's total (per sub-item × names).
    pub qty: String,
    pub maker: Option<String>,
    pub part_number: Option<String>,
    pub description: Option<String>,
    pub article_number: Option<String>,
    /// `kit Starter set`, `for 1,2,4`.
    pub note: Option<String>,
}

/// The part set of `c`, line 0 first; empty when `c` has no part lines (the section is then not
/// shown).
pub fn part_set(doc: &Document, c: DeviceId) -> Vec<PartSetRow> {
    let Some(comp) = doc.devices.get(&c) else { return Vec::new() };
    let (lines, _) = lines(&comp.properties);
    if lines.is_empty() {
        return Vec::new();
    }
    let mut out = vec![PartSetRow {
        n: 0,
        qty: "1".into(),
        maker: clean(comp.manufacturer.as_ref()),
        part_number: clean(comp.part_number.as_ref()),
        description: clean(comp.description.as_ref()),
        article_number: clean(comp.article_number.as_ref()),
        note: None,
    }];
    for l in lines {
        let n_subs = l.subs.len();
        let total = if l.for_text.is_some() { l.qty.checked_mul(n_subs).unwrap_or(l.qty) } else { l.qty };
        let note = match (&l.kit, &l.for_text) {
            (Some(k), Some(f)) => Some(format!("kit {k}, for {f}")),
            (Some(k), None) => Some(format!("kit {k}")),
            (None, Some(f)) => Some(format!("for {f}")),
            (None, None) => None,
        };
        out.push(PartSetRow {
            n: l.n,
            qty: if l.metres { format!("{total} m") } else { total.to_string() },
            maker: l.maker,
            part_number: l.part_number,
            description: l.description,
            article_number: l.article_number,
            note,
        });
    }
    out
}

// ---------------------------------------------------------------- setprop (§2.4)

/// Whether `c` takes part lines: a device or strip (a tag and no `kind`).
pub(crate) fn takes_lines(doc: &Document, c: DeviceId) -> bool {
    crate::locations::is_device(doc, c)
}

/// Whether [`setprop_rule`] has an opinion on `key` of `c`.
pub(crate) fn is_bom_key(key: &str) -> bool {
    key.starts_with(PART_PREFIX)
}

/// The `setprop` rule for 047's keys on component `c` (design §2.4): `part.<n>.*` (the main
/// part's `article_number` is a device field, lane 072 B's row). Returns the value to store.
///
/// # Errors
/// Every refusal of §2.1, §2.3 and §3, in words. Clearing a part line key is always allowed.
pub(crate) fn setprop_rule(doc: &Document, c: DeviceId, key: &str, value: Option<&str>) -> Result<Option<String>, CmdError> {
    let props = doc.devices.get(&c).map(|x| &x.properties);
    let clearing = value.map(str::trim).is_none_or(str::is_empty);
    let Some(parsed) = parse_part_key(key) else { return Ok(value.map(str::to_string)) };
    if clearing {
        return Ok(None);
    }
    let (n, f) = parsed?;
    if !takes_lines(doc, c) {
        return Err(CmdError::refused(PARTLINE_FOR_DEVICES));
    }
    let metres = props.and_then(|p| p.get(&part_key(n, Field::Unit))).is_some_and(|u| parse_unit(u).unwrap_or(false));
    let qty = props.and_then(|p| p.get(&part_key(n, Field::Qty))).map(String::as_str);
    field_value(doc, c, f, value, metres, qty)
}

/// A unit value, checked against the quantity it will apply to.
fn unit_value(qty: Option<&str>, value: Option<&str>) -> Result<Option<String>, CmdError> {
    let metres = parse_unit(value.unwrap_or(""))?;
    if !metres && let Some(q) = qty.and_then(|q| parse_qty(q, true).ok()).filter(|q| !q.whole()) {
        return Err(CmdError::refused(format!("The quantity is {q}: a count of pieces is whole. Set a whole quantity first.")));
    }
    Ok(metres.then(|| METRES.to_string()))
}

/// One field's value for a line of `c` (with the line's unit `metres` and stored `qty`).
fn field_value(doc: &Document, c: DeviceId, f: Field, value: Option<&str>, metres: bool, qty: Option<&str>) -> Result<Option<String>, CmdError> {
    let key = f.word();
    match f {
        Field::PartNumber | Field::Maker | Field::Description => crate::devices::json::property_value(doc, key, value),
        Field::ArticleNumber => plain(key, value, MAX_ARTICLE_CHARS),
        Field::Kit => plain(key, value, MAX_KIT_CHARS),
        Field::Qty => match value.map(str::trim).filter(|v| !v.is_empty()) {
            None => Ok(None),
            Some(v) => parse_qty(v, metres).map(|_| Some(v.to_string())),
        },
        Field::Unit => unit_value(qty, value),
        Field::For => check_for(doc, c, value.unwrap_or("")),
        // Brief 044 (design 044 §8.3): only on a connector, `s` or `p`.
        Field::Half => match value.map(str::trim).filter(|v| !v.is_empty()) {
            None => Ok(None),
            Some(_) if !crate::connectors::connector_of(doc, c).is_some_and(|x| x.mating) => Err(CmdError::refused(format!(
                "`half` is for a connector's part lines; {} is not a connector.",
                crate::devices::tags::tag_words(doc, c)
            ))),
            Some(v) if v.eq_ignore_ascii_case("s") || v.eq_ignore_ascii_case("p") => Ok(Some(v.to_ascii_lowercase())),
            Some(_) => Err(CmdError::refused("`half` is s (the socket half) or p (the plug half).")),
        },
    }
}

// ---------------------------------------------------------------- defaults (§8)

/// Whether the BOM shows its Article number column (`bom.column.article`).
pub(crate) fn article_column(doc: &Document) -> bool {
    ecad_model::naming::words::flag(defaults::resolve(doc, &defaults::bom::COLUMN_ARTICLE, Scope::project()).word()) == Some(true)
}

/// Whether part lines are listed under their device (`bom.lines = nested`).
pub(crate) fn lines_nested(doc: &Document) -> bool {
    defaults::resolve(doc, &defaults::bom::LINES, Scope::project()).choice() == Some(defaults::bom::BomLines::Nested)
}

/// Whether a new item's unit is metres (`bom.item.unit = m`).
pub fn item_unit_metres(doc: &Document) -> bool {
    use ecad_model::objects::loose_part::ItemUnit;
    defaults::resolve(doc, &defaults::bom::ITEM_UNIT, Scope::project()).choice::<ItemUnit>() == Some(ItemUnit::Metres)
}

/// The ops writing `fields` on line `n` of `c`, every value checked.
pub(crate) fn line_ops(doc: &Document, c: DeviceId, n: u8, fields: &[(Field, Option<String>)]) -> Result<Vec<Op>, CmdError> {
    let props = doc.devices.get(&c).map(|x| &x.properties).ok_or(CmdError::refused("no such component"))?;
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
            ops.push(Op::SetProperty { object: ObjectRef::Device(c), key, before, after });
        }
    }
    Ok(ops)
}

/// `Slotted trunking 50x75, 3.6 m in +CP1` (brief 052: the location's code from the list).
pub(crate) fn item_words(doc: &Document, item: &LoosePart) -> String {
    let unit = if item.unit == ecad_model::objects::loose_part::ItemUnit::Metres { " m" } else { "" };
    let (d, q) = (item.description.as_str(), item.qty);
    match item.location.map(|l| crate::locations::designations::display(doc, "location", &l.hex())) {
        Some(l) => format!("{d}, {q}{unit} in {l}"),
        None => format!("{d}, {q}{unit}"),
    }
}

/// Whether a stored value equals `want` (trimmed, case-insensitive).
fn same(v: Option<impl AsRef<str>>, want: &str) -> bool {
    v.is_some_and(|x| x.as_ref().trim().eq_ignore_ascii_case(want))
}

/// Where `article` writes one article number.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ArticleTarget {
    /// A key of an object (a device's field or part line, a strip's part, the project's strip
    /// default) and what it is (`device`, `line`, `strip`).
    Key(ObjectRef, String, &'static str),
    /// An item: its `article_number`.
    Item(LoosePartId),
}

impl ArticleTarget {
    /// What it is, in words: `device`, `line`, `strip` or `item`.
    pub fn what(&self) -> &'static str {
        match self {
            ArticleTarget::Key(_, _, w) => w,
            ArticleTarget::Item(_) => "item",
        }
    }
}

/// The article keys `article` writes for part number `pn` (and maker `maker`) on `doc`. A strip's
/// terminal, accessory and jumper parts are written at the level their part number is stored at
/// (design 047 §5): the strip's own key, or the project's `strip_<x>_article_number`.
pub(crate) fn article_targets(doc: &Document, pn: &str, maker: Option<&str>) -> Vec<ArticleTarget> {
    let mut out = Vec::new();
    for (id, p) in &doc.loose_parts {
        let maker_ok = maker.is_none_or(|m| same(p.manufacturer.as_ref(), m));
        if same(p.part_number.as_ref(), pn) && maker_ok {
            out.push(ArticleTarget::Item(*id));
        }
    }
    for (c, comp) in &doc.devices {
        let props = &comp.properties;
        if comp.tag.is_none() {
            continue;
        }
        let maker_ok = |k: &str| maker.is_none_or(|m| same(comp.get(k), m));
        let strip = crate::terminal_strips::strip_of(doc, *c).is_some();
        if same(comp.part_number.as_ref(), pn) && maker_ok(ecad_model::objects::devices::MAKER_KEY) && !strip {
            out.push(ArticleTarget::Key(ObjectRef::Device(*c), ARTICLE_KEY.to_string(), "device"));
        }
        if strip {
            for (part, mk, article, project_maker) in strip_slots(props) {
                // The maker as the BOM reads it: the strip's, else the project's.
                let maker_here =
                    comp.get(&mk).filter(|v| !v.trim().is_empty()).or_else(|| project_maker.and_then(|k| doc.display.get(&k)).map(String::as_str));
                if same(comp.get(&part), pn) && maker.is_none_or(|m| same(maker_here, m)) {
                    out.push(ArticleTarget::Key(ObjectRef::Device(*c), article, "strip"));
                }
            }
        }
        for n in used_lines(props) {
            if same(props.get(&part_key(n, Field::PartNumber)), pn) && maker_ok(&part_key(n, Field::Maker)) {
                out.push(ArticleTarget::Key(ObjectRef::Device(*c), part_key(n, Field::ArticleNumber), "line"));
            }
        }
    }
    // The project's strip part defaults (`strip_<code>_part_number`, `strip_<item>_part_number`).
    for (part, mk, article) in project_strip_slots() {
        if same(doc.display.get(&part), pn) && maker.is_none_or(|m| same(doc.display.get(&mk), m)) {
            out.push(ArticleTarget::Key(ObjectRef::Project, article, "strip"));
        }
    }
    out
}

/// A strip's own part slots: (part number key, maker key, article key, the project's maker key)
/// of each terminal type (036 §7.2), each accessory (030 §4) and each link's own part (030 §5.6,
/// which has no project default of its own).
fn strip_slots(props: &Properties) -> Vec<(String, String, String, Option<String>)> {
    let mut v: Vec<(String, String, String, Option<String>)> = crate::terminal_strips::terminal_blocks::TYPES
        .iter()
        .map(|t| {
            let k = |f: &str| crate::terminal_strips::strip_model::type_key(t, f);
            (k("part_number"), k("maker"), k(ARTICLE_KEY), Some(format!("strip_{}_maker", t.code)))
        })
        .collect();
    let item_keys = |pre: &str| (format!("{pre}.part_number"), format!("{pre}.maker"), format!("{pre}.{ARTICLE_KEY}"));
    v.extend(crate::terminal_strips::accessories::PART_ITEMS.iter().map(|i| {
        let (p, m, a) = item_keys(i);
        let pm = crate::terminal_strips::accessories::project_key(&m);
        (p, m, a, Some(pm))
    }));
    v.extend(ecad_model::connectivity::jumper_bars::of(props).into_iter().map(|(n, _)| {
        let (p, m, a) = item_keys(&ecad_model::connectivity::jumper_bars::key(n));
        (p, m, a, None)
    }));
    v
}

/// The project's strip part slots: (part number key, maker key, article key), terminal types as
/// `strip_<code>_*` and accessories as `strip_<item>_*`.
fn project_strip_slots() -> Vec<(String, String, String)> {
    let mut v: Vec<(String, String, String)> = crate::terminal_strips::terminal_blocks::TYPES
        .iter()
        .map(|t| (format!("strip_{}_part_number", t.code), format!("strip_{}_maker", t.code), format!("strip_{}_{ARTICLE_KEY}", t.code)))
        .collect();
    let pk = crate::terminal_strips::accessories::project_key;
    v.extend(
        crate::terminal_strips::accessories::PART_ITEMS
            .iter()
            .map(|i| (pk(&format!("{i}.part_number")), pk(&format!("{i}.maker")), pk(&format!("{i}.{ARTICLE_KEY}")))),
    );
    v
}
