//! ecad-trial: the smallest runnable ECAD desktop harness, for a manual trial of the bounded
//! integration experiment.
//!
//! It is CADCraft's own window ([`CadApp`]) with the ECAD commands installed and one ECAD project
//! open. Everything ECAD-specific runs through `ecad-cadcraft`; this crate only adds what a person
//! needs to try it safely:
//!
//! - a banner that labels the window as an experiment with **no project persistence**;
//! - Save and Save As reach the project, which reports that persistence is unavailable (nothing is
//!   written, nothing is marked saved); the harness shows that report in a dialog;
//! - closing an edited project asks **Cancel** or **Discard** and says the design will be lost
//!   (it replaces CADCraft's "Save changes?" prompt for ECAD pages, through `CadApp`'s public
//!   close API; no upstream change);
//! - optional export of inspector snapshots (`--snapshots <dir>`), which are diagnostic files, not
//!   saved projects.
//!
//! Excluded on purpose: file open and save dialogs, CADCraft file formats (no I/O hooks are
//! installed, so plain drawings cannot be written either), autosave, the control channel, saved
//! window or profile state, ECAD menus and interactive ECAD prompts. ECAD commands are typed in
//! their JSON form in the banner's command field, for example `wire {"points":[[0,0],[40,0]]}`
//! (CADCraft's own command line takes a space as Enter, so it cannot take a JSON argument).
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use std::path::{Path, PathBuf};

use cadcraft_engine::Session;
use cadcraft_ui_egui::closing::{Choice, Closing};
use cadcraft_ui_egui::{CadApp, Services};
use ecad_cadcraft::{Persistence, Project, ProjectHandle, SAVE_UNAVAILABLE, session_seed};
use ecad_inspect::EventKind;
use eframe::egui::{self, RichText};

/// Window title.
pub const TITLE: &str = "ECAD trial (experiment) - no project persistence";
/// The banner shown above CADCraft's window contents.
pub const BANNER: &str = "ECAD TRIAL (experiment). No project persistence: Save and Save As only report that saving \
is unavailable, and closing an edited project discards its design.";
/// Said wherever snapshots are mentioned.
pub const SNAPSHOT_NOTE: &str = "Inspector snapshots are diagnostic files for the developer inspector, not saved ECAD projects; they cannot be reopened.";
/// Name of the project the harness creates.
pub const PROJECT_NAME: &str = "Trial";

/// Lines echoed at the command line when a project is created.
const INTRO: &[&str] = &[
    "ECAD trial (experiment): this project cannot be saved.",
    "Type ECAD commands with JSON parameters in the banner's \"Command\" field (the command line below takes a \
space as Enter), for example:  wire {\"points\":[[0,0],[40,0]]}",
    "ECAD commands: convertcomponent, addterminal, wire, connect, disconnect, ecaddelete, setprop, cableadd, place, \
newpage, openpage, ecadinspect. CADCraft drawing commands work as usual.",
];

/// The user's answer to the close dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    Cancel,
    Discard,
}

/// A close of an edited ECAD project, held until the user answers Cancel or Discard.
struct Held {
    /// CADCraft's close request, taken out of `CadApp` so its "Save changes?" prompt never shows.
    closing: Closing,
    title: String,
    changes: u64,
}

/// The harness state around CADCraft's [`CadApp`].
pub struct Trial {
    pub cad: CadApp,
    project: Option<ProjectHandle>,
    snapshots: Option<PathBuf>,
    held: Option<Held>,
    /// SaveUnavailable events already reported.
    saves_reported: usize,
    /// A message waiting in a dialog (save unavailable, snapshot written or failed).
    notice: Option<String>,
    snapshot_count: u32,
    /// The banner's command field (whole lines, spaces included).
    pub command: String,
}

/// File dialogs are not part of the trial. Save As on a drawing gets an empty path, so the save
/// reaches the project (which reports that persistence is unavailable) instead of silently doing
/// nothing; an empty path can never be written. Exports get no path and stop.
pub fn services() -> Services {
    Services {
        pick_open: None,
        pick_save: Some(Box::new(|name: &str| {
            let ext = Path::new(name).extension().map(|e| e.to_string_lossy().to_ascii_lowercase());
            matches!(ext.as_deref(), None | Some("dwg" | "dxf" | "dwt")).then(String::new)
        })),
    }
}

impl Trial {
    /// A harness with one new ECAD project. `snapshots`: where exported snapshots go (none when
    /// `None`).
    ///
    /// # Errors
    /// The project could not be created.
    pub fn new(snapshots: Option<PathBuf>) -> Result<Self, String> {
        let mut t = Trial {
            cad: CadApp::new(Session::empty(), services()),
            project: None,
            snapshots,
            held: None,
            saves_reported: 0,
            notice: None,
            snapshot_count: 0,
            command: String::new(),
        };
        t.new_project()?;
        Ok(t)
    }

    /// Create a new project (only when none is open).
    ///
    /// # Errors
    /// A project is already open, or creating it failed.
    pub fn new_project(&mut self) -> Result<(), String> {
        if self.project_open() {
            return Err("an ECAD project is already open".into());
        }
        let p = Project::create_with(&mut self.cad.session, PROJECT_NAME, session_seed(), Persistence::Unavailable)?;
        self.project = Some(p);
        self.saves_reported = 0;
        self.cad.ui.start_tab = false;
        for line in INTRO {
            self.cad.session.echo(*line);
        }
        Ok(())
    }

    /// The current project (open or already closed).
    pub fn project(&self) -> Option<&ProjectHandle> {
        self.project.as_ref()
    }

    /// Whether the project has a page open.
    pub fn project_open(&self) -> bool {
        self.cad.session.docs.iter().any(|d| self.cad.session.ext.owner_of(d.uid).is_some())
    }

    /// Title of the project close waiting for Cancel or Discard.
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

    /// (pages open, unsaved transactions) of the project, when one is open.
    pub fn status(&mut self) -> Option<(usize, u64)> {
        if !self.project_open() {
            return None;
        }
        let p = self.project.as_ref()?;
        p.read(&mut self.cad.session, |p, _| (p.open_pages(), p.unsaved_transactions())).ok()
    }

    /// Per-frame work after CADCraft's own logic: hold a close of an edited ECAD project for the
    /// Cancel/Discard dialog, and report any save attempt.
    pub fn after_logic(&mut self) {
        self.hold_close();
        self.report_saves();
    }

    fn hold_close(&mut self) {
        if self.held.is_some() {
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
        let changes = self.status().map_or(0, |(_, n)| n);
        self.held = Some(Held { closing, title, changes });
    }

    fn report_saves(&mut self) {
        let Some(p) = self.project.as_ref() else { return };
        let Ok(n) = p.read(&mut self.cad.session, |p, _| p.events.count(EventKind::SaveUnavailable)) else { return };
        if n > self.saves_reported {
            self.saves_reported = n;
            self.notice = Some(SAVE_UNAVAILABLE.into());
        }
    }

    /// Answer the close dialog. Discard first exports a snapshot (when enabled), then lets CADCraft
    /// close the page and carry on with any further closes or the quit it was doing.
    pub fn answer(&mut self, a: Answer) {
        let Some(held) = self.held.take() else { return };
        match a {
            Answer::Cancel => self.cad.session.echo("Close cancelled; the ECAD project stays open."),
            Answer::Discard => {
                if self.snapshots.is_some()
                    && let Err(e) = self.export_snapshot("discarded")
                {
                    self.cad.session.echo(format!("Snapshot not written: {e}"));
                }
                self.cad.closing = Some(held.closing);
                self.cad.answer_close(Choice::Discard);
                if !self.project_open() {
                    self.cad.session.echo("The ECAD project was discarded; its design is gone.");
                }
            }
        }
    }

    /// Run one whole command line, as typed in the banner's field: `name {json}` runs an ECAD or
    /// CADCraft command with parameters; anything else is read like a script line (spaces separate
    /// inputs). Returns the error, which is also echoed at the command line.
    pub fn submit_command(&mut self, text: &str) -> Option<String> {
        let text = text.trim();
        if text.is_empty() {
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

    /// Lay out the banner, the harness dialogs and CADCraft's window contents.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.banner(ui);
        self.dialogs(ui.ctx());
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
        let mut new_project = false;
        let mut export = false;
        let mut submit: Option<String> = None;
        egui::Panel::top("ecad_trial_banner").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(BANNER).strong().color(warn));
                match status {
                    Some((pages, changes)) => {
                        let s = if pages == 1 { "" } else { "s" };
                        ui.label(format!(
                            "Project \"{PROJECT_NAME}\": {pages} page{s} open, {changes} change{} that cannot be saved.",
                            if changes == 1 { "" } else { "s" }
                        ));
                    }
                    None => {
                        ui.label("No ECAD project is open.");
                        new_project = ui.button("New trial project").clicked();
                    }
                }
                if self.snapshots.is_some() && status.is_some() {
                    export = ui.button("Export inspector snapshot").on_hover_text(SNAPSHOT_NOTE).clicked();
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
                if (ui.button("Run").clicked() || enter) && !self.command.trim().is_empty() {
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
        if new_project && let Err(e) = self.new_project() {
            self.notice = Some(e);
        }
        if export {
            self.notice = Some(match self.export_snapshot("manual") {
                Ok(p) => format!("Snapshot written to {}.\n\n{SNAPSHOT_NOTE}", p.display()),
                Err(e) => format!("Snapshot not written: {e}"),
            });
        }
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        if let Some(h) = &self.held {
            let mut answer = None;
            let (title, changes) = (h.title.clone(), h.changes);
            egui::Modal::new(egui::Id::new("ecad_trial_close")).show(ctx, |ui| {
                ui.set_width(420.0);
                ui.label(RichText::new("Close the ECAD trial project?").strong());
                ui.add_space(4.0);
                ui.label(format!(
                    "\"{title}\" is the last open page of project \"{PROJECT_NAME}\", which has {changes} recorded \
change{}.",
                    if changes == 1 { "" } else { "s" }
                ));
                ui.label(
                    RichText::new("This trial cannot save projects. If you close it, its design will be lost.")
                        .color(ui.visuals().warn_fg_color),
                );
                if self.snapshots.is_some() {
                    ui.label(format!("A snapshot is exported first. {SNAPSHOT_NOTE}"));
                }
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        answer = Some(Answer::Cancel);
                    }
                    if ui.button("Discard").clicked() {
                        answer = Some(Answer::Discard);
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
        } else if let Some(msg) = self.notice.clone() {
            let mut ok = false;
            egui::Modal::new(egui::Id::new("ecad_trial_notice")).show(ctx, |ui| {
                ui.set_width(420.0);
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
}

/// Seconds since the epoch, for unique snapshot file names.
fn ecad_stamp() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}
