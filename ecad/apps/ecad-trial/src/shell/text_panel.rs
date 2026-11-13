//! The Text panel (spec §5): over the right panel, closed by default, opened by Ctrl+/ or the
//! status bar's Commands button. Its **Commands** tab is today's Developer section
//! ([`crate::developer`]): the ECAD command field with the exact command names and JSON MCP uses,
//! the debugger, Inspect Project, Agent access and the primitives page. The Ask tab comes later.
//!
//! One shell command is answered here: `ui.cadchrome` (with `on` or `off`, or alone to flip)
//! turns the developer switch.

use ecad_ui::tokens::{Icon, size, space};
use eframe::egui::{self, Rect, pos2, vec2};

use crate::Trial;
use crate::developer;

/// Accessible labels.
pub const COMMANDS_TAB: &str = "Commands";
pub const CLOSE: &str = "Close the Text panel";

/// Draw the Text panel in `rect`.
pub fn show(t: &mut Trial, ui: &mut egui::Ui, rect: Rect) {
    let head = Rect::from_min_size(rect.min, vec2(rect.width(), size::PANEL_HEADER));
    let close = super::in_rect(ui, head.shrink2(vec2(space::L, space::XS)), "ecad.shell.text.head", |ui| {
        ui.horizontal_centered(|ui| {
            let r = ecad_ui::prim::Chip::new(COMMANDS_TAB).selected(true).show(ui).response;
            r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, true, COMMANDS_TAB));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(ecad_ui::prim::IconButton::new(Icon::Close, CLOSE)).clicked()
            })
            .inner
        })
        .inner
    });
    if close {
        t.shell.state.text_panel = false;
        t.developer.open = false;
        return;
    }
    let body = Rect::from_min_max(pos2(rect.left() + space::M, head.bottom()), pos2(rect.right() - space::M, rect.bottom()));
    let running = t.cad.session.running.is_some();
    let project = t.project_open();
    let snapshots = t.snapshots.is_some();
    // The Developer section is the Commands tab: folding it closes the panel (Agent access
    // stays here until U4d moves it to Settings).
    let actions = super::in_rect(ui, body, "ecad.shell.text.body", |ui| {
        developer::show(
            ui,
            developer::Parts {
                dev: &mut t.developer,
                command: &mut t.command,
                hint: t.command_hint,
                focus: &mut t.focus_command,
                debugger: &mut t.debugger,
                running,
                snapshots,
                project,
                agent: developer::AgentRow {
                    mode: t.agent.pref.mode,
                    status: t.agent.status_text(),
                    warning: t.agent.author_pending || matches!(t.agent.listen(), crate::agent::Listen::InUse(_)),
                    locked: t.agent.this_run,
                    port_warning: t.agent.port_warning,
                    restart_note: t.agent.restart_note,
                    // The banner has its own slot under the top bar.
                    banner: None,
                    busy: running,
                    port: &mut t.agent.port_text,
                },
            },
        )
    });
    t.shell.state.text_panel = t.developer.open;
    if t.command.trim().is_empty() {
        t.command_hint = None;
    }
    for a in actions {
        match a {
            developer::Action::Run(line) if is_cad_chrome(&line) => t.cad_chrome_command(&line),
            developer::Action::Run(line) if crate::canvas::is_sheet_command(&line) => t.sheet_command(&line),
            a => t.developer_action(a),
        }
    }
}

/// Whether `line` is the `ui.cadchrome` command.
pub fn is_cad_chrome(line: &str) -> bool {
    line.split_whitespace().next().is_some_and(|w| w.eq_ignore_ascii_case(super::CAD_CHROME_COMMAND))
}

impl Trial {
    /// `ui.cadchrome [on|off]`: the developer switch; the reply says what it is now.
    pub fn cad_chrome_command(&mut self, line: &str) {
        let arg = line.split_whitespace().nth(1).map(str::to_ascii_lowercase);
        let on = match arg.as_deref() {
            Some("on" | "1" | "true") => true,
            Some("off" | "0" | "false") => false,
            None => !self.shell.state.cad_chrome,
            Some(other) => {
                self.developer.reply = vec![format!("ui.cadchrome takes on or off, not {other:?}.")];
                return;
            }
        };
        self.set_cad_chrome(on);
        self.developer.reply = vec![format!("CADCraft's interface is {}.", if on { "shown in the main view" } else { "hidden" })];
    }
}
