//! A project's file on disk: the binding (`ecad_file::binding`, re-exported here) and the parts
//! that read the environment, the home folder ([`home`]) and [`normalize`].

pub use ecad_file::binding::*;
/// Whether `a` and `b` name the same file or folder (`ecad_store::disk::same_path_or_spelling`).
pub use ecad_store::disk::same_path_or_spelling as same_path;

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
    doc.standard.clone().ok_or(ecad_electrical::CmdError::refused(ecad_electrical::styles::commands::styles::NOTHING_TO_COPY))
}

/// A `.ecad` path from a resolved one (no `~` left): `.ecad` added when there is no extension.
///
/// # Errors
/// As [`normalize_with`].
pub fn normalize_resolved(path: &std::path::Path) -> Result<String, PathError> {
    let text = path.to_str().ok_or_else(|| PathError::NotUnicode(path.display().to_string()))?;
    normalize_with(text, None)
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
