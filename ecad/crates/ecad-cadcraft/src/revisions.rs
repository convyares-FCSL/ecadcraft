//! Project revisions (brief 019, design 019 §7.2, decision FP4): a list of text fields kept as
//! project properties `rev.<n>.<field>`, `n` = 1, 2… in issue order.
//!
//! # Semantics
//! - Fields: [`FIELDS`] and `issued` (the version name, set only by issuing).
//! - The row with the highest `n` and no `issued` is the **pending** revision; issued rows never
//!   change (the commands refuse it), except that undoing an issue removes its `issued` key.
//! - Issuing writes the pending row's remaining fields, `issued = "Rev <rev>"` and
//!   `NameVersion { "Rev <rev>", head }` in one transaction, so `Ledger::state_at(at)` reopens the
//!   project exactly as it was issued.
//!
//! This module reads and plans; the commands commit (`crate::sheet_cmds`).

use ecad_model::{Document, ObjectRef, Op};

/// The editable fields of a revision, in table order.
pub const FIELDS: [&str; 6] = ["rev", "date", "description", "drawn", "checked", "approved"];
/// The key prefix of revision properties.
pub const PREFIX: &str = "rev.";
/// The field set by issuing.
pub const ISSUED: &str = "issued";
/// The field set by issuing through a release (brief 027, FP1): the release folder, relative to
/// the project file's folder (absolute on another drive). Unknown to older builds, which skip it.
pub const RELEASED: &str = "released";
/// Most revisions a project may hold (a bound on hostile files).
pub const MAX_REVISIONS: u32 = 10_000;
/// Rev text: 1 to 8 characters `0-9 A-Z a-z`.
pub const MAX_REV_CHARS: usize = 8;
/// The fields issuing needs (owner answer Q7).
pub const NEEDED: [&str; 3] = ["rev", "description", "drawn"];
/// Said when a needed field is empty.
pub const FILL_FIRST: &str = "Fill in Rev, Description and Drawn first.";

/// One revision row.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Revision {
    pub n: u32,
    /// The fields of [`FIELDS`], in that order (empty when unset).
    pub fields: [String; 6],
    /// The version name, once issued.
    pub issued: Option<String>,
    /// The release folder, when it was issued by a release (FP1).
    pub released: Option<String>,
}

impl Revision {
    /// The value of field `name` (empty when unset or unknown).
    pub fn get(&self, name: &str) -> &str {
        FIELDS.iter().position(|f| *f == name).and_then(|i| self.fields.get(i)).map_or("", String::as_str)
    }

    pub fn rev(&self) -> &str {
        self.get("rev")
    }

    /// The key of `field` for this row.
    pub fn key(&self, field: &str) -> String {
        key(self.n, field)
    }
}

/// `rev.<n>.<field>`.
pub fn key(n: u32, field: &str) -> String {
    format!("{PREFIX}{n}.{field}")
}

/// Every revision row, by `n` ascending. Reads only the `rev.` properties, once each: linear in
/// their number.
pub fn list(doc: &Document) -> Vec<Revision> {
    let mut rows: std::collections::BTreeMap<u32, Revision> = std::collections::BTreeMap::new();
    for (k, v) in doc.properties.range(PREFIX.to_string()..) {
        let Some(rest) = k.strip_prefix(PREFIX) else { break };
        let Some((n, field)) = rest.split_once('.') else { continue };
        let Some(n) = n.parse::<u32>().ok().filter(|n| (1..=MAX_REVISIONS).contains(n)) else { continue };
        let row = rows.entry(n).or_insert_with(|| Revision { n, ..Revision::default() });
        if field == ISSUED {
            row.issued = Some(v.clone());
        } else if field == RELEASED {
            row.released = Some(v.clone());
        } else if let Some(slot) = FIELDS.iter().position(|f| *f == field).and_then(|j| row.fields.get_mut(j)) {
            *slot = v.clone();
        }
    }
    rows.into_values().collect()
}

/// The issued revisions, newest first.
pub fn issued(doc: &Document) -> Vec<Revision> {
    let mut v: Vec<Revision> = list(doc).into_iter().filter(|r| r.issued.is_some()).collect();
    v.reverse();
    v
}

/// The pending revision: the highest row when it is not issued.
pub fn pending(doc: &Document) -> Option<Revision> {
    list(doc).into_iter().last().filter(|r| r.issued.is_none())
}

/// The `n` the pending row has or would get.
pub fn pending_n(doc: &Document) -> u32 {
    let all = list(doc);
    match all.last() {
        Some(r) if r.issued.is_none() => r.n,
        Some(r) => r.n.saturating_add(1),
        None => 1,
    }
}

/// The rev suggested after `last` (design 019 §7.2): `A`→`B`, `Z`→`AA`, `0`→`1`, `P3`→`P4`;
/// none: `A`.
pub fn next_rev(last: Option<&str>) -> String {
    let Some(last) = last.map(str::trim).filter(|s| !s.is_empty()) else { return "A".into() };
    let digits = last.len() - last.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    if digits > 0 {
        let (head, num) = last.split_at(last.len() - digits);
        if let Ok(n) = num.parse::<u64>() {
            return format!("{head}{}", n.saturating_add(1));
        }
    }
    if last.chars().all(|c| c.is_ascii_alphabetic()) {
        let upper = last.chars().all(|c| c.is_ascii_uppercase());
        let mut chars: Vec<u8> = last.to_ascii_uppercase().into_bytes();
        let mut i = chars.len();
        loop {
            if i == 0 {
                chars.insert(0, b'A');
                break;
            }
            i -= 1;
            match chars.get_mut(i) {
                Some(c) if *c == b'Z' => *c = b'A',
                Some(c) => {
                    *c += 1;
                    break;
                }
                None => break,
            }
        }
        let s = String::from_utf8(chars).unwrap_or_else(|_| "A".into());
        return if upper { s } else { s.to_ascii_lowercase() };
    }
    format!("{last}1")
}

/// The rev the pending row shows: its own, else the suggestion.
pub fn pending_rev(doc: &Document) -> String {
    match pending(doc) {
        Some(p) if !p.rev().is_empty() => p.rev().to_string(),
        _ => next_rev(issued(doc).first().map(Revision::rev)),
    }
}

/// Check a rev text.
///
/// # Errors
/// In words.
pub fn check_rev(doc: &Document, rev: &str) -> Result<(), String> {
    let n = rev.chars().count();
    if !(1..=MAX_REV_CHARS).contains(&n) || !rev.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err("Rev is 1 to 8 letters or digits.".into());
    }
    if issued(doc).iter().any(|r| r.rev().eq_ignore_ascii_case(rev)) {
        return Err(format!("Rev {rev} was issued before; choose another."));
    }
    Ok(())
}

/// The ops that set the pending row's `field` to `value` (`None` clears), creating the row on its
/// first field. Nothing when it already holds that value.
pub fn set_pending_ops(doc: &Document, field: &str, value: Option<String>) -> Vec<Op> {
    let n = pending_n(doc);
    let k = key(n, field);
    let before = doc.properties.get(&k).cloned();
    if before == value {
        return Vec::new();
    }
    vec![Op::SetProperty { object: ObjectRef::Project, key: k, before, after: value }]
}

/// The version name of an issued rev.
pub fn version_name(rev: &str) -> String {
    format!("Rev {rev}")
}

/// The ops that undo `ops`.
pub fn inverse(ops: &[Op]) -> Vec<Op> {
    ops.iter().rev().map(Op::inverse).collect()
}

/// When the transaction `ops` issues a revision: the rev it issued and its row number.
pub fn issue_in(ops: &[Op]) -> Option<(String, u32)> {
    ops.iter().find_map(|op| {
        let Op::SetProperty { object: ObjectRef::Project, key, after: Some(v), .. } = op else { return None };
        let (n, field) = key.strip_prefix(PREFIX)?.split_once('.')?;
        if field != ISSUED {
            return None;
        }
        Some((v.strip_prefix("Rev ").unwrap_or(v).to_string(), n.parse().ok()?))
    })
}

/// Today's date `YYYY-MM-DD` (UTC; the trial fills the local date in the dialog), or `None`
/// where there is no clock.
pub fn today() -> Option<String> {
    date_of_ms(crate::project::now_ms()?)
}

/// The UTC date `YYYY-MM-DD` of `ms` milliseconds since 1970-01-01, through `chrono` (house rule);
/// `None` beyond what it represents.
pub fn date_of_ms(ms: u64) -> Option<String> {
    let t = chrono::DateTime::from_timestamp_millis(i64::try_from(ms).ok()?)?;
    Some(t.format("%Y-%m-%d").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_revs() {
        for (last, next) in [(None, "A"), (Some("A"), "B"), (Some("Z"), "AA"), (Some("AZ"), "BA"), (Some("0"), "1"), (Some("P3"), "P4")] {
            assert_eq!(next_rev(last), next, "{last:?}");
        }
    }

    #[test]
    fn dates_go_through_chrono() {
        for (ms, date) in [(0, "1970-01-01"), (951_782_400_000, "2000-02-29"), (1_794_441_600_000, "2026-11-12"), (4_107_542_399_999, "2100-02-28")] {
            assert_eq!(date_of_ms(ms).as_deref(), Some(date), "{ms}");
        }
        assert_eq!(date_of_ms(u64::MAX), None);
    }
}
