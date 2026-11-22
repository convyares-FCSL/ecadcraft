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

use ecad_widgets::control::TextEntry;
use ecad_widgets::dialog::{Dialog, action};
use ecad_widgets::prim::{self, Button, Checkbox, Kind, Radio, Tone};
use ecad_widgets::tokens::{size, space};
use eframe::egui;
use serde_json::{Value, json};

/// Changes listed before Show all.
pub const CHANGES_SHOWN: usize = 20;
/// The undo-merge question's window id.
pub const UNDO_MERGE_ID: &str = "ecad.undomerge";

/// What the dialog asks the app to do.
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
        let p = self.preview.clone();
        let update = p["update"] == json!(true);
        let from = p["from"].as_str().unwrap_or_default().to_string();
        let title = p["title"].as_str().unwrap_or("MERGE").to_string();
        let nothing = p["nothing"] == json!(true);
        let unanswered = p["unanswered"].as_array().map_or(0, Vec::len);
        let main_word = if update { from.clone() } else { p["into"].as_str().unwrap_or("main").to_string() };
        let branch_word = if update { "this branch" } else { "branch" };
        // Why Merge cannot be pressed yet, if it cannot.
        let blocked = if let Some(pr) = p["problem"].as_str() {
            Some(pr.to_string())
        } else if unanswered > 0 {
            Some(format!(
                "Choose for {unanswered} conflict{} first, or use Keep {main_word} for all / Keep {branch_word} for all.",
                if unanswered == 1 { "" } else { "s" }
            ))
        } else {
            None
        };
        let target = self.target.clone();
        let shown = Dialog::new(egui::Id::unique("ecad.merge"), &title, "ecad.merge.dialog").medium().show(
            ctx,
            |ui| self.body(ui, &p),
            |ui| {
                // Laid right to left: the primary first.
                if nothing {
                    if !update && action(ui, "Close branch", Kind::Primary, "ecad.merge.close", None).clicked() {
                        let mut v = target.clone();
                        v["close"] = json!(true);
                        return Some(Out::Merge(v));
                    }
                    return action(ui, "Cancel", Kind::Secondary, "ecad.merge.cancel", None).clicked().then_some(Out::Cancel);
                }
                let label = if update { "Update" } else { "Merge" };
                if action(ui, label, Kind::Primary, "ecad.merge.ok", blocked.as_deref()).clicked() {
                    // Its parameters are made after the body's choices of this frame.
                    return Some(Out::Merge(Value::Null));
                }
                action(ui, "Cancel", Kind::Secondary, "ecad.merge.cancel", None).clicked().then_some(Out::Cancel)
            },
        );
        match shown.footer {
            Some(Out::Merge(Value::Null)) => Some(Out::Merge(self.params(false))),
            Some(o) => Some(o),
            None if shown.closed => Some(Out::Cancel),
            None => shown.body,
        }
    }

    /// The dialog's body: the header, the conflicts with their choices, the changes and what
    /// happens after. Returns a new preview when a choice changed.
    fn body(&mut self, ui: &mut egui::Ui, p: &Value) -> Option<Out> {
        let mut out = None;
        let update = p["update"] == json!(true);
        let into = p["into"].as_str().unwrap_or("main").to_string();
        let from = p["from"].as_str().unwrap_or_default().to_string();
        let (main_word, branch_word) = if update { (from.clone(), "this branch".to_string()) } else { (into, "branch".to_string()) };
        ui.spacing_mut().item_spacing.y = space::XXS;
        if let Some(n) = &self.switch_note {
            prim::body(ui, n, true);
        }
        prim::body(ui, p["header"].as_str().unwrap_or_default(), true)
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, "ecad.merge"));
        prim::body(ui, p["counts"].as_str().unwrap_or_default(), false);
        if p["nothing"] == json!(true) {
            let what = if update {
                format!("This branch already has {from}'s changes.")
            } else {
                format!("\"{from}\" has no changes since {}.", p["ancestor"].as_str().unwrap_or("its start"))
            };
            prim::body(ui, &what, false);
            return None;
        }
        let conflicts = self.conflicts();
        egui::ScrollArea::vertical().id_salt("ecad.merge.body").max_height(BODY_HEIGHT).show(ui, |ui| {
            if !conflicts.is_empty() {
                ui.add_space(space::S);
                ui.horizontal(|ui| {
                    prim::caption(ui, &format!("CONFLICTS ({})", conflicts.len()));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let all_branch = format!("Keep {branch_word} for all");
                        if ui.add(Button::new(&all_branch).small()).accessible_name("ecad.merge.allbranch").clicked() {
                            self.all("branch");
                            out = Some(Out::Preview(self.params(true)));
                        }
                        let all_main = format!("Keep {main_word} for all");
                        if ui.add(Button::new(&all_main).small()).accessible_name("ecad.merge.allmain").clicked() {
                            self.all("main");
                            out = Some(Out::Preview(self.params(true)));
                        }
                    });
                });
            }
            for c in &conflicts {
                let id = c["id"].as_str().unwrap_or_default().to_string();
                let kind = c["kind"].as_str().unwrap_or_default();
                ui.add_space(space::XS);
                prim::note(ui, &format!("\u{25b2} {}", c["text"].as_str().unwrap_or_default()), Tone::Warning)
                    .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, format!("ecad.merge.conflict.{id}")));
                if matches!(kind, "value" | "removed" | "order") {
                    ui.horizontal(|ui| {
                        ui.add_space(space::L);
                        prim::body(ui, &format!("{main_word}: {}", c["main"].as_str().unwrap_or_default()), true);
                        ui.add_space(space::XL);
                        prim::body(ui, &format!("{branch_word}: {}", c["branch"].as_str().unwrap_or_default()), true);
                    });
                }
                let chosen = self.choices.get(&id).cloned();
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(space::L);
                    for w in c["choices"].as_array().cloned().unwrap_or_default() {
                        if w.is_object() {
                            // Edit: a field prefilled with the branch's value.
                            let mut typed = self.edits.get(&id).cloned().unwrap_or_else(|| c["branch"].as_str().unwrap_or_default().to_string());
                            let on = chosen.as_ref().is_some_and(Value::is_object);
                            let picked = Radio::new(on, "Edit").show(ui).accessible_name(format!("ecad.merge.conflict.{id}.edit")).clicked();
                            let name = format!("ecad.merge.conflict.{id}.value");
                            let r = TextEntry::new(&mut typed, &name).width(EDIT_WIDTH).show(ui);
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
                                if c["kind"] != "needed" || c["main"].as_str() == Some("leave unconnected") {
                                    "Leave unconnected".into()
                                } else {
                                    "Leave it out".to_string()
                                },
                                "loose",
                            ),
                            "renumber_main" => (cap(c["main"].as_str().unwrap_or("renumber main's")), "renumber"),
                            "renumber_branch" => (cap(c["branch"].as_str().unwrap_or("renumber the branch's")), "renumber"),
                            // Brief 026 (design §8): the third choice of a tag conflict.
                            "one" => (one_label(c).to_string(), "one"),
                            other => (other.to_string(), "other"),
                        };
                        let on = chosen.as_ref().is_some_and(|x| *x == w);
                        if Radio::new(on, &label).show(ui).accessible_name(format!("ecad.merge.conflict.{id}.{name}")).clicked() {
                            self.choices.insert(id.clone(), w.clone());
                            out = Some(Out::Preview(self.params(true)));
                        }
                    }
                    // Brief 026: Make them one, greyed with the reason when they cannot be one.
                    if let Some(why) = c["unavailable"]["why"].as_str() {
                        ui.add_enabled_ui(false, |ui| Radio::new(false, one_label(c)).show(ui))
                            .inner
                            .accessible_name(format!("ecad.merge.conflict.{id}.one"))
                            .on_disabled_hover_text(why);
                        prim::note(ui, why, Tone::Neutral);
                    }
                });
            }
            let changes = p["changes"].as_array().cloned().unwrap_or_default();
            ui.add_space(space::S);
            let whose = if update { from.to_uppercase() } else { "THE BRANCH".to_string() };
            prim::caption(ui, &format!("CHANGES FROM {whose} ({})", changes.len()));
            let shown = if self.show_all { changes.len() } else { changes.len().min(CHANGES_SHOWN) };
            for ch in changes.iter().take(shown) {
                let page = ch["page"].as_str().map(|p| format!("page {p}")).unwrap_or_default();
                ui.horizontal(|ui| {
                    prim::body(ui, &format!("{} {}", ch["sign"].as_str().unwrap_or("~"), ch["text"].as_str().unwrap_or_default()), false);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        prim::body(ui, &page, true);
                    });
                });
            }
            if changes.len() > shown {
                ui.horizontal(|ui| {
                    prim::body(ui, &format!("\u{2026} {} more", changes.len() - shown), true);
                    if ui.add(Button::new("Show all").kind(Kind::Quiet).small()).accessible_name("ecad.merge.showall").clicked() {
                        self.show_all = true;
                    }
                });
            }
        });
        ui.add_space(space::S);
        prim::caption(ui, "AFTER THE MERGE");
        prim::body(ui, p["after"].as_str().unwrap_or_default(), true);
        if !update {
            let label = format!("Close branch \"{from}\" after merging");
            Checkbox::new(&mut self.close_branch, &label).show(ui).accessible_name("ecad.merge.close");
        }
        if let Some(e) = &self.error {
            prim::note(ui, e, Tone::Error);
        }
        out
    }
}

/// The height of the scrolled part of the merge dialog: conflicts and changes.
const BODY_HEIGHT: f32 = size::DIALOG_S;
/// The width of an Edit choice's field.
const EDIT_WIDTH: f32 = size::LABEL_COLUMN + size::CONTROL;

/// Make them one's label: a cable or a device.
fn one_label(c: &Value) -> &'static str {
    if c["text"].as_str().is_some_and(|t| t.contains(" cable ")) { "Make them one cable" } else { "Make them one device" }
}

fn cap(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or_else(String::new, |f| f.to_uppercase().collect::<String>() + c.as_str())
}

/// The question a plain undo of a merge stops at (design 020 §4.3): `Some(true)` to undo it.
pub fn undo_merge(ctx: &egui::Context, question: &str) -> Option<bool> {
    let update = question.contains("Undo the update?");
    let title = if update { "Undo the update?" } else { "Undo the merge?" };
    let shown = Dialog::new(egui::Id::unique(UNDO_MERGE_ID), title, UNDO_MERGE_ID).small().show(
        ctx,
        |ui| prim::body(ui, question, false),
        |ui| {
            if action(ui, if update { "Undo update" } else { "Undo merge" }, Kind::Danger, "ecad.undomerge.ok", None).clicked() {
                Some(true)
            } else if action(ui, "Cancel", Kind::Secondary, "ecad.undomerge.cancel", None).clicked() {
                Some(false)
            } else {
                None
            }
        },
    );
    shown.footer.or(shown.closed.then_some(false))
}
