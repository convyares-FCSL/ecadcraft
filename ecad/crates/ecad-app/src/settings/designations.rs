//! Settings ▸ Project info ▸ **Functions and locations** (brief 052c, design 052 §9.4, rev 3
//! board 5.01): the project's two lists with code, description and device count; add, rename,
//! describe, make default, merge into another, remove, and remove every unused entry.
//!
//! The card's edits are immediate commands, like the inspector's, not part of the popout's
//! Save/Cancel copy: a rename updates the drawing at once, and `u` undoes it.
//!
//! # Update map
//! The card reads the list and its counts (`Project::designation_rows`) when first shown, and again
//! only after a transaction that `ecad_electrical::locations::commands::designations::rows_changed_since` names: an op on the list,
//! on a function or location value, or a component or page made or deleted. Every other commit
//! leaves it as it is.

use ecad_electrical::locations::designations::Aspect;
use ecad_widgets::control;
use ecad_widgets::prim::{self, Tone};
use ecad_widgets::tokens::{space, ui_type};
use eframe::egui;
use serde_json::{Value, json};

use crate::shell::app::App;

/// The card (element name).
pub const CARD: &str = "ecad.projectinfo.designations";
/// The merge picker (element name).
pub(crate) const MERGE_PICKER: &str = "ecad.designation.merge.picker";
/// The card's foot.
pub(crate) const FOOT: &str = "Changes here apply at once; undo with u.";

/// The kept rows: the log length read, and the `designations` answer.
#[derive(Clone, Default)]
struct Kept {
    seen: usize,
    rows: Value,
}

fn kept_id() -> egui::Id {
    egui::Id::unique("settings.designations.kept")
}

/// The rows to show: the kept ones, read again only when a commit could have changed them.
pub fn rows(t: &mut App, ctx: &egui::Context) -> Value {
    let kept: Option<Kept> = ctx.data(|d| d.get_temp(kept_id()));
    let Some(p) = t.project.clone() else { return Value::Null };
    let seen = kept.as_ref().map(|k| k.seen);
    let fresh = p
        .configure(&mut t.cad.session, |p| {
            let n = p.ledger.log().len();
            match seen {
                Some(s) if !ecad_electrical::locations::commands::designations::rows_changed_since(&p.ledger, s) => (n, None),
                _ => (n, Some(p.designation_rows())),
            }
        })
        .ok();
    let out = match (fresh, kept) {
        (Some((n, Some(rows))), _) => Kept { seen: n, rows },
        (Some((n, None)), Some(k)) => Kept { seen: n, rows: k.rows },
        (_, Some(k)) => k,
        _ => Kept::default(),
    };
    let rows = out.rows.clone();
    ctx.data_mut(|d| d.insert_temp(kept_id(), out));
    rows
}

/// What a click asks for: a command line run after the card is drawn.
pub type Ask = Option<String>;

fn line(cmd: &str, params: Value) -> Ask {
    Some(format!("{cmd} {params}"))
}

/// The card's body (inside the Project info card frame): both lists, `ro` read-only.
pub fn body(ui: &mut egui::Ui, rows: &Value, ro: bool) -> Ask {
    let mut ask = None;
    ui.scope(|ui| {
        for a in Aspect::ALL {
            let list = &rows[a.plural()]["entries"];
            let entries = list.as_array().cloned().unwrap_or_default();
            if let Some(x) = aspect(ui, a, &entries, ro) {
                ask = Some(x);
            }
            ui.add_space(space::M);
        }
        ui.label(
            egui::RichText::new("\u{25cf} the project default: proposed where no box or page says otherwise")
                .small()
                .color(ecad_widgets::color(ecad_widgets::SHELL.text_3)),
        );
        ui.label(egui::RichText::new(FOOT).small().color(ecad_widgets::color(ecad_widgets::SHELL.text_3)));
    })
    .response
    .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Group, true, CARD));
    ask
}

fn small(ui: &egui::Ui, text: &str, faint: bool) -> egui::RichText {
    let _ = ui;
    let s = ecad_widgets::SHELL;
    egui::RichText::new(text).size(ui_type::SMALL.size).color(ecad_widgets::color(if faint { s.text_3 } else { s.text_2 }))
}

/// One aspect's list.
fn aspect(ui: &mut egui::Ui, a: Aspect, entries: &[Value], ro: bool) -> Ask {
    let mut ask = None;
    let letter = a.letter();
    let unused = entries.iter().filter(|e| e["unused"] == json!(true)).count();
    let adding_id = egui::Id::unique(("settings.designations.adding", letter));
    let title = match a {
        Aspect::Function => "Functions, =",
        Aspect::Location => "Locations, +",
    };
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title).strong().color(ecad_widgets::color(ecad_widgets::SHELL.text)));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let r = ui.add_enabled(!ro, ecad_widgets::prim::Button::new("+ Add").small());
            r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, !ro, format!("ecad.designation.{letter}.add")));
            if r.clicked() {
                ui.data_mut(|d| d.insert_temp(adding_id, String::new()));
            }
            if unused > 0 {
                let label = format!("Remove unused ({unused})");
                let r = ui.add_enabled(!ro, ecad_widgets::prim::Button::new(&label).small());
                r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, !ro, format!("ecad.designation.{letter}.remove_unused")));
                if r.clicked() {
                    ask = line("designationremove", json!({"aspect": a.word(), "unused": true}));
                }
            }
        });
    });
    if entries.is_empty() && ui.data(|d| d.get_temp::<String>(adding_id)).is_none() {
        let what = match a {
            Aspect::Function => "No functions yet. Add one, or type one on a device or a box.",
            Aspect::Location => "No locations yet. Add one, or type one on a device or a box.",
        };
        ui.label(small(ui, what, true));
    }
    egui::Grid::new(("settings.designations.grid", letter)).num_columns(4).spacing(egui::vec2(space::M, space::XS)).show(ui, |ui| {
        if !entries.is_empty() {
            for h in ["Code", "Description", "Devices", ""] {
                ui.label(small(ui, h, true));
            }
            ui.end_row();
        }
        let codes: Vec<String> = entries.iter().filter_map(|e| e["code"].as_str().map(str::to_string)).collect();
        for e in entries {
            if let Some(x) = entry_row(ui, a, e, &codes, entries, ro) {
                ask = Some(x);
            }
            ui.end_row();
        }
        // The new row: Enter adds, Esc drops it.
        if let Some(mut buf) = ui.data(|d| d.get_temp::<String>(adding_id)) {
            let name = format!("ecad.designation.{letter}.new");
            let r = control::TextEntry::new(&mut buf, &name).placeholder(ecad_electrical::locations::designations::example(a)).width(90.0).show(ui);
            r.request_focus();
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                ui.data_mut(|d| d.remove::<String>(adding_id));
            } else if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !buf.trim().is_empty() {
                ask = line("designationadd", json!({"aspect": a.word(), "code": buf.trim()}));
                ui.data_mut(|d| d.remove::<String>(adding_id));
            } else {
                ui.data_mut(|d| d.insert_temp(adding_id, buf));
            }
            ui.end_row();
        }
    });
    ask
}

/// One entry's row: code and description cells (a rename and a description commit on Enter or on
/// leaving the cell), the count, the default marker and the row's actions.
fn entry_row(ui: &mut egui::Ui, a: Aspect, e: &Value, codes: &[String], entries: &[Value], ro: bool) -> Ask {
    let mut ask = None;
    let key = e["key"].as_str().unwrap_or_default().to_string();
    let code = e["code"].as_str().unwrap_or_default().to_string();
    let base = format!("ecad.designation.{}.{code}", a.letter());
    let unused = e["unused"] == json!(true);
    let uses = e["uses"].as_str().unwrap_or("nothing").to_string();
    // Code: a rename. A code another entry has is refused in the cell, with Merge offered.
    let code_id = egui::Id::unique((&base, "code"));
    let mut buf = ui.data(|d| d.get_temp::<String>(code_id)).unwrap_or_else(|| code.clone());
    let typed = buf.trim().to_string();
    let clash = (!typed.eq_ignore_ascii_case(&code)).then(|| codes.iter().find(|c| c.eq_ignore_ascii_case(&normal(a, &typed))).cloned()).flatten();
    let clash_words = clash.as_ref().map(|other| format!("{other} is already in the list."));
    let code_name = format!("{base}.code");
    let cell = ui
        .vertical(|ui| {
            let r = control::TextEntry::new(&mut buf, &code_name).width(90.0).read_only(ro).invalid(clash_words.as_deref()).show(ui);
            if r.has_focus() {
                ui.data_mut(|d| d.insert_temp(code_id, buf.clone()));
                match &clash_words {
                    Some(w) => {
                        let _ = prim::pill(ui, w, Tone::Error);
                    }
                    None => {
                        let _ = prim::read_value(ui, &format!("Renames it on {uses}."));
                    }
                }
            } else if r.lost_focus() {
                ui.data_mut(|d| d.remove::<String>(code_id));
                if clash.is_none() && !typed.is_empty() && typed != code {
                    ask = line("designationedit", json!({"aspect": a.word(), "key": key, "code": typed}));
                }
            }
            r
        })
        .inner;
    if let Some(other) = &clash
        && !ro
        && ui.add(prim::Button::new(&format!("Merge into {other}")).small()).clicked()
    {
        ask = line("designationmerge", json!({"aspect": a.word(), "from": key, "into": other}));
        ui.data_mut(|d| d.remove::<String>(code_id));
    }
    let _ = cell;
    // Description.
    let desc = e["description"].as_str().unwrap_or_default().to_string();
    let desc_id = egui::Id::unique((&base, "description"));
    let mut dbuf = ui.data(|d| d.get_temp::<String>(desc_id)).unwrap_or_else(|| desc.clone());
    let desc_name = format!("{base}.description");
    let r = control::TextEntry::new(&mut dbuf, &desc_name).width(160.0).read_only(ro).placeholder("—").show(ui);
    if r.has_focus() {
        ui.data_mut(|d| d.insert_temp(desc_id, dbuf.clone()));
    } else if r.lost_focus() {
        ui.data_mut(|d| d.remove::<String>(desc_id));
        if dbuf.trim() != desc {
            let v = if dbuf.trim().is_empty() { Value::Null } else { json!(dbuf.trim()) };
            ask = line("designationedit", json!({"aspect": a.word(), "key": key, "description": v}));
        }
    }
    // Count, default marker and the row's actions.
    ui.horizontal(|ui| {
        let n = e["devices"].as_u64().unwrap_or(0);
        let shown = n.to_string();
        let hint = if unused { "unused" } else { "" };
        let d = control::Decor { hint, ..control::Decor::default() };
        let _ = control::read_text(ui, &shown, d, &format!("{base}.count")).on_hover_text(uses.as_str());
        if e["default"] == json!(true) {
            let r = prim::pill(ui, "\u{25cf} default", Tone::Neutral).on_hover_text("The project default");
            r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, format!("{base}.default")));
        }
    });
    if ro {
        ui.label("");
        return ask;
    }
    ui.menu_button(egui::RichText::new("\u{22ef}").color(ecad_widgets::color(ecad_widgets::SHELL.text_2)), |ui| {
        if e["default"] != json!(true) && ui.button("Make default").clicked() {
            ask = line("projectprops", json!({a.word(): code}));
            ui.close();
        }
        ui.add_enabled(false, egui::Button::new("Show devices")).on_disabled_hover_text("Not available yet");
        ui.menu_button("Merge into\u{2026}", |ui| {
            ui.label(small(ui, &format!("Moves {uses} from {code}, then removes {code}."), true));
            for other in entries.iter().filter(|o| o["key"] != e["key"]) {
                let oc = other["code"].as_str().unwrap_or_default();
                let od = other["description"].as_str().unwrap_or_default();
                let label = if od.is_empty() { oc.to_string() } else { format!("{oc}   {od}") };
                if ui.button(label).clicked() {
                    ask = line("designationmerge", json!({"aspect": a.word(), "from": key, "into": other["key"]}));
                    ui.close();
                }
            }
        })
        .response
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::MenuItem, true, MERGE_PICKER));
        let refusal = format!("{code} is used by {uses}. Give them another {} first.", a.word());
        let r = ui.add_enabled(unused, egui::Button::new("Remove")).on_disabled_hover_text(refusal);
        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, unused, format!("{base}.remove")));
        if r.clicked() {
            ask = line("designationremove", json!({"aspect": a.word(), "key": key}));
            ui.close();
        }
    });
    ask
}

/// A code as typed, with its sign, for comparing with the list (the command checks it again).
fn normal(a: Aspect, t: &str) -> String {
    let t = t.trim();
    if t.starts_with(a.sign()) { t.to_string() } else { format!("{}{t}", a.sign()) }
}
