//! The pre-release check plan: the
//! check's groups ([`GROUPS`]) and counts, `checks.csv`, the file stem and folder names, and the
//! words of the refusals and stops a release gives, and the generated lists of the folder
//! ([`LISTS`]).
//!
//! # Semantics
//! - A check row shows in one [`Group`]: every error-severity row in **Errors**, the others by
//!   their kind's JSON name, and a kind not in [`GROUPS`] in **Other** (a view: kinds keep their
//!   severities).
//! - [`nothing_changed`] warns, never refuses, when every step since the last issue only edited
//!   revision fields.
//!
//! This module intentionally contains **no** file writing, project reading or clock: the engine
//! builds and writes the release folder and records the issue.

use std::collections::BTreeMap;

use ecad_model::{Document, Op, Transaction};
use serde_json::Value;

use crate::checks::{Check, Severity};
use crate::reports::{Cell, Table};

ecad_model::word_enum! {
    /// What a release accepts: the check's errors.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Accept {
        Errors,
    }
}

ecad_model::word_enum! {
    /// What an issue does with provisional tags and wire numbers.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Provisional {
        /// Number them first, in one transaction before the issue.
        Finalise,
        /// Issue them as they are.
        Issue,
    }
}

ecad_model::word_enum! {
    /// What a release does when its folder exists.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Existing {
        /// Write only the files that are missing.
        #[strum(serialize = "add-missing")]
        AddMissing,
        /// Write the folder again.
        Replace,
        /// Write a new folder beside it.
        #[strum(serialize = "keep-both")]
        KeepBoth,
    }
}

/// Most characters of a file stem.
pub(crate) const MAX_STEM: usize = 80;
/// Rows per group `prerelease` returns by default, and at most.
pub const DEFAULT_ROWS: usize = 50;
pub const MAX_ROWS: usize = 2000;

/// Said by the slots for a project with no strips or no cables.
pub const NO_STRIPS: &str = "No terminal strips in this project.";
pub(crate) const NO_CABLES: &str = "No cables in this project.";
/// Said for a project that was never saved.
pub const SAVE_FIRST: &str = "Save the project first, so the revision is kept with it and the folder has a place.";

/// A group of the pre-release check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Group {
    Errors,
    OpenEnds,
    Orphans,
    Provisional,
    Parts,
    Other,
}

impl Group {
    pub const ALL: [Group; 6] = [Group::Errors, Group::OpenEnds, Group::Orphans, Group::Provisional, Group::Parts, Group::Other];

    /// The JSON and element name (`ecad.release.group.<id>`).
    pub fn id(self) -> &'static str {
        match self {
            Group::Errors => "errors",
            Group::OpenEnds => "open-ends",
            Group::Orphans => "orphans",
            Group::Provisional => "provisional",
            Group::Parts => "parts",
            Group::Other => "other",
        }
    }

    /// The header's words.
    pub fn words(self) -> &'static str {
        match self {
            Group::Errors => "Errors",
            Group::OpenEnds => "Open ends",
            Group::Orphans => "Orphans",
            Group::Provisional => "Provisional",
            Group::Parts => "? parts",
            Group::Other => "Other",
        }
    }
}

/// Which kinds each group shows (by their JSON names, so the numbering kinds are placed before they
/// exist). Errors take every error-severity kind; a kind in no row is in [`Group::Other`].
pub const GROUPS: &[(Group, &[&str])] = &[
    (Group::OpenEnds, &["open-end", "loose-wire", "no-wire-yet", "drawn-away", "looks-connected", "unconnected"]),
    (Group::Orphans, &["orphan-reference", "lonely-continuation", "single-point"]),
    (Group::Provisional, &["provisional-tag", "provisional-wires", "unnumbered"]),
    (Group::Parts, &["no-part-number"]),
];

/// The group a row shows in.
pub fn group_of(c: &Check) -> Group {
    if c.severity() == Severity::Error {
        return Group::Errors;
    }
    let name = c.kind.name();
    GROUPS.iter().find(|(_, kinds)| kinds.contains(&name)).map_or(Group::Other, |(g, _)| *g)
}

/// `rows` by group, in [`Group::ALL`] order, each in book order (as `rows` came).
pub fn grouped(rows: &[Check]) -> BTreeMap<Group, Vec<&Check>> {
    let mut out: BTreeMap<Group, Vec<&Check>> = BTreeMap::new();
    for c in rows {
        out.entry(group_of(c)).or_default().push(c);
    }
    out
}

/// Counts by severity in the release check: (errors, warnings, info). A cable not drawn yet counts
/// as a warning here.
pub fn counts(rows: &[Check]) -> (usize, usize, usize) {
    rows.iter().fold((0, 0, 0), |(e, w, i), c| match c.release_severity() {
        Severity::Error => (e + 1, w, i),
        Severity::Warning => (e, w + 1, i),
        Severity::Info => (e, w, i + 1),
    })
}

/// The file stem's cleaning: `\ / : * ? " < > |` and control characters become `-`; at most
/// [`MAX_STEM`] characters; `project` when nothing is left.
pub fn clean_stem(raw: &str) -> String {
    let s: String = raw.chars().map(|c| if c.is_control() || "\\/:*?\"<>|".contains(c) { '-' } else { c }).take(MAX_STEM).collect();
    let s = s.trim().trim_end_matches('.').trim().to_string();
    if s.is_empty() { "project".into() } else { s }
}

/// `Rev B`.
pub fn folder_name(rev: &str) -> String {
    format!("Rev {rev}")
}

/// `checks.csv`: `Severity, Group, Kind, Where, What, Accepted`.
pub fn checks_table(doc: &Document, rows: &[Check], accepted: bool) -> Table {
    let cell = |t: &str| Cell { text: t.to_string(), flag: None };
    let mut out: Vec<(Group, Vec<Cell>)> = rows
        .iter()
        .map(|c| {
            let g = group_of(c);
            let acc = if accepted && c.severity() == Severity::Error { "yes" } else { "" };
            (g, vec![cell(c.severity().name()), cell(g.words()), cell(c.kind.words()), cell(&c.locator(doc)), cell(&c.which), cell(acc)])
        })
        .collect();
    out.sort_by_key(|(g, _)| *g);
    Table { headers: vec!["Severity", "Group", "Kind", "Where", "What", "Accepted"], rows: out.into_iter().map(|(_, r)| r).collect() }
}

/// Whether `params` choose the Finalise numbering.
pub fn finalising(params: &Value) -> bool {
    params.get("provisional").and_then(Value::as_str).is_some_and(|w| matches!(w.trim().to_ascii_lowercase().as_str(), "finalise" | "finalize"))
}

/// The refusal for errors in `rows` without `accept: "errors"`, placed in `doc`.
pub fn errors_refusal(doc: &Document, rows: &[Check]) -> Option<String> {
    let (errors, _, _) = counts(rows);
    if errors == 0 {
        return None;
    }
    let first = rows.iter().find(|c| c.severity() == Severity::Error);
    let what = first.map(|c| format!(" ({}, {})", c.kind.words(), c.locator(doc))).unwrap_or_default();
    let open = if errors == 1 { "1 error is open".to_string() } else { format!("{errors} errors are open") };
    let it = if errors == 1 { "it" } else { "them" };
    Some(format!("{open}{what}. Fix {it}, or give accept: \"errors\" to release with {it}; checks.csv lists {it}."))
}

/// The refusal of a release that would record more transactions than its caller allows.
pub fn over_budget(needed: usize, budget: usize) -> String {
    let what = if needed == 2 { "Finalise numbering and the issue (2 steps)" } else { "the issue (1 step)" };
    let allowed = match budget {
        0 => "no more steps in this call".to_string(),
        1 => "1 step in this call".to_string(),
        n => format!("{n} steps in this call"),
    };
    format!("Release refused: it records {what}, and the caller allows {allowed}. Nothing was issued or written; call release on its own.")
}

/// The words of a release stopped before the issue.
pub fn stopped(reason: &str) -> String {
    let r = reason.trim_end_matches('.');
    format!("Release stopped: {r}. Nothing was issued or written. Choose another folder, or try again.")
}

/// How many provisional rows the check holds (tags, and the wire-numbers row).
pub fn provisional_count(rows: &[Check]) -> usize {
    rows.iter().filter(|c| matches!(c.kind.name(), "provisional-tag" | "provisional-wires")).count()
}

/// `Nothing has changed since Rev A.` when every step after the last issue up to `head` only
/// edited revision fields: a warning, never a refusal. `tx` reads a step of
/// the ledger; it stops at the first other step.
pub fn unchanged_since<'a>(doc: &Document, head: u64, tx: impl Fn(u64) -> Option<&'a Transaction>) -> Option<ecad_model::Rev> {
    let last = crate::releases::revisions::issued(doc).into_iter().next()?;
    let at = *doc.versions().get(&ecad_model::PointName::Revision(last.rev_value?))?;
    let only_revisions = |t: &Transaction| {
        t.ops.iter().all(|o| match o {
            Op::ModifyProject { before, after } => {
                ecad_model::Project { revisions: Vec::new(), ..before.clone() } == ecad_model::Project { revisions: Vec::new(), ..after.clone() }
            }
            _ => false,
        })
    };
    let mut seq = at.saturating_add(2);
    while seq <= head {
        let t = tx(seq)?;
        if !only_revisions(t) {
            return None;
        }
        seq += 1;
    }
    last.rev_value
}

/// [`unchanged_since`] in words: `Nothing has changed since Rev A.`
pub fn nothing_changed<'a>(doc: &Document, head: u64, tx: impl Fn(u64) -> Option<&'a Transaction>) -> Option<String> {
    let rev = unchanged_since(doc, head, tx)?;
    Some(format!("Nothing has changed since Rev {}.", doc.project().rev_scheme.show(rev)))
}

// ---------------------------------------------------------------- slots

/// A generated list of the folder.
pub struct Slot {
    pub id: &'static str,
    /// After `<stem> Rev B`.
    pub suffix: &'static str,
    /// The table, or `None` with [`Slot::missing_why`] when the project gives none.
    pub build: fn(&Document, &crate::reports::kept::ReportModels) -> Option<Table>,
    pub missing_why: &'static str,
}

fn bom(_: &Document, o: &crate::reports::kept::ReportModels) -> Option<Table> {
    Some(o.bom_table(crate::reports::Group::Location))
}
fn wires(_: &Document, o: &crate::reports::kept::ReportModels) -> Option<Table> {
    Some(o.wire_table())
}
fn cables(_: &Document, o: &crate::reports::kept::ReportModels) -> Option<Table> {
    Some(o.cable_table())
}
/// Terminal plan, when the project has strips.
fn terminal_block_plan(_: &Document, o: &crate::reports::kept::ReportModels) -> Option<Table> {
    (!o.strips.is_empty()).then(|| o.terminal_block_plan_csv())
}
/// Cable schedule, when the project has cables.
fn cable_schedule(_: &Document, o: &crate::reports::kept::ReportModels) -> Option<Table> {
    (o.cable_count() > 0).then(|| o.cable_schedule_csv())
}

/// The generated lists, in folder order. The book, `checks` and the manifest are always written.
pub const LISTS: [Slot; 7] = [
    Slot { id: "bom", suffix: " BOM.csv", build: bom, missing_why: "" },
    Slot { id: "wires", suffix: " wires.csv", build: wires, missing_why: "" },
    Slot { id: "cables", suffix: " cables.csv", build: cables, missing_why: "" },
    Slot { id: "terminal-block-plan", suffix: " terminal plan.csv", build: terminal_block_plan, missing_why: NO_STRIPS },
    Slot { id: "cable-schedule", suffix: " cable schedule.csv", build: cable_schedule, missing_why: NO_CABLES },
    // ` IO list.csv` (a `/` cannot be in a file name).
    // The rack overview, before the I/O list in the book.
    Slot { id: "plc-racks", suffix: " PLC racks.csv", build: plc_racks, missing_why: crate::plc::sheets::NO_RACKS },
    Slot { id: "io-list", suffix: " IO list.csv", build: io_list, missing_why: crate::plc::iolist::NO_CARDS },
];

/// PLC racks list, when the project has a rack or a card.
fn plc_racks(doc: &Document, o: &crate::reports::kept::ReportModels) -> Option<Table> {
    crate::plc::sheets::rack_csv(doc, &o.plc)
}

/// I/O list, when the project has a card with channels.
fn io_list(doc: &Document, o: &crate::reports::kept::ReportModels) -> Option<Table> {
    crate::plc::iolist::csv_table(doc, &o.plc)
}

/// Whether `slot` gives a list for `doc` (the check's Writes line, before anything is built):
/// Lists only when the project has strips or cables.
pub fn slot_gives(slot: &Slot, doc: &Document) -> bool {
    match slot.id {
        "terminal-block-plan" => doc.devices().keys().any(|c| crate::terminal_strips::strip_of(doc, *c).is_some()),
        "cable-schedule" => !doc.cables().is_empty(),
        "io-list" => doc.devices().keys().any(|c| crate::plc::card_type_of(doc, *c).is_some_and(|t| t.has_channels())),
        "plc-racks" => doc.devices().keys().any(|c| crate::plc::card_type_of(doc, *c).is_some()),
        _ => slot.missing_why.is_empty(),
    }
}

/// The tags of the cables in `fan` that still exist (a release manifest's `cable_views`).
pub fn fan_tags(doc: &Document, fan: &std::collections::BTreeSet<ecad_model::CableId>) -> Vec<String> {
    fan.iter().filter(|k| doc.cables().contains_key(k)).map(|k| crate::cables::cable_title(doc, *k)).collect()
}
