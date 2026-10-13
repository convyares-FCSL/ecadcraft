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
//! - the read-only **debugger** panel (persistence stage 2b, [`debugger`]), opened from the
//!   banner and never shown by default: selected drawing objects → ECAD IDs → model objects →
//!   transactions with before/after values → saved record offsets and bytes, with pending
//!   changes kept apart from durably saved history.
//!
//! Excluded on purpose: native file dialogs, CADCraft file formats (plain drawings cannot be
//! written), autosave, the control channel, saved window or profile state, ECAD menus and
//! interactive ECAD prompts. ECAD commands are typed in their JSON form
//! in the banner's command field, for example `wire {"points":[[0,0],[40,0]]}` (CADCraft's own
//! command line takes a space as Enter, so it cannot take a JSON argument).
//!
//! The supported place for project files under WSL is the Linux home folder (for example
//! `~/ecad-projects`); other file systems, including Windows drives under `/mnt`, are unverified.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod debugger;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use cadcraft_engine::Session;
use cadcraft_ui_egui::closing::{Choice, Closing};
use cadcraft_ui_egui::{CadApp, Services};
use debugger::Debugger;
use ecad_cadcraft::{FileStatus, Persistence, Project, ProjectHandle, session_seed};
use ecad_inspect::EventKind;
use eframe::egui::{self, RichText};
use serde_json::json;

/// Window title.
pub const TITLE: &str = "ECAD trial (experiment)";
/// The banner shown above CADCraft's window contents.
pub const BANNER: &str = "ECAD TRIAL (experiment). Projects are saved as .ecad files. Keep them in your Linux home \
folder (for example ~/ecad-projects); other locations are unverified.";
/// Said wherever snapshots are mentioned.
pub const SNAPSHOT_NOTE: &str = "Inspector snapshots are diagnostic files for the developer inspector, not saved ECAD projects; they cannot be reopened.";
/// Name of a new project until it is saved.
pub const PROJECT_NAME: &str = "Untitled";
/// Where the Save As dialog suggests saving.
pub const DEFAULT_FOLDER: &str = "~/ecad-projects";

/// Lines echoed at the command line when a project is created.
const INTRO: &[&str] = &[
    "ECAD trial (experiment): Save As writes the project to a .ecad file; Open (in the banner) opens one.",
    "Type ECAD commands with JSON parameters in the banner's \"Command\" field (the command line below takes a \
space as Enter), for example:  wire {\"points\":[[0,0],[40,0]]}",
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
    /// The read-only debugger panel (closed until opened from the banner).
    pub debugger: Debugger,
}

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
    }
}

/// `~/` expanded, for checking folders.
fn expand(path: &str) -> PathBuf {
    match (path.strip_prefix("~/"), std::env::var("HOME")) {
        (Some(rest), Ok(home)) if !home.is_empty() => Path::new(&home).join(rest),
        _ => PathBuf::from(path),
    }
}

impl Trial {
    /// A harness with one new ECAD project. `snapshots`: where exported snapshots go (none when
    /// `None`).
    ///
    /// # Errors
    /// The project could not be created.
    pub fn new(snapshots: Option<PathBuf>) -> Result<Self, String> {
        let mut t = Self::empty(snapshots);
        t.new_project()?;
        Ok(t)
    }

    /// A harness opening the project file at `path`. If it cannot be opened, a new project is
    /// created instead and the reason is shown.
    ///
    /// # Errors
    /// Neither the file nor a new project could be opened.
    pub fn with_file(snapshots: Option<PathBuf>, path: &str) -> Result<Self, String> {
        let mut t = Self::empty(snapshots);
        if let Err(e) = t.open_project(path) {
            t.new_project()?;
            t.notice = Some(format!("The project file could not be opened: {e}"));
        }
        Ok(t)
    }

    fn empty(snapshots: Option<PathBuf>) -> Self {
        let requested = Arc::new(AtomicBool::new(false));
        Trial {
            cad: CadApp::new(Session::empty(), services(Arc::clone(&requested))),
            project: None,
            snapshots,
            held: None,
            dialog: None,
            save_as_requested: requested,
            seen_events: 0,
            notice: None,
            snapshot_count: 0,
            command: String::new(),
            debugger: Debugger::default(),
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
        let (p, summary) = ProjectHandle::open_file(&mut self.cad.session, path, session_seed())?;
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

    /// Save the project as `path` (a `.ecad` file; the extension is added when missing). The
    /// default folder is created if it does not exist yet.
    ///
    /// # Errors
    /// No project is open, or the save failed (also shown in a dialog).
    pub fn save_as(&mut self, path: &str) -> Result<(), String> {
        self.focus_project()?;
        let target = expand(path.trim());
        if let Some(dir) = target.parent()
            && dir == expand(DEFAULT_FOLDER)
            && !dir.exists()
        {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let r = self.cad.run("saveas", json!({ "path": path.trim() })).map(|_| ());
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
                s.file.path.unwrap_or_else(|| format!("{DEFAULT_FOLDER}/{}.ecad", s.name.to_lowercase()))
            }
            _ => format!("{DEFAULT_FOLDER}/"),
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
                let target = ecad_cadcraft::normalize(&path).map(|p| expand(&p));
                let own = self.status().and_then(|s| s.file.path).map(|p| expand(&p));
                match target {
                    Ok(t)
                        if t.exists() && Some(&t) != own.as_ref() && d.confirmed_replace.as_deref() != Some(&path) =>
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
    pub fn submit_command(&mut self, text: &str) -> Option<String> {
        let running = self.cad.session.running.is_some();
        let text = if running { text } else { text.trim_start() };
        if text.is_empty() && !running {
            return None;
        }
        self.cad.session.echo(format!("Command: {text}"));
        let r = self.cad.cmdline(text);
        self.after_logic();
        r
    }

    /// Write a live inspector snapshot to the snapshot folder. Returns the file written.
    ///
    /// # Errors
    /// Snapshots are not enabled, there is no project, or writing failed.
    pub fn export_snapshot(&mut self, why: &str) -> Result<PathBuf, String> {
        let dir = self.snapshots.clone().ok_or("snapshots are not enabled (start with --snapshots <dir>)")?;
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
                    let plural = |n: u64, w: &str| format!("{n} {w}{}", if n == 1 { "" } else { "s" });
                    let file =
                        s.file.path.as_deref().map_or("not saved to a file yet".to_string(), |p| format!("file {p}"));
                    let changes = if s.unsaved == 0 {
                        "no unsaved changes".to_string()
                    } else {
                        format!("{} not saved", plural(s.unsaved, "change"))
                    };
                    ui.label(format!(
                        "Project \"{}\": {}, {file}, {changes}.",
                        s.name,
                        plural(s.pages as u64, "page") + " open"
                    ));
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
                }
            });
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
            egui::Modal::new(egui::Id::new("ecad_trial_notice")).show(ctx, |ui| {
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
        egui::Modal::new(egui::Id::new("ecad_trial_close")).show(ctx, |ui| {
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
        egui::Modal::new(egui::Id::new("ecad_trial_file")).show(ctx, |ui| {
            ui.set_width(520.0);
            ui.label(RichText::new(heading).strong());
            ui.add_space(4.0);
            ui.label(format!(
                "A .ecad project file (the extension is added if missing). Keep projects in your Linux home folder, \
for example {DEFAULT_FOLDER}/demo.ecad; other locations are unverified."
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

/// Seconds since the epoch, for unique snapshot file names.
fn ecad_stamp() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}
