//! The `App` struct: the harness state around CADCraft's [`CadApp`], its start-up, per-frame logic and window
//! title, with the small types that belong to it (status, branch info, name box, file dialog, held close).

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::developer::debugger::Debugger;
use crate::pages::preview::Preview;
use crate::pages::project_panel::{Header, ProjectPanel};
use crate::{agent, developer, library, publish, settings, shell};
use cadcraft_ui_egui::closing::Closing;
use cadcraft_ui_egui::{CadApp, Services};
use ecad_engine::ProjectHandle;
use ecad_engine::cad::Handle;
use ecad_engine::cad::Session;
use ecad_file::binding::FileStatus;
use ecad_model::PageId;
use eframe::egui::{self};
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

/// The experiment line at the top of the Project panel (one small line).
pub const NOTE: &str = "ECAD desktop (experiment): keep .ecad files in a local folder.";
/// Said wherever snapshots are mentioned.
pub const SNAPSHOT_NOTE: &str =
    "Inspector snapshots are diagnostic files for the developer inspector, not saved ECAD projects; they cannot be reopened.";
/// Name of a new project until it is saved.
pub(crate) const PROJECT_NAME: &str = "Untitled";

/// The project's status line at the top of the Project panel, for example
/// `Project "demo": 2 pages, file C:\dev\ecad-projects\demo.ecad, no unsaved changes.`
pub fn status_line(s: &Status) -> String {
    let plural = |n: u64, w: &str| format!("{n} {w}{}", ecad_widgets::plural(n));
    let file = s.file.path.as_deref().map_or("not saved to a file yet".to_string(), |p| format!("file {p}"));
    let changes = if s.unsaved == 0 { "no unsaved changes".to_string() } else { format!("{} not saved", plural(s.unsaved, "change")) };
    format!("Project \"{}\": {}, {file}, {changes}.", s.name, plural(s.pages as u64, "page"))
}

/// Lines echoed at the command line when a project is created.
pub(crate) const INTRO: &[&str] = &[
    "ECAD desktop (experiment): the PROJECT panel on the left lists the pages (click one to show it) and has \
New, Open, Save and Save As.",
    "ECAD tools are in Tool Sets > ECAD: Convert to Device, Add Connection Point, Place Symbol, Wire, Connect and \
Disconnect work by clicking (follow the prompts at the command line; Enter finishes, Esc cancels).",
    "JSON forms go in the ECAD command field (PROJECT panel > Developer, at the top; the command line below takes \
a space as Enter), for example:  wire {\"points\":[[0,0],[40,0]]}",
    "ECAD commands: convertcomponent, addconnectionpoint, wire, connect, disconnect, ecaddelete, setprop, cableadd, place, \
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
/// panel's **+**: prefilled, OK or Enter runs one command (one transaction,
/// one `u`), Cancel or Esc does nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameBox {
    pub kind: NameBoxKind,
    pub text: String,
    /// The command's refusal, shown in the box (which stays open).
    pub error: Option<String>,
    pub(crate) focused: bool,
}

/// The drawing height a Devices tab go to shows at least and at most, in drawing units (mm).
pub(crate) const GOTO_MIN_HEIGHT: f64 = 60.0;
/// See [`GOTO_MIN_HEIGHT`].
pub(crate) const GOTO_MAX_HEIGHT: f64 = 600.0;

/// Accessible label of the name box's field.
pub(crate) const NAME_BOX_FIELD: &str = "Page title";

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
pub(crate) struct FileDialog {
    pub(crate) kind: DialogKind,
    pub(crate) path: String,
    pub(crate) error: Option<String>,
    /// A close waiting for this Save As: it continues once the project is saved.
    pub(crate) then_close: Option<Closing>,
    /// The existing file the user was warned about; saving again replaces it.
    pub(crate) confirmed_replace: Option<String>,
    /// The path field has been given keyboard focus (once, when the dialog opens or fails).
    pub(crate) focused: bool,
}

/// A close of a project with unsaved changes, held until the user answers.
pub(crate) struct Held {
    /// CADCraft's close request, taken out of `CadApp` so its "Save changes?" prompt never shows.
    pub(crate) closing: Closing,
    pub(crate) title: String,
    pub(crate) changes: u64,
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

/// What the title, header and Revisions show about branches.
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

/// The window title: the branch once the project has more than one.
pub fn window_title(info: Option<&BranchInfo>) -> String {
    match info {
        Some(b) if b.count > 1 => format!("{TITLE} — {} · {}", b.file.as_deref().unwrap_or("unsaved"), b.name),
        _ => TITLE.to_string(),
    }
}

/// What ECAD's style depends on: CADCraft re-applies its own fonts and style when one changes.
type EcadStyleKey = (&'static str, cadcraft_ui_egui::theme::ThemePref, Option<egui::Theme>, ecad_widgets::Theme);

/// The harness state around CADCraft's [`CadApp`].
pub struct App {
    pub cad: CadApp,
    pub(crate) project: Option<ProjectHandle>,
    /// The Project panel's Developer section: the command field's reply, the inline debugger and
    /// the project summary.
    pub developer: developer::Developer,
    /// The read-only debugger panel (closed until opened from ECAD Dev).
    pub debugger: Debugger,
    /// The kept views' first builds, on the background pool (issue 48).
    pub warm: ecad_engine::warm::Warm,
    /// The Release dialog's view, built on the background pool (its rev, `None` for the pending
    /// one, and tick), and the rev, tick and state it was asked for.
    pub release_build: ecad_engine::background::Jobs<(Option<String>, bool, ecad_engine::releases::release::View)>,
    pub release_asked: Option<(Option<String>, bool, ecad_engine::background::At)>,
    /// The Project panel.
    pub panel: ProjectPanel,
    /// The symbol search window, opened from the ECAD tab's Find Symbol.
    pub search: crate::library::search::SymbolSearch,
    /// The Library workspace (the browser, or the symbol builder).
    pub library: library::Library,
    /// The Publish workspace.
    pub publish: publish::Publish,
    /// The ECAD section of CADCraft's Properties palette, shared with its closure.
    pub props: crate::settings::sheets::Shared,
    /// Settings, a place you visit (its section, the Standards screen, the inspector's rows).
    pub settings: settings::Settings,
    /// Live agent access (preference, listener, bridge sessions, banner).
    pub agent: agent::Agent,
    /// The shell around the drawing (workspaces, panels, the developer switch).
    pub shell: shell::Shell,
    /// The project's files: snapshots, the file dialog, Save As and closing.
    pub files: Files,
    /// The ECAD command field.
    pub command: CommandField,
    /// The hover description.
    pub hover: Hover,
    /// The branch facts and the revision shown.
    pub(crate) branch: BranchView,
    /// What each copy the app keeps was built for.
    pub seen: Seen,
    /// The open dialogs and what they wait on.
    pub dialogs: Dialogs,
}

/// The project's files: snapshots, the file dialog, Save As and closing.
pub struct Files {
    pub(crate) snapshots: Option<PathBuf>,
    pub(crate) snapshot_count: u32,
    pub(crate) held: Option<Held>,
    pub(crate) dialog: Option<FileDialog>,
    /// Set by CADCraft's Save As (menu, Ctrl+Shift+S, or Save of a project with no file).
    pub(crate) save_as_requested: Arc<AtomicBool>,
    /// A Save As that a close waits for is being confirmed (the close goes on after it).
    pub(crate) confirming_close: bool,
    /// The `keep_open` the app last gave the project: on, except while a close or quit asks.
    pub(crate) keep_open: Option<bool>,
    /// The home folder that `~` in typed paths stands for ([`crate::shell::host::Host::home`] by default).
    pub home: Option<PathBuf>,
}

/// The ECAD command field.
pub struct CommandField {
    /// The ECAD command field's field (whole lines, spaces included).
    pub text: String,
    /// What to fill in, after an ECAD Tool Sets button put a template in the command field.
    pub(crate) hint: Option<&'static str>,
    /// Give the command field keyboard focus on the next frame.
    pub(crate) focus: bool,
    /// Give the drawing the keyboard on the next frame (after a tool started a prompt).
    pub(crate) focus_drawing: bool,
}

/// The hover description.
pub struct Hover {
    /// The ECAD object under the pointer and since when (egui time, seconds).
    pub(crate) under: Option<(Hovered, f64)>,
    /// The hover description shown now.
    pub(crate) lines: Option<Vec<String>>,
    /// How long the pointer rests on an object before its description shows, in seconds.
    pub delay: f64,
}

/// The branch facts and the revision shown.
pub(crate) struct BranchView {
    /// The branch facts the title, header and Revisions show, with the project serial
    /// and branch-action count they were read at (read again only when either moves).
    pub(crate) info: Option<((u64, u64), Option<BranchInfo>)>,
    /// The project's branch-action count and the branch shown, as last seen ([`App::sync_branch`]).
    pub(crate) seen: Option<(u64, Option<String>)>,
    /// Counts branch actions run (rename, archive, a new branch): the branch facts are read again.
    pub(crate) rev: u64,
    /// The revision shown in the panel (its log point), when one is open.
    pub(crate) panel_revision: Option<u64>,
    /// The change count the Revisions dialog was filled at (it follows
    /// `SetProperty(Project, rev.*)` and Name/UnnameVersion, so undo and redo too).
    pub(crate) revisions_seen: u64,
}

/// What each copy the app keeps was built for.
pub struct Seen {
    /// Events of the project already reported.
    pub(crate) events: u64,
    /// Which project the panel's copy belongs to: counts projects attached in this session, and
    /// branch switches (every copy rebuilds once, as on open).
    pub(crate) project_serial: u64,
    /// The window title last sent.
    pub(crate) title_sent: String,
    /// What the section's copy was built for: project serial, change count, page, selection.
    pub(crate) props_seen: Option<(u64, u64, Option<PageId>, Vec<Handle>)>,
    /// How many times the section's copy was built (tests: only when it can differ).
    pub props_rebuilds: u64,
    /// The selection last checked for frame objects.
    pub(crate) frame_checked: Vec<Handle>,
    /// What ECAD's fonts and style were last installed for (CADCraft's interface
    /// language, theme choice and the system theme), and whether the style is applied once more
    /// in the next pass, when ECAD's fonts are in.
    pub(crate) ecad_style: Option<EcadStyleKey>,
    pub(crate) ecad_style_again: bool,
    /// The lineweight display the app last set when it opened a project; `None` before the
    /// first. A different value now means the user chose Show/Hide Lineweight, which is kept.
    pub(crate) lwdisplay_set: Option<bool>,
}

/// The open dialogs and what they wait on.
pub struct Dialogs {
    /// A message waiting in a dialog.
    pub(crate) notice: Option<String>,
    /// The page name box (New Page and Rename Page), when open.
    pub name_box: Option<NameBox>,
    /// The folder the page name box's new page goes in.
    pub(crate) new_page_folder: Option<ecad_model::FolderId>,
    /// The read-only preview of another page, when open.
    pub preview: Option<Preview>,
    /// The Reports window and the Print Book dialog, while open.
    pub reports: Option<crate::reports::panel::Reports>,
    pub print_book: Option<crate::reports::panel::PrintBook>,
    /// The Release dialog.
    pub release: Option<crate::history::release_dialog::ReleaseDialog>,
    /// The Project Properties and Revisions dialogs, when open.
    pub project_props: Option<crate::settings::sheets::ProjectProps>,
    pub revisions: Option<crate::settings::sheets::Revisions>,
    /// The merge dialog, when open.
    pub merge: Option<crate::history::merge_dialog::MergeDialog>,
    /// The Renumber dialog, also Finalise numbering, when open.
    pub renumber: Option<crate::devices::renumber_dialog::RenumberDialog>,
    /// The issue line waiting on the provisional question, and its text.
    pub issue_prompt: Option<(String, String)>,
    /// The issue line to run once Finalise numbering is applied (Finalise numbering now).
    pub(crate) issue_after_finalise: Option<String>,
}

/// What the pointer rests on, for the hover tooltip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hovered {
    /// A terminal within the capture aperture (drawn or symbol, free or connected).
    ConnectionPoint(ecad_model::ConnectionPointId),
    /// A cable reference end (its point or its text) within the pick box.
    Reference(ecad_model::RepresentationId),
    /// The object CADCraft's rollover highlights.
    Object(Handle),
}

/// The hover tooltip's egui id (and element name).
pub const HOVER_ID: &str = "ecad.hover";
/// The default hover delay ([`App::hover`]).
pub(crate) const HOVER_DELAY: f64 = 0.5;

impl App {
    /// A harness with one new ECAD project. `snapshots`: where exported snapshots go (none when
    /// `None`).
    ///
    /// # Errors
    /// The project could not be created.
    pub fn new(snapshots: Option<PathBuf>) -> Result<Self, String> {
        // Headless: the kept views' first builds run on the calling thread, as the tests that read
        // them at once need (the desktop builds them on the background pool: `App::start`).
        let mut t = Self::start::<dyn StartMarks>(snapshots, None, None)?;
        t.warm.set_inline(true);
        Ok(t)
    }

    /// A harness opening the project file at `path`. If it cannot be opened, a new project is
    /// created instead and the reason is shown.
    ///
    /// # Errors
    /// Neither the file nor a new project could be opened.
    pub fn with_file(snapshots: Option<PathBuf>, path: &str) -> Result<Self, String> {
        let mut t = Self::start::<dyn StartMarks>(snapshots, Some(path), None)?;
        t.warm.set_inline(true);
        Ok(t)
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
                    t.dialogs.notice = Some(format!("The project file could not be opened: {e}"));
                }
            }
            None => t.new_project()?,
        }
        if let Some(timing) = timing {
            timing.project_open();
        }
        Ok(t)
    }

    pub(crate) fn empty(snapshots: Option<PathBuf>) -> Self {
        let requested = Arc::new(AtomicBool::new(false));
        // CadApp's own control channel is fed only by the app's agent drain, with
        // the allowlisted reads; the port's requests never reach it directly.
        let (cad_tx, cad_rx) = std::sync::mpsc::sync_channel(64);
        let mut cad = CadApp::new(Session::empty(), Services::default()).with_control(cad_rx);
        let mut agent = agent::Agent::from_settings(ecad_engine::cables::views::Settings::from_env());
        agent.set_cad_channel(cad_tx);
        cad.extra_tool_tabs = vec![crate::commands::tools::tab()];
        let props: crate::settings::sheets::Shared = Arc::default();
        cad.owner_properties = Some(crate::settings::sheets::section(Arc::clone(&props)));
        let shell = shell::Shell::new(Arc::clone(&props));
        App {
            cad,
            project: None,
            developer: developer::Developer::default(),
            debugger: Debugger::default(),
            warm: ecad_engine::warm::Warm::default(),
            release_build: Default::default(),
            release_asked: None,
            panel: ProjectPanel::default(),
            search: crate::library::search::SymbolSearch::default(),
            library: library::Library::default(),
            publish: publish::Publish::default(),
            props,
            settings: settings::Settings::default(),
            agent,
            shell,
            files: Files {
                snapshots,
                held: None,
                dialog: None,
                save_as_requested: requested,
                snapshot_count: 0,
                keep_open: None,
                confirming_close: false,
                home: crate::shell::host::host().home.clone(),
            },
            command: CommandField { text: String::new(), hint: None, focus: false, focus_drawing: false },
            hover: Hover { under: None, lines: None, delay: HOVER_DELAY },
            branch: BranchView { info: None, seen: None, rev: 0, panel_revision: None, revisions_seen: 0 },
            seen: Seen {
                events: 0,
                ecad_style: None,
                ecad_style_again: false,
                project_serial: 0,
                title_sent: TITLE.to_string(),
                lwdisplay_set: None,
                props_seen: None,
                props_rebuilds: 0,
                frame_checked: Vec::new(),
            },
            dialogs: Dialogs {
                notice: None,
                preview: None,
                name_box: None,
                reports: None,
                print_book: None,
                release: None,
                project_props: None,
                revisions: None,
                merge: None,
                renumber: None,
                issue_prompt: None,
                issue_after_finalise: None,
                new_page_folder: None,
            },
        }
    }

    /// The current project (open or already closed).
    /// The flag CADCraft's Save As request sets; ecad-desktop's services hook raises it.
    pub fn save_as_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.files.save_as_requested)
    }

    pub fn project(&self) -> Option<&ProjectHandle> {
        self.project.as_ref()
    }

    /// Whether a project is open (with or without its page window).
    pub fn project_open(&self) -> bool {
        self.project.as_ref().is_some_and(|p| p.is_attached(&self.cad.session))
    }

    /// Whether the project's page window is open.
    pub fn page_window_open(&self) -> bool {
        self.cad.session.docs.iter().any(|d| self.cad.session.ext.owner_of(d.uid).is_some())
    }

    /// The message waiting in the notice dialog.
    pub fn notice(&self) -> Option<&str> {
        self.dialogs.notice.as_deref()
    }

    /// Dismiss the notice dialog.
    pub fn dismiss_notice(&mut self) {
        self.dialogs.notice = None;
    }

    /// The open project's name, pages, unsaved transactions and file.
    pub fn status(&mut self) -> Option<Status> {
        if !self.project_open() {
            return None;
        }
        let p = self.project.as_ref()?;
        p.read(&mut self.cad.session, |p, _| Status {
            name: p.name.clone(),
            pages: p.ledger.document().page_order().len(),
            unsaved: p.unsaved_transactions(),
            file: p.file_status(),
        })
        .ok()
    }

    /// Per-frame work after CADCraft's own logic: hold a close of an edited ECAD project for the
    /// app's dialog, open the Save As dialog when CADCraft asked for a file name, and report
    /// saves.
    pub fn after_logic(&mut self) {
        // Only the page being shown is redrawn on a commit: a page a commit
        // elsewhere left stale is redrawn here once its tab is in front. A busy project is tried
        // again next frame (a failed projection is reported by the redraw itself).
        self.handle_tools();
        self.drop_frame_selection();
        self.hold_close();
        self.sync_keep_open();
        if self.files.save_as_requested.swap(false, Ordering::Relaxed) {
            let owned = self.cad.session.state().is_ok_and(|st| self.cad.session.ext.owner_of(st.uid).is_some());
            if owned {
                if self.files.dialog.is_none() {
                    self.request_save_as();
                }
            } else {
                self.cad.session.echo("Plain CADCraft drawings cannot be saved in the ECAD desktop app.");
            }
        }
        self.report_saves();
    }

    /// Zoom the page window to its drawing without the sheet frame (Zoom Extents fits
    /// the whole sheet). Returns `false` when there is no drawing to zoom to.
    pub fn zoom_to_drawing(&mut self) -> bool {
        let Ok(d) = self.cad.session.doc() else { return false };
        let mut b = ecad_engine::cad::Bounds2::default();
        for e in d.model.iter().filter(|e| e.common.layer != ecad_electrical::pages::frames::LAYER) {
            b = b.union(&ecad_engine::cad::entity_bounds(d, e, 0));
        }
        if b.is_empty() {
            return false;
        }
        let m = ((b.max.x - b.min.x).max(b.max.y - b.min.y) * 0.1).max(5.0);
        let (p1, p2) = ([b.min.x - m, b.min.y - m], [b.max.x + m, b.max.y + m]);
        self.cad.run("zoom", json!({"mode": "window", "p1": p1, "p2": p2})).is_ok()
    }

    /// The sheet frame is never selected: a selection that held only the frame
    /// says why it is empty.
    pub(crate) fn drop_frame_selection(&mut self) {
        // Only when the selection changed: no work on a frame where nothing was picked.
        let sel = self.cad.session.selection();
        if sel == self.seen.frame_checked {
            return;
        }
        let only_frame = ecad_engine::pages::frame::drop_from_selection(&mut self.cad.session);
        self.seen.frame_checked = self.cad.session.selection();
        if only_frame {
            self.cad.session.echo(ecad_electrical::pages::frames::DERIVED);
        }
    }

    pub(crate) fn event_count(&mut self) -> u64 {
        let Some(p) = self.project.as_ref() else { return 0 };
        p.read(&mut self.cad.session, |p, _| p.events.last().map_or(0, |e| e.n + 1)).unwrap_or(0)
    }

    /// Per-frame logic (before layout).
    pub fn logic(&mut self, ctx: &egui::Context) {
        // The author check and the agent's calls, before any input of this frame.
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

    /// Lay out the harness dialogs, the debugger (when open), the shell with
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
        if std::mem::take(&mut self.command.focus_drawing) {
            // Keys go to CADCraft's command line when no text field has the keyboard.
            ui.ctx().memory_mut(|m| {
                if let Some(id) = m.focused() {
                    m.surrender_focus(id);
                }
            });
        }
        self.hover_tooltip(ui.ctx());
    }

    /// ECAD's fonts (IBM Plex Sans for the interface, Barlow Semi Condensed for the
    /// sheet) on top of CADCraft's font set for the language, and ECAD's style
    /// from the tokens. Runs after CADCraft's frame start, which installs its own fonts and style
    /// when the language or theme changes; ECAD's then replace them, in the same pass. The style
    /// is applied again in the next pass, once ECAD's font families exist.
    pub(crate) fn ecad_style(&mut self, ctx: &egui::Context) {
        // The person's light or dark interface, read by every
        // `ecad_widgets::SHELL` colour drawn on this thread from here on.
        let theme = self.shell.state.theme.ui();
        ecad_widgets::set_theme(theme);
        let key = (self.cad.language(), self.cad.ui.theme, ctx.system_theme(), theme);
        if self.seen.ecad_style != Some(key) {
            ecad_widgets::fonts::install(ctx, cadcraft_ui_egui::theme::font_definitions(key.0));
            ecad_widgets::style::apply(ctx);
            self.seen.ecad_style = Some(key);
            self.seen.ecad_style_again = true;
            ctx.request_repaint();
        } else if std::mem::take(&mut self.seen.ecad_style_again) {
            ecad_widgets::style::apply(ctx);
        }
    }

    /// The app is exiting normally: export a final snapshot if enabled and a project is open.
    pub fn on_exit(&mut self) {
        if self.files.snapshots.is_some() && self.project_open() {
            let _ = self.export_snapshot("exit");
        }
        self.agent.shut_down();
    }

    /// The Undo agent session confirmation, while open (the banner itself is one line
    /// in the Developer section until the UI shell).
    pub(crate) fn agent_banner_ui(&mut self, ctx: &egui::Context) {
        self.agent_undo_window(ctx);
    }

    /// Open History with `seq` selected (the banner's last change).
    pub fn open_history_at(&mut self, seq: u64) {
        let Some(p) = self.project.clone() else { return };
        let Ok(branch) = p.read(&mut self.cad.session, |p, _| ecad_engine::history::current(p)) else { return };
        self.panel.tab = crate::pages::project_panel::Tab::History;
        self.panel.history.steps_open = true;
        self.panel.history.selected = Some(crate::history::panel::Sel { branch, seq, kind: ecad_file::history::NodeKind::Fork, name: None });
    }

    /// Bring the Project panel's copy up to date: one change-count comparison per frame, the rows
    /// read again only when it moved (or the project changed). A busy project keeps the copy.
    pub fn refresh_panel(&mut self) {
        let Some(p) = self.project.clone().filter(|p| p.is_attached(&self.cad.session)) else {
            if self.panel.rows().is_empty() && self.panel.active.is_none() {
                return;
            }
            self.panel.clear();
            self.dialogs.preview = None;
            return;
        };
        let serial = self.seen.project_serial;
        let devices_shown = self.panel.tab == crate::pages::project_panel::Tab::Devices || self.lists_shows_devices();
        let selection = if devices_shown { self.cad.session.selection() } else { Vec::new() };
        let panel = &mut self.panel;
        let panel_revision = &mut self.branch.panel_revision;
        let _ = p.read(&mut self.cad.session, |p, _| {
            panel.active = p.active_page();
            // An issued revision opened read-only: its pages, under its bar.
            match &p.display.revision_view {
                Some(rv) => {
                    if *panel_revision != Some(rv.id) {
                        panel.set_tree(ecad_engine::tree_rows(&rv.doc, |_| false));
                        panel.revision = Some(rv.bar.clone().unwrap_or_else(|| crate::settings::sheets::revision_bar(&rv.rev, &rv.date)));
                        *panel_revision = Some(rv.id);
                    }
                    if panel.tab == crate::pages::project_panel::Tab::History {
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
            // Folder start numbers and the generated pages, when anything changed.
            if panel.starts_at != Some(p.change_count()) {
                let doc = p.ledger.document();
                panel.starts = ecad_electrical::pages::page_types::starts(doc);
                panel.generated =
                    doc.page_order().iter().copied().filter(|pg| ecad_electrical::pages::generated::kind_of(doc, *pg).is_some()).collect();
                panel.starts_at = Some(p.change_count());
            }
            // The active page's sheets, listed under its row.
            let key = (p.change_count(), p.page_list_version(), p.active_page());
            if panel.sheets_at != Some(key) {
                let doc = p.ledger.document();
                let sheets = match p.active_page() {
                    Some(pg) if ecad_electrical::pages::generated::kind_of(doc, pg).is_some() => {
                        let book = ecad_model::document::book::Book::of(doc);
                        let numbers = ecad_electrical::pages::generated::sheet_numbers(doc, &book, pg, p.kept_reports());
                        crate::pages::project_panel::Sheets { page: Some(pg), shown: p.shown_sheet(pg).0, numbers }
                    }
                    _ => crate::pages::project_panel::Sheets::default(),
                };
                panel.set_sheets(sheets);
                panel.sheets_at = Some(key);
            }
            // The Devices tab's copy, only while it is shown: rebuilt once, then refreshed only for
            // the devices a change touched.
            if devices_shown {
                panel.devices.sync(serial, p);
                // Symbols selected on the page select their rows.
                panel.devices.follow(p, &selection);
            }
            // The History tab's copy, only while it is shown (its own update map).
            if panel.tab == crate::pages::project_panel::Tab::History {
                panel.history.sync(serial, p);
            }
        });
        // The Checks copy, for the shell's drawer and Checks pill: the
        // project builds its list on the first show and keeps it from each change; the copy
        // follows its revision.
        {
            let (checks, warm) = (&mut self.panel.checks, &mut self.warm);
            let _ = p.configure(&mut self.cad.session, |p| {
                checks.building = !warm.pump(p);
                if !checks.building {
                    checks.sync(serial, p);
                }
            });
        }
        self.panel.previewed = self.dialogs.preview.as_ref().map(|v| v.page);
    }

    /// What the panel's header shows.
    pub(crate) fn header(&mut self) -> Header {
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
                    snapshots: self.files.snapshots.is_some(),
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
            None => Header { note: NOTE, snapshots: self.files.snapshots.is_some(), ..Header::default() },
        }
    }

    /// The branch facts, read again only after a switch or a branch action.
    pub fn branch_info(&mut self) -> Option<BranchInfo> {
        let key = (self.seen.project_serial, self.branch.rev);
        if let Some((k, v)) = &self.branch.info
            && *k == key
        {
            return v.clone();
        }
        let p = self.project.clone()?;
        let info = p
            .read(&mut self.cad.session, |p, _| {
                use ecad_engine::history;
                let file = p.file_status().display_name;
                let Some(v) = p.storage.file.project() else {
                    return BranchInfo { name: "main".into(), count: 1, bar: None, parent: None, release_line: true, file };
                };
                let rec = v.branch_record().clone();
                let rows = history::branch_rows(p).unwrap_or_default();
                let parent = rec.parent;
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
        self.branch.info = Some((key, info.clone()));
        info
    }

    /// The one refresh path for branch actions, clicked or typed: when the
    /// project's branch-action count or the branch shown moved, the title, branch bar, Revisions
    /// and History are read again; after a switch, every copy rebuilds once as on open and a
    /// preview closes. One integer and one ID compared per frame otherwise.
    pub fn sync_branch(&mut self) {
        let Some(p) = self.project.clone() else { return };
        let Ok(now) = p.read(&mut self.cad.session, |p, _| (p.branch_gen(), p.storage.file.project().map(|v| v.branch().hex()))) else { return };
        let Some(seen) = self.branch.seen.replace(now.clone()) else { return };
        if seen == now {
            return;
        }
        self.branch.rev += 1;
        self.panel.history.invalidate();
        if seen.1 != now.1 {
            self.seen.project_serial += 1;
            self.dialogs.preview = None;
            self.panel.devices.invalidate();
            self.panel.checks.invalidate();
            self.panel.history.selected = None;
        }
        if self.dialogs.revisions.is_some() {
            self.open_revisions();
        }
    }

    /// Keep the window title in step with the branch shown; sent only when it
    /// changes.
    pub(crate) fn sync_title(&mut self, ctx: &egui::Context) {
        let info = if self.project_open() { self.branch_info() } else { None };
        let title = window_title(info.as_ref());
        if title != self.seen.title_sent {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.seen.title_sent = title;
        }
    }
}
