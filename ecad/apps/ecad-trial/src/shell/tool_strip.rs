//! The tool strip (UI rev 3, "Tool strip (Draw)"): 56 px at the window's left edge, buttons
//! 54 × 44 with an 18 px icon over a 10 px label. Top to bottom:
//!
//! 1. the panels, Book, Devices and History, each with a › mark (‹ while open);
//! 2. a double divider;
//! 3. the tool groups: Select, Draw, Text | Device, Terminal, PLC | Wire, Cable | Location;
//! 4. at the foot, the Snap, Grid and Ortho toggles.
//!
//! Every group from Draw to Location carries a flyout's corner mark. A click starts the group's
//! last-used tool; hovering opens today's list of its tools (rev 3's flyouts come later); a
//! group with a key (V D T W C) starts its last-used tool, and pressed again steps to the next.
//! The toggles flip CADCraft's snap, grid and ortho modes (their popouts come later).
//!
//! Brief 041: the groups and their tools come from the action table
//! ([`ecad_cadcraft::actions`]): a tool is an action with a group, and a tool whose command does
//! not exist yet is not listed.
//!
//! [`ToolStrip`] also keeps lane U4b's other state between frames: the open wheel and the intent
//! line's cache and refusal.

use ecad_cadcraft::actions::{Action, ToolGroup};
use ecad_ui::tokens::ui_type::TypeStyle;
use ecad_ui::tokens::{Icon, line, size, space};
use eframe::egui::{self, Key, Rect, pos2, vec2};

use super::{LeftTab, bar};
use crate::Trial;

/// A tool group as the strip shows it.
#[derive(Debug, Clone, Copy)]
pub struct Group {
    pub group: ToolGroup,
    /// Its key, if it keeps one (UI rev 3: V D T W C).
    pub key: Option<(char, Key)>,
    pub icon: Icon,
    /// Its accessible label: with the key when it has one (`Wire (W)`), else `Terminal group`
    /// (its tools' own labels stay unique).
    pub label: &'static str,
}

impl Group {
    /// Its tools, in flyout order (the action table's).
    pub fn tools(&self) -> Vec<&'static Action> {
        self.group.tools()
    }

    /// Its key's letter.
    pub fn letter(&self) -> Option<char> {
        self.key.map(|(c, _)| c)
    }
}

const fn group(group: ToolGroup, key: Option<(char, Key)>, label: &'static str) -> Group {
    Group { group, key, icon: group.icon(), label }
}

/// The nine groups, in strip order (UI rev 3).
pub const GROUPS: [Group; 9] = [
    group(ToolGroup::Select, Some(('V', Key::V)), "Select (V)"),
    group(ToolGroup::Draw, Some(('D', Key::D)), "Draw (D)"),
    group(ToolGroup::Text, Some(('T', Key::T)), "Text (T)"),
    group(ToolGroup::Device, None, "Device group"),
    group(ToolGroup::Terminal, None, "Terminal group"),
    group(ToolGroup::Plc, None, "PLC group"),
    group(ToolGroup::Wire, Some(('W', Key::W)), "Wire (W)"),
    group(ToolGroup::Cable, Some(('C', Key::C)), "Cable (C)"),
    group(ToolGroup::Location, None, "Location group"),
];

/// The panels the strip opens, in order, with their icons.
pub const PANELS: [(LeftTab, Icon); 3] = [(LeftTab::Pages, Icon::Book), (LeftTab::Devices, Icon::DeviceGrid), (LeftTab::History, Icon::History)];

/// The toggles at the foot: label, icon, CADCraft's command.
pub const TOGGLES: [(&str, Icon, &str); 3] = [("Snap", Icon::Snap, "snap"), ("Grid", Icon::Grid, "grid"), ("Ortho", Icon::Ortho, "ortho")];

/// The flyout's accessible label for group `g`.
pub fn flyout_label(g: &Group) -> String {
    format!("{} tools", g.label)
}

/// The panel button's accessible label for `tab`.
pub fn panel_label(tab: LeftTab) -> String {
    format!("{} panel", tab.label())
}

/// The 10 px label under a button's icon.
const LABEL: TypeStyle = TypeStyle { size: size::TOOL_LABEL, line: 12.0, weight: 400 };
/// Space between buttons.
const GAP: f32 = 2.0;

/// The strip's state: each group's last-used tool, the open flyout, and the group whose tool
/// the last group key started (pressing it again steps); and lane U4b's wheel and intent line.
#[derive(Debug, Clone, Default)]
pub struct ToolStrip {
    pub last: [usize; 9],
    pub flyout: Option<usize>,
    stepping: Option<usize>,
    /// The wheel, while it is open.
    pub wheel: Option<super::wheel::Open>,
    /// A command was running at the end of the last frame (a right-click then finishes its step
    /// and opens no wheel).
    pub was_running: bool,
    /// The intent line's state, and its text last frame (tests read it).
    pub intent: super::intent::Intent,
    pub intent_text: String,
}

impl Trial {
    /// Start a tool (an action with a group): its command, with nothing picked yet.
    pub fn start_tool(&mut self, a: &'static Action) {
        if let Err(e) = self.run_action(a, None, None) {
            self.cad.session.echo(e);
        }
    }

    /// Whether the running command is the tool group `gi`'s key last started.
    pub fn group_tool_running(&self, gi: usize) -> bool {
        if self.shell.tools.stepping != Some(gi) {
            return false;
        }
        self.group_running(gi)
    }

    /// Whether the running command is group `gi`'s last-used tool.
    fn group_running(&self, gi: usize) -> bool {
        let last = self.shell.tools.last.get(gi).copied().unwrap_or(0);
        let tools = GROUPS.get(gi).map(Group::tools).unwrap_or_default();
        let command = tools.get(last).map(|a| a.command).filter(|c| !c.is_empty());
        command.is_some_and(|c| self.cad.session.running.as_ref().is_some_and(|r| r.id.eq_ignore_ascii_case(c)))
    }

    /// A group key (V D T W C): the group's last-used tool, or the next one when the key was the
    /// last pressed and its tool still runs.
    pub fn group_key(&mut self, letter: char) {
        let Some(gi) = GROUPS.iter().position(|g| g.letter() == Some(letter)) else { return };
        let Some(g) = GROUPS.get(gi) else { return };
        let tools = g.tools();
        if tools.is_empty() {
            return;
        }
        let last = self.shell.tools.last.get(gi).copied().unwrap_or(0).min(tools.len() - 1);
        let again = self.shell.tools.stepping == Some(gi) && self.cad.session.running.is_some();
        let next = if again { (last + 1) % tools.len() } else { last };
        if let Some(slot) = self.shell.tools.last.get_mut(gi) {
            *slot = next;
        }
        self.shell.tools.stepping = Some(gi);
        if let Some(a) = tools.get(next).copied() {
            self.start_tool(a);
        }
    }

    /// Open the panel slot on `tab`, or close it when that panel is already open.
    pub fn toggle_panel(&mut self, tab: LeftTab) {
        if !self.shell.state.left.collapsed && self.shell.state.left.tab == tab {
            self.shell.state.left.collapsed = true;
        } else {
            self.set_left_tab(tab);
        }
    }
}

/// How a strip button is drawn.
#[derive(Clone, Copy, PartialEq)]
enum Look {
    Plain,
    /// The tool in use: the accent.
    Current,
    /// An open panel or an open flyout.
    On,
    /// A toggle that is on: the soft accent, its icon and label in the accent text (the board's).
    Toggle,
}

/// One strip button at `r`: the icon over its label, a hover fill, and its accessible label.
fn button(ui: &mut egui::Ui, r: Rect, icon: Icon, text: &str, label: &str, look: Look, enabled: bool) -> egui::Response {
    let s = ecad_ui::SHELL;
    let resp = bar::hit(ui, r, label, matches!(look, Look::Current | Look::On), look == Look::Current, enabled);
    if look == Look::Toggle {
        ui.painter().rect_filled(r, ecad_ui::style::corner(ecad_ui::tokens::radius::TOOL), ecad_ui::color(s.accent_soft));
    }
    let (icon_ink, text_ink) = match look {
        Look::Current => (ecad_ui::color(ecad_ui::tokens::WHITE), ecad_ui::color(ecad_ui::tokens::WHITE)),
        Look::On => (ecad_ui::color(s.accent_text), ecad_ui::color(s.text)),
        Look::Toggle => (ecad_ui::color(s.accent_text), ecad_ui::color(s.accent_text)),
        Look::Plain if !enabled => (ecad_ui::color(s.text_3), ecad_ui::color(s.text_3)),
        Look::Plain if resp.hovered() => (ecad_ui::color(s.text), ecad_ui::color(s.text)),
        Look::Plain => (ecad_ui::color(s.text_2), ecad_ui::color(s.text_2)),
    };
    let icon_rect = Rect::from_center_size(pos2(r.center().x, r.top() + 6.0 + size::TOOL_ICON / 2.0), vec2(size::TOOL_ICON, size::TOOL_ICON));
    ecad_ui::icons::paint(ui.painter(), icon_rect, icon, icon_ink);
    ui.painter().text(pos2(r.center().x, r.bottom() - 9.0), egui::Align2::CENTER_CENTER, text, bar::font(ui, LABEL), text_ink);
    resp
}

/// A flyout's corner mark: a small triangle in the button's bottom-right corner.
fn corner_mark(ui: &egui::Ui, r: Rect, look: Look) {
    let s = ecad_ui::SHELL;
    let c = if look == Look::Current { ecad_ui::color(ecad_ui::tokens::WHITE) } else { ecad_ui::color(s.text_3) };
    let (x, y) = (r.right() - 3.0, r.bottom() - 3.0);
    ui.painter().add(egui::Shape::convex_polygon(vec![pos2(x, y - 4.0), pos2(x, y), pos2(x - 4.0, y)], c, egui::Stroke::NONE));
}

/// A panel button's › (or ‹ while open) at its right edge, centred on the icon.
fn panel_mark(ui: &egui::Ui, r: Rect, open: bool) {
    let c = ecad_ui::color(ecad_ui::SHELL.text_3);
    let at = pos2(r.right() - 4.0, r.top() + 6.0 + size::TOOL_ICON / 2.0);
    let icon = if open { Icon::ChevronLeft } else { Icon::ChevronRight };
    ecad_ui::icons::paint(ui.painter(), Rect::from_center_size(at, vec2(8.0, 8.0)), icon, c);
}

/// A divider across the strip at `y` (two hairlines for a double one).
fn divider(ui: &egui::Ui, rect: Rect, y: f32, double: bool) {
    let stroke = egui::Stroke::new(line::HAIRLINE_PX, ecad_ui::color(ecad_ui::SHELL.line_strong));
    let xs = (rect.left() + space::M)..=(rect.right() - space::M);
    ui.painter().hline(xs.clone(), y.round() + 0.5, stroke);
    if double {
        ui.painter().hline(xs, y.round() + 3.5, stroke);
    }
}

/// Draw the strip in `rect`.
pub fn show(t: &mut Trial, ui: &mut egui::Ui, rect: Rect) {
    let s = ecad_ui::SHELL;
    super::fill(ui, rect, s.panel);
    ui.painter().vline(rect.right() - 0.5, rect.y_range(), egui::Stroke::new(line::HAIRLINE_PX, ecad_ui::color(s.line)));
    let x = rect.center().x - size::TOOL_BUTTON_W / 2.0;
    let at = |y: f32| Rect::from_min_size(pos2(x, y), vec2(size::TOOL_BUTTON_W, size::TOOL_BUTTON));
    let mut y = rect.top() + space::S;

    // The panels.
    let mut panel = None;
    for (tab, icon) in PANELS {
        let r = at(y);
        y += size::TOOL_BUTTON + GAP;
        let open = !t.shell.state.left.collapsed && t.shell.state.left.tab == tab;
        let look = if open { Look::On } else { Look::Plain };
        if button(ui, r, icon, tab.label(), &panel_label(tab), look, true).clicked() {
            panel = Some(tab);
        }
        panel_mark(ui, r, open);
    }
    y += space::S;
    divider(ui, rect, y, true);
    y += space::M + 2.0;

    // The tool groups.
    let mut start = None;
    let mut hovered_any = false;
    let running = t.cad.session.running.is_some();
    for (gi, g) in GROUPS.iter().enumerate() {
        let tools = g.tools();
        let r = at(y);
        y += size::TOOL_BUTTON + GAP;
        let last = t.shell.tools.last.get(gi).copied().unwrap_or(0);
        let current = tools.get(last).or(tools.first()).copied();
        let tip = match current {
            Some(a) => format!("{}: {}", g.label, a.label),
            None => format!("{}: no tools yet", g.label),
        };
        // Select is current while nothing runs; a group while its last-used tool runs.
        let is_current = if g.group == ToolGroup::Select { !running } else { t.group_running(gi) };
        let look = if is_current {
            Look::Current
        } else if t.shell.tools.flyout == Some(gi) {
            Look::On
        } else {
            Look::Plain
        };
        let resp = button(ui, r, g.icon, g.group.name(), g.label, look, !tools.is_empty()).on_hover_text(tip);
        if g.group != ToolGroup::Select {
            corner_mark(ui, r, look);
        }
        if resp.clicked() {
            start = current.map(|a| (gi, last, a));
            t.shell.tools.flyout = None;
        }
        if resp.hovered() && !tools.is_empty() {
            t.shell.tools.flyout = Some(gi);
            hovered_any = true;
        }
        if t.shell.tools.flyout == Some(gi) {
            let (chosen, over) = flyout(ui, g, &tools, r);
            hovered_any |= over;
            if let Some(i) = chosen {
                start = tools.get(i).map(|a| (gi, i, *a));
                t.shell.tools.flyout = None;
            }
        }
        if g.group.divider_after() {
            y += space::XS;
            divider(ui, rect, y, false);
            y += space::XS + 2.0;
        }
    }
    if !hovered_any {
        t.shell.tools.flyout = None;
    }

    // The toggles at the foot.
    let settings = t.cad.session.settings.clone();
    let mut toggle = None;
    let mut y = rect.bottom() - space::S - TOGGLES.len() as f32 * (size::TOOL_BUTTON + GAP) + GAP;
    divider(ui, rect, y - space::S, false);
    for (label, icon, command) in TOGGLES {
        let r = at(y);
        y += size::TOOL_BUTTON + GAP;
        let on = match command {
            "snap" => settings.snapmode,
            "grid" => settings.gridmode,
            _ => settings.orthomode,
        };
        let look = if on { Look::Toggle } else { Look::Plain };
        let resp = button(ui, r, icon, label, label, look, true).on_hover_text(format!("{label}: {}", if on { "on" } else { "off" }));
        corner_mark(ui, r, look);
        if resp.clicked() {
            toggle = Some(command);
        }
    }

    if let Some(tab) = panel {
        t.toggle_panel(tab);
    }
    if let Some(c) = toggle {
        let _ = t.cad.run(c, serde_json::json!({}));
    }
    if let Some((gi, i, a)) = start {
        if let Some(slot) = t.shell.tools.last.get_mut(gi) {
            *slot = i;
        }
        t.shell.tools.stepping = None;
        t.start_tool(a);
    }
}

/// The flyout of `g` beside its button: the tool clicked, and whether the pointer is over it.
fn flyout(ui: &mut egui::Ui, g: &Group, tools: &[&'static Action], button: Rect) -> (Option<usize>, bool) {
    let label = flyout_label(g);
    let area = egui::Area::new(egui::Id::unique(("ecad.shell.flyout", g.label)))
        .order(egui::Order::Foreground)
        .fixed_pos(pos2(button.right() + space::XS, button.top()))
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style())
                .show(ui, |ui| {
                    ui.set_min_width(180.0);
                    let mut chosen = None;
                    ecad_ui::prim::read_value(ui, &label);
                    for (i, a) in tools.iter().enumerate() {
                        if ui.add(ecad_ui::prim::Button::new(a.label).kind(ecad_ui::prim::Kind::Quiet).small()).clicked() {
                            chosen = Some(i);
                        }
                    }
                    chosen
                })
                .inner
        });
    let over = ui.ctx().pointer_hover_pos().is_some_and(|p| area.response.rect.expand(space::S).contains(p) || button.expand(space::S).contains(p));
    (area.inner, over)
}
