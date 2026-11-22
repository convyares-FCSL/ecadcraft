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
        let path = self.resolve(path).and_then(|p| ecad_engine::normalize_resolved(&p).map_err(|e| e.to_string()))?;
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
        // user has chosen Show/Hide Lineweight since the app started (brief 009 review).
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
    /// [`App::home`]). The default folder is created if it does not exist yet; no other folder is.
    ///
    /// # Errors
    /// No project is open, the path is refused, or the save failed (also shown in a dialog).
    pub fn save_as(&mut self, path: &str) -> Result<(), String> {
        self.focus_project()?;
        let target = self.resolve(path).and_then(|p| ecad_engine::normalize_resolved(&p).map_err(|e| e.to_string()))?;
        // Without a known home the default folder cannot be made; the save goes on and reports what it finds.
        if let Ok(default) = self.resolve(&crate::shell::host::host().default_folder) {
            ecad_engine::ensure_default_folder(std::path::Path::new(&target), &default).map_err(|e| format!("{target}: {e}"))?;
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

    /// Title of the project close waiting for an answer.
    pub fn pending_close(&self) -> Option<&str> {
        self.held.as_ref().map(|h| h.title.as_str())
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
            (DialogKind::SaveAs, Some(s)) => s.file.path.unwrap_or_else(|| self.in_default_folder(&format!("{}.ecad", s.name.to_lowercase()))),
            _ => self.in_default_folder(""),
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
                let target = self.resolve(&path).and_then(|p| ecad_engine::normalize_resolved(&p).map_err(|e| e.to_string())).map(PathBuf::from);
                let own = self.status().and_then(|s| s.file.path).map(PathBuf::from);
                match target {
                    Ok(t)
                        if ecad_engine::save_target(&t, own.as_deref()) == ecad_engine::SaveTarget::Exists
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

    pub(crate) fn hold_close(&mut self) {
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

    /// Show a failed save in a dialog and a successful one at the command line.
    pub(crate) fn report_saves(&mut self) {
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

    /// Write a live inspector snapshot to the snapshot folder. Returns the file written.
    ///
    /// # Errors
    /// Snapshots are not enabled, there is no project, or writing failed.
    pub fn export_snapshot(&mut self, why: &str) -> Result<PathBuf, String> {
        let dir = self.snapshots.clone().ok_or("snapshots are not enabled (start with --snapshots <dir>)")?;
        // PowerShell and cmd pass `~` through unexpanded: it is the home folder, never a `~` folder.
        let dir = match dir.to_str() {
            Some(typed) => self.resolve(typed)?,
            None => dir,
        };
        let p = self.project.as_ref().ok_or("there is no project")?;
        let snap = p.snapshot(&mut self.cad.session, None)?;
        self.snapshot_count += 1;
        let path =
            ecad_engine::dev::snapshot::write_snapshot_file(&snap, &dir, why, self.snapshot_count, (crate::shell::host::host().now_ms)() / 1000)
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
    pub(crate) fn sync_keep_open(&mut self) {
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
        self.resolve(path.trim()).map(|p| p.to_string_lossy().into_owned())
    }

    /// A typed path as a path: `~` is [`App::home`].
    ///
    /// # Errors
    /// The path starts with `~` and the home folder is not known.
    pub(crate) fn resolve(&self, typed: &str) -> Result<PathBuf, String> {
        crate::shell::host::resolve(typed, self.home.as_deref())
    }
}
