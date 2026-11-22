//! Segmented control: one of a few choices, side by side (the workspace switch is one).
//!
//! UI rev 4 (boards 2.05, 2.06, 3.05, 5.02): a soft rounded track on the window ground with a
//! hairline edge, and the selected choice as a raised chip inside it. Every other choice is plain
//! secondary text that takes the hover ground under the pointer; the keyboard focus is the focus
//! ring around the focused choice.
//!
//! States:
//! - **Normal:** clicks and the keyboard (Tab to a choice, Space or Enter) select.
//! - **Read-only** ([`Segmented::read_only`]): the value is read from the drawing. The chip still
//!   shows it, in secondary text; nothing reacts to the pointer.
//! - **Disabled** (inside a disabled `Ui`, `add_enabled_ui(false, …)`): drawn as read-only and
//!   faded by egui's disabled opacity.

use ecad_tokens::{line, radius, size, space, ui_type};
use egui::{Rect, Response, Role, Sense, Stroke, StrokeKind, Ui, Widget, WidgetInfo, pos2, vec2};

use super::{Look, accessible, galley};
use crate::style::corner;
use crate::{Theme, color};

/// A segmented control over `items`, the selected index in `selected`. Each segment is a radio
/// button labelled with its text; the response reports `changed` when the selection changes.
#[derive(Debug)]
pub struct Segmented<'a> {
    selected: &'a mut usize,
    items: &'a [&'a str],
    /// A forced look for one segment (the showcase's hovered segment).
    look: Option<(usize, Look)>,
    read_only: bool,
    fill: bool,
    name: Option<&'a str>,
    hover: Option<&'a str>,
}

/// What a segmented control did: its response (`changed` when the selection changed) and the
/// segment clicked this frame, if any, also when it was already the selected one.
#[derive(Debug)]
pub struct SegmentedResponse {
    pub response: Response,
    pub clicked: Option<usize>,
}

impl<'a> Segmented<'a> {
    pub fn new(selected: &'a mut usize, items: &'a [&'a str]) -> Self {
        Self { selected, items, look: None, read_only: false, fill: false, name: None, hover: None }
    }
    pub fn look(mut self, segment: usize, look: Look) -> Self {
        self.look = Some((segment, look));
        self
    }
    /// Show the value without taking clicks: it is read from the drawing.
    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }
    /// Take the available width, the segments sharing it equally (an inspector row).
    pub fn fill_width(mut self) -> Self {
        self.fill = true;
        self
    }
    /// Name each segment `<name>.<its text in lower case>` for tests and the control channel,
    /// instead of by its text alone.
    pub fn name(mut self, name: &'a str) -> Self {
        self.name = Some(name);
        self
    }
    /// A tooltip on every segment (why it is read-only, for example); empty shows none.
    pub fn hover_text(mut self, text: &'a str) -> Self {
        self.hover = Some(text).filter(|t| !t.is_empty());
        self
    }

    /// Draw it.
    pub fn show(self, ui: &mut Ui) -> SegmentedResponse {
        let s = crate::SHELL;
        let enabled = !self.read_only && ui.is_enabled();
        let inset = space::XXS;
        let bold: Vec<_> = self.items.iter().map(|t| galley(ui, t, ui_type::BODY_EMPHASIS)).collect();
        let mut widths: Vec<f32> = bold.iter().map(|g| g.size().x + 2.0 * space::L).collect();
        let total = if self.fill {
            let avail = ui.available_width().max(2.0 * inset);
            let each = (avail - 2.0 * inset) / widths.len().max(1) as f32;
            widths.iter_mut().for_each(|w| *w = each);
            avail
        } else {
            widths.iter().sum::<f32>() + 2.0 * inset
        };
        let (outer, mut response) = ui.allocate_exact_size(vec2(total, size::CONTROL), Sense::hover());
        let visible = ui.is_rect_visible(outer);
        let p = ui.painter().clone();
        if visible {
            p.rect(outer, corner(radius::BUTTON), color(s.bg), Stroke::new(line::HAIRLINE_PX, color(s.line)), StrokeKind::Inside);
        }
        // The chip is the raised surface that stands out from the track: `active` on Graphite
        // (as on the boards), `raised` with a hairline edge on the light shell, where `active`
        // is almost the track's own colour.
        let (chip, chip_edge) = match crate::theme() {
            Theme::Dark => (s.active, Stroke::NONE),
            Theme::Light => (s.raised, Stroke::new(line::HAIRLINE_PX, color(s.line_strong))),
        };
        let sense = if enabled { Sense::click() } else { Sense::hover() };
        let mut x = outer.left() + inset;
        let before = *self.selected;
        let mut clicked = None;
        for (i, (g, w)) in bold.into_iter().zip(widths).enumerate() {
            let seg = Rect::from_min_max(pos2(x, outer.top() + inset), pos2(x + w, outer.bottom() - inset));
            x += w;
            let mut r = ui.interact(seg, response.id.with(i), sense);
            let on = *self.selected == i;
            let text = self.items.get(i).copied().unwrap_or_default();
            let named = self.name.map(|n| format!("{n}.{}", text.to_lowercase()));
            let label = named.as_deref().unwrap_or(text);
            accessible(&r, || WidgetInfo::selected(Role::RadioButton, enabled, on, label), None);
            if let Some(why) = self.hover {
                r = r.on_hover_text(why).on_disabled_hover_text(why);
            }
            if enabled && r.clicked() {
                *self.selected = i;
                clicked = Some(i);
            }
            if visible {
                let look = self.look.filter(|(n, _)| *n == i).map_or(Look::Auto, |(_, l)| l);
                let hovered = enabled && look.hovered(&r);
                let pressed = enabled && look.pressed(&r);
                let focused = enabled && look.focused(&r);
                let seg_corner = corner(radius::SEGMENT);
                if on {
                    p.rect(seg, seg_corner, color(chip), chip_edge, StrokeKind::Inside);
                } else if pressed {
                    p.rect_filled(seg, seg_corner, color(s.active));
                } else if hovered {
                    p.rect_filled(seg, seg_corner, color(s.hover));
                }
                if focused {
                    p.rect_stroke(seg, seg_corner, Stroke::new(line::FOCUS_RING_PX, color(s.accent_text)), StrokeKind::Inside);
                }
                let ink = match (enabled, on || hovered || pressed) {
                    (true, true) => s.text,
                    (true, false) | (false, true) => s.text_2,
                    (false, false) => s.text_3,
                };
                let g = if on { g } else { galley(ui, text, ui_type::BODY) };
                p.galley(seg.center() - g.size() / 2.0, g, color(ink));
            }
        }
        if *self.selected != before {
            response.mark_changed();
        }
        SegmentedResponse { response, clicked }
    }
}

impl Widget for Segmented<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        self.show(ui).response
    }
}

/// A segmented control.
pub fn segmented(ui: &mut Ui, selected: &mut usize, items: &[&str]) -> Response {
    ui.add(Segmented::new(selected, items))
}
