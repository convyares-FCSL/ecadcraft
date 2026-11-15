//! A property row of the Properties copy ([`crate::sheets_ui::PropsShared`]) drawn the rev 3 way:
//! the grey label, then an input box when it is set here (a value read from elsewhere shows as
//! its grey placeholder, with where it comes from at the right), or grey text when it cannot be
//! set. Edits go to the copy's `edits`, run by the trial after the frame, exactly as before.

use ecad_cadcraft::props::{Row, Target};
use ecad_ui::tokens::ui_type;
use eframe::egui::{self, RichText};

use super::widgets as w;
use crate::sheets_ui::PropsShared;

/// Draw `row` of a section of `kind` editing `target`. `hint` is the grey word at the box's
/// right (`auto`, `from page`); empty for none.
pub fn prop(ui: &mut egui::Ui, sh: &mut PropsShared, target: &Option<Target>, kind: &str, row: &Row, label: &str, hint: &str) {
    let name = format!("ecad.props.{kind}.{}", row.key);
    let error = sh.errors.get(&row.key).cloned();
    let writable = sh.writable && row.editable && target.is_some();
    if !writable {
        let r = w::read(ui, label, &row.value, &name);
        r.on_hover_text(if sh.writable { "Read from the drawing" } else { "The project is read-only." });
        return;
    }
    let mut commit: Option<String> = None;
    w::row(ui, label, |ui| {
        let id = ui.make_persistent_id(&name);
        let shown = if row.varies || row.inherited { String::new() } else { row.value.clone() };
        let mut buf = ui.data_mut(|d| d.get_temp::<String>(id)).unwrap_or_else(|| shown.clone());
        let placeholder = if row.varies || row.inherited { row.value.as_str() } else { "" };
        let menu = !row.choices.is_empty();
        let r = ui
            .scope(|ui| {
                if menu {
                    ui.set_max_width(ui.available_width() - 26.0);
                }
                w::input(ui, &mut buf, placeholder, hint, &name, error.is_some())
            })
            .inner;
        if r.has_focus() {
            ui.data_mut(|d| d.insert_temp(id, buf.clone()));
        } else {
            ui.data_mut(|d| d.remove::<String>(id));
        }
        if r.lost_focus() {
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                ui.data_mut(|d| d.remove::<String>(id));
            } else if buf != shown {
                commit = Some(buf.clone());
            }
        }
        if menu {
            ui.menu_button(RichText::new("\u{25be}").color(w::text_2()), |ui| {
                for c in &row.choices {
                    if ui.button(c).clicked() {
                        commit = Some(c.clone());
                        ui.close();
                    }
                }
            })
            .response
            .on_hover_text("Choose");
        }
    });
    if let (Some(v), Some(tg)) = (commit, target) {
        let value = if v.trim().is_empty() { None } else { Some(v) };
        sh.edits.push((tg.clone(), row.key.clone(), value));
    }
    if let Some(e) = error {
        ui.horizontal_wrapped(|ui| {
            ui.add_space(w::LABEL_W);
            ui.label(w::rich(ui, &e, ui_type::SMALL, ecad_ui::color(ecad_ui::SHELL.error)));
        });
        tag_clash(ui, sh, target, row, &e);
    }
}

/// Brief 026 (design §7.1): a taken tag offers Swap first, then the same device, then the next
/// free tag. Nothing is written until a button is clicked.
fn tag_clash(ui: &mut egui::Ui, sh: &mut PropsShared, target: &Option<Target>, row: &Row, e: &str) {
    if row.key != "tag" {
        return;
    }
    let (Some(tg), Some((typed, _))) = (target, e.split_once(" is used by ")) else { return };
    let typed = typed.trim().to_string();
    let next = e.split("\"next\" (use ").nth(1).and_then(|r| r.split(')').next()).map(str::to_string);
    let same_kind = e.contains("\"swap\"");
    let noun = if matches!(tg, Target::Cable(_)) { "cable" } else { "device" };
    ui.horizontal_wrapped(|ui| {
        ui.add_space(w::LABEL_W);
        if same_kind {
            if w::button(ui, "Swap", true, None).accessible_name("ecad.tagclash.swap").clicked() {
                sh.edits.push((tg.clone(), ecad_cadcraft::props::conflict_key("swap"), Some(typed.clone())));
            }
            if w::button(ui, &format!("Make it the same {noun}"), false, None).accessible_name("ecad.tagclash.merge").clicked() {
                sh.edits.push((tg.clone(), ecad_cadcraft::props::conflict_key("merge"), Some(typed.clone())));
            }
        }
        let label = next.as_deref().map_or_else(|| "Use the next free".to_string(), |n| format!("Use {n}"));
        if w::button(ui, &label, false, None).accessible_name("ecad.tagclash.next").clicked() {
            sh.edits.push((tg.clone(), ecad_cadcraft::props::conflict_key("next"), Some(typed.clone())));
        }
    });
}

/// A section's buttons (`revisions`, `more`, `goto:`, `showplan:`, `showcores:`, `refgoto:`), each
/// pressed one pushed to the copy's `buttons`.
pub fn buttons(ui: &mut egui::Ui, sh: &mut PropsShared, list: &[String]) {
    if list.is_empty() {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        for b in list {
            let label = match b.as_str() {
                "revisions" => "Revisions\u{2026}".to_string(),
                "more" => "More\u{2026}".to_string(),
                g if g.starts_with("goto:") => "Go to".to_string(),
                g if g.starts_with("showplan:") => "Show plan".to_string(),
                g if g.starts_with("showcores:") => "Show cores".to_string(),
                g if g.starts_with("refgoto:") => g.splitn(4, ':').nth(3).unwrap_or("Go to").to_string(),
                other => other.to_string(),
            };
            if w::button(ui, &label, false, None).clicked() {
                sh.buttons.push(b.clone());
            }
        }
    });
}
