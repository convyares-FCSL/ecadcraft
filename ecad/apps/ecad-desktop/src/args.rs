//! The program's command line: [`parse_args`] and [`USAGE`]. The default projects folder it names is
//! [`crate::folder::DEFAULT_FOLDER`].

use std::ffi::OsString;
use std::path::PathBuf;

/// Command-line usage.
pub const USAGE: &str = concat!(
    "usage: ecad-desktop [--snapshots <dir>] [--control <port>] [--cad-ui] [<project.ecad>]\n\n\
ECAD desktop program (experiment). Opens <project.ecad> if given, otherwise starts a new\n\
project. Save and Save As write .ecad project files; keep them in a local folder such as ",
    crate::default_folder!(),
    ".\n--snapshots <dir>  allow exporting inspector snapshots (diagnostic files, not saved projects)\n\
--control <port>   agent access on for this run (Read and change, 127.0.0.1:<port>; not remembered)\n\
--cad-ui           show CADCraft's own interface inside the main view (the developer switch)\n\
See ecad/docs/user/quick-start.md."
);

/// What the command line asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Invocation {
    /// Start the app: open `project` if given, allow snapshots into `snapshots` if given, and
    /// turn agent access on for this run on port `control` if given.
    /// `cad_ui`: the shell's developer switch on from the start (`--cad-ui`).
    Run { snapshots: Option<PathBuf>, project: Option<String>, control: Option<u16>, cad_ui: bool },
    /// Print [`USAGE`].
    Help,
    /// Print the version.
    Version,
}

/// Parse the command line (without the program name), as given by `std::env::args_os`.
///
/// # Errors
/// A usage error, in words (print it with [`USAGE`]): an unknown option, `--snapshots` without a
/// folder, more than one project file, or a project file name that is not valid Unicode.
pub fn parse_args(args: impl IntoIterator<Item = OsString>) -> Result<Invocation, String> {
    let mut snapshots = None;
    let mut project: Option<String> = None;
    let mut control = None;
    let mut cad_ui = false;
    let mut args = args.into_iter();
    while let Some(a) = args.next() {
        match a.to_str() {
            Some("--snapshots") => match args.next() {
                Some(d) => snapshots = Some(PathBuf::from(d)),
                None => return Err("--snapshots needs a folder".into()),
            },
            Some("--control") => match args.next().and_then(|p| p.to_str().and_then(ecad_engine::agent::access::parse_port)) {
                Some(p) => control = Some(p),
                None => return Err("--control needs a port from 1024 to 65535".into()),
            },
            Some("--cad-ui") => cad_ui = true,
            Some("--help" | "-h") => return Ok(Invocation::Help),
            Some("--version" | "-V") => return Ok(Invocation::Version),
            Some(other) if other.starts_with('-') => return Err(format!("unknown argument {other:?}")),
            Some(path) if project.is_none() => project = Some(path.to_string()),
            Some(extra) => return Err(format!("only one project file can be opened ({extra:?})")),
            None => {
                return Err(format!("the argument {:?} is not valid Unicode; project files need a Unicode name", a.to_string_lossy()));
            }
        }
    }
    Ok(Invocation::Run { snapshots, project, control, cad_ui })
}
