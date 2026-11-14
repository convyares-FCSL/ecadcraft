//! The tool strip (UI rev 3, "Tool strip (Draw)"): 56 px at the window's left edge, buttons
//! 54 × 44 with an 18 px icon over a 10 px label. Top to bottom:
//!
//! 1. the panels, Book, Devices and History, each with a › mark (‹ while open);
//! 2. a double divider;
//! 3. the tool groups: Select, Draw, Text | Device, Terminal, PLC | Wire, Cable | Location;
//! 4. at the foot, the Snap, Grid and Ortho toggles.
//!
//! Every group from Draw to Location carries a flyout's corner mark. A click starts the group's
//! last-used tool; hovering opens its flyout (boards 1.09 and 1.10): the group's name and key,
//! a row per item with its icon and key cap, the last-used tool lit, a placeholder greyed in
//! place with its hover saying why, and for some groups a note and a link at the foot (Terminal:
//! the Terminal manager). A group with a key (V D T W C) starts its last-used tool, and pressed
//! again steps to the next. The toggles flip CADCraft's snap, grid and ortho modes; hovering one
//! opens its popout ([`super::toggles`]).
//!
//! Brief 041: the groups and their items come from the action table
//! ([`ecad_cadcraft::actions`]): an item is an action with a group (a tool, a window action, or a
//! placeholder for a tool the board names that has no command yet).
//!
//! [`ToolStrip`] also keeps lane U4b's other state between frames: the open wheel and the intent
//! line's cache and refusal; and UI rev 3's picker and connector maker.

use ecad_cadcraft::actions::{self, Action, ToolGroup};
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
    /// The toggle (index in [`TOGGLES`]) whose popout is open.
    pub popout: Option<usize>,
    stepping: Option<usize>,
    /// The wheel, while it is open.
    pub wheel: Option<super::wheel::Open>,
    /// A command was running at the end of the last frame (a right-click then finishes its step
    /// and opens no wheel).
    pub was_running: bool,
    /// The intent line's state, and its text last frame (tests read it).
    pub intent: super::intent::Intent,
    pub intent_text: String,
    /// UI rev 3: the symbol picker while placing, and the folded chip's text last frame (tests).
    pub placing: super::placing::State,
    pub chip_text: String,
    /// UI rev 3: the connector maker's answers while Device ▸ Connector runs.
    pub maker: super::connector::State,
    /// The right edge of the picker, its chip or the maker this frame: the intent line starts
    /// clear of it.
    pub panel_right: Option<f32>,
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

    /// The one group lit for the running tool: the group whose last-used tool it is, else the
    /// first group that lists it (Bridge is in Terminal and Wire; only one lights).
    pub fn lit_group(&self) -> Option<usize> {
        (0..GROUPS.len()).find(|gi| self.group_running(*gi)).or_else(|| GROUPS.iter().position(|g| self.group_has_running(&g.tools())))
    }

    /// Whether the running command is one of `tools` (a tool, not a window action).
    fn group_has_running(&self, tools: &[&'static Action]) -> bool {
        let Some(r) = self.cad.session.running.as_ref() else { return false };
        tools.iter().any(|a| a.is_tool() && !a.command.is_empty() && r.id.eq_ignore_ascii_case(a.command))
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
    let mut link = None;
    let mut hovered_any = false;
    let running = t.cad.session.running.is_some();
    let writable = t.writable();
    let lit = t.lit_group();
    for (gi, g) in GROUPS.iter().enumerate() {
        let tools = g.tools();
        let r = at(y);
        y += size::TOOL_BUTTON + GAP;
        let last = t.shell.tools.last.get(gi).copied().unwrap_or(0);
        let current = tools.get(last).or(tools.first()).copied();
        // Select is current while nothing runs; a group while one of its tools runs.
        let is_current = if g.group == ToolGroup::Select { !running } else { lit == Some(gi) };
        let look = if is_current {
            Look::Current
        } else if t.shell.tools.flyout == Some(gi) {
            Look::On
        } else {
            Look::Plain
        };
        // The flyout is the group's hover (it names the tools), so the button has no tooltip;
        // Select, which has no flyout, names itself.
        let mut resp = button(ui, r, g.icon, g.group.name(), g.label, look, !tools.is_empty());
        if g.group == ToolGroup::Select {
            resp = resp.on_hover_text(g.label);
        }
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
            let (chosen, over) = flyout(ui, g, &tools, last, writable, r);
            hovered_any |= over;
            match chosen {
                Some(Chosen::Tool(i)) => start = tools.get(i).map(|a| (gi, i, *a)),
                Some(Chosen::Link) => link = g.group.link(),
                None => {}
            }
            if chosen.is_some() {
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
    // The toggles at the foot: a click flips the mode, hovering opens its popout.
    let settings = t.cad.session.drafting_settings();
    let mut toggle = None;
    let mut popout_cmds = Vec::new();
    let mut over_popout = false;
    let mut y = rect.bottom() - space::S - TOGGLES.len() as f32 * (size::TOOL_BUTTON + GAP) + GAP;
    divider(ui, rect, y - space::S, false);
    for (ti, (label, icon, command)) in TOGGLES.into_iter().enumerate() {
        let r = at(y);
        y += size::TOOL_BUTTON + GAP;
        let on = super::toggles::is_on(&settings, command);
        let look = if on { Look::Toggle } else { Look::Plain };
        let resp = button(ui, r, icon, label, label, look, true);
        corner_mark(ui, r, look);
        if resp.clicked() {
            toggle = Some(command);
        }
        if resp.hovered() {
            t.shell.tools.popout = Some(ti);
        }
        if t.shell.tools.popout == Some(ti) {
            let (cmds, over) = super::toggles::popout(ui, command, &settings, r);
            popout_cmds = cmds;
            over_popout |= over;
        }
    }
    if !over_popout {
        t.shell.tools.popout = None;
    }

    if let Some(tab) = panel {
        t.toggle_panel(tab);
    }
    if let Some(c) = toggle {
        for (cmd, params) in super::toggles::click(&settings, c) {
            let _ = t.cad.run(cmd, params);
        }
    }
    for (cmd, params) in popout_cmds {
        let _ = t.cad.run(cmd, params);
    }
    if let Some((gi, i, a)) = start {
        // A tool becomes the group's last-used one; a window action or a placeholder does not.
        if a.is_tool()
            && let Some(slot) = t.shell.tools.last.get_mut(gi)
        {
            *slot = i;
        }
        t.shell.tools.stepping = None;
        t.start_tool(a);
    }
    if let Some(a) = link {
        t.start_tool(a);
    }
}

/// A flyout's row height, and its narrowest width (UI rev 3, boards 1.09 and 1.10).
pub const FLYOUT_ROW: f32 = 28.0;
pub const FLYOUT_MIN_W: f32 = 248.0;

/// What a click in a flyout chose: one of its tools (by index), or its foot's link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chosen {
    Tool(usize),
    Link,
}

/// A popover beside the strip (a flyout or a toggle's popout): the board's frame, at `x`, centred
/// on `button` and kept inside the window. `width` and `height` are its content's size.
pub(crate) fn popover<R>(ui: &mut egui::Ui, id: &str, button: Rect, size: egui::Vec2, add: impl FnOnce(&mut egui::Ui, Rect) -> R) -> (R, Rect) {
    let screen = ui.ctx().content_rect();
    let top =
        (button.center().y - size.y / 2.0).clamp(screen.top() + size::TOP_BAR + space::XS, (screen.bottom() - size.y - space::XS).max(screen.top()));
    let rect = Rect::from_min_size(pos2(button.right() + space::XS, top.round()), size);
    let area =
        egui::Area::new(egui::Id::unique(("ecad.shell.popover", id))).order(egui::Order::Foreground).fixed_pos(rect.min).show(ui.ctx(), |ui| {
            let s = ecad_ui::SHELL;
            let corner = ecad_ui::style::corner(ecad_ui::tokens::radius::POPOVER);
            ui.painter().add(ecad_ui::style::shadow().as_shape(rect, corner));
            ui.painter().rect(
                rect,
                corner,
                ecad_ui::color(s.panel),
                egui::Stroke::new(line::HAIRLINE_PX, ecad_ui::color(s.line_strong)),
                egui::StrokeKind::Inside,
            );
            let (_, resp) = ui.allocate_exact_size(size, egui::Sense::hover());
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Window, true, id));
            add(ui, rect)
        });
    (area.inner, rect)
}

/// One flyout row at `r`: icon, label and key cap, lit when it is the group's last-used tool,
/// greyed when it cannot run (its hover says why).
fn flyout_row(ui: &mut egui::Ui, r: Rect, a: &Action, current: bool, greyed: Option<&str>) -> egui::Response {
    let s = ecad_ui::SHELL;
    let resp =
        ui.interact(r, egui::Id::unique(("ecad.shell.flyout.row", a.id)), if greyed.is_some() { egui::Sense::hover() } else { egui::Sense::click() });
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, greyed.is_none(), current, a.label));
    let row = r.shrink2(vec2(space::XS, 1.0));
    if current {
        ui.painter().rect_filled(row, ecad_ui::style::corner(ecad_ui::tokens::radius::BUTTON), ecad_ui::color(s.accent_soft));
    } else if resp.hovered() && greyed.is_none() {
        ui.painter().rect_filled(row, ecad_ui::style::corner(ecad_ui::tokens::radius::BUTTON), ecad_ui::color(s.hover));
    }
    let ink = if greyed.is_some() { s.text_3 } else { s.text };
    let icon = Rect::from_center_size(pos2(r.left() + space::L + 8.0, r.center().y), vec2(16.0, 16.0));
    ecad_ui::icons::paint(ui.painter(), icon, a.icon, ecad_ui::color(if greyed.is_some() { s.text_3 } else { s.text_2 }));
    ui.painter().text(
        pos2(icon.right() + space::L, r.center().y),
        egui::Align2::LEFT_CENTER,
        a.label,
        bar::font(ui, ecad_ui::tokens::ui_type::BODY),
        ecad_ui::color(ink),
    );
    if let Some(k) = a.key {
        keycap_at(ui, pos2(r.right() - space::M, r.center().y), k.words());
    }
    match greyed {
        Some(why) => resp.on_hover_text(why),
        None => resp,
    }
}

/// A key cap whose right edge is at `right`, centred on its y.
fn keycap_at(ui: &mut egui::Ui, right: egui::Pos2, keys: &str) {
    let w = bar::text_width(ui, keys, ecad_ui::tokens::ui_type::CAPTION) + 2.0 * space::S;
    let r = Rect::from_min_size(pos2(right.x - w, right.y - 9.0), vec2(w, 18.0));
    ui.scope_builder(egui::UiBuilder::new().max_rect(r), |ui| ecad_ui::prim::keycap(ui, keys));
}

/// The flyout of `g` beside its button (UI rev 3, boards 1.09 and 1.10): a header with the
/// group's name and key, a row per item (icon, label, key cap; greyed placeholders in place) and,
/// for some groups, a note with a link at its foot. Returns what was clicked, and whether the
/// pointer is over the flyout or its button.
fn flyout(ui: &mut egui::Ui, g: &Group, tools: &[&'static Action], last: usize, writable: bool, button: Rect) -> (Option<Chosen>, bool) {
    let label = flyout_label(g);
    let s = ecad_ui::SHELL;
    let note = g.group.note();
    let link = g.group.link();
    let body = ecad_ui::tokens::ui_type::BODY;
    let widest = tools.iter().map(|a| bar::text_width(ui, a.label, body)).fold(0.0, f32::max);
    let width = (widest + 2.0 * space::L + 16.0 + 64.0).max(if link.is_some() { 280.0 } else { FLYOUT_MIN_W }).round();
    let note_h = if note.is_some() { 64.0 } else { 0.0 };
    let height = FLYOUT_ROW + tools.len() as f32 * FLYOUT_ROW + space::S + note_h;
    let cx = actions::Context { target: None, writable };
    let (chosen, rect) = popover(ui, &label, button, vec2(width, height), |ui, rect| {
        let mut chosen = None;
        // Header: the group's name and its key.
        let head = Rect::from_min_size(rect.min, vec2(rect.width(), FLYOUT_ROW));
        ui.painter().text(
            pos2(head.left() + space::L, head.center().y),
            egui::Align2::LEFT_CENTER,
            g.group.name(),
            bar::font(ui, ecad_ui::tokens::ui_type::SMALL),
            ecad_ui::color(s.text_2),
        );
        if let Some(k) = g.letter() {
            keycap_at(ui, pos2(head.right() - space::M - space::XS, head.center().y), &k.to_string());
        }
        let mut y = head.bottom();
        for (i, a) in tools.iter().enumerate() {
            let r = Rect::from_min_size(pos2(rect.left(), y), vec2(rect.width(), FLYOUT_ROW));
            y += FLYOUT_ROW;
            let greyed = match (a.enabled)(&cx) {
                actions::Enabled::Greyed { why, .. } => Some(why),
                actions::Enabled::Ok => None,
            };
            if flyout_row(ui, r, a, i == last && greyed.is_none(), greyed.as_deref()).clicked() {
                chosen = Some(Chosen::Tool(i));
            }
        }
        // The foot: a note, and a link beside it.
        if let Some(n) = note {
            let top = y + space::S;
            ui.painter().hline(rect.x_range(), top.round() + 0.5, egui::Stroke::new(line::HAIRLINE_PX, ecad_ui::color(s.line)));
            let link_w = link.map_or(0.0, |a| bar::text_width(ui, a.label, body) + 2.0 * space::L + space::M);
            let text_rect =
                Rect::from_min_max(pos2(rect.left() + space::L, top + space::M), pos2(rect.right() - space::L - link_w, rect.bottom() - space::M));
            let galley =
                ui.painter().layout(n.to_string(), bar::font(ui, ecad_ui::tokens::ui_type::SMALL), ecad_ui::color(s.text_2), text_rect.width());
            ui.painter().galley(pos2(text_rect.left(), text_rect.center().y - galley.size().y / 2.0), galley, ecad_ui::color(s.text_2));
            if let Some(a) = link {
                let r = Rect::from_min_size(
                    pos2(rect.right() - space::L - link_w + space::M, text_rect.center().y - 13.0),
                    vec2(link_w - space::M, 26.0),
                );
                let ok = ui.scope_builder(egui::UiBuilder::new().max_rect(r), |ui| ui.add(ecad_ui::prim::Button::new(a.label).small())).inner;
                if ok.clicked() {
                    chosen = Some(Chosen::Link);
                }
            }
        }
        chosen
    });
    let over = ui
        .ctx()
        .pointer_hover_pos()
        .is_some_and(|p| rect.expand(space::S).contains(p) || button.expand(space::S).contains(p) || gap(button, rect).contains(p));
    (chosen, over)
}

/// The space between a button and its popover, so the pointer can cross it.
fn gap(button: Rect, pop: Rect) -> Rect {
    Rect::from_min_max(pos2(button.right(), pop.top().min(button.top())), pos2(pop.left() + 1.0, pop.bottom().max(button.bottom())))
}
