//! Opening the ECAD window for `--launch` (brief 035, design §9.3), on Windows and Linux.
//!
//! # Semantics
//! - The trial is found as `ECAD_TRIAL` (a full path), else `ecad-trial(.exe)` beside the running
//!   `ecad-mcp`, else on `PATH` ([`find_trial`]).
//! - It starts with `--control <port>` (and the project, when the server was given one), with
//!   stdin, stdout and stderr to null, so nothing of it reaches the MCP stream, and detached from
//!   the server (Windows: a new process group without a console; Linux: its own process group).
//! - Linux without `DISPLAY` or `WAYLAND_DISPLAY` is refused before anything starts.
//! - The server then waits up to [`LAUNCH_WAIT`] for the port, probing every 500 ms.
//!
//! Excluded: macOS (no packaged trial yet), and any non-loopback address.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// How long a launched window has to open its port.
pub const LAUNCH_WAIT: Duration = Duration::from_secs(30);
/// How often the port is probed while waiting.
pub const LAUNCH_PROBE: Duration = Duration::from_millis(500);
/// The environment variable naming the trial's full path.
pub const TRIAL_ENV: &str = "ECAD_TRIAL";

/// The trial's file name on this platform.
pub fn trial_file_name() -> &'static str {
    if cfg!(windows) { "ecad-trial.exe" } else { "ecad-trial" }
}

/// Said when no trial is found.
pub const NOT_FOUND: &str = "--launch could not find ecad-trial; set ECAD_TRIAL to its path.";
/// Said on Linux without a display.
pub const NO_DISPLAY: &str = "No display for the ECAD window (DISPLAY/WAYLAND_DISPLAY unset); running headless is not possible with --launch.";

/// Where the trial is: `env` (the value of [`TRIAL_ENV`]) when it names a file, else beside
/// `exe` (the running server), else in one of the `path` folders (the value of `PATH`). `exists`
/// says whether a file exists (tests pass their own).
pub fn find_trial(env: Option<OsString>, exe: Option<&Path>, path: Option<OsString>, exists: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    if let Some(e) = env.filter(|e| !e.is_empty()) {
        let p = PathBuf::from(e);
        return exists(&p).then_some(p);
    }
    let name = trial_file_name();
    if let Some(dir) = exe.and_then(Path::parent) {
        let p = dir.join(name);
        if exists(&p) {
            return Some(p);
        }
    }
    path.iter().flat_map(std::env::split_paths).map(|d| d.join(name)).find(|p| exists(p))
}

/// The trial's arguments for `port` and `project`.
pub fn trial_args(port: u16, project: Option<&str>) -> Vec<String> {
    let mut a = vec!["--control".to_string(), port.to_string()];
    if let Some(p) = project {
        a.push(p.to_string());
    }
    a
}

/// Whether this platform has a display for a window: always on Windows; on Linux and other Unix,
/// `DISPLAY` or `WAYLAND_DISPLAY` set (the values given).
pub fn has_display(display: Option<OsString>, wayland: Option<OsString>) -> bool {
    if cfg!(windows) {
        return true;
    }
    display.is_some_and(|d| !d.is_empty()) || wayland.is_some_and(|d| !d.is_empty())
}

/// The command that starts `trial` with `args`: null stdio, detached.
pub fn command(trial: &Path, args: &[String]) -> Command {
    let mut c = Command::new(trial);
    c.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    detach(&mut c);
    c
}

#[cfg(windows)]
fn detach(c: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    c.creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS);
}

#[cfg(unix)]
fn detach(c: &mut Command) {
    use std::os::unix::process::CommandExt;
    c.process_group(0);
}

/// Other platforms: no detaching is known; the child still has null stdio.
#[cfg(not(any(windows, unix)))]
fn detach(_c: &mut Command) {}

/// The project `--launch` passes to the window (set once by `main` from the command line).
static PROJECT: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Remember the project the window should open when launched.
pub fn set_project(p: &str) {
    let _ = PROJECT.set(p.to_string());
}

/// Open the trial with the port and wait until it answers.
///
/// # Errors
/// No trial, no display, the start failed, or the port did not answer in time; in words.
pub fn launch_and_wait(port: u16) -> Result<(), String> {
    if cfg!(target_os = "macos") {
        return Err("--launch is not available on macOS yet; open ecad-trial with --control yourself.".into());
    }
    if !has_display(std::env::var_os("DISPLAY"), std::env::var_os("WAYLAND_DISPLAY")) {
        return Err(NO_DISPLAY.into());
    }
    let exe = std::env::current_exe().ok();
    let trial = find_trial(std::env::var_os(TRIAL_ENV), exe.as_deref(), std::env::var_os("PATH"), Path::is_file).ok_or(NOT_FOUND)?;
    command(&trial, &trial_args(port, PROJECT.get().map(String::as_str)))
        .spawn()
        .map_err(|e| format!("--launch could not start {}: {e}", trial.display()))?;
    let start = Instant::now();
    while start.elapsed() < LAUNCH_WAIT {
        std::thread::sleep(LAUNCH_PROBE);
        if crate::bridge::probe(port) {
            return Ok(());
        }
    }
    Err(format!(
        "ECAD was started but its port {port} did not answer within 30 s; if ECAD was already open without agent access, turn it on in ECAD Dev."
    ))
}
