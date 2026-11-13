//! The left panel (spec §2): navigation only, the same in every workspace. Tabs Pages, Devices
//! and History host today's Project panel tabs unchanged ([`crate::project_panel`]); each tab
//! keeps its own width; the panel drags between 200 and 420 px at its inner edge and collapses to
//! a 44 px rail of the tabs' icons.

use ecad_ui::tokens::{Icon, size, space};
use eframe::egui::{self, Color32, Rect, pos2, vec2};

use super::{LeftTab, Slots};
use crate::Trial;
use crate::project_panel::{self, Tab};

/// The colours the moved panels draw with, from the shell's tokens (they used CADCraft's theme
/// before brief 038). Named as the CADCraft tokens they replace, so their code reads the same.
#[derive(Debug, Clone, Copy)]
pub struct Colours {
    pub text: Color32,
    pub text_dim: Color32,
    pub text_faint: Color32,
    pub accent: Color32,
    pub accent_text: Color32,
    /// A hovered row.
    pub chrome: Color32,
    /// A bar's ground (the branch and revision bars).
    pub chrome_dark: Color32,
    /// The active row.
    pub tab_active: Color32,
    pub control_hover: Color32,
    pub warn: Color32,
    pub error: Color32,
}

/// The moved panels' colours.
pub fn colours() -> Colours {
    let s = ecad_ui::SHELL;
    let c = ecad_ui::color;
    Colours {
        text: c(s.text),
        text_dim: c(s.text_2),
        text_faint: c(s.text_3),
        accent: c(s.accent),
        accent_text: c(s.accent_text),
        chrome: c(s.hover),
        chrome_dark: c(s.raised),
        tab_active: c(s.active),
        control_hover: c(s.hover),
        warn: c(s.warning),
        error: c(s.error),
    }
}

fn tab_icon(tab: LeftTab) -> Icon {
    match tab {
        LeftTab::Pages => Icon::Pages,
        LeftTab::Devices => Icon::Devices,
        LeftTab::History => Icon::History,
    }
}

impl Trial {
    /// Show a left tab (the panel expands if it was a rail).
    pub fn set_left_tab(&mut self, tab: LeftTab) {
        let panel_tab = match tab {
            LeftTab::Pages => Tab::Pages,
            LeftTab::Devices => Tab::Devices,
            LeftTab::History => Tab::History,
        };
        if self.panel.tab != panel_tab {
            // Shown again after being hidden: rebuilt once, then kept up to date.
            match panel_tab {
                Tab::Devices => self.panel.devices.invalidate(),
                Tab::History => self.panel.history.invalidate(),
                Tab::Pages | Tab::Checks => {}
            }
        }
        self.panel.tab = panel_tab;
        self.panel.armed = false;
        self.shell.state.left.tab = tab;
        self.shell.state.left.collapsed = false;
    }
}

/// Draw the left panel (or its rail).
pub fn show(t: &mut Trial, ui: &mut egui::Ui, slots: &Slots) {
    let s = ecad_ui::SHELL;
    let rect = slots.left;
    super::fill(ui, rect, s.panel);
    ui.painter().vline(rect.right() - 0.5, rect.y_range(), egui::Stroke::new(ecad_ui::tokens::line::HAIRLINE_PX, ecad_ui::color(s.line)));
    // The panel's tab follows the shell's (History may have been opened by the agent banner).
    let shown = match t.panel.tab {
        Tab::Devices => LeftTab::Devices,
        Tab::History => LeftTab::History,
        Tab::Pages | Tab::Checks => t.shell.state.left.tab,
    };
    if shown != t.shell.state.left.tab || t.panel.tab == Tab::Checks {
        t.set_left_tab(shown);
    }
    if slots.left_rail {
        rail(t, ui, rect);
        return;
    }
    let header = Rect::from_min_size(rect.min, vec2(rect.width(), size::PANEL_HEADER));
    let mut pick = None;
    let mut collapse = false;
    super::in_rect(ui, header.shrink2(vec2(space::M, space::XS)), "ecad.shell.left.tabs", |ui| {
        ui.horizontal_centered(|ui| {
            for tab in LeftTab::ALL {
                let on = t.shell.state.left.tab == tab;
                let r = ecad_ui::prim::Chip::new(tab.label()).selected(on).show(ui).response;
                r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, on, tab.label()));
                if r.clicked() {
                    pick = Some(tab);
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                collapse = ui.add(ecad_ui::prim::IconButton::new(Icon::Collapse, project_panel::COLLAPSE)).clicked();
            });
        });
    });
    if let Some(tab) = pick {
        t.set_left_tab(tab);
    }
    if collapse {
        t.shell.state.left.collapsed = true;
    }
    let body = Rect::from_min_max(pos2(rect.left() + space::S, header.bottom()), pos2(rect.right() - space::S, rect.bottom()));
    let head = t.header();
    if t.panel.tab == Tab::Devices && t.lists_shows_devices() {
        super::in_rect(ui, body, "ecad.shell.left.body", |ui| ecad_ui::prim::read_value(ui, super::DEVICES_IN_LISTS));
        resize_edge(t, ui, rect);
        return;
    }
    let actions = super::in_rect(ui, body, "ecad.shell.left.body", |ui| t.panel.show(ui, &head));
    for a in actions {
        t.panel_action(a);
    }
    resize_edge(t, ui, rect);
}

/// The collapsed rail: one icon per tab; a click opens the panel on that tab.
fn rail(t: &mut Trial, ui: &mut egui::Ui, rect: Rect) {
    let mut pick = None;
    super::in_rect(ui, rect.shrink(space::S), "ecad.shell.left.rail", |ui| {
        ui.vertical_centered(|ui| {
            let expand = ui.add(ecad_ui::prim::IconButton::new(Icon::ChevronRight, project_panel::EXPAND));
            if expand.clicked() {
                pick = Some(t.shell.state.left.tab);
            }
            for tab in LeftTab::ALL {
                if ui.add(ecad_ui::prim::IconButton::new(tab_icon(tab), tab.label()).selected(t.shell.state.left.tab == tab)).clicked() {
                    pick = Some(tab);
                }
            }
        });
    });
    if let Some(tab) = pick {
        t.set_left_tab(tab);
    }
}

/// The inner edge: drag to resize the tab's width (200–420), double-click for its default.
fn resize_edge(t: &mut Trial, ui: &mut egui::Ui, rect: Rect) {
    let edge = Rect::from_min_max(pos2(rect.right() - space::XS, rect.top()), pos2(rect.right() + space::XS, rect.bottom()));
    let r = ui.interact(edge, egui::Id::unique("ecad.shell.left.edge"), egui::Sense::click_and_drag());
    if r.hovered() || r.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }
    let tab = t.shell.state.left.tab;
    let Some(w) = t.shell.state.left.width_per_tab.get_mut(tab.index()) else { return };
    if r.double_clicked() {
        *w = super::layout::default_width(tab);
    } else if r.dragged()
        && let Some(p) = r.interact_pointer_pos()
    {
        *w = (p.x - rect.left()).clamp(super::layout::LEFT_MIN, super::layout::LEFT_MAX);
    }
}
