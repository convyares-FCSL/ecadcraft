//! The conflict list (brief 052d, design 052 §5.5, §9.2): after a placement, a move or a box
//! proposed a function or location that a device already has another value for, the devices are
//! listed under the intent line, one row per device and aspect, with **Move all**, **Leave all**
//! and, per row, **Move** (Choose). Nothing is written until a button is clicked; the default is
//! Leave, Esc is Leave, and any other edit leaves them. It is non-modal, like every prompt.
//!
//! Update map: each frame a page is shown it copies the project's waiting conflicts
//! (`Project::conflicts`, a short list kept by the proposing commands and cleared by any other
//! edit); it reads nothing else of the project.

use ecad_ui::prim::{self, Tone};
use ecad_ui::tokens::space;
use eframe::egui::{self, Rect};
use serde_json::json;

use crate::Trial;

/// The list (element name).
pub const LIST: &str = "ecad.prompt.location_conflicts";
/// The buttons.
pub const MOVE_ALL: &str = "Move all";
pub const LEAVE_ALL: &str = "Leave all";
/// Most rows shown; the rest are counted.
pub const MAX_ROWS: usize = 8;

/// One row: device id, tag, aspect word, from, to.
type Row = (ecad_model::ComponentId, String, &'static str, String, String);

impl Trial {
    /// Draw the conflict list under the intent line while the project has conflicts waiting.
    pub(crate) fn conflicts_frame(&mut self, ui: &mut egui::Ui) {
        let Some(page) = self.shell.page_rect else { return };
        let rows: Vec<Row> = self
            .project
            .clone()
            .and_then(|p| {
                p.read(&mut self.cad.session, |p, _| {
                    let doc = p.ledger.document();
                    p.conflicts
                        .iter()
                        .map(|c| (c.component, ecad_cadcraft::devices::tag_words(doc, c.component), c.aspect.word(), c.from.clone(), c.to.clone()))
                        .collect()
                })
                .ok()
            })
            .unwrap_or_default();
        if rows.is_empty() {
            return;
        }
        let mut ask: Option<String> = None;
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) && self.cad.session.running.is_none() {
            ask = Some(format!("conflicts {}", json!({"answer": "leave"})));
        }
        let s = ecad_ui::SHELL;
        let devices: std::collections::BTreeSet<_> = rows.iter().map(|r| r.0).collect();
        let n = devices.len();
        let head =
            if n == 1 { "1 device has another location or function.".to_string() } else { format!("{n} devices have another location or function.") };
        let rect = Rect::from_min_size(page.min, egui::vec2(page.width(), 0.0));
        egui::Area::new(egui::Id::unique(LIST))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(rect.left() + space::XXL, rect.top() + space::L + 48.0))
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(ecad_ui::color(s.panel))
                    .stroke(egui::Stroke::new(ecad_ui::tokens::line::HAIRLINE_PX, ecad_ui::color(s.line_strong)))
                    .corner_radius(ecad_ui::style::corner(ecad_ui::tokens::radius::POPOVER))
                    .shadow(ecad_ui::style::shadow())
                    .inner_margin(egui::Margin::symmetric(space::L as i8, space::S as i8))
                    .show(ui, |ui| {
                        ui.scope(|ui| {
                            prim::pill(ui, "Location", Tone::Neutral);
                            prim::read_value(ui, &head);
                            for (c, tag, aspect, from, to) in rows.iter().take(MAX_ROWS) {
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new(tag).monospace().color(ecad_ui::color(s.text)));
                                    ui.label(egui::RichText::new(from).monospace().color(ecad_ui::color(s.warning)));
                                    ui.label(egui::RichText::new(format!("\u{2192} {to}")).monospace().color(ecad_ui::color(s.text)));
                                    let r = ui.add(prim::Button::new("Move").small()).on_hover_text(format!("Set {tag}'s {aspect} to {to}"));
                                    if r.clicked() {
                                        ask = Some(format!("conflicts {}", json!({"answer": "move", "components": [c.hex()]})));
                                    }
                                });
                            }
                            if rows.len() > MAX_ROWS {
                                prim::read_value(ui, &format!("+{} more (Move to set them one by one)", rows.len() - MAX_ROWS));
                            }
                            ui.horizontal(|ui| {
                                if ui.add(prim::Button::new(MOVE_ALL).small()).clicked() {
                                    ask = Some(format!("conflicts {}", json!({"answer": "move"})));
                                }
                                if ui.add(prim::Button::new(LEAVE_ALL).kind(prim::Kind::Quiet).small()).clicked() {
                                    ask = Some(format!("conflicts {}", json!({"answer": "leave"})));
                                }
                                prim::keycap(ui, "Esc");
                                prim::read_value(ui, "Leave (default)");
                            });
                        })
                        .response
                        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Group, true, LIST));
                    });
            });
        if let Some(cmd) = ask {
            let _ = self.run_line(&cmd);
        }
    }
}
