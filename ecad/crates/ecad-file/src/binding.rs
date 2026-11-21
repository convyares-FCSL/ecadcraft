//! A project's file on disk (persistence stage 2a; container version 2 since brief 021): the
//! binding between a project (ecad-engine's `Project`) and its `.ecad` file, through
//! [`crate::disk::DiskProjectV2`], and the checks on a path typed for one.
//!
//! It reads no environment: the home folder is passed in ([`normalize_with`]).

use crate::ReportV2;
use ecad_model::{Ledger, ProjectId};

/// What Save says when a project has never been saved and no file was chosen.
pub const NO_FILE_YET: &str = "this project has no file yet; use Save As and choose a .ecad file";
/// What Save As says without a file name.
pub const SAVE_AS_NEEDS_PATH: &str = "Save As needs a file name (a .ecad file)";

/// Why a path typed for a project file cannot be used ([`normalize_with`], [`expand_home`]).
///
/// # Semantics
/// The `Display` text says what is wrong and what to type instead; nothing was opened or written.
///
/// # Example
/// ```
/// use ecad_file::binding::{normalize_with, PathError};
/// assert_eq!(normalize_with("plan.dxf", None), Err(PathError::Extension { ext: "dxf".into(), path: "plan.dxf".into() }));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PathError {
    /// Nothing was typed.
    #[error("{SAVE_AS_NEEDS_PATH}")]
    Empty,
    /// A folder, not a file.
    #[error("{SAVE_AS_NEEDS_PATH}, not a folder ({0})")]
    Folder(String),
    /// `C:` or `C:name`: relative to that drive's current folder.
    #[error("{path} is relative to drive {drive}:'s current folder; type a full path such as {drive}:\\dev\\ecad-projects\\demo")]
    DriveRelative { path: String, drive: char },
    /// A leading `~` without a known home folder.
    #[error("{HOME_UNKNOWN}")]
    HomeUnknown,
    /// The expanded path is not valid Unicode.
    #[error("{0} is not a valid Unicode path")]
    NotUnicode(String),
    /// `.`, `..` or `.ecad`: no file name.
    #[error("{SAVE_AS_NEEDS_PATH}, not {name} ({path})")]
    NoName { name: String, path: String },
    /// `:` in the file name.
    #[error("{0} cannot be used: a file name cannot contain ':'")]
    Colon(String),
    /// A trailing dot, which Windows drops.
    #[error("{0} cannot be used: Windows drops a file name's trailing dot; remove it")]
    TrailingDot(String),
    /// A device name Windows reserves (`CON`, `NUL`, `COM1`, …).
    #[error("{name} cannot be used: {device} is a name Windows reserves for a device; choose another")]
    Reserved { name: String, device: String },
    /// An extension other than `.ecad`.
    #[error("ECAD projects are saved as .ecad files, not .{ext} ({path})")]
    Extension { ext: String, path: String },
}

/// Why a project file could not be opened or saved through its [`Binding`].
///
/// # Semantics
/// Nothing more is acknowledged than before: a failed open binds nothing, a failed save keeps the
/// previous binding and the project unsaved.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BindingError {
    /// The file exists but is not an ECAD project file.
    #[error("could not open {0}: it is not an ECAD project file")]
    NotAProject(String),
    /// Save with no file yet.
    #[error("{NO_FILE_YET}")]
    NoFileYet,
    /// Opening or saving on disk failed.
    #[error(transparent)]
    Disk(#[from] crate::disk::DiskErrorV2),
}

/// Where a project's file stands, for status displays.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileStatus {
    /// The bound file.
    pub path: Option<String>,
    /// Why the project cannot be saved to that file (damaged, or another session is editing it).
    pub read_only: Option<String>,
    /// The last Save As replaced its target, but durability was not confirmed; Save confirms it.
    pub unconfirmed: bool,
}

/// The outcome of writing a project to a new file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Written {
    /// Durable and acknowledged.
    Confirmed,
    /// In place, but its durability could not be confirmed: the message says what failed.
    Unconfirmed(String),
}

/// What opening a project file found.
#[derive(Debug)]
pub struct Opened {
    pub binding: Binding,
    pub ledger: Ledger,
    pub report: ReportV2,
    pub project: ProjectId,
    /// A lock file left by a session that ended without releasing it, reclaimed: its record.
    pub reclaimed_lock: Option<String>,
    /// The tag high-water mark at the snapshot the ledger starts from (brief 021).
    pub tag_high: ecad_model::tag::HighWater,
}

/// The file a project is bound to, holding its writer lock while bound.
#[derive(Debug, Default)]
pub struct Binding {
    disk: Option<Box<crate::disk::DiskProjectV2>>,
}

/// What the debugger's Storage view shows of a version 2 file (copies; no file access).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StorageFacts {
    /// The branch this session edits.
    pub branch: String,
    /// The branch head in the file.
    pub head: u64,
    /// The snapshot the next one is built from (the latest written or opened from).
    pub snapshot_seq: u64,
    /// Transactions between automatic snapshots.
    pub snapshot_every: u64,
    /// What the last save wrote: first and last `seq`, bytes, chunks and snapshots written.
    pub last_save: Option<crate::SavedV2>,
    pub writable: bool,
}

impl StorageFacts {
    /// The facts of the project in a file (copies; no file access).
    pub fn of(p: &crate::ProjectV2) -> Self {
        StorageFacts {
            branch: format!("{} ({})", p.branch_record().name, p.branch().hex()),
            head: p.head().0,
            snapshot_seq: p.snapshot_seq(),
            snapshot_every: p.snapshot_every(),
            last_save: p.last_save().cloned(),
            writable: p.read_only().is_none(),
        }
    }
}

/// Refuse a file that exists but is not an ECAD project (a file that cannot be read is left to
/// opening, which says why).
fn not_a_project(path: &str) -> Result<(), BindingError> {
    if crate::disk::starts_like_project(std::path::Path::new(path)).ok() == Some(false) {
        return Err(BindingError::NotAProject(path.to_string()));
    }
    Ok(())
}

/// What a path starting with `~` says when the home folder is not known.
pub const HOME_UNKNOWN: &str = "the home folder is not known, so ~ cannot be used; type the full path";

/// Device names Windows reserves in every folder (`CON.ecad` is the console, not a file). Refused
/// on every platform, so that a project's file name works everywhere.
const RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5",
    "LPT6", "LPT7", "LPT8", "LPT9",
];

/// `path` with a leading `~`, `~/` or `~\` replaced by `home`, the rest joined component by
/// component (split on `/` and `\`) with `Path::join`, so that it uses the platform's separator.
/// Any other path is returned as typed.
///
/// # Errors
/// The path starts with `~` but `home` is `None` (a folder literally named `~` is never made).
pub fn expand_home(path: &str, home: Option<&std::path::Path>) -> Result<std::path::PathBuf, PathError> {
    let rest = match path.strip_prefix('~') {
        Some("") => "",
        Some(r) if r.starts_with(['/', '\\']) => r.trim_start_matches(['/', '\\']),
        _ => return Ok(std::path::PathBuf::from(path)),
    };
    let home = home.ok_or(PathError::HomeUnknown)?;
    Ok(rest.split(['/', '\\']).filter(|c| !c.is_empty()).fold(home.to_path_buf(), |p, c| p.join(c)))
}

/// A `.ecad` path from what the user typed, with the home folder given: surrounding spaces
/// trimmed, a leading `~` expanded to `home`, and `.ecad` added when there is no extension.
///
/// # Errors
/// Empty, no file name (a folder, `.`, `..`, `.ecad`), a drive-relative path (`C:`, `C:name`),
/// `:` in the file name, a trailing dot, `~` without a known home, a reserved device name
/// (`CON.ecad`), or a different extension. A path it returns normalises to itself.
pub fn normalize_with(path: &str, home: Option<&std::path::Path>) -> Result<String, PathError> {
    let mut p = path.trim();
    // Explorer's "Copy as path" puts the path in double quotes.
    if let Some(inner) = p.strip_prefix('"').and_then(|r| r.strip_suffix('"')) {
        p = inner.trim();
    }
    if p.is_empty() {
        return Err(PathError::Empty);
    }
    if p == "~" || p.ends_with(['/', '\\']) {
        return Err(PathError::Folder(p.to_string()));
    }
    let mut chars = p.chars();
    if let (Some(drive), Some(':')) = (chars.next(), chars.next())
        && drive.is_ascii_alphabetic()
        && !chars.next().is_some_and(|c| c == '/' || c == '\\')
    {
        return Err(PathError::DriveRelative { path: p.to_string(), drive });
    }
    let full = expand_home(p, home)?;
    let p = full.to_str().ok_or_else(|| PathError::NotUnicode(full.display().to_string()))?;
    // The last component after either separator, so that a Windows path reads the same everywhere.
    let name = p.rsplit(['/', '\\']).next().unwrap_or(p);
    if name == "." || name == ".." || name.eq_ignore_ascii_case(".ecad") {
        return Err(PathError::NoName { name: name.to_string(), path: p.to_string() });
    }
    if name.contains(':') {
        return Err(PathError::Colon(name.to_string()));
    }
    if name.ends_with('.') {
        return Err(PathError::TrailingDot(name.to_string()));
    }
    let device = name.split('.').next().unwrap_or_default().trim_end().to_ascii_uppercase();
    if RESERVED.contains(&device.as_str()) {
        return Err(PathError::Reserved { name: name.to_string(), device });
    }
    let ext = std::path::Path::new(name).extension().map(|e| e.to_string_lossy().to_ascii_lowercase());
    match ext.as_deref() {
        None => Ok(format!("{p}.ecad")),
        Some("ecad") => Ok(p.to_string()),
        Some(other) => Err(PathError::Extension { ext: other.to_string(), path: p.to_string() }),
    }
}

impl Binding {
    /// Open the project file at `path` (already normalised): take its writer lock and open it
    /// from its latest snapshot.
    ///
    /// # Errors
    /// The file cannot be read or is not a project file this build supports.
    pub fn open(path: &str, seed: u64, writer: &str) -> Result<Opened, BindingError> {
        not_a_project(path)?;
        let o = crate::disk::DiskProjectV2::open(std::path::Path::new(path), seed, writer)?;
        let project = o.project.project().project_id();
        let tag_high = o.project.project().base_tag_high().clone();
        Ok(Opened {
            binding: Binding { disk: Some(Box::new(o.project)) },
            ledger: o.ledger,
            report: o.report,
            project,
            reclaimed_lock: o.reclaimed_lock,
            tag_high,
        })
    }

    /// [`Binding::open`] for reading only: no writer lock, and the file is never changed.
    ///
    /// # Errors
    /// As [`Binding::open`].
    pub fn open_read_only(path: &str, seed: u64, writer: &str) -> Result<Opened, BindingError> {
        not_a_project(path)?;
        let o = crate::disk::DiskProjectV2::open_read_only(std::path::Path::new(path), seed, writer)?;
        let project = o.project.project().project_id();
        let tag_high = o.project.project().base_tag_high().clone();
        Ok(Opened {
            binding: Binding { disk: Some(Box::new(o.project)) },
            ledger: o.ledger,
            report: o.report,
            project,
            reclaimed_lock: o.reclaimed_lock,
            tag_high,
        })
    }

    /// Whether the project has been written to (or opened from) a file.
    pub fn bound(&self) -> bool {
        self.disk.is_some()
    }

    /// Make the opened project read-only for the unknown words found on open
    /// ([`crate::disk::DiskProjectV2::refuse_unknown_words`]).
    pub fn refuse_unknown_words(&mut self, words: Vec<ecad_model::strict::UnknownWord>) {
        if let Some(d) = self.disk.as_mut() {
            d.refuse_unknown_words(words);
        }
    }

    /// Where the file stands.
    pub fn status(&self) -> FileStatus {
        let Some(d) = &self.disk else { return FileStatus::default() };
        FileStatus { path: Some(d.path().display().to_string()), read_only: d.read_only().map(ToString::to_string), unconfirmed: d.unconfirmed() }
    }

    /// Save to the bound file.
    ///
    /// # Errors
    /// No file yet, or the save failed.
    pub fn save(&mut self, ledger: &mut Ledger) -> Result<(), BindingError> {
        let d = self.disk.as_mut().ok_or(BindingError::NoFileYet)?;
        d.save(ledger).map(|_| ()).map_err(BindingError::Disk)
    }

    /// Save As `path` (already normalised), keeping `project`'s ID; binds to it on success.
    ///
    /// # Errors
    /// The file could not be written; the previous binding is kept.
    pub fn save_as(
        &mut self, path: &str, project: ProjectId, ledger: &mut Ledger, writer: &str, created_ms: Option<u64>,
    ) -> Result<Written, BindingError> {
        let p = std::path::Path::new(path);
        let r = match &mut self.disk {
            Some(d) => d.save_as(p, ledger, created_ms, writer),
            None => crate::disk::DiskProjectV2::create(p, project, ledger, None, created_ms, writer).map(|(d, unconfirmed)| {
                self.disk = Some(Box::new(d));
                unconfirmed
            }),
        };
        match r {
            Ok(None) => Ok(Written::Confirmed),
            Ok(Some(e)) => Ok(Written::Unconfirmed(crate::disk::DiskErrorV2::Unconfirmed(e).to_string())),
            Err(e) => Err(BindingError::Disk(e)),
        }
    }

    /// Let go of the file and its writer lock (the project was closed).
    pub fn release(&mut self) {
        self.disk = None;
    }

    /// The writer lock file, while the project holds it.
    pub fn lock_file(&self) -> Option<String> {
        self.disk.as_ref().and_then(|d| d.lock_file()).map(|p| p.display().to_string())
    }

    /// The version 2 file's facts (copies, for the debugger's Storage view; no file access).
    pub fn storage(&self) -> Option<StorageFacts> {
        self.disk.as_ref().map(|d| StorageFacts::of(d.project()))
    }

    /// The bound file's database, for the debugger's on-request reads (table sizes, branches,
    /// versions). Reads only.
    pub fn store(&self) -> Option<std::sync::Arc<crate::Store>> {
        self.disk.as_ref().map(|d| d.project().store().clone())
    }

    /// The project in the bound file (brief 020: its branches and history).
    pub fn v2(&self) -> Option<&crate::ProjectV2> {
        self.disk.as_deref().map(|d| d.project())
    }

    /// The project in the bound file, for branch actions (brief 020).
    pub fn v2_mut(&mut self) -> Option<&mut crate::ProjectV2> {
        self.disk.as_deref_mut().map(|d| d.project_mut())
    }
}
