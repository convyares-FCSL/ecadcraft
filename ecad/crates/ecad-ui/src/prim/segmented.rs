//! Segmented control: one of a few choices, side by side (the workspace switch is one).

use ecad_tokens::{radius, size, space, ui_type};
use egui::{Rect, Response, Role, Sense, Ui, Widget, WidgetInfo, pos2, vec2};

use super::{Look, accessible, galley};
use crate::color;
use crate::style::corner;

/// A segmented control over `items`, the selected index in `selected`. Each segment is a radio
/// button labelled with its text; the response reports `changed` when the selection changes.
#[derive(Debug)]
pub struct Segmented<'a> {
    selected: &'a mut usize,
    items: &'a [&'a str],
    /// A forced look for one segment (the showcase's hovered segment).
    look: Option<(usize, Look)>,
}

impl<'a> Segmented<'a> {
    pub fn new(selected: &'a mut usize, items: &'a [&'a str]) -> Self {
        Self { selected, items, look: None }
    }
    pub fn look(mut self, segment: usize, look: Look) -> Self {
        self.look = Some((segment, look));
        self
    }
}

impl Widget for Segmented<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let s = crate::SHELL;
        let h = size::CONTROL;
        let galleys: Vec<_> = self.items.iter().map(|t| galley(ui, t, ui_type::BODY_EMPHASIS)).collect();
        let widths: Vec<f32> = galleys.iter().map(|g| g.size().x + 2.0 * space::L).collect();
        let total = widths.iter().sum::<f32>() + 2.0 * space::XXS;
        let (outer, mut response) = ui.allocate_exact_size(vec2(total, h), Sense::hover());
        let visible = ui.is_rect_visible(outer);
        if visible {
            ui.painter().rect_filled(outer, corner(radius::BUTTON), color(s.raised));
        }
        let mut x = outer.left() + space::XXS;
        let before = *self.selected;
        for (i, (g, w)) in galleys.into_iter().zip(widths).enumerate() {
            let seg = Rect::from_min_max(pos2(x, outer.top() + space::XXS), pos2(x + w, outer.bottom() - space::XXS));
            x += w;
            let r = ui.interact(seg, response.id.with(i), Sense::click());
            let on = *self.selected == i;
            let text = self.items.get(i).copied().unwrap_or_default();
            accessible(&r, || WidgetInfo::selected(Role::RadioButton, true, on, text), None);
            if r.clicked() {
                *self.selected = i;
            }
            if visible {
                let look = self.look.filter(|(n, _)| *n == i).map_or(Look::Auto, |(_, l)| l);
                let fill = if on || look.pressed(&r) {
                    Some(s.active)
                } else if look.hovered(&r) {
                    Some(s.hover)
                } else {
                    None
                };
                if let Some(fill) = fill {
                    ui.painter().rect_filled(seg, corner(radius::CHIP), color(fill));
                }
                let ink = if on { s.text } else { s.text_2 };
                ui.painter().galley(seg.center() - g.size() / 2.0, g, color(ink));
            }
        }
        if *self.selected != before {
            response.mark_changed();
        }
        response
    }
}

/// A segmented control.
pub fn segmented(ui: &mut Ui, selected: &mut usize, items: &[&str]) -> Response {
    ui.add(Segmented::new(selected, items))
}
