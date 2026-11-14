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
/// `ecad.check-warning`.
const WARNING: Color32 = Color32::from_rgb(0xFF, 0xFF, 0x00);

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

/// What a [`ProjectProps`] shows in its Settings section (brief 042).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// Settings ▸ Project ▸ Info.
    Info,
    /// Settings ▸ Project ▸ Settings: sheet, references, tags, revisions, numbering, wires.
    Settings,
    /// Settings ▸ Text styles (brief 031's Text section).
    Text,
}

/// Element names of the two popouts (now the id salts of their Settings sections).
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
    /// `page-zone`, `page-column-row` or `page-column`.
    pub style: String,
    mirror_orig: String,
    /// Where contact tables go by default (brief 025): `auto`, `below`, `right` or `off`.
    pub mirror: String,
    padding_orig: String,
    pub padding: String,
    prefixes_orig: Vec<(String, String)>,
    /// Starter symbol key → its prefix as the project uses it (`-K`).
    pub prefixes: Vec<(String, String)>,
    colour_style_orig: String,
    /// `iec` or `three-letter` (brief 022).
    pub colour_style: String,
    label_height_orig: String,
    /// Wire label height, millimetres (brief 022).
    pub label_height: String,
    provisional_orig: bool,
    /// Brief 026: Provisional tags until Finalise.
    pub provisional: bool,
    tag_dash_orig: bool,
    /// Brief 033 (design 033 §8.1): tags written with a leading `-`.
    pub tag_dash: bool,
    tag_case_orig: String,
    /// `upper` or `typed`.
    pub tag_case: String,
    pending_orig: String,
    /// Brief 033 (design 033 §6.2): `star`, `draft` or `off`.
    pub pending: String,
    rev_columns_orig: String,
    /// Brief 033 (design 033 §7.1): the revision columns as written.
    pub rev_columns: String,
    /// Brief 031: the Text section (Project settings only), filled from `textstyle {}`.
    pub text: Option<crate::text_ui::TextSection>,
    /// Brief 032 §4.4: the wire label rows (Project settings only), filled from `wiresettings {}`.
    pub labels: Option<WireLabels>,
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
    /// Brief 020a: show main (Save and switch rules apply), from Revisions on another branch.
    SwitchToMain,
    /// Brief 020b: merge the branch shown into main (the merge dialog).
    MergeIntoMain,
    /// Brief 027: the Release dialog, for the pending revision (`None`) or to write an issued
    /// revision's folder again.
    Release(Option<String>),
    /// Brief 027: open a release folder in the system's file manager.
    OpenFolder(std::path::PathBuf),
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
        let issued: Vec<&Value> = v.get("revisions").and_then(Value::as_array).into_iter().flatten().filter(|r| !r["issued"].is_null()).collect();
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
        // Brief 033: the stored column list, else the columns as the project draws them.
        let rev_columns = v.get("revision_columns_stored").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| {
            v.get("revision_columns")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(|c| c["field"].as_str()).collect::<Vec<_>>().join(", "))
                .unwrap_or_default()
        });
        let mirror = v.get("contact_mirror").and_then(Value::as_str).unwrap_or("auto").to_string();
        let padding = v.get("tag_padding").and_then(Value::as_u64).unwrap_or(2).to_string();
        let colour_style = v.get("colour_style").and_then(Value::as_str).unwrap_or("iec").to_string();
        let label_height =
            v.get("label_height").and_then(Value::as_f64).map_or_else(|| ecad_model::wiring::DEFAULT_LABEL_HEIGHT.to_string(), |h| h.to_string());
        let prefixes: Vec<(String, String)> = v
            .get("tag_prefixes")
            .and_then(Value::as_object)
            .map(|m| m.iter().map(|(k, x)| (k.clone(), x.as_str().unwrap_or_default().to_string())).collect())
            .unwrap_or_default();
        ProjectProps {
            popout,
            style_orig: style.clone(),
            style,
            mirror_orig: mirror.clone(),
            mirror,
            padding_orig: padding.clone(),
            padding,
            prefixes_orig: prefixes.clone(),
            prefixes,
            colour_style_orig: colour_style.clone(),
            colour_style,
            label_height_orig: label_height.clone(),
            label_height,
            tag_dash_orig: v.get("tag_dash").and_then(Value::as_bool).unwrap_or(true),
            tag_dash: v.get("tag_dash").and_then(Value::as_bool).unwrap_or(true),
            tag_case_orig: text("tag_case"),
            tag_case: text("tag_case"),
            pending_orig: text("revision_pending"),
            pending: text("revision_pending"),
            rev_columns_orig: rev_columns.clone(),
            rev_columns,
            provisional_orig: v.get("provisional").and_then(Value::as_bool).unwrap_or(false),
            provisional: v.get("provisional").and_then(Value::as_bool).unwrap_or(false),
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
            text: None,
            labels: None,
            read_only,
            error: None,
        }
    }

    /// The sheet the fields describe, or why it cannot be.
    pub fn sheet(&self) -> Result<SheetSpec, String> {
        let words = |b: bool| if b { "letters" } else { "numbers" };
        let size = if self.size.starts_with("Custom") { format!("{}x{}", self.width.trim(), self.height.trim()) } else { self.size.clone() };
        SheetSpec::parse(&format!("{size} {}x{} {} {}", self.columns.trim(), self.rows.trim(), words(self.column_letters), words(self.row_letters)))
            .map_err(|e| if e.starts_with("Sheet") { "Width 420 to 2400 mm, height 250 to 2400 mm, 1 to 26 columns and rows.".into() } else { e })
    }

    /// The command lines writing what changed in this popout, one per line (`projectprops`, then
    /// brief 031's `textstyle` for the Text section); `None` when nothing did.
    pub fn line(&self) -> Result<Option<String>, String> {
        let mut m = Map::new();
        if self.popout == Popout::Info {
            self.info_fields(&mut m);
        } else {
            self.settings_fields(&mut m)?;
        }
        let mut lines: Vec<String> = Vec::new();
        if !m.is_empty() {
            lines.push(format!("projectprops {}", Value::Object(m)));
        }
        if self.popout == Popout::Settings
            && let Some(t) = &self.text
            && let Some(l) = t.line()?
        {
            lines.push(l);
        }
        if self.popout == Popout::Settings
            && let Some(w) = &self.labels
            && let Some(l) = w.line()?
        {
            lines.push(l);
        }
        Ok((!lines.is_empty()).then(|| {
            lines.join(
                "
",
            )
        }))
    }

    fn settings_fields(&self, m: &mut Map<String, Value>) -> Result<(), String> {
        if self.style != self.style_orig {
            m.insert("reference_style".into(), Value::String(self.style.clone()));
        }
        if self.mirror != self.mirror_orig {
            m.insert("contact_mirror".into(), Value::String(self.mirror.clone()));
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
        if self.colour_style != self.colour_style_orig {
            m.insert("colour_style".into(), Value::String(self.colour_style.clone()));
        }
        if self.provisional != self.provisional_orig {
            m.insert("provisional".into(), json!(self.provisional));
        }
        // Brief 033: the tag style (the command rewrites the tags in the same transaction), the
        // pending marker and the revision columns.
        if self.tag_dash != self.tag_dash_orig {
            m.insert("tag_dash".into(), json!(self.tag_dash));
        }
        if self.tag_case != self.tag_case_orig {
            m.insert("tag_case".into(), Value::String(self.tag_case.clone()));
        }
        if self.pending != self.pending_orig {
            m.insert("revision_pending".into(), Value::String(self.pending.clone()));
        }
        if self.rev_columns.trim() != self.rev_columns_orig.trim() {
            let t = self.rev_columns.trim();
            if !t.is_empty() {
                ecad_cadcraft::revisions::parse_columns(t)?;
            }
            m.insert("revision_columns".into(), if t.is_empty() { Value::Null } else { Value::String(t.to_string()) });
        }
        if self.label_height.trim() != self.label_height_orig {
            let h: f64 = self
                .label_height
                .trim()
                .parse()
                .ok()
                .filter(|h| ecad_model::wiring::LABEL_HEIGHTS.contains(h))
                .ok_or_else(|| "Label height is 0.5 to 10 mm.".to_string())?;
            m.insert("label_height".into(), json!(h));
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

    /// The popout's contents drawn in place, as a Settings section (brief 042: the popouts became
    /// Settings ▸ Project and Text styles; contents unchanged). `part` picks what is shown:
    /// [`Part::Settings`] is the project settings without the Text section, which
    /// [`Part::Text`] shows. OK writes every changed field of the copy, whichever part shows it.
    /// Returns what was asked this frame.
    pub fn show_in(&mut self, ui: &mut egui::Ui, part: Part) -> Option<Out> {
        let mut out = None;
        let ro = self.read_only;
        let id = match self.popout {
            Popout::Info => PROJECT_INFO_ID,
            Popout::Settings => PROJECT_SETTINGS_ID,
        };
        let info = self.popout == Popout::Info;
        let text_only = part == Part::Text;
        let max_body = (ui.ctx().content_rect().height() * 0.55).max(200.0);
        ui.push_id(id, |ui| {
            // Brief 031: the Text section makes the settings taller than a small window; the
            // fields scroll, OK and Cancel stay in view. The fields take their own height up to
            // `max_body`, not the height the host had last frame: an auto-sized host offers its
            // last size as the room left, so the row under the fields would overflow it and it
            // would grow by that row every frame (U2 follow-up).
            egui::ScrollArea::vertical().max_height(max_body).min_scrolled_height(max_body).auto_shrink([false, true]).show(ui, |ui| {
                if text_only {
                    // Brief 031 (design §4.1): text styles.
                    if let Some(t) = &mut self.text {
                        t.show(ui, ro, WARNING, RED);
                    }
                } else if info {
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
                            egui::ComboBox::from_id_salt("ecad.projectprops.size").selected_text(&self.size).show_ui(ui, |ui| {
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
                            });
                            if self.size.starts_with("Custom") {
                                ui.add(egui::TextEdit::singleline(&mut self.width).desired_width(44.0)).accessible_name("ecad.projectprops.width");
                                ui.label("×");
                                ui.add(egui::TextEdit::singleline(&mut self.height).desired_width(44.0)).accessible_name("ecad.projectprops.height");
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("Columns");
                            ui.add(egui::TextEdit::singleline(&mut self.columns).desired_width(30.0)).accessible_name("ecad.projectprops.columns");
                            ui.checkbox(&mut self.column_letters, "Letters");
                            ui.label("Rows");
                            ui.add(egui::TextEdit::singleline(&mut self.rows).desired_width(30.0)).accessible_name("ecad.projectprops.rows");
                            ui.checkbox(&mut self.row_letters, "Letters");
                        });
                        ui.horizontal(|ui| {
                            ui.label("Page number step");
                            ui.add(egui::TextEdit::singleline(&mut self.step).desired_width(40.0)).accessible_name("ecad.projectprops.step");
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
                            for (word, shown) in [("page-zone", "105/B3"), ("page-column-row", "105-B-3"), ("page-column", "/105.3")] {
                                if ui.radio(self.style == word, shown).clicked() {
                                    self.style = word.to_string();
                                }
                            }
                        });
                        // Brief 025 (FP3): where a coil's contact table goes unless the device says.
                        ui.horizontal(|ui| {
                            ui.label("Contact tables");
                            egui::ComboBox::from_id_salt("ecad.projectprops.contact_mirror").selected_text(&self.mirror).show_ui(ui, |ui| {
                                for w in ["auto", "below", "right", "off"] {
                                    if ui.selectable_label(self.mirror == w, w).clicked() {
                                        self.mirror = w.to_string();
                                    }
                                }
                            });
                        });
                    });
                    ui.separator();
                    ui.label(RichText::new("Tags").small().strong());
                    ui.add_enabled_ui(!ro, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Digits");
                            ui.add(egui::TextEdit::singleline(&mut self.padding).desired_width(30.0)).accessible_name("ecad.projectsettings.padding");
                        });
                        // Brief 033 (design 033 §8.1): the leading - and the case, a writing rule.
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut self.tag_dash, "Leading - on tags").accessible_name("ecad.project.props.tag_dash");
                            ui.label("Case");
                            let shown = if self.tag_case == "typed" { "As typed" } else { "Upper case" };
                            egui::ComboBox::from_id_salt("ecad.project.props.tag_case").selected_text(shown).show_ui(ui, |ui| {
                                for (w, label) in [("upper", "Upper case"), ("typed", "As typed")] {
                                    if ui.selectable_label(self.tag_case == w, label).clicked() {
                                        self.tag_case = w.to_string();
                                    }
                                }
                            });
                        });
                        let case = if self.tag_case == "typed" { ecad_model::tag::Case::Typed } else { ecad_model::tag::Case::Upper };
                        let pad = self.padding.trim().parse::<u8>().unwrap_or(2);
                        let example =
                            ecad_model::tag::normalise("k5", ecad_model::tag::Style { padding: pad, dash: self.tag_dash, case }).unwrap_or_default();
                        ui.label(RichText::new(format!("k5 is written {example}")).small()).accessible_name("ecad.project.props.tag_example");
                        egui::Grid::new("ecad.projectsettings.prefixes").num_columns(4).show(ui, |ui| {
                            for (i, (k, v)) in self.prefixes.iter_mut().enumerate() {
                                ui.label(RichText::new(k.as_str()).small());
                                ui.add(egui::TextEdit::singleline(v).desired_width(40.0)).accessible_name(format!("ecad.projectsettings.prefix.{k}"));
                                if i % 2 == 1 {
                                    ui.end_row();
                                }
                            }
                        });
                    });
                    // Brief 033 (design 033 §6.2, §7.1): the pending marker and the revision columns.
                    ui.separator();
                    ui.label(RichText::new("Revisions").small().strong());
                    ui.add_enabled_ui(!ro, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Pending changes");
                            let shown = match self.pending.as_str() {
                                "draft" => "E (draft)",
                                "off" => "Off",
                                _ => "D*",
                            };
                            egui::ComboBox::from_id_salt("ecad.project.props.revision_pending").selected_text(shown).show_ui(ui, |ui| {
                                for (w, label) in [("star", "D*"), ("draft", "E (draft)"), ("off", "Off")] {
                                    if ui.selectable_label(self.pending == w, label).clicked() {
                                        self.pending = w.to_string();
                                    }
                                }
                            });
                        });
                        ui.horizontal(|ui| {
                            ui.label("Columns");
                            ui.add(egui::TextEdit::singleline(&mut self.rev_columns).desired_width(260.0))
                                .accessible_name("ecad.project.props.revision_columns")
                                .on_hover_text("field[:Header][@width], … e.g. rev, date, ecr:ECR, description, drawn");
                        });
                        let t = self.rev_columns.trim();
                        if !t.is_empty()
                            && let Err(e) = ecad_cadcraft::revisions::parse_columns(t)
                        {
                            ui.label(RichText::new(e).small().color(RED));
                        }
                    });
                    // Brief 026 (design §5.1): numbering.
                    ui.separator();
                    ui.label(RichText::new("Numbering").small().strong());
                    ui.add_enabled_ui(!ro, |ui| {
                        ui.checkbox(&mut self.provisional, "Provisional tags until Finalise")
                            .accessible_name("ecad.settings.provisional")
                            .on_hover_text("New devices get -K?1, cables -W?1, new nets 24V-?; Finalise numbering numbers them in drawing order");
                    });
                    // Brief 031's text styles are Settings ▸ Text styles (brief 042).
                    // Brief 022 (owner, 2026-11-09): wire label colours and size.
                    ui.separator();
                    ui.label(RichText::new("Wires").small().strong());
                    ui.add_enabled_ui(!ro, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Colours");
                            for (word, shown, name) in [
                                ("iec", "IEC  BN BU GNYE", "ecad.projectsettings.colours.iec"),
                                ("three-letter", "BRN BLU GRN/YEL", "ecad.projectsettings.colours.three"),
                            ] {
                                if ui.radio(self.colour_style == word, shown).accessible_name(name).clicked() {
                                    self.colour_style = word.to_string();
                                }
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("Label height");
                            ui.add(egui::TextEdit::singleline(&mut self.label_height).desired_width(40.0))
                                .accessible_name("ecad.projectsettings.label_height");
                            ui.label(RichText::new("mm").small());
                        });
                        if let Some(w) = &mut self.labels {
                            w.show(ui);
                        }
                    });
                }
            });
            if let Some(e) = &self.error {
                ui.label(RichText::new(e).small().color(RED));
            }
            ui.horizontal(|ui| {
                let line = self.line();
                if !ro {
                    let ok = ui.add_enabled(line.is_ok(), egui::Button::new("OK")).accessible_name("ecad.projectprops.ok");
                    if ok.clicked() {
                        out = Some(match line {
                            Ok(Some(l)) => Out::Run(l),
                            _ => Out::Close,
                        });
                    }
                }
                if ui.button("Cancel").accessible_name("ecad.projectprops.cancel").clicked() {
                    out = Some(Out::Close);
                }
            });
        });
        out
    }
}

/// One row of the Revisions dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevRow {
    pub fields: [String; 6],
    /// Brief 033: the values of the project's custom columns ([`Revisions::custom`] order).
    pub extra: Vec<String>,
    pub issued: bool,
    /// Brief 027 (FP1): the release folder as recorded, and the folder it names once resolved
    /// against the project (read from disk when the window opens and after a change).
    pub released: Option<String>,
    pub folder: Option<std::path::PathBuf>,
    pub folder_exists: bool,
}

/// The Revisions dialog (design 019 §7.2): issued rows read-only, newest first; the pending row's
/// fields write `revision` when they lose focus; Issue asks first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Revisions {
    pub issued: Vec<RevRow>,
    pub pending: [String; 6],
    orig: [String; 6],
    /// Brief 033 (design 033 §7.3): the project's custom columns, (field, header), after the
    /// built-in six; the pending row's values and what was last sent.
    pub custom: Vec<(String, String)>,
    pub custom_pending: Vec<String>,
    custom_orig: Vec<String>,
    pub read_only: bool,
    /// The question of Issue Rev B… is open.
    pub confirming: bool,
    pub error: Option<String>,
    /// Brief 020a: shown on a branch other than a release line (its name): Issue is disabled,
    /// saying why, and Merge into main… is offered (design 020 §2.5.2, owner answer 10).
    pub off_main: Option<String>,
}

impl Revisions {
    /// Filled from `projectprops {}`'s result.
    pub fn from_json(v: &Value, read_only: bool) -> Self {
        let custom: Vec<(String, String)> = v
            .get("revision_columns")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|c| Some((c["field"].as_str()?.to_string(), c["header"].as_str()?.to_string())))
            .filter(|(f, _)| !ecad_cadcraft::revisions::FIELDS.contains(&f.as_str()))
            .collect();
        let rows: Vec<RevRow> = v
            .get("revisions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|r| RevRow {
                fields: ecad_cadcraft::revisions::FIELDS.map(|f| r[f].as_str().unwrap_or_default().to_string()),
                extra: custom.iter().map(|(f, _)| r[f.as_str()].as_str().unwrap_or_default().to_string()).collect(),
                issued: !r["issued"].is_null(),
                released: r.get("released").and_then(Value::as_str).map(str::to_string),
                folder: None,
                folder_exists: false,
            })
            .collect();
        let mut pending = rows.iter().rev().find(|r| !r.issued).map(|r| r.fields.clone()).unwrap_or_default();
        let custom_pending: Vec<String> =
            rows.iter().rev().find(|r| !r.issued).map(|r| r.extra.clone()).unwrap_or_else(|| vec![String::new(); custom.len()]);
        if pending[0].is_empty() {
            pending[0] = v.get("pending_rev").and_then(Value::as_str).unwrap_or("A").to_string();
        }
        let mut issued: Vec<RevRow> = rows.into_iter().filter(|r| r.issued).collect();
        issued.reverse();
        Revisions {
            issued,
            orig: pending.clone(),
            pending,
            custom_orig: custom_pending.clone(),
            custom_pending,
            custom,
            read_only,
            confirming: false,
            error: None,
            off_main: None,
        }
    }

    /// Take `fresh` (the project now), keeping what the user typed in the pending row and has not
    /// sent yet.
    pub fn refresh(&mut self, fresh: Revisions) {
        let typed: Vec<Option<String>> = self.pending.iter().zip(self.orig.iter()).map(|(p, o)| (p != o).then(|| p.clone())).collect();
        let typed_custom: Vec<(String, String)> = self
            .custom
            .iter()
            .zip(self.custom_pending.iter().zip(self.custom_orig.iter()))
            .filter(|(_, (p, o))| p != o)
            .map(|((f, _), (p, _))| (f.clone(), p.clone()))
            .collect();
        let (error, confirming) = (self.error.take(), self.confirming);
        *self = fresh;
        for (i, t) in typed.into_iter().enumerate() {
            if let (Some(t), Some(slot)) = (t, self.pending.get_mut(i)) {
                *slot = t;
            }
        }
        for (f, t) in typed_custom {
            if let Some(i) = self.custom.iter().position(|(g, _)| *g == f)
                && let Some(slot) = self.custom_pending.get_mut(i)
            {
                *slot = t;
            }
        }
        self.error = error;
        self.confirming = confirming;
    }

    /// Whether Issue can be pressed: Rev, Description and Drawn are filled (owner answer Q7).
    pub fn can_issue(&self) -> bool {
        [0, 2, 3].iter().all(|i| self.pending.get(*i).is_some_and(|v| !v.trim().is_empty()))
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Option<Out> {
        let mut out = None;
        let mut open = true;
        let mut headers: Vec<String> = ["Rev", "Date", "Description", "Drawn", "Checked", "Approved"].map(String::from).to_vec();
        headers.extend(self.custom.iter().map(|(_, h)| h.clone()));
        headers.push("Folder".into());
        let columns = headers.len() + 1;
        egui::Window::new("REVISIONS")
            .id(egui::Id::unique(REVISIONS_ID))
            .collapsible(false)
            .default_width(680.0)
            .min_width(620.0)
            .open(&mut open)
            .show(ctx, |ui| {
                egui::Grid::new("ecad.revisions.grid").num_columns(columns).striped(true).show(ui, |ui| {
                    for h in &headers {
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
                            let r = ui.add(egui::TextEdit::singleline(v).desired_width(w)).accessible_name(format!("ecad.revisions.pending.{f}"));
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
                        // Brief 033: the custom columns' inputs (`revision {fields}`).
                        for (i, (f, _)) in self.custom.iter().enumerate() {
                            let Some(v) = self.custom_pending.get_mut(i) else { continue };
                            let r = ui.add(egui::TextEdit::singleline(v).desired_width(60.0)).accessible_name(format!("ecad.revisions.pending.{f}"));
                            if r.lost_focus() && self.custom_orig.get(i) != Some(v) {
                                let value = if v.trim().is_empty() { Value::Null } else { Value::String(v.clone()) };
                                out = Some(Out::Run(format!("revision {}", json!({"fields": {f.as_str(): value}}))));
                                if let Some(o) = self.custom_orig.get_mut(i) {
                                    *o = v.clone();
                                }
                            }
                        }
                        ui.label("");
                        ui.label(RichText::new("pending").small());
                        ui.end_row();
                    }
                    for r in &self.issued {
                        for v in r.fields.iter().chain(r.extra.iter()) {
                            ui.label(v);
                        }
                        let rev = r.fields[0].clone();
                        // Brief 027 (design 027 §3): Open the release folder, or Write… it.
                        match (&r.folder, r.folder_exists) {
                            (Some(f), true) => {
                                if ui
                                    .small_button("Open")
                                    .on_hover_text(format!("Open {} in the file manager", f.display()))
                                    .accessible_name(format!("ecad.revisions.folder.{rev}"))
                                    .clicked()
                                {
                                    out = Some(Out::OpenFolder(f.clone()));
                                }
                            }
                            _ => {
                                if ui
                                    .small_button("Write…")
                                    .on_hover_text(format!("Write Rev {rev}'s release folder from Rev {rev} as issued; records nothing"))
                                    .accessible_name(format!("ecad.revisions.folder.{rev}"))
                                    .clicked()
                                {
                                    out = Some(Out::Release(Some(rev.clone())));
                                }
                            }
                        }
                        if ui
                            .button("Reopen")
                            .on_hover_text("Reopen this revision, read-only")
                            .accessible_name(format!("ecad.revisions.open.{rev}"))
                            .clicked()
                        {
                            out = Some(Out::Open(rev));
                        }
                        ui.end_row();
                    }
                });
                if let Some(e) = &self.error {
                    ui.label(RichText::new(e).small().color(RED));
                }
                ui.horizontal(|ui| {
                    if let Some(b) = self.off_main.clone() {
                        let rev = self.pending[0].clone();
                        ui.add_enabled(false, egui::Button::new(format!("Issue Rev {rev}…")))
                            .on_disabled_hover_text(format!("Revisions are issued on main. Merge \"{b}\" into main first."));
                        if ui.button("Switch to main").accessible_name("ecad.revisions.switchmain").clicked() {
                            out = Some(Out::SwitchToMain);
                        }
                        if ui.button("Merge into main…").accessible_name("ecad.revisions.merge").clicked() {
                            out = Some(Out::MergeIntoMain);
                        }
                    }
                    if !self.read_only {
                        let rev = self.pending[0].clone();
                        let b =
                            ui.add_enabled(self.can_issue(), egui::Button::new(format!("Issue Rev {rev}…"))).accessible_name("ecad.revisions.issue");
                        let b = if self.can_issue() { b } else { b.on_disabled_hover_text(ecad_cadcraft::revisions::FILL_FIRST) };
                        if b.clicked() {
                            self.confirming = true;
                        }
                        // Brief 027: issue and write the folder in one step (design 027 §3, Q1).
                        let r = ui
                            .add_enabled(self.can_issue(), egui::Button::new(RichText::new(format!("Release Rev {rev}…")).strong()))
                            .accessible_name("ecad.revisions.release");
                        let r = if self.can_issue() {
                            r.on_hover_text("Check, issue and write the release folder")
                        } else {
                            r.on_disabled_hover_text(ecad_cadcraft::revisions::FILL_FIRST)
                        };
                        if r.clicked() {
                            out = Some(Out::Release(None));
                        }
                    }
                    if ui.button("Close").clicked() {
                        out = Some(Out::Close);
                    }
                });
                if self.confirming {
                    let rev = self.pending[0].clone();
                    ui.separator();
                    ui.label(format!("Issue Rev {rev}? The project as it is now is kept as \"Rev {rev}\" and can be opened read-only."));
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
    /// Buttons pressed: `revisions`, `more`, `goto:<hex>`, `refgoto:<source>:<target>:<label>`.
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
    // Borrowed out for the frame (moved, not copied) and put back after.
    let view = std::mem::take(&mut sh.view);
    for s in &view.sections {
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
                        // Brief 028: a strip's plan, a cable's cores.
                        g if g.starts_with("showplan:") => "Show plan".to_string(),
                        g if g.starts_with("showcores:") => "Show cores".to_string(),
                        // Brief 025: `refgoto:<source>:<target>:<label>`.
                        g if g.starts_with("refgoto:") => g.splitn(4, ':').nth(3).unwrap_or("Go to").to_string(),
                        other => other.to_string(),
                    };
                    if ui.small_button(label).clicked() {
                        sh.buttons.push(b.clone());
                    }
                }
            });
        }
    }
    sh.view = view;
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
            ui.label(RichText::new(&row.value).color(colour)).on_hover_text(if sh.writable { "Read-only" } else { "The project is read-only." });
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
        ui.label(RichText::new(&e).small().color(RED));
        // Brief 026 (design §7.1): a taken tag offers Swap first, then the same device, then the
        // next free tag. Nothing is written until a button is clicked.
        if row.key == "tag"
            && let Some(tg) = target
            && let Some((typed, _)) = e.split_once(" is used by ")
        {
            let typed = typed.trim().to_string();
            let next = e.split("\"next\" (use ").nth(1).and_then(|r| r.split(')').next()).map(str::to_string);
            let same_kind = e.contains("\"swap\"");
            let noun = if matches!(tg, ecad_cadcraft::props::Target::Cable(_)) { "cable" } else { "device" };
            ui.horizontal(|ui| {
                if same_kind {
                    let swap = egui::Button::new(RichText::new("Swap").color(t.text)).fill(t.accent);
                    if ui.add(swap).accessible_name("ecad.tagclash.swap").clicked() {
                        sh.edits.push((tg.clone(), ecad_cadcraft::props::conflict_key("swap"), Some(typed.clone())));
                    }
                    if ui.button(format!("Make it the same {noun}")).accessible_name("ecad.tagclash.merge").clicked() {
                        sh.edits.push((tg.clone(), ecad_cadcraft::props::conflict_key("merge"), Some(typed.clone())));
                    }
                }
                let label = next.as_deref().map_or_else(|| "Use the next free".to_string(), |n| format!("Use {n}"));
                if ui.button(label).accessible_name("ecad.tagclash.next").clicked() {
                    sh.edits.push((tg.clone(), ecad_cadcraft::props::conflict_key("next"), Some(typed.clone())));
                }
            });
        }
    }
}

/// `Rev A, 2026-11-08: read-only` (the revision bar's text).
pub fn revision_bar(rev: &str, date: &str) -> String {
    if date.is_empty() { format!("Rev {rev}: read-only") } else { format!("Rev {rev}, {date}: read-only") }
}

// ---------------------------------------------------------------- wire labels (brief 032 §4.4)

/// The Project Settings rows of the wire label rule: one label per net per page or every wire, and
/// the shortest labelled wire (only with every wire). Committed as one `wiresettings` line.
#[derive(Debug, Clone, PartialEq)]
pub struct WireLabels {
    rule_orig: String,
    /// `net` or `each`.
    pub rule: String,
    min_orig: String,
    /// Shortest labelled wire, millimetres, as typed.
    pub min_length: String,
}

impl WireLabels {
    /// From `wiresettings {}`.
    pub fn from_json(v: &Value) -> Self {
        let rule = v.get("label_rule").and_then(Value::as_str).unwrap_or(ecad_model::wiring::DEFAULT_LABEL_RULE).to_string();
        let min = v.get("label_min_length").and_then(Value::as_f64).unwrap_or(ecad_model::wiring::DEFAULT_LABEL_MIN_LENGTH).to_string();
        WireLabels { rule_orig: rule.clone(), rule, min_orig: min.clone(), min_length: min }
    }

    /// The `wiresettings` line of what changed, `None` when nothing did.
    ///
    /// # Errors
    /// `Shortest labelled wire is 0 to 100 mm.`
    pub fn line(&self) -> Result<Option<String>, String> {
        let mut m = Map::new();
        if self.rule != self.rule_orig {
            m.insert("label_rule".into(), Value::String(self.rule.clone()));
        }
        if self.min_length.trim() != self.min_orig {
            let v: f64 = self
                .min_length
                .trim()
                .parse()
                .ok()
                .filter(|x| ecad_model::wiring::LABEL_MIN_LENGTHS.contains(x))
                .ok_or_else(|| "Shortest labelled wire is 0 to 100 mm.".to_string())?;
            m.insert("label_min_length".into(), json!(v));
        }
        Ok((!m.is_empty()).then(|| format!("wiresettings {}", Value::Object(m))))
    }

    fn show(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Labels");
            for (word, shown, name) in [
                (ecad_model::wiring::LABEL_RULE_NET, "One per net per page", "ecad.projectsettings.label_rule.net"),
                (ecad_model::wiring::LABEL_RULE_EACH, "Every wire", "ecad.projectsettings.label_rule.each"),
            ] {
                if ui.radio(self.rule == word, shown).accessible_name(name).clicked() {
                    self.rule = word.to_string();
                }
            }
        });
        ui.add_enabled_ui(self.rule == ecad_model::wiring::LABEL_RULE_EACH, |ui| {
            ui.horizontal(|ui| {
                ui.label("Shortest labelled wire");
                ui.add(egui::TextEdit::singleline(&mut self.min_length).desired_width(40.0)).accessible_name("ecad.projectsettings.label_min_length");
                ui.label(RichText::new("mm").small());
            });
        });
    }
}
