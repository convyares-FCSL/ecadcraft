//! The conflict list (brief 052d, design 052 §5.5, §9.2): after a placement, a move or a box
//! proposed a function or location that a device already has another value for, the devices are
//! listed under the intent line, one row per device and aspect, with **Move all**, **Leave all**
//! and, per row, **Move** (Choose) and, for a connector half just placed, **Half**. The listed
//! devices' symbols on the shown page are ringed in the attach colour; the row under the pointer
//! rings its device in the warning colour. Nothing is written until a button is clicked; the default is
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
/// A connector half's own location (design 052 §5.3).
pub const HALF: &str = "Half";
/// Most rows shown; the rest are counted.
pub const MAX_ROWS: usize = 8;

/// One row: device id, tag, aspect word, from, to, and whether Half answers it.
type Row = (ecad_model::ComponentId, String, &'static str, String, String, bool);

/// The rings' radius on the page, in millimetres.
const RING_MM: f32 = 4.0;

impl Trial {
    /// Draw the conflict list under the intent line while the project has conflicts waiting.
    pub(crate) fn conflicts_frame(&mut self, ui: &mut egui::Ui) {
        let Some(page) = self.shell.page_rect else { return };
        // The rows, and the listed devices' symbols on the shown page (their anchors) for the
        // rings (design 052 §9.2).
        let (rows, anchors): (Vec<Row>, Vec<(ecad_model::ComponentId, ecad_model::Pt)>) = self
            .project
            .clone()
            .and_then(|p| {
                p.read(&mut self.cad.session, |p, _| {
                    let doc = p.ledger.document();
                    let rows: Vec<Row> = p
                        .conflicts
                        .iter()
                        .map(|c| {
                            (
                                c.component,
                                ecad_cadcraft::devices::tag_words(doc, c.component),
                                c.aspect.word(),
                                c.from.clone(),
                                c.to.clone(),
                                c.half.is_some(),
                            )
                        })
                        .collect();
                    let shown = p.active_page();
                    let anchors = rows
                        .iter()
                        .flat_map(|r| doc.representations_of(ecad_model::Subject::Component(r.0)).map(move |(_, x)| (r.0, x)))
                        .filter(|(_, x)| Some(x.page) == shown)
                        .filter_map(|(c, x)| ecad_cadcraft::locations::anchor(doc, x).map(|a| (c, a)))
                        .collect();
                    (rows, anchors)
                })
                .ok()
            })
            .unwrap_or_default();
        if rows.is_empty() {
            return;
        }
        let mut ask: Option<String> = None;
        let mut chosen: Option<ecad_model::ComponentId> = None;
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
                            for (c, tag, aspect, from, to, half) in rows.iter().take(MAX_ROWS) {
                                let row = ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new(tag).monospace().color(ecad_ui::color(s.text)));
                                    ui.label(egui::RichText::new(from).monospace().color(ecad_ui::color(s.warning)));
                                    ui.label(egui::RichText::new(format!("\u{2192} {to}")).monospace().color(ecad_ui::color(s.text)));
                                    let r = ui.add(prim::Button::new("Move").small()).on_hover_text(format!("Set {tag}'s {aspect} to {to}"));
                                    if r.clicked() {
                                        ask = Some(format!("conflicts {}", json!({"answer": "move", "components": [c.hex()]})));
                                    }
                                    // A connector half just placed: give that half its own location.
                                    if *half {
                                        let r = ui.add(prim::Button::new(HALF).small()).on_hover_text(format!("Set only this half of {tag} to {to}"));
                                        if r.clicked() {
                                            ask = Some(format!("conflicts {}", json!({"answer": "half", "components": [c.hex()]})));
                                        }
                                    }
                                });
                                // Choose: the row under the pointer rings its device in the
                                // warning colour (it will be left unless moved).
                                if row.response.hovered() || row.response.contains_pointer() {
                                    chosen = Some(*c);
                                }
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
        // The rings: every listed device's symbols on the shown page in the attach colour
        // (`ecad.attach-preview`); the device of the row under the pointer in the warning colour.
        if let Some(xf) = self.cad.canvas.xf.as_ref() {
            let painter = ui.painter_at(page);
            let r = (RING_MM * xf.scale as f32).max(6.0);
            for (c, a) in &anchors {
                let colour = if Some(*c) == chosen { ecad_ui::tokens::sheet::PAPER.warning } else { ecad_ui::tokens::sheet::PAPER.wire_end_attached };
                let at = xf.to_screen(cadcraft_engine::doc::geom::Vec2::new(a.x, a.y));
                painter.circle_stroke(at, r, egui::Stroke::new(2.0, ecad_ui::color(colour)));
            }
        }
        if let Some(cmd) = ask {
            let _ = self.run_line(&cmd);
        }
    }
}
