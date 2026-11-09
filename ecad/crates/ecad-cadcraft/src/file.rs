//! A project's file on disk (persistence stage 2a; container version 2 since brief 021): the
//! binding between a [`crate::Project`] and its `.ecad` file, through
//! `ecad_file::v2::disk::DiskProjectV2`. A version 1 file is never opened for editing: opening one
//! offers to convert it ([`needs_conversion`], [`convert`]). Native only; in a browser build every
//! operation reports that project files are not available.

use ecad_file::v2::ReportV2;
use ecad_model::{Ledger, ProjectId};

/// What Save says when a project has never been saved and no file was chosen.
pub const NO_FILE_YET: &str = "this project has no file yet; use Save As and choose a .ecad file";
/// What Save As says without a file name.
pub const SAVE_AS_NEEDS_PATH: &str = "Save As needs a file name (a .ecad file)";
/// What opening a version 1 file says until it is converted.
pub const NEEDS_CONVERSION: &str = "is a version 1 project file and is not opened for editing; convert it to a new \
version 2 file (the original is left untouched)";

/// Where a project's file stands, for status displays.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileStatus {
    /// The bound file.
    pub path: Option<String>,
    /// Why the project cannot be saved to that file (damaged, or another session is editing it).
    pub read_only: Option<String>,
    /// The last Save As replaced its target, but durability was not confirmed; Save confirms it.
    pub unconfirmed: bool,
    /// Where the interrupted trailing bytes found at open were preserved (version 1 only).
    pub preserved_tail: Option<String>,
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
    #[cfg(not(target_arch = "wasm32"))]
    disk: Option<Box<ecad_file::v2::disk::DiskProjectV2>>,
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
    /// What the last save wrote: first and last `seq`, bytes, parts and snapshots written.
    pub last_save: Option<ecad_file::v2::SavedV2>,
    /// The version 1 history a converted file continues: its final hash and length.
    pub link_v1: Option<(String, u64)>,
    pub writable: bool,
}

/// The new file opening `path` would first convert it to, when `path` is a version 1 file;
/// `None` for anything else (a file that cannot be read included: opening reports that).
pub fn needs_conversion(path: &str) -> Option<String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let p = std::path::Path::new(path);
        if ecad_file::v2::convert::sniff(p).ok()? == ecad_file::v2::convert::Major::V1 {
            return Some(ecad_file::v2::convert::default_output(p).display().to_string());
        }
    }
    #[cfg(target_arch = "wasm32")]
    let _ = path;
    None
}

/// Refuse a file that exists but is neither version of an ECAD project.
fn not_a_project(path: &str) -> Result<(), String> {
    #[cfg(not(target_arch = "wasm32"))]
    if ecad_file::v2::convert::sniff(std::path::Path::new(path)).ok() == Some(ecad_file::v2::convert::Major::Unknown) {
        return Err(format!("could not open {path}: it is not an ECAD project file"));
    }
    #[cfg(target_arch = "wasm32")]
    let _ = path;
    Ok(())
}

/// Convert the version 1 file at `path` to a new version 2 file (`output`, or `<name>.v2.ecad`
/// beside it), verified by replaying both. The report in words, and the new file's path.
///
/// # Errors
/// Why it was not converted (the source is never changed).
pub fn convert(path: &str, output: Option<&str>, writer: &str) -> Result<(String, String), String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let out = output.map(std::path::Path::new);
        let r = ecad_file::v2::convert::convert_file(std::path::Path::new(path), out, writer)?;
        Ok((r.describe(), r.output.display().to_string()))
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (path, output, writer);
        Err(NO_FILES.into())
    }
}

/// What a path starting with `~` says when the home folder is not known.
pub const HOME_UNKNOWN: &str = "the home folder is not known, so ~ cannot be used; type the full path";

/// Device names Windows reserves in every folder (`CON.ecad` is the console, not a file). Refused
/// on every platform, so that a project's file name works everywhere.
const RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2",
    "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// The user's home folder (`std::env::home_dir`: on Windows the user profile, whether or not
/// `HOME` is set). `None` when it is not known.
pub fn home() -> Option<std::path::PathBuf> {
    std::env::home_dir().filter(|h| !h.as_os_str().is_empty())
}

/// `path` with a leading `~`, `~/` or `~\` replaced by `home`, the rest joined component by
/// component (split on `/` and `\`) with `Path::join`, so that it uses the platform's separator.
/// Any other path is returned as typed.
///
/// # Errors
/// The path starts with `~` but `home` is `None` (a folder literally named `~` is never made).
pub fn expand_home(path: &str, home: Option<&std::path::Path>) -> Result<std::path::PathBuf, String> {
    let rest = match path.strip_prefix('~') {
        Some("") => "",
        Some(r) if r.starts_with(['/', '\\']) => r.trim_start_matches(['/', '\\']),
        _ => return Ok(std::path::PathBuf::from(path)),
    };
    let home = home.ok_or(HOME_UNKNOWN)?;
    Ok(rest.split(['/', '\\']).filter(|c| !c.is_empty()).fold(home.to_path_buf(), |p, c| p.join(c)))
}

/// A `.ecad` path from what the user typed: surrounding spaces trimmed, a leading `~` expanded
/// to the home folder ([`home`]), and `.ecad` added when there is no extension.
///
/// # Errors
/// As [`normalize_with`].
pub fn normalize(path: &str) -> Result<String, String> {
    normalize_with(path, home().as_deref())
}

/// [`normalize`] with the home folder given.
///
/// # Errors
/// Empty, no file name (a folder, `.`, `..`, `.ecad`), a drive-relative path (`C:`, `C:name`),
/// `:` in the file name, a trailing dot, `~` without a known home, a reserved device name
/// (`CON.ecad`), or a different extension. A path it returns normalises to itself.
pub fn normalize_with(path: &str, home: Option<&std::path::Path>) -> Result<String, String> {
    let mut p = path.trim();
    // Explorer's "Copy as path" puts the path in double quotes.
    if let Some(inner) = p.strip_prefix('"').and_then(|r| r.strip_suffix('"')) {
        p = inner.trim();
    }
    if p.is_empty() {
        return Err(SAVE_AS_NEEDS_PATH.into());
    }
    if p == "~" || p.ends_with(['/', '\\']) {
        return Err(format!("{SAVE_AS_NEEDS_PATH}, not a folder ({p})"));
    }
    let mut chars = p.chars();
    if let (Some(drive), Some(':')) = (chars.next(), chars.next())
        && drive.is_ascii_alphabetic()
        && !chars.next().is_some_and(|c| c == '/' || c == '\\')
    {
        return Err(format!(
            "{p} is relative to drive {drive}:'s current folder; type a full path such as {drive}:\\dev\\ecad-projects\\demo"
        ));
    }
    let full = expand_home(p, home)?;
    let p = full.to_str().ok_or_else(|| format!("{} is not a valid Unicode path", full.display()))?;
    // The last component after either separator, so that a Windows path reads the same everywhere.
    let name = p.rsplit(['/', '\\']).next().unwrap_or(p);
    if name == "." || name == ".." || name.eq_ignore_ascii_case(".ecad") {
        return Err(format!("{SAVE_AS_NEEDS_PATH}, not {name} ({p})"));
    }
    if name.contains(':') {
        return Err(format!("{name} cannot be used: a file name cannot contain ':'"));
    }
    if name.ends_with('.') {
        return Err(format!("{name} cannot be used: Windows drops a file name's trailing dot; remove it"));
    }
    let device = name.split('.').next().unwrap_or_default().trim_end().to_ascii_uppercase();
    if RESERVED.contains(&device.as_str()) {
        return Err(format!("{name} cannot be used: {device} is a name Windows reserves for a device; choose another"));
    }
    let ext = std::path::Path::new(name).extension().map(|e| e.to_string_lossy().to_ascii_lowercase());
    match ext.as_deref() {
        None => Ok(format!("{p}.ecad")),
        Some("ecad") => Ok(p.to_string()),
        Some(other) => Err(format!("ECAD projects are saved as .ecad files, not .{other} ({p})")),
    }
}

/// Whether `a` and `b` name the same file or folder: their canonical paths when both exist,
/// otherwise their spelling (ignoring case and the separator on Windows).
pub fn same_path(a: &std::path::Path, b: &std::path::Path) -> bool {
    if let (Ok(x), Ok(y)) = (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        return x == y;
    }
    if cfg!(windows) {
        let fold = |p: &std::path::Path| p.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_lowercase();
        fold(a) == fold(b)
    } else {
        a == b
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Binding {
    /// Open the project file at `path` (already normalised): take its writer lock and open it
    /// from its latest snapshot.
    ///
    /// # Errors
    /// The file cannot be read, is a version 1 file ([`NEEDS_CONVERSION`]), or is not a project
    /// file this build supports.
    pub fn open(path: &str, seed: u64, writer: &str) -> Result<Opened, String> {
        if let Some(out) = needs_conversion(path) {
            return Err(format!("{path} {NEEDS_CONVERSION}: {out}"));
        }
        not_a_project(path)?;
        let o = ecad_file::v2::disk::DiskProjectV2::open(std::path::Path::new(path), seed, writer)
            .map_err(|e| e.to_string())?;
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
    pub fn open_read_only(path: &str, seed: u64, writer: &str) -> Result<Opened, String> {
        if let Some(out) = needs_conversion(path) {
            return Err(format!("{path} {NEEDS_CONVERSION}: {out}"));
        }
        let o = ecad_file::v2::disk::DiskProjectV2::open_read_only(std::path::Path::new(path), seed, writer)
            .map_err(|e| e.to_string())?;
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

    /// Where the file stands.
    pub fn status(&self) -> FileStatus {
        let Some(d) = &self.disk else { return FileStatus::default() };
        FileStatus {
            path: Some(d.path().display().to_string()),
            read_only: d.read_only().map(ToString::to_string),
            unconfirmed: d.unconfirmed(),
            preserved_tail: None,
        }
    }

    /// Save to the bound file.
    ///
    /// # Errors
    /// No file yet, or the save failed.
    pub(crate) fn save(&mut self, ledger: &mut Ledger) -> Result<(), String> {
        let d = self.disk.as_mut().ok_or(NO_FILE_YET)?;
        d.save(ledger).map(|_| ()).map_err(|e| e.to_string())
    }

    /// Save As `path` (already normalised), keeping `project`'s ID; binds to it on success.
    ///
    /// # Errors
    /// The file could not be written; the previous binding is kept.
    pub(crate) fn save_as(
        &mut self,
        path: &str,
        project: ProjectId,
        ledger: &mut Ledger,
        writer: &str,
        created_ms: Option<u64>,
    ) -> Result<Written, String> {
        let p = std::path::Path::new(path);
        let r = match &mut self.disk {
            Some(d) => d.save_as(p, ledger, created_ms, writer),
            None => ecad_file::v2::disk::DiskProjectV2::create(p, project, ledger, None, created_ms, writer).map(
                |(d, unconfirmed)| {
                    self.disk = Some(Box::new(d));
                    unconfirmed
                },
            ),
        };
        match r {
            Ok(None) => Ok(Written::Confirmed),
            Ok(Some(e)) => Ok(Written::Unconfirmed(ecad_file::v2::disk::DiskErrorV2::Unconfirmed(e).to_string())),
            Err(e) => Err(e.to_string()),
        }
    }

    /// Let go of the file and its writer lock (the project was closed).
    pub(crate) fn release(&mut self) {
        self.disk = None;
    }

    /// The writer lock file, while the project holds it.
    pub fn lock_file(&self) -> Option<String> {
        self.disk.as_ref().and_then(|d| d.lock_file()).map(|p| p.display().to_string())
    }

    /// The version 2 file's facts (copies, for the debugger's Storage view; no file access).
    pub fn storage(&self) -> Option<StorageFacts> {
        let d = self.disk.as_ref()?;
        let p = d.project();
        Some(StorageFacts {
            branch: format!("main ({})", p.branch().hex()),
            head: p.head().0,
            snapshot_seq: p.snapshot_seq(),
            snapshot_every: p.snapshot_every(),
            last_save: p.last_save().cloned(),
            link_v1: p.link_v1().map(|l| (l.hash.clone(), l.transactions)),
            writable: p.read_only().is_none(),
        })
    }

    /// The bound file's database, for the debugger's on-request reads (table sizes, branches,
    /// versions). Reads only.
    pub fn store(&self) -> Option<std::sync::Arc<ecad_file::v2::Store>> {
        self.disk.as_ref().map(|d| d.project().store().clone())
    }
}

#[cfg(target_arch = "wasm32")]
const NO_FILES: &str = "project files are not available in a browser build";

#[cfg(target_arch = "wasm32")]
impl Binding {
    /// Not available in a browser build.
    ///
    /// # Errors
    /// Always.
    pub fn open(_path: &str, _seed: u64, _writer: &str) -> Result<Opened, String> {
        Err(NO_FILES.into())
    }

    /// Not available in a browser build.
    ///
    /// # Errors
    /// Always.
    pub fn open_read_only(_path: &str, _seed: u64, _writer: &str) -> Result<Opened, String> {
        Err(NO_FILES.into())
    }

    /// No file.
    pub fn status(&self) -> FileStatus {
        FileStatus::default()
    }

    /// Never bound in a browser build.
    pub fn bound(&self) -> bool {
        false
    }

    /// No lock file.
    pub fn lock_file(&self) -> Option<String> {
        None
    }

    /// No file.
    pub fn storage(&self) -> Option<StorageFacts> {
        None
    }

    /// No file.
    pub fn store(&self) -> Option<std::sync::Arc<ecad_file::v2::Store>> {
        None
    }

    pub(crate) fn save(&mut self, _ledger: &mut Ledger) -> Result<(), String> {
        Err(NO_FILES.into())
    }

    pub(crate) fn save_as(
        &mut self,
        _path: &str,
        _project: ProjectId,
        _ledger: &mut Ledger,
        _writer: &str,
        _created_ms: Option<u64>,
    ) -> Result<Written, String> {
        Err(NO_FILES.into())
    }

    pub(crate) fn release(&mut self) {}
}
