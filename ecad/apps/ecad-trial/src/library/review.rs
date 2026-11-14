//! "Before saving" (design 048 §6.1 step 3, screen k): the drawer's body after Save of an edit.
//! One row per placed copy (`symbolimpact`'s rows): its tag, page, what changes there and the
//! outcome, a tick per row (a clashing row cannot be ticked), and the choice Update all · Update
//! selected · Save as a new symbol. Update selected is pre-selected when any row clashes (owner
//! answer 10). Save writes `symboledit` with the choice, one transaction.
//!
//! Compare (design 048 §8.4) is the same body for one copy, with no Save.

use eframe::egui::{self, Color32, RichText};
use serde_json::{Value, json};

/// Element names (design 048 §6.1).
pub const BODY: &str = "drawer.symbol-review";
pub const HEADER: &str = "drawer.symbol-review.header";
pub const SAVE: &str = "drawer.symbol-review.save";
pub const BACK: &str = "drawer.symbol-review.back";
pub const BUILTINS: &str = "drawer.symbol-review.builtins";
pub const CLOSE: &str = "drawer.symbol-review.close";
pub const UPDATE: &str = "drawer.symbol-review.update";
pub const REFUSAL: &str = "drawer.symbol-review.refusal";
/// The drawer's tab while the review shows.
pub const TAB: &str = "Before saving";
pub const COMPARE_TAB: &str = "Compare";

/// The three choices (design 048 §6.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    All,
    Selected,
    New,
}

impl Choice {
    pub fn word(self) -> &'static str {
        match self {
            Choice::All => "all",
            Choice::Selected => "selected",
            Choice::New => "new",
        }
    }
}

/// One placed copy's row.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub rep: String,
    pub tag: String,
    pub page_number: String,
    pub changes: Vec<String>,
    pub clash: Option<String>,
    pub builtin: bool,
    pub checked: bool,
    /// The device's description, read from the project after the review is made.
    pub description: String,
}

impl Row {
    /// The line on what changes there (`Point 3 added · A2 moves 2.5 mm, wire 24V-05 follows`).
    pub fn line(&self) -> String {
        match &self.clash {
            Some(c) => c.clone(),
            None if self.changes.is_empty() => "Nothing changes there.".into(),
            None => self.changes.join(" · "),
        }
    }
}

/// The review's state.
#[derive(Debug, Clone, PartialEq)]
pub struct Review {
    pub name: String,
    pub version: u64,
    pub pages: u64,
    pub rows: Vec<Row>,
    pub choice: Choice,
    /// An edited built-in: "Also update the N placed built-in copies".
    pub builtins: bool,
    pub builtin_edit: bool,
    /// The content and `definition` the edit saves.
    params: Value,
    /// Save's refusal.
    pub refusal: Option<String>,
    /// Compare: one copy, no Save.
    pub compare: bool,
}

fn s(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or_default().to_string()
}

impl Review {
    /// The review of `symbolimpact`'s answer `v` for the edit `params`.
    pub fn of(v: &Value, params: Value, builtin_edit: bool, name: &str) -> Self {
        let rows: Vec<Row> = v
            .get("rows")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|r| {
                let clash = r.get("clash").and_then(Value::as_str).map(str::to_string);
                let builtin = r.get("builtin").and_then(Value::as_bool).unwrap_or(false);
                Row {
                    rep: s(r, "rep"),
                    tag: s(r, "tag"),
                    page_number: s(r, "page_number"),
                    changes: r.get("changes").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect(),
                    checked: clash.is_none() && !builtin,
                    description: String::new(),
                    clash,
                    builtin,
                }
            })
            .collect();
        let clashes = rows.iter().any(|r| r.clash.is_some());
        Review {
            name: name.to_string(),
            version: v.get("version").and_then(Value::as_u64).unwrap_or(1),
            pages: v.get("pages").and_then(Value::as_u64).unwrap_or(0),
            rows,
            choice: if clashes { Choice::Selected } else { Choice::All },
            builtins: false,
            builtin_edit,
            params,
            refusal: None,
            compare: false,
        }
    }

    /// Fill each row's device description from `doc`.
    pub fn describe(&mut self, doc: &ecad_model::Document) {
        for r in &mut self.rows {
            let rep = u128::from_str_radix(&r.rep, 16).ok().map(ecad_model::RepresentationId::from);
            if let Some(ecad_model::Subject::Component(c)) = rep.and_then(|x| doc.representations.get(&x)).map(|x| x.subject) {
                r.description = ecad_cadcraft::devices::description(doc, c);
            }
        }
    }

    /// Keep only copy `rep`'s row (Compare).
    pub fn only(&mut self, rep: ecad_model::RepresentationId) {
        let h = rep.hex();
        self.rows.retain(|r| r.rep == h);
    }

    pub fn clashes(&self) -> usize {
        self.rows.iter().filter(|r| r.clash.is_some()).count()
    }

    fn builtin_rows(&self) -> usize {
        self.rows.iter().filter(|r| r.builtin).count()
    }

    /// The header: `Coil v3 → v4 · 12 placed copies on 7 pages · 2 would clash`.
    pub fn header(&self) -> String {
        let from = self.version.saturating_sub(1);
        let n = self.rows.len();
        let mut h = if self.builtin_edit {
            format!("{} (built-in) → project copy v{}", self.name, self.version)
        } else {
            format!("{} v{from} → v{}", self.name, self.version)
        };
        h.push_str(&format!(
            " · {n} placed {} on {} {}",
            if n == 1 { "copy" } else { "copies" },
            self.pages,
            if self.pages == 1 { "page" } else { "pages" }
        ));
        let c = self.clashes();
        if c > 0 {
            h.push_str(&format!(" · {c} would clash"));
        }
        h
    }

    /// The outcome of a row under the choice: `Updated`, or keeps its version.
    pub fn outcome(&self, r: &Row) -> (String, bool) {
        let updated = match self.choice {
            Choice::All => r.clash.is_none() && (!r.builtin || self.builtins),
            Choice::Selected => r.checked && r.clash.is_none(),
            Choice::New => false,
        };
        if updated {
            ("Updated".into(), true)
        } else if self.choice == Choice::New {
            ("Unchanged".into(), false)
        } else {
            (format!("Keeps v{}, update available", self.version.saturating_sub(1).max(1)), false)
        }
    }

    /// Why Save cannot be pressed, if it cannot.
    pub fn save_refusal(&self) -> Option<String> {
        if self.choice == Choice::All && self.clashes() > 0 {
            return Some("Update all is off while a copy would clash: update selected, or move or remove the wires first.".into());
        }
        None
    }

    /// `symboledit`'s parameters for the choice.
    pub fn edit_params(&self) -> Value {
        let mut p = self.params.clone();
        p["update"] = json!(self.choice.word());
        if self.choice == Choice::Selected {
            p["copies"] = json!(self.rows.iter().filter(|r| r.checked && r.clash.is_none()).map(|r| r.rep.clone()).collect::<Vec<_>>());
        }
        p["builtins"] = json!(self.builtins);
        p
    }
}

/// What the drawer's review asks of the window.
#[derive(Debug, Clone, PartialEq)]
pub enum Act {
    Save,
    /// Back to the drawing (the review closes, the draft stays).
    Back,
    /// Go to that copy (Back returns).
    GoTo(String),
    /// Compare: update this copy.
    Update(String),
    Close,
}

fn c32(c: ecad_ui::tokens::Color) -> Color32 {
    ecad_ui::color(c)
}

/// The review's body in the drawer.
pub fn show(ui: &mut egui::Ui, r: &mut Review, writable: bool) -> Vec<Act> {
    let shell = ecad_ui::SHELL;
    let mut acts = Vec::new();
    ui.horizontal(|ui| {
        ui.label(RichText::new(r.header()).strong()).widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, HEADER));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if r.compare {
                let c = ui.add(ecad_ui::prim::Button::new("Close").kind(ecad_ui::prim::Kind::Quiet).small());
                c.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, CLOSE));
                if c.clicked() {
                    acts.push(Act::Close);
                }
            }
        });
    });
    if let Some(e) = &r.refusal {
        ui.label(RichText::new(e).color(c32(shell.error)).small()).widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, REFUSAL));
    }
    if r.builtin_edit && !r.compare && r.builtin_rows() > 0 {
        let label = format!("Also update the {} placed built-in copies", r.builtin_rows());
        let resp = ui.checkbox(&mut r.builtins, label);
        resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, r.builtins, BUILTINS));
    }
    let weak = c32(shell.text_2);
    let ok = c32(shell.ok);
    let warn = c32(shell.warning);
    let error = c32(shell.error);
    let choice = r.choice;
    let rows: Vec<(usize, Row)> = r.rows.iter().cloned().enumerate().collect();
    let table_h = (ui.available_height() - if r.compare { 0.0 } else { 34.0 }).max(40.0);
    egui::ScrollArea::vertical().id_salt("drawer.symbol-review.scroll").max_height(table_h).auto_shrink([false, true]).show(ui, |ui| {
        egui::Grid::new("drawer.symbol-review.grid").num_columns(5).striped(true).spacing([16.0, 4.0]).show(ui, |ui| {
            ui.label("");
            for h in ["Placed symbol", "Page", "What changes there", "Outcome"] {
                ui.label(RichText::new(h).small().strong().color(weak));
            }
            ui.end_row();
            for (i, row) in rows {
                let name = format!("{BODY}.row.{}", row.rep);
                if r.compare {
                    ui.label("");
                } else {
                    let can = row.clash.is_none() && choice == Choice::Selected;
                    let mut checked = row.checked && row.clash.is_none();
                    let tip = row.clash.as_ref().map(|_| "Move or remove the wire first".to_string());
                    let cb = ui.add_enabled(can, egui::Checkbox::without_text(&mut checked));
                    let cb = match tip {
                        Some(t) => cb.on_disabled_hover_text(t),
                        None => cb,
                    };
                    cb.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, can, checked, format!("{name}.check")));
                    if let Some(x) = r.rows.get_mut(i) {
                        x.checked = checked;
                    }
                }
                let shown = if row.description.is_empty() { row.tag.clone() } else { format!("{} {}", row.tag, row.description) };
                let tag = ui.add(egui::Label::new(RichText::new(shown).small()).sense(egui::Sense::click()));
                tag.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Link, true, &name));
                if tag.clicked() {
                    acts.push(Act::GoTo(row.rep.clone()));
                }
                ui.label(RichText::new(&row.page_number).small());
                ui.horizontal(|ui| {
                    if row.clash.is_some() {
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                        ui.painter().circle_filled(rect.center(), 4.0, error);
                    }
                    ui.label(RichText::new(row.line()).small().color(if row.clash.is_some() { error } else { c32(shell.text) }));
                });
                let (text, updated) = if r.compare {
                    (if row.clash.is_some() { "Would clash".to_string() } else { "Can be updated".to_string() }, row.clash.is_none())
                } else {
                    r.outcome(&row)
                };
                ui.label(RichText::new(text).small().color(if updated { ok } else { warn }));
                if r.compare && row.clash.is_none() && writable {
                    let u = ui.add(ecad_ui::prim::Button::new("Update").small());
                    u.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, UPDATE));
                    if u.clicked() {
                        acts.push(Act::Update(row.rep.clone()));
                    }
                }
                ui.end_row();
            }
        });
    });
    if !r.compare {
        // The choice (design 048 §6.1), as screen k's pills, then Save and Back.
        ui.horizontal(|ui| {
            let n = r.rows.len();
            let picked = r.rows.iter().filter(|x| x.checked && x.clash.is_none()).count();
            for (c, label) in [
                (Choice::All, format!("Update all {n}")),
                (Choice::Selected, format!("Update selected, {picked}")),
                (Choice::New, "Save as a new symbol".to_string()),
            ] {
                let off = c == Choice::All && r.clashes() > 0;
                let resp = ui.add_enabled_ui(!off, |ui| ecad_ui::prim::Chip::new(&label).selected(r.choice == c).show(ui).response).inner;
                resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::RadioButton, !off, r.choice == c, format!("{BODY}.choice.{}", c.word())));
                if resp.clicked() && !off {
                    r.choice = c;
                }
            }
            ui.add_space(24.0);
            let why = if writable { r.save_refusal() } else { Some("Writes are off for this project.".to_string()) };
            let mut save = ecad_ui::prim::Button::new("Save").kind(ecad_ui::prim::Kind::Primary).small();
            if let Some(w) = &why {
                save = save.disabled(w);
            }
            let s = ui.add(save);
            s.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, why.is_none(), SAVE));
            if s.clicked() && why.is_none() {
                acts.push(Act::Save);
            }
            let back = ui.add(ecad_ui::prim::Button::new("Back to the drawing").kind(ecad_ui::prim::Kind::Quiet).small());
            back.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, BACK));
            if back.clicked() {
                acts.push(Act::Back);
            }
            if r.choice == Choice::New {
                ui.label(RichText::new("A new symbol needs a key of its own: change it in Details first.").small().color(c32(ecad_ui::SHELL.text_3)));
            }
        });
    }
    acts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn impact() -> Value {
        json!({"name": "Coil", "version": 4, "copies": 3, "pages": 2, "clashes": 1, "rows": [
            {"rep": "a", "tag": "-K01", "page_number": "2", "changes": ["Point 3 added"], "clash": null, "builtin": false},
            {"rep": "b", "tag": "-K02", "page_number": "2", "changes": [], "clash": null, "builtin": false},
            {"rep": "c", "tag": "-K03", "page_number": "4", "changes": [], "clash": "Point 2 is removed; wire 105 is attached to it.", "builtin": false},
        ]})
    }

    #[test]
    fn a_clash_preselects_update_selected_with_the_clashing_row_unticked() {
        let r = Review::of(&impact(), json!({"definition": "d"}), false, "Coil");
        assert_eq!(r.choice, Choice::Selected);
        assert_eq!(r.header(), "Coil v3 → v4 · 3 placed copies on 2 pages · 1 would clash");
        assert!(r.save_refusal().is_none());
        let p = r.edit_params();
        assert_eq!(p["update"], "selected");
        assert_eq!(p["copies"], json!(["a", "b"]));
        let mut all = r.clone();
        all.choice = Choice::All;
        assert!(all.save_refusal().is_some(), "Update all is off while a row clashes");
    }

    #[test]
    fn no_clash_preselects_update_all() {
        let mut v = impact();
        v["rows"].as_array_mut().unwrap().pop();
        let r = Review::of(&v, json!({}), false, "Coil");
        assert_eq!(r.choice, Choice::All);
        assert_eq!(r.outcome(&r.rows[0]).0, "Updated");
    }
}
