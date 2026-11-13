//! The right panel (spec §2): details only. In U2 it holds the ECAD properties section that was
//! in CADCraft's Properties palette ([`crate::sheets_ui::section`], design 019), unchanged; it
//! collapses to a 44 px rail. The Text panel opens over it ([`super::text_panel`]).

use ecad_ui::tokens::{Icon, size, space};
use eframe::egui::{self, Rect, pos2, vec2};

use super::Slots;
use crate::Trial;

/// Accessible labels.
pub const COLLAPSE: &str = "Collapse the details panel";
pub const EXPAND: &str = "Details";
pub const TITLE: &str = "PROPERTIES";

/// Draw the right panel (or its rail, or the Text panel over it).
pub fn show(t: &mut Trial, ui: &mut egui::Ui, slots: &Slots) {
    let s = ecad_ui::SHELL;
    let rect = slots.right;
    super::fill(ui, rect, s.panel);
    ui.painter().vline(rect.left() + 0.5, rect.y_range(), egui::Stroke::new(ecad_ui::tokens::line::HAIRLINE_PX, ecad_ui::color(s.line)));
    if t.shell.state.text_panel && !slots.right_rail {
        super::text_panel::show(t, ui, rect);
        return;
    }
    if slots.right_rail {
        let expand = super::in_rect(ui, rect.shrink(space::S), "ecad.shell.right.rail", |ui| {
            ui.vertical_centered(|ui| ui.add(ecad_ui::prim::IconButton::new(Icon::ChevronLeft, EXPAND)).clicked()).inner
        });
        if expand {
            t.shell.state.right_collapsed = false;
        }
        return;
    }
    let head = Rect::from_min_size(rect.min, vec2(rect.width(), size::PANEL_HEADER));
    let collapse = super::in_rect(ui, head.shrink2(vec2(space::L, space::XS)), "ecad.shell.right.head", |ui| {
        ui.horizontal_centered(|ui| {
            ecad_ui::prim::read_value(ui, TITLE);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(ecad_ui::prim::IconButton::new(Icon::Collapse, COLLAPSE)).clicked()
            })
            .inner
        })
        .inner
    });
    if collapse {
        t.shell.state.right_collapsed = true;
    }
    let body = Rect::from_min_max(pos2(rect.left() + space::L, head.bottom()), pos2(rect.right() - space::M, rect.bottom()));
    let section = &mut t.shell.right_section;
    super::in_rect(ui, body, "ecad.shell.right.body", |ui| {
        egui::ScrollArea::vertical().id_salt("ecad.shell.right.scroll").auto_shrink([false, false]).show(ui, |ui| section(ui));
    });
}
