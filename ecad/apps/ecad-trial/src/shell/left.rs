//! The panel slot (UI rev 3): beside the tool strip in Draw, one panel at a time, opened by the
//! strip's Book, Devices and History buttons and closed by its own close button or the same strip
//! button. Book, Devices and History host today's Project panel tabs unchanged
//! ([`crate::project_panel`]); each keeps its own width (Book and Devices 248 px, History 280 px)
//! and drags between 200 and 420 px at its inner edge. Each header has a pin: the pinned panel
//! docks and the drawing moves over for it; an unpinned one floats over the drawing with a shadow
//! and closes when drawing starts (a press on the canvas). One panel is pinned at a time.

use ecad_ui::tokens::{Icon, size, space};
use eframe::egui::{self, Color32, Rect, pos2, vec2};

use super::state::BookView;
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
/// Accessible label of the header's pin (UI rev 3: pinned docks, unpinned floats).
pub const PIN: &str = "Pin the panel";
/// Book's view switch (board 1.02).
pub const BOOK_LIST: &str = "Show the pages as a list";
pub const BOOK_THUMBS: &str = "Show the pages with thumbnails";
/// A header button's side.
const HEAD_BTN: f32 = 26.0;

impl Trial {
    /// Pin or unpin the open panel: one panel is pinned at a time, so pinning this one unpins the
    /// other.
    pub fn toggle_pin(&mut self) {
        let left = &mut self.shell.state.left;
        left.pinned = if left.pinned == Some(left.tab) { None } else { Some(left.tab) };
    }
}

/// Close an unpinned panel when drawing starts: a press on the canvas outside it (UI rev 3,
/// "Unpinned: … closes when you start drawing"). Read before the canvas takes the press.
pub fn close_on_draw(t: &mut Trial, ctx: &egui::Context, slots: &Slots) {
    let (Some(panel), true) = (slots.panel, slots.panel_floats) else { return };
    let pressed = ctx.input(|i| if i.pointer.primary_pressed() || i.pointer.secondary_pressed() { i.pointer.interact_pos() } else { None });
    if let Some(at) = pressed
        && slots.main.contains(at)
        && !panel.contains(at)
        && ctx.layer_id_at(at).is_none_or(|l| l.order == egui::Order::Background)
    {
        t.shell.state.left.collapsed = true;
    }
}

/// Draw the panel slot, when a panel is open: docked in the slot when pinned, else floating over
/// the drawing with a shadow.
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
    if slots.panel_floats {
        let area = egui::Area::new(egui::Id::unique("ecad.shell.left.float"))
            .order(egui::Order::Middle)
            .fixed_pos(rect.min)
            .constrain(false)
            .movable(false)
            .interactable(true);
        area.show(ui.ctx(), |ui| {
            let shadow = egui::epaint::Shadow { offset: [6, 0], blur: 18, spread: 0, color: Color32::from_black_alpha(110) };
            ui.painter().add(shadow.as_shape(rect, 0.0));
            ui.allocate_rect(rect, egui::Sense::hover());
            panel(t, ui, rect);
        });
    } else {
        panel(t, ui, rect);
    }
}

/// The panel at `rect`: its header (title, Book's view switch, pin, close) and its body.
fn panel(t: &mut Trial, ui: &mut egui::Ui, rect: Rect) {
    let s = ecad_ui::SHELL;
    super::fill(ui, rect, s.panel);
    ui.painter().vline(rect.right() - 0.5, rect.y_range(), egui::Stroke::new(ecad_ui::tokens::line::HAIRLINE_PX, ecad_ui::color(s.line)));
    let header = Rect::from_min_size(rect.min, vec2(rect.width(), size::PANEL_HEADER));
    let tab = t.shell.state.left.tab;
    super::bar::text_at(ui, header.left() + space::L, header.center().y, tab.label(), ecad_ui::tokens::ui_type::TITLE, ecad_ui::color(s.text));
    let y = header.center().y;
    let mut x = header.right() - space::S - HEAD_BTN;
    let at = |x: f32| Rect::from_center_size(pos2(x + HEAD_BTN / 2.0, y), vec2(HEAD_BTN, HEAD_BTN));
    if super::bar::icon_button(ui, at(x), Icon::Close, CLOSE, 14.0, true).clicked() {
        t.shell.state.left.collapsed = true;
    }
    x -= HEAD_BTN + space::XS;
    let pinned = t.shell.state.left.pinned == Some(tab);
    if glyph_button(ui, at(x), PIN, pinned, true, super::glyphs::pin).clicked() {
        t.toggle_pin();
    }
    if tab == LeftTab::Pages {
        let view = t.shell.state.left.book_view;
        x -= HEAD_BTN + space::XS;
        if glyph_button(ui, at(x), BOOK_THUMBS, view == BookView::Thumbnails, false, super::glyphs::thumbs).clicked() {
            t.shell.state.left.book_view = BookView::Thumbnails;
        }
        x -= HEAD_BTN;
        if glyph_button(ui, at(x), BOOK_LIST, view == BookView::List, false, super::glyphs::list).clicked() {
            t.shell.state.left.book_view = BookView::List;
        }
    }
    t.panel.thumbnails = t.shell.state.left.book_view == BookView::Thumbnails;
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

/// A header button at `rect` drawing a [`super::glyphs`] glyph; `on` shows it set (in the accent
/// when `accent`, as the pin, else the active ground, as Book's view switch).
fn glyph_button(ui: &mut egui::Ui, rect: Rect, label: &str, on: bool, accent: bool, glyph: fn(&egui::Painter, Rect, Color32)) -> egui::Response {
    let r = super::bar::hit(ui, rect, label, on, accent, true);
    let c = if r.hovered() && !on { ecad_ui::color(ecad_ui::SHELL.text) } else { super::bar::ink(on, accent, true) };
    glyph(ui.painter(), Rect::from_center_size(rect.center(), vec2(14.0, 14.0)), c);
    r
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
