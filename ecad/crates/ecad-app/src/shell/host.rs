//! What the host program supplies: the platform's default projects folder, the person's login name,
//! the file manager, the project files in a folder, and the clock. The app has no disk, OS or time-zone access of its own
//! (layer purpose); ecad-desktop installs a [`Host`] at start-up, and until then [`Host::default`]
//! answers with neutral values (UTC, no file manager).

use std::path::Path;
use std::sync::OnceLock;

/// Facts and actions only the program can provide.
#[derive(Debug, Clone)]
pub struct Host {
    /// Where Save As suggests saving, as typed (the platform's policy).
    pub default_folder: String,
    /// The home folder that `~` in a typed path stands for; `None` when the platform has none.
    pub home: Option<std::path::PathBuf>,
    /// The person's login name; the top bar shows its initials.
    pub user: String,
    /// Open a folder in the system's file manager.
    pub open_folder: fn(&Path) -> std::io::Result<()>,
    /// The `.ecad` file names in a folder, sorted (none when it cannot be read); the Open dialog
    /// lists them to pick from.
    pub list_projects: fn(&Path) -> Vec<String>,
    /// The wall clock, milliseconds since the epoch.
    pub now_ms: fn() -> u64,
    /// `%H:%M:%S`-style local time of `ms` since the epoch: the format is `HH:MM:SS` when `seconds`
    /// is true, else `HH:MM`.
    pub local_time: fn(ms: u64, seconds: bool) -> String,
}

fn no_file_manager(_: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "no file manager is available"))
}

fn no_listing(_: &Path) -> Vec<String> {
    Vec::new()
}

fn system_now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok().and_then(|d| u64::try_from(d.as_millis()).ok()).unwrap_or(0)
}

fn utc_time(ms: u64, seconds: bool) -> String {
    let day = ms / 1000 % 86_400;
    let (h, m, s) = (day / 3600, day / 60 % 60, day % 60);
    if seconds { format!("{h:02}:{m:02}:{s:02}") } else { format!("{h:02}:{m:02}") }
}

impl Default for Host {
    fn default() -> Self {
        Host {
            default_folder: "~/ecad-projects".into(),
            home: None,
            user: String::new(),
            open_folder: no_file_manager,
            list_projects: no_listing,
            now_ms: system_now_ms,
            local_time: utc_time,
        }
    }
}

/// A typed path as a path: a leading `~` is `home` (a folder literally named `~` is never meant).
/// The program supplies `home` ([`Host::home`]); nothing is read from the environment here.
///
/// # Errors
/// The path starts with `~` and `home` is `None` ([`ecad_engine::file::PathError::HomeUnknown`]).
pub(crate) fn resolve(typed: &str, home: Option<&Path>) -> Result<std::path::PathBuf, ecad_file::binding::PathError> {
    let rest = match typed.strip_prefix('~') {
        Some("") => "",
        Some(r) if r.starts_with(['/', '\\']) => r.trim_start_matches(['/', '\\']),
        _ => return Ok(std::path::PathBuf::from(typed)),
    };
    let home = home.ok_or(ecad_file::binding::PathError::HomeUnknown)?;
    Ok(rest.split(['/', '\\']).filter(|c| !c.is_empty()).fold(home.to_path_buf(), |p, c| p.join(c)))
}

static HOST: OnceLock<Host> = OnceLock::new();

/// Install the program's host. The first call wins; later ones are ignored.
pub fn install(host: Host) {
    let _ = HOST.set(host);
}

/// The installed host, or the neutral default.
pub(crate) fn host() -> &'static Host {
    HOST.get_or_init(Host::default)
}
