//! A project's file on disk (persistence stage 2a): the binding between a [`crate::Project`] and
//! its `.ecad` file, through `ecad_file::DiskProject`. Native only; in a browser build every
//! operation reports that project files are not available.

use ecad_file::OpenReport;
use ecad_model::{Ledger, ProjectId};

/// What Save says when a project has never been saved and no file was chosen.
pub const NO_FILE_YET: &str = "this project has no file yet; use Save As and choose a .ecad file";
/// What Save As says without a file name.
pub const SAVE_AS_NEEDS_PATH: &str = "Save As needs a file name (a .ecad file)";

/// Where a project's file stands, for status displays.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileStatus {
    /// The bound file.
    pub path: Option<String>,
    /// Why the project cannot be saved to that file (damaged, or another session is editing it).
    pub read_only: Option<String>,
    /// The last Save As replaced its target, but durability was not confirmed; Save confirms it.
    pub unconfirmed: bool,
    /// Where the interrupted trailing bytes found at open were preserved.
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
    pub report: OpenReport,
    pub project: ProjectId,
    /// A lock file left by a session that ended without releasing it, reclaimed: its record.
    pub reclaimed_lock: Option<String>,
}

/// The file a project is bound to, holding its writer lock while bound.
#[derive(Debug, Default)]
pub struct Binding {
    #[cfg(not(target_arch = "wasm32"))]
    disk: Option<Box<ecad_file::DiskProject>>,
    /// Faults for the next first save (tests).
    #[cfg(not(target_arch = "wasm32"))]
    pub faults: ecad_file::StepFaults,
}

/// A `.ecad` path from what the user typed: surrounding spaces trimmed, a leading `~/` expanded
/// to the home folder, and `.ecad` added when there is no extension.
///
/// # Errors
/// Empty, or a different extension.
pub fn normalize(path: &str) -> Result<String, String> {
    let p = path.trim();
    if p.is_empty() {
        return Err(SAVE_AS_NEEDS_PATH.into());
    }
    let p = match (p.strip_prefix("~/"), std::env::var("HOME")) {
        (Some(rest), Ok(home)) if !home.is_empty() => format!("{}/{rest}", home.trim_end_matches('/')),
        _ => p.to_string(),
    };
    let ext = std::path::Path::new(&p).extension().map(|e| e.to_string_lossy().to_ascii_lowercase());
    match ext.as_deref() {
        None => Ok(format!("{p}.ecad")),
        Some("ecad") => Ok(p),
        Some(other) => Err(format!("ECAD projects are saved as .ecad files, not .{other} ({p})")),
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Binding {
    /// Open the project file at `path` (already normalised): take its writer lock and replay it.
    ///
    /// # Errors
    /// The file cannot be read, or is not a project file this build supports.
    pub fn open(path: &str, seed: u64, writer: &str) -> Result<Opened, String> {
        let o = ecad_file::DiskProject::open(std::path::Path::new(path), seed, writer).map_err(|e| e.to_string())?;
        let project = o.project.project_id().ok_or("the file has no project ID")?;
        Ok(Opened {
            binding: Binding { disk: Some(Box::new(o.project)), faults: ecad_file::StepFaults::default() },
            ledger: o.ledger,
            report: o.report,
            project,
            reclaimed_lock: o.reclaimed_lock,
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
            preserved_tail: d.preserved_fragment().map(|p| p.display().to_string()),
        }
    }

    /// Save to the bound file.
    ///
    /// # Errors
    /// No file yet, or the save failed (see `ecad_file::DiskError`).
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
        let durability = match &mut self.disk {
            Some(d) => d.save_as(p, ledger, created_ms, writer).map_err(|e| e.to_string())?,
            None => {
                let faults = std::mem::take(&mut self.faults);
                let (d, durability) = ecad_file::DiskProject::create(p, project, ledger, created_ms, writer, faults)
                    .map_err(|e| e.to_string())?;
                self.disk = Some(Box::new(d));
                durability
            }
        };
        Ok(match durability {
            ecad_file::Durability::Confirmed => Written::Confirmed,
            ecad_file::Durability::Unconfirmed(e) => {
                Written::Unconfirmed(ecad_file::DiskError::Unconfirmed(e).to_string())
            }
        })
    }

    /// Let go of the file and its writer lock (the project was closed).
    pub(crate) fn release(&mut self) {
        self.disk = None;
    }

    /// The writer lock file, while the project holds it.
    pub fn lock_file(&self) -> Option<String> {
        self.disk.as_ref().and_then(|d| d.lock_file()).map(|p| p.display().to_string())
    }

    /// The file's header, writer summary and record spans (copies, for the debugger; a failed
    /// save's bytes are not copied).
    pub fn writer(&self) -> Option<(ecad_file::Header, ecad_file::WriterSummary, Vec<ecad_file::RecordSpan>)> {
        let f = self.disk.as_ref()?.file();
        Some((f.header().clone(), f.summary(), f.spans().to_vec()))
    }

    /// The disk project, to inject faults in tests.
    pub fn disk_mut(&mut self) -> Option<&mut ecad_file::DiskProject> {
        self.disk.as_deref_mut()
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
    pub fn writer(&self) -> Option<(ecad_file::Header, ecad_file::WriterSummary, Vec<ecad_file::RecordSpan>)> {
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
