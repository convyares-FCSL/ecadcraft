//! The popouts of the Snap, Grid and Ortho toggles at the foot of the tool strip (UI rev 3, "Tool
//! strip (Draw)"). Hovering a toggle opens its popout beside it; a click on the toggle still flips
//! the mode. Everything here runs CADCraft's own drafting commands (`snap`, `grid`, `ortho`,
//! `polar`, `dsettings`); an option with no setting behind it yet is shown greyed in place.
//!
//! - Snap: spacing; snapping to connection points (greyed: ECAD always snaps to them).
//! - Grid: shown or hidden, spacing, style (greyed: one style today), major lines.
//! - Ortho: 90° only (ortho), 45° steps (polar tracking at 45°), or free.

use cadcraft_engine::Settings;
use ecad_ui::tokens::{space, ui_type};
use eframe::egui::{self, Rect, pos2, vec2};
use serde_json::{Value, json};

use super::bar;

/// Accessible labels of the popouts.
pub const SNAP: &str = "Snap settings";
pub const GRID: &str = "Grid settings";
pub const ORTHO: &str = "Ortho settings";

/// Spacings offered (mm), and major-line counts.
pub const SNAP_STEPS: [f64; 4] = [0.5, 1.0, 2.5, 5.0];
pub const GRID_STEPS: [f64; 4] = [2.5, 5.0, 10.0, 20.0];
pub const MAJOR_STEPS: [u32; 3] = [4, 5, 10];

/// Why the greyed options are greyed.
pub const ALWAYS_POINTS: &str = "Always on: ECAD's tools snap to connection points.";
pub const ONE_STYLE: &str = "One grid style for now: dots.";

/// Ortho's three ways.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ortho {
    Right,
    Steps45,
    Free,
}

/// Ortho as the settings have it.
pub fn ortho(s: &Settings) -> Ortho {
    if s.orthomode {
        Ortho::Right
    } else if s.polarmode && (s.polarang.to_degrees() - 45.0).abs() < 1e-6 {
        Ortho::Steps45
    } else {
        Ortho::Free
    }
}

/// Whether toggle `command` (`snap`, `grid`, `ortho`) is on.
pub fn is_on(s: &Settings, command: &str) -> bool {
    match command {
        "snap" => s.snapmode,
        "grid" => s.gridmode,
        _ => ortho(s) != Ortho::Free,
    }
}

/// The commands for Ortho set to `o`.
pub fn set_ortho(o: Ortho) -> Vec<(&'static str, Value)> {
    match o {
        Ortho::Right => vec![("polar", json!({"on": false})), ("ortho", json!({"on": true}))],
        Ortho::Steps45 => vec![("ortho", json!({"on": false})), ("dsettings", json!({"polarang": 45.0})), ("polar", json!({"on": true}))],
        Ortho::Free => vec![("ortho", json!({"on": false})), ("polar", json!({"on": false}))],
    }
}

/// The commands a click on toggle `command` runs: Snap and Grid flip; Ortho goes to 90° from
/// free, and back to free from either constrained way.
pub fn click(s: &Settings, command: &str) -> Vec<(&'static str, Value)> {
    match command {
        "snap" | "grid" => vec![(if command == "snap" { "snap" } else { "grid" }, json!({}))],
        _ if ortho(s) == Ortho::Free => set_ortho(Ortho::Right),
        _ => set_ortho(Ortho::Free),
    }
}

const ROW: f32 = 30.0;
const LABEL_W: f32 = 88.0;
const WIDTH: f32 = 300.0;

/// Draw toggle `command`'s popout beside `button`. Returns the commands its controls ran this
/// frame, and whether the pointer is over it or its button.
pub fn popout(ui: &mut egui::Ui, command: &str, s: &Settings, button: Rect) -> (Vec<(&'static str, Value)>, bool) {
    let (title, rows) = match command {
        "snap" => (SNAP, 3),
        "grid" => (GRID, 5),
        _ => (ORTHO, 1),
    };
    let height = (rows as f32) * ROW + space::M * 2.0;
    let (out, rect) = super::tool_strip::popover(ui, title, button, vec2(WIDTH, height), |ui, rect| {
        let mut out = Vec::new();
        let inner = rect.shrink2(vec2(space::L, space::M));
        let mut y = inner.top();
        let mut row = |ui: &mut egui::Ui, label: &str, add: &mut dyn FnMut(&mut egui::Ui)| {
            let r = Rect::from_min_size(pos2(inner.left(), y), vec2(inner.width(), ROW));
            y += ROW;
            ui.painter().text(
                pos2(r.left(), r.center().y),
                egui::Align2::LEFT_CENTER,
                label,
                bar::font(ui, ui_type::SMALL),
                ecad_ui::color(ecad_ui::SHELL.text_2),
            );
            let field = Rect::from_min_max(pos2(r.left() + LABEL_W, r.top() + 2.0), pos2(r.right(), r.bottom() - 2.0));
            ui.scope_builder(egui::UiBuilder::new().max_rect(field).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
                ui.spacing_mut().item_spacing.x = space::XS;
                add(ui);
            });
        };
        let head = |ui: &mut egui::Ui, text: &str, on: bool, out: &mut Vec<(&'static str, Value)>, cmd: &'static str| {
            if ecad_ui::prim::Chip::new(if on { "On" } else { "Off" }).selected(on).show(ui).response.clicked() {
                out.push((cmd, json!({"on": !on})));
            }
            ecad_ui::prim::read_value(ui, text);
        };
        match command {
            "snap" => {
                row(ui, "Snap", &mut |ui| head(ui, "to the spacing below", s.snapmode, &mut out, "snap"));
                row(ui, "Spacing", &mut |ui| {
                    for v in SNAP_STEPS {
                        let on = (s.snapunit.x - v).abs() < 1e-9;
                        if ecad_ui::prim::Chip::new(&mm(v)).selected(on).show(ui).response.clicked() {
                            out.push(("dsettings", json!({"snapunit": [v, v]})));
                        }
                    }
                });
                row(ui, "Points", &mut |ui| {
                    ui.add(ecad_ui::prim::Button::new("Connection points").small().disabled(ALWAYS_POINTS));
                });
            }
            "grid" => {
                row(ui, "Grid", &mut |ui| head(ui, "shown, apart from snap", s.gridmode, &mut out, "grid"));
                let unit = if s.gridunit.x > 0.0 { s.gridunit.x } else { s.snapunit.x };
                row(ui, "Spacing", &mut |ui| {
                    for v in GRID_STEPS {
                        let on = (unit - v).abs() < 1e-9;
                        if ecad_ui::prim::Chip::new(&mm(v)).selected(on).show(ui).response.clicked() {
                            out.push(("dsettings", json!({"gridunit": [v, v]})));
                        }
                    }
                });
                row(ui, "Style", &mut |ui| {
                    ecad_ui::prim::Chip::new("Dots").selected(true).show(ui);
                    ui.add(ecad_ui::prim::Button::new("Lines").small().disabled(ONE_STYLE));
                });
                row(ui, "Major lines", &mut |ui| {
                    for m in MAJOR_STEPS {
                        if ecad_ui::prim::Chip::new(&format!("every {m}")).selected(s.gridmajor == m).show(ui).response.clicked() {
                            out.push(("dsettings", json!({"gridmajor": m})));
                        }
                    }
                });
            }
            _ => {
                let now = ortho(s);
                row(ui, "Angles", &mut |ui| {
                    for (o, text) in [(Ortho::Right, "90° only"), (Ortho::Steps45, "45° steps"), (Ortho::Free, "Free")] {
                        if ecad_ui::prim::Chip::new(text).selected(now == o).show(ui).response.clicked() && now != o {
                            out.extend(set_ortho(o));
                        }
                    }
                });
            }
        }
        out
    });
    let over = ui.ctx().pointer_hover_pos().is_some_and(|p| {
        rect.expand(space::S).contains(p)
            || button.expand(space::S).contains(p)
            || Rect::from_min_max(pos2(button.right(), button.top()), pos2(rect.left() + 1.0, button.bottom())).contains(p)
    });
    (out, over)
}

/// A spacing in mm, without trailing zeros.
fn mm(v: f64) -> String {
    let t = format!("{v}");
    format!("{t} mm")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ortho_reads_and_sets_its_three_ways() {
        let mut s = Settings::default();
        assert_eq!(ortho(&s), Ortho::Free);
        s.orthomode = true;
        assert_eq!(ortho(&s), Ortho::Right);
        s.orthomode = false;
        s.polarmode = true;
        s.polarang = 45f64.to_radians();
        assert_eq!(ortho(&s), Ortho::Steps45);
        assert!(is_on(&s, "ortho"));
        let cmds: Vec<&str> = click(&s, "ortho").iter().map(|c| c.0).collect();
        assert_eq!(cmds, ["ortho", "polar"], "a click from 45° frees the angle");
    }

    #[test]
    fn spacings_read_as_millimetres() {
        assert_eq!(mm(2.5), "2.5 mm");
        assert_eq!(mm(5.0), "5 mm");
    }
}
