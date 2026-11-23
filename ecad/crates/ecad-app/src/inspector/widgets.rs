//! The inspector's pieces: a titled section that folds and shows a
//! one-line summary of what is set, the label-and-value rows, a segmented control and the small
//! bits drawn alike on every tab. Grey text is read from the drawing; an input box is something
//! you set here.

use std::collections::BTreeSet;

use ecad_widgets::tokens::{space, ui_type};
use eframe::egui::{self, Color32, Rect, Response, RichText, Sense, Stroke, vec2};

/// The label column's width.
pub(crate) const LABEL_W: f32 = ecad_widgets::tokens::size::LABEL_COLUMN;
/// A row's height (a value box is `size::CONTROL_SMALL`, centred in it).
pub(crate) const FIELD_H: f32 = ecad_widgets::tokens::size::CONTROL;

pub fn font(ui: &egui::Ui, style: ecad_widgets::tokens::ui_type::TypeStyle) -> egui::FontId {
    ecad_widgets::fonts::font(ui.ctx(), style)
}

pub fn text() -> Color32 {
    ecad_widgets::color(ecad_widgets::SHELL.text)
}
pub fn text_2() -> Color32 {
    ecad_widgets::color(ecad_widgets::SHELL.text_2)
}
pub fn text_3() -> Color32 {
    ecad_widgets::color(ecad_widgets::SHELL.text_3)
}
pub fn raised() -> Color32 {
    ecad_widgets::color(ecad_widgets::SHELL.raised)
}
pub fn line() -> Color32 {
    ecad_widgets::color(ecad_widgets::SHELL.line)
}

/// Text in a style and colour.
pub fn rich(ui: &egui::Ui, s: impl Into<String>, style: ui_type::TypeStyle, colour: Color32) -> RichText {
    RichText::new(s).font(font(ui, style)).color(colour)
}

/// A hairline across the panel (between sections), from edge to edge of `full`.
pub fn rule(ui: &mut egui::Ui, full: egui::Rangef) {
    ecad_widgets::panel::hairline(ui, full);
}

/// Accessible name of a section's header.
pub fn section_name(id: &str) -> String {
    format!("ecad.inspector.section.{id}")
}

/// A titled section that folds: `˅ Title ......... summary`. `folded` holds the ids of the folded
/// sections (a section starts folded when `start_folded` and it has never been toggled; the set
/// then holds `!id` for an unfolded one). Returns whether it is open.
pub fn section(
    ui: &mut egui::Ui, folded: &mut BTreeSet<String>, full: egui::Rangef, id: &str, title: &str, summary: &str, start_folded: bool,
    body: impl FnOnce(&mut egui::Ui),
) -> bool {
    let open_key = format!("!{id}");
    let is_folded = if start_folded { !folded.contains(&open_key) } else { folded.contains(id) };
    ui.add_space(space::S);
    let resp = ecad_widgets::panel::section_header(ui, !is_folded, title, summary);
    let name = section_name(id);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &name));
    if resp.clicked() {
        if start_folded {
            if !folded.remove(&open_key) {
                folded.insert(open_key);
            }
        } else if !folded.remove(id) {
            folded.insert(id.to_string());
        }
    }
    if !is_folded {
        ui.add_space(ecad_widgets::panel::BODY_GAP);
        body(ui);
    }
    ecad_widgets::panel::rule(ui, full);
    !is_folded
}

/// `text` cut to `n` characters with an ellipsis.
pub(crate) fn ellipsis(text: &str, n: usize) -> String {
    if text.chars().count() <= n {
        return text.to_string();
    }
    let mut s: String = text.chars().take(n.saturating_sub(1)).collect();
    s.push('\u{2026}');
    s
}

/// A row: the grey label in its column, then `add` in the rest.
pub fn row<R>(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.horizontal(|ui| {
        ui.set_min_height(FIELD_H);
        ui.spacing_mut().item_spacing.x = 0.0;
        ecad_widgets::panel::label(ui, label);
        ui.spacing_mut().item_spacing.x = space::S;
        add(ui)
    })
    .inner
}

/// A value read from the drawing: grey text in the value column.
pub fn read(ui: &mut egui::Ui, label: &str, value: &str, name: &str) -> Response {
    row(ui, label, |ui| {
        let r = ui.add(egui::Label::new(rich(ui, value, ui_type::BODY, text_2())).truncate());
        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, name));
        r
    })
}

/// The box a value set here sits in (`ecad_widgets::panel::field_box`), `w` wide in a row's height;
/// returns the box.
pub fn field_box(ui: &mut egui::Ui, w: f32) -> Rect {
    let (rect, _) = ui.allocate_exact_size(vec2(w, FIELD_H), Sense::hover());
    ecad_widgets::panel::field_box(ui, rect, "").0
}

/// A text entry (`ecad_widgets::control::TextEntry`) with an optional grey `hint` at its right
/// (`auto`, `from page`); `invalid` marks it (the caller shows the message under the row).
/// Returns the edit's response.
pub fn input(ui: &mut egui::Ui, buf: &mut String, placeholder: &str, hint: &str, name: &str, invalid: bool) -> Response {
    ecad_widgets::control::TextEntry::new(buf, name).placeholder(placeholder).hint(hint).invalid(invalid.then_some("")).show(ui)
}

/// A segmented control as wide as the row (`ecad_widgets::prim::Segmented`). `enabled` false shows it
/// read from the drawing (no clicks); `why` is its tooltip. Each segment is named
/// `<name>.<its text in lower case>`. Returns the segment clicked, also the selected one.
pub fn segmented(ui: &mut egui::Ui, items: &[&str], selected: Option<usize>, enabled: bool, name: &str, why: &str) -> Option<usize> {
    // Out of range when nothing is selected, so no segment shows as chosen.
    let mut i = selected.unwrap_or(items.len());
    ecad_widgets::prim::Segmented::new(&mut i, items).fill_width().read_only(!enabled).name(name).hover_text(why).show(ui).clicked
}

/// A small button (secondary, or primary for a check's first fix); `disabled` gives the reason it
/// does nothing yet.
pub fn button(ui: &mut egui::Ui, label: &str, primary: bool, disabled: Option<&str>) -> Response {
    let kind = if primary { ecad_widgets::prim::Kind::Primary } else { ecad_widgets::prim::Kind::Secondary };
    let mut b = ecad_widgets::prim::Button::new(label).kind(kind).small();
    if let Some(why) = disabled {
        b = b.disabled(why);
    }
    ui.add(b)
}

/// A one-line status pill (`ecad_widgets::panel::status_pill`): a tick or a dot, then the words, then
/// `after` in the label grey.
pub fn status_pill(ui: &mut egui::Ui, ok: bool, words: &str, after: &str, name: &str) -> Response {
    ui.horizontal(|ui| {
        let tone = if ok { ecad_widgets::prim::Tone::Ok } else { ecad_widgets::prim::Tone::Error };
        let r = ecad_widgets::panel::status_pill(ui, tone, words, name);
        if !after.is_empty() {
            ui.label(rich(ui, after, ui_type::SMALL, text_3()));
        }
        r
    })
    .inner
}

/// A severity mark: an error's ring with a bar, a warning's triangle, or a note's ring with an i.
pub(crate) fn severity_mark(p: &egui::Painter, centre: egui::Pos2, severity: ecad_electrical::checks::Severity) {
    use ecad_electrical::checks::Severity;
    let s = ecad_widgets::SHELL;
    if severity == Severity::Info {
        let c = ecad_widgets::color(s.text_2);
        p.circle_stroke(centre, 6.5, Stroke::new(1.4, c));
        p.line_segment([centre + vec2(0.0, -0.5), centre + vec2(0.0, 3.5)], Stroke::new(1.4, c));
        p.circle_filled(centre + vec2(0.0, -3.0), 0.9, c);
    } else if severity == Severity::Error {
        let c = ecad_widgets::color(s.error);
        p.circle_stroke(centre, 6.5, Stroke::new(1.4, c));
        p.line_segment([centre + vec2(0.0, -3.5), centre + vec2(0.0, 0.8)], Stroke::new(1.4, c));
        p.circle_filled(centre + vec2(0.0, 3.0), 0.9, c);
    } else {
        let c = ecad_widgets::color(s.warning);
        let pts = vec![centre + vec2(0.0, -6.5), centre + vec2(7.0, 5.5), centre + vec2(-7.0, 5.5)];
        p.add(egui::Shape::closed_line(pts, Stroke::new(1.4, c)));
        p.line_segment([centre + vec2(0.0, -2.5), centre + vec2(0.0, 1.5)], Stroke::new(1.4, c));
        p.circle_filled(centre + vec2(0.0, 3.6), 0.9, c);
    }
}
