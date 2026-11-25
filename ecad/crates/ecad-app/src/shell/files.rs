//! The app's project files: new, open, save, save as, the file dialog's state, conversion of old
//! files, snapshots, and closing (the held close, quitting).

use crate::shell::app::*;
use cadcraft_ui_egui::closing::{Choice, Closing};
use ecad_engine::history::event_log::EventKind;
use ecad_engine::{Persistence, Project, ProjectHandle, session_seed};
use serde_json::json;
use std::path::PathBuf;

impl App {
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
        let path = self.resolve_ecad(path).map_err(|e| e.to_string())?;
        let (p, summary) = ProjectHandle::open_file(&mut self.cad.session, &path, session_seed())?;
        self.attach(p);
        for line in summary.lines() {
            self.cad.session.echo(line.to_string());
        }
        if summary.lines().count() > 1 {
            self.dialogs.notice = Some(summary.clone());
        }
        Ok(summary)
    }

    fn attach(&mut self, p: ProjectHandle) {
        self.project = Some(p);
        self.seen.project_serial += 1;
        self.branch.seen = None;
        self.files.keep_open = None;
        self.panel.clear();
        self.dialogs.preview = None;
        self.sync_keep_open();
        self.debugger.reset();
        self.seen.events = self.event_count();
        self.cad.ui.start_tab = false;
        // A project always opens on Draw.
        self.shell.state.workspace = crate::shell::Workspace::Draw;
        self.shell.settings_open = false;
        // Thick and thin cable lines only show with lineweights on, unless the
        // user has chosen Show/Hide Lineweight since the app started.
        let shown = &mut self.cad.session.settings.lwdisplay;
        if self.seen.lwdisplay_set.is_none_or(|set| *shown == set) {
            *shown = true;
            self.seen.lwdisplay_set = Some(true);
        }
        // Symbols snap to the 5 mm terminal grid of ECAD pages;
        // F9 turns it off as in CADCraft.
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
    /// the home folder ([`App::files`])). The default folder is created if it does not exist yet; no other folder is.
    ///
    /// # Errors
    /// No project is open, the path is refused, or the save failed (also shown in a dialog).
    pub fn save_as(&mut self, path: &str) -> Result<(), String> {
        self.save_as_checked(path, true).map_err(|e| e.to_string())
    }

    /// [`App::save_as`] that refuses to replace another file unless `replace` is set, in the
    /// same call as the save.
    ///
    /// # Errors
    /// As [`App::save_as`]; [`ecad_engine::file::SaveAsRefused::Exists`] when another file is there.
    pub(crate) fn save_as_checked(&mut self, path: &str, replace: bool) -> Result<(), ecad_engine::file::SaveAsError> {
        use ecad_engine::file::SaveAsError;
        self.focus_project().map_err(SaveAsError::Failed)?;
        let target = self.resolve_ecad(path).map_err(|e| SaveAsError::Failed(e.to_string()))?;
        // Without a known home the default folder cannot be made; the save goes on and reports what it finds.
        if let Ok(default) = self.resolve(&crate::shell::host::host().default_folder) {
            ecad_engine::ensure_default_folder(std::path::Path::new(&target), &default).map_err(|e| SaveAsError::Failed(format!("{target}: {e}")))?;
        }
        let own = self.status().and_then(|s| s.file.path).map(PathBuf::from);
        let r = ecad_engine::save_project_as(&mut self.cad.session, std::path::Path::new(&target), own.as_deref(), replace);
        match &r {
            Ok(v) => {
                if let Some(m) = v.get("message").and_then(serde_json::Value::as_str) {
                    for l in m.lines() {
                        self.cad.session.echo(l.to_string());
                    }
                }
            }
            Err(e) => self.cad.session.echo(e.to_string()),
        }
        self.after_logic();
        r.map(|_| ())
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
        // The page window was closed: show the page again first.
        self.show_page_window().map(|_| ())
    }

    /// Open the page window again, on the page shown last (else the first page). Returns its uid.
    fn show_page_window(&mut self) -> Result<u64, String> {
        let p = self.project.clone().ok_or("no ECAD project is open")?;
        let page = p
            .read(&mut self.cad.session, |p, _| p.active_page().or(self.panel.active).or_else(|| p.ledger.document().page_order().first().copied()))?
            .ok_or("the project has no pages")?;
        p.open_page(&mut self.cad.session, page)
    }

    /// Title of the project close waiting for an answer.
    pub fn pending_close(&self) -> Option<&str> {
        self.files.held.as_ref().map(|h| h.title.as_str())
    }

    /// The open file dialog: its kind and the path typed so far.
    pub fn dialog(&self) -> Option<(DialogKind, &str, Option<&str>)> {
        self.files.dialog.as_ref().map(|d| (d.kind, d.path.as_str(), d.error.as_deref()))
    }

    /// Open the Save As dialog, suggesting a name in the default folder (or the current file).
    pub fn request_save_as(&mut self) {
        self.open_dialog(DialogKind::SaveAs, None);
    }

    /// New or Open from the menus: an open project is closed first, as **Close** does (unsaved
    /// changes are asked about), and `then` follows once it has closed; cancelling the close
    /// keeps the project and drops `then`.
    pub fn close_then(&mut self, then: AfterClose) {
        if !self.project_open() {
            self.files.after_close = None;
            self.after_close(then);
            return;
        }
        self.files.after_close = Some(then);
        self.close_project();
        self.follow_close();
    }

    /// Carry out [`App::close_then`]'s step once the project has closed, or drop it when the
    /// close ended with the project still open (cancelled, or a save that failed and was left).
    pub(crate) fn follow_close(&mut self) {
        let Some(then) = self.files.after_close else { return };
        if !self.project_open() {
            self.files.after_close = None;
            self.after_close(then);
            return;
        }
        let asking = self.files.held.is_some()
            || self.files.dialog.as_ref().is_some_and(|d| d.then_close.is_some())
            || self.files.confirming_close
            || self.cad.closing.is_some();
        if !asking {
            self.files.after_close = None;
        }
    }

    fn after_close(&mut self, then: AfterClose) {
        match then {
            AfterClose::New => {
                if let Err(e) = self.new_project() {
                    self.dialogs.notice = Some(e);
                }
            }
            AfterClose::Open => self.request_open(),
        }
    }

    /// Open the Open dialog.
    pub fn request_open(&mut self) {
        self.open_dialog(DialogKind::Open, None);
    }

    fn open_dialog(&mut self, kind: DialogKind, then_close: Option<Closing>) {
        let suggested = match (kind, self.status()) {
            (DialogKind::SaveAs, Some(s)) => s.file.path.unwrap_or_else(|| self.in_default_folder(&format!("{}.ecad", s.name.to_lowercase()))),
            _ => self.in_default_folder(""),
        };
        self.files.dialog = Some(FileDialog { kind, path: suggested, error: None, then_close, confirmed_replace: None, focused: false });
    }

    /// Type a path into the open file dialog.
    pub fn set_dialog_path(&mut self, path: &str) {
        if let Some(d) = &mut self.files.dialog {
            d.path = path.to_string();
        }
    }

    /// Cancel the open file dialog (a close waiting for it is cancelled too).
    pub fn cancel_dialog(&mut self) {
        if self.files.dialog.take().is_some_and(|d| d.then_close.is_some()) {
            self.cad.session.echo("Close cancelled; the ECAD project stays open.");
        }
        self.sync_keep_open();
    }

    /// Confirm the open file dialog: open, or save as, the path typed. On failure the dialog
    /// stays open with the reason. Saving over an existing file other than the project's own asks
    /// once more first.
    pub fn confirm_dialog(&mut self) {
        let Some(mut d) = self.files.dialog.take() else { return };
        self.files.confirming_close = d.then_close.is_some();
        let path = d.path.trim().to_string();
        let r = match d.kind {
            DialogKind::Open => self.open_project(&path).map(|_| ()),
            DialogKind::SaveAs => {
                let replace = d.confirmed_replace.as_deref() == Some(&path);
                match self.save_as_checked(&path, replace) {
                    Err(ecad_engine::file::SaveAsError::Refused(ecad_engine::file::SaveAsRefused::Exists(t))) => {
                        d.confirmed_replace = Some(path.clone());
                        Err(format!("{} already exists. Press Save again to replace it.", t.display()))
                    }
                    other => other.map_err(|e| e.to_string()),
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
                self.dialogs.notice = None;
                d.error = Some(e);
                d.focused = false;
                self.files.dialog = Some(d);
            }
        }
        self.files.confirming_close = false;
        self.sync_keep_open();
    }

    pub(crate) fn hold_close(&mut self) {
        if self.files.held.is_some() || self.files.dialog.as_ref().is_some_and(|d| d.then_close.is_some()) {
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
        self.files.held = Some(Held { closing, title, changes });
    }

    /// Show a failed save in a dialog and a successful one at the command line.
    pub(crate) fn report_saves(&mut self) {
        let Some(p) = self.project.as_ref() else { return };
        let seen = self.seen.events;
        let Ok(new) = p.read(&mut self.cad.session, |p, _| {
            p.events.events().filter(|e| e.n >= seen).map(|e| (e.n, e.kind, e.message.clone())).collect::<Vec<_>>()
        }) else {
            return;
        };
        for (n, kind, message) in new {
            self.seen.events = n + 1;
            match kind {
                EventKind::Saved => self.cad.session.echo(format!("Saved: {message}")),
                EventKind::SaveFailed => {
                    self.cad.session.echo(format!("Not saved: {message}"));
                    if self.files.dialog.is_none() {
                        self.dialogs.notice = Some(format!("The project was not saved.\n\n{message}"));
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
        let Some(held) = self.files.held.take() else { return };
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
                if self.files.snapshots.is_some()
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

    /// Write a live inspector snapshot to the snapshot folder. Returns the file written.
    ///
    /// # Errors
    /// Snapshots are not enabled, there is no project, or writing failed.
    pub fn export_snapshot(&mut self, why: &str) -> Result<PathBuf, String> {
        let dir = self.files.snapshots.clone().ok_or("snapshots are not enabled (start with --snapshots <dir>)")?;
        // PowerShell and cmd pass `~` through unexpanded: it is the home folder, never a `~` folder.
        let dir = match dir.to_str() {
            Some(typed) => self.resolve(typed).map_err(|e| e.to_string())?,
            None => dir,
        };
        let p = self.project.as_ref().ok_or("there is no project")?;
        let snap = p.snapshot(&mut self.cad.session, None)?;
        self.files.snapshot_count += 1;
        let path = ecad_engine::dev::snapshot::write_snapshot_file(
            &snap,
            &dir,
            why,
            self.files.snapshot_count,
            (crate::shell::host::host().now_ms)() / 1000,
        )
        .map_err(|e| format!("{}: {e}", dir.display()))?;
        self.cad.session.echo(format!("Snapshot written: {} ({SNAPSHOT_NOTE})", path.display()));
        Ok(path)
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

    /// Close the app as the window's close button does: [`App::prepare_quit`], then CADCraft's
    /// quit (which asks about unsaved changes). Returns whether the close has to wait.
    pub fn request_quit(&mut self) -> bool {
        self.prepare_quit();
        let wait = self.cad.request_quit();
        self.after_logic();
        wait
    }

    /// Give the project `keep_open` (only when it differs from what it was given last).
    fn set_keep_open(&mut self, on: bool) {
        if self.files.keep_open == Some(on) {
            return;
        }
        if let Some(p) = &self.project
            && p.is_attached(&self.cad.session)
            && p.configure(&mut self.cad.session, |p| p.display.keep_open = on).is_ok()
        {
            self.files.keep_open = Some(on);
        }
    }

    /// `keep_open` is on except while a close of the project or a quit is being asked about.
    pub(crate) fn sync_keep_open(&mut self) {
        let closing = self.files.confirming_close
            || self.cad.closing.is_some()
            || self.files.held.is_some()
            || self.files.dialog.as_ref().is_some_and(|d| d.then_close.is_some());
        if !closing {
            self.set_keep_open(true);
        }
    }

    /// Close the project, as the Project panel's **Close** does: unsaved changes are asked about
    /// (Save, Discard, Cancel).
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

    /// The project file's path without `.ecad`, or its name, for default output names.
    pub(crate) fn report_stem(&mut self) -> String {
        let Some(p) = self.project.clone() else { return "project".into() };
        p.read(&mut self.cad.session, |p, _| match &p.path {
            Some(path) => ecad_engine::without_extension(std::path::Path::new(path)),
            None => p.name.clone(),
        })
        .unwrap_or_else(|_| "project".into())
    }

    /// `name` in the default folder, as typed (with the platform's separator; a trailing one when `name` is empty).
    pub(crate) fn in_default_folder(&self, name: &str) -> String {
        format!("{}{}{name}", crate::shell::host::host().default_folder, std::path::MAIN_SEPARATOR)
    }

    /// A typed output path with `~` expanded.
    pub(crate) fn report_path(&self, path: &str) -> Result<String, String> {
        self.resolve(path.trim()).map(|p| p.to_string_lossy().into_owned()).map_err(|e| e.to_string())
    }

    /// A typed path as a path: `~` is the home folder ([`App::files`]).
    ///
    /// # Errors
    /// The path starts with `~` and the home folder is not known.
    pub(crate) fn resolve(&self, typed: &str) -> Result<PathBuf, ecad_file::binding::PathError> {
        crate::shell::host::resolve(typed, self.files.home.as_deref())
    }

    /// A typed project path as the `.ecad` file it names: `~` resolved, `.ecad` added.
    ///
    /// # Errors
    /// As [`App::resolve`], or the path is not a file name.
    pub(crate) fn resolve_ecad(&self, typed: &str) -> Result<String, ecad_file::binding::PathError> {
        self.resolve(typed).and_then(|p| ecad_engine::normalize_resolved(&p))
    }
}
