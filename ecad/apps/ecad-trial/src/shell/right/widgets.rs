//! The inspector's pieces (UI rev 3, boards 1.05 to 1.07): a titled section that folds and shows a
//! one-line summary of what is set, the label-and-value rows, a segmented control and the small
//! bits drawn alike on every tab. Grey text is read from the drawing; an input box is something
//! you set here.

use std::collections::BTreeSet;

use ecad_ui::tokens::{radius, space, ui_type};
use eframe::egui::{self, Color32, CornerRadius, Rect, Response, RichText, Sense, Stroke, StrokeKind, pos2, vec2};

/// The label column's width.
pub const LABEL_W: f32 = 96.0;
/// A value box's height.
pub const FIELD_H: f32 = 28.0;

pub fn font(ui: &egui::Ui, style: ecad_ui::tokens::ui_type::TypeStyle) -> egui::FontId {
    ecad_ui::fonts::font(ui.ctx(), style)
}

pub fn text() -> Color32 {
    ecad_ui::color(ecad_ui::SHELL.text)
}
pub fn text_2() -> Color32 {
    ecad_ui::color(ecad_ui::SHELL.text_2)
}
pub fn text_3() -> Color32 {
    ecad_ui::color(ecad_ui::SHELL.text_3)
}
pub fn raised() -> Color32 {
    ecad_ui::color(ecad_ui::SHELL.raised)
}
pub fn line() -> Color32 {
    ecad_ui::color(ecad_ui::SHELL.line)
}

/// Text in a style and colour.
pub fn rich(ui: &egui::Ui, s: impl Into<String>, style: ui_type::TypeStyle, colour: Color32) -> RichText {
    RichText::new(s).font(font(ui, style)).color(colour)
}

/// A hairline across the panel (between sections), from edge to edge of `full`.
pub fn rule(ui: &mut egui::Ui, full: egui::Rangef) {
    let y = ui.cursor().top();
    ui.painter().hline(full, y + 0.5, Stroke::new(ecad_ui::tokens::line::HAIRLINE_PX, line()));
    ui.add_space(1.0);
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
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 26.0), Sense::click());
    let name = section_name(id);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &name));
    let p = ui.painter();
    let chevron = Rect::from_center_size(pos2(rect.left() + 6.0, rect.center().y), vec2(10.0, 10.0));
    let icon = if is_folded { ecad_ui::tokens::Icon::ChevronRight } else { ecad_ui::tokens::Icon::ChevronDown };
    ecad_ui::icons::paint(p, chevron, icon, if resp.hovered() { text() } else { text_2() });
    p.text(pos2(rect.left() + 20.0, rect.center().y), egui::Align2::LEFT_CENTER, title, font(ui, ui_type::BODY_EMPHASIS), text());
    if !summary.is_empty() {
        let g = p.layout_no_wrap(summary.to_string(), font(ui, ui_type::SMALL), text_3());
        let room = (rect.width() - 30.0 - title.len() as f32 * 7.5).max(40.0);
        let shown = if g.size().x > room { ellipsis(summary, (room / 6.5) as usize) } else { summary.to_string() };
        p.text(pos2(rect.right(), rect.center().y), egui::Align2::RIGHT_CENTER, shown, font(ui, ui_type::SMALL), text_3());
    }
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
        ui.add_space(space::XS);
        body(ui);
    }
    ui.add_space(space::S);
    rule(ui, full);
    !is_folded
}

/// `text` cut to `n` characters with an ellipsis.
pub fn ellipsis(text: &str, n: usize) -> String {
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
        ui.set_min_height(FIELD_H + 4.0);
        let (lr, _) = ui.allocate_exact_size(vec2(LABEL_W, FIELD_H), Sense::hover());
        ui.painter().text(pos2(lr.left(), lr.center().y), egui::Align2::LEFT_CENTER, label, font(ui, ui_type::SMALL), text_2());
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

/// The box a value set here sits in: raised, rounded, `w` wide; returns its rect.
pub fn field_box(ui: &mut egui::Ui, w: f32) -> Rect {
    let (rect, _) = ui.allocate_exact_size(vec2(w, FIELD_H), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(radius::FIELD as u8), raised());
    rect
}

/// A text input inside a field box with an optional grey `hint` at its right (`auto`, `from
/// page`). Returns the edit's response.
pub fn input(ui: &mut egui::Ui, buf: &mut String, placeholder: &str, hint: &str, name: &str, invalid: bool) -> Response {
    let w = ui.available_width();
    let rect = field_box(ui, w);
    let hint_w =
        if hint.is_empty() { 0.0 } else { ui.painter().layout_no_wrap(hint.to_string(), font(ui, ui_type::SMALL), text_3()).size().x + space::M };
    // One line of text, centred in the box.
    let body = font(ui, ui_type::BODY);
    let line_h = ui.fonts_mut(|f| f.row_height(&body)) + 2.0;
    let top = rect.center().y - line_h / 2.0;
    let edit_rect = Rect::from_min_max(pos2(rect.left() + space::M, top), pos2(rect.right() - space::M - hint_w, top + line_h));
    let mut edit = egui::TextEdit::singleline(buf).frame(egui::Frame::NONE).font(font(ui, ui_type::BODY)).text_color(text());
    if !placeholder.is_empty() {
        edit = edit.hint_text(rich(ui, placeholder, ui_type::BODY, text_3()));
    }
    let r = ui.put(edit_rect, edit);
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, name));
    if !hint.is_empty() {
        ui.painter().text(pos2(rect.right() - space::M, rect.center().y), egui::Align2::RIGHT_CENTER, hint, font(ui, ui_type::SMALL), text_3());
    }
    if invalid {
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(radius::FIELD as u8),
            Stroke::new(1.0, ecad_ui::color(ecad_ui::SHELL.error)),
            StrokeKind::Outside,
        );
    } else if r.has_focus() {
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(radius::FIELD as u8),
            Stroke::new(1.0, ecad_ui::color(ecad_ui::SHELL.accent_text)),
            StrokeKind::Inside,
        );
    }
    r
}

/// A segmented control `w` wide. `enabled` false draws it read from the drawing (grey, no
/// clicks). Returns the segment clicked.
pub fn segmented(ui: &mut egui::Ui, items: &[&str], selected: Option<usize>, enabled: bool, name: &str, why: &str) -> Option<usize> {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, FIELD_H), Sense::hover());
    let s = ecad_ui::SHELL;
    let p = ui.painter().clone();
    p.rect(
        rect,
        CornerRadius::same(radius::BUTTON as u8),
        ecad_ui::color(s.bg),
        Stroke::new(ecad_ui::tokens::line::HAIRLINE_PX, line()),
        StrokeKind::Inside,
    );
    let n = items.len().max(1) as f32;
    let seg_w = (rect.width() - 4.0) / n;
    let mut out = None;
    for (i, item) in items.iter().enumerate() {
        let r = Rect::from_min_size(pos2(rect.left() + 2.0 + i as f32 * seg_w, rect.top() + 2.0), vec2(seg_w, rect.height() - 4.0));
        let resp = ui.interact(r, egui::Id::unique((name, i)), if enabled { Sense::click() } else { Sense::hover() });
        let label = format!("{name}.{}", item.to_lowercase());
        resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::RadioButton, enabled, selected == Some(i), &label));
        let on = selected == Some(i);
        if on {
            p.rect_filled(r, CornerRadius::same((radius::BUTTON - 1.0) as u8), ecad_ui::color(s.active));
        } else if enabled && resp.hovered() {
            p.rect_filled(r, CornerRadius::same((radius::BUTTON - 1.0) as u8), ecad_ui::color(s.hover));
        }
        let ink = match (enabled, on) {
            (true, true) => text(),
            (true, false) => text_2(),
            (false, true) => text_2(),
            (false, false) => text_3(),
        };
        p.text(r.center(), egui::Align2::CENTER_CENTER, *item, font(ui, if on { ui_type::BODY_EMPHASIS } else { ui_type::BODY }), ink);
        let resp = if why.is_empty() { resp } else { resp.on_hover_text(why) };
        if enabled && resp.clicked() {
            out = Some(i);
        }
    }
    out
}

/// A small button (secondary, or primary for a check's first fix); `disabled` gives the reason it
/// does nothing yet.
pub fn button(ui: &mut egui::Ui, label: &str, primary: bool, disabled: Option<&str>) -> Response {
    let kind = if primary { ecad_ui::prim::Kind::Primary } else { ecad_ui::prim::Kind::Secondary };
    let mut b = ecad_ui::prim::Button::new(label).kind(kind).small();
    if let Some(why) = disabled {
        b = b.disabled(why);
    }
    ui.add(b)
}

/// A one-line status pill: a tick or a dot, then the words.
pub fn status_pill(ui: &mut egui::Ui, ok: bool, words: &str, after: &str, name: &str) -> Response {
    let s = ecad_ui::SHELL;
    ui.horizontal(|ui| {
        let g = ui.painter().layout_no_wrap(words.to_string(), font(ui, ui_type::SMALL), text_2());
        let (rect, r) = ui.allocate_exact_size(vec2(g.size().x + 30.0, 22.0), Sense::hover());
        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, name));
        let p = ui.painter();
        p.rect_filled(rect, CornerRadius::same(11), ecad_ui::color(s.raised));
        let mark = pos2(rect.left() + 12.0, rect.center().y);
        if ok {
            let c = ecad_ui::color(s.ok);
            p.line_segment([mark + vec2(-3.5, 0.0), mark + vec2(-1.0, 2.5)], Stroke::new(1.5, c));
            p.line_segment([mark + vec2(-1.0, 2.5), mark + vec2(4.0, -3.0)], Stroke::new(1.5, c));
        } else {
            p.circle_filled(mark, 3.5, ecad_ui::color(s.error));
        }
        p.galley(pos2(rect.left() + 22.0, rect.center().y - g.size().y / 2.0), g, text_2());
        if !after.is_empty() {
            ui.label(rich(ui, after, ui_type::SMALL, text_3()));
        }
        r
    })
    .inner
}

/// A severity mark: an error's ring with a bar, a warning's triangle, or a note's ring with an i.
pub fn severity_mark(p: &egui::Painter, centre: egui::Pos2, severity: ecad_cadcraft::checks::Severity) {
    use ecad_cadcraft::checks::Severity;
    let s = ecad_ui::SHELL;
    if severity == Severity::Info {
        let c = ecad_ui::color(s.text_2);
        p.circle_stroke(centre, 6.5, Stroke::new(1.4, c));
        p.line_segment([centre + vec2(0.0, -0.5), centre + vec2(0.0, 3.5)], Stroke::new(1.4, c));
        p.circle_filled(centre + vec2(0.0, -3.0), 0.9, c);
    } else if severity == Severity::Error {
        let c = ecad_ui::color(s.error);
        p.circle_stroke(centre, 6.5, Stroke::new(1.4, c));
        p.line_segment([centre + vec2(0.0, -3.5), centre + vec2(0.0, 0.8)], Stroke::new(1.4, c));
        p.circle_filled(centre + vec2(0.0, 3.0), 0.9, c);
    } else {
        let c = ecad_ui::color(s.warning);
        let pts = vec![centre + vec2(0.0, -6.5), centre + vec2(7.0, 5.5), centre + vec2(-7.0, 5.5)];
        p.add(egui::Shape::closed_line(pts, Stroke::new(1.4, c)));
        p.line_segment([centre + vec2(0.0, -2.5), centre + vec2(0.0, 1.5)], Stroke::new(1.4, c));
        p.circle_filled(centre + vec2(0.0, 3.6), 0.9, c);
    }
}
