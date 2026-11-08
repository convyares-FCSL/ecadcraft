//! Start-up timing for the trial (brief 015), switched on by `ECAD_STARTUP_TIMING=1`.
//!
//! Marks the steps from process start to the first presented frame, prints each on stderr and
//! writes them all to a small JSON file. With the switch off nothing is created, printed or written.
//!
//! # Semantics
//! Every mark has its time since the origin and since the previous mark (the first mark's step is
//! its time since the origin), so the steps add up to the total. The origin is the process creation
//! time where the OS gives it without `unsafe` or a new dependency: on Linux, `/proc/self/stat`
//! with `/proc/uptime`. Elsewhere (Windows included: `GetProcessTimes` needs `unsafe`, which the
//! workspace forbids, and no crate already in the lock file wraps it) the origin is `main`, and the
//! JSON records `main`'s wall-clock time so that a launcher can measure the gap before `main`.
//!
//! This module excludes any timing inside eframe, egui, wgpu or CADCraft (the window, adapter,
//! device, font atlas and canvas pipelines each fall inside one step here); splitting those would
//! need upstream changes.
//!
//! Nothing here may stop the trial: stderr writes ignore errors (a closed pipe), and a JSON file
//! that cannot be written is reported, never fatal.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use cadcraft_ui_egui::switch::Switch;
use serde_json::json;

/// The environment variable that switches timing on.
pub const ENV: &str = "ECAD_STARTUP_TIMING";

/// Whether `value` (of [`ENV`]) switches timing on: `1/on/true/yes`; off when unset or anything else.
pub fn enabled(value: Option<&str>) -> bool {
    Switch::parse(ENV, value).or(false)
}

/// A start-up step, in the order the trial reaches them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// `main` entered (before it: the OS loader, DLLs, the virus scanner).
    Main,
    /// `ecad_cadcraft::install()` done.
    Install,
    /// `eframe::run_native` about to be called (options built).
    RunNative,
    /// The creator closure entered: eframe has created the event loop, the (still hidden) window,
    /// the wgpu instance, adapter, device and surface, and egui's context. Not egui's fonts: egui
    /// builds its font atlas during the first frame (step 7).
    Creator,
    /// CADCraft's app and shell built (`CadApp::new` and the trial around it).
    Shell,
    /// ECAD's project opened or created.
    Project,
    /// CADCraft's GPU canvas set up (`set_wgpu`: pipelines and shaders).
    SetWgpu,
    /// The first `ui` pass finished (first frame built, including egui's font atlas).
    FirstUpdate,
    /// The first frame after a full `ui` pass started, so that pass was painted and presented
    /// (eframe has no call-back after presenting): an upper bound that includes tessellation, the
    /// font texture upload, painting, presenting, showing the window and the wait for the next
    /// frame.
    FirstPresent,
}

impl Step {
    /// Every step, in order.
    pub const ALL: [Step; 9] = [
        Step::Main,
        Step::Install,
        Step::RunNative,
        Step::Creator,
        Step::Shell,
        Step::Project,
        Step::SetWgpu,
        Step::FirstUpdate,
        Step::FirstPresent,
    ];

    /// The mark's number in brief 015 (step 5 is split into `5a` and `5b`).
    pub fn number(self) -> &'static str {
        match self {
            Step::Main => "1",
            Step::Install => "2",
            Step::RunNative => "3",
            Step::Creator => "4",
            Step::Shell => "5a",
            Step::Project => "5b",
            Step::SetWgpu => "6",
            Step::FirstUpdate => "7",
            Step::FirstPresent => "8",
        }
    }

    /// A short machine name (the JSON `name`).
    pub fn name(self) -> &'static str {
        match self {
            Step::Main => "main",
            Step::Install => "install",
            Step::RunNative => "run_native",
            Step::Creator => "creator",
            Step::Shell => "shell",
            Step::Project => "project",
            Step::SetWgpu => "set_wgpu",
            Step::FirstUpdate => "first_update",
            Step::FirstPresent => "first_present",
        }
    }

    /// What the mark means, in words.
    pub fn label(self) -> &'static str {
        match self {
            Step::Main => "main entered",
            Step::Install => "ecad_cadcraft::install() done",
            Step::RunNative => "run_native called",
            Step::Creator => "creator entered (window, wgpu device, egui context)",
            Step::Shell => "CADCraft app and shell built",
            Step::Project => "ECAD project opened",
            Step::SetWgpu => "set_wgpu done (canvas pipelines)",
            Step::FirstUpdate => "first update built (incl. font atlas)",
            Step::FirstPresent => "first frame presented (next frame started)",
        }
    }
}

/// Where the times are measured from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// The process creation time given by the OS.
    ProcessCreation,
    /// `main` entered (the OS gave no creation time).
    Main,
}

impl Origin {
    fn name(self) -> &'static str {
        match self {
            Origin::ProcessCreation => "process creation",
            Origin::Main => "main",
        }
    }
}

/// One recorded mark.
#[derive(Debug, Clone, PartialEq)]
pub struct Mark {
    pub step: Step,
    /// Milliseconds since the origin.
    pub since_start_ms: f64,
    /// Milliseconds since the previous mark (since the origin for the first).
    pub step_ms: f64,
}

/// The wgpu adapter eframe picked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Adapter {
    pub backend: String,
    pub name: String,
    pub device_type: String,
}

/// Start-up timing in progress. Create it first thing in `main` (it records [`Step::Main`]).
#[derive(Debug)]
pub struct StartupTiming {
    origin: Origin,
    /// From the origin to `main` (zero when the origin is `main`).
    before_main: Duration,
    main_at: Instant,
    /// `main`'s wall-clock time, milliseconds since the Unix epoch.
    main_unix_ms: Option<u64>,
    marks: Vec<Mark>,
    adapter: Option<Adapter>,
    /// Print each mark on stderr as it is recorded.
    echo: bool,
}

impl StartupTiming {
    /// Timing from the environment: `Some` (with [`Step::Main`] recorded and printed) only when
    /// [`ENV`] switches it on. Call it first thing in `main`.
    pub fn from_env() -> Option<Self> {
        let main_at = Instant::now();
        let wall = SystemTime::now();
        if !enabled(std::env::var(ENV).ok().as_deref()) {
            return None;
        }
        let mut t = Self::starting(main_at, wall, process_age(), true);
        t.mark(Step::Main);
        Some(t)
    }

    /// Timing that started at `main_at` (wall-clock `wall`), `before_main` after the process was
    /// created when known. Nothing is recorded yet; `echo` prints each mark on stderr.
    pub fn starting(main_at: Instant, wall: SystemTime, before_main: Option<Duration>, echo: bool) -> Self {
        let (origin, before_main) = match before_main {
            Some(d) => (Origin::ProcessCreation, d),
            None => (Origin::Main, Duration::ZERO),
        };
        StartupTiming {
            origin,
            before_main,
            main_at,
            main_unix_ms: wall.duration_since(UNIX_EPOCH).ok().and_then(|d| u64::try_from(d.as_millis()).ok()),
            marks: Vec::with_capacity(Step::ALL.len()),
            adapter: None,
            echo,
        }
    }

    /// Where the times are measured from.
    pub fn origin(&self) -> Origin {
        self.origin
    }

    /// The marks recorded so far, in order.
    pub fn marks(&self) -> &[Mark] {
        &self.marks
    }

    /// Record `step` now (and print it when echoing). Marks are monotonic: a clock that went
    /// backwards gives a zero step, never a negative one.
    pub fn mark(&mut self, step: Step) {
        let since_start = self.before_main.saturating_add(self.main_at.elapsed());
        let since_start_ms = since_start.as_secs_f64() * 1000.0;
        let previous = self.marks.last().map_or(0.0, |m| m.since_start_ms);
        let since_start_ms = since_start_ms.max(previous);
        let mark = Mark { step, since_start_ms, step_ms: since_start_ms - previous };
        if self.echo {
            say(&mark_line(&mark));
        }
        self.marks.push(mark);
    }

    /// Record the adapter eframe picked (shown with the summary).
    pub fn set_adapter(&mut self, adapter: Adapter) {
        self.adapter = Some(adapter);
    }

    /// The adapter line, as printed.
    pub fn adapter_line(&self) -> String {
        match &self.adapter {
            Some(a) => format!(
                "ecad-startup: wgpu adapter: backend {}, name {:?}, device type {}",
                a.backend, a.name, a.device_type
            ),
            None => "ecad-startup: wgpu adapter: none (no wgpu render state)".to_string(),
        }
    }

    /// The marks and the adapter as JSON.
    pub fn to_json(&self) -> serde_json::Value {
        let marks: Vec<_> = self
            .marks
            .iter()
            .map(|m| {
                json!({
                    "mark": m.step.number(),
                    "name": m.step.name(),
                    "label": m.step.label(),
                    "since_start_ms": round(m.since_start_ms),
                    "step_ms": round(m.step_ms),
                })
            })
            .collect();
        let adapter = self
            .adapter
            .as_ref()
            .map(|a| json!({ "backend": a.backend, "name": a.name, "device_type": a.device_type }));
        json!({
            "format": "ecad-startup-timing/1",
            "product": env!("CARGO_PKG_VERSION"),
            "origin": self.origin.name(),
            "before_main_ms": round(self.before_main.as_secs_f64() * 1000.0),
            "main_unix_ms": self.main_unix_ms,
            "complete": self.complete(),
            "total_ms": round(self.marks.last().map_or(0.0, |m| m.since_start_ms)),
            "adapter": adapter,
            "marks": marks,
        })
    }

    /// Whether every step up to [`Step::FirstPresent`] was recorded.
    pub fn complete(&self) -> bool {
        self.marks.last().is_some_and(|m| m.step == Step::FirstPresent)
    }

    /// Write the JSON into the existing folder `dir` as a new file `ecad-startup-<unix ms>.json`
    /// (`-1`, `-2`, ... added when that name exists; an existing file is never replaced), and return
    /// its path.
    ///
    /// # Errors
    /// The file could not be written (the folder is missing, or 100 names are taken).
    pub fn write_json(&self, dir: &Path) -> Result<PathBuf, String> {
        let stamp = self.main_unix_ms.unwrap_or(0);
        let text = serde_json::to_string_pretty(&self.to_json()).map_err(|e| e.to_string())?;
        let mut last = String::new();
        for n in 0..100u32 {
            let name =
                if n == 0 { format!("ecad-startup-{stamp}.json") } else { format!("ecad-startup-{stamp}-{n}.json") };
            let path = dir.join(name);
            match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut f) => {
                    f.write_all(text.as_bytes()).map_err(|e| format!("{}: {e}", path.display()))?;
                    return Ok(path);
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => last = format!("{}: {e}", path.display()),
                Err(e) => return Err(format!("{}: {e}", path.display())),
            }
        }
        Err(format!("no free name for the timing file: {last}"))
    }

    /// Write the JSON into `preferred` (created if missing) when given, otherwise, or if that
    /// fails, into the temp folder. Returns the path and, after a fallback, why `preferred` failed.
    ///
    /// # Errors
    /// Neither folder took the file (both reasons in words).
    pub fn write_json_preferring(&self, preferred: Option<&Path>) -> Result<(PathBuf, Option<String>), String> {
        let mut why = None;
        if let Some(dir) = preferred {
            let attempt = std::fs::create_dir_all(dir)
                .map_err(|e| format!("{}: {e}", dir.display()))
                .and_then(|()| self.write_json(dir));
            match attempt {
                Ok(p) => return Ok((p, None)),
                Err(e) => why = Some(e),
            }
        }
        match self.write_json(&std::env::temp_dir()) {
            Ok(p) => Ok((p, why)),
            Err(e) => Err(match why {
                Some(w) => format!("{w}; {e}"),
                None => e,
            }),
        }
    }

    /// Print the adapter line and the total, and write the JSON (see
    /// [`StartupTiming::write_json_preferring`]), saying on stderr where it went or why not.
    pub fn finish(&self, preferred: Option<&Path>) {
        say(&self.adapter_line());
        let total = self.marks.last().map_or(0.0, |m| m.since_start_ms);
        let partial = if self.complete() { "" } else { " (incomplete: closed before the first frame was presented)" };
        say(&format!("ecad-startup: total {total:.1} ms from {}{partial}", self.origin.name()));
        match self.write_json_preferring(preferred) {
            Ok((p, None)) => say(&format!("ecad-startup: written {}", p.display())),
            Ok((p, Some(why))) => {
                say(&format!("ecad-startup: written {} (the snapshots folder failed: {why})", p.display()))
            }
            Err(e) => say(&format!("ecad-startup: the timing file could not be written: {e}")),
        }
    }
}

/// One line on stderr, ignoring errors: a closed pipe must never stop the trial.
fn say(line: &str) {
    let _ = writeln!(std::io::stderr().lock(), "{line}");
}

/// The folder the timing file prefers: the `--snapshots` folder with `~` expanded as snapshot
/// export does. `None` without snapshots or when `~` cannot be expanded (the temp folder is used).
pub fn preferred_folder(snapshots: Option<&Path>, home: Option<&Path>) -> Option<PathBuf> {
    let dir = snapshots?;
    match dir.to_str() {
        Some(typed) => ecad_cadcraft::expand_home(typed, home).ok(),
        None => Some(dir.to_path_buf()),
    }
}

/// A mark as printed: number, label, time since the origin and the step.
pub fn mark_line(m: &Mark) -> String {
    format!(
        "ecad-startup: {:>2} {:<48} {:>9.1} ms (+{:.1} ms)",
        m.step.number(),
        m.step.label(),
        m.since_start_ms,
        m.step_ms
    )
}

fn round(ms: f64) -> f64 {
    (ms * 1000.0).round() / 1000.0
}

/// How long ago the process was created, where the OS says without `unsafe` (Linux only).
#[cfg(target_os = "linux")]
pub fn process_age() -> Option<Duration> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    let uptime = std::fs::read_to_string("/proc/uptime").ok()?;
    // AT_CLKTCK from the auxiliary vector; 100 is the Linux user-space value when it is missing.
    let ticks = std::fs::read("/proc/self/auxv").ok().and_then(|a| auxv_clock_ticks(&a)).unwrap_or(100);
    process_age_from(&stat, &uptime, ticks)
}

/// How long ago the process was created: not available without `unsafe` or a new dependency on
/// this platform (Windows: `GetProcessTimes`), so the marks are measured from `main`.
#[cfg(not(target_os = "linux"))]
pub fn process_age() -> Option<Duration> {
    None
}

/// The process age from `/proc/self/stat`'s start time (in clock ticks since boot), `/proc/uptime`
/// and the clock ticks per second. `None` for anything malformed or implausible (negative, or over
/// a day).
pub fn process_age_from(stat: &str, uptime: &str, ticks_per_second: u64) -> Option<Duration> {
    if ticks_per_second == 0 {
        return None;
    }
    // The command name (field 2) is in parentheses and may hold spaces or parentheses itself.
    let after = stat.get(stat.rfind(')')?.checked_add(1)?..)?;
    // Fields from 3 (state) on; the start time is field 22.
    let start_ticks: u64 = after.split_whitespace().nth(22 - 3)?.parse().ok()?;
    let up: f64 = uptime.split_whitespace().next()?.parse().ok()?;
    let started = start_ticks as f64 / ticks_per_second as f64;
    let age = up - started;
    (age.is_finite() && (0.0..=86_400.0).contains(&age)).then(|| Duration::from_secs_f64(age))
}

/// `AT_CLKTCK` (17) from a native-endian auxiliary vector of word pairs.
pub fn auxv_clock_ticks(auxv: &[u8]) -> Option<u64> {
    const AT_CLKTCK: u64 = 17;
    let word = (usize::BITS / 8) as usize;
    let read = |b: &[u8]| -> Option<u64> {
        match word {
            8 => b.try_into().ok().map(u64::from_ne_bytes),
            4 => b.try_into().ok().map(u32::from_ne_bytes).map(u64::from),
            _ => None,
        }
    };
    for pair in auxv.chunks_exact(word.checked_mul(2)?) {
        let key = read(pair.get(..word)?)?;
        let value = read(pair.get(word..)?)?;
        match key {
            0 => return None,
            AT_CLKTCK => return (value > 0).then_some(value),
            _ => {}
        }
    }
    None
}
