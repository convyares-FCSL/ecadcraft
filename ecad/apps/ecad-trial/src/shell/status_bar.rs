//! The status bar (spec §2: 26 px): save state, the page shown, the standard, the zone under the
//! cursor, zoom, the agent bridge's state, and the Commands button (the Text panel).

use ecad_ui::tokens::space;
use eframe::egui::{self, Rect};

use crate::Trial;

/// Accessible labels.
pub const COMMANDS: &str = "Commands";
pub const SAVE_STATE: &str = "Save state";

/// Draw the status bar in `rect`.
pub fn show(t: &mut Trial, ui: &mut egui::Ui, rect: Rect) {
    let s = ecad_ui::SHELL;
    super::fill(ui, rect, s.panel);
    ui.painter().hline(rect.x_range(), rect.top() + 0.5, egui::Stroke::new(ecad_ui::tokens::line::HAIRLINE_PX, ecad_ui::color(s.line)));
    let save = t.save_state();
    let page = t.panel.active.and_then(|a| t.panel.rows().iter().find(|r| r.id == a)).map(|r| format!("Shown: {} {}", r.number, r.title));
    let zone = t.zone_under_cursor(ui.ctx());
    let zoom = t.cad.canvas.xf.map(|xf| xf.scale);
    let mcp = t.agent.status_text();
    let mut commands = false;
    super::in_rect(ui, rect.shrink2(egui::vec2(space::M, 0.0)), "ecad.shell.status", |ui| {
        ui.horizontal_centered(|ui| {
            let r = ecad_ui::prim::read_value(ui, &save);
            r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, SAVE_STATE));
            if let Some(p) = &page {
                ecad_ui::prim::read_value(ui, p);
            }
            // The built-in defaults registry (031); named standards come with U4c.
            ecad_ui::prim::read_value(ui, "Standard: built-in");
            if let Some(z) = &zone {
                ecad_ui::prim::read_value(ui, &format!("Zone {z}"));
            }
            if let Some(scale) = zoom {
                // Pixels per millimetre against a 96 dpi screen's real size.
                ecad_ui::prim::read_value(ui, &format!("Zoom {:.0} %", scale * 25.4 / 96.0 * 100.0));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                commands = ui.add(ecad_ui::prim::Button::new(COMMANDS).kind(ecad_ui::prim::Kind::Quiet).small()).clicked();
                ecad_ui::prim::read_value(ui, &format!("MCP: {mcp}"));
            });
        });
    });
    if commands {
        t.toggle_text_panel();
    }
}

impl Trial {
    /// "Saved", "Unsaved changes", "Read-only", or "No project".
    pub fn save_state(&mut self) -> String {
        match self.status() {
            None => "No project".into(),
            Some(s) if s.file.read_only.is_some() => "Read-only".into(),
            Some(s) if s.unsaved > 0 => format!("Unsaved changes ({})", s.unsaved),
            Some(s) if s.file.path.is_none() => "Not saved yet".into(),
            Some(_) => "Saved".into(),
        }
    }

    /// The sheet zone (column and row labels) under the pointer on the page shown, if any.
    pub fn zone_under_cursor(&mut self, ctx: &egui::Context) -> Option<String> {
        let at = ctx.pointer_hover_pos().filter(|at| self.over_shown_page(ctx, *at))?;
        let xf = self.cad.canvas.xf?;
        let w = xf.to_world(at);
        let p = self.project.clone()?;
        let page = self.panel.active?;
        p.read(&mut self.cad.session, |p, _| {
            let grid = ecad_model::sheet::Grid::of(p.ledger.document(), page);
            let (c, r) = (grid.column_of(w.x)?, grid.row_of(w.y)?);
            Some(format!("{}-{}", grid.column_label(c), grid.row_label(r)))
        })
        .ok()
        .flatten()
    }

    /// Open or close the Text panel (Ctrl+/, the Commands button), its field taking the keyboard.
    pub fn toggle_text_panel(&mut self) {
        self.shell.state.text_panel = !self.shell.state.text_panel;
        self.developer.open = self.shell.state.text_panel;
        if self.shell.state.text_panel {
            self.focus_command = true;
        }
    }
}
