//! The default projects folder: [`DEFAULT_FOLDER`], the [`BANNER`] that names it, and
//! [`default_folder!`](crate::default_folder) for text built at compile time (ecad-desktop's usage).

/// The default projects folder, as typed (see [`DEFAULT_FOLDER`]): a string literal, so it can go
/// in `concat!`.
#[cfg(windows)]
#[macro_export]
macro_rules! default_folder {
    () => {
        r"C:\dev\ecad-projects"
    };
}
/// The default projects folder, as typed (see [`DEFAULT_FOLDER`]): a string literal, so it can go
/// in `concat!`.
#[cfg(not(windows))]
#[macro_export]
macro_rules! default_folder {
    () => {
        "~/ecad-projects"
    };
}

/// What the app is, in full (the Project panel's experiment line shows [`NOTE`](crate::NOTE) with this as
/// its hover text; the top banner that showed it was removed, owner 2026-11-08).
pub const BANNER: &str = concat!(
    "ECAD DESKTOP (experiment). Projects are saved as .ecad files. Keep them in a local folder (for example ",
    crate::default_folder!(),
    "); other locations are unverified."
);

/// Where the Save As dialog suggests saving, created by the first save into it: `C:\dev\ecad-projects`
/// on Windows (owner decision 2026-10-31; never the OneDrive-redirected Documents folder),
/// `~/ecad-projects` elsewhere.
pub const DEFAULT_FOLDER: &str = crate::default_folder!();
