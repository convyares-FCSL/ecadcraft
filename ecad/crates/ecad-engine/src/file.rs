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

/// Whether a file or folder exists at `path` (the app asks the engine; it never reads the disk itself).
#[must_use]
pub fn path_exists(path: &std::path::Path) -> bool {
    path.exists()
}

/// Make the default projects folder when `target`'s folder is it and it does not exist yet; no other folder
/// is made. `default` and `target` are typed paths (`~` is `home`).
///
/// # Errors
/// The folder could not be made.
pub fn ensure_default_folder(target: &str, default: &str, home: Option<&std::path::Path>) -> std::io::Result<()> {
    if let Some(dir) = std::path::Path::new(target).parent()
        && let Ok(default) = expand_home(default, home)
        && same_path(dir, &default)
        && !dir.exists()
    {
        std::fs::create_dir_all(dir)?;
    }
    Ok(())
}

/// `path` without its extension, as text (a default name for output files).
#[must_use]
pub fn without_extension(path: &str) -> String {
    std::path::Path::new(path).with_extension("").to_string_lossy().into_owned()
}
