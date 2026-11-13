//! The tool strip (spec §2, "Tool groups"): seven group buttons in Draw. A button shows its
//! group's last-used tool and a click starts it; hovering it opens the flyout listing the group's
//! tools; the group's key starts the last-used tool, and pressed again steps to the next.
//!
//! In U2 the flyouts list today's tools that fit each group: the ECAD Tool Sets tab's buttons
//! ([`crate::tools::TOOLS`]) and, for Draw and Text, CADCraft's own drawing commands. Select's
//! flyout also holds the tools that act on existing things until U4b moves them to the wheel.
//! Every ECAD Tool Sets button is in exactly one flyout (a test checks it).

use ecad_ui::tokens::{Icon, size, space};
use eframe::egui::{self, Key, Rect, pos2, vec2};

use crate::Trial;

/// One flyout entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entry {
    /// Back to selecting: ends the running command.
    Select,
    /// An ECAD Tool Sets button, by id ([`crate::tools::TOOLS`]).
    Ecad(&'static str),
    /// A CADCraft command started by name, with its label.
    Cad(&'static str, &'static str),
}

impl Entry {
    /// Its label in the flyout.
    pub fn label(self) -> &'static str {
        match self {
            Entry::Select => "Select",
            Entry::Ecad(id) => crate::tools::find(id).map_or(id, |t| t.label),
            Entry::Cad(_, label) => label,
        }
    }

    /// Whether the group key steps to it (tools that start at the page; not dialogs or
    /// templates).
    fn steps(self) -> bool {
        match self {
            Entry::Select | Entry::Cad(..) => true,
            Entry::Ecad(id) => crate::tools::find(id).is_some_and(|t| matches!(t.kind, crate::tools::Kind::Start(_))),
        }
    }
}

/// A tool group.
#[derive(Debug, Clone, Copy)]
pub struct Group {
    pub letter: char,
    pub key: Key,
    pub icon: Icon,
    /// Its accessible label, with the key: `Wire (W)`.
    pub label: &'static str,
    pub entries: &'static [Entry],
}

use Entry::{Cad, Ecad};

/// The seven groups, in strip order (spec §2).
pub const GROUPS: [Group; 7] = [
    Group {
        letter: 'V',
        key: Key::V,
        icon: Icon::Select,
        label: "Select (V)",
        entries: &[
            Entry::Select,
            Ecad("ecad.deviceprops"),
            Ecad("ecad.setprop"),
            Ecad("ecad.devicedelete"),
            Ecad("ecad.delete"),
            Ecad("ecad.renumber"),
            Ecad("ecad.finalise"),
        ],
    },
    Group {
        letter: 'W',
        key: Key::W,
        icon: Icon::Wire,
        label: "Wire (W)",
        entries: &[
            Ecad("ecad.wire"),
            Ecad("ecad.junction"),
            Ecad("ecad.joinwires"),
            Ecad("ecad.connect"),
            Ecad("ecad.disconnect"),
            Ecad("ecad.tool.jumper"),
            Ecad("ecad.wirenumber"),
            Ecad("ecad.renumberwires"),
        ],
    },
    Group {
        letter: 'P',
        key: Key::P,
        icon: Icon::Place,
        label: "Place (P)",
        entries: &[
            Ecad("ecad.place"),
            Ecad("ecad.findsymbol"),
            Ecad("ecad.terminals"),
            Ecad("ecad.bridge"),
            Ecad("ecad.tool.accessory"),
            Ecad("ecad.addterminal"),
            Ecad("ecad.convertcomponent"),
        ],
    },
    Group {
        letter: 'C',
        key: Key::C,
        icon: Icon::Cable,
        label: "Cable (C)",
        entries: &[
            Ecad("ecad.cableslash"),
            Ecad("ecad.cablecontinue"),
            Ecad("ecad.cableadd"),
            Ecad("ecad.cablefan"),
            Ecad("ecad.cableref"),
            Ecad("ecad.cablecore"),
            Ecad("ecad.cableview"),
            Ecad("ecad.cablegoto"),
            Ecad("ecad.cablepreview"),
        ],
    },
    Group {
        letter: 'D',
        key: Key::D,
        icon: Icon::Draw,
        label: "Draw (D)",
        entries: &[Cad("line", "Line"), Cad("pline", "Polyline"), Cad("rectang", "Rectangle"), Cad("circle", "Circle"), Cad("arc", "Arc")],
    },
    Group { letter: 'B', key: Key::B, icon: Icon::Box, label: "Box (B)", entries: &[Ecad("ecad.tool.locationbox"), Ecad("ecad.mirrormove")] },
    Group {
        letter: 'T',
        key: Key::T,
        icon: Icon::Text,
        label: "Text (T)",
        entries: &[Cad("text", "Text"), Ecad("ecad.tool.showtext"), Ecad("ecad.tool.movetext"), Ecad("ecad.view.hiddentext")],
    },
];

/// The flyout's accessible label for group `g`.
pub fn flyout_label(g: &Group) -> String {
    format!("{} tools", g.label)
}

/// The strip's state: each group's last-used entry, the open flyout, and the group whose tool
/// the last group key started (pressing it again steps).
#[derive(Debug, Clone, Default)]
pub struct ToolStrip {
    pub last: [usize; 7],
    pub flyout: Option<usize>,
    stepping: Option<usize>,
}

impl Trial {
    /// Start a flyout entry.
    pub fn start_entry(&mut self, e: Entry) {
        match e {
            Entry::Select => {
                if self.cad.session.running.is_some() {
                    self.cad.session.cancel();
                }
            }
            Entry::Ecad(id) => {
                if let Err(err) = self.tool(id) {
                    self.cad.session.echo(err);
                }
            }
            Entry::Cad(name, _) => {
                if self.cad.session.running.is_some() {
                    self.cad.session.cancel();
                }
                self.cad.start(name);
            }
        }
    }

    /// Whether the running command is the tool group `gi`'s key last started.
    pub fn group_tool_running(&self, gi: usize) -> bool {
        if self.shell.tools.stepping != Some(gi) {
            return false;
        }
        let last = self.shell.tools.last.get(gi).copied().unwrap_or(0);
        let command = GROUPS.get(gi).and_then(|g| g.entries.get(last)).and_then(|e| match e {
            Entry::Ecad(id) => crate::tools::find(id).and_then(|t| match t.kind {
                crate::tools::Kind::Start(c) => Some(c),
                _ => None,
            }),
            Entry::Cad(name, _) => Some(*name),
            Entry::Select => None,
        });
        command.is_some_and(|c| self.cad.session.running.as_ref().is_some_and(|r| r.id.eq_ignore_ascii_case(c)))
    }

    /// A group key (V W P C D B T): the group's last-used tool, or the next one when the key was
    /// the last pressed and its tool still runs.
    pub fn group_key(&mut self, letter: char) {
        let Some(gi) = GROUPS.iter().position(|g| g.letter == letter) else { return };
        let Some(g) = GROUPS.get(gi) else { return };
        let steps: Vec<usize> = g.entries.iter().enumerate().filter(|(_, e)| e.steps()).map(|(i, _)| i).collect();
        if steps.is_empty() {
            return;
        }
        let last = self.shell.tools.last.get(gi).copied().unwrap_or(0);
        let again = self.shell.tools.stepping == Some(gi) && self.cad.session.running.is_some();
        let at = steps.iter().position(|i| *i == last).unwrap_or(0);
        let next = if again { steps.get((at + 1) % steps.len()) } else { steps.get(at) }.copied().unwrap_or(0);
        if let Some(slot) = self.shell.tools.last.get_mut(gi) {
            *slot = next;
        }
        self.shell.tools.stepping = Some(gi);
        if let Some(e) = g.entries.get(next).copied() {
            self.start_entry(e);
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
        let r = Rect::from_min_size(pos2(x, y), vec2(size::TOOL_BUTTON, size::TOOL_BUTTON));
        y += size::TOOL_BUTTON + space::XS;
        let last = t.shell.tools.last.get(gi).copied().unwrap_or(0);
        let tip = g.entries.get(last).map_or(g.label.to_string(), |e| format!("{}: {}", g.label, e.label()));
        let resp = super::in_rect(ui, r, g.label, |ui| {
            ui.add(ecad_ui::prim::IconButton::new(g.icon, g.label).tool().selected(t.shell.tools.flyout == Some(gi)))
        });
        let resp = resp.on_hover_text(tip);
        if resp.clicked() {
            if let Some(e) = g.entries.get(last).copied() {
                start = Some(e);
            }
            t.shell.tools.flyout = None;
        }
        if resp.hovered() {
            t.shell.tools.flyout = Some(gi);
            hovered_any = true;
        }
        if t.shell.tools.flyout == Some(gi) {
            let (chosen, over) = flyout(ui, g, r);
            hovered_any |= over;
            if let Some(i) = chosen {
                if let Some(slot) = t.shell.tools.last.get_mut(gi) {
                    *slot = i;
                }
                start = g.entries.get(i).copied();
                t.shell.tools.flyout = None;
            }
        }
    }
    if !hovered_any {
        t.shell.tools.flyout = None;
    }
    if let Some(e) = start {
        t.shell.tools.stepping = None;
        t.start_entry(e);
    }
}

/// The flyout of `g` beside its button: the entry clicked, and whether the pointer is over it.
fn flyout(ui: &mut egui::Ui, g: &Group, button: Rect) -> (Option<usize>, bool) {
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
                    for (i, e) in g.entries.iter().enumerate() {
                        let b = ui.add(ecad_ui::prim::Button::new(e.label()).kind(ecad_ui::prim::Kind::Quiet).small());
                        let b = match e {
                            Entry::Ecad(id) => b.on_hover_text(crate::tools::find(id).map_or("", |t| t.tooltip)),
                            _ => b,
                        };
                        if b.clicked() {
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
