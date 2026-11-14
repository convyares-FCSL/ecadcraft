//! The panel slot (UI rev 3): beside the tool strip in Draw, one panel at a time, opened by the
//! strip's Book, Devices and History buttons and closed by its own close button or the same strip
//! button. Book, Devices and History host today's Project panel tabs unchanged
//! ([`crate::project_panel`]); each keeps its own width (Book and Devices 248 px, History 280 px)
//! and drags between 200 and 420 px at its inner edge. Pinning and floating come later: the panel
//! docks, and the drawing moves over for it.

use ecad_ui::tokens::{Icon, size, space};
use eframe::egui::{self, Color32, Rect, pos2, vec2};

use super::{LeftTab, Slots};
use crate::Trial;
use crate::project_panel::Tab;

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

impl Trial {
    /// Open the panel slot on `tab`.
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

/// Accessible label of the panel's close button.
pub const CLOSE: &str = "Close the panel";

/// Draw the panel slot, when a panel is open.
pub fn show(t: &mut Trial, ui: &mut egui::Ui, slots: &Slots) {
    // The panel's tab follows the shell's (History may have been opened by the agent banner).
    let shown = match t.panel.tab {
        Tab::Devices => LeftTab::Devices,
        Tab::History => LeftTab::History,
        Tab::Pages | Tab::Checks => t.shell.state.left.tab,
    };
    if shown != t.shell.state.left.tab {
        // Asked for elsewhere (the agent banner's History): it opens.
        t.set_left_tab(shown);
    } else if t.panel.tab == Tab::Checks {
        // Checks live in the inspector now: the panel's own tab goes back, nothing opens.
        let closed = t.shell.state.left.collapsed;
        t.set_left_tab(shown);
        t.shell.state.left.collapsed = closed;
    }
    let Some(rect) = slots.panel else { return };
    let s = ecad_ui::SHELL;
    super::fill(ui, rect, s.panel);
    ui.painter().vline(rect.right() - 0.5, rect.y_range(), egui::Stroke::new(ecad_ui::tokens::line::HAIRLINE_PX, ecad_ui::color(s.line)));
    let header = Rect::from_min_size(rect.min, vec2(rect.width(), size::PANEL_HEADER));
    let title = t.shell.state.left.tab.label();
    let close = super::in_rect(ui, header.shrink2(vec2(space::L, space::XS)), "ecad.shell.left.head", |ui| {
        ui.horizontal_centered(|ui| {
            ui.label(egui::RichText::new(title).font(ecad_ui::fonts::font(ui.ctx(), ecad_ui::tokens::ui_type::TITLE)).color(ecad_ui::color(s.text)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(ecad_ui::prim::IconButton::new(Icon::Collapse, CLOSE)).clicked()
            })
            .inner
        })
        .inner
    });
    if close {
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
