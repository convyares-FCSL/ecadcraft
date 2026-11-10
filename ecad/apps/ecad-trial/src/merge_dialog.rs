//! The merge dialog (brief 020b, design 020 §4.2) and the question an undo of a merge stops at
//! (§4.3). The dialog shows the `merge {preview: true}` JSON; each choice asks for the preview
//! again with the choices so far (the prepared merge is kept, so nothing is read from the file
//! again), and Merge runs `merge {from, into, choices, close}`: one merge step.
//!
//! Nothing is preselected (owner answer 7): Merge is enabled when every conflict has a choice;
//! Keep main for all / Keep branch for all fill every unanswered conflict that offers it.
//!
//! Update map: built from the preview when opened; changed only by its own choices (a new
//! preview); every other change does nothing while it is open (it is modal over the target).

use std::collections::BTreeMap;

use eframe::egui::{self, Color32, RichText};
use serde_json::{Value, json};

/// Changes listed before Show all.
pub const CHANGES_SHOWN: usize = 20;
/// The undo-merge question's window id.
pub const UNDO_MERGE_ID: &str = "ecad.undomerge";

/// What the dialog asks the trial to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Out {
    /// Ask for the preview again with these parameters (choices changed).
    Preview(Value),
    /// Run the merge with these parameters.
    Merge(Value),
    /// Close without writing.
    Cancel,
}

/// The dialog's state.
#[derive(Debug, Clone, PartialEq)]
pub struct MergeDialog {
    /// `{from, into}`.
    pub target: Value,
    /// The latest preview.
    pub preview: Value,
    /// Choices so far, by conflict ID, as command words.
    pub choices: BTreeMap<String, Value>,
    /// Text typed in Edit fields, by conflict ID.
    pub edits: BTreeMap<String, String>,
    pub close_branch: bool,
    pub show_all: bool,
    /// Said first when the merge switches branch (`This switches to main …`).
    pub switch_note: Option<String>,
    pub error: Option<String>,
}

impl MergeDialog {
    /// A dialog for `target` (`{from, into?}`) with its first preview.
    pub fn new(target: Value, preview: Value, switch_note: Option<String>) -> Self {
        Self { target, preview, choices: BTreeMap::new(), edits: BTreeMap::new(), close_branch: true, show_all: false, switch_note, error: None }
    }

    /// The parameters of a preview or merge with the choices so far.
    pub fn params(&self, preview: bool) -> Value {
        let mut v = self.target.clone();
        if !self.choices.is_empty() {
            v["choices"] = json!(self.choices);
        }
        if preview {
            v["preview"] = json!(true);
        } else if self.preview["update"] != json!(true) {
            v["close"] = json!(self.close_branch);
        }
        v
    }

    fn conflicts(&self) -> Vec<Value> {
        self.preview["conflicts"].as_array().cloned().unwrap_or_default()
    }

    /// Fill every unanswered conflict that offers `word` with it.
    fn all(&mut self, word: &str) {
        for c in self.conflicts() {
            let id = c["id"].as_str().unwrap_or_default().to_string();
            if !self.choices.contains_key(&id) && c["choices"].as_array().is_some_and(|a| a.iter().any(|x| x == word)) {
                self.choices.insert(id, json!(word));
            }
        }
    }

    /// The dialog (design §4.2).
    pub fn show(&mut self, ctx: &egui::Context) -> Option<Out> {
        let mut out = None;
        let p = self.preview.clone();
        let update = p["update"] == json!(true);
        let into = p["into"].as_str().unwrap_or("main").to_string();
        let from = p["from"].as_str().unwrap_or_default().to_string();
        let (main_word, branch_word) = if update { (from.clone(), "this branch".to_string()) } else { (into.clone(), "branch".to_string()) };
        let red = Color32::from_rgb(0xe0, 0x60, 0x60);
        let warn = Color32::from_rgb(0xe0, 0xb0, 0x40);
        let title = p["title"].as_str().unwrap_or("MERGE").to_string();
        egui::Window::new(title)
            .id(egui::Id::unique("ecad.merge"))
            .collapsible(false)
            .resizable(true)
            .default_width(640.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 3.0;
                if let Some(n) = &self.switch_note {
                    ui.label(RichText::new(n).small());
                }
                ui.label(RichText::new(p["header"].as_str().unwrap_or_default()).small())
                    .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, "ecad.merge"));
                ui.label(RichText::new(p["counts"].as_str().unwrap_or_default()).small().strong());
                if p["nothing"] == json!(true) {
                    let what = if update {
                        format!("This branch already has {from}'s changes.")
                    } else {
                        format!("\"{from}\" has no changes since {}.", p["ancestor"].as_str().unwrap_or("its start"))
                    };
                    ui.label(what);
                    ui.horizontal(|ui| {
                        if !update && ui.button("Close branch").accessible_name("ecad.merge.close").clicked() {
                            let mut v = self.target.clone();
                            v["close"] = json!(true);
                            out = Some(Out::Merge(v));
                        }
                        if ui.button("Cancel").accessible_name("ecad.merge.cancel").clicked() {
                            out = Some(Out::Cancel);
                        }
                    });
                    return;
                }
                let conflicts = self.conflicts();
                egui::ScrollArea::vertical().id_salt("ecad.merge.body").max_height(420.0).show(ui, |ui| {
                    if !conflicts.is_empty() {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("── Conflicts ({})", conflicts.len())).small().strong());
                            if ui.small_button(format!("Keep {main_word} for all")).accessible_name("ecad.merge.allmain").clicked() {
                                self.all("main");
                                out = Some(Out::Preview(self.params(true)));
                            }
                            if ui.small_button(format!("Keep {branch_word} for all")).accessible_name("ecad.merge.allbranch").clicked() {
                                self.all("branch");
                                out = Some(Out::Preview(self.params(true)));
                            }
                        });
                    }
                    for c in &conflicts {
                        let id = c["id"].as_str().unwrap_or_default().to_string();
                        let kind = c["kind"].as_str().unwrap_or_default();
                        ui.label(RichText::new(format!("\u{25b2} {}", c["text"].as_str().unwrap_or_default())).small().color(warn))
                            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, format!("ecad.merge.conflict.{id}")));
                        if matches!(kind, "value" | "removed" | "order") {
                            ui.label(
                                RichText::new(format!(
                                    "    {main_word}: {}        {branch_word}: {}",
                                    c["main"].as_str().unwrap_or_default(),
                                    c["branch"].as_str().unwrap_or_default()
                                ))
                                .small(),
                            );
                        }
                        let chosen = self.choices.get(&id).cloned();
                        ui.horizontal_wrapped(|ui| {
                            ui.add_space(12.0);
                            for w in c["choices"].as_array().cloned().unwrap_or_default() {
                                if w.is_object() {
                                    // Edit: a field prefilled with the branch's value.
                                    let mut typed =
                                        self.edits.get(&id).cloned().unwrap_or_else(|| c["branch"].as_str().unwrap_or_default().to_string());
                                    let on = chosen.as_ref().is_some_and(Value::is_object);
                                    let picked = ui.radio(on, "Edit").accessible_name(format!("ecad.merge.conflict.{id}.edit")).clicked();
                                    let r = ui.add(egui::TextEdit::singleline(&mut typed).desired_width(120.0));
                                    let typed_done = r.lost_focus() && on;
                                    self.edits.insert(id.clone(), typed.clone());
                                    if picked || typed_done {
                                        self.choices.insert(id.clone(), json!({"value": typed}));
                                        out = Some(Out::Preview(self.params(true)));
                                    }
                                    continue;
                                }
                                let word = w.as_str().unwrap_or_default().to_string();
                                let (label, name) = match word.as_str() {
                                    "main" => (format!("Keep {main_word}"), "main"),
                                    "branch" => (format!("Keep {branch_word}"), "branch"),
                                    "bringback" => ("Bring it back".to_string(), "bringback"),
                                    "loose" => (
                                        if c["main"].as_str() == Some("leave unconnected") {
                                            "Leave unconnected".into()
                                        } else {
                                            "Leave it out".to_string()
                                        },
                                        "loose",
                                    ),
                                    "renumber_main" => (cap(c["main"].as_str().unwrap_or("renumber main's")), "renumber"),
                                    "renumber_branch" => (cap(c["branch"].as_str().unwrap_or("renumber the branch's")), "renumber"),
                                    other => (other.to_string(), "other"),
                                };
                                let on = chosen.as_ref().is_some_and(|x| *x == w);
                                if ui.radio(on, label).accessible_name(format!("ecad.merge.conflict.{id}.{name}")).clicked() {
                                    self.choices.insert(id.clone(), w.clone());
                                    out = Some(Out::Preview(self.params(true)));
                                }
                            }
                        });
                    }
                    let changes = p["changes"].as_array().cloned().unwrap_or_default();
                    ui.label(
                        RichText::new(format!("── Changes from {} ({})", if update { from.as_str() } else { "the branch" }, changes.len()))
                            .small()
                            .strong(),
                    );
                    let shown = if self.show_all { changes.len() } else { changes.len().min(CHANGES_SHOWN) };
                    for ch in changes.iter().take(shown) {
                        let page = ch["page"].as_str().map(|p| format!("page {p}")).unwrap_or_default();
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("{} {}", ch["sign"].as_str().unwrap_or("~"), ch["text"].as_str().unwrap_or_default()))
                                    .small()
                                    .monospace(),
                            );
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(RichText::new(page).small());
                            });
                        });
                    }
                    if changes.len() > shown {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("  … {} more", changes.len() - shown)).small());
                            if ui.small_button("Show all").accessible_name("ecad.merge.showall").clicked() {
                                self.show_all = true;
                            }
                        });
                    }
                });
                ui.label(RichText::new("── After the merge").small().strong());
                ui.label(RichText::new(p["after"].as_str().unwrap_or_default()).small());
                if !update {
                    ui.checkbox(&mut self.close_branch, format!("Close branch \"{from}\" after merging")).accessible_name("ecad.merge.close");
                }
                if let Some(e) = &self.error {
                    ui.label(RichText::new(e).small().color(red));
                }
                let unanswered = p["unanswered"].as_array().map_or(0, Vec::len);
                ui.horizontal(|ui| {
                    let ok =
                        ui.add_enabled(unanswered == 0, egui::Button::new(if update { "Update" } else { "Merge" })).accessible_name("ecad.merge.ok");
                    let ok = if unanswered == 0 {
                        ok
                    } else {
                        ok.on_disabled_hover_text(format!(
                            "Choose for {unanswered} conflict{} first, or use Keep {main_word} for all / Keep {branch_word} for all.",
                            if unanswered == 1 { "" } else { "s" }
                        ))
                    };
                    if ok.clicked() {
                        out = Some(Out::Merge(self.params(false)));
                    }
                    if ui.button("Cancel").accessible_name("ecad.merge.cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        out = Some(Out::Cancel);
                    }
                });
            });
        out
    }
}

fn cap(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or_else(String::new, |f| f.to_uppercase().collect::<String>() + c.as_str())
}

/// The question a plain undo of a merge stops at (design 020 §4.3): `Some(true)` to undo it.
pub fn undo_merge(ctx: &egui::Context, question: &str) -> Option<bool> {
    let mut out = None;
    let update = question.contains("Undo the update?");
    egui::Window::new(if update { "Undo the update?" } else { "Undo the merge?" })
        .id(egui::Id::unique(UNDO_MERGE_ID))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(question);
            ui.horizontal(|ui| {
                if ui.button(if update { "Undo update" } else { "Undo merge" }).accessible_name("ecad.undomerge.ok").clicked() {
                    out = Some(true);
                }
                if ui.button("Cancel").accessible_name("ecad.undomerge.cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    out = Some(false);
                }
            });
        });
    out
}
