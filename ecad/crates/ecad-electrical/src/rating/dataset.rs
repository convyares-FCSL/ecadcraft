//! Rating tables: a user's data set of ratings and factors (design 057 §5; owner, Q11).
//!
//! **ECAD ships no values.** This module holds the row structure, the CSV parser with its bounds,
//! the lookups and the plausibility checks. The rows come from the user (Standards ▸ Rating tables,
//! or `ratingtable {csv}`), from a source they are licensed to use. The only rows in the repository
//! are synthetic test fixtures under `tests/`.
//!
//! # Rows (CSV, design 057 §5.2)
//! One row per line, comma-separated; a header line starting `kind,` is skipped; a line starting
//! `#` is a comment. Kinds: `capacity` (method, insulation, loaded, size, amps), `ambient`
//! (insulation, °C, factor), `grouping` (method, circuits, factor), `resistance` (class, size,
//! Ω/km at 20 °C). Method names are IEC 60364-5-52's (`A1 A2 B1 B2 C D1 D2 E F G`), insulation
//! `pvc`, `xlpe` or a θmax number 50–250.
//!
//! # Bounds
//! At most [`MAX_BYTES`] of text and [`MAX_ROWS`] rows (checked before any row is read), fields of
//! at most [`MAX_FIELD`] characters, no indexing.

use crate::CmdError;
use std::collections::BTreeMap;

/// The most text a rating table holds (256 KiB).
pub const MAX_BYTES: usize = 256 * 1024;
/// The most rows a rating table holds.
pub const MAX_ROWS: usize = 4_000;
/// The longest field read.
pub const MAX_FIELD: usize = 32;
/// The most bad lines an import refusal lists.
pub const MAX_LISTED: usize = 10;

/// IEC 60364-5-52's installation method names (Q17).
pub const METHODS: [&str; 10] = ["A1", "A2", "B1", "B2", "C", "D1", "D2", "E", "F", "G"];

/// Words for a method, for the drop-down (the description is ECAD's own, short).
pub fn method_words(m: &str) -> &'static str {
    match m {
        "A1" => "insulated conductors in conduit in an insulated wall",
        "A2" => "multicore in conduit in an insulated wall",
        "B1" => "insulated conductors in conduit or trunking on a wall",
        "B2" => "multicore in conduit on a wall",
        "C" => "clipped direct",
        "D1" => "in ducts in the ground",
        "D2" => "direct in the ground",
        "E" => "perforated tray",
        "F" => "single-core cables on a tray, touching",
        "G" => "single-core cables, spaced",
        _ => "",
    }
}

/// The method name `text` gives, in its canonical spelling.
pub fn method(text: &str) -> Option<&'static str> {
    let t = text.trim();
    METHODS.iter().copied().find(|m| m.eq_ignore_ascii_case(t))
}

/// An insulation column: `pvc` (70 °C), `xlpe` (90 °C) or another θmax in °C.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Insulation {
    Pvc,
    Xlpe,
    /// θmax in whole °C, 50–250.
    Other(u16),
}

impl Insulation {
    /// The column a conductor of maximum temperature `theta_max` °C reads (design 057 §3.2).
    pub fn of_max(theta_max: f64) -> Option<Self> {
        if !theta_max.is_finite() {
            return None;
        }
        if (theta_max - 70.0).abs() < 1e-9 {
            return Some(Insulation::Pvc);
        }
        if (theta_max - 90.0).abs() < 1e-9 {
            return Some(Insulation::Xlpe);
        }
        let r = theta_max.round();
        ((50.0..=250.0).contains(&r) && (theta_max - r).abs() < 1e-9).then_some(Insulation::Other(r as u16))
    }

    pub fn parse(text: &str) -> Option<Self> {
        let t = text.trim();
        if t.eq_ignore_ascii_case("pvc") {
            return Some(Insulation::Pvc);
        }
        if t.eq_ignore_ascii_case("xlpe") {
            return Some(Insulation::Xlpe);
        }
        if t.len() <= 3 && ecad_model::words::is_digits(t) {
            return t.parse::<u16>().ok().filter(|n| (50..=250).contains(n)).and_then(|n| Insulation::of_max(f64::from(n)));
        }
        None
    }

    /// Its word in a row and a message.
    pub fn word(self) -> String {
        match self {
            Insulation::Pvc => "pvc".into(),
            Insulation::Xlpe => "xlpe".into(),
            Insulation::Other(n) => n.to_string(),
        }
    }
}

/// A size or temperature as a key: hundredths, so `1.5` and `1.50` are one key.
fn key_of(x: f64) -> i64 {
    (x * 100.0).round() as i64
}

/// One row's key (a repeated key is refused).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RowKey {
    Capacity { method: &'static str, insulation: Insulation, loaded: u8, size: i64 },
    Ambient { insulation: Insulation, temp: i64 },
    Grouping { method: &'static str, circuits: u32 },
    Resistance { class: u8, size: i64 },
}

impl RowKey {
    /// In words, as a refusal names it: `capacity E pvc 3 1.5`.
    pub fn words(&self) -> String {
        let n = |k: i64| fmt_num(k as f64 / 100.0);
        match self {
            RowKey::Capacity { method, insulation, loaded, size } => format!("capacity {method} {} {loaded} {}", insulation.word(), n(*size)),
            RowKey::Ambient { insulation, temp } => format!("ambient {} {}", insulation.word(), n(*temp)),
            RowKey::Grouping { method, circuits } => format!("grouping {method} {circuits}"),
            RowKey::Resistance { class, size } => format!("resistance {class} {}", n(*size)),
        }
    }
}

/// A number as a person writes it: no trailing zeros (`1.5`, `20`, `0.94`).
pub fn fmt_num(x: f64) -> String {
    let s = format!("{x:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" { "0".into() } else { s.to_string() }
}

/// A rating table read: its rows by key, and the lines it could not read.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Dataset {
    pub rows: BTreeMap<RowKey, f64>,
    /// Lines not read: (line number, the reason in words).
    pub bad: Vec<(usize, String)>,
    /// How many bad lines in all (`bad` lists at most [`MAX_LISTED`]).
    pub bad_count: usize,
    /// The text was over [`MAX_BYTES`] or [`MAX_ROWS`]: nothing was read.
    pub too_big: Option<String>,
}

fn num(field: Option<&str>, what: &str, min: f64, max: f64) -> Result<f64, CmdError> {
    let f = field.map(str::trim).filter(|f| !f.is_empty()).ok_or_else(|| CmdError::refused(format!("{what} missing")))?;
    if f.chars().nth(MAX_FIELD).is_some() {
        return Err(CmdError::refused(format!("{what} too long")));
    }
    let ok = !f.is_empty() && f.bytes().all(|b| matches!(b, b'0'..=b'9' | b'.')) && f.bytes().filter(|b| *b == b'.').count() <= 1;
    let x = if ok { f.parse::<f64>().ok() } else { None };
    let shown: String = f.chars().take(12).collect();
    match x {
        Some(x) if x.is_finite() && (min..=max).contains(&x) => Ok(x),
        Some(_) => Err(CmdError::refused(format!("{what} '{shown}' is outside {}–{}", fmt_num(min), fmt_num(max)))),
        None => Err(CmdError::refused(format!("{what} '{shown}'"))),
    }
}

fn field<'a>(fields: &[&'a str], i: usize) -> Option<&'a str> {
    fields.get(i).copied()
}

/// Read one data line (not a comment or header).
fn row(line: &str) -> Result<(RowKey, f64), CmdError> {
    let fields: Vec<&str> = line.split(',').take(8).collect();
    let kind = field(&fields, 0).map(str::trim).unwrap_or_default().to_ascii_lowercase();
    let want = |n: usize| -> Result<(), CmdError> {
        let have = line.split(',').take(n + 2).count();
        if have == n { Ok(()) } else { Err(CmdError::refused(format!("{kind} takes {n} columns, not {have}"))) }
    };
    let method_at = |i: usize| -> Result<&'static str, CmdError> {
        let t = field(&fields, i).unwrap_or_default();
        method(t)
            .ok_or_else(|| CmdError::refused(format!("method '{}' is not an IEC 60364-5-52 name", t.trim().chars().take(12).collect::<String>())))
    };
    let insulation_at = |i: usize| -> Result<Insulation, CmdError> {
        let t = field(&fields, i).unwrap_or_default();
        Insulation::parse(t)
            .ok_or_else(|| CmdError::refused(format!("insulation '{}' (pvc, xlpe or 50–250)", t.trim().chars().take(12).collect::<String>())))
    };
    match kind.as_str() {
        "capacity" => {
            want(6)?;
            let method = method_at(1)?;
            let insulation = insulation_at(2)?;
            let loaded = num(field(&fields, 3), "loaded", 2.0, 3.0)?;
            if loaded.fract() != 0.0 {
                return Err(CmdError::refused("loaded is 2 or 3"));
            }
            let size = num(field(&fields, 4), "size", 0.05, 1000.0)?;
            let amps = num(field(&fields, 5), "amps", 0.1, 6300.0)?;
            Ok((RowKey::Capacity { method, insulation, loaded: loaded as u8, size: key_of(size) }, amps))
        }
        "ambient" => {
            want(4)?;
            let insulation = insulation_at(1)?;
            let t = field(&fields, 2).map(str::trim).unwrap_or_default();
            let temp = match t.strip_prefix('-') {
                Some(rest) => -num(Some(rest), "temperature", 0.0, 40.0)?,
                None => num(Some(t), "temperature", 0.0, 90.0)?,
            };
            let factor = num(field(&fields, 3), "factor", 0.01, 2.0)?;
            Ok((RowKey::Ambient { insulation, temp: key_of(temp) }, factor))
        }
        "grouping" => {
            want(4)?;
            let method = method_at(1)?;
            let circuits = num(field(&fields, 2), "circuits", 1.0, 100.0)?;
            if circuits.fract() != 0.0 {
                return Err(CmdError::refused("circuits is a whole number"));
            }
            let factor = num(field(&fields, 3), "factor", 0.01, 2.0)?;
            Ok((RowKey::Grouping { method, circuits: circuits as u32 }, factor))
        }
        "resistance" => {
            want(4)?;
            let class = field(&fields, 1).map(str::trim).unwrap_or_default();
            let class: u8 = match class {
                "1" => 1,
                "2" => 2,
                "5" => 5,
                "6" => 6,
                other => return Err(CmdError::refused(format!("class '{}' (1, 2, 5 or 6)", other.chars().take(12).collect::<String>()))),
            };
            let size = num(field(&fields, 2), "size", 0.05, 1000.0)?;
            let ohms = num(field(&fields, 3), "resistance", 0.001, 1000.0)?;
            Ok((RowKey::Resistance { class, size: key_of(size) }, ohms))
        }
        "" => Err(CmdError::refused("empty kind")),
        other => Err(CmdError::refused(format!("kind '{}' (capacity, ambient, grouping or resistance)", other.chars().take(12).collect::<String>()))),
    }
}

/// Read a rating table's text. Never fails: bad lines are listed in [`Dataset::bad`] (the first
/// [`MAX_LISTED`]) and counted; text over the bounds reads as nothing, with
/// [`Dataset::too_big`] saying why.
pub fn parse(text: &str) -> Dataset {
    let mut d = Dataset::default();
    if text.len() > MAX_BYTES {
        d.too_big = Some(format!("The table is over {} KiB.", MAX_BYTES / 1024));
        return d;
    }
    let data_lines = text.lines().filter(|l| is_data(l)).take(MAX_ROWS + 1).count();
    if data_lines > MAX_ROWS {
        d.too_big = Some(format!("The table has more than {MAX_ROWS} rows."));
        return d;
    }
    for (i, line) in text.lines().enumerate() {
        if !is_data(line) {
            continue;
        }
        let n = i + 1;
        let mut bad = |why: String| {
            d.bad_count += 1;
            if d.bad.len() < MAX_LISTED {
                d.bad.push((n, why));
            }
        };
        match row(line) {
            Ok((k, v)) => match d.rows.entry(k) {
                std::collections::btree_map::Entry::Occupied(e) => bad(format!("repeats {}", e.key().words())),
                std::collections::btree_map::Entry::Vacant(e) => {
                    e.insert(v);
                }
            },
            Err(e) => bad(e.to_string()),
        }
    }
    d
}

fn is_data(line: &str) -> bool {
    let t = line.trim();
    !(t.is_empty() || t.starts_with('#') || t.to_ascii_lowercase().starts_with("kind,"))
}

/// Read a table for import (design 057 §5.2): the whole file is refused if any row is bad.
///
/// # Errors
/// In words: the bound it breaks, or the first [`MAX_LISTED`] bad lines with their numbers.
pub fn parse_strict(text: &str) -> Result<Dataset, CmdError> {
    let d = parse(text);
    if let Some(why) = &d.too_big {
        return Err(CmdError::refused(format!("{why} Nothing was written.")));
    }
    if d.bad_count > 0 {
        let lines: Vec<String> = d.bad.iter().map(|(n, why)| format!("line {n}: {why}")).collect();
        let more = if d.bad_count > d.bad.len() { format!(" and {} more", d.bad_count - d.bad.len()) } else { String::new() };
        return Err(CmdError::refused(format!(
            "{} row{} not read ({}{more}). Nothing was written.",
            d.bad_count,
            if d.bad_count == 1 { "" } else { "s" },
            lines.join("; ")
        )));
    }
    Ok(d)
}

/// The rows as CSV, one per line, in key order, with a header: what Export writes.
pub fn to_csv(d: &Dataset) -> String {
    let mut out = String::from("kind,a,b,c,d,value\n");
    for (k, v) in &d.rows {
        let n = |k: i64| fmt_num(k as f64 / 100.0);
        let line = match k {
            RowKey::Capacity { method, insulation, loaded, size } => {
                format!("capacity,{method},{},{loaded},{},{}", insulation.word(), n(*size), fmt_num(*v))
            }
            RowKey::Ambient { insulation, temp } => format!("ambient,{},{},{}", insulation.word(), n(*temp), fmt_num(*v)),
            RowKey::Grouping { method, circuits } => format!("grouping,{method},{circuits},{}", fmt_num(*v)),
            RowKey::Resistance { class, size } => format!("resistance,{class},{},{}", n(*size), fmt_num(*v)),
        };
        out.push_str(&line);
        out.push('\n');
    }
    out
}

impl Dataset {
    /// The capacity row for these, if any.
    pub fn capacity(&self, method: &'static str, insulation: Insulation, loaded: u8, size: f64) -> Option<f64> {
        self.rows.get(&RowKey::Capacity { method, insulation, loaded, size: key_of(size) }).copied()
    }

    /// Whether the table has any ambient row for `insulation`.
    pub fn has_ambient(&self, insulation: Insulation) -> bool {
        self.rows.keys().any(|k| matches!(k, RowKey::Ambient { insulation: i, .. } if *i == insulation))
    }

    /// The ambient factor at `theta`: the row at the next higher (or equal) listed temperature
    /// (design 057 §3.2), with that row's temperature; `None` above the last row.
    pub fn ambient(&self, insulation: Insulation, theta: f64) -> Option<(f64, f64)> {
        let want = key_of(theta);
        self.rows
            .iter()
            .filter_map(|(k, v)| match k {
                RowKey::Ambient { insulation: i, temp } if *i == insulation && *temp >= want => Some((*temp, *v)),
                _ => None,
            })
            .min_by_key(|(t, _)| *t)
            .map(|(t, v)| (v, t as f64 / 100.0))
    }

    /// The grouping factor for `n` circuits in `method`: 1.00 at one circuit without a row, else
    /// the row at the next higher (or equal) listed count, with that count.
    pub fn grouping(&self, method: &str, n: u32) -> Option<(f64, u32)> {
        if n <= 1 {
            return Some((1.0, 1));
        }
        self.rows
            .iter()
            .filter_map(|(k, v)| match k {
                RowKey::Grouping { method: m, circuits } if *m == method && *circuits >= n => Some((*circuits, *v)),
                _ => None,
            })
            .min_by_key(|(c, _)| *c)
            .map(|(c, v)| (v, c))
    }

    /// R20 for this class and size, Ω/km.
    pub fn resistance(&self, class: u8, size: f64) -> Option<f64> {
        self.rows.get(&RowKey::Resistance { class, size: key_of(size) }).copied()
    }

    /// The sizes of a class with a resistance row, ascending (the volt-drop suggestion).
    pub fn resistance_sizes(&self, class: u8) -> Vec<f64> {
        self.rows
            .keys()
            .filter_map(|k| match k {
                RowKey::Resistance { class: c, size } if *c == class => Some(*size as f64 / 100.0),
                _ => None,
            })
            .collect()
    }

    /// The sizes with a capacity row for these, ascending (the Use-a-larger-size fix).
    pub fn capacity_sizes(&self, method: &str, insulation: Insulation, loaded: u8) -> Vec<(f64, f64)> {
        self.rows
            .iter()
            .filter_map(|(k, v)| match k {
                RowKey::Capacity { method: m, insulation: i, loaded: l, size } if *m == method && *i == insulation && *l == loaded => {
                    Some((*size as f64 / 100.0, *v))
                }
                _ => None,
            })
            .collect()
    }

    /// The methods with capacity rows (the Method drop-down).
    pub fn methods(&self) -> Vec<&'static str> {
        let mut v: Vec<&'static str> =
            self.rows.keys().filter_map(|k| if let RowKey::Capacity { method, .. } = k { Some(*method) } else { None }).collect();
        v.sort_by_key(|m| METHODS.iter().position(|x| x == m));
        v.dedup();
        v
    }

    /// The first implausible pair (design 057 §5.4), in words, or `None`.
    pub fn odd(&self) -> Option<String> {
        let mut prev: Option<(&RowKey, f64)> = None;
        // Capacity rises with size within one method, insulation and loaded count.
        for (k, v) in &self.rows {
            if let RowKey::Capacity { method, insulation, loaded, size } = k {
                if let Some((RowKey::Capacity { method: m, insulation: i, loaded: l, size: s }, pv)) = prev
                    && m == method
                    && i == insulation
                    && l == loaded
                    && *v < pv
                {
                    return Some(format!(
                        "capacity at {} mm\u{b2} is below {} mm\u{b2} ({method}, {}, {loaded})",
                        fmt_num(*size as f64 / 100.0),
                        fmt_num(*s as f64 / 100.0),
                        insulation.word()
                    ));
                }
                prev = Some((k, *v));
                // A 3-loaded capacity is not above its 2-loaded one.
                if *loaded == 3
                    && let Some(two) = self.rows.get(&RowKey::Capacity { method, insulation: *insulation, loaded: 2, size: *size })
                    && *v > *two
                {
                    return Some(format!(
                        "capacity with 3 loaded is above 2 loaded at {} mm\u{b2} ({method}, {})",
                        fmt_num(*size as f64 / 100.0),
                        insulation.word()
                    ));
                }
            }
        }
        // Grouping factors do not rise with the count; resistance falls as size rises.
        let mut prev: Option<(&RowKey, f64)> = None;
        for (k, v) in &self.rows {
            match (k, prev) {
                (RowKey::Grouping { method, circuits }, Some((RowKey::Grouping { method: m, circuits: c }, pv))) if m == method && *v > pv => {
                    return Some(format!("grouping factor at {circuits} circuits is above {c} circuits ({method})"));
                }
                (RowKey::Resistance { class, size }, Some((RowKey::Resistance { class: c, size: s }, pv))) if c == class && *v >= pv => {
                    return Some(format!(
                        "resistance at {} mm\u{b2} is not below {} mm\u{b2} (class {class})",
                        fmt_num(*size as f64 / 100.0),
                        fmt_num(*s as f64 / 100.0)
                    ));
                }
                _ => {}
            }
            prev = Some((k, *v));
        }
        None
    }
}
