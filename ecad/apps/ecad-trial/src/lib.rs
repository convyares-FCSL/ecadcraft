//! ecad-trial: the smallest runnable ECAD desktop harness, for manual trials of the ECAD
//! integration (an experiment, not an application).
//!
//! It is CADCraft's own window ([`CadApp`]) with the ECAD commands installed and one ECAD project
//! open at a time. Everything ECAD-specific runs through `ecad-cadcraft`; this crate only adds
//! what a person needs to try it:
//!
//! - a banner naming the experiment, the project, its file and its unsaved changes, with New,
//!   Open, Save and Save As;
//! - **project files** (`.ecad`, persistence stage 2a): Save writes to the project's file, Save As
//!   (and the first Save) asks for a file name in a small dialog of its own (the trial has no
//!   native file dialogs), Open opens a project file; a project another session is editing, or a
//!   damaged one, opens read-only and can be saved as another file;
//! - closing a project with unsaved changes asks **Save**, **Discard** or **Cancel** (it replaces
//!   CADCraft's prompt for ECAD pages, through `CadApp`'s public close API; no upstream change);
//! - optional export of inspector snapshots (`--snapshots <dir>`), which are diagnostic files, not
//!   saved projects;
//! - an **ECAD** tab in CADCraft's Tool Sets panel ([`tools`]): buttons start the click-based
//!   forms of the ECAD commands (brief 008), run a command, or prefill the command field;
//! - terminal and wire-end marks over ECAD pages (drawn by CADCraft from the project's overlay),
//!   and a hover tooltip naming the ECAD object under the pointer and its connections
//!   ([`Trial::hover_lines`]), read only.
//! - the read-only **debugger** panel (persistence stage 2b, [`debugger`]), opened from the
//!   banner and never shown by default: selected drawing objects → ECAD IDs → model objects →
//!   transactions with before/after values → saved record offsets and bytes, with pending
//!   changes kept apart from durably saved history.
//!
//! Excluded on purpose: native file dialogs, CADCraft file formats (plain drawings cannot be
//! written), autosave, the control channel, saved window or profile state and ECAD menus
//! (CADCraft shows no top-level menu supplied by an embedder). The JSON forms of ECAD commands are
//! typed in the banner's command field, for example `wire {"points":[[0,0],[40,0]]}` (CADCraft's
//! own command line takes a space as Enter, so it cannot take a JSON argument); a bare command
//! name starts its interactive form in either field.
//!
//! Project files belong in a local folder: by default `C:\dev\ecad-projects` on Windows (never the
//! OneDrive-redirected Documents folder) and `~/ecad-projects` on Linux, including the Linux home
//! under WSL. Network, synced and removable folders, and Windows drives under `/mnt`, are
//! unverified.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod debugger;
pub mod startup;
pub mod tools;

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use cadcraft_engine::Session;
use cadcraft_engine::doc::Handle;
use cadcraft_ui_egui::closing::{Choice, Closing};
use cadcraft_ui_egui::{CadApp, Services};
use debugger::Debugger;
use ecad_cadcraft::{FileStatus, Persistence, Project, ProjectHandle, session_seed};
use ecad_inspect::EventKind;
use eframe::egui::{self, RichText};
use serde_json::json;

/// Window title.
pub const TITLE: &str = "ECAD trial (experiment)";
/// The default projects folder, as typed (see [`DEFAULT_FOLDER`]).
#[cfg(windows)]
macro_rules! default_folder {
    () => {
        r"C:\dev\ecad-projects"
    };
}
#[cfg(not(windows))]
macro_rules! default_folder {
    () => {
        "~/ecad-projects"
    };
}

/// The banner shown above CADCraft's window contents.
pub const BANNER: &str = concat!(
    "ECAD TRIAL (experiment). Projects are saved as .ecad files. Keep them in a local folder (for example ",
    default_folder!(),
    "); other locations are unverified."
);
/// Said wherever snapshots are mentioned.
pub const SNAPSHOT_NOTE: &str = "Inspector snapshots are diagnostic files for the developer inspector, not saved ECAD projects; they cannot be reopened.";
/// Name of a new project until it is saved.
pub const PROJECT_NAME: &str = "Untitled";
/// Where the Save As dialog suggests saving, created by the first save into it: `C:\dev\ecad-projects`
/// on Windows (owner decision 2026-10-31; never the OneDrive-redirected Documents folder),
/// `~/ecad-projects` elsewhere.
pub const DEFAULT_FOLDER: &str = default_folder!();
/// Command-line usage.
pub const USAGE: &str = concat!(
    "usage: ecad-trial [--snapshots <dir>] [<project.ecad>]\n\n\
ECAD desktop trial harness (experiment). Opens <project.ecad> if given, otherwise starts a new\n\
project. Save and Save As write .ecad project files; keep them in a local folder such as ",
    default_folder!(),
    ".\n--snapshots <dir>  allow exporting inspector snapshots (diagnostic files, not saved projects)\n\
See ecad/docs/quick-start.md."
);

/// What the command line asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Invocation {
    /// Start the trial: open `project` if given, allow snapshots into `snapshots` if given.
    Run { snapshots: Option<PathBuf>, project: Option<String> },
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
    let mut args = args.into_iter();
    while let Some(a) = args.next() {
        match a.to_str() {
            Some("--snapshots") => match args.next() {
                Some(d) => snapshots = Some(PathBuf::from(d)),
                None => return Err("--snapshots needs a folder".into()),
            },
            Some("--help" | "-h") => return Ok(Invocation::Help),
            Some("--version" | "-V") => return Ok(Invocation::Version),
            Some(other) if other.starts_with('-') => return Err(format!("unknown argument {other:?}")),
            Some(path) if project.is_none() => project = Some(path.to_string()),
            Some(extra) => return Err(format!("only one project file can be opened ({extra:?})")),
            None => {
                return Err(format!(
                    "the argument {:?} is not valid Unicode; project files need a Unicode name",
                    a.to_string_lossy()
                ));
            }
        }
    }
    Ok(Invocation::Run { snapshots, project })
}

/// The project line of the banner, for example
/// `Project "demo": 1 page open, file C:\dev\ecad-projects\demo.ecad, no unsaved changes.`
pub fn status_line(s: &Status) -> String {
    let plural = |n: u64, w: &str| format!("{n} {w}{}", if n == 1 { "" } else { "s" });
    let file = s.file.path.as_deref().map_or("not saved to a file yet".to_string(), |p| format!("file {p}"));
    let changes = if s.unsaved == 0 {
        "no unsaved changes".to_string()
    } else {
        format!("{} not saved", plural(s.unsaved, "change"))
    };
    format!("Project \"{}\": {}, {file}, {changes}.", s.name, plural(s.pages as u64, "page") + " open")
}

/// Lines echoed at the command line when a project is created.
const INTRO: &[&str] = &[
    "ECAD trial (experiment): Save As writes the project to a .ecad file; Open (in the banner) opens one.",
    "ECAD tools are in Tool Sets > ECAD: Convert to Component, Add Terminal, Place Symbol, Wire, Connect and \
Disconnect work by clicking (follow the prompts at the command line; Enter finishes, Esc cancels).",
    "JSON forms go in the banner's \"Command\" field (the command line below takes a space as Enter), for \
example:  wire {\"points\":[[0,0],[40,0]]}",
    "ECAD commands: convertcomponent, addterminal, wire, connect, disconnect, ecaddelete, setprop, cableadd, place, \
newpage, openpage, ecadinspect. CADCraft drawing commands work as usual.",
];

/// The user's answer to the close dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    Save,
    Discard,
    Cancel,
}

/// Which file dialog is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogKind {
    Open,
    SaveAs,
}

/// The trial's own file dialog: a path field.
struct FileDialog {
    kind: DialogKind,
    path: String,
    error: Option<String>,
    /// A close waiting for this Save As: it continues once the project is saved.
    then_close: Option<Closing>,
    /// The existing file the user was warned about; saving again replaces it.
    confirmed_replace: Option<String>,
    /// The path field has been given keyboard focus (once, when the dialog opens or fails).
    focused: bool,
}

/// A close of a project with unsaved changes, held until the user answers.
struct Held {
    /// CADCraft's close request, taken out of `CadApp` so its "Save changes?" prompt never shows.
    closing: Closing,
    title: String,
    changes: u64,
}

/// What the banner shows about the open project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub name: String,
    pub pages: usize,
    /// Transactions not yet saved.
    pub unsaved: u64,
    pub file: FileStatus,
}

/// The harness state around CADCraft's [`CadApp`].
pub struct Trial {
    pub cad: CadApp,
    project: Option<ProjectHandle>,
    snapshots: Option<PathBuf>,
    held: Option<Held>,
    dialog: Option<FileDialog>,
    /// Set by CADCraft's Save As (menu, Ctrl+Shift+S, or Save of a project with no file).
    save_as_requested: Arc<AtomicBool>,
    /// Events of the project already reported.
    seen_events: u64,
    /// A message waiting in a dialog.
    notice: Option<String>,
    snapshot_count: u32,
    /// The banner's command field (whole lines, spaces included).
    pub command: String,
    /// What to fill in, after an ECAD Tool Sets button put a template in the command field.
    command_hint: Option<&'static str>,
    /// Give the command field keyboard focus on the next frame.
    focus_command: bool,
    /// The read-only debugger panel (closed until opened from the banner).
    pub debugger: Debugger,
    /// Give the drawing the keyboard on the next frame (after a tool started a prompt).
    focus_drawing: bool,
    /// The ECAD object under the pointer and since when (egui time, seconds).
    hovered: Option<(Hovered, f64)>,
    /// The hover description shown now (design 008 §4.3).
    hover: Option<Vec<String>>,
    /// How long the pointer rests on an object before its description shows, in seconds.
    pub hover_delay: f64,
    /// The home folder that `~` in typed paths stands for ([`ecad_cadcraft::home`] by default).
    pub home: Option<PathBuf>,
    /// The lineweight display the trial last set when it opened a project; `None` before the
    /// first. A different value now means the user chose Show/Hide Lineweight, which is kept.
    lwdisplay_set: Option<bool>,
}

/// What the pointer rests on, for the hover tooltip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hovered {
    /// A terminal within the capture aperture (drawn or symbol, free or connected).
    Terminal(ecad_model::TerminalId),
    /// A cable reference end (its point or its text) within the pick box (brief 010).
    Reference(ecad_model::RepresentationId),
    /// The object CADCraft's rollover highlights.
    Object(Handle),
}

/// The hover tooltip's egui id (and element name, design 008 §4.3).
pub const HOVER_ID: &str = "ecad.hover";
/// Default [`Trial::hover_delay`].
pub const HOVER_DELAY: f64 = 0.5;

/// The trial has no native file dialogs. When CADCraft asks for a file name to save a drawing,
/// the request is passed on (`requested` is set) and the trial opens its own path dialog for the
/// ECAD project; CADCraft itself gets no path, so it writes nothing. Exports get no path and stop.
pub fn services(requested: Arc<AtomicBool>) -> Services {
    Services {
        pick_open: None,
        pick_save: Some(Box::new(move |name: &str| {
            let ext = Path::new(name).extension().map(|e| e.to_string_lossy().to_ascii_lowercase());
            if matches!(ext.as_deref(), None | Some("dwg" | "dxf" | "dwt" | "ecad")) {
                requested.store(true, Ordering::Relaxed);
            }
            None
        })),
        // No APPLOAD file dialog either: CADCraft asks for the file name at its command line.
        pick_lisp: None,
        no_file_system: false,
    }
}

impl Trial {
    /// A harness with one new ECAD project. `snapshots`: where exported snapshots go (none when
    /// `None`).
    ///
    /// # Errors
    /// The project could not be created.
    pub fn new(snapshots: Option<PathBuf>) -> Result<Self, String> {
        Self::start(snapshots, None, None)
    }

    /// A harness opening the project file at `path`. If it cannot be opened, a new project is
    /// created instead and the reason is shown.
    ///
    /// # Errors
    /// Neither the file nor a new project could be opened.
    pub fn with_file(snapshots: Option<PathBuf>, path: &str) -> Result<Self, String> {
        Self::start(snapshots, Some(path), None)
    }

    /// [`Trial::with_file`] when `project` is given, otherwise [`Trial::new`], marking
    /// [`startup::Step::Shell`] after CADCraft's app is built and [`startup::Step::Project`] after
    /// the project is open when `timing` is given.
    ///
    /// # Errors
    /// Neither the file (if any) nor a new project could be opened.
    pub fn start(
        snapshots: Option<PathBuf>,
        project: Option<&str>,
        mut timing: Option<&mut startup::StartupTiming>,
    ) -> Result<Self, String> {
        let mut t = Self::empty(snapshots);
        if let Some(timing) = timing.as_deref_mut() {
            timing.mark(startup::Step::Shell);
        }
        match project {
            Some(path) => {
                if let Err(e) = t.open_project(path) {
                    t.new_project()?;
                    t.notice = Some(format!("The project file could not be opened: {e}"));
                }
            }
            None => t.new_project()?,
        }
        if let Some(timing) = timing {
            timing.mark(startup::Step::Project);
        }
        Ok(t)
    }

    fn empty(snapshots: Option<PathBuf>) -> Self {
        let requested = Arc::new(AtomicBool::new(false));
        let mut cad = CadApp::new(Session::empty(), services(Arc::clone(&requested)));
        cad.extra_tool_tabs = vec![tools::tab()];
        Trial {
            cad,
            project: None,
            snapshots,
            held: None,
            dialog: None,
            save_as_requested: requested,
            seen_events: 0,
            notice: None,
            snapshot_count: 0,
            command: String::new(),
            command_hint: None,
            focus_command: false,
            debugger: Debugger::default(),
            focus_drawing: false,
            hovered: None,
            hover: None,
            hover_delay: HOVER_DELAY,
            home: ecad_cadcraft::home(),
            lwdisplay_set: None,
        }
    }

    /// Create a new, unsaved project (only when none is open).
    ///
    /// # Errors
    /// A project is already open, or creating it failed.
    pub fn new_project(&mut self) -> Result<(), String> {
        if self.project_open() {
            return Err("an ECAD project is already open; close it first".into());
        }
        let p = Project::create_with(&mut self.cad.session, PROJECT_NAME, session_seed(), Persistence::File)?;
        self.attach(p);
        for line in INTRO {
            self.cad.session.echo(*line);
        }
        Ok(())
    }

    /// Open the project file at `path` (only when no project is open). Returns what opening
    /// found, in words; anything beyond a clean open is also shown in a dialog.
    ///
    /// # Errors
    /// A project is already open, or the file could not be opened.
    pub fn open_project(&mut self, path: &str) -> Result<String, String> {
        if self.project_open() {
            return Err("an ECAD project is already open; close it first".into());
        }
        let path = ecad_cadcraft::normalize_with(path, self.home.as_deref())?;
        let (p, summary) = ProjectHandle::open_file(&mut self.cad.session, &path, session_seed())?;
        self.attach(p);
        for line in summary.lines() {
            self.cad.session.echo(line.to_string());
        }
        if summary.lines().count() > 1 {
            self.notice = Some(summary.clone());
        }
        Ok(summary)
    }

    fn attach(&mut self, p: ProjectHandle) {
        self.project = Some(p);
        self.debugger.reset();
        self.seen_events = self.event_count();
        self.cad.ui.start_tab = false;
        // Thick and thin cable lines only show with lineweights on (design 009 §9), unless the
        // user has chosen Show/Hide Lineweight since the trial started (brief 009 review).
        let shown = &mut self.cad.session.settings.lwdisplay;
        if self.lwdisplay_set.is_none_or(|set| *shown == set) {
            *shown = true;
            self.lwdisplay_set = Some(true);
        }
    }

    /// Save the project to its file (Save As when it has none yet).
    ///
    /// # Errors
    /// No project is open, or the save failed (also shown in a dialog).
    pub fn save(&mut self) -> Result<(), String> {
        self.focus_project()?;
        let r = self.cad.run("qsave", serde_json::Value::Null).map(|_| ());
        self.after_logic();
        r
    }

    /// Save the project as `path` (a `.ecad` file; the extension is added when missing; `~` is
    /// [`Trial::home`]). The default folder is created if it does not exist yet; no other folder is.
    ///
    /// # Errors
    /// No project is open, the path is refused, or the save failed (also shown in a dialog).
    pub fn save_as(&mut self, path: &str) -> Result<(), String> {
        self.focus_project()?;
        let target = ecad_cadcraft::normalize_with(path, self.home.as_deref())?;
        if let Some(dir) = Path::new(&target).parent()
            && let Ok(default) = ecad_cadcraft::expand_home(DEFAULT_FOLDER, self.home.as_deref())
            && ecad_cadcraft::same_path(dir, &default)
            && !dir.exists()
        {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let r = self.cad.run("saveas", json!({ "path": target })).map(|_| ());
        self.after_logic();
        r
    }

    /// Make a page of the project the active document (Save and Save As act on it).
    fn focus_project(&mut self) -> Result<(), String> {
        let owned = |t: &Self, uid: u64| t.cad.session.ext.owner_of(uid).is_some();
        if self.cad.session.state().is_ok_and(|st| owned(self, st.uid)) {
            return Ok(());
        }
        let i = self.cad.session.docs.iter().position(|d| owned(self, d.uid)).ok_or("no ECAD project is open")?;
        self.cad.run("document.switch", json!({ "index": i })).map(|_| ())
    }

    /// The current project (open or already closed).
    pub fn project(&self) -> Option<&ProjectHandle> {
        self.project.as_ref()
    }

    /// Whether the project has a page open.
    pub fn project_open(&self) -> bool {
        self.cad.session.docs.iter().any(|d| self.cad.session.ext.owner_of(d.uid).is_some())
    }

    /// Title of the project close waiting for an answer.
    pub fn pending_close(&self) -> Option<&str> {
        self.held.as_ref().map(|h| h.title.as_str())
    }

    /// The message waiting in the notice dialog.
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// Dismiss the notice dialog.
    pub fn dismiss_notice(&mut self) {
        self.notice = None;
    }

    /// The open file dialog: its kind and the path typed so far.
    pub fn dialog(&self) -> Option<(DialogKind, &str, Option<&str>)> {
        self.dialog.as_ref().map(|d| (d.kind, d.path.as_str(), d.error.as_deref()))
    }

    /// Open the Save As dialog, suggesting a name in the default folder (or the current file).
    pub fn request_save_as(&mut self) {
        self.open_dialog(DialogKind::SaveAs, None);
    }

    /// Open the Open dialog.
    pub fn request_open(&mut self) {
        self.open_dialog(DialogKind::Open, None);
    }

    fn open_dialog(&mut self, kind: DialogKind, then_close: Option<Closing>) {
        let suggested = match (kind, self.status()) {
            (DialogKind::SaveAs, Some(s)) => {
                s.file.path.unwrap_or_else(|| in_default_folder(&format!("{}.ecad", s.name.to_lowercase())))
            }
            _ => in_default_folder(""),
        };
        self.dialog = Some(FileDialog {
            kind,
            path: suggested,
            error: None,
            then_close,
            confirmed_replace: None,
            focused: false,
        });
    }

    /// Type a path into the open file dialog.
    pub fn set_dialog_path(&mut self, path: &str) {
        if let Some(d) = &mut self.dialog {
            d.path = path.to_string();
        }
    }

    /// Cancel the open file dialog (a close waiting for it is cancelled too).
    pub fn cancel_dialog(&mut self) {
        if self.dialog.take().is_some_and(|d| d.then_close.is_some()) {
            self.cad.session.echo("Close cancelled; the ECAD project stays open.");
        }
    }

    /// Confirm the open file dialog: open, or save as, the path typed. On failure the dialog
    /// stays open with the reason. Saving over an existing file other than the project's own asks
    /// once more first.
    pub fn confirm_dialog(&mut self) {
        let Some(mut d) = self.dialog.take() else { return };
        let path = d.path.trim().to_string();
        let r = match d.kind {
            DialogKind::Open => self.open_project(&path).map(|_| ()),
            DialogKind::SaveAs => {
                let target = ecad_cadcraft::normalize_with(&path, self.home.as_deref()).map(PathBuf::from);
                let own = self.status().and_then(|s| s.file.path).map(PathBuf::from);
                match target {
                    Ok(t)
                        if t.exists()
                            && !own.as_deref().is_some_and(|o| ecad_cadcraft::same_path(&t, o))
                            && d.confirmed_replace.as_deref() != Some(&path) =>
                    {
                        d.confirmed_replace = Some(path.clone());
                        Err(format!("{} already exists. Press Save again to replace it.", t.display()))
                    }
                    _ => self.save_as(&path),
                }
            }
        };
        match r {
            Ok(()) => {
                if let Some(closing) = d.then_close {
                    // Saved: let CADCraft carry on closing (its Save finds nothing left to write).
                    self.cad.closing = Some(closing);
                    self.cad.answer_close(Choice::Save);
                    self.after_logic();
                }
            }
            Err(e) => {
                // A failed save is shown in the dialog itself, not twice.
                self.notice = None;
                d.error = Some(e);
                d.focused = false;
                self.dialog = Some(d);
            }
        }
    }

    /// The open project's name, pages, unsaved transactions and file.
    pub fn status(&mut self) -> Option<Status> {
        if !self.project_open() {
            return None;
        }
        let p = self.project.as_ref()?;
        p.read(&mut self.cad.session, |p, _| Status {
            name: p.name.clone(),
            pages: p.open_pages(),
            unsaved: p.unsaved_transactions(),
            file: p.file_status(),
        })
        .ok()
    }

    /// Per-frame work after CADCraft's own logic: hold a close of an edited ECAD project for the
    /// trial's dialog, open the Save As dialog when CADCraft asked for a file name, and report
    /// saves.
    pub fn after_logic(&mut self) {
        // Only the page being shown is redrawn on a commit (owner, 2026-11-08): a page a commit
        // elsewhere left stale is redrawn here once its tab is in front. A busy project is tried
        // again next frame (a failed projection is reported by the redraw itself).
        if let Some(p) = self.project.as_ref() {
            let _ = p.refresh_shown(&mut self.cad.session);
        }
        self.handle_tools();
        self.hold_close();
        if self.save_as_requested.swap(false, Ordering::Relaxed) {
            let owned = self.cad.session.state().is_ok_and(|st| self.cad.session.ext.owner_of(st.uid).is_some());
            if owned {
                if self.dialog.is_none() {
                    self.request_save_as();
                }
            } else {
                self.cad.session.echo("Plain CADCraft drawings cannot be saved in the ECAD trial.");
            }
        }
        self.report_saves();
    }

    fn hold_close(&mut self) {
        if self.held.is_some() || self.dialog.as_ref().is_some_and(|d| d.then_close.is_some()) {
            // One question at a time: a further close request waits until this one is answered.
            self.cad.closing = None;
            return;
        }
        let Some(title) = self.cad.close_prompt().map(str::to_string) else { return };
        // CADCraft switches to the drawing it asks about, so the active document is that one.
        let owned = self.cad.session.state().is_ok_and(|st| self.cad.session.ext.owner_of(st.uid).is_some());
        if !owned {
            return; // a plain CADCraft drawing keeps CADCraft's own prompt
        }
        let Some(closing) = self.cad.closing.take() else { return };
        let changes = self.status().map_or(0, |s| s.unsaved);
        self.held = Some(Held { closing, title, changes });
    }

    fn event_count(&mut self) -> u64 {
        let Some(p) = self.project.as_ref() else { return 0 };
        p.read(&mut self.cad.session, |p, _| p.events.last().map_or(0, |e| e.n + 1)).unwrap_or(0)
    }

    /// Show a failed save in a dialog and a successful one at the command line.
    fn report_saves(&mut self) {
        let Some(p) = self.project.as_ref() else { return };
        let seen = self.seen_events;
        let Ok(new) = p.read(&mut self.cad.session, |p, _| {
            p.events.events().filter(|e| e.n >= seen).map(|e| (e.n, e.kind, e.message.clone())).collect::<Vec<_>>()
        }) else {
            return;
        };
        for (n, kind, message) in new {
            self.seen_events = n + 1;
            match kind {
                EventKind::Saved => self.cad.session.echo(format!("Saved: {message}")),
                EventKind::SaveFailed => {
                    self.cad.session.echo(format!("Not saved: {message}"));
                    if self.dialog.is_none() {
                        self.notice = Some(format!("The project was not saved.\n\n{message}"));
                    }
                }
                _ => {}
            }
        }
    }

    /// Answer the close dialog.
    /// - Save: save (asking for a file name first if the project has none, or is read-only),
    ///   then close; if saving fails, the project stays open.
    /// - Discard: export a snapshot (when enabled), then close without saving.
    /// - Cancel: keep everything open.
    pub fn answer(&mut self, a: Answer) {
        let Some(held) = self.held.take() else { return };
        match a {
            Answer::Cancel => self.cad.session.echo("Close cancelled; the ECAD project stays open."),
            Answer::Save => {
                let file = self.status().map(|s| s.file).unwrap_or_default();
                if file.path.is_some() && file.read_only.is_none() {
                    self.cad.closing = Some(held.closing);
                    self.cad.answer_close(Choice::Save);
                    self.after_logic();
                } else {
                    self.open_dialog(DialogKind::SaveAs, Some(held.closing));
                }
            }
            Answer::Discard => {
                let saved_before = self.status().and_then(|s| s.file.path);
                if self.snapshots.is_some()
                    && let Err(e) = self.export_snapshot("discarded")
                {
                    self.cad.session.echo(format!("Snapshot not written: {e}"));
                }
                self.cad.closing = Some(held.closing);
                self.cad.answer_close(Choice::Discard);
                if !self.project_open() {
                    self.cad.session.echo(match saved_before {
                        Some(p) => format!("Unsaved changes were discarded; {p} keeps its last saved state."),
                        None => "The ECAD project was discarded without being saved.".to_string(),
                    });
                }
            }
        }
    }

    /// Run one whole command line, as typed in the banner's field: `name {json}` runs an ECAD or
    /// CADCraft command with parameters; anything else is read like a script line (spaces separate
    /// inputs). Returns the error, which is also echoed at the command line.
    ///
    /// Spaces are inputs: a trailing space is an Enter (`line 0,0 10,0 ` ends LINE), and while a
    /// command is running an empty line is an Enter too. Only leading spaces before a new command
    /// are dropped.
    ///
    /// A line still holding a `<placeholder>` of a template put there by an ECAD Tool Sets button
    /// is refused, so a template is never run with its placeholders.
    pub fn submit_command(&mut self, text: &str) -> Option<String> {
        let running = self.cad.session.running.is_some();
        let text = if running { text } else { text.trim_start() };
        if text.is_empty() && !running {
            return None;
        }
        if self.command_hint.is_some() {
            let left = tools::placeholders(text);
            if !left.is_empty() {
                let e = format!("Replace the placeholders first: {}", left.join(", "));
                self.cad.session.echo(e.clone());
                return Some(e);
            }
        }
        let r = self.run_line(text);
        if r.is_none() {
            self.command_hint = None;
        }
        r
    }

    /// Run one command line through the command path, as the command field does.
    fn run_line(&mut self, text: &str) -> Option<String> {
        self.cad.session.echo(format!("Command: {text}"));
        let r = self.cad.cmdline(text);
        self.after_logic();
        r
    }

    /// What to fill in for the template in the command field, if one was put there.
    pub fn command_hint(&self) -> Option<&str> {
        self.command_hint
    }

    /// Act on a click of an ECAD Tool Sets button (see [`tools`]): start its command's interactive
    /// form (as typing its bare name does; the Command field is cleared and the drawing gets the
    /// keyboard, so the next key answers the prompt), run its command through the command line
    /// (the same path as typing it), or put its template in the command field. The selection is
    /// left as it is. A command waiting at a prompt is cancelled first, as CADCraft's own Tool Sets
    /// buttons do, so the button's line is never taken as prompt input.
    ///
    /// # Errors
    /// An unknown tool, or the command's own refusal (also echoed at the command line).
    pub fn tool(&mut self, id: &str) -> Result<(), String> {
        let Some(tool) = tools::find(id) else {
            let e = format!("unknown ECAD tool {id}");
            self.cad.session.echo(e.clone());
            return Err(e);
        };
        if self.cad.session.running.is_some() {
            self.cad.session.cancel();
        }
        match tool.kind {
            tools::Kind::Start(command) => {
                self.command.clear();
                self.command_hint = None;
                self.focus_drawing = true;
                let r = self.cad.session.start(command).map_err(|e| e.to_string());
                if let Err(e) = &r {
                    self.cad.session.echo(e.clone());
                }
                self.after_logic();
                r
            }
            // A template waiting in the command field keeps its note and its placeholder check.
            tools::Kind::Run(line) => match self.run_line(line) {
                None => Ok(()),
                Some(e) => Err(e),
            },
            tools::Kind::Template => {
                let picked = self.picked();
                let (line, hint) = tools::template(id, &picked).ok_or_else(|| format!("no template for {id}"))?;
                self.command = line;
                self.command_hint = Some(hint);
                self.focus_command = true;
                Ok(())
            }
        }
    }

    /// The current selection and the ECAD objects it names (empty off an ECAD page).
    fn picked(&mut self) -> tools::Picked {
        let Ok(st) = self.cad.session.state() else { return tools::Picked::default() };
        let (uid, handles) = (st.uid, st.selection.clone());
        let Some(p) = self.project.as_ref() else { return tools::Picked { handles, ..Default::default() } };
        let fallback = tools::Picked { handles: handles.clone(), ..Default::default() };
        p.read(&mut self.cad.session, |p, _| tools::Picked::from_selection(p, uid, handles)).unwrap_or(fallback)
    }

    /// Act on every ECAD Tool Sets click waiting since the last frame, each once.
    fn handle_tools(&mut self) {
        for id in self.cad.take_tool_requests() {
            // Refusals are echoed at the command line by the command path itself.
            let _ = self.tool(&id);
        }
    }

    /// Write a live inspector snapshot to the snapshot folder. Returns the file written.
    ///
    /// # Errors
    /// Snapshots are not enabled, there is no project, or writing failed.
    pub fn export_snapshot(&mut self, why: &str) -> Result<PathBuf, String> {
        let dir = self.snapshots.clone().ok_or("snapshots are not enabled (start with --snapshots <dir>)")?;
        // PowerShell and cmd pass `~` through unexpanded: it is the home folder, never a `~` folder.
        let dir = match dir.to_str() {
            Some(typed) => ecad_cadcraft::expand_home(typed, self.home.as_deref())?,
            None => dir,
        };
        let p = self.project.as_ref().ok_or("there is no project")?;
        let snap = p.snapshot(&mut self.cad.session, None)?;
        let head = snap.get("seq").and_then(serde_json::Value::as_u64).unwrap_or(0);
        self.snapshot_count += 1;
        let stamp = ecad_stamp();
        let path = dir.join(format!("ecad-trial-{stamp}-{:02}-seq{head:03}-{why}.json", self.snapshot_count));
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let bytes = serde_json::to_vec_pretty(&snap).map_err(|e| e.to_string())?;
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        self.cad.session.echo(format!("Snapshot written: {} ({SNAPSHOT_NOTE})", path.display()));
        Ok(path)
    }

    /// Per-frame logic (before layout).
    pub fn logic(&mut self, ctx: &egui::Context) {
        self.cad.logic(ctx);
        self.after_logic();
        if std::mem::take(&mut self.cad.quit_requested) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// Open or close the debugger panel.
    pub fn toggle_debugger(&mut self) {
        self.debugger.open = !self.debugger.open;
        if self.debugger.open {
            self.refresh_debugger(false);
        }
    }

    /// Bring the debugger's snapshot up to date (a new one only when something changed, or
    /// always with `force`). The project is locked only while it is copied.
    pub fn refresh_debugger(&mut self, force: bool) {
        if let Some(p) = &self.project {
            self.debugger.refresh(p, &mut self.cad.session, force);
        }
    }

    /// Apply a debugger action that needs the project (Refresh, a check).
    pub fn debugger_action(&mut self, a: debugger::Action) {
        if let Some(p) = &self.project {
            self.debugger.apply(a, p, &mut self.cad.session);
        }
    }

    /// Lay out the banner, the harness dialogs, the debugger (when open) and CADCraft's window
    /// contents.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.banner(ui);
        self.dialogs(ui.ctx());
        if self.debugger.open {
            // The snapshot is taken (and the project released) before anything is drawn.
            self.refresh_debugger(false);
            let action = egui::Panel::right("ecad_debugger")
                .resizable(true)
                .default_size(460.0)
                .min_size(280.0)
                .max_size(620.0)
                .show(ui, |ui| self.debugger.show(ui))
                .inner;
            if let Some(a) = action {
                self.debugger_action(a);
            }
        }
        self.cad.ui(ui);
        self.handle_tools();
        if std::mem::take(&mut self.focus_drawing) {
            // Keys go to CADCraft's command line when no text field has the keyboard.
            ui.ctx().memory_mut(|m| {
                if let Some(id) = m.focused() {
                    m.surrender_focus(id);
                }
            });
        }
        self.hover_tooltip(ui.ctx());
    }

    /// The terminal within the capture aperture (APERTURE pixels) of screen point `at` on the
    /// drawing area, with its hover description; none while a paper-space layout is shown. Only
    /// reads.
    /// The cable reference end under the pointer (its point within the pick box, or its text),
    /// with its hover lines; model space only, as for terminals.
    fn reference_under(&mut self, at: egui::Pos2) -> Option<(ecad_model::RepresentationId, Vec<String>)> {
        if self.cad.session.layout_space() != cadcraft_engine::doc::Space::Model {
            return None;
        }
        let (rect, xf) = (self.cad.canvas.rect?, self.cad.canvas.xf?);
        if !rect.contains(at) {
            return None;
        }
        let world = xf.to_world(at);
        let radius = self.cad.session.pixel_size() * self.cad.session.settings.pickbox.max(1.0) * 1.5;
        let uid = self.cad.session.state().ok()?.uid;
        let p = self.project.as_ref()?;
        p.read(&mut self.cad.session, |p, _| ecad_cadcraft::interact::hover_reference(p, uid, world, radius))
            .ok()
            .flatten()
    }

    fn terminal_under(&mut self, at: egui::Pos2) -> Option<(ecad_model::TerminalId, Vec<String>)> {
        // Terminals are at model positions: in a paper-space layout (or one of its viewports) the
        // canvas shows the sheet, and nothing on it is a terminal.
        if self.cad.session.layout_space() != cadcraft_engine::doc::Space::Model {
            return None;
        }
        let (rect, xf) = (self.cad.canvas.rect?, self.cad.canvas.xf?);
        if !rect.contains(at) {
            return None;
        }
        let world = xf.to_world(at);
        let aperture = self.cad.session.pixel_size() * self.cad.session.settings.aperture.max(1.0);
        let uid = self.cad.session.state().ok()?.uid;
        let p = self.project.as_ref()?;
        p.read(&mut self.cad.session, |p, _| ecad_cadcraft::interact::hover_terminal(p, uid, world, aperture))
            .ok()
            .flatten()
    }

    /// The hover description shown now, if any (design 008 §4.3).
    pub fn hover_lines(&self) -> Option<&[String]> {
        self.hover.as_deref()
    }

    /// With no command running, the ECAD object CADCraft highlights under the pointer gets a
    /// tooltip after [`Trial::hover_delay`]: its name or ID and its connections. It goes when the
    /// pointer leaves the object, a button is pressed or a command starts. Only reads.
    fn hover_tooltip(&mut self, ctx: &egui::Context) {
        let (now, pointer, pressed) = ctx.input(|i| (i.time, i.pointer.hover_pos(), i.pointer.any_down()));
        let idle = self.cad.session.running.is_none() && !pressed;
        // A terminal under the pointer wins over the wire ending on it or the symbol around it.
        let terminal = match pointer.filter(|_| idle) {
            Some(at) => self.terminal_under(at),
            None => None,
        };
        // Then a cable reference end, which wins over the wire starting on its point (brief 010).
        let reference = match (pointer.filter(|_| idle), &terminal) {
            (Some(at), None) => self.reference_under(at),
            _ => None,
        };
        let target = match (terminal, reference) {
            (Some((t, lines)), _) => Some((Hovered::Terminal(t), Some(lines))),
            (None, Some((r, lines))) => Some((Hovered::Reference(r), Some(lines))),
            (None, None) => self.cad.canvas.hover.filter(|_| idle).map(|h| (Hovered::Object(h), None)),
        };
        let (Some((h, terminal_lines)), Some(pointer)) = (target, pointer) else {
            self.hovered = None;
            self.hover = None;
            return;
        };
        let since = match self.hovered {
            Some((was, since)) if was == h => since,
            _ => {
                self.hover = None;
                now
            }
        };
        self.hovered = Some((h, since));
        if now - since < self.hover_delay {
            // A hostile delay (NaN, infinite, huge) never panics: it falls back to a short wait.
            let wait = std::time::Duration::try_from_secs_f64((self.hover_delay - (now - since)).clamp(0.01, 3600.0))
                .unwrap_or(std::time::Duration::from_millis(10));
            ctx.request_repaint_after(wait);
            return;
        }
        if self.hover.is_none() {
            self.hover = match h {
                Hovered::Terminal(_) | Hovered::Reference(_) => terminal_lines,
                Hovered::Object(h) => {
                    let uid = self.cad.session.state().map(|st| st.uid).ok();
                    match (self.project.as_ref(), uid) {
                        (Some(p), Some(uid)) => p
                            .read(&mut self.cad.session, |p, _| ecad_cadcraft::interact::hover_lines(p, uid, h))
                            .ok()
                            .flatten(),
                        _ => None,
                    }
                }
            };
        }
        let Some(lines) = &self.hover else { return };
        let t = cadcraft_ui_egui::theme::Tokens::get();
        egui::Area::new(egui::Id::unique(HOVER_ID))
            .order(egui::Order::Tooltip)
            .fixed_pos(pointer + egui::vec2(16.0, 16.0))
            .constrain(true)
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(t.panel)
                    .stroke(egui::Stroke::new(1.0, t.border))
                    .corner_radius(0.0)
                    .inner_margin(egui::Margin::symmetric(6, 4))
                    .show(ui, |ui| {
                        for (i, line) in lines.iter().enumerate() {
                            let dim = i > 0 && (line.starts_with("id ") || line.contains("not connected"));
                            let text = RichText::new(line).monospace().small();
                            ui.label(text.color(if dim { t.text_dim } else { t.text }));
                        }
                    });
            });
    }

    /// The app is exiting normally: export a final snapshot if enabled and a project is open.
    pub fn on_exit(&mut self) {
        if self.snapshots.is_some() && self.project_open() {
            let _ = self.export_snapshot("exit");
        }
    }

    fn banner(&mut self, ui: &mut egui::Ui) {
        let status = self.status();
        let warn = ui.visuals().warn_fg_color;
        #[derive(PartialEq)]
        enum Click {
            New,
            Open,
            Save,
            SaveAs,
            Export,
            Debugger,
        }
        let mut click = None;
        let mut submit: Option<String> = None;
        egui::Panel::top("ecad_trial_banner").show(ui, |ui| {
            ui.label(RichText::new(BANNER).strong().color(warn));
            ui.horizontal_wrapped(|ui| match &status {
                Some(s) => {
                    ui.label(status_line(s));
                    if let Some(r) = &s.file.read_only {
                        ui.label(RichText::new(format!("Read-only: {r}. Use Save As.")).color(warn));
                    }
                    if s.file.unconfirmed {
                        ui.label(RichText::new("Last save not confirmed durable: Save again.").color(warn));
                    }
                    if ui.button("Save").clicked() {
                        click = Some(Click::Save);
                    }
                    if ui.button("Save As…").clicked() {
                        click = Some(Click::SaveAs);
                    }
                    if self.snapshots.is_some()
                        && ui.button("Export inspector snapshot").on_hover_text(SNAPSHOT_NOTE).clicked()
                    {
                        click = Some(Click::Export);
                    }
                    let label = if self.debugger.open { "Hide debugger" } else { "Debugger" };
                    if ui.button(label).on_hover_text(debugger::HOVER).clicked() {
                        click = Some(Click::Debugger);
                    }
                }
                None => {
                    ui.label("No ECAD project is open.");
                    if ui.button("New project").clicked() {
                        click = Some(Click::New);
                    }
                    if ui.button("Open project…").clicked() {
                        click = Some(Click::Open);
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label("Command:");
                let field = ui.add(
                    egui::TextEdit::singleline(&mut self.command)
                        .hint_text("wire {\"points\":[[0,0],[40,0]]}")
                        .desired_width(ui.available_width() - 60.0),
                );
                let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let running = self.cad.session.running.is_some();
                if (ui.button("Run").clicked() || enter) && (running || !self.command.trim().is_empty()) {
                    submit = Some(std::mem::take(&mut self.command));
                    field.request_focus();
                } else if std::mem::take(&mut self.focus_command) {
                    // An ECAD Tool Sets button put a template here: edit it next.
                    field.request_focus();
                }
            });
            if self.command.trim().is_empty() && submit.is_none() {
                self.command_hint = None;
            }
            if let Some(hint) = self.command_hint {
                ui.label(RichText::new(hint).small());
            }
        });
        if let Some(text) = submit
            && self.submit_command(&text).is_some()
        {
            self.command = text; // keep it for correction
        }
        match click {
            Some(Click::New) => {
                if let Err(e) = self.new_project() {
                    self.notice = Some(e);
                }
            }
            Some(Click::Open) => self.request_open(),
            Some(Click::Save) => {
                let _ = self.save();
            }
            Some(Click::SaveAs) => self.request_save_as(),
            Some(Click::Debugger) => self.toggle_debugger(),
            Some(Click::Export) => {
                self.notice = Some(match self.export_snapshot("manual") {
                    Ok(p) => format!("Snapshot written to {}.\n\n{SNAPSHOT_NOTE}", p.display()),
                    Err(e) => format!("Snapshot not written: {e}"),
                });
            }
            None => {}
        }
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        if let Some(h) = &self.held {
            self.close_dialog(ctx, h.title.clone(), h.changes);
        } else if self.dialog.is_some() {
            self.file_dialog(ctx);
        } else if let Some(msg) = self.notice.clone() {
            let mut ok = false;
            egui::Modal::new(egui::Id::unique("ecad_trial_notice")).show(ctx, |ui| {
                ui.set_width(460.0);
                ui.label(RichText::new("ECAD trial").strong());
                ui.add_space(4.0);
                ui.label(msg);
                ui.add_space(10.0);
                ok = ui.button("OK").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape));
            });
            if ok {
                self.notice = None;
            }
        }
    }

    fn close_dialog(&mut self, ctx: &egui::Context, title: String, changes: u64) {
        let file = self.status().map(|s| s.file).unwrap_or_default();
        let name = self.status().map_or_else(|| PROJECT_NAME.to_string(), |s| s.name);
        let mut answer = None;
        egui::Modal::new(egui::Id::unique("ecad_trial_close")).show(ctx, |ui| {
            ui.set_width(460.0);
            ui.label(RichText::new(format!("Save changes to project \"{name}\"?")).strong());
            ui.add_space(4.0);
            ui.label(format!(
                "\"{title}\" is the last open page of the project, which has {changes} unsaved change{}.",
                if changes == 1 { "" } else { "s" }
            ));
            ui.label(match (&file.path, &file.read_only) {
                (Some(_), Some(r)) => format!("The project is read-only ({r}); Save asks for another file name."),
                (Some(p), None) => format!("Save writes them to {p}."),
                (None, _) => "The project has no file yet; Save asks for a file name.".to_string(),
            });
            ui.label(
                RichText::new("Discard closes it and loses the unsaved changes.").color(ui.visuals().warn_fg_color),
            );
            if self.snapshots.is_some() {
                ui.label(format!("Discard exports a snapshot first. {SNAPSHOT_NOTE}"));
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    answer = Some(Answer::Save);
                }
                if ui.button("Discard").clicked() {
                    answer = Some(Answer::Discard);
                }
                if ui.button("Cancel").clicked() {
                    answer = Some(Answer::Cancel);
                }
            });
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                answer = Some(Answer::Cancel);
            }
        });
        if let Some(a) = answer {
            self.answer(a);
            ctx.request_repaint();
        }
    }

    fn file_dialog(&mut self, ctx: &egui::Context) {
        let Some(d) = &mut self.dialog else { return };
        let (heading, action) = match d.kind {
            DialogKind::Open => ("Open project", "Open"),
            DialogKind::SaveAs => ("Save project as", "Save"),
        };
        let mut confirm = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::unique("ecad_trial_file")).show(ctx, |ui| {
            ui.set_width(520.0);
            ui.label(RichText::new(heading).strong());
            ui.add_space(4.0);
            ui.label(format!(
                "A .ecad project file (the extension is added if missing). Keep projects in a local folder, for \
example {}; other locations are unverified.",
                in_default_folder("demo.ecad")
            ));
            let field = ui.add(egui::TextEdit::singleline(&mut d.path).desired_width(f32::INFINITY));
            // Enter ends the text field's input (it gives up focus): checked before focus is
            // requested, which would otherwise cancel that.
            if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                confirm = true;
            } else if !d.focused {
                field.request_focus();
                d.focused = true;
            }
            if let Some(e) = &d.error {
                ui.label(RichText::new(e.as_str()).color(ui.visuals().warn_fg_color));
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                confirm |= ui.button(action).clicked();
                cancel = ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape));
            });
        });
        if cancel {
            self.cancel_dialog();
        } else if confirm {
            self.confirm_dialog();
        }
        if cancel || confirm {
            ctx.request_repaint();
        }
    }
}

/// `name` in [`DEFAULT_FOLDER`], as typed (with the platform's separator; a trailing one when
/// `name` is empty).
fn in_default_folder(name: &str) -> String {
    format!("{DEFAULT_FOLDER}{}{name}", std::path::MAIN_SEPARATOR)
}

/// Seconds since the epoch, for unique snapshot file names.
fn ecad_stamp() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}
