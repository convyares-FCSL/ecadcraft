//! What the app gets from the operating system: the default projects folder, the login name, the file
//! manager, the project files in a folder, the local clock and CADCraft's file-pick hooks (the app has no disk, OS or
//! time-zone access of its own).

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use cadcraft_ui_egui::Services;
use ecad_app::shell::host::Host;

/// The host for this platform.
#[must_use]
pub fn host() -> Host {
    Host {
        default_folder: crate::folder::DEFAULT_FOLDER.to_string(),
        home: ecad_engine::home(),
        user: std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_default(),
        open_folder,
        list_projects,
        now_ms,
        local_time,
    }
}

/// Open `folder` in the system's file manager (a separate process; nothing waits for it).
///
/// # Errors
/// The file manager could not be started.
pub fn open_folder(folder: &Path) -> std::io::Result<()> {
    let program = if cfg!(target_os = "windows") {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(program).arg(folder).spawn().map(|_| ())
}

/// The `.ecad` files in `folder`, by name, sorted; none when the folder cannot be read. At most
/// 500, so a huge folder cannot stall the Open dialog.
fn list_projects(folder: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(folder) else { return Vec::new() };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| Path::new(n).extension().is_some_and(|x| x.eq_ignore_ascii_case("ecad")))
        .take(500)
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names
}

fn now_ms() -> u64 {
    u64::try_from(chrono::Utc::now().timestamp_millis()).unwrap_or(0)
}

/// Local time of `ms` since the epoch: `HH:MM:SS`, or `HH:MM` without `seconds`.
fn local_time(ms: u64, seconds: bool) -> String {
    use chrono::{Local, TimeZone};
    let format = if seconds { "%H:%M:%S" } else { "%H:%M" };
    i64::try_from(ms).ok().and_then(|ms| Local.timestamp_millis_opt(ms).single()).map_or_else(String::new, |t| t.format(format).to_string())
}

/// The app has no native file dialogs. When CADCraft asks for a file name to save a drawing,
/// the request is passed on (`requested` is set) and the app opens its own path dialog for the
/// ECAD project; CADCraft itself gets no path, so it writes nothing. Exports get no path and stop.
#[must_use]
pub fn services(requested: Arc<AtomicBool>) -> Services {
    Services {
        pick_open: None,
        pick_pgp: None,
        pick_save: Some(Box::new(move |name: &str| {
            let ext = Path::new(name).extension().map(|e| e.to_string_lossy().to_ascii_lowercase());
            if matches!(ext.as_deref(), None | Some("dwg" | "dxf" | "dwt" | "ecad")) {
                requested.store(true, Ordering::Relaxed);
            }
            None
        })),
        // No APPLOAD file dialog either: CADCraft asks for the file name at its command line.
        pick_lisp: None,
        // The save request above answers at once, so no dialog is left open across frames.
        start_pick: None,
        no_file_system: false,
        // The desktop's own exit (saving the project, releasing the lock) runs in its `on_exit`.
        at_exit: None,
    }
}
