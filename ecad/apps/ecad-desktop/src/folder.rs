//! The default projects folder: [`DEFAULT_FOLDER`], the [`BANNER`] that names it, and
//! [`default_folder!`](crate::default_folder) for text built at compile time. It is platform policy, so it
//! lives in the program and the app gets it through its host ([`ecad_app::shell::host::Host`]).

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

/// What the app is, in full.
pub const BANNER: &str = concat!(
    "ECAD DESKTOP (experiment). Projects are saved as .ecad files. Keep them in a local folder (for example ",
    crate::default_folder!(),
    "); other locations are unverified."
);

/// Where the Save As dialog suggests saving, created by the first save into it: `C:\dev\ecad-projects`
/// on Windows (never the OneDrive-redirected Documents folder),
/// `~/ecad-projects` elsewhere.
pub const DEFAULT_FOLDER: &str = crate::default_folder!();
