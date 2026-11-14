//! Button and icon button.

use ecad_tokens::{Icon, WHITE, line, radius, size, space, ui_type};
use egui::{Response, Role, Sense, Stroke, StrokeKind, Ui, Widget, WidgetInfo, vec2};

use super::{Look, accessible, galley};
use crate::style::corner;
use crate::{color, icons};

/// A button's kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    /// The one main action: accent ground, white text.
    Primary,
    /// Any other action: raised ground.
    #[default]
    Secondary,
    /// Delete or discard: error text.
    Danger,
    /// A light action: no ground until hovered.
    Quiet,
}

/// A text button. A disabled button keeps its reason, shown on hover and given to screen readers.
///
/// # Example
/// ```
/// # egui::__run_test_ui(|ui| {
/// use ecad_ui::prim::{Button, Kind};
/// if ui.add(Button::new("Save").kind(Kind::Primary)).clicked() { /* save */ }
/// ui.add(Button::new("Release").disabled("Nothing changed since revision B"));
/// # });
/// ```
#[derive(Clone, Debug)]
pub struct Button<'a> {
    label: &'a str,
    kind: Kind,
    disabled: Option<&'a str>,
    small: bool,
    look: Look,
}

impl<'a> Button<'a> {
    pub fn new(label: &'a str) -> Self {
        Self { label, kind: Kind::Secondary, disabled: None, small: false, look: Look::Auto }
    }
    pub fn kind(mut self, kind: Kind) -> Self {
        self.kind = kind;
        self
    }
    /// Disable it, saying why.
    pub fn disabled(mut self, reason: &'a str) -> Self {
        self.disabled = Some(reason);
        self
    }
    /// The small control height (24 px) instead of 28 px.
    pub fn small(mut self) -> Self {
        self.small = true;
        self
    }
    pub fn look(mut self, look: Look) -> Self {
        self.look = look;
        self
    }
}

impl Widget for Button<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let s = crate::SHELL;
        let enabled = self.disabled.is_none() && ui.is_enabled();
        let text = if !enabled {
            s.text_3
        } else {
            match self.kind {
                Kind::Primary => WHITE,
                Kind::Secondary => s.text,
                Kind::Danger => s.error,
                Kind::Quiet => s.text_2,
            }
        };
        let g = galley(ui, self.label, ui_type::BODY_EMPHASIS);
        let h = if self.small { size::CONTROL_SMALL } else { size::CONTROL };
        let sense = if enabled { Sense::click() } else { Sense::hover() };
        let (rect, mut response) = ui.allocate_exact_size(vec2(g.size().x + 2.0 * space::L, h), sense);
        let label = self.label;
        accessible(&response, || WidgetInfo::labeled(Role::Button, enabled, label), self.disabled);
        if let Some(reason) = self.disabled {
            response = response.on_hover_text(reason);
        }
        if ui.is_rect_visible(rect) {
            let (hovered, pressed, focused) =
                (enabled && self.look.hovered(&response), enabled && self.look.pressed(&response), self.look.focused(&response));
            let hair = |c| Stroke::new(line::HAIRLINE_PX, color(c));
            let (fill, stroke) = match (self.kind, enabled) {
                (Kind::Primary, true) if pressed => (s.accent, Stroke::new(line::FOCUS_RING_PX, color(s.accent_text))),
                (Kind::Primary, true) if hovered => (s.accent, hair(s.accent_text)),
                (Kind::Primary, true) => (s.accent, Stroke::NONE),
                (Kind::Quiet, _) if pressed => (s.active, Stroke::NONE),
                (Kind::Quiet, _) if hovered => (s.hover, Stroke::NONE),
                (Kind::Quiet, _) => (s.raised.at_percent(0), Stroke::NONE),
                (Kind::Danger, true) if hovered || pressed => (if pressed { s.active } else { s.hover }, hair(s.error)),
                (_, true) if pressed => (s.active, hair(s.line_strong)),
                (_, true) if hovered => (s.hover, hair(s.line_strong)),
                _ => (s.raised, hair(s.line)),
            };
            let p = ui.painter();
            p.rect(rect, corner(radius::BUTTON), color(fill), stroke, StrokeKind::Inside);
            if focused {
                p.rect_stroke(rect, corner(radius::BUTTON), Stroke::new(line::FOCUS_RING_PX, color(s.accent_text)), StrokeKind::Outside);
            }
            p.galley(rect.center() - g.size() / 2.0, g, color(text));
        }
        response
    }
}

/// A secondary, primary, danger or quiet button.
pub fn button(ui: &mut Ui, label: &str, kind: Kind) -> Response {
    ui.add(Button::new(label).kind(kind))
}

/// A square button showing an icon; its label is its accessible name and its tooltip.
#[derive(Clone, Debug)]
pub struct IconButton<'a> {
    icon: Icon,
    label: &'a str,
    selected: bool,
    tool: bool,
    disabled: Option<&'a str>,
    look: Look,
}

impl<'a> IconButton<'a> {
    pub fn new(icon: Icon, label: &'a str) -> Self {
        Self { icon, label, selected: false, tool: false, disabled: None, look: Look::Auto }
    }
    /// Drawn selected (the active tool or mode): accent ground, white icon.
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
    /// The tool strip's size: a 36 px square with a 20 px icon (else 28 px with 16 px).
    pub fn tool(mut self) -> Self {
        self.tool = true;
        self
    }
    pub fn disabled(mut self, reason: &'a str) -> Self {
        self.disabled = Some(reason);
        self
    }
    pub fn look(mut self, look: Look) -> Self {
        self.look = look;
        self
    }
}

impl Widget for IconButton<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let s = crate::SHELL;
        let enabled = self.disabled.is_none() && ui.is_enabled();
        let (side, icon_side, r) =
            if self.tool { (size::TOOL_BUTTON, size::TOOL_ICON, radius::TOOL) } else { (size::ICON_BUTTON, size::ICON, radius::BUTTON) };
        let sense = if enabled { Sense::click() } else { Sense::hover() };
        let (rect, response) = ui.allocate_exact_size(vec2(side, side), sense);
        let (label, selected) = (self.label, self.selected);
        accessible(&response, || WidgetInfo::selected(Role::Button, enabled, selected, label), self.disabled);
        let response = response.on_hover_text(self.disabled.map_or_else(|| label.to_string(), |why| format!("{label}: {why}")));
        if ui.is_rect_visible(rect) {
            let (hovered, pressed, focused) =
                (enabled && self.look.hovered(&response), enabled && self.look.pressed(&response), self.look.focused(&response));
            let (fill, ink) = if !enabled {
                (None, s.text_3)
            } else if self.selected {
                (Some(s.accent), WHITE)
            } else if pressed {
                (Some(s.active), s.text)
            } else if hovered {
                (Some(s.hover), s.text)
            } else {
                (None, s.text_2)
            };
            let p = ui.painter();
            if let Some(fill) = fill {
                p.rect_filled(rect, corner(r), color(fill));
            }
            if focused {
                p.rect_stroke(rect, corner(r), Stroke::new(line::FOCUS_RING_PX, color(s.accent_text)), StrokeKind::Outside);
            }
            let icon_rect = egui::Rect::from_center_size(rect.center(), vec2(icon_side, icon_side));
            icons::paint(p, icon_rect, self.icon, color(ink));
        }
        response
    }
}

/// An icon button at the ordinary size (28 px).
pub fn icon_button(ui: &mut Ui, icon: Icon, label: &str) -> Response {
    ui.add(IconButton::new(icon, label))
}
