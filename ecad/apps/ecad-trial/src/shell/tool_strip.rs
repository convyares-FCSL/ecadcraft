//! The tool strip (spec §2, "Tool groups"): seven group buttons in Draw. A button shows its
//! group's last-used tool and a click starts it; hovering it opens the flyout listing the group's
//! tools; the group's key starts the last-used tool, and pressed again steps to the next.
//!
//! Brief 041: the groups and their tools come from the action table
//! ([`ecad_cadcraft::actions`]): a tool is an action with a group, and a tool whose command does
//! not exist yet is not listed. Tools only start new things; what acts on an existing thing is on
//! that thing's wheel ([`super::wheel`]).
//!
//! [`ToolStrip`] also keeps lane U4b's other state between frames: the open wheel and the intent
//! line's cache and refusal.

use ecad_cadcraft::actions::{Action, ToolGroup};
use ecad_ui::tokens::{Icon, size, space};
use eframe::egui::{self, Key, Rect, pos2, vec2};

use crate::Trial;

/// A tool group as the strip shows it.
#[derive(Debug, Clone, Copy)]
pub struct Group {
    pub group: ToolGroup,
    pub letter: char,
    pub key: Key,
    pub icon: Icon,
    /// Its accessible label, with the key: `Wire (W)`.
    pub label: &'static str,
}

impl Group {
    /// Its tools, in flyout order (the action table's).
    pub fn tools(&self) -> Vec<&'static Action> {
        self.group.tools()
    }
}

const fn group(group: ToolGroup, key: Key, label: &'static str) -> Group {
    Group { group, letter: group.letter(), key, icon: group.icon(), label }
}

/// The seven groups, in strip order (spec §2).
pub const GROUPS: [Group; 7] = [
    group(ToolGroup::Select, Key::V, "Select (V)"),
    group(ToolGroup::Wire, Key::W, "Wire (W)"),
    group(ToolGroup::Place, Key::P, "Place (P)"),
    group(ToolGroup::Cable, Key::C, "Cable (C)"),
    group(ToolGroup::Draw, Key::D, "Draw (D)"),
    group(ToolGroup::Box, Key::B, "Box (B)"),
    group(ToolGroup::Text, Key::T, "Text (T)"),
];

/// The flyout's accessible label for group `g`.
pub fn flyout_label(g: &Group) -> String {
    format!("{} tools", g.label)
}

/// The strip's state: each group's last-used tool, the open flyout, and the group whose tool
/// the last group key started (pressing it again steps); and lane U4b's wheel and intent line.
#[derive(Debug, Clone, Default)]
pub struct ToolStrip {
    pub last: [usize; 7],
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
        let last = self.shell.tools.last.get(gi).copied().unwrap_or(0);
        let tools = GROUPS.get(gi).map(Group::tools).unwrap_or_default();
        let command = tools.get(last).map(|a| a.command).filter(|c| !c.is_empty());
        command.is_some_and(|c| self.cad.session.running.as_ref().is_some_and(|r| r.id.eq_ignore_ascii_case(c)))
    }

    /// A group key (V W P C D B T): the group's last-used tool, or the next one when the key was
    /// the last pressed and its tool still runs.
    pub fn group_key(&mut self, letter: char) {
        let Some(gi) = GROUPS.iter().position(|g| g.letter == letter) else { return };
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
}

/// Draw the strip in `rect`.
pub fn show(t: &mut Trial, ui: &mut egui::Ui, rect: Rect) {
    let s = ecad_ui::SHELL;
    super::fill(ui, rect, s.panel);
    let mut start = None;
    let mut y = rect.top() + space::S;
    let x = rect.center().x - size::TOOL_BUTTON / 2.0;
    let mut hovered_any = false;
    for (gi, g) in GROUPS.iter().enumerate() {
        let tools = g.tools();
        let r = Rect::from_min_size(pos2(x, y), vec2(size::TOOL_BUTTON, size::TOOL_BUTTON));
        y += size::TOOL_BUTTON + space::XS;
        let last = t.shell.tools.last.get(gi).copied().unwrap_or(0);
        let current = tools.get(last).or(tools.first()).copied();
        let tip = current.map_or(g.label.to_string(), |a| format!("{}: {}", g.label, a.label));
        // The button shows the last-used tool's icon (spec §2).
        let icon = current.map_or(g.icon, |a| a.icon);
        let running = t.group_tool_running(gi);
        let resp = super::in_rect(ui, r, g.label, |ui| {
            ui.add(ecad_ui::prim::IconButton::new(icon, g.label).tool().selected(t.shell.tools.flyout == Some(gi) || running))
        });
        let resp = resp.on_hover_text(tip);
        if resp.clicked() {
            start = current.map(|a| (gi, last, a));
            t.shell.tools.flyout = None;
        }
        if resp.hovered() {
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
    }
    if !hovered_any {
        t.shell.tools.flyout = None;
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
    let area = egui::Area::new(egui::Id::unique(("ecad.shell.flyout", g.letter)))
        .order(egui::Order::Foreground)
        .fixed_pos(pos2(button.right(), button.top()))
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
