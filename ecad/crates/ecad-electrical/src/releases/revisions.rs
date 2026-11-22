//! Project revisions (brief 019, design 019 §7.2, decision FP4; design 072 §5.3): the project's
//! revision rows ([`ecad_model::Revision`]), `n` = 1, 2… in issue order, changed through
//! `ModifyProject`.
//!
//! # Semantics
//! - Fields: [`FIELDS`] and `issued` (the version name, set only by issuing).
//! - The row with the highest `n` and no `issued` is the **pending** revision; issued rows never
//!   change (the commands refuse it), except that undoing an issue removes its `issued` key.
//! - Issuing writes the pending row's remaining fields, `issued = "Rev <rev>"` (`ModifyProject`)
//!   and `NameVersion { "Rev <rev>", head }` in one transaction, so `Ledger::state_at(at)` reopens the
//!   project exactly as it was issued.
//!
//! This module reads and plans; the commands commit (`ecad_engine::pages::commands::sheets`).

use crate::CmdError;
use ecad_model::document::book::MAX_FOLDER_DEPTH;
use ecad_model::{Document, FolderId, ObjectRef, Op, Project, RevField, RevValue, Word};

/// The editable fields of a revision, in table order.
pub const FIELDS: [&str; 6] = ["rev", "date", "description", "drawn", "checked", "approved"];
/// The old key prefix of revision properties (reserved on the project).
pub const PREFIX: &str = ecad_model::document::project::REVISIONS_KEY;
/// The field set by issuing.
pub const ISSUED: &str = "issued";
/// The field set by issuing through a release (brief 027, FP1): the release folder, relative to
/// the project file's folder (absolute on another drive). Unknown to older builds, which skip it.
pub const RELEASED: &str = "released";
/// Most revisions a project may hold (a bound on hostile files).
pub const MAX_REVISIONS: usize = ecad_model::document::project::MAX_REVISIONS;
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
    /// Brief 033 (design 033 §7.2): custom fields (`rev.<n>.ecr`), by field name.
    pub extra: std::collections::BTreeMap<String, String>,
    /// The version name, once issued.
    pub issued: Option<String>,
    /// The release folder, when it was issued by a release (FP1).
    pub released: Option<String>,
}

impl Revision {
    /// The value of field `name`, built-in or custom (empty when unset or unknown).
    pub fn get(&self, name: &str) -> &str {
        match FIELDS.iter().position(|f| *f == name) {
            Some(i) => self.fields.get(i).map_or("", String::as_str),
            None => self.extra.get(name).map_or("", String::as_str),
        }
    }

    pub fn rev(&self) -> &str {
        self.get("rev")
    }

    /// The view of model row `r`, row number `n`.
    fn of(n: u32, r: &ecad_model::Revision) -> Revision {
        let mut fields: [String; 6] = Default::default();
        for (slot, f) in fields.iter_mut().zip(RevField::ALL) {
            *slot = r.field(*f).unwrap_or_default().to_string();
        }
        Revision {
            n,
            fields,
            extra: r.custom.iter().map(|(k, v)| (k.as_str().to_string(), v.to_string())).collect(),
            issued: r.issued.as_ref().map(ToString::to_string),
            released: r.released.as_ref().map(ToString::to_string),
        }
    }
}

/// `project` with field `field` of row `n` (from 1) set to `value` (`None` clears it); row
/// `len + 1` is made by its first field. `field` is a built-in field, a custom one, `issued` or
/// `released`.
///
/// # Errors
/// No such row, a field name that is none, or a value the field cannot hold.
pub fn with_field(project: &Project, n: u32, field: &str, value: Option<&str>) -> Result<Project, CmdError> {
    let mut p = project.clone();
    let i = usize::try_from(n).ok().and_then(|n| n.checked_sub(1)).ok_or_else(|| CmdError::refused("Revision rows start at 1."))?;
    if i == p.revisions.len() {
        if p.revisions.len() >= MAX_REVISIONS {
            return Err(CmdError::refused(format!("A project holds at most {MAX_REVISIONS} revisions.")));
        }
        p.revisions.push(ecad_model::Revision::default());
    }
    let row = p.revisions.get_mut(i).ok_or_else(|| CmdError::refused(format!("There is no revision row {n}.")))?;
    let text = |v: &str| RevValue::new(v).map_err(|e| CmdError::refused(format!("\"{v}\" is refused: {e}.")));
    let value = value.map(str::trim).filter(|v| !v.is_empty());
    if field == ISSUED {
        row.issued = value.map(text).transpose()?;
    } else if field == RELEASED {
        row.released =
            value.map(|v| ecad_model::naming::text::Text::new(v).map_err(|e| CmdError::refused(format!("That folder is refused: {e}.")))).transpose()?;
    } else if field == "rev" {
        row.rev = value.map(|v| ecad_model::Rev::new(v).map_err(|e| CmdError::refused(format!("\"{v}\" is not a rev: {e}.")))).transpose()?;
    } else if let Some(f) = RevField::parse(field) {
        row.set_field(f, value.map(text).transpose()?);
    } else {
        let name = ecad_model::CustomField::new(field)
            .ok_or_else(|| CmdError::refused(format!("\"{field}\" is not a field name. Use 1 to 20 letters, digits or _.")))?;
        match value {
            Some(v) => {
                row.custom.insert(name, text(v)?);
            }
            None => {
                row.custom.remove(&name);
            }
        }
    }
    // A row emptied by clearing its last field is no row (the pending row only).
    if i + 1 == p.revisions.len() && p.revisions.last().is_some_and(ecad_model::Revision::is_empty) {
        p.revisions.pop();
    }
    Ok(p)
}

/// Whether a `ModifyProject` from `before` to `after` issued a row or took an issue back (what the
/// title block's Rev box and the issued-revision lists show).
pub fn issue_changed(before: &Project, after: &Project) -> bool {
    let issued = |p: &Project| p.revisions.iter().map(|r| r.issued.clone()).collect::<Vec<_>>();
    issued(before) != issued(after)
}

/// The `ModifyProject` from `before` to `after`, if they differ.
pub fn modify(before: &Project, after: Project) -> Option<Op> {
    (*before != after).then(|| Op::ModifyProject { before: before.clone(), after })
}

/// Set the release folder of row `n` in the last `ModifyProject` of `ops` (the issue's) to
/// `folder`. Nothing when `ops` hold none.
pub fn set_released(ops: &mut [Op], n: u32, folder: &str) {
    let Some(Op::ModifyProject { after, .. }) = ops.iter_mut().rev().find(|o| matches!(o, Op::ModifyProject { .. })) else { return };
    let i = usize::try_from(n).ok().and_then(|n| n.checked_sub(1));
    if let (Some(row), Ok(t)) = (i.and_then(|i| after.revisions.get_mut(i)), ecad_model::naming::text::Text::new(folder)) {
        row.released = Some(t);
    }
}

// ---------------------------------------------------------------- columns (brief 033, design 033 §7)

/// The project property of the revision table's columns (FP2).
pub const COLUMNS_KEY: &str = "revisions.columns";
/// The project property of the pending-revision marker (FP3): `star`, `draft` or `off`.
pub const PENDING_KEY: &str = "revisions.pending";
/// Most columns.
pub const MAX_COLUMNS: usize = 10;
/// Most characters of a custom field name and of a header.
pub const MAX_FIELD_CHARS: usize = 20;
/// Column widths a project may set, millimetres on A3 (a bound on hostile values).
pub const MIN_WIDTH_MM: f64 = 6.0;
pub const MAX_WIDTH_MM: f64 = 200.0;
/// Most characters of a stored column list (a bound on hostile values).
pub const MAX_COLUMNS_CHARS: usize = 600;
/// Description is never narrowed below this before the other columns are (design 033 §7.1).
pub const MIN_DESCRIPTION_MM: f64 = 30.0;
/// The revision table's width on A3, millimetres (design 019 §4).
pub const A3_TABLE_MM: f64 = 200.0;
/// Said when the fixed widths leave Description too narrow.
pub const NARROWED: &str = "Columns are narrowed to fit the 200 mm revision table on A3.";

/// The headers of the built-in fields, in [`FIELDS`] order.
const BUILTIN_HEADERS: [&str; 6] = ["Rev", "Date", "Description", "Drawn", "Checked", "Approved"];

/// Whether `field` may name a custom revision field: 1 to [`MAX_FIELD_CHARS`] of `a-z 0-9 _`,
/// not a built-in, not `issued` or `released`.
pub fn is_custom_field(field: &str) -> bool {
    (1..=MAX_FIELD_CHARS).contains(&field.len())
        && field.bytes().all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_'))
        && !FIELDS.contains(&field)
        && field != ISSUED
        && field != RELEASED
}

/// One column as written: its field, and its header and width when the list gives them.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnSpec {
    pub field: String,
    pub header: Option<String>,
    pub width: Option<f64>,
}

/// Read a column list `field[:Header][@width], …` (design 033 §7.1). Empty: no columns (the
/// caller uses the built-in six).
///
/// # Errors
/// In the design's words: a bad field name, `issued`/`released`, a bad header or width, a field
/// twice, Rev or Description missing, more than [`MAX_COLUMNS`].
pub fn parse_columns(text: &str) -> Result<Vec<ColumnSpec>, CmdError> {
    if text.chars().count() > MAX_COLUMNS_CHARS {
        return Err(CmdError::refused(format!("A column list is at most {MAX_COLUMNS_CHARS} characters.")));
    }
    let mut out: Vec<ColumnSpec> = Vec::new();
    for item in text.split(',').map(str::trim).filter(|t| !t.is_empty()) {
        if out.len() >= MAX_COLUMNS {
            return Err(CmdError::refused("At most 10 columns."));
        }
        let (rest, width) = match item.rsplit_once('@') {
            Some((r, w)) => {
                let x = w.trim().parse::<f64>().ok().filter(|x| x.is_finite() && (MIN_WIDTH_MM..=MAX_WIDTH_MM).contains(x));
                (r, Some(x.ok_or_else(|| CmdError::refused(format!("\"{}\" is not a width. Use 6 to 200 (millimetres).", w.trim())))?))
            }
            None => (item, None),
        };
        let (field, header) = match rest.split_once(':') {
            Some((f, h)) => {
                let h = h.trim();
                if !(1..=MAX_FIELD_CHARS).contains(&h.chars().count()) || h.chars().any(char::is_control) {
                    return Err(CmdError::refused(format!("\"{h}\" is not a header. Use 1 to 20 characters.")));
                }
                (f.trim(), Some(h.to_string()))
            }
            None => (rest.trim(), None),
        };
        if field == ISSUED || field == RELEASED {
            return Err(CmdError::refused(format!("\"{field}\" is set by issuing.")));
        }
        if !FIELDS.contains(&field) && !is_custom_field(field) {
            return Err(CmdError::refused(format!("\"{field}\" is not a field name. Use 1 to 20 letters, digits or _.")));
        }
        if out.iter().any(|c| c.field == field) {
            return Err(CmdError::refused(format!("\"{field}\" is a column twice.")));
        }
        out.push(ColumnSpec { field: field.to_string(), header, width });
    }
    if out.is_empty() {
        return Ok(Vec::new());
    }
    if !out.iter().any(|c| c.field == "rev") || !out.iter().any(|c| c.field == "description") {
        return Err(CmdError::refused("Rev and Description must be columns."));
    }
    Ok(out)
}

/// Whether `text` is a column list [`parse_columns`] accepts (the registry's parser).
pub fn columns_ok(text: &str) -> bool {
    parse_columns(text).is_ok()
}

/// A column as the tables draw it.
#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    pub field: String,
    pub header: String,
    /// Millimetres on A3; `None` for Description, which takes what is left.
    pub width: Option<f64>,
}

/// The project's revision columns (registry `revisions.columns`; absent, empty or unreadable:
/// the built-in six), each with its header and width (the built-in list's for a built-in
/// field, `revisions.custom_width` for a custom one).
pub fn columns(doc: &Document) -> Vec<Column> {
    use crate::defaults::{Scope, Value, print, resolve};
    let builtin: Vec<ColumnSpec> = match print::REVISION_COLUMNS.builtin() {
        Value::Text(t) => parse_columns(t).unwrap_or_default(),
        _ => Vec::new(),
    };
    let stored = resolve(doc, &print::REVISION_COLUMNS, Scope::project()).text();
    let specs = parse_columns(&stored).ok().filter(|c| !c.is_empty()).unwrap_or_else(|| builtin.clone());
    let custom = resolve(doc, &print::REVISION_CUSTOM_WIDTH, Scope::project()).number();
    specs
        .into_iter()
        .map(|c| {
            let at = FIELDS.iter().position(|f| *f == c.field);
            let header = c.header.clone().unwrap_or_else(|| match at.and_then(|i| BUILTIN_HEADERS.get(i)) {
                Some(h) => (*h).to_string(),
                None => c.field.to_uppercase(),
            });
            let width = if c.field == "description" {
                None
            } else {
                c.width.or_else(|| builtin.iter().find(|b| b.field == c.field).and_then(|b| b.width)).or(Some(custom))
            };
            Column { field: c.field, header, width }
        })
        .collect()
}

/// The widths of `cols` across `total` millimetres (design 033 §7.1): fixed widths as given,
/// Description the rest; when that leaves Description under [`MIN_DESCRIPTION_MM`], every fixed
/// width scales down. Also whether they were narrowed.
pub fn widths(cols: &[Column], total: f64) -> (Vec<f64>, bool) {
    let total = if total.is_finite() && total > 0.0 { total } else { 0.0 };
    let fixed: f64 = cols.iter().filter_map(|c| c.width).sum();
    let narrowed = total - fixed < MIN_DESCRIPTION_MM;
    let scale = if narrowed && fixed > 0.0 { ((total - MIN_DESCRIPTION_MM).max(0.0) / fixed).min(1.0) } else { 1.0 };
    let rest = (total - fixed * scale).max(0.0);
    (cols.iter().map(|c| c.width.map_or(rest, |w| w * scale)).collect(), narrowed)
}

// ---------------------------------------------------------------- pending marker (design 033 §6)

ecad_model::word_enum! {
    /// How the title block marks a pending revision ([`PENDING_KEY`]).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub enum PendingStyle {
        /// `D*` (the default).
        #[default]
        Star,
        /// `E (draft)`.
        Draft,
        Off,
    }
}

impl PendingStyle {
    /// The project's style (registry `revisions.pending`).
    pub fn of(doc: &Document) -> PendingStyle {
        let word = crate::defaults::resolve(doc, &crate::defaults::print::REVISION_PENDING, crate::defaults::Scope::project()).word();
        <PendingStyle as ecad_model::Word>::parse(word).unwrap_or_default()
    }
}

/// Most undo-stack entries the pending walk reads; more count as pending.
pub const MAX_PENDING_WALK: usize = 10_000;

/// Whether `ops` only fill in the pending row `pending` or set the revision table's own
/// settings: not a drawing change (design 033 §6.1).
fn fills_pending_row(ops: &[Op], pending: u32) -> bool {
    let at = usize::try_from(pending).ok().and_then(|n| n.checked_sub(1));
    ops.iter().all(|op| match op {
        Op::SetProperty { object: ObjectRef::Project, key, .. } => key == COLUMNS_KEY || key == PENDING_KEY,
        Op::ModifyProject { before, after } => {
            let rows = |p: &Project| -> Vec<ecad_model::Revision> {
                p.revisions.iter().enumerate().filter(|(i, _)| Some(*i) != at).map(|(_, r)| r.clone()).collect()
            };
            Project { revisions: Vec::new(), ..before.clone() } == Project { revisions: Vec::new(), ..after.clone() } && rows(before) == rows(after)
        }
        _ => false,
    })
}

/// Whether the project holds a change still in effect after its newest issued revision (design
/// 033 §6.1, overseer ruling 4): the undo stack is walked from the top; an issue ends it (not
/// pending); any other edit or merge ends it (pending), except one that only fills in the
/// pending row. With no issued revision there is no marker. A transaction the ledger cannot
/// read, or a walk longer than [`MAX_PENDING_WALK`], counts as pending.
pub fn is_pending(ledger: &ecad_model::Ledger) -> bool {
    let doc = ledger.document();
    if issued(doc).is_empty() {
        return false;
    }
    let pending = pending_n(doc);
    let (undo, _) = ledger.stacks();
    for (i, (seq, _)) in undo.iter().rev().enumerate() {
        if i >= MAX_PENDING_WALK {
            return true;
        }
        let Some(tx) = ledger.transaction(*seq) else { return true };
        if issue_in(&tx.ops).is_some() {
            return false;
        }
        if tx.kind.is_edit() && !fills_pending_row(&tx.ops, pending) {
            return true;
        }
    }
    false
}

/// The Rev box's text (design 033 §6.2): the newest issued rev (`-` with none), marked when
/// `pending`: `D*`, `E (draft)` (the pending row's rev, else the suggested next), or plain.
pub fn rev_box(doc: &Document, pending: bool) -> String {
    let Some(newest) = issued(doc).into_iter().next() else { return "-".into() };
    match (pending, PendingStyle::of(doc)) {
        (true, PendingStyle::Star) => format!("{}*", newest.rev()),
        (true, PendingStyle::Draft) => format!("{} (draft)", pending_rev(doc)),
        _ => newest.rev().to_string(),
    }
}

/// The value of `field` (built-in, custom, `issued` or `released`) on row `n` of `project`, if set.
pub fn field_of(project: &Project, n: u32, field: &str) -> Option<String> {
    let r = rows(project).into_iter().find(|r| r.n == n)?;
    match field {
        ISSUED => r.issued,
        RELEASED => r.released,
        f => Some(r.get(f).to_string()).filter(|v| !v.is_empty()),
    }
}

/// Every revision row of `project`, by `n` ascending.
pub fn rows(project: &Project) -> Vec<Revision> {
    project.revisions.iter().zip(1u32..).map(|(r, n)| Revision::of(n, r)).collect()
}

/// Every revision row, by `n` ascending.
pub fn list(doc: &Document) -> Vec<Revision> {
    rows(&doc.project)
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

/// Print Book's note for a project whose drawing changed since its newest issue (design 033
/// §6.2), unless the marker is off.
pub fn pending_note(doc: &Document, pending: bool) -> Option<String> {
    let newest = issued(doc).into_iter().next()?;
    if !pending || PendingStyle::of(doc) == PendingStyle::Off {
        return None;
    }
    let shown = rev_box(doc, true);
    Some(format!("The drawing has changed since Rev {}; the title block shows {shown}.", newest.rev()))
}

/// Check a rev text.
///
/// # Errors
/// In words.
pub fn check_rev(doc: &Document, rev: &str) -> Result<(), CmdError> {
    let n = rev.chars().count();
    if !(1..=MAX_REV_CHARS).contains(&n) || !rev.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(CmdError::refused("Rev is 1 to 8 letters or digits."));
    }
    if issued(doc).iter().any(|r| r.rev().eq_ignore_ascii_case(rev)) {
        return Err(CmdError::refused(format!("Rev {rev} was issued before; choose another.")));
    }
    Ok(())
}

/// The ops that set the pending row's `field` to `value` (`None` clears), creating the row on its
/// first field. Nothing when it already holds that value.
///
/// # Errors
/// As [`with_field`].
pub fn set_pending_ops(doc: &Document, field: &str, value: Option<String>) -> Result<Vec<Op>, CmdError> {
    let after = with_field(&doc.project, pending_n(doc), field, value.as_deref())?;
    Ok(modify(&doc.project, after).into_iter().collect())
}

/// The version name of an issued rev.
pub fn version_name(rev: &str) -> String {
    format!("Rev {rev}")
}

/// The ops that undo `ops`.
pub fn inverse(ops: &[Op]) -> Vec<Op> {
    ops.iter().rev().map(Op::inverse).collect()
}

/// When the transaction `ops` issues a revision: the rev it issued and its row number (a
/// `ModifyProject` giving a row its `issued`).
pub fn issue_in(ops: &[Op]) -> Option<(String, u32)> {
    ops.iter().find_map(|op| {
        let Op::ModifyProject { before, after } = op else { return None };
        after.revisions.iter().zip(1u32..).find_map(|(r, n)| {
            let was = usize::try_from(n).ok().and_then(|n| before.revisions.get(n - 1)).and_then(|b| b.issued.as_ref());
            let v = r.issued.as_ref().filter(|_| was.is_none())?;
            Some((v.as_str().strip_prefix("Rev ").unwrap_or(v.as_str()).to_string(), n))
        })
    })
}

/// The UTC date `YYYY-MM-DD` of `ms` milliseconds since 1970-01-01, through `chrono` (house rule);
/// `None` beyond what it represents.
pub fn date_of_ms(ms: u64) -> Option<String> {
    let t = chrono::DateTime::from_timestamp_millis(i64::try_from(ms).ok()?)?;
    Some(t.format("%Y-%m-%d").to_string())
}

/// `Schematic layouts / Spare circuits` (empty at the top level).
pub fn folder_path(doc: &Document, folder: Option<FolderId>) -> String {
    let mut names = Vec::new();
    let mut at = folder;
    while let Some(f) = at {
        let Some(x) = doc.folders.get(&f) else { break };
        names.push(x.name.clone());
        if names.len() > MAX_FOLDER_DEPTH {
            break;
        }
        at = x.parent;
    }
    names.reverse();
    names.join(" / ")
}

#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn next_revs() {
        for (last, next) in [(None, "A"), (Some("A"), "B"), (Some("Z"), "AA"), (Some("AZ"), "BA"), (Some("0"), "1"), (Some("P3"), "P4")] {
            assert_eq!(next_rev(last), next, "{last:?}");
        }
    }

    #[test]
    fn the_column_grammar() {
        let c = parse_columns("rev, date, ecr:ECR@20, description, validated:Validated").unwrap();
        assert_eq!(c.len(), 5);
        assert_eq!(c[2], ColumnSpec { field: "ecr".into(), header: Some("ECR".into()), width: Some(20.0) });
        assert_eq!(parse_columns("").unwrap(), Vec::new());
        for (bad, words) in [
            ("rev, ecr!, description", "\"ecr!\" is not a field name"),
            ("rev, date", "Rev and Description must be columns."),
            ("rev, released, description", "\"released\" is set by issuing."),
            ("rev, x@NaN, description", "is not a width"),
            ("rev, x@-5, description", "is not a width"),
            ("rev, x@1e308, description", "is not a width"),
            ("rev, rev, description", "a column twice"),
        ] {
            let e = parse_columns(bad).unwrap_err().to_string();
            assert!(e.contains(words), "{bad}: {e}");
        }
        let many = (0..1000).map(|i| format!("f{i}")).collect::<Vec<_>>().join(", ");
        assert!(parse_columns(&format!("rev, description, {many}")).is_err());
    }

    #[test]
    fn hostile_settings_read_as_absent() {
        let mut doc = Document::default();
        for v in ["x".repeat(10_000), "rev, ecr@NaN, description".into(), (0..1000).map(|i| format!("f{i}")).collect::<Vec<_>>().join(","), "".into()]
        {
            doc.display.insert(COLUMNS_KEY.into(), v);
            let cols = columns(&doc);
            // Brief 055 (FP5): the built-in columns, ECR after Rev.
            assert_eq!(
                cols.iter().map(|c| c.field.as_str()).collect::<Vec<_>>(),
                ["rev", "ecr", "date", "description", "drawn", "checked", "approved"],
                "the built-in columns"
            );
        }
        for v in ["", "x", "STAR?"] {
            doc.display.insert(PENDING_KEY.into(), v.into());
            assert_eq!(PendingStyle::of(&doc), PendingStyle::Star);
        }
        doc.display.insert("tags.case".into(), "x".into());
        doc.display.insert("tags.dash".into(), "maybe".into());
        assert_eq!(crate::defaults::print::tag_style(&doc), ecad_model::naming::tag::Style::from(doc.settings.tag_padding()));
    }

    #[test]
    fn the_registry_and_ecad_model_read_the_tag_style_alike() {
        let mut doc = Document::default();
        for (dash, case) in [("yes", "upper"), ("no", "typed"), ("NO", "Typed"), ("", "")] {
            doc.display.insert("tags.dash".into(), dash.into());
            doc.display.insert("tags.case".into(), case.into());
            assert_eq!(crate::defaults::print::tag_style(&doc), ecad_model::naming::tag::Style::of(&doc), "{dash} {case}");
        }
    }

    #[test]
    fn widths_narrow_rather_than_refuse() {
        let cols = |w: &[Option<f64>]| w.iter().map(|w| Column { field: "f".into(), header: "F".into(), width: *w }).collect::<Vec<_>>();
        let (w, narrowed) = widths(&cols(&[Some(12.0), None, Some(22.0)]), 170.0);
        assert_eq!((w, narrowed), (vec![12.0, 136.0, 22.0], false));
        let (w, narrowed) = widths(&cols(&[Some(100.0), None, Some(100.0)]), 170.0);
        assert!(narrowed && (w[1] - MIN_DESCRIPTION_MM).abs() < 1e-9 && (w.iter().sum::<f64>() - 170.0).abs() < 1e-9, "{w:?}");
    }

    #[test]
    fn dates_go_through_chrono() {
        for (ms, date) in [(0, "1970-01-01"), (951_782_400_000, "2000-02-29"), (1_794_441_600_000, "2026-11-12"), (4_107_542_399_999, "2100-02-28")] {
            assert_eq!(date_of_ms(ms).as_deref(), Some(date), "{ms}");
        }
        assert_eq!(date_of_ms(u64::MAX), None);
    }
}
