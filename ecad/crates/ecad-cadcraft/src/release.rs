//! Release (brief 027, design 027): the pre-release check, the named revision and the stamped
//! folder.
//!
//! # Semantics
//! - **Pre-release check** ([`check_rows`], [`grouped`]): the kept Checks of the state to be
//!   issued (the project's own kept list and report models: opening it never rescans the
//!   project), grouped for a release by [`GROUPS`] (a view: kinds keep their severities). A kind
//!   not in the table lands in **Other**.
//! - **Release of the pending revision** ([`run_release`]), in this order so a failure leaves
//!   nothing half-done (design 027 §5.2): refusals (each with its way out); 026's finalise (behind
//!   [`numbering_seam`]); every output built from the state **as issued** (`ledger.preview` of the
//!   issue's ops) and written to `<folder>.partial`; the issue transaction with FP1's
//!   `rev.<n>.released`; a save; `release.json`; the rename. A failure before the issue removes
//!   the staging folder and records nothing; one after it keeps the issue and says how to finish.
//! - **Writing an issued revision's folder** (`release {rev}`): built from the state after that
//!   revision's issue transaction (`history::issued_state`), recording nothing; works read-only,
//!   on a branch and with an agent's writes off.
//! - **An existing folder** is never overwritten silently: add missing files, replace (the old
//!   one is moved to `release/replaced/`), or keep both (`Rev B (2)`). Nothing is ever deleted
//!   but this module's own staging folder.
//! - **Slots** ([`LISTS`]): the generated lists as a table; brief 028 fills `terminal-plan` and
//!   `cable-schedule` (its terminal plan and cable schedule, columns named by its JSON keys).
//!
//! # Update map (design 027 §11.1)
//! The release folder is built on request only. The dialog's check reads the kept Checks (its
//! `orphan-reference` rows follow `crate::checks`'s map); nothing here runs on an ordinary
//! commit.
//!
//! Excluded: approval and signatures, transmittals, zip or upload, DWG/DXF and XLSX outputs, a
//! folder per branch, page-by-page diffs.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use cadcraft_engine::{CommandSpec, Session};
use ecad_model::{Document, ObjectRef, Op};
use serde_json::{Map, Value, json};

use crate::access::Pages;
use crate::checks::{self, Check, Severity};
use crate::commands::{err, opt_typed, with_any_page, with_project};
use crate::outputs::{self, Group as BomGroup, Outputs, Table};
use crate::printbook::{self, Options};
use crate::project::Project;
use crate::revisions;

/// The manifest's file name.
pub const MANIFEST: &str = "release.json";
/// The manifest's version (`ecad_release`); a breaking change bumps it.
pub const MANIFEST_VERSION: u64 = 1;
/// The folder that holds release folders, beside the project file.
pub const RELEASE_DIR: &str = "release";
/// Where a replaced folder is moved, inside [`RELEASE_DIR`].
pub const REPLACED_DIR: &str = "replaced";
/// The staging folder's suffix.
pub const PARTIAL: &str = ".partial";
/// The empty file that marks a staging folder as a release's own (only such a folder is ever
/// cleared); removed just before the folder is renamed into place.
pub const STAGING_MARK: &str = ".ecad-release-staging";
/// Most characters of a file stem.
pub const MAX_STEM: usize = 80;
/// Rows per group `prerelease` returns by default, and at most.
pub const DEFAULT_ROWS: usize = 50;
pub const MAX_ROWS: usize = 2000;
/// Largest `release.json` read back (a hostile or foreign file is not read further).
const MAX_MANIFEST_BYTES: u64 = 4 << 20;
/// Most `Rev B (n)` folders tried for Keep both.
const MAX_KEEP_BOTH: u32 = 1000;
/// Said by 028's slots for a project with no strips or no cables (design 027 §7.2, Q7).
pub const NO_STRIPS: &str = "No terminal strips in this project.";
pub const NO_CABLES: &str = "No cables in this project.";
/// Said for a project that was never saved.
pub const SAVE_FIRST: &str = "Save the project first, so the revision is kept with it and the folder has a place.";
/// Said on a build that cannot write files.
pub const NO_FILES_HERE: &str = "Release writes files; open the project in the desktop app.";

// ---------------------------------------------------------------- groups (design 027 §4.1)

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

/// Which kinds each group shows (by their JSON names, so 026's kinds are placed before they
/// exist). Errors take every error-severity kind; a kind in no row is in [`Group::Other`].
pub const GROUPS: &[(Group, &[&str])] = &[
    (Group::OpenEnds, &["open-end", "loose-wire", "drawn-away", "looks-connected", "unconnected"]),
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

/// Counts by severity: (errors, warnings, info).
pub fn counts(rows: &[Check]) -> (usize, usize, usize) {
    rows.iter().fold((0, 0, 0), |(e, w, i), c| match c.severity() {
        Severity::Error => (e + 1, w, i),
        Severity::Warning => (e, w + 1, i),
        Severity::Info => (e, w, i + 1),
    })
}

/// The check rows of the state to be issued (design 027 §4.1): the project's kept Checks and the
/// report models' rows, in book order. The pending revision's field writes change no check, so
/// these are the live project's.
pub fn check_rows(p: &mut Project) -> Vec<Check> {
    p.outputs();
    p.checks();
    let doc = p.ledger.document();
    let mut rows: Vec<Check> = p.checks.as_ref().map(|c| c.rows().cloned().collect()).unwrap_or_default();
    rows.extend(p.outputs.as_ref().map(Outputs::checks).unwrap_or_default());
    checks::sort(doc, rows)
}

// ---------------------------------------------------------------- 026 seam

/// The one place 027 calls brief 026's numbering (design 027 §4.4): the provisional items, the
/// refusal without a choice, and `numbering_cmds::before_issue` (Finalise numbering as its own
/// transaction before the issue, or issue them as they are).
pub mod numbering_seam {
    use serde_json::Value;

    use crate::access::Pages;
    use crate::checks::Check;
    use crate::project::Project;

    /// How many provisional rows the check holds (tags, and the wire-numbers row).
    pub fn provisional_count(rows: &[Check]) -> usize {
        rows.iter().filter(|c| matches!(c.kind.name(), "provisional-tag" | "provisional-wires")).count()
    }

    /// What `release` would refuse for provisional items without a choice (026's words), if any.
    pub fn refusal(p: &Project, _rows: &[Check], params: &Value) -> Option<String> {
        if params.get("provisional").is_some_and(|v| !v.is_null()) {
            return None;
        }
        let items = crate::numbering_cmds::provisional_items(p.ledger.document());
        (!items.is_empty()).then(|| {
            format!(
                "{}. Give provisional: \"finalise\" (number them first, in one transaction before the issue) or \"issue\" (issue as they are).",
                items.words()
            )
        })
    }

    /// 026's provisional step, planned without writing (release records it with the issue).
    ///
    /// # Errors
    /// 026's refusal or a bad `provisional`.
    pub fn plan(p: &mut Project, params: &Value) -> Result<crate::numbering_cmds::BeforeIssue, String> {
        crate::numbering_cmds::plan_before_issue(p, params)
    }

    /// Record a planned Finalise numbering.
    ///
    /// # Errors
    /// The commit was refused.
    pub(crate) fn commit_finalise(p: &mut Project, pg: &mut Pages<'_>, plan: &ecad_model::numbering::Plan) -> Result<(), String> {
        crate::numbering_cmds::commit_finalise(p, pg, plan)
    }
}

// ---------------------------------------------------------------- slots (design 027 §7.2)

/// A generated list of the folder.
pub struct Slot {
    pub id: &'static str,
    /// After `<stem> Rev B`.
    pub suffix: &'static str,
    /// The table, or `None` with [`Slot::missing_why`] when the project gives none.
    pub build: fn(&Document, &Outputs) -> Result<Option<Table>, String>,
    pub missing_why: &'static str,
}

fn bom(_: &Document, o: &Outputs) -> Result<Option<Table>, String> {
    Ok(Some(o.bom_table(BomGroup::Location)))
}
fn wires(_: &Document, o: &Outputs) -> Result<Option<Table>, String> {
    Ok(Some(o.wire_table()))
}
fn cables(_: &Document, o: &Outputs) -> Result<Option<Table>, String> {
    Ok(Some(o.cable_table()))
}
/// Brief 028's terminal plan, when the project has strips.
fn terminal_plan(_: &Document, o: &Outputs) -> Result<Option<Table>, String> {
    Ok((!o.strips.is_empty()).then(|| o.terminal_plan_csv()))
}
/// Brief 028's cable schedule, when the project has cables.
fn cable_schedule(_: &Document, o: &Outputs) -> Result<Option<Table>, String> {
    Ok((o.cable_count() > 0).then(|| o.cable_schedule_csv()))
}

/// The generated lists, in folder order. The book, `checks` and the manifest are always written.
pub const LISTS: [Slot; 7] = [
    Slot { id: "bom", suffix: " BOM.csv", build: bom, missing_why: "" },
    Slot { id: "wires", suffix: " wires.csv", build: wires, missing_why: "" },
    Slot { id: "cables", suffix: " cables.csv", build: cables, missing_why: "" },
    Slot { id: "terminal-plan", suffix: " terminal plan.csv", build: terminal_plan, missing_why: NO_STRIPS },
    Slot { id: "cable-schedule", suffix: " cable schedule.csv", build: cable_schedule, missing_why: NO_CABLES },
    // Brief 045c (design 045 §8.3): ` IO list.csv` (a `/` cannot be in a file name).
    // Brief 051 (design 051 §4.2): the rack overview, before the I/O list in the book.
    Slot { id: "plc-racks", suffix: " PLC racks.csv", build: plc_racks, missing_why: crate::plc::sheets::NO_RACKS },
    Slot { id: "io-list", suffix: " IO list.csv", build: io_list, missing_why: crate::plc::iolist::NO_CARDS },
];

/// Brief 051's PLC racks list, when the project has a rack or a card.
fn plc_racks(doc: &Document, o: &Outputs) -> Result<Option<Table>, String> {
    Ok(crate::plc::sheets::rack_csv(doc, &o.plc))
}

/// Brief 045's I/O list, when the project has a card with channels.
fn io_list(doc: &Document, o: &Outputs) -> Result<Option<Table>, String> {
    Ok(crate::plc::iolist::csv_table(doc, &o.plc))
}

/// Whether `slot` gives a list for `doc` (the check's Writes line, before anything is built):
/// brief 028's lists only when the project has strips or cables.
fn slot_gives(slot: &Slot, doc: &Document) -> bool {
    match slot.id {
        "terminal-plan" => doc.components.keys().any(|c| crate::strips::is_strip(doc, *c)),
        "cable-schedule" => !doc.cables.is_empty(),
        "io-list" => doc.components.keys().any(|c| crate::plc::card_type_of(doc, *c).is_some_and(|t| t.has_channels())),
        "plc-racks" => doc.components.keys().any(|c| crate::plc::card_type_of(doc, *c).is_some()),
        _ => slot.missing_why.is_empty(),
    }
}

/// Every slot id `outputs` may name.
pub fn slot_ids() -> Vec<&'static str> {
    let mut v = vec!["book"];
    v.extend(LISTS.iter().map(|s| s.id));
    v.extend(["checks", "manifest"]);
    v
}

/// The lists chosen by `outputs` (default all).
///
/// # Errors
/// An unknown slot id.
fn chosen(params: &Value) -> Result<Option<BTreeSet<String>>, String> {
    match params.get("outputs") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(a)) => {
            let ids = slot_ids();
            let mut out = BTreeSet::new();
            for v in a.iter().take(64) {
                let t = v.as_str().map(str::trim).ok_or("`outputs` is a list of slot names")?;
                if !ids.contains(&t) {
                    return Err(format!("`outputs` names {t:?}; the slots are {}.", ids.join(", ")));
                }
                out.insert(t.to_string());
            }
            Ok(Some(out))
        }
        Some(_) => Err("`outputs` is a list of slot names".into()),
    }
}

// ---------------------------------------------------------------- names and places (§7.1)

/// The file stem: the drawing number, else the project file's name, else the project's name;
/// `\ / : * ? " < > |` and control characters become `-`; at most [`MAX_STEM`] characters.
pub fn stem(p: &Project) -> String {
    let doc = p.ledger.document();
    let raw = doc
        .properties
        .get(crate::frame::DRAWING_NUMBER)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| p.path.as_deref().and_then(|x| Path::new(x).file_stem()).map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_else(|| p.name.clone());
    clean_stem(&raw)
}

/// [`stem`]'s cleaning.
pub fn clean_stem(raw: &str) -> String {
    let s: String = raw.chars().map(|c| if c.is_control() || "\\/:*?\"<>|".contains(c) { '-' } else { c }).take(MAX_STEM).collect();
    let s = s.trim().trim_end_matches('.').trim().to_string();
    if s.is_empty() { "project".into() } else { s }
}

/// `Rev B`.
pub fn folder_name(rev: &str) -> String {
    revisions::version_name(rev)
}

/// The project file's folder, when it has a file.
fn project_dir(p: &Project) -> Option<PathBuf> {
    let path = p.path.as_deref()?;
    Path::new(path).parent().map(Path::to_path_buf)
}

/// The default folder of `rev`: `<project folder>/release/Rev B`.
pub fn default_folder(p: &Project, rev: &str) -> Option<PathBuf> {
    Some(project_dir(p)?.join(RELEASE_DIR).join(folder_name(rev)))
}

/// FP1's value for `folder`: relative to the project file's folder (with `/`), absolute when it
/// is elsewhere (another drive).
pub fn recorded(p: &Project, folder: &Path) -> String {
    if let Some(dir) = project_dir(p)
        && let Ok(rel) = folder.strip_prefix(&dir)
    {
        return rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/");
    }
    folder.to_string_lossy().into_owned()
}

/// FP1's value read back as a folder, only when it is safe to write there again (review of
/// PR #54, 1): a relative value must name a folder inside `<project folder>/release/`; an
/// absolute one must hold a `release.json` (a folder a release wrote). Anything else (`""`, `"."`,
/// a foreign folder, a hostile file) gives `None`, and callers fall back to [`default_folder`].
pub fn from_recorded(p: &Project, v: &str) -> Option<PathBuf> {
    let path = Path::new(v);
    if path.is_absolute() {
        return (path.join(MANIFEST).is_file() && check_target(p, path).is_ok()).then(|| path.to_path_buf());
    }
    // Recorded with `/` on every platform; joined part by part, so the path reads natively.
    let dir = project_dir(p)?;
    let folder = v.split('/').filter(|c| !c.is_empty() && *c != "." && *c != "..").fold(dir.clone(), |d, c| d.join(c));
    let releases = lexical(&dir.join(RELEASE_DIR));
    let f = lexical(&folder);
    (same_or_below(&f, &releases) && !same_path(&f, &releases) && check_target(p, &folder).is_ok()).then_some(folder)
}

/// `path` with `.` dropped and `..` taken back, without touching the disk.
fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// The parts of a path to compare: case-insensitive where the file system is (Windows, macOS).
fn parts(path: &Path) -> Vec<String> {
    let fold = cfg!(any(target_os = "windows", target_os = "macos"));
    path.components()
        .filter(|c| !matches!(c, std::path::Component::CurDir))
        .map(|c| {
            let t = c.as_os_str().to_string_lossy().replace('\\', "/");
            if fold { t.to_lowercase() } else { t }
        })
        .collect()
}

fn same_path(a: &Path, b: &Path) -> bool {
    parts(a) == parts(b)
}

/// Whether `a` is `b` or lies inside it.
fn same_or_below(a: &Path, b: &Path) -> bool {
    let (a, b) = (parts(a), parts(b));
    a.len() >= b.len() && a.iter().zip(&b).all(|(x, y)| x == y)
}

/// Refuse a release folder that is the project's own folder, holds the project file, lies above
/// it, or is `release/` itself or `release/replaced/` (review of PR #54, 1): Replace moves the
/// target aside and Keep both writes beside it, so such a target would move the project.
///
/// # Errors
/// In words, with the way out.
pub fn check_target(p: &Project, folder: &Path) -> Result<(), String> {
    let f = lexical(folder);
    let bad = match project_dir(p) {
        Some(dir) => {
            let dir = lexical(&dir);
            same_or_below(&dir, &f) || same_path(&f, &dir.join(RELEASE_DIR)) || same_path(&f, &dir.join(RELEASE_DIR).join(REPLACED_DIR))
        }
        None => false,
    };
    if bad || f.parent().is_none() {
        return Err(format!(
            "{} cannot be a release folder: it holds the project or other releases. Choose a folder inside release, or another folder.",
            shown(folder)
        ));
    }
    Ok(())
}

/// `<folder>.partial`.
pub fn partial_of(folder: &Path) -> PathBuf {
    let mut name = folder.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(PARTIAL);
    folder.with_file_name(name)
}

fn shown(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

// ---------------------------------------------------------------- steps and faults

/// A step of the release sequence (design 027 §5.2) that can fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Writing the staging folder.
    Partial,
    /// The issue transaction.
    Issue,
    Save,
    Manifest,
    Rename,
}

fn injected(p: &mut Project, step: Step) -> Result<(), String> {
    if p.faults.fail_release_at == Some(step) {
        p.faults.fail_release_at = None;
        return Err(format!("injected failure at {step:?}"));
    }
    Ok(())
}

// ---------------------------------------------------------------- files

/// One file written into the folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    pub slot: String,
    pub name: String,
    pub bytes: usize,
    pub sha256: String,
    pub sheets: Option<usize>,
    pub rows: Option<usize>,
    pub flagged: Option<usize>,
}

impl Written {
    fn json(&self, folder: Option<&Path>) -> Value {
        let mut v = json!({"slot": self.slot, "name": self.name, "bytes": self.bytes, "sha256": self.sha256});
        if let Some(f) = folder {
            v["path"] = json!(shown(&f.join(&self.name)));
        }
        if let Some(n) = self.sheets {
            v["sheets"] = json!(n);
        }
        if let Some(n) = self.rows {
            v["rows"] = json!(n);
        }
        if let Some(n) = self.flagged {
            v["flagged"] = json!(n);
        }
        v
    }

    fn manifest(&self) -> Value {
        let mut v = json!({"kind": self.slot, "name": self.name});
        for (k, n) in [("sheets", self.sheets), ("rows", self.rows), ("flagged", self.flagged)] {
            if let Some(n) = n {
                v[k] = json!(n);
            }
        }
        v["bytes"] = json!(self.bytes);
        v["sha256"] = json!(self.sha256);
        v
    }
}

/// Lowercase hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    ecad_file::v2::schema::asset_hash(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// Write `bytes` as `name` in `dir`, recording it.
fn put(dir: &Path, slot: &str, name: String, bytes: &[u8], out: &mut Vec<Written>) -> Result<(), String> {
    std::fs::write(dir.join(&name), bytes).map_err(|e| format!("Can't write to {}: {e}", shown(&dir.join(&name))))?;
    out.push(Written { slot: slot.into(), name, bytes: bytes.len(), sha256: sha256_hex(bytes), sheets: None, rows: None, flagged: None });
    Ok(())
}

/// What a release writes, before the manifest.
#[derive(Debug, Default)]
pub struct Staged {
    pub files: Vec<Written>,
    pub not_included: Vec<(String, String)>,
    pub sheets: usize,
    pub lists: usize,
}

/// The inputs of a build: the state as issued, its report models, its check rows.
pub struct Build<'a> {
    pub doc: &'a Document,
    pub outputs: &'a Outputs,
    pub rows: &'a [Check],
    pub fan: &'a BTreeSet<ecad_model::CableId>,
    pub tol: f64,
    pub options: Options,
    pub stem: String,
    pub rev: String,
    /// Errors accepted with `accept: "errors"`.
    pub accepted: bool,
    pub chosen: Option<BTreeSet<String>>,
}

/// Build every output into `dir` (created fresh; an old one removed first).
///
/// # Errors
/// A projection, plot or write failure, in words.
pub fn stage(b: &Build<'_>, dir: &Path, progress: &mut dyn FnMut(usize, usize) -> bool) -> Result<Staged, String> {
    // An old staging folder (a crash, or a folder left to finish) is cleared only when it is one
    // of ours; a folder of that name made by someone else is never deleted.
    if dir.exists() {
        if !dir.join(STAGING_MARK).exists() {
            return Err(format!("{} exists and was not made by a release; move it, or choose another folder", shown(dir)));
        }
        std::fs::remove_dir_all(dir).map_err(|e| format!("Can't clear {}: {e}", shown(dir)))?;
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("Can't write to {}: {e}", shown(dir)))?;
    std::fs::write(dir.join(STAGING_MARK), b"").map_err(|e| format!("Can't write to {}: {e}", shown(dir)))?;
    let mut st = Staged::default();
    let base = format!("{} Rev {}", b.stem, b.rev);
    // The book: every page, generated ones included (Print Book, design 024 §4).
    let pages = printbook::resolve(b.doc, &printbook::Scope::All)?.pages;
    let done = printbook::print(b.doc, &pages, b.fan, b.tol, Some(b.outputs), b.options, None, progress)?;
    st.sheets = done.sheets;
    for (i, (_, sheets, _, data)) in done.files.iter().enumerate() {
        let name = if i == 0 { format!("{base}.pdf") } else { format!("{base}-{}.pdf", i + 1) };
        put(dir, "book", name, data.as_deref().unwrap_or_default(), &mut st.files)?;
        if let Some(w) = st.files.last_mut() {
            w.sheets = Some(*sheets);
        }
    }
    for slot in &LISTS {
        if b.chosen.as_ref().is_some_and(|c| !c.contains(slot.id)) {
            st.not_included.push((slot.id.into(), "Not chosen.".into()));
            continue;
        }
        match (slot.build)(b.doc, b.outputs)? {
            Some(t) => {
                put(dir, slot.id, format!("{base}{}", slot.suffix), outputs::csv(&t, true).as_bytes(), &mut st.files)?;
                if let Some(w) = st.files.last_mut() {
                    w.rows = Some(t.rows.len());
                    w.flagged = Some(t.flagged());
                }
                st.lists += 1;
            }
            None => st.not_included.push((slot.id.into(), slot.missing_why.into())),
        }
    }
    let t = checks_table(b.doc, b.rows, b.accepted);
    put(dir, "checks", format!("{base} checks.csv"), outputs::csv(&t, true).as_bytes(), &mut st.files)?;
    if let Some(w) = st.files.last_mut() {
        w.rows = Some(t.rows.len());
    }
    Ok(st)
}

/// `checks.csv` (design 027 §7.2): `Severity, Group, Kind, Where, What, Accepted`.
pub fn checks_table(doc: &Document, rows: &[Check], accepted: bool) -> Table {
    let cell = |t: &str| outputs::Cell { text: t.to_string(), flag: None };
    let mut out: Vec<(Group, Vec<outputs::Cell>)> = rows
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

// ---------------------------------------------------------------- the existing folder (§9)

/// What to do with a folder already there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Existing {
    AddMissing,
    Replace,
    KeepBoth,
}

impl Existing {
    fn parse(params: &Value) -> Result<Option<Existing>, String> {
        match params.get("existing") {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(t)) => match t.trim() {
                "add-missing" => Ok(Some(Existing::AddMissing)),
                "replace" => Ok(Some(Existing::Replace)),
                "keep-both" => Ok(Some(Existing::KeepBoth)),
                other => Err(format!("`existing` is \"add-missing\", \"replace\" or \"keep-both\", not {other:?}")),
            },
            Some(_) => Err("`existing` is \"add-missing\", \"replace\" or \"keep-both\"".into()),
        }
    }
}

/// An existing folder's manifest, read back (bounded).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OldFolder {
    /// Whether it holds a readable `release.json`.
    pub manifest: bool,
    pub written: Option<String>,
    pub app: Option<String>,
    /// File name → SHA-256.
    pub files: BTreeMap<String, String>,
    /// The whole manifest, for Add missing.
    pub raw: Value,
}

/// Read `folder`'s manifest, if any.
pub fn read_old(folder: &Path) -> OldFolder {
    let path = folder.join(MANIFEST);
    let small = std::fs::metadata(&path).is_ok_and(|m| m.len() <= MAX_MANIFEST_BYTES);
    let Some(raw) = small.then(|| std::fs::read(&path).ok()).flatten().and_then(|b| serde_json::from_slice::<Value>(&b).ok()) else {
        return OldFolder::default();
    };
    let files = raw
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(10_000)
        .filter_map(|f| Some((f.get("name")?.as_str()?.to_string(), f.get("sha256")?.as_str()?.to_string())))
        .collect();
    OldFolder {
        manifest: true,
        written: raw.get("written").and_then(Value::as_str).map(str::to_string),
        app: raw.get("app").and_then(Value::as_str).map(str::to_string),
        files,
        raw,
    }
}

/// The staged files against an old folder.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Compare {
    pub same: Vec<String>,
    pub new: Vec<String>,
    pub different: Vec<String>,
}

impl Compare {
    pub fn of(old: &OldFolder, staged: &[Written]) -> Compare {
        let mut c = Compare::default();
        for w in staged {
            match old.files.get(&w.name) {
                None => c.new.push(w.name.clone()),
                Some(h) if *h == w.sha256 => c.same.push(w.name.clone()),
                Some(_) => c.different.push(w.name.clone()),
            }
        }
        c
    }

    /// `4 files the same, 2 new (terminal plan, cable schedule)`.
    pub fn words(&self, old: &OldFolder) -> String {
        if !old.manifest {
            return "This folder was not written by a release".into();
        }
        let n = |k: usize, w: &str| if k == 1 { format!("1 file {w}") } else { format!("{k} files {w}") };
        let mut parts = vec![n(self.same.len(), "the same")];
        if !self.new.is_empty() {
            parts.push(format!("{} new ({})", self.new.len(), self.new.join(", ")));
        }
        if !self.different.is_empty() {
            parts.push(format!("{} different ({})", self.different.len(), self.different.join(", ")));
        }
        parts.join(", ")
    }

    pub fn json(&self) -> Value {
        json!({"same": self.same, "new": self.new, "different": self.different})
    }
}

/// `Rev B (2)`, `(3)`… : the first that does not exist.
fn keep_both(folder: &Path) -> Result<PathBuf, String> {
    let base = folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    for i in 2..=MAX_KEEP_BOTH {
        let f = folder.with_file_name(format!("{base} ({i})"));
        if !f.exists() && !partial_of(&f).exists() {
            return Ok(f);
        }
    }
    Err(format!("Too many folders named {base}; choose another folder."))
}

/// `release/replaced/Rev B 2026-11-12 14-30` (its written time, else now), made unique.
fn replaced_place(folder: &Path, old: &OldFolder) -> PathBuf {
    let base = folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let when = old.written.as_deref().and_then(|w| chrono::DateTime::parse_from_rfc3339(w).ok()).map(|t| t.with_timezone(&chrono::Utc));
    let when = when.or_else(|| crate::project::now_ms().and_then(|ms| chrono::DateTime::from_timestamp_millis(i64::try_from(ms).ok()?)));
    let stamp = when.map(|t| t.format("%Y-%m-%d %H-%M").to_string()).unwrap_or_else(|| "earlier".into());
    let dir = folder.parent().map_or_else(|| PathBuf::from(REPLACED_DIR), |p| p.join(REPLACED_DIR));
    let first = dir.join(format!("{base} {stamp}"));
    if !first.exists() {
        return first;
    }
    (2..=MAX_KEEP_BOTH).map(|i| dir.join(format!("{base} {stamp} ({i})"))).find(|f| !f.exists()).unwrap_or(first)
}

/// Where the staged folder goes, worked out before anything is recorded.
#[derive(Debug, Clone)]
struct Landing {
    /// The final folder.
    folder: PathBuf,
    /// The old folder to move aside first, and where.
    move_old: Option<(PathBuf, PathBuf)>,
    /// Add the missing staged files to the old folder instead of renaming.
    add_missing: Option<OldFolder>,
    compare: Option<Compare>,
}

/// Decide what an existing `folder` means (design 027 §9).
///
/// # Errors
/// A folder there and no `existing`, or Add missing when files differ: with the ways forward.
fn landing(folder: &Path, staged: &Staged, existing: Option<Existing>) -> Result<Landing, String> {
    if !folder.exists() {
        return Ok(Landing { folder: folder.to_path_buf(), move_old: None, add_missing: None, compare: None });
    }
    let old = read_old(folder);
    let cmp = Compare::of(&old, &staged.files);
    match existing {
        None => Err(format!(
            "{} exists ({}). Give existing: \"add-missing\", \"replace\" (the old folder is moved to {}) or \"keep-both\".",
            shown(folder),
            cmp.words(&old),
            shown(&folder.parent().map_or_else(|| PathBuf::from(REPLACED_DIR), |p| p.join(REPLACED_DIR)))
        )),
        Some(Existing::AddMissing) if !old.manifest || !cmp.different.is_empty() => {
            Err(format!("{} cannot only be completed ({}). Give existing: \"replace\" or \"keep-both\".", shown(folder), cmp.words(&old)))
        }
        Some(Existing::AddMissing) => Ok(Landing { folder: folder.to_path_buf(), move_old: None, add_missing: Some(old), compare: Some(cmp) }),
        Some(Existing::Replace) => {
            let to = replaced_place(folder, &old);
            Ok(Landing { folder: folder.to_path_buf(), move_old: Some((folder.to_path_buf(), to)), add_missing: None, compare: Some(cmp) })
        }
        Some(Existing::KeepBoth) => Ok(Landing { folder: keep_both(folder)?, move_old: None, add_missing: None, compare: Some(cmp) }),
    }
}

/// Put the staged folder in place (step 7).
fn land(staging: &Path, l: &Landing, staged: &Staged, manifest: &Value) -> Result<(), String> {
    if let Some(old) = &l.add_missing {
        let missing: Vec<&Written> = staged.files.iter().filter(|w| !old.files.contains_key(&w.name)).collect();
        for w in &missing {
            std::fs::copy(staging.join(&w.name), l.folder.join(&w.name)).map_err(|e| format!("{}: {e}", shown(&l.folder.join(&w.name))))?;
        }
        let mut m = old.raw.clone();
        if let Some(files) = m.get_mut("files").and_then(Value::as_array_mut) {
            files.extend(missing.iter().map(|w| w.manifest()));
        }
        let keep: BTreeSet<&str> = staged.files.iter().map(|w| w.slot.as_str()).collect();
        if let Some(ni) = m.get_mut("not_included").and_then(Value::as_array_mut) {
            ni.retain(|x| x.get("kind").and_then(Value::as_str).is_none_or(|k| !keep.contains(k)));
        }
        m["completed"] = manifest.get("written").cloned().unwrap_or(Value::Null);
        let bytes = serde_json::to_vec_pretty(&m).map_err(|e| e.to_string())?;
        std::fs::write(l.folder.join(MANIFEST), bytes).map_err(|e| format!("{}: {e}", shown(&l.folder.join(MANIFEST))))?;
        std::fs::remove_dir_all(staging).map_err(|e| format!("{}: {e}", shown(staging)))?;
        return Ok(());
    }
    if let Some((from, to)) = &l.move_old {
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", shown(parent)))?;
        }
        std::fs::rename(from, to).map_err(|e| format!("could not move {} aside: {e}", shown(from)))?;
    }
    let _ = std::fs::remove_file(staging.join(STAGING_MARK));
    std::fs::rename(staging, &l.folder).map_err(|e| {
        // Still ours: a release of this revision again may clear it.
        let _ = std::fs::write(staging.join(STAGING_MARK), b"");
        format!("could not rename {}: {e}", shown(staging))
    })
}

/// Remove a staging folder after a failure, only when it is ours.
fn clear_staging(dir: &Path) {
    if dir.join(STAGING_MARK).exists() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

// ---------------------------------------------------------------- the manifest (FP2)

/// What the manifest says of the issue.
#[derive(Debug, Clone, Default)]
pub struct Issued {
    pub branch: String,
    pub transaction: u64,
    pub point: u64,
    pub chain: Option<String>,
}

/// The manifest (design 027 §2, FP2).
pub fn manifest(
    doc: &Document, row: &revisions::Revision, issued: &Issued, b: &Build<'_>, st: &Staged, rows: &[Check], fan_tags: &[String],
) -> Value {
    let prop = |k: &str| doc.properties.get(k).cloned().unwrap_or_default();
    let (e, w, i) = counts(rows);
    let mut iss = json!({"branch": issued.branch, "transaction": issued.transaction, "point": issued.point});
    if let Some(c) = &issued.chain {
        iss["chain"] = json!(c);
    }
    let views: Map<String, Value> = fan_tags.iter().map(|t| (t.clone(), json!("fan"))).collect();
    json!({
        "ecad_release": MANIFEST_VERSION,
        "project": b.stem.clone(),
        "drawing_number": prop(crate::frame::DRAWING_NUMBER),
        "title": prop(crate::frame::TITLE),
        "rev": row.rev(),
        "date": row.get("date"),
        "description": row.get("description"),
        "drawn": row.get("drawn"),
        "checked": row.get("checked"),
        "approved": row.get("approved"),
        "issued": iss,
        "written": now_utc().unwrap_or_default(),
        "app": app(),
        "options": {
            "colour": if b.options.mono { "mono" } else { "screen" },
            "lineweights": b.options.lineweights,
            "excel_safe": true,
            "bom_group": "location",
            "cable_views": views,
        },
        "files": st.files.iter().map(Written::manifest).collect::<Vec<_>>(),
        "not_included": st.not_included.iter().map(|(k, why)| json!({"kind": k, "why": why})).collect::<Vec<_>>(),
        "checks": {"errors": e, "warnings": w, "info": i, "accepted_errors": b.accepted && e > 0},
    })
}

/// `ECAD 0.1.0-dev (CADCraft 227e3c3)`.
pub fn app() -> String {
    format!("ECAD {} (CADCraft {})", env!("CARGO_PKG_VERSION"), crate::UPSTREAM_COMMIT.get(..7).unwrap_or_default())
}

/// Now, UTC, `2026-11-12T14:30:05Z` (through `chrono`).
pub fn now_utc() -> Option<String> {
    let ms = crate::project::now_ms()?;
    Some(chrono::DateTime::from_timestamp_millis(i64::try_from(ms).ok()?)?.format("%Y-%m-%dT%H:%M:%SZ").to_string())
}

// ---------------------------------------------------------------- refusals and warnings

/// Why the pending revision cannot be released now, with the way out (design 027 §6); `None`
/// when it can.
pub fn pending_refusal(p: &Project, rows: &[Check], params: &Value) -> Option<String> {
    if cfg!(target_arch = "wasm32") {
        return Some(NO_FILES_HERE.into());
    }
    if p.path.is_none() {
        return Some(SAVE_FIRST.into());
    }
    if let Some(why) = read_only(p) {
        return Some(format!(
            "The project is read-only ({why}), so the pending revision cannot be issued. Issued revisions can still be written out: give rev."
        ));
    }
    if let Err(e) = crate::sheet_cmds::plan_issue(p, &json!({})) {
        return Some(e);
    }
    // With Finalise chosen, the errors that count are those of the finalised state, the rows the
    // folder ships (review of PR #54, 2): checked by `release_pending` once it has them.
    if !finalising(params)
        && params.get("accept").and_then(Value::as_str) != Some("errors")
        && let Some(r) = errors_refusal(p.ledger.document(), rows)
    {
        return Some(r);
    }
    numbering_seam::refusal(p, rows, params)
}

/// Whether `params` choose 026's Finalise numbering.
fn finalising(params: &Value) -> bool {
    params.get("provisional").and_then(Value::as_str).is_some_and(|w| matches!(w.trim().to_ascii_lowercase().as_str(), "finalise" | "finalize"))
}

/// The refusal for errors in `rows` without `accept: "errors"` (design 027 §4.3), placed in `doc`.
fn errors_refusal(doc: &Document, rows: &[Check]) -> Option<String> {
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

/// Why the project cannot be written, if it cannot.
fn read_only(p: &Project) -> Option<String> {
    if p.persistence == crate::project::Persistence::Unavailable {
        return Some("saving is not available".into());
    }
    p.file_status().read_only
}

/// `Nothing has changed since Rev A.` when every step since the last issue only edited revision
/// fields (design 027 §6, Q11): a warning, never a refusal. Stops at the first other step.
pub fn nothing_changed(p: &Project) -> Option<String> {
    let doc = p.ledger.document();
    let last = revisions::issued(doc).into_iter().next()?;
    let name = last.issued.clone()?;
    let at = *doc.versions.get(&name)?;
    let head = p.ledger.head();
    let only_revisions = |t: &ecad_model::Transaction| {
        t.ops.iter().all(|o| matches!(o, Op::SetProperty { object: ObjectRef::Project, key, .. } if key.starts_with(revisions::PREFIX)))
    };
    let mut seq = at.saturating_add(2);
    while seq <= head {
        let t = p.ledger.transaction(seq)?;
        if !only_revisions(t) {
            return None;
        }
        seq += 1;
    }
    Some(format!("Nothing has changed since {name}."))
}

// ---------------------------------------------------------------- prerelease

/// What the Release dialog and `prerelease` show (design 027 §4, §5.1, §8): the check of the
/// state to be issued, grouped, with each row's place in words, and what a release would write
/// or refuse.
#[derive(Debug, Clone, Default)]
pub struct View {
    pub rev: String,
    /// `None` for the pending revision; the issued rev written again otherwise.
    pub issued: Option<String>,
    /// Each group with rows, in [`Group::ALL`] order: its rows and their `where` texts.
    pub groups: Vec<(Group, Vec<(Check, String)>)>,
    pub errors: usize,
    pub warnings: usize,
    pub info: usize,
    pub provisional: usize,
    pub folder: Option<PathBuf>,
    /// The folder exists: its manifest read back.
    pub existing: Option<OldFolder>,
    /// (slot, file) written.
    pub writes: Vec<(String, String)>,
    /// (slot, why) not written.
    pub not_included: Vec<(String, String)>,
    /// What `release` would refuse (with `accept` as given).
    pub refusal: Option<String>,
    pub warning: Option<String>,
    /// The pending revision's fields (`revisions::FIELDS` order).
    pub fields: [String; 6],
    /// The rows as JSON (`checks`' form), for `prerelease`.
    json_rows: Vec<Vec<Value>>,
}

/// The [`View`] of `rev` (an issued rev, or `None` for the pending one); `accept` as the dialog's
/// tick. Reads the kept Checks and report models for the pending revision (no rescan); builds an
/// issued revision's state and models on request.
///
/// # Errors
/// An issued `rev` that is not on this line, or its state could not be read.
pub fn view(p: &mut Project, rev: Option<&str>, accept: bool, limit: usize) -> Result<View, String> {
    let issued_rev = rev.map(|r| issued_row(p, r)).transpose()?;
    let mut v = View::default();
    let to_rows = |doc: &Document, rows: &[Check], v: &mut View| {
        let groups = grouped(rows);
        for g in Group::ALL {
            if let Some(r) = groups.get(&g) {
                v.groups.push((g, r.iter().map(|c| ((*c).clone(), c.locator(doc))).collect()));
                v.json_rows.push(r.iter().take(limit).map(|c| c.to_json(doc)).collect());
            }
        }
        (v.errors, v.warnings, v.info) = counts(rows);
        v.provisional = numbering_seam::provisional_count(rows);
    };
    match &issued_rev {
        Some(row) => {
            let (_, _, then) = crate::history::issued_state(p, row.rev())?;
            let o = Outputs::build(&then);
            let rows = checks::all_with(&then, p.tolerance, &o);
            to_rows(&then, &rows, &mut v);
            v.rev = row.rev().to_string();
            v.issued = Some(v.rev.clone());
            v.fields = row.fields.clone();
            v.folder = row.released.as_deref().and_then(|x| from_recorded(p, x)).or_else(|| default_folder(p, &v.rev));
        }
        None => {
            let rows = check_rows(p);
            let params = if accept { json!({"accept": "errors"}) } else { json!({}) };
            v.refusal = pending_refusal(p, &rows, &params);
            v.warning = nothing_changed(p);
            let doc = p.ledger.document();
            to_rows(doc, &rows, &mut v);
            v.rev = revisions::pending_rev(doc);
            v.fields = revisions::pending(doc).map(|r| r.fields).unwrap_or_default();
            if v.fields[0].is_empty() {
                v.fields[0] = v.rev.clone();
            }
            v.folder = default_folder(p, &v.rev);
        }
    }
    let base = format!("{} Rev {}", stem(p), v.rev);
    v.writes.push(("book".into(), format!("{base}.pdf")));
    let doc = p.ledger.document();
    for s in &LISTS {
        if slot_gives(s, doc) {
            v.writes.push((s.id.into(), format!("{base}{}", s.suffix)));
        } else {
            v.not_included.push((s.id.into(), s.missing_why.into()));
        }
    }
    v.writes.push(("checks".into(), format!("{base} checks.csv")));
    v.writes.push(("manifest".into(), MANIFEST.into()));
    v.existing = v.folder.as_deref().filter(|f| f.exists()).map(read_old);
    Ok(v)
}

impl View {
    /// `prerelease`'s JSON.
    pub fn json(&self) -> Value {
        let mut v = json!({
            "rev": self.rev, "state": if self.issued.is_some() { "issued" } else { "pending" },
            "groups": self.groups.iter().zip(&self.json_rows).map(|((g, r), rows)| json!({
                "group": g.id(), "words": g.words(), "count": r.len(), "rows": rows,
            })).collect::<Vec<_>>(),
            "errors": self.errors, "warnings": self.warnings, "info": self.info,
            "provisional": self.provisional,
            "folder": self.folder.as_deref().map(shown),
            "exists": self.existing.is_some(),
            "writes": self.writes.iter().map(|(s, f)| json!({"slot": s, "file": f})).collect::<Vec<_>>(),
            "not_included": self.not_included.iter().map(|(s, w)| json!({"slot": s, "why": w})).collect::<Vec<_>>(),
        });
        if let Some(old) = &self.existing {
            v["existing"] = json!({"manifest": old.manifest, "written": old.written, "app": old.app, "files": old.files.len()});
        }
        if let Some(r) = &self.refusal {
            v["refusal"] = json!(r);
        }
        if let Some(w) = &self.warning {
            v["warning"] = json!(w);
        }
        v
    }

    /// The dialog's line about an existing folder (design 027 §5.1).
    pub fn exists_words(&self) -> Option<String> {
        let old = self.existing.as_ref()?;
        let rev = &self.rev;
        Some(match (&old.written, old.manifest) {
            (Some(w), true) => {
                format!("Rev {rev}'s folder exists (written {}). Release asks what to do with it.", w.replacen('T', " ", 1).trim_end_matches('Z'))
            }
            _ => format!("Rev {rev}'s folder exists (not written by a release). Release asks what to do with it."),
        })
    }
}

/// `prerelease {rev?, limit?}` (design 027 §10): the check, grouped, and what `release` would
/// write or refuse. Records nothing.
///
/// # Errors
/// An issued `rev` that is not on this line, or its state could not be read.
pub fn prerelease(p: &mut Project, params: &Value) -> Result<Value, String> {
    let limit = opt_typed(params, "limit", "a whole number", Value::as_u64)?.map_or(DEFAULT_ROWS, |n| n as usize).clamp(1, MAX_ROWS);
    let want = opt_typed(params, "rev", "text such as \"A\"", |v| v.as_str().map(str::to_string))?;
    let accept = params.get("accept").and_then(Value::as_str) == Some("errors");
    Ok(view(p, want.as_deref(), accept, limit)?.json())
}

/// What the Release dialog's view of the pending revision depends on (design 027 §11.1, review of
/// PR #54, 3): the rows (only when some row changed, `Checks::changed` and the report models'
/// check key), the pending revision's fields, the refusal's inputs (file, read-only, branch,
/// provisional items) and the nothing-changed warning, with the tick. An edit that changes none of
/// these leaves it, and the dialog takes no new view. `None` until the Checks are kept.
pub fn dialog_key(p: &Project, accept: bool) -> Option<u64> {
    use std::hash::{Hash, Hasher};
    let checks = p.checks.as_ref()?;
    let outputs = p.outputs.as_ref()?;
    let doc = p.ledger.document();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (checks.changed, outputs.checks_key(), accept).hash(&mut h);
    revisions::pending(doc).map(|r| r.fields).hash(&mut h);
    revisions::issued(doc).len().hash(&mut h);
    (&p.path, read_only(p), crate::sheet_cmds::release_line_refusal(p, true), nothing_changed(p)).hash(&mut h);
    Some(h.finish())
}

/// The issued row `rev` names.
///
/// # Errors
/// It is the pending revision, or was never issued: with the way on.
fn issued_row(p: &Project, rev: &str) -> Result<revisions::Revision, String> {
    let rev = rev.trim().trim_start_matches("Rev ").trim();
    let doc = p.ledger.document();
    if let Some(r) = revisions::issued(doc).into_iter().find(|r| r.rev().eq_ignore_ascii_case(rev)) {
        return Ok(r);
    }
    if revisions::pending_rev(doc).eq_ignore_ascii_case(rev) {
        return Err(format!("Rev {rev} is not issued yet; release without `rev` to issue it and write its folder."));
    }
    Err(format!("Rev {rev} has not been issued."))
}

// ---------------------------------------------------------------- release

fn options(doc: &ecad_model::Document, params: &Value) -> Result<Options, String> {
    // Brief 040: no `colour` given prints as the project's `print.colour` says (mono by default).
    let mono = crate::defaults::print::mono_param(doc, params)?;
    let lineweights = opt_typed(params, "lineweights", "true or false", Value::as_bool)?.unwrap_or(true);
    Ok(Options { mono, lineweights, pending: false })
}

/// The tags of the cables shown fan (the manifest's `cable_views`).
fn fan_tags(doc: &Document, fan: &BTreeSet<ecad_model::CableId>) -> Vec<String> {
    fan.iter().filter(|k| doc.cables.contains_key(k)).map(|k| crate::views::cable_title(doc, *k)).collect()
}

/// The words of a finished release.
fn done_words(rev: &str, folder: &Path, st: &Staged, rows: &[Check]) -> String {
    let (e, w, _) = counts(rows);
    let sheets = if st.sheets == 1 { "1 sheet".to_string() } else { format!("{} sheets", st.sheets) };
    let lists = if st.lists == 1 { "1 list".to_string() } else { format!("{} lists", st.lists) };
    let errs = match e {
        0 => String::new(),
        1 => "1 error and ".into(),
        n => format!("{n} errors and "),
    };
    let warns = if w == 1 { "1 warning".to_string() } else { format!("{w} warnings") };
    format!("Rev {rev} released to {}: {sheets}, {lists}, {errs}{warns} listed.", shown(folder))
}

/// The folder `params` names (`folder`), else `default`.
fn folder_param(p: &Project, params: &Value, default: Option<PathBuf>) -> Result<PathBuf, String> {
    match opt_typed(params, "folder", "a folder path", |v| v.as_str().map(str::to_string))? {
        Some(f) if !f.trim().is_empty() => {
            let path = PathBuf::from(f.trim());
            Ok(if path.is_absolute() { path } else { project_dir(p).map_or(path.clone(), |d| d.join(&path)) })
        }
        _ => default.ok_or_else(|| SAVE_FIRST.to_string()),
    }
    .and_then(|f| check_target(p, &f).map(|()| f))
}

/// `release` of the pending revision (design 027 §5.2).
///
/// # Errors
/// A refusal of §6 or a failure before the issue (nothing recorded, no `.partial` left); after
/// the issue, a finished result carries the problem in its message instead.
pub(crate) fn release_pending(
    p: &mut Project, pg: &mut Pages<'_>, params: &Value, progress: &mut dyn FnMut(usize, usize) -> bool,
) -> Result<Value, String> {
    // 1. Refusals.
    let chosen = chosen(params)?;
    let existing = Existing::parse(params)?;
    let o = options(p.ledger.document(), params)?;
    let rows = check_rows(p);
    if let Some(r) = pending_refusal(p, &rows, params) {
        return Err(r);
    }
    let accepted = params.get("accept").and_then(Value::as_str) == Some("errors");
    let rev0 = revisions::pending_rev(p.ledger.document());
    let target = folder_param(p, params, default_folder(p, &rev0))?;
    let warning = nothing_changed(p);
    // 2. 026's finalise, when asked: planned now, recorded only with the issue (step 4), so a
    // release that stops before the issue records nothing at all (tester 027, 1).
    let (finalise, note) = match numbering_seam::plan(p, params)? {
        crate::numbering_cmds::BeforeIssue::Go(note) => (None, note),
        crate::numbering_cmds::BeforeIssue::Finalise(plan) => (Some(plan), None),
    };
    // The caller's transaction budget (an agent's cap of one per call, two with Finalise): known
    // before anything reaches the disk, never found out after (tester 027, 3).
    let needed = 1 + usize::from(finalise.is_some());
    if let Some(budget) = p.tx_budget
        && needed > budget
    {
        return Err(over_budget(needed, budget));
    }
    // 3. Build into staging, from the state as it will be issued: Finalise's ops (if any), then
    // the issue's, on the current document.
    let mut plan = crate::sheet_cmds::plan_issue(p, &json!({}))?;
    if finalise.is_some() {
        // The issue names the step before it: Finalise, once recorded.
        plan.head += 1;
        for op in plan.ops.iter_mut() {
            if let Op::NameVersion { at, .. } = op {
                *at = plan.head;
            }
        }
    }
    let rev = plan.rev.clone();
    let target = if rev == rev0 { target } else { folder_param(p, params, default_folder(p, &rev))? };
    let staging = partial_of(&target);
    plan.ops.insert(
        plan.ops.len().saturating_sub(1),
        Op::SetProperty {
            object: ObjectRef::Project,
            key: revisions::key(plan.n, revisions::RELEASED),
            before: None,
            after: Some(recorded(p, &target)),
        },
    );
    let all_ops: Vec<Op> = finalise.iter().flat_map(|f| f.ops.iter().cloned()).chain(plan.ops.iter().cloned()).collect();
    let issued_doc = p.ledger.preview(&all_ops).map_err(|e| e.to_string())?;
    // With Finalise, the lists and the check are those of the finalised state (built on this
    // copy, as for a re-release); otherwise the kept ones (the issue changes no list or check).
    let finalised = finalise.as_ref().map(|_| {
        let o = Outputs::build(&issued_doc);
        let r = checks::all_with(&issued_doc, p.tolerance, &o);
        (o, r)
    });
    let rows = finalised.as_ref().map_or(rows, |(_, r)| r.clone());
    // The errors gate on the rows the folder ships: after Finalise, when chosen.
    if finalised.is_some()
        && !accepted
        && let Some(r) = errors_refusal(&issued_doc, &rows)
    {
        return Err(r);
    }
    let stem = stem(p);
    let staged = {
        let fail = injected(p, Step::Partial);
        p.outputs();
        let kept = p.outputs.as_ref().ok_or("the report models could not be built")?;
        let outputs = finalised.as_ref().map_or(kept, |(o, _)| o);
        let b =
            Build { doc: &issued_doc, outputs, rows: &rows, fan: &p.views, tol: p.tolerance, options: o, stem, rev: rev.clone(), accepted, chosen };
        let built = fail.and_then(|()| stage(&b, &staging, progress));
        match built.and_then(|st| landing(&target, &st, existing).map(|l| (st, l))) {
            Ok(x) => (x, b.stem.clone(), b.options),
            Err(e) => {
                clear_staging(&staging);
                return Err(stopped(&e));
            }
        }
    };
    let ((st, mut l), stem, o) = staged;
    // Keep both lands elsewhere: FP1 names the folder it lands in.
    if l.folder != target {
        for op in plan.ops.iter_mut() {
            if let Op::SetProperty { key, after, .. } = op
                && *key == revisions::key(plan.n, revisions::RELEASED)
            {
                *after = Some(recorded(p, &l.folder));
            }
        }
    }
    // 4. Finalise (when asked), then the issue. An issue that fails takes Finalise back with it.
    let mut finalise_tx = None;
    if let Some(f) = &finalise {
        if let Err(e) = numbering_seam::commit_finalise(p, pg, f) {
            clear_staging(&staging);
            return Err(stopped(&e));
        }
        finalise_tx = Some(p.ledger.head());
    }
    let issued = injected(p, Step::Issue).and_then(|()| p.apply_edit(pg, "release", &format!("Issue Rev {rev}"), plan.ops.clone()));
    if let Err(e) = issued {
        clear_staging(&staging);
        if finalise_tx.is_some() {
            return Err(match p.undo_step(pg) {
                Ok(()) => format!("Release stopped: {e}. Finalise numbering was undone; nothing was issued or written."),
                Err(u) => format!("Release stopped: {e}. Finalise numbering stays recorded ({u}); nothing was issued or written. Undo it with u."),
            });
        }
        return Err(e);
    }
    let tx = p.ledger.head();
    let mut problems: Vec<String> = Vec::new();
    // 5. Save.
    let saved = injected(p, Step::Save).and_then(|()| p.save_now(pg));
    if let Err(e) = &saved {
        problems.push(format!("The project could not be saved: {e}. Save it before closing, or Rev {rev} is not kept."));
    }
    // 6. Manifest, 7. rename.
    let chain =
        p.file.v2().filter(|_| saved.is_ok()).and_then(|v| v.chain_of(v.branch(), tx).ok()).map(|h| h.iter().map(|b| format!("{b:02x}")).collect());
    let iss = Issued { branch: crate::history::current_name(p), transaction: tx, point: plan.head, chain };
    let doc = p.ledger.document().clone();
    let row = revisions::list(&doc).into_iter().find(|r| r.n == plan.n).unwrap_or_default();
    let outputs = p.outputs.clone().unwrap_or_default();
    let fan = p.views.clone();
    let b =
        Build { doc: &doc, outputs: &outputs, rows: &rows, fan: &fan, tol: p.tolerance, options: o, stem, rev: rev.clone(), accepted, chosen: None };
    let m = manifest(&doc, &row, &iss, &b, &st, &rows, &fan_tags(&doc, &fan));
    let finished = injected(p, Step::Manifest)
        .and_then(|()| write_manifest(&staging, &m))
        .and_then(|()| injected(p, Step::Rename))
        .and_then(|()| land(&staging, &l, &st, &m));
    let landed = match finished {
        Ok(()) => true,
        Err(e) => {
            problems.push(format!(
                "Rev {rev} is issued, but its folder could not be finished ({e}). The files are in {}; release Rev {rev} again to write the folder.",
                shown(&staging)
            ));
            false
        }
    };
    if !landed {
        l.folder = staging.clone();
    }
    let mut message = if landed { done_words(&rev, &l.folder, &st, &rows) } else { format!("Rev {rev} issued.") };
    if let Some(n) = note {
        message.push_str(&format!(" {n}"));
    }
    for pr in &problems {
        message.push(' ');
        message.push_str(pr);
    }
    pg.echo(message.clone());
    let mut transactions: Vec<u64> = finalise_tx.into_iter().collect();
    transactions.push(tx);
    let mut v = json!({
        "rev": rev, "folder": shown(&l.folder), "finished": landed,
        "files": st.files.iter().map(|w| w.json(Some(&l.folder))).collect::<Vec<_>>(),
        "not_included": st.not_included.iter().map(|(k, why)| json!({"slot": k, "why": why})).collect::<Vec<_>>(),
        "transactions": transactions, "saved": saved.is_ok(), "message": message,
    });
    if let Some(c) = &l.compare {
        v["compare"] = c.json();
    }
    if let Some(wn) = warning {
        v["warning"] = json!(wn);
    }
    Ok(v)
}

/// The refusal of a release that would record more transactions than its caller allows.
fn over_budget(needed: usize, budget: usize) -> String {
    let what = if needed == 2 { "Finalise numbering and the issue (2 steps)" } else { "the issue (1 step)" };
    let allowed = match budget {
        0 => "no more steps in this call".to_string(),
        1 => "1 step in this call".to_string(),
        n => format!("{n} steps in this call"),
    };
    format!("Release refused: it records {what}, and the caller allows {allowed}. Nothing was issued or written; call release on its own.")
}

/// The words of a release stopped before the issue.
fn stopped(reason: &str) -> String {
    let r = reason.trim_end_matches('.');
    format!("Release stopped: {r}. Nothing was issued or written. Choose another folder, or try again.")
}

fn write_manifest(dir: &Path, m: &Value) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(m).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(MANIFEST), bytes).map_err(|e| format!("{}: {e}", shown(&dir.join(MANIFEST))))
}

/// `release {rev}`: write an issued revision's folder again from its issued state (design 027
/// §8). Records nothing.
///
/// # Errors
/// Not issued, no place for the folder, a write failure (no `.partial` left), or an existing
/// folder without `existing`.
pub fn release_issued(p: &mut Project, params: &Value, rev: &str, progress: &mut dyn FnMut(usize, usize) -> bool) -> Result<Value, String> {
    if cfg!(target_arch = "wasm32") {
        return Err(NO_FILES_HERE.into());
    }
    let chosen = chosen(params)?;
    let existing = Existing::parse(params)?;
    let o = options(p.ledger.document(), params)?;
    let row = issued_row(p, rev)?;
    let rev = row.rev().to_string();
    let default = row.released.as_deref().and_then(|v| from_recorded(p, v)).or_else(|| default_folder(p, &rev));
    let target = folder_param(p, params, default)?;
    let (named, seq, then) = crate::history::issued_state(p, &rev)?;
    let outputs = Outputs::build(&then);
    let rows = checks::all_with(&then, p.tolerance, &outputs);
    let staging = partial_of(&target);
    let fan = p.views.clone();
    let b = Build {
        doc: &then,
        outputs: &outputs,
        rows: &rows,
        fan: &fan,
        tol: p.tolerance,
        options: o,
        stem: stem(p),
        rev: rev.clone(),
        accepted: false,
        chosen,
    };
    let built =
        injected(p, Step::Partial).and_then(|()| stage(&b, &staging, progress)).and_then(|st| landing(&target, &st, existing).map(|l| (st, l)));
    let (st, l) = match built {
        Ok(x) => x,
        Err(e) => {
            clear_staging(&staging);
            return Err(format!(
                "Rev {rev}'s folder was not written: {}. Nothing was written. Choose another folder, or try again.",
                e.trim_end_matches('.')
            ));
        }
    };
    let branch = crate::history::find_branch(p, &named.branch.hex()).map(|b| b.1.name).unwrap_or_else(|_| "main".into());
    let chain = p.file.v2().and_then(|v| v.chain_of(named.branch, seq).ok()).map(|h| h.iter().map(|b| format!("{b:02x}")).collect());
    let iss = Issued { branch, transaction: seq, point: named.seq, chain };
    let m = manifest(&then, &row, &iss, &b, &st, &rows, &fan_tags(&then, &fan));
    let finished = injected(p, Step::Manifest)
        .and_then(|()| write_manifest(&staging, &m))
        .and_then(|()| injected(p, Step::Rename))
        .and_then(|()| land(&staging, &l, &st, &m));
    if let Err(e) = finished {
        clear_staging(&staging);
        return Err(format!(
            "Rev {rev}'s folder was not written: {}. Nothing was written. Choose another folder, or try again.",
            e.trim_end_matches('.')
        ));
    }
    let message = format!("{} (written again; nothing recorded)", done_words(&rev, &l.folder, &st, &rows).trim_end_matches('.'));
    let mut v = json!({
        "rev": rev, "folder": shown(&l.folder), "finished": true,
        "files": st.files.iter().map(|w| w.json(Some(&l.folder))).collect::<Vec<_>>(),
        "not_included": st.not_included.iter().map(|(k, why)| json!({"slot": k, "why": why})).collect::<Vec<_>>(),
        "transactions": [], "saved": false, "message": format!("{message}."),
    });
    if let Some(c) = &l.compare {
        v["compare"] = c.json();
    }
    Ok(v)
}

// ---------------------------------------------------------------- commands

/// The commands this module adds.
pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("prerelease", "Pre-release Check", run_prerelease)
            .params("{rev?: an issued rev (default the pending one), limit?: rows per group (50, max 2000)} (read only)")
            .noundo(),
        CommandSpec::new("release", "Release", run_release)
            .params(
                "{rev?: an issued rev to write again (records nothing; default: issue the pending one), folder?, outputs?: [slot], \
accept?: \"errors\", provisional?: \"finalise\"|\"issue\", existing?: \"add-missing\"|\"replace\"|\"keep-both\", colour?: \"mono\"|\"screen\", \
lineweights?: bool}",
            )
            .noundo(),
    ]
}

fn run_prerelease(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    with_any_page(s, |p, _| prerelease(p, params))
}

fn run_release(s: &mut Session, params: &Value) -> cadcraft_engine::Result<Value> {
    let rev = params
        .get("rev")
        .filter(|v| !v.is_null())
        .map(|v| v.as_str().map(str::to_string).ok_or_else(|| err("`rev` is text such as \"A\"")))
        .transpose()?;
    match rev {
        Some(r) => {
            let v = with_any_page(s, |p, _| release_issued(p, params, &r, &mut |_, _| true))?;
            if let Some(m) = v.get("message").and_then(Value::as_str) {
                s.echo(m.to_string());
            }
            Ok(v)
        }
        None => with_project(s, |p, pg, _| release_pending(p, pg, params, &mut |_, _| true)),
    }
}
