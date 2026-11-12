//! The Release dialog (brief 027, design 027 §5.1, §8, §9): the pre-release check of the state to
//! be issued, the pending revision's fields, the folder and what will be written, and the question
//! when the folder exists. Non-modal: the user can fix things while it is open, and the check
//! follows (the dialog's copy is taken again only when the kept Checks or the project change).
//!
//! Everything it does goes through the commands' JSON forms: `revision` for a field, `release`
//! to release; the engine's [`ecad_cadcraft::release::View`] is what it shows.

use std::collections::BTreeSet;

use cadcraft_ui_egui::theme::Tokens;
use ecad_cadcraft::checks::{Check, Severity};
use ecad_cadcraft::release::{Group, View};
use eframe::egui::{self, Color32, RichText};
use serde_json::{Map, Value, json};

/// The window's id and accessible name.
pub const RELEASE_ID: &str = "ecad.release";
/// Rows shown per group before `Show all N in Checks`.
pub const ROWS_SHOWN: usize = 5;
/// The clean check's words.
pub const NOTHING_OPEN: &str = "Nothing open. Ready to release.";

/// What the dialog asks for.
#[derive(Debug, Clone, PartialEq)]
pub enum Out {
    /// Run this command line (a pending field: `revision {…}`).
    Run(String),
    /// `release` with these parameters.
    Release(Value),
    /// Go to this row (the dialog stays open).
    GoTo(Box<Check>),
    /// Reports, BOM tab, Only ? (the `?` parts group).
    ShowReports,
    /// The Checks tab (`Show all N in Checks`).
    ShowChecks,
    /// Save As… (a project never saved).
    SaveAs,
    /// Write the last issued revision's folder again (nothing changed since it).
    WriteIssued(String),
    Close,
}

/// The dialog's state.
#[derive(Debug, Clone, Default)]
pub struct ReleaseDialog {
    /// `None`: release the pending revision; `Some(rev)`: write that issued revision's folder.
    pub rev: Option<String>,
    /// The engine's view, taken again when [`ReleaseDialog::key`] changes.
    pub view: Option<View>,
    /// (head, Checks revision, report models revision, accept) the view was taken at.
    pub key: Option<(u64, u64, u64, bool)>,
    /// `Release with 1 error` is ticked.
    pub accept: bool,
    /// The folder typed or browsed to; empty: the default.
    pub folder: String,
    /// The pending fields as typed, and as last sent.
    pub fields: [String; 6],
    orig: [String; 6],
    /// Groups collapsed (Other starts collapsed).
    pub collapsed: BTreeSet<Group>,
    pub error: Option<String>,
    /// The existing-folder question (the refusal's words), while it is asked.
    pub exists: Option<String>,
    /// The project is read-only: the pending revision cannot be released.
    pub read_only: bool,
}

impl ReleaseDialog {
    pub fn new(rev: Option<String>, read_only: bool) -> Self {
        ReleaseDialog { rev, read_only, collapsed: BTreeSet::from([Group::Other]), ..ReleaseDialog::default() }
    }

    /// Take a fresh view, keeping what the user typed in a field and has not sent.
    pub fn set_view(&mut self, v: View, key: (u64, u64, u64, bool)) {
        let typed: Vec<Option<String>> = self.fields.iter().zip(self.orig.iter()).map(|(f, o)| (f != o).then(|| f.clone())).collect();
        self.fields = v.fields.clone();
        self.orig = v.fields.clone();
        for (i, t) in typed.into_iter().enumerate() {
            if let (Some(t), Some(slot)) = (t, self.fields.get_mut(i)) {
                *slot = t;
            }
        }
        if self.folder.is_empty()
            && let Some(f) = &v.folder
        {
            self.folder = f.to_string_lossy().into_owned();
        }
        self.view = Some(v);
        self.key = Some(key);
    }

    /// The parameters of `release` from the dialog's choices (`existing` when answering §9).
    pub fn params(&self, existing: Option<&str>) -> Value {
        let mut m = Map::new();
        if let Some(r) = &self.rev {
            m.insert("rev".into(), json!(r));
        }
        let default = self.view.as_ref().and_then(|v| v.folder.as_ref()).map(|f| f.to_string_lossy().into_owned());
        if !self.folder.trim().is_empty() && Some(self.folder.trim()) != default.as_deref() {
            m.insert("folder".into(), json!(self.folder.trim()));
        }
        if self.accept {
            m.insert("accept".into(), json!("errors"));
        }
        if self.view.as_ref().is_some_and(|v| v.provisional > 0) {
            m.insert("provisional".into(), json!("issue"));
        }
        if let Some(e) = existing {
            m.insert("existing".into(), json!(e));
        }
        Value::Object(m)
    }

    /// Whether Release can be pressed, and why not.
    pub fn blocked(&self) -> Option<String> {
        let v = self.view.as_ref()?;
        if self.rev.is_some() {
            return None;
        }
        if [0, 2, 3].iter().any(|i| self.fields.get(*i).is_none_or(|f| f.trim().is_empty())) {
            return Some(ecad_cadcraft::revisions::FILL_FIRST.into());
        }
        if v.errors > 0 && !self.accept {
            let n = if v.errors == 1 { "1 error".to_string() } else { format!("{} errors", v.errors) };
            return Some(format!("Fix the error, or tick Release with {n}."));
        }
        v.refusal.clone()
    }

    fn colour(t: &Tokens, s: Severity) -> Color32 {
        match s {
            Severity::Error => Color32::from_rgb(0xFF, 0x00, 0x00),
            Severity::Warning => Color32::from_rgb(0xFF, 0xFF, 0x00),
            Severity::Info => t.text_dim,
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Option<Out> {
        let mut out = None;
        let mut open = true;
        let t = Tokens::get();
        let Some(v) = self.view.clone() else { return None };
        let title = match &self.rev {
            Some(r) => format!("WRITE REV {}'S FOLDER", r.to_uppercase()),
            None => format!("RELEASE REV {}", v.rev.to_uppercase()),
        };
        egui::Window::new(title).id(egui::Id::unique(RELEASE_ID)).collapsible(false).default_width(560.0).min_width(560.0).open(&mut open).show(
            ctx,
            |ui| {
                ui.style_mut().interaction.selectable_labels = false;
                // The revision fields: the pending row's, edited as in Revisions (one `revision`
                // each on leaving it); an issued row's, read only.
                ui.horizontal_wrapped(|ui| {
                    for (i, (f, label)) in
                        ecad_cadcraft::revisions::FIELDS.iter().zip(["Rev", "Date", "Description", "Drawn", "Checked", "Approved"]).enumerate()
                    {
                        ui.label(RichText::new(label).small());
                        let w = match *f {
                            "description" => 180.0,
                            "date" => 80.0,
                            "rev" => 36.0,
                            _ => 48.0,
                        };
                        let Some(val) = self.fields.get_mut(i) else { continue };
                        let editable = self.rev.is_none() && !self.read_only;
                        let r =
                            ui.add_enabled(editable, egui::TextEdit::singleline(val).desired_width(w)).accessible_name(format!("ecad.release.{f}"));
                        if r.lost_focus() && self.orig.get(i) != Some(val) {
                            let value = if val.trim().is_empty() { Value::Null } else { Value::String(val.clone()) };
                            let mut m = Map::new();
                            m.insert((*f).into(), value);
                            out = Some(Out::Run(format!("revision {}", Value::Object(m))));
                            if let Some(o) = self.orig.get_mut(i) {
                                *o = val.clone();
                            }
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Folder").small());
                    ui.add(egui::TextEdit::singleline(&mut self.folder).desired_width(440.0)).accessible_name("ecad.release.folder");
                });
                let writes: Vec<String> =
                    v.writes.iter().filter(|(s, _)| s != "manifest").map(|(s, f)| if s == "book" { f.clone() } else { s.clone() }).collect();
                ui.add(egui::Label::new(RichText::new(format!("Writes  {}", writes.join(" \u{b7} "))).small()).wrap())
                    .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, "ecad.release.writes"));
                if !v.not_included.is_empty() {
                    ui.label(RichText::new("Terminal plan, cable schedule: come with terminal strips (028)").small().color(t.text_dim));
                }
                if let Some(w) = v.exists_words() {
                    ui.label(RichText::new(w).small().color(ui.visuals().warn_fg_color));
                }
                if let Some(w) = &v.warning {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(w).small().color(ui.visuals().warn_fg_color));
                        if let Some(last) = w.strip_prefix("Nothing has changed since Rev ").map(|s| s.trim_end_matches('.').to_string())
                            && ui.small_button(format!("Write Rev {last}'s folder again")).clicked()
                        {
                            out = Some(Out::WriteIssued(last));
                        }
                    });
                }
                ui.separator();
                ui.label(RichText::new("Check").small().strong());
                if v.groups.is_empty() {
                    ui.label(RichText::new(NOTHING_OPEN).small().color(t.text_faint));
                }
                egui::ScrollArea::vertical().id_salt("ecad.release.rows").max_height(320.0).show(ui, |ui| {
                    for (g, rows) in &v.groups {
                        let glyph = rows.first().map_or(Severity::Info, |(c, _)| c.severity());
                        let folded = self.collapsed.contains(g);
                        ui.horizontal(|ui| {
                            let arrow = if folded { "\u{25b8}" } else { "\u{25be}" };
                            let head = ui
                                .add(
                                    egui::Button::new(RichText::new(format!("{arrow} {} {} ({})", glyph.glyph(), g.words(), rows.len())).small())
                                        .frame(false),
                                )
                                .accessible_name(format!("ecad.release.group.{}", g.id()));
                            if head.clicked() {
                                if folded {
                                    self.collapsed.remove(g);
                                } else {
                                    self.collapsed.insert(*g);
                                }
                            }
                            if *g == Group::Parts && ui.small_button("Show in Reports").accessible_name("ecad.release.reports").clicked() {
                                out = Some(Out::ShowReports);
                            }
                        });
                        if folded {
                            continue;
                        }
                        for (i, (c, place)) in rows.iter().take(ROWS_SHOWN).enumerate() {
                            let text = RichText::new(format!("   {} {:<34} {:<9} {}", c.severity().glyph(), c.kind.words(), place, c.which))
                                .small()
                                .monospace();
                            let r = ui
                                .add(egui::Button::new(text.color(Self::colour(&t, c.severity()))).frame(false))
                                .accessible_name(format!("ecad.release.row.{}.{i}", g.id()));
                            if r.on_hover_text("Show it: its page, centred, with a ring on it").clicked() && c.page.is_some() {
                                out = Some(Out::GoTo(Box::new(c.clone())));
                            }
                        }
                        if rows.len() > ROWS_SHOWN && ui.small_button(format!("Show all {} in Checks", rows.len())).clicked() {
                            out = Some(Out::ShowChecks);
                        }
                    }
                });
                if self.rev.is_none() && v.errors > 0 {
                    let n = if v.errors == 1 { "1 error".to_string() } else { format!("{} errors", v.errors) };
                    let them = if v.errors == 1 { "it is" } else { "they are" };
                    ui.checkbox(&mut self.accept, format!("Release with {n} ({them} listed in the folder's checks.csv)"))
                        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::CheckBox, true, "ecad.release.accept"));
                }
                if let Some(e) = &self.error {
                    ui.add(egui::Label::new(RichText::new(e).small().color(Color32::from_rgb(0xFF, 0x00, 0x00))).wrap());
                }
                // §9: the folder exists.
                if let Some(q) = self.exists.clone() {
                    ui.separator();
                    ui.add(egui::Label::new(RichText::new(q).small()).wrap())
                        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, "ecad.release.exists"));
                    ui.horizontal(|ui| {
                        for (label, id) in [("Add missing files", "add-missing"), ("Replace", "replace"), ("Keep both", "keep-both")] {
                            if ui.button(label).accessible_name(format!("ecad.release.exists.{id}")).clicked() {
                                out = Some(Out::Release(self.params(Some(id))));
                            }
                        }
                        if ui.button("Cancel").accessible_name("ecad.release.exists.cancel").clicked() {
                            self.exists = None;
                        }
                    });
                }
                ui.horizontal(|ui| {
                    if self.rev.is_none() && v.refusal.as_deref() == Some(ecad_cadcraft::release::SAVE_FIRST) && ui.button("Save As…").clicked() {
                        out = Some(Out::SaveAs);
                    }
                    let label = match (&self.rev, v.provisional) {
                        (Some(r), _) => format!("Write Rev {r}'s folder"),
                        (None, 0) => format!("Release Rev {}", v.rev),
                        (None, n) => format!("Release Rev {} ({n} provisional)", v.rev),
                    };
                    let blocked = self.blocked().filter(|_| self.exists.is_none());
                    let go = ui
                        .add_enabled(blocked.is_none() && self.exists.is_none(), egui::Button::new(RichText::new(label).strong()))
                        .accessible_name("ecad.release.go");
                    let go = match &blocked {
                        Some(why) => go.on_disabled_hover_text(why),
                        None => go,
                    };
                    if go.clicked() {
                        out = Some(Out::Release(self.params(None)));
                    }
                    if ui.button("Cancel").accessible_name("ecad.release.cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        out = Some(Out::Close);
                    }
                });
            },
        );
        if !open {
            out = Some(Out::Close);
        }
        out
    }
}

/// Open `folder` in the system's file manager (a separate process; nothing waits for it).
///
/// # Errors
/// The file manager could not be started.
pub fn open_folder(folder: &std::path::Path) -> Result<(), String> {
    let program = if cfg!(target_os = "windows") {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(program).arg(folder).spawn().map(|_| ()).map_err(|e| format!("Could not open {}: {e}", folder.display()))
}

/// The folders of issued rows, resolved against the project, and whether each exists.
pub fn fill_folders(rows: &mut [crate::sheets_ui::RevRow], p: &ecad_cadcraft::Project) {
    for r in rows {
        r.folder = r.released.as_deref().and_then(|v| ecad_cadcraft::release::from_recorded(p, v));
        r.folder_exists = r.folder.as_deref().is_some_and(|f| f.join(ecad_cadcraft::release::MANIFEST).exists());
    }
}

/// The badge of the Project panel's Release button (design 027 §3): errors (red), else warnings,
/// from the kept rows; `None` when the Checks have not been kept yet (nothing is built for it).
pub fn badge(p: &ecad_cadcraft::Project) -> Option<(usize, usize)> {
    let list = p.checks.as_ref()?;
    let mut rows: Vec<&Check> = list.rows().collect();
    let extra = p.outputs.as_ref().map(ecad_cadcraft::outputs::Outputs::checks).unwrap_or_default();
    rows.extend(extra.iter());
    let e = rows.iter().filter(|c| c.severity() == Severity::Error).count();
    let w = rows.iter().filter(|c| c.severity() == Severity::Warning).count();
    Some((e, w))
}
