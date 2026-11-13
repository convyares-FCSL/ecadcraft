//! The companion leader line (brief 040, step 5; spec §5 "How live is shown", 038 open point 8):
//! in a companion view, a selection made on one side draws a line for one second between the
//! companion and the object on the page, so the eye finds it. It repaints only while live.

use eframe::egui::{self, Pos2, Stroke};

/// How long the line shows, seconds.
pub const SECONDS: f64 = 1.0;

/// A leader line from the companion side to the object.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Leader {
    pub from: Pos2,
    pub to: Pos2,
    /// egui time it started, seconds.
    pub since: f64,
}

/// Paint `leader` while it is live, fading out; drop it once its second is over. Whether it is
/// still live (repaint next frame).
pub fn paint(p: &egui::Painter, sheet: &ecad_ui::tokens::Sheet, leader: &mut Option<Leader>, now: f64) -> bool {
    let Some(l) = *leader else { return false };
    let age = now - l.since;
    if !(0.0..SECONDS).contains(&age) {
        *leader = None;
        return false;
    }
    let fade = (1.0 - age / SECONDS).clamp(0.0, 1.0);
    let colour = sheet.selection.at_percent((fade * 100.0).round().clamp(0.0, 100.0) as u8);
    let stroke = Stroke::new(1.5, ecad_ui::color(colour));
    p.line_segment([l.from, l.to], stroke);
    p.circle_stroke(l.to, 6.0, stroke);
    true
}
