//! The Renumber dialog (brief 026, design 026 §3), also Finalise numbering (§5.3): what to number
//! (devices, cables, wires, all), which (all, the selection, a prefix or potential, a folder, the
//! provisional items), start and digits, and the preview of every row, old → new, before
//! anything is written. Apply runs `renumber` (or `finalise`) with the preview's change count:
//! one transaction; a stale preview is refused by the command and computed again here.
//!
//! Update map: the preview is computed when the dialog opens and when an option changes, never on
//! commits (design §11); a refused Apply asks for it again. Words are short (design §10): `Dev`,
//! `Cbl`, `Wire`, `Sel`, `Prov`, `At`, row marks `→ F = –`.

use eframe::egui::{self, Color32, RichText};
use serde_json::{Value, json};

/// The dialog's window id.
pub const DIALOG_ID: &str = "ecad.renumber.dialog";
/// Row height of the preview list.
const ROW_H: f32 = 16.0;

/// What to number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum What {
    Devices,
    Cables,
    Wires,
    /// Brief 028: terminal labels per strip.
    Terminals,
    All,
}

impl What {
    fn word(self) -> &'static str {
        match self {
            What::Devices => "devices",
            What::Cables => "cables",
            What::Wires => "wires",
            What::Terminals => "terminals",
            What::All => "all",
        }
    }
}

/// Which items.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which {
    All,
    Selection,
    Prefix,
    Folder,
    Provisional,
}

/// What the dialog asks the trial to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Out {
    /// Ask for the preview with these parameters (`renumber` or `finalise` with `preview`).
    Preview(&'static str, Value),
    /// Apply with these parameters.
    Apply(&'static str, Value),
    /// Show this row's anchor (page hex, point); the dialog stays open.
    Goto(String, [f64; 2]),
    /// Close without writing.
    Cancel,
}

/// The dialog's state.
#[derive(Debug, Clone, PartialEq)]
pub struct RenumberDialog {
    pub what: What,
    pub which: Which,
    /// Finalise numbering: all kinds, provisional, start `next` (read-only).
    pub finalise: bool,
    pub prefixes: Vec<String>,
    pub potentials: Vec<String>,
    pub prefix: String,
    /// `(folder hex, path)`.
    pub folders: Vec<(String, String)>,
    pub folder: usize,
    /// The selection's devices, cables and wires (hex IDs), when opened.
    pub selection: Vec<String>,
    pub provisional: usize,
    pub start: String,
    pub digits: String,
    pub new_tags: bool,
    pub read_only: bool,
    /// The latest preview.
    pub preview: Value,
    pub error: Option<String>,
}

impl RenumberDialog {
    /// A dialog with `choices` (from `numbering_cmds::choices`), the selection's objects and the
    /// project's digits.
    pub fn new(choices: &ecad_cadcraft::numbering_cmds::Choices, selection: Vec<String>, digits: u8, read_only: bool, finalise: bool) -> Self {
        let mut d = RenumberDialog {
            what: if finalise { What::All } else { What::Devices },
            which: if finalise { Which::Provisional } else { Which::All },
            finalise,
            prefixes: choices.prefixes.clone(),
            potentials: choices.potentials.clone(),
            prefix: choices.prefixes.first().cloned().unwrap_or_else(|| "-K".into()),
            folders: choices.folders.iter().map(|(f, p)| (f.hex(), p.clone())).collect(),
            folder: 0,
            selection,
            provisional: choices.provisional,
            start: if finalise { "next".into() } else { "1".into() },
            digits: digits.to_string(),
            new_tags: false,
            read_only,
            preview: Value::Null,
            error: None,
        };
        if finalise {
            d.start = "next".into();
        }
        d
    }

    /// Opened from Renumber Wires (022): Wires chosen.
    pub fn for_wires(mut self, digits: u8) -> Self {
        self.what = What::Wires;
        self.digits = digits.to_string();
        self
    }

    /// Opened from a folder row: Folder chosen.
    pub fn for_folder(mut self, folder: &str) -> Self {
        if let Some(i) = self.folders.iter().position(|(f, _)| f == folder) {
            self.which = Which::Folder;
            self.folder = i;
        }
        self
    }

    /// The command and its parameters for a preview (`preview`) or an Apply.
    ///
    /// # Errors
    /// Start or digits are not whole numbers in range (design §3's words).
    pub fn params(&self, preview: bool) -> Result<(&'static str, Value), String> {
        let mut v = json!({});
        if preview {
            v["preview"] = json!(true);
        } else if let Some(n) = self.preview["change_count"].as_u64() {
            v["based_on"] = json!(n);
        }
        if self.finalise {
            return Ok(("finalise", v));
        }
        v["kind"] = json!(self.what.word());
        v["scope"] = match self.which {
            Which::All => json!("all"),
            Which::Provisional => json!("provisional"),
            Which::Selection => {
                v["objects"] = json!(self.selection);
                json!("selection")
            }
            Which::Prefix => json!({"prefix": self.prefix}),
            Which::Folder => json!({"folder": self.folders.get(self.folder).map(|f| f.0.clone()).unwrap_or_default()}),
        };
        if self.which != Which::Provisional || self.start.trim() != "next" {
            let s = self.start.trim().parse::<u64>().ok().filter(|n| (1..=999_999_999).contains(n)).ok_or("Start is a whole number from 1.")?;
            v["start"] = json!(s);
        }
        let d = self.digits.trim().parse::<u8>().ok().filter(|n| (1..=6).contains(n)).ok_or("Digits is a whole number from 1 to 6.")?;
        v["digits"] = json!(d);
        if self.new_tags {
            v["digits_for_new"] = json!(true);
        }
        Ok(("renumber", v))
    }

    fn ask(&self) -> Option<Out> {
        self.params(true).ok().map(|(c, v)| Out::Preview(c, v))
    }

    /// The dialog (design §3).
    pub fn show(&mut self, ctx: &egui::Context) -> Option<Out> {
        let mut out = None;
        let mut changed = false;
        let red = Color32::from_rgb(0xe0, 0x60, 0x60);
        let orange = Color32::from_rgb(0xff, 0x7f, 0x00);
        let title = if self.finalise { "Finalise numbering" } else { "Renumber" };
        egui::Window::new(title)
            .id(egui::Id::unique(DIALOG_ID))
            .collapsible(false)
            .resizable(true)
            .default_width(560.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.style_mut().spacing.item_spacing = egui::vec2(8.0, 4.0);
                ui.label(RichText::new(title).small()).widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, DIALOG_ID));
                let fixed = self.finalise;
                ui.add_enabled_ui(!fixed, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("What").small());
                        for (w, label, name) in [
                            (What::Devices, "Dev", "devices"),
                            (What::Cables, "Cbl", "cables"),
                            (What::Wires, "Wire", "wires"),
                            (What::Terminals, "Trm", "terminals"),
                            (What::All, "All", "all"),
                        ] {
                            if ui.radio(self.what == w, RichText::new(label).small()).accessible_name(format!("ecad.renumber.kind.{name}")).clicked()
                            {
                                self.what = w;
                                if w == What::Wires && self.which == Which::Prefix {
                                    self.prefix = self.potentials.first().cloned().unwrap_or_default();
                                }
                                changed = true;
                            }
                        }
                    });
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("Which").small());
                        if ui.radio(self.which == Which::All, RichText::new("All").small()).accessible_name("ecad.renumber.scope.all").clicked() {
                            self.which = Which::All;
                            changed = true;
                        }
                        let n = self.selection.len();
                        let sel = ui
                            .add_enabled(n > 0, egui::RadioButton::new(self.which == Which::Selection, RichText::new(format!("Sel ({n})")).small()))
                            .accessible_name("ecad.renumber.scope.selection");
                        if sel.clicked() {
                            self.which = Which::Selection;
                            changed = true;
                        }
                        if ui
                            .radio(self.which == Which::Prefix, RichText::new("Prefix").small())
                            .accessible_name("ecad.renumber.scope.prefix")
                            .clicked()
                        {
                            self.which = Which::Prefix;
                            changed = true;
                        }
                        let list = if self.what == What::Wires { self.potentials.clone() } else { self.prefixes.clone() };
                        egui::ComboBox::from_id_salt("ecad.renumber.prefix").selected_text(RichText::new(&self.prefix).small().monospace()).show_ui(
                            ui,
                            |ui| {
                                for p in &list {
                                    if ui.selectable_label(*p == self.prefix, RichText::new(p).small().monospace()).clicked() {
                                        self.prefix = p.clone();
                                        self.which = Which::Prefix;
                                        changed = true;
                                    }
                                }
                            },
                        );
                        let has_folders = !self.folders.is_empty();
                        if ui
                            .add_enabled(has_folders, egui::RadioButton::new(self.which == Which::Folder, RichText::new("Folder").small()))
                            .accessible_name("ecad.renumber.scope.folder")
                            .clicked()
                        {
                            self.which = Which::Folder;
                            changed = true;
                        }
                        let shown = self.folders.get(self.folder).map(|f| f.1.clone()).unwrap_or_default();
                        egui::ComboBox::from_id_salt("ecad.renumber.folder").selected_text(RichText::new(shown).small()).show_ui(ui, |ui| {
                            for (i, (_, path)) in self.folders.iter().enumerate() {
                                if ui.selectable_label(i == self.folder, RichText::new(path).small()).clicked() {
                                    self.folder = i;
                                    self.which = Which::Folder;
                                    changed = true;
                                }
                            }
                        });
                        let prov = format!("Prov ({})", self.provisional);
                        if ui
                            .radio(self.which == Which::Provisional, RichText::new(prov).small())
                            .accessible_name("ecad.renumber.scope.provisional")
                            .clicked()
                        {
                            self.which = Which::Provisional;
                            changed = true;
                        }
                    });
                });
                ui.add_space(4.0);
                ui.label(RichText::new("Order  page · col · row").small());
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Start").small());
                    let next = self.finalise || (self.which == Which::Provisional && self.start.trim() == "next");
                    let r = ui.add_enabled(!next, egui::TextEdit::singleline(&mut self.start).desired_width(70.0).font(egui::TextStyle::Monospace));
                    let r = r.on_disabled_hover_text("Each prefix continues after its highest number in use.");
                    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, "ecad.renumber.start"));
                    if r.changed() {
                        changed = true;
                    }
                    ui.label(RichText::new("Digits").small());
                    let r = ui.add_enabled(
                        !self.finalise,
                        egui::TextEdit::singleline(&mut self.digits).desired_width(30.0).font(egui::TextStyle::Monospace),
                    );
                    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, "ecad.renumber.digits"));
                    if r.changed() {
                        changed = true;
                    }
                    if !self.finalise {
                        ui.checkbox(&mut self.new_tags, RichText::new("Also for new tags").small()).accessible_name("ecad.renumber.newtags");
                    }
                });
                if let Err(e) = self.params(true) {
                    ui.label(RichText::new(e).small().color(red));
                }
                ui.separator();
                let p = self.preview.clone();
                ui.label(RichText::new(p["summary"].as_str().unwrap_or_default()).small().strong());
                let rows: Vec<Value> = p["rows"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .chain(p["skipped"].as_array().cloned().unwrap_or_default())
                    .collect();
                let all = self.what == What::All || self.finalise;
                ui.label(
                    RichText::new(format!("{}{:<8} {:<3}{:<8} {:<10} Desc", if all { "     " } else { "" }, "Old", "", "New", "At"))
                        .small()
                        .monospace(),
                );
                egui::ScrollArea::vertical().id_salt("ecad.renumber.rows").max_height(320.0).auto_shrink([false, true]).show_rows(
                    ui,
                    ROW_H,
                    rows.len(),
                    |ui, range| {
                        for i in range {
                            let Some(r) = rows.get(i) else { continue };
                            let skipped = r.get("why").is_some_and(|w| !w.is_null()) || r.get("tag").is_some();
                            let (old, new, mark, at) = if r.get("tag").is_some() {
                                (r["tag"].as_str().unwrap_or_default().to_string(), String::new(), "–".to_string(), String::new())
                            } else {
                                (
                                    r["old"].as_str().unwrap_or_default().to_string(),
                                    r["new"].as_str().unwrap_or_default().to_string(),
                                    r["mark"].as_str().unwrap_or_default().to_string(),
                                    r["at"].as_str().unwrap_or_default().to_string(),
                                )
                            };
                            let new = if matches!(mark.as_str(), "→" | "F" | "=") { new } else { String::new() };
                            let why = r["why"].as_str().or_else(|| r["desc"].as_str()).unwrap_or_default();
                            let kind = match r["kind"].as_str() {
                                Some("devices") => "Dev  ",
                                Some("cables") => "Cbl  ",
                                Some("terminals") => "Trm  ",
                                _ => "Wire ",
                            };
                            let text = format!("{}{old:<8} {mark:<3}{new:<8} {at:<10} {why}", if all { kind } else { "" });
                            let colour = if old.contains('?') && !skipped { orange } else { ui.visuals().text_color() };
                            let resp = ui.add(egui::Label::new(RichText::new(text).small().monospace().color(colour)).sense(egui::Sense::click()));
                            resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("ecad.renumber.row.{i}")));
                            if resp.clicked()
                                && let (Some(page), Some(pt)) = (r["page"].as_str(), r["point"].as_array())
                            {
                                let x = pt.first().and_then(Value::as_f64).unwrap_or(0.0);
                                let y = pt.get(1).and_then(Value::as_f64).unwrap_or(0.0);
                                out = Some(Out::Goto(page.to_string(), [x, y]));
                            }
                        }
                    },
                );
                if p["more"].as_u64().unwrap_or(0) > 0 {
                    ui.label(RichText::new(format!("… {} more", p["more"])).small());
                }
                ui.separator();
                if let Some(e) = &self.error {
                    ui.label(RichText::new(e).small().color(red));
                }
                ui.horizontal(|ui| {
                    let n = p["changes"].as_u64().unwrap_or(0);
                    if self.read_only {
                        ui.label(RichText::new("Read-only: preview only.").small());
                    } else {
                        let ok = ui
                            .add_enabled(n > 0 && self.params(false).is_ok(), egui::Button::new(format!("Apply {n}")))
                            .accessible_name("ecad.renumber.apply");
                        let ok = ok.on_disabled_hover_text("Nothing to change.");
                        if ok.clicked()
                            && let Ok((c, v)) = self.params(false)
                        {
                            out = Some(Out::Apply(c, v));
                        }
                    }
                    if ui.button("Cancel").accessible_name("ecad.renumber.cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        out = Some(Out::Cancel);
                    }
                });
            });
        if out.is_none() && changed {
            out = self.ask();
        }
        out
    }
}
