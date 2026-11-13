//! ecad-trial: the smallest runnable ECAD desktop harness, for manual trials of the ECAD
//! integration (an experiment, not an application).
//!
//! It is CADCraft's own window ([`CadApp`]) with the ECAD commands installed and one ECAD project
//! open at a time. Everything ECAD-specific runs through `ecad-cadcraft`; this crate only adds
//! what a person needs to try it:
//!
//! - the **Project** panel on the far left ([`project_panel`], brief 016): the project's pages,
//!   one click to make a page the one active page, new / rename / reorder through the page
//!   commands, and in its header the project's status (name, file, unsaved changes, read-only
//!   and unconfirmed-save warnings), New, Open, Save, Save As and Close, and the experiment note
//!   (they were in a top banner until owner, 2026-11-08);
//! - a read-only **preview** of another page ([`preview`], brief 016), floating over the drawing;
//! - **project files** (`.ecad`, persistence stage 2a): Save writes to the project's file, Save As
//!   (and the first Save) asks for a file name in a small dialog of its own (the trial has no
//!   native file dialogs), Open opens a project file; a project another session is editing, or a
//!   damaged one, opens read-only and can be saved as another file;
//! - one page window: closing its tab leaves the project open with no page shown (a click in the
//!   panel shows a page again); closing the project (the panel's Close, or quitting) with unsaved
//!   changes asks **Save**, **Discard** or **Cancel** (it replaces CADCraft's prompt for ECAD
//!   pages, through `CadApp`'s public close API; no upstream change);
//! - optional export of inspector snapshots (`--snapshots <dir>`), which are diagnostic files, not
//!   saved projects;
//! - an **ECAD** tab in CADCraft's Tool Sets panel ([`tools`]): buttons start the click-based
//!   forms of the ECAD commands (brief 008), run a command, or open the ECAD command field with a
//!   template; and an **ECAD Dev** tab beside it: the debugger, snapshot export and the ECAD
//!   command window;
//! - terminal and wire-end marks over ECAD pages (drawn by CADCraft from the project's overlay),
//!   and a hover tooltip naming the ECAD object under the pointer and its connections
//!   ([`Trial::hover_lines`]), read only.
//! - the read-only **debugger** panel (persistence stage 2b, [`debugger`]), opened from ECAD Dev
//!   and never shown by default: selected drawing objects → ECAD IDs → model objects →
//!   transactions with before/after values → saved record offsets and bytes, with pending
//!   changes kept apart from durably saved history.
//!
//! Excluded on purpose: native file dialogs, CADCraft file formats (plain drawings cannot be
//! written), autosave, the control channel, saved window or profile state and ECAD menus
//! (CADCraft shows no top-level menu supplied by an embedder). The JSON forms of ECAD commands are
//! typed in the ECAD command field, for example `wire {"points":[[0,0],[40,0]]}` (CADCraft's
//! own command line takes a space as Enter, so it cannot take a JSON argument with spaces); a bare
//! command name starts its interactive form in either.
//!
//! Project files belong in a local folder: by default `C:\dev\ecad-projects` on Windows (never the
//! OneDrive-redirected Documents folder) and `~/ecad-projects` on Linux, including the Linux home
//! under WSL. Network, synced and removable folders, and Windows drives under `/mnt`, are
//! unverified.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod checks_panel;
pub mod debugger;
pub mod developer;
pub mod devices_panel;
pub mod history_panel;
pub mod merge_dialog;
pub mod outputs_ui;
pub mod preview;
pub mod project_panel;
pub mod release_ui;
pub mod renumber_dialog;
pub mod search;
pub mod sheets_ui;
pub mod startup;
pub mod text_ui;
pub mod tools;
pub mod window;

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
use ecad_model::PageId;
use eframe::egui::{self, RichText};
use preview::Preview;
use project_panel::{Header, ProjectPanel};
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

/// What the trial is, in full (the Project panel's experiment line shows [`NOTE`] with this as
/// its hover text; the top banner that showed it was removed, owner 2026-11-08).
pub const BANNER: &str = concat!(
    "ECAD TRIAL (experiment). Projects are saved as .ecad files. Keep them in a local folder (for example ",
    default_folder!(),
    "); other locations are unverified."
);
/// The experiment line at the top of the Project panel (one small line; [`BANNER`] on hover).
pub const NOTE: &str = "ECAD trial (experiment): keep .ecad files in a local folder.";
/// Said wherever snapshots are mentioned.
pub const SNAPSHOT_NOTE: &str =
    "Inspector snapshots are diagnostic files for the developer inspector, not saved ECAD projects; they cannot be reopened.";
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
                return Err(format!("the argument {:?} is not valid Unicode; project files need a Unicode name", a.to_string_lossy()));
            }
        }
    }
    Ok(Invocation::Run { snapshots, project })
}

/// The project's status line at the top of the Project panel, for example
/// `Project "demo": 2 pages, file C:\dev\ecad-projects\demo.ecad, no unsaved changes.`
pub fn status_line(s: &Status) -> String {
    let plural = |n: u64, w: &str| format!("{n} {w}{}", if n == 1 { "" } else { "s" });
    let file = s.file.path.as_deref().map_or("not saved to a file yet".to_string(), |p| format!("file {p}"));
    let changes = if s.unsaved == 0 { "no unsaved changes".to_string() } else { format!("{} not saved", plural(s.unsaved, "change")) };
    format!("Project \"{}\": {}, {file}, {changes}.", s.name, plural(s.pages as u64, "page"))
}

/// Lines echoed at the command line when a project is created.
const INTRO: &[&str] = &[
    "ECAD trial (experiment): the PROJECT panel on the left lists the pages (click one to show it) and has \
New, Open, Save and Save As.",
    "ECAD tools are in Tool Sets > ECAD: Convert to Component, Add Connection Point, Place Symbol, Wire, Connect and \
Disconnect work by clicking (follow the prompts at the command line; Enter finishes, Esc cancels).",
    "JSON forms go in the ECAD command field (PROJECT panel > Developer, at the top; the command line below takes \
a space as Enter), for example:  wire {\"points\":[[0,0],[40,0]]}",
    "ECAD commands: convertcomponent, addterminal, wire, connect, disconnect, ecaddelete, setprop, cableadd, place, \
newpage, openpage, ecadinspect. CADCraft drawing commands work as usual.",
];

/// What the page name box does on OK.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameBoxKind {
    /// `newpage` with the typed title; the new page is shown.
    New,
    /// `renamepage` of this page with the typed title.
    Rename(PageId),
}

/// The small page name box opened by Tool Sets' New Page and Rename Page and the Project
/// panel's **+** (owner, 2026-11-08): prefilled, OK or Enter runs one command (one transaction,
/// one `u`), Cancel or Esc does nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameBox {
    pub kind: NameBoxKind,
    pub text: String,
    /// The command's refusal, shown in the box (which stays open).
    pub error: Option<String>,
    focused: bool,
}

/// The drawing height a Devices tab go to shows at least and at most, in drawing units (mm).
pub const GOTO_MIN_HEIGHT: f64 = 60.0;
/// See [`GOTO_MIN_HEIGHT`].
pub const GOTO_MAX_HEIGHT: f64 = 600.0;

/// Accessible label of the name box's field.
pub const NAME_BOX_FIELD: &str = "Page title";

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

/// What the Project panel's header shows about the open project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub name: String,
    /// Pages in the project.
    pub pages: usize,
    /// Transactions not yet saved.
    pub unsaved: u64,
    pub file: FileStatus,
}

/// What the title, header and Revisions show about branches (design 020 §3.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchInfo {
    /// The branch shown.
    pub name: String,
    /// Branches in the file (open or not).
    pub count: usize,
    /// On a branch other than main: `Branch "Option B" · from Rev A`.
    pub bar: Option<String>,
    pub parent: Option<String>,
    /// Revisions may be issued here.
    pub release_line: bool,
    /// The file's name, for the title.
    pub file: Option<String>,
}

/// The window title (owner answer 15): the branch once the project has more than one.
pub fn window_title(info: Option<&BranchInfo>) -> String {
    match info {
        Some(b) if b.count > 1 => format!("{TITLE} — {} · {}", b.file.as_deref().unwrap_or("unsaved"), b.name),
        _ => TITLE.to_string(),
    }
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
    /// A version 1 file the user tried to open, and the new file converting it would write
    /// (brief 021: offered, never silent).
    convert_offer: Option<(String, String)>,
    snapshot_count: u32,
    /// The ECAD command field's field (whole lines, spaces included).
    pub command: String,
    /// What to fill in, after an ECAD Tool Sets button put a template in the command field.
    command_hint: Option<&'static str>,
    /// Give the command field keyboard focus on the next frame.
    focus_command: bool,
    /// The Project panel's Developer section: the command field's reply, the inline debugger and
    /// the project summary.
    pub developer: developer::Developer,
    /// The read-only debugger panel (closed until opened from ECAD Dev).
    pub debugger: Debugger,
    /// The Project panel (brief 016).
    pub panel: ProjectPanel,
    /// The read-only preview of another page (brief 016), when open.
    pub preview: Option<Preview>,
    /// Which project the panel's copy belongs to: counts projects attached in this session, and
    /// branch switches (brief 020a: every copy rebuilds once, as on open).
    project_serial: u64,
    /// Brief 020a: the branch facts the title, header and Revisions show, with the project serial
    /// and branch-action count they were read at (read again only when either moves).
    branch_info: Option<((u64, u64), Option<BranchInfo>)>,
    /// The project's branch-action count and the branch shown, as last seen ([`Trial::sync_branch`]).
    branch_seen: Option<(u64, Option<String>)>,
    /// Counts branch actions run (rename, archive, a new branch): the branch facts are read again.
    branch_rev: u64,
    /// The window title last sent.
    title_sent: String,
    /// The `keep_open` the trial last gave the project: on, except while a close or quit asks.
    keep_open: Option<bool>,
    /// The page name box (New Page and Rename Page), when open.
    pub name_box: Option<NameBox>,
    /// A Save As that a close waits for is being confirmed (the close goes on after it).
    confirming_close: bool,
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
    /// The symbol search window (brief 018), opened from the ECAD tab's Find Symbol.
    pub search: search::SymbolSearch,
    /// Brief 024: the Reports window and the Print Book dialog, while open.
    pub reports: Option<outputs_ui::Reports>,
    pub print_book: Option<outputs_ui::PrintBook>,
    /// Brief 027: the Release dialog.
    pub release: Option<release_ui::ReleaseDialog>,

    /// Brief 019: the ECAD section of CADCraft's Properties palette, shared with its closure.
    pub props: sheets_ui::Shared,
    /// What the section's copy was built for: project serial, change count, page, selection.
    props_seen: Option<(u64, u64, Option<PageId>, Vec<Handle>)>,
    /// How many times the section's copy was built (tests: only when it can differ).
    pub props_rebuilds: u64,
    /// The Project Properties and Revisions dialogs, when open.
    pub project_props: Option<sheets_ui::ProjectProps>,
    pub revisions: Option<sheets_ui::Revisions>,
    /// The merge dialog (brief 020b), when open.
    pub merge: Option<merge_dialog::MergeDialog>,
    /// The Renumber dialog (brief 026), also Finalise numbering, when open.
    pub renumber: Option<renumber_dialog::RenumberDialog>,
    /// Brief 026 (design §5.5): the issue line waiting on the provisional question, and its text.
    pub issue_prompt: Option<(String, String)>,
    /// The issue line to run once Finalise numbering is applied (Finalise numbering now).
    issue_after_finalise: Option<String>,
    /// The folder the page name box's new page goes in.
    new_page_folder: Option<ecad_model::FolderId>,
    /// The revision shown in the panel (its log point), when one is open.
    panel_revision: Option<u64>,
    /// The change count the Revisions dialog was filled at (design 019 §9: it follows
    /// `SetProperty(Project, rev.*)` and Name/UnnameVersion, so undo and redo too).
    revisions_seen: u64,
    /// The selection last checked for frame objects.
    frame_checked: Vec<Handle>,
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
    pub fn start(snapshots: Option<PathBuf>, project: Option<&str>, mut timing: Option<&mut startup::StartupTiming>) -> Result<Self, String> {
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
        let props: sheets_ui::Shared = Arc::default();
        cad.owner_properties = Some(sheets_ui::section(Arc::clone(&props)));
        Trial {
            cad,
            project: None,
            snapshots,
            held: None,
            dialog: None,
            save_as_requested: requested,
            seen_events: 0,
            notice: None,
            convert_offer: None,
            snapshot_count: 0,
            command: String::new(),
            command_hint: None,
            focus_command: false,
            developer: developer::Developer::default(),
            debugger: Debugger::default(),
            panel: ProjectPanel::default(),
            preview: None,
            project_serial: 0,
            branch_info: None,
            branch_seen: None,
            branch_rev: 0,
            title_sent: TITLE.to_string(),
            keep_open: None,
            confirming_close: false,
            name_box: None,
            focus_drawing: false,
            hovered: None,
            hover: None,
            hover_delay: HOVER_DELAY,
            home: ecad_cadcraft::home(),
            lwdisplay_set: None,
            search: search::SymbolSearch::default(),
            reports: None,
            print_book: None,
            release: None,

            props,
            props_seen: None,
            props_rebuilds: 0,
            project_props: None,
            revisions: None,
            merge: None,
            renumber: None,
            issue_prompt: None,
            issue_after_finalise: None,
            new_page_folder: None,
            panel_revision: None,
            revisions_seen: 0,
            frame_checked: Vec::new(),
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
        if let Some(out) = ecad_cadcraft::file::needs_conversion(&path) {
            // Never opened for editing, never converted silently: offer it.
            self.convert_offer = Some((path.clone(), out.clone()));
            return Err(format!(
                "{path} is a version 1 project file. It can be converted to a new version 2 file, {out}; \
                 the original is left untouched."
            ));
        }
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
        self.project_serial += 1;
        self.branch_seen = None;
        self.keep_open = None;
        self.panel.clear();
        self.preview = None;
        self.sync_keep_open();
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
        // Symbols snap to the 5 mm terminal grid of ECAD pages (design 018 §5.3, owner answer
        // Q8); F9 turns it off as in CADCraft.
        self.cad.session.settings.snapmode = true;
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
        if let Some(i) = self.cad.session.docs.iter().position(|d| owned(self, d.uid)) {
            return self.cad.run("document.switch", json!({ "index": i })).map(|_| ());
        }
        // The page window was closed (design 016 §4.1): show the page again first.
        self.show_page_window().map(|_| ())
    }

    /// Open the page window again, on the page shown last (else the first page). Returns its uid.
    fn show_page_window(&mut self) -> Result<u64, String> {
        let p = self.project.clone().ok_or("no ECAD project is open")?;
        let page = p
            .read(&mut self.cad.session, |p, _| p.active_page().or(self.panel.active).or_else(|| p.ledger.document().page_order.first().copied()))?
            .ok_or("the project has no pages")?;
        p.open_page(&mut self.cad.session, page)
    }

    /// The current project (open or already closed).
    pub fn project(&self) -> Option<&ProjectHandle> {
        self.project.as_ref()
    }

    /// Whether a project is open (with or without its page window, design 016 §4.1).
    pub fn project_open(&self) -> bool {
        self.project.as_ref().is_some_and(|p| p.is_attached(&self.cad.session))
    }

    /// Whether the project's page window is open.
    pub fn page_window_open(&self) -> bool {
        self.cad.session.docs.iter().any(|d| self.cad.session.ext.owner_of(d.uid).is_some())
    }

    /// Title of the project close waiting for an answer.
    pub fn pending_close(&self) -> Option<&str> {
        self.held.as_ref().map(|h| h.title.as_str())
    }

    /// The version 1 file waiting for the user's answer to the conversion offer, and the file it
    /// would write.
    pub fn convert_offer(&self) -> Option<(&str, &str)> {
        self.convert_offer.as_ref().map(|(a, b)| (a.as_str(), b.as_str()))
    }

    /// Accept the conversion offer: convert the version 1 file to its new version 2 file
    /// (verified by replaying both; the original is untouched), then open the new file.
    ///
    /// # Errors
    /// Nothing is offered, the conversion failed (nothing was written), or the new file could
    /// not be opened.
    pub fn accept_conversion(&mut self) -> Result<String, String> {
        let (source, output) = self.convert_offer.take().ok_or("no conversion is offered")?;
        // The blank project the trial started with (a file was given at startup) closes first;
        // one with unsaved work asks, and then the offer stays.
        if self.project_open() {
            self.close_project();
            if self.project_open() {
                self.convert_offer = Some((source, output));
                return Err("close the open project first; the conversion is still offered".into());
            }
        }
        let (report, out) = ecad_cadcraft::file::convert(&source, Some(&output), &ecad_cadcraft::writer())?;
        self.cad.session.echo(report.clone());
        self.dialog = None;
        let summary = self.open_project(&out)?;
        self.notice = Some(format!("{report}\n\n{summary}"));
        Ok(summary)
    }

    /// Decline the conversion offer: nothing is opened or written.
    pub fn decline_conversion(&mut self) {
        if self.convert_offer.take().is_some() {
            self.cad.session.echo("Not converted; nothing was opened or written.");
        }
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
            (DialogKind::SaveAs, Some(s)) => s.file.path.unwrap_or_else(|| in_default_folder(&format!("{}.ecad", s.name.to_lowercase()))),
            _ => in_default_folder(""),
        };
        self.dialog = Some(FileDialog { kind, path: suggested, error: None, then_close, confirmed_replace: None, focused: false });
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
        self.sync_keep_open();
    }

    /// Confirm the open file dialog: open, or save as, the path typed. On failure the dialog
    /// stays open with the reason. Saving over an existing file other than the project's own asks
    /// once more first.
    pub fn confirm_dialog(&mut self) {
        let Some(mut d) = self.dialog.take() else { return };
        self.confirming_close = d.then_close.is_some();
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
        self.confirming_close = false;
        self.sync_keep_open();
    }

    /// The open project's name, pages, unsaved transactions and file.
    pub fn status(&mut self) -> Option<Status> {
        if !self.project_open() {
            return None;
        }
        let p = self.project.as_ref()?;
        p.read(&mut self.cad.session, |p, _| Status {
            name: p.name.clone(),
            pages: p.ledger.document().page_order.len(),
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
        self.handle_tools();
        self.drop_frame_selection();
        self.hold_close();
        self.sync_keep_open();
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

    /// Zoom the page window to its drawing without the sheet frame (brief 019: Zoom Extents fits
    /// the whole sheet). Returns `false` when there is no drawing to zoom to.
    pub fn zoom_to_drawing(&mut self) -> bool {
        let Ok(d) = self.cad.session.doc() else { return false };
        let mut b = cadcraft_engine::geom::Bounds2::default();
        for e in d.model.iter().filter(|e| e.common.layer != ecad_cadcraft::frame::LAYER) {
            b = b.union(&cadcraft_engine::doc::entity_bounds(d, e, 0));
        }
        if b.is_empty() {
            return false;
        }
        let m = ((b.max.x - b.min.x).max(b.max.y - b.min.y) * 0.1).max(5.0);
        let (p1, p2) = ([b.min.x - m, b.min.y - m], [b.max.x + m, b.max.y + m]);
        self.cad.run("zoom", json!({"mode": "window", "p1": p1, "p2": p2})).is_ok()
    }

    /// The sheet frame is never selected (design 019 §3.2): a selection that held only the frame
    /// says why it is empty.
    fn drop_frame_selection(&mut self) {
        // Only when the selection changed: no work on a frame where nothing was picked.
        let sel = self.cad.session.selection();
        if sel == self.frame_checked {
            return;
        }
        let only_frame = ecad_cadcraft::frame::drop_from_selection(&mut self.cad.session);
        self.frame_checked = self.cad.session.selection();
        if only_frame {
            self.cad.session.echo(ecad_cadcraft::frame::DERIVED);
        }
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
            Answer::Cancel => {
                self.cad.session.echo("Close cancelled; the ECAD project stays open.");
                self.sync_keep_open();
            }
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
            tools::Kind::Search => {
                self.search.toggle();
                Ok(())
            }
            tools::Kind::Template => {
                let picked = self.picked();
                let (line, hint) = tools::template(id, &picked).ok_or_else(|| format!("no template for {id}"))?;
                self.command = line;
                self.command_hint = Some(hint);
                self.focus_command = true;
                self.developer.open = true;
                Ok(())
            }
            tools::Kind::Trial => self.trial_tool(id),
        }
    }

    /// The tools the trial handles itself: Preview Reference and the ECAD Dev tab.
    fn trial_tool(&mut self, id: &str) -> Result<(), String> {
        match id {
            // Brief 026: the Renumber dialog (Renumber Wires opens it with Wires chosen).
            "ecad.renumber" => {
                self.open_renumber(false, false, None);
                Ok(())
            }
            "ecad.renumberwires" => {
                self.open_renumber(false, true, None);
                Ok(())
            }
            "ecad.finalise" => {
                self.open_renumber(true, false, None);
                Ok(())
            }
            "ecad.cablepreview" => self.preview_reference(),
            "ecad.dev.debugger" => {
                self.toggle_debugger();
                Ok(())
            }
            "ecad.dev.snapshot" => {
                let r = self.export_snapshot("manual");
                self.notice = Some(match &r {
                    Ok(p) => format!("Snapshot written to {}.\n\n{SNAPSHOT_NOTE}", p.display()),
                    Err(e) => format!("Snapshot not written: {e}"),
                });
                r.map(|_| ())
            }
            "ecad.dev.command" => {
                self.developer.open = true;
                self.focus_command = true;
                Ok(())
            }
            _ => Err(format!("unknown ECAD tool {id}")),
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

    /// The symbol search window (brief 018): its rows and second step are read from the project
    /// only when the query, the pick or a device, page or setting changed (search.rs's update
    /// map); a chosen row starts Place Symbol with it.
    fn symbol_search(&mut self, ctx: &egui::Context) {
        if !self.search.open {
            return;
        }
        let (project, session, finder, serial) = (&self.project, &mut self.cad.session, &mut self.search, self.project_serial);
        let q = finder.query.clone();
        // Both are copies kept under the search's update map (search.rs): a frame reads nothing
        // from the project unless a change touched a device, a page or the settings.
        let read = project.as_ref().and_then(|p| {
            p.read(session, |p, _| {
                let at = Some((serial, p.change_count()));
                let unchanged = |since: u64| p.device_changes_since(since).is_some_and(|t| t.leaves_search());
                let rows = finder.rows_for(at, unchanged, || search::rows(Some(p.ledger.document()), &q));
                let choices = finder.choices_for(at, unchanged, |pick| search::choices(p, pick));
                (rows, choices)
            })
            .ok()
        });
        let (rows, choices) = match read {
            Some(v) => v,
            None => {
                let rows = finder.rows_for(None, |_| true, || search::rows(None, &q));
                (rows, Vec::new())
            }
        };
        if let Some(choice) = finder.show(ctx, &rows, &choices) {
            let _ = self.start_choice(&choice);
        }
    }

    /// Start Place Symbol for a choice of the search window's second step: the symbol, and the
    /// device it joins (as `D` and its tag would), so the next click on the page places it.
    ///
    /// # Errors
    /// The command's refusal.
    pub fn start_choice(&mut self, choice: &search::Choice) -> Result<(), String> {
        self.start_pick(search::Pick::Symbol(choice.symbol))?;
        let r = match &choice.device {
            Some(tag) => self
                .cad
                .session
                .input(cadcraft_engine::Input::Keyword("Device".into()))
                .and_then(|()| self.cad.session.input(cadcraft_engine::Input::Text(tag.clone())))
                .map_err(|e| e.to_string()),
            None => Ok(()),
        };
        if let Err(e) = &r {
            self.cad.session.echo(e.clone());
        }
        self.after_logic();
        r
    }

    /// Start Place Symbol for a search row: a symbol goes to the insertion point, a device asks
    /// which symbol to add to it (design 018 §5.2). Refusals are echoed at the command line.
    ///
    /// # Errors
    /// The command's refusal.
    pub fn start_pick(&mut self, pick: search::Pick) -> Result<(), String> {
        if self.cad.session.running.is_some() {
            self.cad.session.cancel();
        }
        let text = match pick {
            search::Pick::Symbol(k) => k.to_string(),
            search::Pick::Device(tag) => tag,
        };
        let r = self.cad.session.start("place").and_then(|()| self.cad.session.input(cadcraft_engine::Input::Text(text))).map_err(|e| e.to_string());
        if let Err(e) = &r {
            self.cad.session.echo(e.clone());
        }
        self.focus_drawing = true;
        self.after_logic();
        r
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
        if ctx.input(|i| i.viewport().close_requested()) {
            self.prepare_quit();
        }
        self.cad.logic(ctx);
        self.after_logic();
        if std::mem::take(&mut self.cad.quit_requested) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// The window is closing: a project with unsaved changes must be asked about, so closing its
    /// page window closes the project again (`keep_open` off), and a project whose page window
    /// was closed shows a page first, so CADCraft's quit asks about it (never lose work).
    pub fn prepare_quit(&mut self) {
        if !self.project_open() {
            return;
        }
        self.set_keep_open(false);
        let unsaved = self.status().is_some_and(|s| s.unsaved > 0);
        if unsaved && !self.page_window_open() {
            let _ = self.show_page_window();
        }
    }

    /// Close the app as the window's close button does: [`Trial::prepare_quit`], then CADCraft's
    /// quit (which asks about unsaved changes). Returns whether the close has to wait.
    pub fn request_quit(&mut self) -> bool {
        self.prepare_quit();
        let wait = self.cad.request_quit();
        self.after_logic();
        wait
    }

    /// Give the project `keep_open` (only when it differs from what it was given last).
    fn set_keep_open(&mut self, on: bool) {
        if self.keep_open == Some(on) {
            return;
        }
        if let Some(p) = &self.project
            && p.is_attached(&self.cad.session)
            && p.configure(&mut self.cad.session, |p| p.keep_open = on).is_ok()
        {
            self.keep_open = Some(on);
        }
    }

    /// `keep_open` is on except while a close of the project or a quit is being asked about.
    fn sync_keep_open(&mut self) {
        let closing = self.confirming_close
            || self.cad.closing.is_some()
            || self.held.is_some()
            || self.dialog.as_ref().is_some_and(|d| d.then_close.is_some());
        if !closing {
            self.set_keep_open(true);
        }
    }

    /// Close the project, as the Project panel's **Close** does: unsaved changes are asked about
    /// (Save, Discard, Cancel), as when the last page of a project was closed before brief 016.
    pub fn close_project(&mut self) {
        if !self.project_open() {
            return;
        }
        if !self.page_window_open() && self.show_page_window().is_err() {
            return;
        }
        let owned = |t: &Self, uid: u64| t.cad.session.ext.owner_of(uid).is_some();
        let Some(i) = self.cad.session.docs.iter().position(|d| owned(self, d.uid)) else { return };
        self.set_keep_open(false);
        self.cad.request_close(Some(i));
        self.after_logic();
    }

    /// Show or hide the debugger, inline in the Project panel's Developer section (which unfolds).
    pub fn toggle_debugger(&mut self) {
        self.debugger.open = !self.debugger.open;
        if self.debugger.open {
            self.developer.open = true;
            self.developer.body = developer::Body::Debugger;
            self.refresh_debugger(false);
        } else if self.developer.body == developer::Body::Debugger {
            self.developer.body = developer::Body::None;
        }
    }

    /// **Inspect Project** in the Developer section: a readable summary of the project (pages,
    /// devices, cables, counts, file and transaction), shown there. Read only.
    pub fn inspect_project(&mut self) {
        self.developer.open = true;
        self.developer.body = developer::Body::Inspect;
        let Some(p) = self.project.clone() else {
            self.developer.inspect = Some("No ECAD project is open.".into());
            return;
        };
        let file = self.status().map(|s| status_line(&s)).unwrap_or_default();
        let text = p.read(&mut self.cad.session, |p, _| {
            let doc = p.ledger.document();
            let mut out = vec![file, format!("Transaction {} (saved through {}).", p.ledger.head(), p.ledger.persisted())];
            out.push(String::new());
            out.push(format!("Pages ({}):", doc.page_order.len()));
            for (i, page) in doc.page_order.iter().enumerate() {
                let title = doc.pages.get(page).map(|x| x.title.as_str()).unwrap_or_default();
                let items = doc.items.values().filter(|it| it.page == *page).count();
                out.push(format!("  {:>3}  {title}  ({items} items)  {}", i + 1, page.hex()));
            }
            let devices = ecad_cadcraft::devices::devices(doc, None);
            out.push(String::new());
            out.push(format!("Devices ({}):", devices.len()));
            for c in &devices {
                out.push(format!(
                    "  {:<8} {}  ({})  {}",
                    ecad_cadcraft::devices::tag_words(doc, *c),
                    ecad_cadcraft::devices::description(doc, *c),
                    ecad_cadcraft::devices::pages_words(doc, *c),
                    c.hex()
                ));
            }
            out.push(String::new());
            out.push(format!("Cables ({}):", doc.cables.len()));
            for (k, cable) in &doc.cables {
                out.push(format!("  {:<8} {} members  {}", ecad_cadcraft::views::cable_title(doc, *k), cable.members.len(), k.hex()));
            }
            out.push(String::new());
            out.push(format!(
                "Counts: {} components, {} connection points, {} conductors, {} items, {} representations, {} definitions.",
                doc.components.len(),
                doc.terminals.len(),
                doc.conductors.len(),
                doc.items.len(),
                doc.representations.len(),
                doc.definitions.len()
            ));
            out.join("\n")
        });
        self.developer.inspect = Some(text.unwrap_or_else(|e| format!("The project could not be read: {e}")));
    }

    /// Act on what the Developer section asked for.
    fn developer_action(&mut self, a: developer::Action) {
        match a {
            developer::Action::Run(line) => {
                let n = self.cad.session.log.len();
                let failed = self.submit_command(&line);
                self.developer.reply = self.cad.session.log.iter().skip(n).filter(|l| !l.starts_with("Command: ")).take(40).cloned().collect();
                if failed.is_some() {
                    self.command = line; // kept for correction
                }
            }
            developer::Action::ToggleDebugger => self.toggle_debugger(),
            developer::Action::Inspect => self.inspect_project(),
            developer::Action::Snapshot => {
                let _ = self.trial_tool("ecad.dev.snapshot");
            }
            developer::Action::Debugger(a) => {
                let close = a == debugger::Action::Close;
                self.debugger_action(a);
                if close {
                    self.developer.body = developer::Body::None;
                }
            }
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

    /// Lay out the harness dialogs, the debugger (when open), the Project panel, CADCraft's window
    /// contents, the ECAD command field and the preview (when open).
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.sync_branch();
        self.dialogs(ui.ctx());
        self.sync_title(&ui.ctx().clone());
        if self.debugger.open && self.developer.open && self.developer.body == developer::Body::Debugger {
            // The snapshot is taken (and the project released) before anything is drawn.
            self.refresh_debugger(false);
        }
        self.project_panel(ui);
        self.sync_props();
        // Read before CADCraft's window takes the keys: Esc, or a click on the drawing area.
        let ring_input = ui.ctx().input(|i| {
            let esc = i.key_pressed(egui::Key::Escape);
            let click = i.pointer.primary_clicked()
                && self.cad.canvas.hover.is_none()
                && i.pointer.interact_pos().is_some_and(|at| self.cad.canvas.rect.is_some_and(|r| r.contains(at)));
            (esc, click)
        });
        self.cad.ui(ui);
        self.clear_goto_ring(ring_input);
        self.take_props();
        self.drop_frame_selection();
        self.handle_tools();
        self.sheet_windows(ui.ctx());
        self.output_windows(ui.ctx());
        self.merge_windows(ui.ctx());
        self.renumber_window(ui.ctx());
        self.issue_prompt_window(ui.ctx());
        self.preview_window(ui.ctx());
        self.symbol_search(ui.ctx());
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
        p.read(&mut self.cad.session, |p, _| ecad_cadcraft::interact::hover_reference(p, uid, world, radius)).ok().flatten()
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
        p.read(&mut self.cad.session, |p, _| ecad_cadcraft::interact::hover_terminal(p, uid, world, aperture)).ok().flatten()
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
                        (Some(p), Some(uid)) => p.read(&mut self.cad.session, |p, _| ecad_cadcraft::interact::hover_lines(p, uid, h)).ok().flatten(),
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

    /// Bring the Project panel's copy up to date: one change-count comparison per frame, the rows
    /// read again only when it moved (or the project changed). A busy project keeps the copy.
    pub fn refresh_panel(&mut self) {
        let Some(p) = self.project.clone().filter(|p| p.is_attached(&self.cad.session)) else {
            if self.panel.rows().is_empty() && self.panel.active.is_none() {
                return;
            }
            self.panel.clear();
            self.preview = None;
            return;
        };
        let serial = self.project_serial;
        let panel = &mut self.panel;
        let panel_revision = &mut self.panel_revision;
        let _ = p.read(&mut self.cad.session, |p, _| {
            panel.active = p.active_page();
            // An issued revision opened read-only (design 019 §7.3): its pages, under its bar.
            match &p.revision_view {
                Some(rv) => {
                    if *panel_revision != Some(rv.id) {
                        panel.set_tree(ecad_cadcraft::tree_rows(&rv.doc, |_| false));
                        panel.revision = Some(rv.bar.clone().unwrap_or_else(|| sheets_ui::revision_bar(&rv.rev, &rv.date)));
                        *panel_revision = Some(rv.id);
                    }
                    if panel.tab == project_panel::Tab::History {
                        panel.history.sync(serial, p);
                    }
                    return;
                }
                None if panel_revision.is_some() => {
                    *panel_revision = None;
                    panel.revision = None;
                    panel.set_tree(p.tree_rows());
                }
                None => {}
            }
            // Update map: the tree is re-read only when its version moved (pages added, removed,
            // renamed or moved, page properties, folder ops, a page becoming (un)available).
            panel.sync(serial, p.page_list_version(), || Some(p.tree_rows()));
            // The Devices tab's copy, only while it is shown: rebuilt once, then refreshed only for
            // the devices a change touched.
            if panel.tab == project_panel::Tab::Devices {
                panel.devices.sync(serial, p);
            }
            // The History tab's copy, only while it is shown (brief 020a; its own update map).
            if panel.tab == project_panel::Tab::History {
                panel.history.sync(serial, p);
            }
        });
        // The Checks tab's copy, only while it is shown (brief 022): the project builds its list
        // on the first show and keeps it from each change; the copy follows its revision.
        if self.panel.tab == project_panel::Tab::Checks {
            let checks = &mut self.panel.checks;
            let _ = p.configure(&mut self.cad.session, |p| checks.sync(serial, p));
        }
        self.panel.previewed = self.preview.as_ref().map(|v| v.page);
    }

    /// What the panel's header shows.
    fn header(&mut self) -> Header {
        match self.status() {
            Some(s) => {
                let mut warnings = Vec::new();
                if let Some(r) = &s.file.read_only {
                    warnings.push(format!("Read-only: {r}. Use Save As."));
                }
                if s.file.unconfirmed {
                    warnings.push("Last save not confirmed durable: Save again.".to_string());
                }
                let info = self.branch_info();
                let mut status = status_line(&s);
                let branch = info.as_ref().filter(|b| b.count > 1).map(|b| b.name.clone());
                if let Some(b) = &branch {
                    status = format!("Branch {b} · {status}");
                }
                Header {
                    snapshots: self.snapshots.is_some(),
                    name: Some(s.name.clone()),
                    status,
                    read_only: s.file.read_only.is_some(),
                    warnings,
                    note: NOTE,
                    branch,
                    branch_bar: info.as_ref().and_then(|b| b.bar.clone()),
                    parent: info.as_ref().and_then(|b| b.parent.clone()),
                    release: self.release_badge(),
                }
            }
            None => Header { note: NOTE, snapshots: self.snapshots.is_some(), ..Header::default() },
        }
    }

    /// The Project panel (or its collapsed strip) at the far left, and what it asks for.
    fn project_panel(&mut self, ui: &mut egui::Ui) {
        self.refresh_panel();
        let header = self.header();
        let t = cadcraft_ui_egui::theme::Tokens::get();
        let (actions, dev) = if self.panel.collapsed {
            egui::Panel::left("ecad_project_strip")
                .resizable(false)
                .exact_size(project_panel::STRIP_W)
                .frame(egui::Frame::NONE.fill(t.chrome))
                .show(ui, |ui| self.panel.strip(ui));
            (Vec::new(), Vec::new())
        } else {
            let running = self.cad.session.running.is_some();
            let project = header.name.is_some();
            egui::Panel::left("ecad_project")
                .resizable(true)
                .default_size(220.0)
                .min_size(160.0)
                // Wide enough for the debugger when it is shown inline.
                .max_size(720.0)
                .frame(egui::Frame::NONE.fill(t.panel).inner_margin(egui::Margin::symmetric(6, 4)))
                .show(ui, |ui| {
                    let dev = developer::show(
                        ui,
                        developer::Parts {
                            dev: &mut self.developer,
                            command: &mut self.command,
                            hint: self.command_hint,
                            focus: &mut self.focus_command,
                            debugger: &mut self.debugger,
                            running,
                            snapshots: header.snapshots,
                            project,
                        },
                    );
                    (self.panel.show(ui, &header), dev)
                })
                .inner
        };
        if self.command.trim().is_empty() {
            self.command_hint = None;
        }
        for a in actions {
            self.panel_action(a);
        }
        for a in dev {
            self.developer_action(a);
        }
    }

    /// Act on one thing the Project panel asked for. Writes go through the page commands' JSON
    /// forms on the command path, so each is exactly that command's transaction and one undo step.
    pub fn panel_action(&mut self, a: project_panel::Action) {
        use project_panel::Action as A;
        self.panel.note = None;
        match a {
            A::Open(page) => {
                let _ = self.show_page(page);
            }
            A::New => self.open_new_page_box(),
            A::Rename(page, title) => {
                let line = format!("renamepage {}", json!({"page": page.hex(), "title": title}));
                match self.run_line(&line) {
                    None => self.panel.rename = None,
                    Some(e) => {
                        if let Some(r) = &mut self.panel.rename {
                            r.text = title;
                            r.error = Some(e);
                        }
                    }
                }
            }
            A::Move(page, index) => {
                let _ = self.run_line(&format!("movepage {}", json!({"page": page.hex(), "index": index})));
            }
            A::Preview(page) => self.preview_page(page),
            A::NewProject => {
                if let Err(e) = self.new_project() {
                    self.notice = Some(e);
                }
            }
            A::OpenProject => {
                if self.project_open() {
                    self.notice = Some("Close this project first (Close, in the Project panel).".into());
                } else {
                    self.request_open();
                }
            }
            A::Save => {
                let _ = self.save();
            }
            A::SaveAs => self.request_save_as(),
            A::CloseProject => self.close_project(),
            A::RenameInBox(page) => self.open_rename_box(Some(page)),
            A::GotoSymbol(rep) => {
                let _ = self.goto_symbol(rep);
            }
            A::GotoCheck(check) => {
                let _ = self.goto_check(&check);
            }
            A::RenumberPage(page) => {
                if self.show_page(page).is_ok() {
                    self.panel.tab = project_panel::Tab::Pages;
                    self.panel.start_number_edit(page);
                }
            }
            A::MoveTable(check) => {
                if self.goto_check(&check).is_ok() {
                    if self.cad.session.running.is_some() {
                        self.cad.session.cancel();
                    }
                    if let Err(e) = self.cad.session.start("mirrormove") {
                        self.cad.session.echo(e.to_string());
                    }
                    self.after_logic();
                }
            }
            A::AddJunction(page, at) => {
                if self.show_page(page).is_ok() {
                    let _ = self.run_line(&format!("junction {}", json!({"at": [at.x, at.y]})));
                }
            }
            A::JoinWires(page, at) => {
                if self.show_page(page).is_ok() {
                    let _ = self.run_line(&format!("joinwires {}", json!({"at": [at.x, at.y]})));
                }
            }
            A::NumberThem => {
                let _ = self.run_line(&format!("renumberwires {}", json!({"scope": "unnumbered"})));
            }
            A::Finalise => self.open_renumber(true, false, None),
            A::Renumber => self.open_renumber(false, false, None),
            A::RenumberFolder(f) => self.open_renumber(false, false, Some(f)),
            A::UseBoxLocation(c) => {
                let _ = self.run_line(&format!("setprop {}", json!({"component": c.hex(), "key": "location", "value": null})));
            }
            A::ShowReports => {
                self.open_reports();
                if let Some(r) = &mut self.reports {
                    r.tab = outputs_ui::Tab::Bom;
                    r.only_flagged = true;
                    r.invalidate();
                }
            }
            // Brief 028's Checks fixes (design 028 §8).
            A::OfferDrawn(c) => {
                let value = self.project.clone().and_then(|p| {
                    p.read(&mut self.cad.session, |p, _| {
                        let doc = p.ledger.document();
                        let drawn = ecad_cadcraft::strips::drawn_terminals(doc, c);
                        let pe = ecad_cadcraft::strips::offer_of(doc, c).map_or(0, |o| o.pe);
                        let drawn_pe = drawn.iter().filter(|(_, _, pe)| *pe).count() as u32;
                        ecad_cadcraft::strips::Offer { positions: drawn.len() as u32, pe: pe.max(drawn_pe).min(drawn.len() as u32) }.text()
                    })
                    .ok()
                });
                if let Some(v) = value {
                    let _ = self.run_line(&format!("setprop {}", json!({"component": c.hex(), "key": "terminals", "value": v})));
                }
            }
            A::RenumberStrip(c) => {
                let _ = self.run_line(&format!("renumber {}", json!({"kind": "terminals", "scope": "selection", "strips": [c.hex()]})));
            }
            A::SetCores(k) => self.focus_cable_cores(k),
            A::NextFreeCore(w) => {
                let label = self.project.clone().and_then(|p| {
                    p.read(&mut self.cad.session, |p, _| {
                        let doc = p.ledger.document();
                        doc.cables_of(w).first().and_then(|k| ecad_cadcraft::strips::next_free_label(doc, *k))
                    })
                    .ok()
                    .flatten()
                });
                if let Some(l) = label {
                    let _ = self.run_line(&format!("cablecore {}", json!({"conductor": w.hex(), "core": l})));
                }
            }
            A::AccessoryFix(fix, i) => {
                let cmd: Option<String> = self.project.clone().and_then(|p| {
                    p.configure(&mut self.cad.session, |p| {
                        let o = p.outputs();
                        ecad_cadcraft::outputs::accessory_fix_commands(o, fix).into_iter().nth(i).map(|(_, c, v)| format!("{c} {v}"))
                    })
                    .ok()
                    .flatten()
                });
                if let Some(line) = cmd {
                    let _ = self.run_line(&line);
                }
            }
            A::ShowCables => {
                self.open_reports();
                if let Some(r) = &mut self.reports {
                    r.tab = outputs_ui::Tab::Cables;
                    r.cores_of = None;
                    r.only_flagged = true;
                    r.invalidate();
                }
            }
            A::Reports => self.open_reports(),
            A::PrintBook(folder) => self.open_print_book(folder),
            A::Release => self.open_release(None),
            A::NewGenerated(kind) => {
                let mut v = json!({"generated": kind.value()});
                if let Some(f) = self.panel.target_folder() {
                    v["folder"] = json!(f.hex());
                }
                let _ = self.run_line(&format!("newpage {v}"));
            }
            A::MoveInto { page, folder, index } => {
                let mut v = json!({"page": page.hex(), "folder": folder.map(|f| f.hex())});
                if let Some(i) = index {
                    v["index"] = json!(i);
                }
                let _ = self.run_line(&format!("movepage {v}"));
            }
            A::MoveFolder { folder, parent } => {
                let line = format!("movefolder {}", json!({"folder": folder.hex(), "parent": parent.map(|f| f.hex())}));
                if let Some(e) = self.run_line(&line) {
                    self.panel.note = Some(e);
                }
            }
            A::NewFolder(parent) => {
                let r = self.run_value("newfolder", json!({"name": "New folder", "parent": parent.map(|f| f.hex())}));
                match r {
                    Ok(v) => {
                        self.refresh_panel();
                        if let Some(f) = v["folder"].as_str().and_then(ecad_model::FolderId::parse) {
                            self.panel.folded.remove(&f);
                            self.panel.start_folder_rename(f);
                        }
                    }
                    Err(e) => self.panel.note = Some(e),
                }
            }
            A::RenameFolder(folder, name) => {
                let line = format!("renamefolder {}", json!({"folder": folder.hex(), "name": name}));
                match self.run_line(&line) {
                    None => self.panel.folder_rename = None,
                    Some(e) => {
                        if let Some(r) = &mut self.panel.folder_rename {
                            r.text = name;
                            r.error = Some(e);
                        }
                    }
                }
            }
            A::DeleteFolder(folder) => {
                let _ = self.run_line(&format!("deletefolder {}", json!({"folder": folder.hex()})));
            }
            A::NewPageIn(folder) => {
                self.open_new_page_box();
                self.new_page_folder = folder;
            }
            A::SetNumber(page, number) => {
                let line = format!("pageprops {}", json!({"page": page.hex(), "number": number}));
                match self.run_line(&line) {
                    None => self.panel.number_edit = None,
                    Some(e) => {
                        if let Some(r) = &mut self.panel.number_edit {
                            r.text = number;
                            r.error = Some(e);
                        }
                    }
                }
            }
            A::ProjectInfo => self.open_project_popout(sheets_ui::Popout::Info),
            A::ProjectSettings => self.open_project_popout(sheets_ui::Popout::Settings),
            A::Revisions => self.open_revisions(),
            A::BackToCurrent => {
                let _ = self.run_line("closerevision {}");
                self.preview = None;
            }
            A::RevisionPage(page) => self.preview_revision_page(page),
            A::History(d) => self.history_action(d),
            A::SetDeviceProperty { component, key, value, conflict } => {
                let value = if key == "tag_fixed" && value.is_empty() { serde_json::Value::Null } else { json!(value) };
                let line = format!("setprop {}", json!({"component": component.hex(), "key": key, "value": value, "conflict": conflict}));
                let value = value.as_str().unwrap_or_default().to_string();
                if let Some(e) = self.run_line(&line) {
                    self.panel.devices.refused(component, key, &value, e);
                } else {
                    self.panel.devices.error = None;
                }
            }
        }
        self.refresh_panel();
    }

    /// The branch facts (design 020 §3.6), read again only after a switch or a branch action.
    pub fn branch_info(&mut self) -> Option<BranchInfo> {
        let key = (self.project_serial, self.branch_rev);
        if let Some((k, v)) = &self.branch_info
            && *k == key
        {
            return v.clone();
        }
        let p = self.project.clone()?;
        let info = p
            .read(&mut self.cad.session, |p, _| {
                use ecad_cadcraft::history;
                let file = p.path.as_deref().and_then(|x| std::path::Path::new(x).file_name()).map(|f| f.to_string_lossy().into_owned());
                let Some(v) = p.file.v2() else {
                    return BranchInfo { name: "main".into(), count: 1, bar: None, parent: None, release_line: true, file };
                };
                let rec = v.branch_record().clone();
                let rows = history::branch_rows(p).unwrap_or_default();
                let parent = rec.parent.as_deref().and_then(ecad_file::v2::schema::BranchId::parse);
                let parent_name = parent.and_then(|id| rows.iter().find(|(i, _)| *i == id)).map(|(_, b)| b.name.clone());
                let bar = parent.zip(rec.fork_point.as_ref()).map(|(pid, f)| {
                    let named = history::named_points(p, pid).unwrap_or_default();
                    let from = named
                        .iter()
                        .find(|n| n.branch == pid && n.seq == f.seq)
                        .map_or_else(|| format!("{}, step {}", parent_name.clone().unwrap_or_default(), f.seq), |n| n.name.clone());
                    format!("Branch \"{}\" · from {from}", rec.name)
                });
                BranchInfo { name: rec.name.clone(), count: rows.len(), bar, parent: parent_name, release_line: rec.release_line, file }
            })
            .ok();
        self.branch_info = Some((key, info.clone()));
        info
    }

    /// The one refresh path for branch actions, clicked or typed (reviewer finding 1): when the
    /// project's branch-action count or the branch shown moved, the title, branch bar, Revisions
    /// and History are read again; after a switch, every copy rebuilds once as on open and a
    /// preview closes. One integer and one ID compared per frame otherwise.
    pub fn sync_branch(&mut self) {
        let Some(p) = self.project.clone() else { return };
        let Ok(now) = p.read(&mut self.cad.session, |p, _| (p.branch_gen(), p.file.v2().map(|v| v.branch().hex()))) else { return };
        let Some(seen) = self.branch_seen.replace(now.clone()) else { return };
        if seen == now {
            return;
        }
        self.branch_rev += 1;
        self.panel.history.invalidate();
        if seen.1 != now.1 {
            self.project_serial += 1;
            self.preview = None;
            self.panel.devices.invalidate();
            self.panel.checks.invalidate();
            self.panel.history.selected = None;
        }
        if self.revisions.is_some() {
            self.open_revisions();
        }
    }

    /// Show branch `branch` (named `name`) as the Branch list does: straight away with nothing
    /// unsaved, else through Save and switch (or Save As on a read-only file).
    pub fn ask_switch(&mut self, branch: &str, name: &str) {
        let (unsaved, read_only) = match self.status() {
            Some(s) => (s.unsaved, s.file.read_only.is_some()),
            None => return,
        };
        if unsaved == 0 {
            self.history_action(history_panel::Do::Run("switchbranch", json!({"branch": branch})));
            return;
        }
        self.panel.tab = project_panel::Tab::History;
        self.panel.history.dialog = Some(if read_only {
            history_panel::Dialog::SwitchReadOnly
        } else {
            history_panel::Dialog::Switch { branch: branch.to_string(), name: name.to_string() }
        });
    }

    /// Keep the window title in step with the branch shown (owner answer 15); sent only when it
    /// changes.
    fn sync_title(&mut self, ctx: &egui::Context) {
        let info = if self.project_open() { self.branch_info() } else { None };
        let title = window_title(info.as_ref());
        if title != self.title_sent {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title_sent = title;
        }
    }

    /// Act on what the History tab asked for (brief 020a): each write is its command's JSON form.
    pub fn history_action(&mut self, d: history_panel::Do) {
        use history_panel::Do;
        match d {
            Do::Run(cmd, params) => {
                match self.run_click(cmd, params) {
                    Ok(_) => {
                        self.panel.history.done();
                        // The same refresh as for a typed command (reviewer finding 1).
                        self.sync_branch();
                    }
                    Err(e) => self.panel.history.refused(e),
                }
            }
            Do::AskRestore(params) => {
                let mut ask = params.clone();
                ask["preview"] = json!(true);
                match self.run_click("restore", ask) {
                    Ok(v) => {
                        let question = v["question"].as_str().unwrap_or_default().to_string();
                        self.panel.history.dialog = Some(history_panel::Dialog::Restore { params, question });
                    }
                    Err(e) => self.panel.history.refused(e),
                }
            }
            Do::SaveAs => self.request_save_as(),
            Do::ShowPoint(params) => match self.run_click("showpoint", params) {
                Ok(_) => self.preview = None,
                Err(e) => self.panel.history.refused(e),
            },
            Do::AskMerge(target) => self.ask_merge(target),
        }
    }

    /// Open the merge dialog for `target` (`{from, into?}`) with its preview (design 020 §4.1):
    /// nothing is written until Merge; a merge into another branch than the one shown says it
    /// switches there first.
    pub fn ask_merge(&mut self, target: serde_json::Value) {
        let mut ask = target.clone();
        ask["preview"] = json!(true);
        match self.run_click("merge", ask) {
            Ok(v) => {
                let shown = self.branch_info().map(|b| b.name).unwrap_or_else(|| "main".into());
                let into = v["into"].as_str().unwrap_or("main").to_string();
                let note = (v["update"] != json!(true) && into != shown)
                    .then(|| format!("This switches to {into} (saving \"{shown}\" first) and merges into it."));
                self.merge = Some(merge_dialog::MergeDialog::new(target, v, note));
            }
            Err(e) => self.panel.history.refused(e),
        }
    }

    /// Open the Renumber dialog (brief 026, design §3), or Finalise numbering (`finalise`), with
    /// Wires chosen (`wires`) or a folder preset; its preview is computed now (reads only).
    pub fn open_renumber(&mut self, finalise: bool, wires: bool, folder: Option<ecad_model::FolderId>) {
        let Some(p) = self.project.clone() else { return };
        let read_only = self.header().read_only;
        let (uid, handles) = match self.cad.session.state() {
            Ok(st) => (Some(st.uid), st.selection.clone()),
            Err(_) => (None, Vec::new()),
        };
        let read = p.read(&mut self.cad.session, |p, _| {
            let doc = p.ledger.document();
            let objects = uid.and_then(|u| p.page_of(u)).map(|pg| ecad_cadcraft::props::selected_objects(p, pg, &handles)).unwrap_or_default();
            (ecad_cadcraft::numbering_cmds::choices(doc), objects, ecad_cadcraft::devices::padding(doc), doc.settings.wiring.digits)
        });
        let Ok((choices, objects, digits, wire_digits)) = read else { return };
        let mut d = renumber_dialog::RenumberDialog::new(&choices, objects, digits, read_only, finalise);
        if wires {
            d = d.for_wires(wire_digits);
        }
        if let Some(f) = folder {
            d = d.for_folder(&f.hex());
        }
        if let Ok((c, v)) = d.params(true) {
            match self.run_click(c, v) {
                Ok(v) => d.preview = v,
                Err(e) => d.error = Some(e),
            }
        }
        self.renumber = Some(d);
    }

    /// The provisional question before an issue (brief 026, design §5.5), for the refused `line`.
    fn ask_issue_provisional(&mut self, line: String) {
        let Some(p) = self.project.clone() else { return };
        let items = p.read(&mut self.cad.session, |p, _| ecad_cadcraft::numbering_cmds::provisional_items(p.ledger.document()));
        if let Ok(items) = items {
            self.issue_prompt = Some((line, format!("{}:\n{}", items.words(), items.list_line())));
        }
    }

    /// The provisional question's window: Finalise numbering now, Issue anyway or Cancel.
    fn issue_prompt_window(&mut self, ctx: &egui::Context) {
        let Some((line, text)) = self.issue_prompt.clone() else { return };
        let rev = self.revisions.as_ref().and_then(|d| d.pending.first().cloned()).unwrap_or_default();
        let mut chosen = None;
        egui::Window::new(format!("Issue Rev {rev}"))
            .id(egui::Id::unique("ecad.issue"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(egui::RichText::new(&text).small().monospace());
                ui.horizontal(|ui| {
                    if ui.button("Finalise numbering now").accessible_name("ecad.issue.finalise").clicked() {
                        chosen = Some(1);
                    }
                    if ui.button("Issue anyway").accessible_name("ecad.issue.anyway").clicked() {
                        chosen = Some(2);
                    }
                    if ui.button("Cancel").accessible_name("ecad.issue.cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        chosen = Some(0);
                    }
                });
            });
        match chosen {
            Some(1) => {
                self.issue_prompt = None;
                self.issue_after_finalise = Some(line);
                self.open_renumber(true, false, None);
            }
            Some(2) => {
                self.issue_prompt = None;
                let line = with_param(&line, "provisional", json!("issue"));
                let r = self.run_line(&line);
                if let Some(d) = &mut self.revisions {
                    d.error = r.clone();
                }
                if r.is_none() {
                    self.open_revisions();
                }
            }
            Some(_) => self.issue_prompt = None,
            None => {}
        }
    }

    /// The Renumber dialog per frame: previews on option changes, Apply, go to a row (brief 026).
    fn renumber_window(&mut self, ctx: &egui::Context) {
        let Some(d) = &mut self.renumber else { return };
        match d.show(ctx) {
            Some(renumber_dialog::Out::Preview(c, v)) => {
                let r = self.run_click(c, v);
                if let Some(d) = &mut self.renumber {
                    match r {
                        Ok(v) => {
                            d.preview = v;
                            d.error = None;
                        }
                        Err(e) => d.error = Some(e),
                    }
                }
            }
            Some(renumber_dialog::Out::Apply(c, v)) => match self.run_click(c, v) {
                Ok(_) => {
                    self.renumber = None;
                    // Finalise numbering now (design §5.5): the issue follows, its own step.
                    if let Some(line) = self.issue_after_finalise.take() {
                        let _ = self.run_line(&line);
                        self.open_revisions();
                    }
                }
                Err(e) => {
                    // A stale preview is computed again (design §3).
                    let again = self.renumber.as_ref().and_then(|d| d.params(true).ok());
                    let fresh = again.map(|(c, v)| self.run_click(c, v));
                    if let Some(d) = &mut self.renumber {
                        d.error = Some(e);
                        if let Some(Ok(v)) = fresh {
                            d.preview = v;
                        }
                    }
                }
            },
            Some(renumber_dialog::Out::Goto(page, at)) => {
                if let Some(page) = u128::from_str_radix(&page, 16).ok().map(PageId)
                    && self.show_page(page).is_ok()
                    && let Ok(st) = self.cad.session.state_mut()
                {
                    let height = st.view().height.clamp(GOTO_MIN_HEIGHT, GOTO_MAX_HEIGHT);
                    let _ = st.set_view(cadcraft_engine::View { center: cadcraft_engine::geom::Vec2::new(at[0], at[1]), height, twist: 0.0 });
                }
            }
            Some(renumber_dialog::Out::Cancel) => {
                self.renumber = None;
                self.issue_after_finalise = None;
            }
            None => {}
        }
    }

    /// The merge dialog and the question an undo of a merge stops at (brief 020b).
    fn merge_windows(&mut self, ctx: &egui::Context) {
        if let Some(d) = &mut self.merge {
            match d.show(ctx) {
                Some(merge_dialog::Out::Preview(params)) => match self.run_click("merge", params) {
                    Ok(v) => {
                        if let Some(d) = &mut self.merge {
                            d.preview = v;
                            d.error = None;
                        }
                    }
                    Err(e) => {
                        if let Some(d) = &mut self.merge {
                            d.error = Some(e);
                        }
                    }
                },
                Some(merge_dialog::Out::Merge(params)) => match self.run_click("merge", params) {
                    Ok(_) => {
                        self.merge = None;
                        self.panel.history.done();
                        self.sync_branch();
                    }
                    Err(e) => {
                        if let Some(d) = &mut self.merge {
                            d.error = Some(e);
                        }
                        self.sync_branch();
                    }
                },
                Some(merge_dialog::Out::Cancel) => {
                    // The prepared states are dropped with the dialog.
                    self.merge = None;
                    if let Some(p) = self.project.clone() {
                        let _ = p.configure(&mut self.cad.session, |p| p.merge_cache = None);
                    }
                }
                None => {}
            }
        }
        let question = self.project.clone().and_then(|p| p.read(&mut self.cad.session, |p, _| p.merge_undo_question.clone()).ok().flatten());
        if let Some(q) = question {
            match merge_dialog::undo_merge(ctx, &q) {
                Some(true) => {
                    let _ = self.run_line("undomerge {}");
                    self.panel.history.invalidate();
                }
                Some(false) => {
                    if let Some(p) = self.project.clone() {
                        let _ = p.configure(&mut self.cad.session, |p| p.merge_undo_question = None);
                    }
                }
                None => {}
            }
        }
    }

    /// Make `page` the active page, as a click on its row does: a command waiting at a prompt is
    /// cancelled first (nothing written), then the page is shown in the one page window. A
    /// preview of that page closes.
    ///
    /// # Errors
    /// No project, or the page cannot be shown (echoed at the command line).
    pub fn show_page(&mut self, page: PageId) -> Result<(), String> {
        let p = self.project.clone().ok_or("no ECAD project is open")?;
        if self.cad.session.running.is_some() {
            self.cad.session.cancel();
        }
        let r = p.open_page(&mut self.cad.session, page).map(|_| ());
        if let Err(e) = &r {
            self.cad.session.echo(e.clone());
        }
        self.panel.reveal(page);
        self.after_logic();
        r
    }

    /// The Devices tab's go to: `symbol`'s page becomes the active page (the path of `openpage` and
    /// Go to Reference), the view is centred on it at the current zoom and it is selected.
    ///
    /// # Errors
    /// No project, an unknown symbol, or a page that cannot be shown (echoed).
    pub fn goto_symbol(&mut self, symbol: ecad_model::RepresentationId) -> Result<(), String> {
        let p = self.project.clone().ok_or("no ECAD project is open")?;
        let page = p
            .read(&mut self.cad.session, |p, _| ecad_cadcraft::device_list::symbol_target(p, symbol).map(|t| t.0))?
            .ok_or("that symbol no longer exists")?;
        self.show_page(page)?;
        let (_, at, handles) =
            p.read(&mut self.cad.session, |p, _| ecad_cadcraft::device_list::symbol_target(p, symbol))?.ok_or("that symbol no longer exists")?;
        if let (Some(at), Ok(st)) = (at, self.cad.session.state_mut()) {
            // The current zoom, as Go to Reference keeps it, within a readable range (a page shown
            // only while empty may have a tiny or a huge view).
            let height = st.view().height.clamp(GOTO_MIN_HEIGHT, GOTO_MAX_HEIGHT);
            let _ = st.set_view(cadcraft_engine::View { center: cadcraft_engine::geom::Vec2::new(at.x, at.y), height, twist: 0.0 });
        }
        self.cad.session.set_selection(handles);
        Ok(())
    }

    /// The Checks tab's go to (design 022 §8): the row's page becomes the active page, the view is
    /// centred on its point at the current zoom (kept between 60 and 600 mm high), its object is
    /// selected, and a ring in the row's severity colour marks the point until the next change, Esc
    /// or a click on empty space. Nothing is recorded.
    ///
    /// # Errors
    /// No project, a row with no page, or a page that cannot be shown (echoed).
    pub fn goto_check(&mut self, check: &ecad_cadcraft::checks::Check) -> Result<(), String> {
        let p = self.project.clone().ok_or("no ECAD project is open")?;
        let page = check.page.ok_or("that row is not on a page")?;
        self.show_page(page)?;
        let handles = p.configure(&mut self.cad.session, |p| {
            p.goto_ring = check.at.map(|at| (page, at, check.severity()));
            ecad_cadcraft::checks::handles_of(p, page, check.object)
        })?;
        if let (Some(at), Ok(st)) = (check.at, self.cad.session.state_mut()) {
            let height = st.view().height.clamp(GOTO_MIN_HEIGHT, GOTO_MAX_HEIGHT);
            let _ = st.set_view(cadcraft_engine::View { center: cadcraft_engine::geom::Vec2::new(at.x, at.y), height, twist: 0.0 });
        }
        self.cad.session.set_selection(handles);
        Ok(())
    }

    /// The go-to ring clears on Esc and on a click on empty space, i.e. a click on the drawing
    /// area with no object under the pointer (owner, 2026-11-09); the next change or go-to clears
    /// it too. The project is touched only on such a frame.
    fn clear_goto_ring(&mut self, (esc, click): (bool, bool)) {
        if !(esc || click) {
            return;
        }
        if let Some(p) = self.project.clone()
            && p.is_attached(&self.cad.session)
        {
            let _ = p.configure(&mut self.cad.session, |p| p.goto_ring = None);
        }
    }

    /// Open the page name box for a new page, prefilled with its number (the panel's **+** and
    /// Tool Sets' New Page).
    pub fn open_new_page_box(&mut self) {
        self.new_page_folder = None;
        let n = self.project_page_count() + 1;
        self.name_box = Some(NameBox { kind: NameBoxKind::New, text: n.to_string(), error: None, focused: false });
    }

    /// Open the page name box to rename `page` (the active page when `None`), prefilled with its
    /// title (Tool Sets' Rename Page). Nothing opens when there is no such page.
    pub fn open_rename_box(&mut self, page: Option<PageId>) {
        let Some(p) = self.project.clone() else { return };
        let found = p.read(&mut self.cad.session, |p, _| {
            let page = page.or_else(|| p.active_page())?;
            Some((page, p.ledger.document().pages.get(&page)?.title.clone()))
        });
        match found {
            Ok(Some((page, title))) => self.name_box = Some(NameBox { kind: NameBoxKind::Rename(page), text: title, error: None, focused: false }),
            _ => self.cad.session.echo("Rename Page: show the page to rename first."),
        }
    }

    fn project_page_count(&mut self) -> usize {
        let Some(p) = self.project.clone() else { return 0 };
        p.read(&mut self.cad.session, |p, _| p.ledger.document().page_order.len()).unwrap_or(0)
    }

    /// OK in the page name box: one `newpage` (the new page is shown at once) or `renamepage`. A
    /// refusal keeps the box open with the message.
    pub fn confirm_name_box(&mut self) {
        let Some(b) = self.name_box.clone() else { return };
        if self.cad.session.running.is_some() {
            self.cad.session.cancel();
        }
        let line = match b.kind {
            NameBoxKind::New => match self.new_page_folder {
                Some(f) => format!("newpage {}", json!({ "title": b.text, "folder": f.hex() })),
                None => format!("newpage {}", json!({ "title": b.text })),
            },
            NameBoxKind::Rename(page) => format!("renamepage {}", json!({"page": page.hex(), "title": b.text})),
        };
        match self.run_line(&line) {
            None => {
                self.name_box = None;
                self.refresh_panel();
                if let Some(page) = self.panel.active {
                    self.panel.reveal(page);
                }
            }
            Some(e) => {
                if let Some(nb) = &mut self.name_box {
                    nb.error = Some(e);
                    nb.focused = false;
                }
            }
        }
    }

    // ---------------------------------------------------------------- brief 019

    /// Run a command's JSON form as the command path does, returning its result.
    /// A History or merge click's command (reviewer 020b, finding 2): the command echoes its own
    /// line, so only a refusal is echoed here (`CadApp::run` would echo the reply's `message` a
    /// second time).
    fn run_click(&mut self, cmd: &str, params: serde_json::Value) -> Result<serde_json::Value, String> {
        let r = self.cad.session.execute(cmd, &params).map_err(|e| e.to_string());
        if let Err(e) = &r {
            self.cad.session.echo(e.clone());
        }
        self.after_logic();
        r
    }

    fn run_value(&mut self, cmd: &str, params: serde_json::Value) -> Result<serde_json::Value, String> {
        let r = self.cad.run(cmd, params).map_err(|e| e.to_string());
        self.after_logic();
        r
    }

    /// Open the Project info or Project settings popout, filled from `projectprops {}` (reads only).
    pub fn open_project_popout(&mut self, popout: sheets_ui::Popout) {
        let read_only = self.header().read_only;
        match self.run_value("projectprops", json!({})) {
            Ok(v) => {
                let mut d = sheets_ui::ProjectProps::from_json(&v, read_only, popout);
                // Brief 031: the Text section reads `textstyle {}` (reads only).
                if popout == sheets_ui::Popout::Settings {
                    d.text = self.run_value("textstyle", json!({})).ok().map(|t| text_ui::TextSection::from_json(&t));
                    // Brief 032 §4.4: the wire label rows read `wiresettings {}` (reads only).
                    d.labels = self.run_value("wiresettings", json!({})).ok().map(|w| sheets_ui::WireLabels::from_json(&w));
                }
                self.project_props = Some(d);
            }
            Err(e) => self.cad.session.echo(e),
        }
    }

    /// Open the Revisions dialog, filled from `projectprops {}` (reads only).
    pub fn open_revisions(&mut self) {
        let read_only = self.header().read_only;
        let off_main = self.branch_info().filter(|b| !b.release_line).map(|b| b.name);
        match self.run_value("projectprops", json!({})) {
            Ok(v) => {
                let mut r = sheets_ui::Revisions::from_json(&v, read_only || off_main.is_some());
                r.off_main = off_main;
                self.fill_folders(&mut r);
                self.revisions = Some(r);
                self.revisions_seen = self.change_count();
            }
            Err(e) => self.cad.session.echo(e),
        }
    }

    /// The project's change count (0 without a project).
    fn change_count(&mut self) -> u64 {
        let Some(p) = self.project.clone() else { return 0 };
        p.read(&mut self.cad.session, |p, _| p.change_count()).unwrap_or(0)
    }

    /// Bring the open Revisions dialog up to date after a change of its update map (the project's
    /// properties, issuing, undo and redo of them), keeping unsent typing. One integer compared
    /// per frame otherwise.
    fn refresh_revisions(&mut self) {
        if self.revisions.is_none() {
            return;
        }
        let Some(p) = self.project.clone() else { return };
        let seen = self.revisions_seen;
        let Ok((count, touched)) = p.read(&mut self.cad.session, |p, _| {
            let count = p.change_count();
            (count, if count == seen { Some(Default::default()) } else { p.props_changes_since(seen) })
        }) else {
            return;
        };
        self.revisions_seen = count;
        let hit = touched.is_none_or(|t: std::collections::BTreeSet<ecad_model::ObjectRef>| t.contains(&ecad_model::ObjectRef::Project));
        if !hit {
            return;
        }
        let read_only = self.header().read_only;
        if let Ok(v) = self.run_value("projectprops", json!({})) {
            let mut fresh = sheets_ui::Revisions::from_json(&v, read_only);
            self.fill_folders(&mut fresh);
            if let Some(d) = &mut self.revisions {
                d.refresh(fresh);
            }
        }
    }

    /// Preview `page` of the revision opened read-only (design 019 §7.3).
    pub fn preview_revision_page(&mut self, page: PageId) {
        let Some(p) = self.project.clone() else { return };
        let rv = p.read(&mut self.cad.session, |p, _| p.revision_view.clone()).ok().flatten();
        let Some(rv) = rv else { return };
        let mut v = Preview::new(page, None, None);
        v.revision = Some((rv.doc.clone(), format!("Rev {}", rv.rev)));
        if let Some(r) = self.cad.canvas.rect {
            v.place(r);
        }
        self.preview = Some(v);
    }

    /// Bring the Properties section's copy up to date (design 019 §9): rebuilt on a new selection
    /// or page, or after a change that touched what it shows; otherwise kept.
    pub fn sync_props(&mut self) {
        let Some(p) = self.project.clone().filter(|p| p.is_attached(&self.cad.session)) else {
            if let Ok(mut sh) = self.props.lock() {
                sh.view = ecad_cadcraft::props::PropsView::default();
            }
            self.props_seen = None;
            return;
        };
        let owned = self.cad.session.state().is_ok_and(|st| self.cad.session.ext.owner_of(st.uid).is_some());
        let selection = if owned { self.cad.session.selection() } else { Vec::new() };
        let serial = self.project_serial;
        let seen = &self.props_seen;
        let read = p.read(&mut self.cad.session, |p, _| {
            let page = p.active_page();
            let same = seen.as_ref().is_some_and(|(s, _, pg, sel)| *s == serial && *pg == page && *sel == selection);
            // Nothing changed since the copy was built: nothing to look at (the usual frame).
            let since = seen.as_ref().map(|(_, n, _, _)| *n);
            let touched = match since {
                Some(n) if n == p.change_count() => Some(Default::default()),
                Some(n) => p.props_changes_since(n),
                None => None,
            };
            (page, p.change_count(), same, touched)
        });
        let Ok((page, count, same, touched)) = read else { return };
        let stale = !same
            || touched.is_none_or(|t: std::collections::BTreeSet<ecad_model::ObjectRef>| {
                self.props.lock().map_or(true, |sh| t.iter().any(|o| sh.view.deps.contains(o)))
            });
        if !stale {
            if let Some(seen) = &mut self.props_seen {
                seen.1 = count;
            }
            return;
        }
        let view = p.read(&mut self.cad.session, |p, _| ecad_cadcraft::props::view(p, page, &selection));
        if let Ok(view) = view {
            let writable = !self.header().read_only;
            if let Ok(mut sh) = self.props.lock() {
                // A refusal stays under its field until what the section shows changes.
                if sh.view != view {
                    sh.errors.clear();
                }
                sh.view = view;
                sh.writable = writable;
            }
            self.props_rebuilds += 1;
            self.props_seen = Some((serial, count, page, selection));
        }
    }

    /// Run what the Properties section asked for: each edit through its command's JSON form, a
    /// refusal kept under its field.
    fn take_props(&mut self) {
        let (edits, buttons) = match self.props.lock() {
            Ok(mut sh) => (std::mem::take(&mut sh.edits), std::mem::take(&mut sh.buttons)),
            Err(_) => return,
        };
        // A command waiting at a prompt is cancelled first (nothing written), as a page click does.
        if (!edits.is_empty() || !buttons.is_empty()) && self.cad.session.running.is_some() {
            self.cad.session.cancel();
        }
        for (target, key, value) in edits {
            let line = ecad_cadcraft::props::edit_line(&target, &key, value.as_deref());
            let r = self.run_line(&line);
            // A clash box's choice (`tag#swap`, brief 026) answers the Tag field's refusal.
            let key = key.split('#').next().unwrap_or_default().to_string();
            if let Ok(mut sh) = self.props.lock() {
                match r {
                    Some(e) => {
                        sh.errors.insert(key, e);
                    }
                    None => {
                        sh.errors.remove(&key);
                    }
                }
            }
        }
        for b in buttons {
            match b.as_str() {
                "revisions" => self.open_revisions(),
                "more" => self.open_project_popout(sheets_ui::Popout::Info),
                g => {
                    if let Some(r) = g.strip_prefix("goto:") {
                        let _ = self.run_line(&format!("cablegoto {}", json!({ "reference": r })));
                    } else if let Some(c) = g.strip_prefix("showplan:") {
                        // Brief 028: Reports ▸ Terminals on this strip.
                        self.show_plan(ecad_model::ComponentId::parse(c));
                    } else if let Some(k) = g.strip_prefix("showcores:").and_then(ecad_model::CableId::parse) {
                        self.show_cores(k);
                    } else if let Some(rest) = g.strip_prefix("refgoto:") {
                        // Brief 025: a device reference's Go to.
                        let mut it = rest.splitn(3, ':');
                        let (source, target) = (it.next().unwrap_or_default(), it.next().unwrap_or_default());
                        let mut params = json!({ "source": source });
                        if !target.is_empty() {
                            params["target"] = json!(target);
                        }
                        let _ = self.run_line(&format!("refgoto {params}"));
                    }
                }
            }
        }
    }

    // ---------------------------------------------------------------- brief 024

    /// The project file's path without `.ecad`, or its name, for default output names.
    fn output_stem(&mut self) -> String {
        let Some(p) = self.project.clone() else { return "project".into() };
        p.read(&mut self.cad.session, |p, _| match &p.path {
            Some(path) => std::path::Path::new(path).with_extension("").to_string_lossy().into_owned(),
            None => p.name.clone(),
        })
        .unwrap_or_else(|_| "project".into())
    }

    /// A typed output path with `~` expanded.
    fn output_path(&self, path: &str) -> Result<String, String> {
        ecad_cadcraft::expand_home(path.trim(), self.home.as_deref()).map(|p| p.to_string_lossy().into_owned())
    }

    /// Open the Reports window (design 024 §6.2).
    pub fn open_reports(&mut self) {
        if self.reports.is_none() {
            let stem = self.output_stem();
            let mut r = outputs_ui::Reports::default();
            r.stem = stem;
            self.reports = Some(r);
        }
    }

    /// Open the Print Book dialog (design 024 §4.1), scoped to `folder` when given.
    pub fn open_print_book(&mut self, folder: Option<ecad_model::FolderId>) {
        let Some(p) = self.project.clone() else { return };
        let folders = p
            .read(&mut self.cad.session, |p, _| {
                let doc = p.ledger.document();
                doc.folders.keys().map(|f| (f.hex(), ecad_cadcraft::sheet_cmds::folder_path(doc, Some(*f)))).collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let chosen = folder.and_then(|f| folders.iter().position(|(h, _)| *h == f.hex()));
        let path = format!("{}.pdf", self.output_stem());
        self.print_book = Some(outputs_ui::PrintBook {
            scope: if chosen.is_some() { outputs_ui::ScopeKind::Folder } else { outputs_ui::ScopeKind::All },
            folders,
            folder: chosen.unwrap_or(0),
            path,
            mono: true,
            lineweights: true,
            ..outputs_ui::PrintBook::default()
        });
    }

    /// The Reports window and the Print Book dialog.
    fn output_windows(&mut self, ctx: &egui::Context) {
        let serial = self.project_serial;
        if let Some(p) = self.project.clone()
            && p.is_attached(&self.cad.session)
            && p.configure(&mut self.cad.session, |p| std::mem::take(&mut p.open_reports)).unwrap_or(false)
        {
            self.open_reports();
        }
        if let (Some(r), Some(p)) = (&mut self.reports, self.project.clone()) {
            let _ = p.configure(&mut self.cad.session, |p| r.sync(serial, p));
        }
        let out = self.reports.as_mut().and_then(|r| r.show(ctx));
        match out {
            Some(outputs_ui::Out::Close) => self.reports = None,
            Some(outputs_ui::Out::Export(path)) => self.export_report(&path),
            Some(outputs_ui::Out::Run(line)) => {
                let _ = self.run_line(&line);
            }
            // Brief 028: double-clicking a cable shows its cores.
            Some(outputs_ui::Out::Select(outputs_ui::Tab::Cables, i)) if self.reports.as_ref().is_some_and(|r| r.cores_of.is_none()) => {
                self.show_cores_of_row(i)
            }
            Some(outputs_ui::Out::Goto(tab, i) | outputs_ui::Out::Select(tab, i)) => self.goto_report_row(tab, i),
            None => {}
        }
        self.print_window(ctx);
        self.release_window(ctx);
    }

    /// Write the Reports tab as shown to `path` as CSV (design 024 §6.3).
    pub fn export_report(&mut self, path: &str) {
        let path = self.output_path(path);
        let Some(r) = &mut self.reports else { return };
        let t = r.shown_table();
        let flagged = t.flagged();
        let text = ecad_cadcraft::outputs::csv(&t, true);
        match path.and_then(|path| ecad_cadcraft::printbook::write_file(&path, text.as_bytes()).map(|()| path)) {
            Ok(path) => {
                let marked = if flagged == 1 { "1 cell marked ?".to_string() } else { format!("{flagged} cells marked ?") };
                let m = format!("Wrote {} rows to {path} ({marked}).", t.rows.len());
                r.note = Some(m.clone());
                r.export = None;
                self.cad.session.echo(m);
            }
            Err(e) => r.note = Some(e),
        }
    }

    /// Brief 028: show the core list of the cable on shown row `i` of the Cables tab.
    fn show_cores_of_row(&mut self, i: usize) {
        let Some(k) = self.reports.as_ref().and_then(|r| r.row_index(i)) else { return };
        let Some(p) = self.project.clone() else { return };
        let cable = p.configure(&mut self.cad.session, |p| p.outputs().cables().get(k).map(|c| c.cable)).ok().flatten();
        if let (Some(r), Some(c)) = (&mut self.reports, cable) {
            r.cores_of = Some(c);
        }
    }

    /// Brief 028: Reports on the Terminals tab for `strip` (Properties' **Show plan**).
    pub fn show_plan(&mut self, strip: Option<ecad_model::ComponentId>) {
        self.open_reports();
        if let Some(r) = &mut self.reports {
            r.tab = outputs_ui::Tab::Terminals;
            r.strip = strip;
            r.cores_of = None;
            r.invalidate();
        }
    }

    /// Brief 028: Checks' **Set cores…**: the cable's slash is shown and selected, so Properties
    /// shows its Cores field.
    pub fn focus_cable_cores(&mut self, cable: ecad_model::CableId) {
        let Some(p) = self.project.clone() else { return };
        let found = p
            .read(&mut self.cad.session, |p, _| {
                let doc = p.ledger.document();
                let (_, r) = doc.representations_of(ecad_model::Subject::Cable(cable)).find(|(_, r)| r.role == "slash")?;
                let at = r.items.first().and_then(|i| doc.items.get(i)).and_then(|i| match &i.kind {
                    ecad_model::ItemKind::Primitive(ecad_model::Primitive::Line { a, b }) => {
                        Some(ecad_model::Pt::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0))
                    }
                    _ => None,
                });
                Some((r.page, r.items.clone(), at))
            })
            .ok()
            .flatten();
        let Some((page, items, at)) = found else {
            self.notice = Some("That cable has no slash drawn; set its cores in the Reports' Cables tab or with setprop.".into());
            return;
        };
        if self.show_page(page).is_err() {
            return;
        }
        let handles = p
            .read(&mut self.cad.session, |p, _| {
                p.pages.get(&page).map(|v| items.iter().filter_map(|i| v.map.item(*i)).collect::<Vec<_>>()).unwrap_or_default()
            })
            .unwrap_or_default();
        if let (Some(at), Ok(st)) = (at, self.cad.session.state_mut()) {
            let height = st.view().height.clamp(GOTO_MIN_HEIGHT, GOTO_MAX_HEIGHT);
            let _ = st.set_view(cadcraft_engine::View { center: cadcraft_engine::geom::Vec2::new(at.x, at.y), height, twist: 0.0 });
        }
        self.cad.session.set_selection(handles);
    }

    /// Brief 028: Reports on the Cables tab with `cable`'s cores (Properties' **Show cores**).
    pub fn show_cores(&mut self, cable: ecad_model::CableId) {
        self.open_reports();
        if let Some(r) = &mut self.reports {
            r.tab = outputs_ui::Tab::Cables;
            r.cores_of = Some(cable);
            r.invalidate();
        }
    }

    /// Go to a Reports row: a BOM row's first device's main symbol, a wire's first route, a
    /// cable's label box (design 024 §6.2), a terminal (brief 028). Nothing is recorded.
    pub fn goto_report_row(&mut self, tab: outputs_ui::Tab, i: usize) {
        enum To {
            Symbol(ecad_model::RepresentationId),
            Place(PageId, ecad_model::Pt),
        }
        let Some(k) = self.reports.as_ref().and_then(|r| r.row_index(i)) else { return };
        let Some(p) = self.project.clone() else { return };
        let group = self.reports.as_ref().map(|r| r.group).unwrap_or_default();
        let terminal = self.reports.as_ref().and_then(|r| r.gos.get(k).copied().flatten());
        let cores = self.reports.as_ref().is_some_and(|r| r.cores_of.is_some());
        let to = p
            .configure(&mut self.cad.session, |p| {
                let o = p.outputs();
                match tab {
                    outputs_ui::Tab::Bom => {
                        let row = o.bom(group).into_iter().nth(k)?;
                        let c = *row.devices.first()?;
                        o.device(c).and_then(|d| d.main).map(|(rep, _, _)| To::Symbol(rep))
                    }
                    outputs_ui::Tab::Wires => o.wires().get(k).and_then(|w| w.go).map(|(pg, at)| To::Place(pg, at)),
                    outputs_ui::Tab::Cables if cores => None,
                    outputs_ui::Tab::Cables => o.cables().get(k).and_then(|c| c.go).map(|(pg, at)| To::Place(pg, at)),
                    outputs_ui::Tab::Terminals => terminal.map(|(pg, at)| To::Place(pg, at)),
                }
            })
            .ok()
            .flatten();
        match to {
            Some(To::Symbol(rep)) => {
                let _ = self.goto_symbol(rep);
            }
            Some(To::Place(page, at)) => {
                if self.show_page(page).is_ok()
                    && let Ok(st) = self.cad.session.state_mut()
                {
                    let height = st.view().height.clamp(GOTO_MIN_HEIGHT, GOTO_MAX_HEIGHT);
                    let _ = st.set_view(cadcraft_engine::View { center: cadcraft_engine::geom::Vec2::new(at.x, at.y), height, twist: 0.0 });
                }
            }
            None => {}
        }
    }

    /// Brief 027: the issued rows' release folders, read from disk now.
    fn fill_folders(&mut self, r: &mut sheets_ui::Revisions) {
        if let Some(p) = self.project.clone() {
            let _ = p.read(&mut self.cad.session, |p, _| release_ui::fill_folders(&mut r.issued, p));
        }
    }

    /// The Release badge: counted again only when the kept Checks or report models change.
    fn release_badge(&mut self) -> Option<(String, Option<(usize, usize)>)> {
        let p = self.project.clone()?;
        p.read(&mut self.cad.session, |p, _| (ecad_cadcraft::revisions::pending_rev(p.ledger.document()), release_ui::badge(p))).ok()
    }

    /// Open the Release dialog (design 027 §5.1): the pending revision, or `rev` written again.
    pub fn open_release(&mut self, rev: Option<String>) {
        let read_only = self.header().read_only;
        self.release = Some(release_ui::ReleaseDialog::new(rev, read_only));
        self.refresh_release();
    }

    /// Take the dialog's view again when the project, its kept Checks or the tick changed.
    pub fn refresh_release(&mut self) {
        let Some(p) = self.project.clone() else { return };
        let Some(d) = &self.release else { return };
        let (rev, accept, key) = (d.rev.clone(), d.accept, d.key);
        let r = p.configure(&mut self.cad.session, |p| {
            // An issued revision's view is built once (its state never changes); the pending one
            // again only when what it shows can differ (`release::dialog_key`).
            if key.is_some() && (rev.is_some() || key == ecad_cadcraft::release::dialog_key(p, accept)) {
                return Ok(None);
            }
            let v = ecad_cadcraft::release::view(p, rev.as_deref(), accept, ecad_cadcraft::release::MAX_ROWS)?;
            // The kept lists exist now: the key is read after the view built them.
            let k = if rev.is_some() { 0 } else { ecad_cadcraft::release::dialog_key(p, accept).unwrap_or(0) };
            Ok::<_, String>(Some((v, k)))
        });
        match r {
            Ok(Ok(Some((v, k)))) => {
                if let Some(d) = &mut self.release {
                    d.set_view(v, k);
                }
            }
            Ok(Ok(None)) => {}
            Ok(Err(e)) | Err(e) => {
                if let Some(d) = &mut self.release {
                    d.error = Some(e);
                }
            }
        }
    }

    /// Release with the dialog's choices: `release` through the command path. A folder that
    /// exists asks §9's question; anything else refused stays in the dialog with its way out.
    pub fn release_from_dialog(&mut self, params: serde_json::Value) -> Result<serde_json::Value, String> {
        let r = self.run_value("release", params);
        match &r {
            Ok(_) => {
                // The command echoed what it wrote (design 027 §5.1).
                self.release = None;
                if self.revisions.is_some() {
                    self.open_revisions();
                }
            }
            Err(e) => {
                if let Some(d) = &mut self.release {
                    if e.contains(" exists (") && e.contains("Give existing:") {
                        d.exists = Some(e.clone());
                        d.error = None;
                    } else {
                        d.exists = None;
                        d.error = Some(e.clone());
                    }
                }
            }
        }
        r
    }

    /// The Release dialog, live while open.
    fn release_window(&mut self, ctx: &egui::Context) {
        if self.release.is_none() {
            return;
        }
        self.refresh_release();
        let answer = self.release.as_mut().and_then(|d| d.show(ctx));
        match answer {
            Some(release_ui::Out::Run(line)) => {
                let e = self.run_line(&line);
                if let Some(d) = &mut self.release {
                    d.error = e;
                }
            }
            Some(release_ui::Out::Release(params)) => {
                let _ = self.release_from_dialog(params);
            }
            Some(release_ui::Out::GoTo(c)) => {
                let _ = self.goto_check(&c);
            }
            Some(release_ui::Out::ShowReports) => {
                self.open_reports();
                if let Some(r) = &mut self.reports {
                    r.tab = outputs_ui::Tab::Bom;
                    r.only_flagged = true;
                    r.invalidate();
                }
            }
            Some(release_ui::Out::Finalise) => self.open_renumber(true, false, None),
            Some(release_ui::Out::ShowChecks) => self.panel.tab = project_panel::Tab::Checks,
            Some(release_ui::Out::SaveAs) => self.request_save_as(),
            Some(release_ui::Out::WriteIssued(rev)) => self.open_release(Some(rev)),
            Some(release_ui::Out::Close) => self.release = None,
            None => {}
        }
    }

    /// Print with the dialog's choices (Print): `printbook` through the command path.
    pub fn print_from_dialog(&mut self) -> Result<serde_json::Value, String> {
        let Some(d) = self.print_book.clone() else { return Err("Print Book is not open".into()) };
        let mut params = d.params();
        params["path"] = json!(self.output_path(&d.path)?);
        let r = self.run_value("printbook", params);
        match &r {
            Ok(_) => self.print_book = None,
            Err(e) => {
                if let Some(d) = &mut self.print_book {
                    d.error = Some(e.clone());
                }
            }
        }
        r
    }

    /// The Print Book dialog: live counts, then `printbook` on Print.
    fn print_window(&mut self, ctx: &egui::Context) {
        let Some(d) = &self.print_book else { return };
        let scope = match d.scope {
            outputs_ui::ScopeKind::All => ecad_cadcraft::printbook::Scope::All,
            outputs_ui::ScopeKind::Folder => match d.folders.get(d.folder).and_then(|f| ecad_model::FolderId::parse(&f.0)) {
                Some(f) => ecad_cadcraft::printbook::Scope::Folder(f),
                None => ecad_cadcraft::printbook::Scope::All,
            },
            outputs_ui::ScopeKind::Pages => ecad_cadcraft::printbook::Scope::Pages(d.pages.clone()),
        };
        if let Some(p) = self.project.clone()
            && let Ok((sheets, unknown, flagged)) = p.configure(&mut self.cad.session, |p| ecad_cadcraft::printbook::count(p, &scope))
            && let Some(d) = &mut self.print_book
        {
            d.sheets = sheets;
            d.unknown = unknown;
            d.flagged = flagged;
        }
        if let Some(p) = self.project.clone()
            && let Ok(notes) = p.configure(&mut self.cad.session, |p| ecad_cadcraft::printbook::notes(p, &scope))
            && let Some(d) = &mut self.print_book
        {
            d.notes = notes;
        }
        let answer = self.print_book.as_mut().and_then(|d| d.show(ctx));
        match answer {
            Some(true) => {
                let _ = self.print_from_dialog();
            }
            Some(false) => self.print_book = None,
            None => {}
        }
    }

    /// The Project Properties and Revisions windows and the question a plain undo across an issue
    /// stops at.
    fn sheet_windows(&mut self, ctx: &egui::Context) {
        if let Some(d) = &mut self.project_props {
            match d.show(ctx) {
                // Brief 031: OK may write two lines (`projectprops`, `textstyle`); the first refusal
                // stops and is shown.
                Some(sheets_ui::Out::Run(lines)) => match lines.lines().find_map(|l| self.run_line(l)) {
                    None => self.project_props = None,
                    Some(e) => {
                        if let Some(d) = &mut self.project_props {
                            d.error = Some(e);
                        }
                    }
                },
                Some(sheets_ui::Out::Close) => self.project_props = None,
                Some(sheets_ui::Out::OpenRevisions) => self.open_revisions(),
                Some(
                    sheets_ui::Out::Open(_)
                    | sheets_ui::Out::SwitchToMain
                    | sheets_ui::Out::MergeIntoMain
                    | sheets_ui::Out::Release(_)
                    | sheets_ui::Out::OpenFolder(_),
                )
                | None => {}
            }
        }
        self.refresh_revisions();
        if let Some(d) = &mut self.revisions {
            match d.show(ctx) {
                Some(sheets_ui::Out::Run(line)) => {
                    let issuing = line.starts_with("issuerevision");
                    let r = self.run_line(&line);
                    // Brief 026 (design §5.5): provisional items ask first.
                    let provisional = issuing && r.as_deref().is_some_and(|e| e.contains("provisional: \"finalise\""));
                    if provisional {
                        self.ask_issue_provisional(line);
                    } else {
                        if let Some(d) = &mut self.revisions {
                            d.error = r.clone();
                        }
                        if r.is_none() && issuing {
                            self.open_revisions();
                        }
                    }
                }
                Some(sheets_ui::Out::Open(rev)) => {
                    if let Some(e) = self.run_line(&format!("openrevision {}", json!({ "rev": rev })))
                        && let Some(d) = &mut self.revisions
                    {
                        d.error = Some(e);
                    }
                    self.refresh_panel();
                }
                Some(sheets_ui::Out::Close) => self.revisions = None,
                Some(sheets_ui::Out::SwitchToMain) => self.ask_switch("main", "main"),
                Some(sheets_ui::Out::MergeIntoMain) => {
                    self.revisions = None;
                    let shown = self.branch_info().map(|b| b.name).unwrap_or_default();
                    self.ask_merge(json!({"from": shown, "into": "main"}));
                }
                Some(sheets_ui::Out::Release(rev)) => self.open_release(rev),
                Some(sheets_ui::Out::OpenFolder(f)) => {
                    if let Err(e) = release_ui::open_folder(&f)
                        && let Some(d) = &mut self.revisions
                    {
                        d.error = Some(e);
                    }
                }
                Some(sheets_ui::Out::OpenRevisions) | None => {}
            }
        }
        let question = self.project.clone().and_then(|p| p.read(&mut self.cad.session, |p, _| p.issue_undo_question.clone()).ok().flatten());
        if let Some(q) = question {
            match sheets_ui::issue_undo(ctx, &q) {
                Some(true) => {
                    let _ = self.run_line("undoissue {}");
                }
                Some(false) => {
                    if let Some(p) = self.project.clone() {
                        let _ = p.configure(&mut self.cad.session, |p| p.issue_undo_question = None);
                    }
                }
                None => {}
            }
        }
    }

    /// Cancel in the page name box: nothing is written.
    pub fn cancel_name_box(&mut self) {
        self.name_box = None;
    }

    fn name_box_dialog(&mut self, ctx: &egui::Context) {
        let Some(b) = &mut self.name_box else { return };
        let heading = match b.kind {
            NameBoxKind::New => "New page",
            NameBoxKind::Rename(_) => "Rename page",
        };
        let (mut ok, mut cancel) = (false, false);
        egui::Modal::new(egui::Id::unique("ecad.page_name_box")).show(ctx, |ui| {
            ui.set_width(320.0);
            ui.label(RichText::new(heading).strong());
            ui.add_space(4.0);
            let id = egui::Id::unique("ecad.page_name_box.field");
            let field = ui.add(egui::TextEdit::singleline(&mut b.text).id(id).desired_width(f32::INFINITY)).accessible_name(NAME_BOX_FIELD);
            if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                ok = true;
            } else if !b.focused {
                field.request_focus();
                if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id) {
                    let all = egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(b.text.chars().count()));
                    state.cursor.set_char_range(Some(all));
                    state.store(ui.ctx(), id);
                    b.focused = true;
                }
            }
            if let Some(e) = &b.error {
                ui.label(RichText::new(e.as_str()).color(ui.visuals().warn_fg_color));
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ok |= ui.button("OK").clicked();
                cancel = ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape));
            });
        });
        // The key that closed the box is the box's: CADCraft's command line must not take it too.
        if ok || cancel {
            ctx.input_mut(|i| {
                i.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
                i.consume_key(egui::Modifiers::NONE, egui::Key::Escape);
            });
        }
        if cancel {
            self.cancel_name_box();
        } else if ok {
            self.confirm_name_box();
        }
    }

    /// Preview `page` at its extents (the panel's Preview, Space or context menu). Asked again for
    /// the page previewed now, the preview closes.
    pub fn preview_page(&mut self, page: PageId) {
        if self.preview.as_ref().is_some_and(|v| v.page == page) {
            self.preview = None;
            return;
        }
        if self.panel.active == Some(page) {
            self.panel.note = Some(project_panel::ALREADY_SHOWN.into());
            self.cad.session.echo(project_panel::ALREADY_SHOWN);
            return;
        }
        let mut v = Preview::new(page, None, None);
        if let Some(r) = self.cad.canvas.rect {
            v.place(r);
        }
        self.preview = Some(v);
    }

    /// **Preview Reference** (design 016 §4.4): the selected cable reference's other end, in the
    /// preview. Starts no command and records nothing.
    ///
    /// # Errors
    /// No reference selected, or an orphan (also echoed).
    pub fn preview_reference(&mut self) -> Result<(), String> {
        let r = (|| {
            let st = self.cad.session.state().map_err(|_| ecad_cadcraft::preview::NOT_A_REFERENCE.to_string())?;
            let (uid, handles) = (st.uid, st.selection.clone());
            let p = self.project.clone().ok_or(ecad_cadcraft::preview::NOT_A_REFERENCE)?;
            p.read(&mut self.cad.session, |p, _| {
                let r = ecad_cadcraft::preview::selected_reference(p, uid, &handles).ok_or(ecad_cadcraft::preview::NOT_A_REFERENCE)?;
                ecad_cadcraft::preview::other_end(p, r)
            })?
        })();
        match r {
            Ok((page, at)) => {
                let mut v = Preview::at_reference(page, at);
                if let Some(r) = self.cad.canvas.rect {
                    v.place(r);
                }
                self.preview = Some(v);
                Ok(())
            }
            Err(e) => {
                self.cad.session.echo(e.clone());
                Err(e)
            }
        }
    }

    /// The preview window: brought up to date (only when a change touched its page), shown, and
    /// closed when its page is gone or became active.
    fn preview_window(&mut self, ctx: &egui::Context) {
        let Some(project) = self.project.clone() else {
            self.preview = None;
            return;
        };
        let Some(v) = &mut self.preview else { return };
        if !v.update(&project, &mut self.cad.session) {
            self.preview = None;
            self.cad.session.echo("The previewed page no longer exists.");
            return;
        }
        if self.panel.active == Some(v.page) && v.revision.is_none() {
            self.preview = None;
            return;
        }
        match v.show(ctx) {
            Some(preview::Action::Close) => self.preview = None,
            Some(preview::Action::Open(page, view)) => {
                self.preview = None;
                if self.show_page(page).is_ok()
                    && let Ok(st) = self.cad.session.state_mut()
                {
                    let c = cadcraft_engine::geom::Vec2::new(view.center.x, view.center.y);
                    let _ = st.set_view(cadcraft_engine::View { center: c, height: view.height, twist: 0.0 });
                }
            }
            None => {}
        }
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        if let Some(h) = &self.held {
            self.close_dialog(ctx, h.title.clone(), h.changes);
        } else if let Some((source, output)) = self.convert_offer.clone() {
            let mut answer = None;
            egui::Modal::new(egui::Id::unique("ecad_trial_convert")).show(ctx, |ui| {
                ui.set_width(520.0);
                ui.label(RichText::new("Convert this project to the new file format?").strong());
                ui.add_space(4.0);
                ui.label(format!("{source} is a version 1 project file. This version saves projects in version 2."));
                ui.label(format!(
                    "Converting writes a new file, {output}, checks it by replaying both, and opens it. \
                     The original file is left untouched."
                ));
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("Convert and open").clicked() {
                        answer = Some(true);
                    }
                    if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        answer = Some(false);
                    }
                });
            });
            match answer {
                Some(true) => {
                    if let Err(e) = self.accept_conversion() {
                        self.notice = Some(format!("The project was not converted.\n\n{e}"));
                    }
                }
                Some(false) => self.decline_conversion(),
                None => {}
            }
        } else if self.dialog.is_some() {
            self.file_dialog(ctx);
        } else if self.name_box.is_some() {
            self.name_box_dialog(ctx);
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
            let _ = title;
            ui.label(format!("Closing the project. It has {changes} unsaved change{}.", if changes == 1 { "" } else { "s" }));
            ui.label(match (&file.path, &file.read_only) {
                (Some(_), Some(r)) => format!("The project is read-only ({r}); Save asks for another file name."),
                (Some(p), None) => format!("Save writes them to {p}."),
                (None, _) => "The project has no file yet; Save asks for a file name.".to_string(),
            });
            ui.label(RichText::new("Discard closes it and loses the unsaved changes.").color(ui.visuals().warn_fg_color));
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

/// `line` (`name {json}`) with `key` set to `value` in its JSON (brief 026: the issue's answer).
fn with_param(line: &str, key: &str, value: serde_json::Value) -> String {
    let (name, rest) = line.split_once(' ').unwrap_or((line, "{}"));
    let mut v: serde_json::Value = serde_json::from_str(rest).unwrap_or_else(|_| json!({}));
    if let Some(o) = v.as_object_mut() {
        o.insert(key.to_string(), value);
    }
    format!("{name} {v}")
}
