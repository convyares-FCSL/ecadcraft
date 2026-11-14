//! Pill, chip, badge and key cap: small labels.

use ecad_tokens::{Icon, line, radius, size, space, ui_type};
use egui::{Rect, Response, Role, Sense, Stroke, StrokeKind, Ui, Widget, WidgetInfo, vec2};

use super::{Look, Tone, accessible, galley};
use crate::style::corner;
use crate::{color, icons};

/// The height of a pill, badge or key cap: the caption line and a 2 px step above and below.
fn small_height() -> f32 {
    ui_type::CAPTION.line + 2.0 * space::XXS
}

/// A fully rounded status label: "Saved", "3 errors", "Agent connected".
pub fn pill(ui: &mut Ui, text: &str, tone: Tone) -> Response {
    let g = galley(ui, text, ui_type::CAPTION);
    let h = small_height();
    let (rect, response) = ui.allocate_exact_size(vec2(g.size().x + 2.0 * space::M, h), Sense::hover());
    accessible(&response, || WidgetInfo::labeled(Role::Label, true, text), Some(tone.name()));
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        p.rect_filled(rect, corner(radius::pill(h)), color(tone.ground()));
        p.galley(rect.center() - g.size() / 2.0, g, color(tone.text()));
    }
    response
}

/// A small outlined label: a count or a short code, in a tone. The origin badge of U4c is one.
pub fn badge(ui: &mut Ui, text: impl ToString, tone: Tone) -> Response {
    let text = text.to_string();
    let g = galley(ui, &text, ui_type::CAPTION);
    let h = small_height();
    let w = (g.size().x + 2.0 * space::S).max(h);
    let (rect, response) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
    accessible(&response, || WidgetInfo::labeled(Role::Label, true, &text), Some(tone.name()));
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        p.rect(
            rect,
            corner(radius::BADGE),
            color(tone.ground()),
            Stroke::new(line::HAIRLINE_PX, color(crate::SHELL.line_strong)),
            StrokeKind::Inside,
        );
        p.galley(rect.center() - g.size() / 2.0, g, color(tone.text()));
    }
    response
}

/// A key or chord in key caps: `"Ctrl+K"` draws two caps. Its accessible label is the text.
pub fn keycap(ui: &mut Ui, keys: &str) -> Response {
    let s = crate::SHELL;
    let mut parts: Vec<&str> = keys.split('+').filter(|k| !k.is_empty()).collect();
    if keys.ends_with("++") || keys == "+" {
        parts.push("+");
    }
    let galleys: Vec<_> = parts.iter().map(|k| galley(ui, k, ui_type::CAPTION)).collect();
    let h = small_height();
    let widths: Vec<f32> = galleys.iter().map(|g| (g.size().x + 2.0 * space::S).max(h)).collect();
    let total = widths.iter().sum::<f32>() + space::XXS * (widths.len().saturating_sub(1)) as f32;
    let (rect, response) = ui.allocate_exact_size(vec2(total, h), Sense::hover());
    accessible(&response, || WidgetInfo::labeled(Role::Label, true, keys), None);
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let mut x = rect.left();
        for (g, w) in galleys.into_iter().zip(widths) {
            let cap = Rect::from_min_size(egui::pos2(x, rect.top()), vec2(w, h));
            p.rect(cap, corner(radius::KEYCAP), color(s.raised), Stroke::new(line::HAIRLINE_PX, color(s.line_strong)), StrokeKind::Inside);
            p.galley(cap.center() - g.size() / 2.0, g, color(s.text_2));
            x += w + space::XXS;
        }
    }
    response
}

/// What a chip reports.
#[derive(Debug)]
pub struct ChipResponse {
    /// The chip itself (clicked: toggle or open it).
    pub response: Response,
    /// Its close button was clicked.
    pub closed: bool,
}

/// A chip: a filter, a tag or a picked value, optionally removable.
#[derive(Clone, Debug)]
pub struct Chip<'a> {
    text: &'a str,
    selected: bool,
    closable: bool,
    look: Look,
}

impl<'a> Chip<'a> {
    pub fn new(text: &'a str) -> Self {
        Self { text, selected: false, closable: false, look: Look::Auto }
    }
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
    /// Show a close button (accessible label "Remove <text>").
    pub fn closable(mut self) -> Self {
        self.closable = true;
        self
    }
    pub fn look(mut self, look: Look) -> Self {
        self.look = look;
        self
    }

    pub fn show(self, ui: &mut Ui) -> ChipResponse {
        let s = crate::SHELL;
        let ink = if self.selected { s.text } else { s.text_2 };
        let g = galley(ui, self.text, ui_type::SMALL);
        let h = size::CONTROL_SMALL;
        // The close button: an icon-sized square after the text.
        let close_w = if self.closable { size::ICON } else { 0.0 };
        let (rect, response) = ui.allocate_exact_size(vec2(g.size().x + 2.0 * space::M + close_w, h), Sense::click());
        let (text, selected) = (self.text, self.selected);
        accessible(&response, || WidgetInfo::selected(Role::Button, true, selected, text), None);
        if ui.is_rect_visible(rect) {
            let hovered = self.look.hovered(&response);
            let (fill, stroke) = if self.selected {
                (s.accent_soft, Stroke::new(line::HAIRLINE_PX, color(s.accent_text)))
            } else if hovered {
                (s.hover, Stroke::NONE)
            } else {
                (s.raised, Stroke::NONE)
            };
            let p = ui.painter();
            p.rect(rect, corner(radius::CHIP), color(fill), stroke, StrokeKind::Inside);
            p.galley(egui::pos2(rect.left() + space::M, rect.center().y - g.size().y / 2.0), g, color(ink));
        }
        let mut closed = false;
        if self.closable {
            let side = size::ICON - space::XS;
            let close = Rect::from_center_size(egui::pos2(rect.right() - space::S - side / 2.0, rect.center().y), vec2(side, side));
            let close_response = ui.interact(close, response.id.with("close"), Sense::click());
            let name = format!("Remove {text}");
            accessible(&close_response, || WidgetInfo::labeled(Role::Button, true, &name), None);
            closed = close_response.clicked();
            if ui.is_rect_visible(close) {
                let ink = if close_response.hovered() { s.text } else { s.text_3 };
                icons::paint(ui.painter(), close, Icon::Close, color(ink));
            }
        }
        ChipResponse { response, closed }
    }
}

/// A plain chip.
pub fn chip(ui: &mut Ui, text: &str) -> Response {
    Chip::new(text).show(ui).response
}

impl Widget for Chip<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        self.show(ui).response
    }
}
