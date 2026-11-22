//! ecad-app: the ECAD application. The program that opens its window, parses the command line and
//! times start-up is `ecad-desktop` (design 065 §2.2).
//!
//! It is CADCraft's own window ([`CadApp`]) with the ECAD commands installed and one ECAD project
//! open at a time. Everything ECAD-specific runs through `ecad-engine`; this crate only adds
//! what a person needs to try it:
//!
//! - the **Project** panel on the far left ([`project_panel`], brief 016): the project's pages,
//!   one click to make a page the one active page, new / rename / reorder through the page
//!   commands, and in its header the project's status (name, file, unsaved changes, read-only
//!   and unconfirmed-save warnings), New, Open, Save, Save As and Close, and the experiment note
//!   (they were in a top banner until owner, 2026-11-08);
//! - a read-only **preview** of another page ([`preview`], brief 016), floating over the drawing;
//! - **project files** (`.ecad`, persistence stage 2a): Save writes to the project's file, Save As
//!   (and the first Save) asks for a file name in a small dialog of its own (the app has no
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
//!   ([`App::hover_lines`]), read only.
//! - the read-only **debugger** panel (persistence stage 2b, [`debugger`]), opened from ECAD Dev
//!   and never shown by default: selected drawing objects → ECAD IDs → model objects →
//!   transactions with before/after values → saved record offsets and bytes, with pending
//!   changes kept apart from durably saved history.
//!
//! - **live agent access** ([`agent`], brief 035): off by default; when the person turns it on
//!   (ECAD Dev ▸ Agent access, or `--control PORT` for one run), an agent connected through
//!   `ecad-mcp --connect` changes the open project between frames, recorded as the agent's, while
//!   the person watches; a minimal banner names it and offers Undo session, Pause and Disconnect.
//!
//! Excluded on purpose: native file dialogs, CADCraft file formats (plain drawings cannot be
//! written), autosave, CADCraft's raw control channel, saved window or profile state and ECAD menus
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

pub mod actions;
pub mod agent;
pub mod canvas;
pub mod checks_panel;
pub mod debugger;
pub mod developer;
pub mod devices_panel;
pub mod history_panel;
pub mod library;
pub mod lists;
pub mod merge_dialog;
pub mod outputs_ui;
pub mod preview;
pub mod project_panel;
pub mod publish;
pub mod release_ui;
pub mod renumber_dialog;
pub mod search;
pub mod settings;
pub mod sheets_ui;
pub mod shell;
pub mod text_ui;
pub mod tools;

mod file_ops;
mod folder;
mod run;
mod windows;

pub use folder::{BANNER, DEFAULT_FOLDER};

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use cadcraft_ui_egui::closing::{Choice, Closing};
use cadcraft_ui_egui::{CadApp, Services};
use debugger::Debugger;
use ecad_engine::cad::Handle;
use ecad_engine::cad::Session;
use ecad_engine::{FileStatus, Persistence, Project, ProjectHandle, session_seed};
use ecad_inspect::EventKind;
use ecad_model::PageId;
use eframe::egui::{self, RichText};
use preview::Preview;
use project_panel::{Header, ProjectPanel};
use serde_json::json;

/// The start-up marks [`App::start`] sets as it goes; ecad-desktop's start-up timing implements
/// them (its steps Shell and Project).
pub trait StartMarks {
    /// CADCraft's app is built.
    fn shell_built(&mut self);
    /// The project is open.
    fn project_open(&mut self);
}

/// Window title.
pub const TITLE: &str = "ECAD desktop (experiment)";

/// The experiment line at the top of the Project panel (one small line; [`BANNER`] on hover).
pub const NOTE: &str = "ECAD desktop (experiment): keep .ecad files in a local folder.";
/// Said wherever snapshots are mentioned.
pub const SNAPSHOT_NOTE: &str =
    "Inspector snapshots are diagnostic files for the developer inspector, not saved ECAD projects; they cannot be reopened.";
/// Name of a new project until it is saved.
pub const PROJECT_NAME: &str = "Untitled";

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
    "ECAD desktop (experiment): the PROJECT panel on the left lists the pages (click one to show it) and has \
New, Open, Save and Save As.",
    "ECAD tools are in Tool Sets > ECAD: Convert to Device, Add Connection Point, Place Symbol, Wire, Connect and \
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

/// The app's own file dialog: a path field.
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

/// What ECAD's style depends on: CADCraft re-applies its own fonts and style when one changes.
type EcadStyleKey = (&'static str, cadcraft_ui_egui::theme::ThemePref, Option<egui::Theme>, ecad_ui::Theme);

/// The harness state around CADCraft's [`CadApp`].
pub struct App {
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
    /// The ECAD command field's field (whole lines, spaces included).
    pub command: String,
    /// What to fill in, after an ECAD Tool Sets button put a template in the command field.
    command_hint: Option<&'static str>,
    /// Give the command field keyboard focus on the next frame.
    focus_command: bool,
    /// The Project panel's Developer section: the command field's reply, the inline debugger and
    /// the project summary.
    pub developer: developer::Developer,
    /// Brief 037: what ECAD's fonts and style were last installed for (CADCraft's interface
    /// language, theme choice and the system theme), and whether the style is applied once more
    /// in the next pass, when ECAD's fonts are in.
    ecad_style: Option<EcadStyleKey>,
    ecad_style_again: bool,
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
    /// The project's branch-action count and the branch shown, as last seen ([`App::sync_branch`]).
    branch_seen: Option<(u64, Option<String>)>,
    /// Counts branch actions run (rename, archive, a new branch): the branch facts are read again.
    branch_rev: u64,
    /// The window title last sent.
    title_sent: String,
    /// The `keep_open` the app last gave the project: on, except while a close or quit asks.
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
    /// The home folder that `~` in typed paths stands for ([`ecad_engine::home`] by default).
    pub home: Option<PathBuf>,
    /// The lineweight display the app last set when it opened a project; `None` before the
    /// first. A different value now means the user chose Show/Hide Lineweight, which is kept.
    lwdisplay_set: Option<bool>,
    /// The symbol search window (brief 018), opened from the ECAD tab's Find Symbol.
    pub search: search::SymbolSearch,
    /// Brief 048: the Library workspace (the browser, or the symbol builder).
    pub library: library::Library,
    /// Brief 024: the Reports window and the Print Book dialog, while open.
    pub reports: Option<outputs_ui::Reports>,
    pub print_book: Option<outputs_ui::PrintBook>,
    /// Brief 027: the Release dialog.
    pub release: Option<release_ui::ReleaseDialog>,
    /// UI rev 3: the Publish workspace (board 4.01).
    pub publish: publish::Publish,

    /// Brief 019: the ECAD section of CADCraft's Properties palette, shared with its closure.
    pub props: sheets_ui::Shared,
    /// What the section's copy was built for: project serial, change count, page, selection.
    props_seen: Option<(u64, u64, Option<PageId>, Vec<Handle>)>,
    /// How many times the section's copy was built (tests: only when it can differ).
    pub props_rebuilds: u64,
    /// The Project Properties and Revisions dialogs, when open.
    pub project_props: Option<sheets_ui::ProjectProps>,
    /// Brief 042: Settings, a place you visit (its section, the Standards screen, the inspector's rows).
    pub settings: settings::Settings,
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
    /// Brief 035: live agent access (preference, listener, bridge sessions, banner).
    pub agent: agent::Agent,
    /// Brief 038: the shell around the drawing (workspaces, panels, the developer switch).
    pub shell: shell::Shell,
}

/// What the pointer rests on, for the hover tooltip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hovered {
    /// A terminal within the capture aperture (drawn or symbol, free or connected).
    Terminal(ecad_model::ConnectionPointId),
    /// A cable reference end (its point or its text) within the pick box (brief 010).
    Reference(ecad_model::RepresentationId),
    /// The object CADCraft's rollover highlights.
    Object(Handle),
}

/// The hover tooltip's egui id (and element name, design 008 §4.3).
pub const HOVER_ID: &str = "ecad.hover";
/// Default [`App::hover_delay`].
pub const HOVER_DELAY: f64 = 0.5;

/// The app has no native file dialogs. When CADCraft asks for a file name to save a drawing,
/// the request is passed on (`requested` is set) and the app opens its own path dialog for the
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

impl App {
    /// A harness with one new ECAD project. `snapshots`: where exported snapshots go (none when
    /// `None`).
    ///
    /// # Errors
    /// The project could not be created.
    pub fn new(snapshots: Option<PathBuf>) -> Result<Self, String> {
        Self::start::<dyn StartMarks>(snapshots, None, None)
    }

    /// A harness opening the project file at `path`. If it cannot be opened, a new project is
    /// created instead and the reason is shown.
    ///
    /// # Errors
    /// Neither the file nor a new project could be opened.
    pub fn with_file(snapshots: Option<PathBuf>, path: &str) -> Result<Self, String> {
        Self::start::<dyn StartMarks>(snapshots, Some(path), None)
    }

    /// [`App::with_file`] when `project` is given, otherwise [`App::new`], marking
    /// [`StartMarks::shell_built`] after CADCraft's app is built and [`StartMarks::project_open`]
    /// after the project is open when `timing` is given (ecad-desktop's start-up timing).
    ///
    /// # Errors
    /// Neither the file (if any) nor a new project could be opened.
    pub fn start<M: StartMarks + ?Sized>(snapshots: Option<PathBuf>, project: Option<&str>, mut timing: Option<&mut M>) -> Result<Self, String> {
        let mut t = Self::empty(snapshots);
        if let Some(timing) = timing.as_deref_mut() {
            timing.shell_built();
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
            timing.project_open();
        }
        Ok(t)
    }

    fn empty(snapshots: Option<PathBuf>) -> Self {
        let requested = Arc::new(AtomicBool::new(false));
        // Brief 035: CadApp's own control channel is fed only by the app's agent drain, with
        // the allowlisted reads; the port's requests never reach it directly.
        let (cad_tx, cad_rx) = std::sync::mpsc::sync_channel(64);
        let mut cad = CadApp::new(Session::empty(), services(Arc::clone(&requested))).with_control(cad_rx);
        let mut agent = agent::Agent::from_settings(ecad_engine::views::Settings::from_env());
        agent.set_cad_channel(cad_tx);
        cad.extra_tool_tabs = vec![tools::tab()];
        let props: sheets_ui::Shared = Arc::default();
        cad.owner_properties = Some(sheets_ui::section(Arc::clone(&props)));
        let shell = shell::Shell::new(Arc::clone(&props));
        App {
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
            developer: developer::Developer::default(),
            ecad_style: None,
            ecad_style_again: false,
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
            home: ecad_engine::home(),
            lwdisplay_set: None,
            search: search::SymbolSearch::default(),
            library: library::Library::default(),
            reports: None,
            print_book: None,
            release: None,
            publish: publish::Publish::default(),

            props,
            props_seen: None,
            props_rebuilds: 0,
            project_props: None,
            settings: settings::Settings::default(),
            revisions: None,
            merge: None,
            renumber: None,
            issue_prompt: None,
            issue_after_finalise: None,
            new_page_folder: None,
            panel_revision: None,
            revisions_seen: 0,
            frame_checked: Vec::new(),
            agent,
            shell,
        }
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

    /// The message waiting in the notice dialog.
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// Dismiss the notice dialog.
    pub fn dismiss_notice(&mut self) {
        self.notice = None;
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
    /// app's dialog, open the Save As dialog when CADCraft asked for a file name, and report
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
                self.cad.session.echo("Plain CADCraft drawings cannot be saved in the ECAD desktop app.");
            }
        }
        self.report_saves();
    }

    /// Zoom the page window to its drawing without the sheet frame (brief 019: Zoom Extents fits
    /// the whole sheet). Returns `false` when there is no drawing to zoom to.
    pub fn zoom_to_drawing(&mut self) -> bool {
        let Ok(d) = self.cad.session.doc() else { return false };
        let mut b = ecad_engine::cad::Bounds2::default();
        for e in d.model.iter().filter(|e| e.common.layer != ecad_engine::frame::LAYER) {
            b = b.union(&ecad_engine::cad::entity_bounds(d, e, 0));
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
        let only_frame = ecad_engine::frame::drop_from_selection(&mut self.cad.session);
        self.frame_checked = self.cad.session.selection();
        if only_frame {
            self.cad.session.echo(ecad_engine::frame::DERIVED);
        }
    }

    fn event_count(&mut self) -> u64 {
        let Some(p) = self.project.as_ref() else { return 0 };
        p.read(&mut self.cad.session, |p, _| p.events.last().map_or(0, |e| e.n + 1)).unwrap_or(0)
    }

    /// Per-frame logic (before layout).
    pub fn logic(&mut self, ctx: &egui::Context) {
        // Brief 035: the author check and the agent's calls, before any input of this frame.
        self.agent_frame(ctx);
        if ctx.input(|i| i.viewport().close_requested()) {
            self.prepare_quit();
        }
        self.cad.logic(ctx);
        self.after_logic();
        if std::mem::take(&mut self.cad.quit_requested) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// Lay out the harness dialogs, the debugger (when open), the shell (brief 038) with
    /// CADCraft's drawing in its main view, the floating windows and the hover tooltip.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        // ECAD's fonts and style first: on the first frame nothing is laid out yet, so egui takes
        // the fonts at once (CADCraft no longer installs its own; `Embedding::style` is off).
        self.ecad_style(ui.ctx());
        self.sync_branch();
        self.dialogs(ui.ctx());
        self.sync_title(&ui.ctx().clone());
        if self.debugger.open && self.developer.open && self.developer.body == developer::Body::Debugger {
            // The snapshot is taken (and the project released) before anything is drawn.
            self.refresh_debugger(false);
        }
        self.shell_ui(ui);
        self.agent_banner_ui(ui.ctx());
        self.drop_frame_selection();
        self.handle_tools();
        self.sheet_windows(ui.ctx());
        self.host_windows();
        self.report_windows(ui.ctx());
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

    /// Brief 037: ECAD's fonts (IBM Plex Sans for the interface, Barlow Semi Condensed for the
    /// sheet) on top of CADCraft's font set for the language (upstream seam US2), and ECAD's style
    /// from the tokens. Runs after CADCraft's frame start, which installs its own fonts and style
    /// when the language or theme changes; ECAD's then replace them, in the same pass. The style
    /// is applied again in the next pass, once ECAD's font families exist.
    fn ecad_style(&mut self, ctx: &egui::Context) {
        // UI rev 3 (boards 1.17, 1.18): the person's light or dark interface, read by every
        // `ecad_ui::SHELL` colour drawn on this thread from here on.
        let theme = self.shell.state.theme.ui();
        ecad_ui::set_theme(theme);
        let key = (self.cad.language(), self.cad.ui.theme, ctx.system_theme(), theme);
        if self.ecad_style != Some(key) {
            ecad_ui::fonts::install(ctx, cadcraft_ui_egui::theme::font_definitions(key.0));
            ecad_ui::style::apply(ctx);
            self.ecad_style = Some(key);
            self.ecad_style_again = true;
            ctx.request_repaint();
        } else if std::mem::take(&mut self.ecad_style_again) {
            ecad_ui::style::apply(ctx);
        }
    }

    /// The app is exiting normally: export a final snapshot if enabled and a project is open.
    pub fn on_exit(&mut self) {
        if self.snapshots.is_some() && self.project_open() {
            let _ = self.export_snapshot("exit");
        }
        self.agent.shut_down();
    }

    /// Brief 035: the Undo agent session confirmation, while open (the banner itself is one line
    /// in the Developer section until the UI shell).
    fn agent_banner_ui(&mut self, ctx: &egui::Context) {
        self.agent_undo_window(ctx);
    }

    /// Open History with `seq` selected (the banner's last change).
    pub fn open_history_at(&mut self, seq: u64) {
        let Some(p) = self.project.clone() else { return };
        let Ok(branch) = p.read(&mut self.cad.session, |p, _| ecad_engine::history::current(p)) else { return };
        self.panel.tab = project_panel::Tab::History;
        self.panel.history.steps_open = true;
        self.panel.history.selected = Some(history_panel::Sel { branch, seq, kind: ecad_file::history::NodeKind::Fork, name: None });
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
        let devices_shown = self.panel.tab == project_panel::Tab::Devices || self.lists_shows_devices();
        let selection = if devices_shown { self.cad.session.selection() } else { Vec::new() };
        let panel = &mut self.panel;
        let panel_revision = &mut self.panel_revision;
        let _ = p.read(&mut self.cad.session, |p, _| {
            panel.active = p.active_page();
            // An issued revision opened read-only (design 019 §7.3): its pages, under its bar.
            match &p.revision_view {
                Some(rv) => {
                    if *panel_revision != Some(rv.id) {
                        panel.set_tree(ecad_engine::tree_rows(&rv.doc, |_| false));
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
            // Brief 054: folder start numbers and the generated pages, when anything changed.
            if panel.starts_at != Some(p.change_count()) {
                let doc = p.ledger.document();
                panel.starts = ecad_engine::page_types::starts(doc);
                panel.generated = doc.page_order.iter().copied().filter(|pg| ecad_engine::generated::kind_of(doc, *pg).is_some()).collect();
                panel.starts_at = Some(p.change_count());
            }
            // The Devices tab's copy, only while it is shown: rebuilt once, then refreshed only for
            // the devices a change touched.
            if devices_shown {
                panel.devices.sync(serial, p);
                // Brief 039: symbols selected on the page select their rows.
                panel.devices.follow(p, &selection);
            }
            // The History tab's copy, only while it is shown (brief 020a; its own update map).
            if panel.tab == project_panel::Tab::History {
                panel.history.sync(serial, p);
            }
        });
        // The Checks copy (brief 022), for the shell's drawer and Checks pill (brief 038): the
        // project builds its list on the first show and keeps it from each change; the copy
        // follows its revision.
        {
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
                use ecad_engine::history;
                let file = p.path.as_deref().and_then(|x| std::path::Path::new(x).file_name()).map(|f| f.to_string_lossy().into_owned());
                let Some(v) = p.file.project() else {
                    return BranchInfo { name: "main".into(), count: 1, bar: None, parent: None, release_line: true, file };
                };
                let rec = v.branch_record().clone();
                let rows = history::branch_rows(p).unwrap_or_default();
                let parent = rec.parent.as_deref().and_then(ecad_file::schema::BranchId::parse);
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
        let Ok(now) = p.read(&mut self.cad.session, |p, _| (p.branch_gen(), p.file.project().map(|v| v.branch().hex()))) else { return };
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
}
