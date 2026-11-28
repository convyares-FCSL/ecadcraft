//! A project's file on disk: the parts of binding a project to its file (`ecad_file::binding`)
//! that read the environment, the home folder ([`home`]) and [`normalize`].

use ecad_file::binding::*;
use ecad_store::disk::same_path;

/// The user's home folder (`std::env::home_dir`: on Windows the user profile, whether or not
/// `HOME` is set). `None` when it is not known.
pub fn home() -> Option<std::path::PathBuf> {
    std::env::home_dir().filter(|h| !h.as_os_str().is_empty())
}

/// A `.ecad` path from what the user typed: surrounding spaces trimmed, a leading `~` expanded
/// to the home folder ([`home`]), and `.ecad` added when there is no extension.
///
/// # Errors
/// As [`normalize_with`].
pub fn normalize(path: &str) -> Result<String, PathError> {
    normalize_with(path, home().as_deref())
}

/// The standard of the `.ecad` project at `path`, opened read-only (the file is never changed):
/// `standard {from}` and the Standards page's preview.
///
/// # Errors
/// A path or file that does not open as a project, or a project with no standard.
pub fn read_standard(path: &str) -> Result<ecad_model::Standard, ecad_electrical::CmdError> {
    let path = normalize(path)?;
    let opened = Binding::open_read_only(&path, 0x050a, "ecad standard copy")?;
    let doc = opened.ledger.document();
    doc.standard().cloned().ok_or(ecad_electrical::CmdError::refused(ecad_electrical::styles::commands::styles::NOTHING_TO_COPY))
}

/// A `.ecad` path from a resolved one (no `~` left): `.ecad` added when there is no extension.
///
/// # Errors
/// As [`normalize_with`].
pub fn normalize_resolved(path: &std::path::Path) -> Result<String, PathError> {
    let text = path.to_str().ok_or_else(|| PathError::NotUnicode(path.display().to_string()))?;
    normalize_with(text, None)
}

/// Why a project file could not be opened, saved, saved as, or switched to another branch, by what the person
/// can do about it. Each variant keeps the words the person sees ([`std::fmt::Display`]); callers decide on the
/// variant, never on the words.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProjectFileError {
    /// The path typed is not one a project file can have.
    #[error(transparent)]
    Path(#[from] PathError),
    /// There is no file at the path.
    #[error("{0}")]
    NotFound(String),
    /// The file is not an ECAD project.
    #[error("{0}")]
    NotAProject(String),
    /// The file is damaged so badly that nothing can be shown (a damaged page stopped the storage engine, or the
    /// container facts do not read). A file damaged further in opens read-only instead.
    #[error("{0}")]
    Damaged(String),
    /// The file is of another format version and this build cannot read all of it: open it with a build that
    /// reads its format.
    #[error("{0}")]
    OtherFormat(String),
    /// Another session holds the file (a Save As target, or a file no session can read while it is held).
    #[error("{0}")]
    Locked(String),
    /// The system refused access to the file or its folder.
    #[error("{0}")]
    Denied(String),
    /// The project was opened read-only: Save As saves a copy.
    #[error("{0}")]
    ReadOnly(String),
    /// Something else replaced, moved or changed the file since it was opened: Save As saves a copy.
    #[error("{0}")]
    Replaced(String),
    /// Written, but the system could not confirm the change is durable: save again.
    #[error("{0}")]
    Unconfirmed(String),
    /// The project has no file yet: Save As first.
    #[error("{0}")]
    NoFileYet(String),
    /// A branch action refused in its own words (a branch that does not exist, a name taken).
    #[error("{0}")]
    Refused(String),
    /// Any other failure to read or write the file.
    #[error("{0}")]
    Io(String),
    /// The project opened but could not be shown (not expected for a file that verifies).
    #[error("{0}")]
    Other(String),
}

impl ProjectFileError {
    /// The same failure in the words of a save of project `name`: "<name> was not saved: …". A refused path
    /// keeps its own words.
    #[must_use]
    pub fn not_saved(self, name: &str) -> Self {
        let say = |m: String| format!("{name} was not saved: {m}");
        match self {
            Self::Path(p) => Self::Path(p),
            Self::NotFound(m) => Self::NotFound(say(m)),
            Self::NotAProject(m) => Self::NotAProject(say(m)),
            Self::Damaged(m) => Self::Damaged(say(m)),
            Self::OtherFormat(m) => Self::OtherFormat(say(m)),
            Self::Locked(m) => Self::Locked(say(m)),
            Self::Denied(m) => Self::Denied(say(m)),
            Self::ReadOnly(m) => Self::ReadOnly(say(m)),
            Self::Replaced(m) => Self::Replaced(say(m)),
            Self::Unconfirmed(m) => Self::Unconfirmed(say(m)),
            Self::NoFileYet(m) => Self::NoFileYet(say(m)),
            Self::Refused(m) => Self::Refused(say(m)),
            Self::Io(m) => Self::Io(say(m)),
            Self::Other(m) => Self::Other(say(m)),
        }
    }

    /// A storage failure, by what it means to the person; `message` is what they see.
    fn of_store(e: &ecad_file::StoreError, message: String) -> Self {
        use ecad_file::StoreError as S;
        match e {
            S::Halted(_) | S::Damaged(_) => Self::Damaged(message),
            S::Busy => Self::Locked(message),
            S::Denied(_) => Self::Denied(message),
            S::ReadOnly => Self::ReadOnly(message),
            S::Engine(_) | S::Refused(_) | S::PreviousIo => Self::Io(message),
        }
    }
}

impl From<BindingError> for ProjectFileError {
    fn from(e: BindingError) -> Self {
        use ecad_file::disk::{DiskError as D, OpenFailure as F};
        use ecad_file::{OpenError as O, SaveError as S};
        let message = e.to_string();
        match &e {
            BindingError::NotAProject(_) => Self::NotAProject(message),
            BindingError::NoFileYet => Self::NoFileYet(message),
            BindingError::Disk(d) => match d {
                D::Open { error, .. } => match error {
                    F::NoFile => Self::NotFound(message),
                    F::Store(s) | F::Project(O::Store(s)) => Self::of_store(s, message),
                    F::Project(O::Unsupported(_)) => Self::OtherFormat(message),
                    F::Project(O::Damaged(_)) => Self::Damaged(message),
                },
                D::TargetLocked { .. } => Self::Locked(message),
                D::Unconfirmed(_) => Self::Unconfirmed(message),
                D::Replaced(_) | D::Save(S::ExternalChange(_)) => Self::Replaced(message),
                D::ReadOnly(_) | D::Save(S::ReadOnly(_)) => Self::ReadOnly(message),
                D::Save(S::Store(s)) => Self::of_store(s, message),
                D::BeforeReplacement { .. } | D::CannotCheck { .. } | D::Save(S::Encode(_)) => Self::Io(message),
            },
        }
    }
}

impl From<ecad_file::BranchError> for ProjectFileError {
    fn from(e: ecad_file::BranchError) -> Self {
        use ecad_file::BranchError as B;
        let message = e.to_string();
        match &e {
            B::ReadOnly(_) => Self::ReadOnly(message),
            B::Refused(_) | B::NameTaken(_) => Self::Refused(message),
            B::Store(s) => Self::of_store(s, message),
        }
    }
}

/// Why a Save As was not made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveAsRefused {
    /// Another file is already there; ask again with `replace` to overwrite it.
    Exists(std::path::PathBuf),
}

/// Why [`save_project_as`] did not save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveAsError {
    /// Refused before anything was written.
    Refused(SaveAsRefused),
    /// `saveas` itself failed, in words.
    Failed(String),
}

impl std::fmt::Display for SaveAsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveAsError::Refused(SaveAsRefused::Exists(p)) => write!(f, "{} already exists", p.display()),
            SaveAsError::Failed(m) => f.write_str(m),
        }
    }
}

/// Save the open project as `target` (a resolved `.ecad` path): refused with
/// [`SaveAsRefused::Exists`] when another file is there and `replace` is not set, else `saveas`.
/// The project's own file (`own`) is always saved over. The check and the save are one call.
///
/// # Errors
/// [`SaveAsError::Refused`] before anything is written, or [`SaveAsError::Failed`] when `saveas` fails.
pub fn save_project_as(
    session: &mut cadcraft_engine::Session, target: &std::path::Path, own: Option<&std::path::Path>, replace: bool,
) -> Result<serde_json::Value, SaveAsError> {
    let is_own = own.is_some_and(|o| same_path(target, o));
    if !replace && !is_own && target.exists() {
        return Err(SaveAsError::Refused(SaveAsRefused::Exists(target.to_path_buf())));
    }
    session.execute("saveas", &serde_json::json!({ "path": target.to_string_lossy() })).map_err(|e| SaveAsError::Failed(e.to_string()))
}

/// Make the default projects folder when `target`'s folder is it and it does not exist yet; no other folder
/// is made. Both are resolved paths.
///
/// # Errors
/// The folder could not be made.
pub fn ensure_default_folder(target: &std::path::Path, default: &std::path::Path) -> std::io::Result<()> {
    if let Some(dir) = target.parent()
        && same_path(dir, default)
        && !dir.exists()
    {
        std::fs::create_dir_all(dir)?;
    }
    Ok(())
}

/// `path` without its extension, as text (a default name for output files).
#[must_use]
pub fn without_extension(path: &std::path::Path) -> String {
    path.with_extension("").to_string_lossy().into_owned()
}
