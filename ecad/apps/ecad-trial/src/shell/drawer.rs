//! The drawer under the main view (spec §2): Checks, 150 px open under a 36 px header row that
//! stays when it is collapsed. The body is today's Checks tab ([`crate::checks_panel`]), as it is
//! (U3 converts it to the table).

use ecad_ui::tokens::{Icon, space};
use eframe::egui::{self, Rect, pos2, vec2};

use super::Slots;
use crate::Trial;

/// Accessible labels.
pub const OPEN: &str = "Open the Checks drawer";
pub const CLOSE: &str = "Close the Checks drawer";

/// Draw the drawer.
pub fn show(t: &mut Trial, ui: &mut egui::Ui, slots: &Slots) {
    let s = ecad_ui::SHELL;
    let rect = slots.drawer;
    super::fill(ui, rect, s.panel);
    ui.painter().hline(rect.x_range(), rect.top() + 0.5, egui::Stroke::new(ecad_ui::tokens::line::HAIRLINE_PX, ecad_ui::color(s.line)));
    let head = Rect::from_min_size(rect.min, vec2(rect.width(), super::layout::DRAWER_HEADER));
    let title = t.panel.checks.title();
    let open = slots.drawer_open;
    let mut toggle = false;
    super::in_rect(ui, head.shrink2(vec2(space::M, space::XS)), "ecad.shell.drawer.head", |ui| {
        ui.horizontal_centered(|ui| {
            let r = ecad_ui::prim::Chip::new(&title).selected(open).show(ui).response;
            r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, open, crate::checks_panel::CHECKS_TAB));
            if r.clicked() && !open {
                toggle = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (icon, label) = if open { (Icon::ChevronDown, CLOSE) } else { (Icon::ChevronUp, OPEN) };
                toggle |= ui.add(ecad_ui::prim::IconButton::new(icon, label)).clicked();
            });
        });
    });
    if toggle {
        t.shell.state.drawer_open = !open;
    }
    if !open {
        return;
    }
    let body = Rect::from_min_max(pos2(rect.left() + space::M, head.bottom()), pos2(rect.right() - space::M, rect.bottom()));
    let writable = {
        let h = t.header();
        h.name.is_some() && !h.read_only && t.panel.revision.is_none()
    };
    if !t.project_open() {
        return;
    }
    if t.lists_shows_checks() {
        super::in_rect(ui, body, "ecad.shell.drawer.body", |ui| ecad_ui::prim::read_value(ui, super::CHECKS_IN_LISTS));
        return;
    }
    let actions = super::in_rect(ui, body, "ecad.shell.drawer.body", |ui| t.panel.checks.show(ui, writable));
    for a in actions {
        t.panel_action(a);
    }
}
