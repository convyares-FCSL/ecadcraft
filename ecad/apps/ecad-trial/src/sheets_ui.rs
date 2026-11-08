//! Brief 019's windows and the ECAD section of CADCraft's Properties palette (design 019 §7, §8):
//! the **Project Properties** dialog, the **Revisions** dialog with Issue and Open, the question
//! a plain undo across an issued revision stops at, and the section drawn through CADCraft's
//! `owner_properties` seam.
//!
//! Everything here only shows copies and returns what the user asked for; the trial runs it
//! through the commands' JSON forms (`projectprops`, `revision`, `issuerevision`, `openrevision`,
//! `undoissue`, `setprop`, `pageprops`), so each write is one transaction and one `u`.
//!
//! Excluded: free-text validation beyond the commands' own (their refusals are shown), custom
//! title blocks, the revision-list page (F6).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use cadcraft_ui_egui::theme::Tokens;
use ecad_cadcraft::props::{PropsView, Row, Target};
use ecad_model::SheetSpec;
use eframe::egui::{self, Color32, RichText};
use serde_json::{Map, Value, json};

/// `ecad.orphan` red.
const RED: Color32 = Color32::from_rgb(0xff, 0x00, 0x00);

/// Element names (design 019 §10).
pub const PROJECT_PROPS_ID: &str = "ecad.projectprops";
pub const REVISIONS_ID: &str = "ecad.revisions";
pub const ISSUE_UNDO_ID: &str = "ecad.issueundo";

/// The fields of the dialog's top group: key, label.
pub const FIELDS: [(&str, &str); 6] = [
    ("title", "Title"),
    ("drawing_number", "Drawing no."),
    ("customer", "Customer"),
    ("status", "Status"),
    ("function", "Function"),
    ("location", "Location"),
];

/// The sizes the Size combo offers.
pub const SIZES: [&str; 5] = ["A3", "A2", "A1", "A0", "Custom…"];

/// Which project popout a [`ProjectProps`] is (owner, fix round of 2026-11-08: project-level
/// things are popouts of the Project panel, not part of Properties).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Popout {
    /// Title, drawing number, customer, status, `=`/`+` defaults, free properties.
    Info,
    /// Sheet size and grid, reference style, page-number step, tag padding and prefixes.
    Settings,
}

/// Element names of the two popouts.
pub const PROJECT_INFO_ID: &str = "ecad.projectinfo";
pub const PROJECT_SETTINGS_ID: &str = "ecad.projectsettings";

/// The Project info or Project settings popout (design 019 §7.1, split by the owner): a copy of
/// the project's properties and settings taken when it opens; OK writes the changed fields as one
/// `projectprops` (one transaction), Cancel or Esc writes nothing.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectProps {
    pub popout: Popout,
    orig: BTreeMap<String, String>,
    pub fields: BTreeMap<String, String>,
    sheet_orig: SheetSpec,
    pub size: String,
    pub width: String,
    pub height: String,
    pub columns: String,
    pub rows: String,
    pub column_letters: bool,
    pub row_letters: bool,
    step_orig: u32,
    pub step: String,
    others_orig: Vec<(String, String)>,
    pub others: Vec<(String, String)>,
    pub revision: String,
    style_orig: String,
    /// `page-column-row` or `page-column`.
    pub style: String,
    padding_orig: String,
    pub padding: String,
    prefixes_orig: Vec<(String, String)>,
    /// Starter symbol key → its prefix as the project uses it (`-K`).
    pub prefixes: Vec<(String, String)>,
    pub read_only: bool,
    /// The command's refusal of the last OK.
    pub error: Option<String>,
}

/// What a dialog asked for this frame.
#[derive(Debug, Clone, PartialEq)]
pub enum Out {
    /// Run this command line (JSON form).
    Run(String),
    Close,
    OpenRevisions,
    /// Open this issued revision read-only.
    Open(String),
}

impl ProjectProps {
    /// The dialog filled from `projectprops {}`'s result.
    pub fn from_json(v: &Value, read_only: bool, popout: Popout) -> Self {
        let text = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
        let fields: BTreeMap<String, String> = FIELDS.iter().map(|(k, _)| (k.to_string(), text(k))).collect();
        let sheet = SheetSpec::parse(&text("sheet")).unwrap_or_default();
        let others: Vec<(String, String)> = v
            .get("props")
            .and_then(Value::as_object)
            .map(|m| m.iter().map(|(k, x)| (k.clone(), x.as_str().unwrap_or_default().to_string())).collect())
            .unwrap_or_default();
        let step = v.get("page_number_step").and_then(Value::as_u64).unwrap_or(1) as u32;
        let issued: Vec<&Value> = v
            .get("revisions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|r| !r["issued"].is_null())
            .collect();
        let revision = match issued.last() {
            Some(r) => format!(
                "Current: {}, {} \"{}\"",
                r["rev"].as_str().unwrap_or_default(),
                r["date"].as_str().unwrap_or_default(),
                r["description"].as_str().unwrap_or_default()
            ),
            None => "Current: none issued".into(),
        };
        let style = v.get("reference_style").and_then(Value::as_str).unwrap_or("page-column-row").to_string();
        let padding = v.get("tag_padding").and_then(Value::as_u64).unwrap_or(2).to_string();
        let prefixes: Vec<(String, String)> = v
            .get("tag_prefixes")
            .and_then(Value::as_object)
            .map(|m| m.iter().map(|(k, x)| (k.clone(), x.as_str().unwrap_or_default().to_string())).collect())
            .unwrap_or_default();
        ProjectProps {
            popout,
            style_orig: style.clone(),
            style,
            padding_orig: padding.clone(),
            padding,
            prefixes_orig: prefixes.clone(),
            prefixes,
            orig: fields.clone(),
            fields,
            sheet_orig: sheet,
            size: sheet.preset_name().map_or_else(|| "Custom…".to_string(), str::to_string),
            width: sheet.width.to_string(),
            height: sheet.height.to_string(),
            columns: sheet.columns.to_string(),
            rows: sheet.rows.to_string(),
            column_letters: sheet.column_labels == ecad_model::GridLabels::Letters,
            row_letters: sheet.row_labels == ecad_model::GridLabels::Letters,
            step_orig: step,
            step: step.to_string(),
            others_orig: others.clone(),
            others,
            revision,
            read_only,
            error: None,
        }
    }

    /// The sheet the fields describe, or why it cannot be.
    pub fn sheet(&self) -> Result<SheetSpec, String> {
        let words = |b: bool| if b { "letters" } else { "numbers" };
        let size = if self.size.starts_with("Custom") {
            format!("{}x{}", self.width.trim(), self.height.trim())
        } else {
            self.size.clone()
        };
        SheetSpec::parse(&format!(
            "{size} {}x{} {} {}",
            self.columns.trim(),
            self.rows.trim(),
            words(self.column_letters),
            words(self.row_letters)
        ))
        .map_err(|e| {
            if e.starts_with("Sheet") {
                "Width 420 to 2400 mm, height 250 to 2400 mm, 1 to 26 columns and rows.".into()
            } else {
                e
            }
        })
    }

    /// The `projectprops` line writing what changed in this popout; `None` when nothing did.
    pub fn line(&self) -> Result<Option<String>, String> {
        let mut m = Map::new();
        if self.popout == Popout::Info {
            self.info_fields(&mut m);
        } else {
            self.settings_fields(&mut m)?;
        }
        Ok((!m.is_empty()).then(|| format!("projectprops {}", Value::Object(m))))
    }

    fn settings_fields(&self, m: &mut Map<String, Value>) -> Result<(), String> {
        if self.style != self.style_orig {
            m.insert("reference_style".into(), Value::String(self.style.clone()));
        }
        if self.padding.trim() != self.padding_orig {
            let n: u64 = self.padding.trim().parse().map_err(|_| "Tag digits is a whole number.".to_string())?;
            m.insert("tag_padding".into(), json!(n));
        }
        let mut px = Map::new();
        for (k, v) in &self.prefixes {
            if !self.prefixes_orig.contains(&(k.clone(), v.clone())) {
                px.insert(k.clone(), Value::String(v.trim().trim_start_matches('-').to_string()));
            }
        }
        if !px.is_empty() {
            m.insert("tag_prefixes".into(), Value::Object(px));
        }
        let sheet = self.sheet()?;
        if sheet != self.sheet_orig {
            m.insert("sheet".into(), Value::String(sheet.text()));
        }
        let step: u32 = self.step.trim().parse().map_err(|_| "The page number step is a whole number.".to_string())?;
        if step != self.step_orig {
            m.insert("page_number_step".into(), json!(step));
        }
        Ok(())
    }

    fn info_fields(&self, m: &mut Map<String, Value>) {
        for (k, v) in &self.fields {
            if self.orig.get(k) != Some(v) {
                m.insert(k.clone(), if v.trim().is_empty() { Value::Null } else { Value::String(v.clone()) });
            }
        }
        let mut props = Map::new();
        for (k, v) in &self.others {
            if !self.others_orig.contains(&(k.clone(), v.clone())) && !k.trim().is_empty() {
                props.insert(k.trim().to_string(), Value::String(v.clone()));
            }
        }
        for (k, _) in &self.others_orig {
            if !self.others.iter().any(|(x, _)| x == k) {
                props.insert(k.clone(), Value::Null);
            }
        }
        if !props.is_empty() {
            m.insert("props".into(), Value::Object(props));
        }
    }

    /// The window. Returns what was asked this frame.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<Out> {
        let mut out = None;
        let ro = self.read_only;
        let mut open = true;
        let (title, id) = match self.popout {
            Popout::Info => ("PROJECT INFO", PROJECT_INFO_ID),
            Popout::Settings => ("PROJECT SETTINGS", PROJECT_SETTINGS_ID),
        };
        let info = self.popout == Popout::Info;
        egui::Window::new(title)
            .id(egui::Id::unique(id))
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .open(&mut open)
            .show(ctx, |ui| {
                if info {
                    egui::Grid::new("ecad.projectprops.grid").num_columns(2).show(ui, |ui| {
                        for (k, label) in FIELDS {
                            ui.label(label);
                            let v = self.fields.entry(k.to_string()).or_default();
                            if k == "status" {
                                ui.horizontal(|ui| {
                                    ui.add_enabled(!ro, egui::TextEdit::singleline(v).desired_width(220.0))
                                        .on_hover_text("Any text; or pick one")
                                        .accessible_name("ecad.projectprops.status");
                                    ui.add_enabled_ui(!ro, |ui| {
                                        ui.menu_button("v", |ui| {
                                            for s in ecad_cadcraft::sheet_cmds::STATUS_SUGGESTIONS {
                                                if ui.button(s).clicked() {
                                                    *v = s.to_string();
                                                    ui.close();
                                                }
                                            }
                                        });
                                    });
                                });
                            } else {
                                ui.add_enabled(!ro, egui::TextEdit::singleline(v).desired_width(260.0))
                                    .accessible_name(format!("ecad.projectprops.{k}"));
                            }
                            ui.end_row();
                        }
                    });
                    ui.separator();
                    ui.label(RichText::new("Other properties").small().strong());
                    let mut remove = None;
                    for (i, (k, v)) in self.others.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            ui.add_enabled(!ro, egui::TextEdit::singleline(k).desired_width(120.0));
                            ui.add_enabled(!ro, egui::TextEdit::singleline(v).desired_width(200.0));
                            if !ro && ui.small_button("x").clicked() {
                                remove = Some(i);
                            }
                        });
                    }
                    if let Some(i) = remove {
                        self.others.remove(i);
                    }
                    if !ro && ui.button("Add property").clicked() {
                        self.others.push((String::new(), String::new()));
                    }
                    ui.label(RichText::new(&self.revision).small());
                } else {
                    ui.label(RichText::new("Sheet").small().strong());
                    ui.add_enabled_ui(!ro, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Size");
                            egui::ComboBox::from_id_salt("ecad.projectprops.size").selected_text(&self.size).show_ui(
                                ui,
                                |ui| {
                                    for s in SIZES {
                                        if ui.selectable_label(self.size == s, s).clicked() {
                                            self.size = s.to_string();
                                            if let Some(p) = SheetSpec::preset(s) {
                                                self.columns = p.columns.to_string();
                                                self.rows = p.rows.to_string();
                                                self.width = p.width.to_string();
                                                self.height = p.height.to_string();
                                            }
                                        }
                                    }
                                },
                            );
                            if self.size.starts_with("Custom") {
                                ui.add(egui::TextEdit::singleline(&mut self.width).desired_width(44.0))
                                    .accessible_name("ecad.projectprops.width");
                                ui.label("×");
                                ui.add(egui::TextEdit::singleline(&mut self.height).desired_width(44.0))
                                    .accessible_name("ecad.projectprops.height");
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("Columns");
                            ui.add(egui::TextEdit::singleline(&mut self.columns).desired_width(30.0))
                                .accessible_name("ecad.projectprops.columns");
                            ui.checkbox(&mut self.column_letters, "Letters");
                            ui.label("Rows");
                            ui.add(egui::TextEdit::singleline(&mut self.rows).desired_width(30.0))
                                .accessible_name("ecad.projectprops.rows");
                            ui.checkbox(&mut self.row_letters, "Letters");
                        });
                        ui.horizontal(|ui| {
                            ui.label("Page number step");
                            ui.add(egui::TextEdit::singleline(&mut self.step).desired_width(40.0))
                                .accessible_name("ecad.projectprops.step");
                        });
                    });
                    let sheet = self.sheet();
                    if let Err(e) = &sheet {
                        ui.label(RichText::new(e).small().color(RED));
                    } else if sheet.as_ref().ok() != Some(&self.sheet_orig) {
                        ui.label(RichText::new("Changing the grid changes reference texts on every page.").small());
                    }
                    ui.separator();
                    ui.label(RichText::new("References").small().strong());
                    ui.add_enabled_ui(!ro, |ui| {
                        ui.horizontal(|ui| {
                            for (word, shown) in [("page-column-row", "105-C-4"), ("page-column", "/105.C")] {
                                if ui.radio(self.style == word, shown).clicked() {
                                    self.style = word.to_string();
                                }
                            }
                        });
                    });
                    ui.separator();
                    ui.label(RichText::new("Tags").small().strong());
                    ui.add_enabled_ui(!ro, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Digits");
                            ui.add(egui::TextEdit::singleline(&mut self.padding).desired_width(30.0))
                                .accessible_name("ecad.projectsettings.padding");
                        });
                        egui::Grid::new("ecad.projectsettings.prefixes").num_columns(4).show(ui, |ui| {
                            for (i, (k, v)) in self.prefixes.iter_mut().enumerate() {
                                ui.label(RichText::new(k.as_str()).small());
                                ui.add(egui::TextEdit::singleline(v).desired_width(40.0))
                                    .accessible_name(format!("ecad.projectsettings.prefix.{k}"));
                                if i % 2 == 1 {
                                    ui.end_row();
                                }
                            }
                        });
                    });
                }
                if let Some(e) = &self.error {
                    ui.label(RichText::new(e).small().color(RED));
                }
                ui.horizontal(|ui| {
                    let line = self.line();
                    if !ro {
                        let ok = ui
                            .add_enabled(line.is_ok(), egui::Button::new("OK"))
                            .accessible_name("ecad.projectprops.ok");
                        if ok.clicked() {
                            out = Some(match line {
                                Ok(Some(l)) => Out::Run(l),
                                _ => Out::Close,
                            });
                        }
                    }
                    if ui.button("Cancel").accessible_name("ecad.projectprops.cancel").clicked()
                        || ui.input(|i| i.key_pressed(egui::Key::Escape))
                    {
                        out = Some(Out::Close);
                    }
                });
            });
        if !open {
            out = Some(Out::Close);
        }
        out
    }
}

/// One row of the Revisions dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevRow {
    pub fields: [String; 6],
    pub issued: bool,
}

/// The Revisions dialog (design 019 §7.2): issued rows read-only, newest first; the pending row's
/// fields write `revision` when they lose focus; Issue asks first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Revisions {
    pub issued: Vec<RevRow>,
    pub pending: [String; 6],
    orig: [String; 6],
    pub read_only: bool,
    /// The question of Issue Rev B… is open.
    pub confirming: bool,
    pub error: Option<String>,
}

impl Revisions {
    /// Filled from `projectprops {}`'s result.
    pub fn from_json(v: &Value, read_only: bool) -> Self {
        let rows: Vec<RevRow> = v
            .get("revisions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|r| RevRow {
                fields: ecad_cadcraft::revisions::FIELDS.map(|f| r[f].as_str().unwrap_or_default().to_string()),
                issued: !r["issued"].is_null(),
            })
            .collect();
        let mut pending = rows.iter().rev().find(|r| !r.issued).map(|r| r.fields.clone()).unwrap_or_default();
        if pending[0].is_empty() {
            pending[0] = v.get("pending_rev").and_then(Value::as_str).unwrap_or("A").to_string();
        }
        let mut issued: Vec<RevRow> = rows.into_iter().filter(|r| r.issued).collect();
        issued.reverse();
        Revisions { issued, orig: pending.clone(), pending, read_only, confirming: false, error: None }
    }

    /// Whether Issue can be pressed: Rev, Description and Drawn are filled (owner answer Q7).
    pub fn can_issue(&self) -> bool {
        [0, 2, 3].iter().all(|i| self.pending.get(*i).is_some_and(|v| !v.trim().is_empty()))
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Option<Out> {
        let mut out = None;
        let mut open = true;
        let headers = ["Rev", "Date", "Description", "Drawn", "Checked", "Approved"];
        egui::Window::new("REVISIONS")
            .id(egui::Id::unique(REVISIONS_ID))
            .collapsible(false)
            .default_width(680.0)
            .min_width(620.0)
            .open(&mut open)
            .show(ctx, |ui| {
                egui::Grid::new("ecad.revisions.grid").num_columns(7).striped(true).show(ui, |ui| {
                    for h in headers {
                        ui.label(RichText::new(h).small().strong());
                    }
                    ui.label("");
                    ui.end_row();
                    if !self.read_only {
                        for (i, f) in ecad_cadcraft::revisions::FIELDS.iter().enumerate() {
                            let Some(v) = self.pending.get_mut(i) else { continue };
                            let w = match *f {
                                "description" => 200.0,
                                "date" => 84.0,
                                "rev" => 40.0,
                                _ => 60.0,
                            };
                            let r = ui
                                .add(egui::TextEdit::singleline(v).desired_width(w))
                                .accessible_name(format!("ecad.revisions.pending.{f}"));
                            if r.lost_focus() && self.orig.get(i) != Some(v) {
                                let value = if v.trim().is_empty() { Value::Null } else { Value::String(v.clone()) };
                                let mut m = Map::new();
                                m.insert((*f).into(), value);
                                out = Some(Out::Run(format!("revision {}", Value::Object(m))));
                                if let Some(o) = self.orig.get_mut(i) {
                                    *o = v.clone();
                                }
                            }
                        }
                        ui.label(RichText::new("pending").small());
                        ui.end_row();
                    }
                    for r in &self.issued {
                        for v in &r.fields {
                            ui.label(v);
                        }
                        let rev = r.fields[0].clone();
                        if ui.button("Reopen").on_hover_text("Reopen this revision, read-only").accessible_name(format!("ecad.revisions.open.{rev}")).clicked() {
                            out = Some(Out::Open(rev));
                        }
                        ui.end_row();
                    }
                });
                if let Some(e) = &self.error {
                    ui.label(RichText::new(e).small().color(RED));
                }
                ui.horizontal(|ui| {
                    if !self.read_only {
                        let rev = self.pending[0].clone();
                        let b = ui
                            .add_enabled(self.can_issue(), egui::Button::new(format!("Issue Rev {rev}…")))
                            .accessible_name("ecad.revisions.issue");
                        let b = if self.can_issue() { b } else { b.on_disabled_hover_text(ecad_cadcraft::revisions::FILL_FIRST) };
                        if b.clicked() {
                            self.confirming = true;
                        }
                    }
                    if ui.button("Close").clicked() {
                        out = Some(Out::Close);
                    }
                });
                if self.confirming {
                    let rev = self.pending[0].clone();
                    ui.separator();
                    ui.label(format!(
                        "Issue Rev {rev}? The project as it is now is kept as \"Rev {rev}\" and can be opened read-only."
                    ));
                    ui.horizontal(|ui| {
                        if ui.button("Issue").clicked() {
                            self.confirming = false;
                            // Fields still being typed go with the issue (one transaction).
                            let mut m = Map::new();
                            for (i, f) in ecad_cadcraft::revisions::FIELDS.iter().enumerate() {
                                if let (Some(v), Some(o)) = (self.pending.get(i), self.orig.get(i))
                                    && v != o
                                    && !v.trim().is_empty()
                                {
                                    m.insert((*f).into(), Value::String(v.clone()));
                                }
                            }
                            out = Some(Out::Run(format!("issuerevision {}", Value::Object(m))));
                        }
                        if ui.button("Cancel").clicked() {
                            self.confirming = false;
                        }
                    });
                }
            });
        if !open {
            out = Some(Out::Close);
        }
        out
    }
}

/// The question a plain `u` across an issued revision stops at (owner answer Q7). Returns
/// `Some(true)` to undo the issue, `Some(false)` to keep it.
pub fn issue_undo(ctx: &egui::Context, question: &str) -> Option<bool> {
    let mut out = None;
    egui::Window::new("Undo the issue?")
        .id(egui::Id::unique(ISSUE_UNDO_ID))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(question);
            ui.horizontal(|ui| {
                if ui.button("Undo the issue").clicked() {
                    out = Some(true);
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    out = Some(false);
                }
            });
        });
    out
}

/// What the Properties section shares between the trial and the seam closure: the copy it draws,
/// and what the user asked for, taken by the trial after CADCraft's frame.
#[derive(Debug, Default)]
pub struct PropsShared {
    pub view: PropsView,
    pub writable: bool,
    /// Edits: target, key, value (`None`: clear).
    pub edits: Vec<(Target, String, Option<String>)>,
    /// Buttons pressed: `revisions`, `more`, `goto:<hex>`.
    pub buttons: Vec<String>,
    /// A refusal per row key, shown under the field.
    pub errors: BTreeMap<String, String>,
    /// How many frames drew the section (tests).
    pub frames: u64,
}

/// The shared state's handle.
pub type Shared = Arc<Mutex<PropsShared>>;

/// The closure CADCraft calls at the top of its Properties palette.
pub fn section(shared: Shared) -> Box<dyn FnMut(&mut egui::Ui)> {
    Box::new(move |ui: &mut egui::Ui| {
        let Ok(mut sh) = shared.lock() else { return };
        sh.frames += 1;
        draw(ui, &mut sh);
    })
}

fn draw(ui: &mut egui::Ui, sh: &mut PropsShared) {
    if sh.view.sections.is_empty() {
        return;
    }
    let t = Tokens::get();
    let sections = sh.view.sections.clone();
    for s in sections {
        ui.add_space(4.0);
        ui.label(RichText::new(&s.title).strong().color(t.text))
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, format!("ecad.props.{}", s.kind)));
        for row in &s.rows {
            prop_row(ui, sh, &s.target, s.kind, row);
        }
        if let Some(n) = &s.note {
            ui.label(RichText::new(n).small().color(t.text_dim));
        }
        if !s.buttons.is_empty() {
            ui.horizontal(|ui| {
                for b in &s.buttons {
                    let label = match b.as_str() {
                        "revisions" => "Revisions…".to_string(),
                        "more" => "More…".to_string(),
                        g if g.starts_with("goto:") => "Go to".to_string(),
                        other => other.to_string(),
                    };
                    if ui.small_button(label).clicked() {
                        sh.buttons.push(b.clone());
                    }
                }
            });
        }
    }
    ui.separator();
}

fn prop_row(ui: &mut egui::Ui, sh: &mut PropsShared, target: &Option<Target>, kind: &str, row: &Row) {
    let t = Tokens::get();
    let name = format!("ecad.props.{kind}.{}", row.key);
    let error = sh.errors.get(&row.key).cloned();
    ui.horizontal(|ui| {
        let (lr, _) = ui.allocate_exact_size(egui::vec2(100.0, 20.0), egui::Sense::hover());
        ui.painter().text(
            egui::pos2(lr.right() - 6.0, lr.center().y),
            egui::Align2::RIGHT_CENTER,
            &row.label,
            egui::FontId::proportional(12.0),
            t.text_dim,
        );
        let writable = sh.writable && row.editable && target.is_some();
        if !writable {
            let colour = if row.varies { t.text_faint } else { t.text };
            ui.label(RichText::new(&row.value).color(colour)).on_hover_text(if sh.writable {
                "Read-only"
            } else {
                "The project is read-only."
            });
            return;
        }
        let id = ui.make_persistent_id(&name);
        let shown = if row.varies || row.inherited { String::new() } else { row.value.clone() };
        let mut buf = ui.data_mut(|d| d.get_temp::<String>(id)).unwrap_or_else(|| shown.clone());
        let mut edit = egui::TextEdit::singleline(&mut buf).desired_width(ui.available_width() - 30.0);
        if row.varies || row.inherited {
            edit = edit.hint_text(RichText::new(&row.value).color(t.text_faint));
        }
        let resp = ui.add(edit).accessible_name(&name);
        let resp = match &error {
            Some(_) => {
                ui.painter().rect_stroke(resp.rect, 0.0, egui::Stroke::new(1.0, RED), egui::StrokeKind::Outside);
                resp
            }
            None => resp,
        };
        let mut commit: Option<String> = None;
        if resp.has_focus() {
            ui.data_mut(|d| d.insert_temp(id, buf.clone()));
        } else {
            ui.data_mut(|d| d.remove::<String>(id));
        }
        if resp.lost_focus() {
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                ui.data_mut(|d| d.remove::<String>(id));
            } else if buf != shown {
                commit = Some(buf.clone());
            }
        }
        if !row.choices.is_empty() {
            ui.menu_button("v", |ui| {
                for c in &row.choices {
                    if ui.button(c).clicked() {
                        commit = Some(c.clone());
                        ui.close();
                    }
                }
            });
        }
        if let (Some(v), Some(tg)) = (commit, target) {
            let value = if v.trim().is_empty() { None } else { Some(v) };
            sh.edits.push((tg.clone(), row.key.clone(), value));
        }
    });
    if let Some(e) = error {
        ui.label(RichText::new(e).small().color(RED));
    }
}

/// `Rev A, 2026-11-08: read-only` (the revision bar's text).
pub fn revision_bar(rev: &str, date: &str) -> String {
    if date.is_empty() { format!("Rev {rev}: read-only") } else { format!("Rev {rev}, {date}: read-only") }
}
