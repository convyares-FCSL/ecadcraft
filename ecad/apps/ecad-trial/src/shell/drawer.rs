//! Brief 048's drawer under the canvas: the symbol review ("Before saving") in the builder, or
//! Compare. UI rev 3 has no drawer otherwise (Checks are the inspector's tab), so it shows only
//! while one of them is open: its header names it, and its body is the review
//! ([`crate::library::review`]). Closing the review (its own Back or Close) closes the drawer.

use ecad_ui::tokens::space;
use eframe::egui::{self, Rect, pos2, vec2};

use super::Slots;
use crate::Trial;

/// Draw the drawer, while a review or Compare is shown.
pub fn show(t: &mut Trial, ui: &mut egui::Ui, slots: &Slots) {
    let Some(rect) = slots.drawer else { return };
    let Some(title) = t.drawer_review_tab() else { return };
    let s = ecad_ui::SHELL;
    super::fill(ui, rect, s.panel);
    ui.painter().hline(rect.x_range(), rect.top() + 0.5, egui::Stroke::new(ecad_ui::tokens::line::HAIRLINE_PX, ecad_ui::color(s.line)));
    let head = Rect::from_min_size(rect.min, vec2(rect.width(), super::layout::DRAWER_HEADER));
    super::in_rect(ui, head.shrink2(vec2(space::M, space::XS)), "ecad.shell.drawer.head", |ui| {
        ui.horizontal_centered(|ui| {
            let r = ecad_ui::prim::Chip::new(title).selected(true).show(ui).response;
            r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, true, title));
        });
    });
    if !t.project_open() {
        return;
    }
    let body = Rect::from_min_max(pos2(rect.left() + space::M, head.bottom()), pos2(rect.right() - space::M, rect.bottom()));
    super::in_rect(ui, body, crate::library::review::BODY, |ui| t.library_drawer(ui));
}
