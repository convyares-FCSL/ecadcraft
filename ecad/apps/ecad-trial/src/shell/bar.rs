//! Drawing helpers shared by UI rev 3's bars and rails: the bordered pill every group sits in, and
//! text and icon buttons laid out at an exact rect (the bars are placed by arithmetic, so they
//! match the board's measurements, rather than by egui's flowing layouts).
//!
//! Each button gets an accessible label (design §9: tests find elements by label).

use ecad_ui::tokens::ui_type::TypeStyle;
use ecad_ui::tokens::{Icon, line, radius, ui_type};
use eframe::egui::{self, Color32, Rect, Response, Sense, Stroke, StrokeKind, pos2, vec2};

/// The font of `style`.
pub fn font(ui: &egui::Ui, style: TypeStyle) -> egui::FontId {
    ecad_ui::fonts::font(ui.ctx(), style)
}

/// The width `text` takes in `style`.
pub fn text_width(ui: &egui::Ui, text: &str, style: TypeStyle) -> f32 {
    ui.painter().layout_no_wrap(text.to_string(), font(ui, style), Color32::WHITE).size().x
}

/// A bordered pill: the window ground inside a hairline (UI rev 3: every group of the bottom bar,
/// the project name and the workspace switch).
pub fn pill(ui: &egui::Ui, rect: Rect) {
    let s = ecad_ui::SHELL;
    ui.painter().rect(
        rect,
        ecad_ui::style::corner(radius::BUTTON),
        ecad_ui::color(s.bg),
        Stroke::new(line::HAIRLINE_PX, ecad_ui::color(s.line)),
        StrokeKind::Inside,
    );
}

/// A vertical hairline at `x` from `top` to `bottom`.
pub fn vline(ui: &egui::Ui, x: f32, top: f32, bottom: f32) {
    ui.painter().vline(x.round() + 0.5, top..=bottom, Stroke::new(line::HAIRLINE_PX, ecad_ui::color(ecad_ui::SHELL.line)));
}

/// A clickable area at `rect` with accessible name `label` and a hover fill; `on` paints it as
/// the active one (the accent, or `active` ground when `accent` is false).
pub fn hit(ui: &mut egui::Ui, rect: Rect, label: &str, on: bool, accent: bool, enabled: bool) -> Response {
    let s = ecad_ui::SHELL;
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let r = ui.interact(rect, egui::Id::unique(("ecad.bar", label)), sense);
    r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, enabled, on, label));
    let fill = if on && accent {
        Some(s.accent)
    } else if on {
        Some(s.active)
    } else if r.hovered() && enabled {
        Some(s.hover)
    } else {
        None
    };
    if let Some(c) = fill {
        ui.painter().rect_filled(rect, ecad_ui::style::corner(radius::CHIP), ecad_ui::color(c));
    }
    r
}

/// The text colour on a [`hit`] area.
pub fn ink(on: bool, accent: bool, enabled: bool) -> Color32 {
    let s = ecad_ui::SHELL;
    ecad_ui::color(if on && accent {
        ecad_ui::tokens::WHITE
    } else if !enabled {
        s.text_3
    } else if on {
        s.text
    } else {
        s.text_2
    })
}

/// A text button at `rect`: [`hit`], with `text` centred.
pub fn text_button(ui: &mut egui::Ui, rect: Rect, text: &str, label: &str, on: bool, accent: bool) -> Response {
    let r = hit(ui, rect, label, on, accent, true);
    let c = ink(on, accent, true);
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, text, font(ui, ui_type::BODY), c);
    r
}

/// A small text button at `rect` (a page in the well): the accent when `on`.
pub fn text_button_small(ui: &mut egui::Ui, rect: Rect, text: &str, label: &str, on: bool) -> Response {
    let r = hit(ui, rect, label, on, true, true);
    let c = if on { ink(true, true, true) } else { ecad_ui::color(ecad_ui::SHELL.text) };
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, text, font(ui, ui_type::BODY_EMPHASIS), c);
    r
}

/// An icon button at `rect`: [`hit`], with `icon` at `icon_px` centred.
pub fn icon_button(ui: &mut egui::Ui, rect: Rect, icon: Icon, label: &str, icon_px: f32, enabled: bool) -> Response {
    let r = hit(ui, rect, label, false, false, enabled);
    let c = if r.hovered() && enabled { ecad_ui::color(ecad_ui::SHELL.text) } else { ink(false, false, enabled) };
    ecad_ui::icons::paint(ui.painter(), Rect::from_center_size(rect.center(), vec2(icon_px, icon_px)), icon, c);
    r
}

/// Text left-aligned at `x`, centred on `y`, in `style` and `color`; returns its right edge.
pub fn text_at(ui: &egui::Ui, x: f32, y: f32, text: &str, style: TypeStyle, color: Color32) -> f32 {
    let r = ui.painter().text(pos2(x, y), egui::Align2::LEFT_CENTER, text, font(ui, style), color);
    r.right()
}

/// A small filled status dot.
pub fn dot(ui: &egui::Ui, at: egui::Pos2, r: f32, color: Color32) {
    ui.painter().circle_filled(at, r, color);
}
