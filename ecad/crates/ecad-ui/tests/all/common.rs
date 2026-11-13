//! Shared helpers.

use egui_kittest::Harness;

/// A harness drawing `app` each frame over `state`, with ECAD's fonts installed in the first
/// frame (as the trial installs them on top of CADCraft's set) and ECAD's style applied.
pub fn harness<'a, S>(mut app: impl FnMut(&mut egui::Ui, &mut S) + 'a, state: S) -> Harness<'a, S> {
    let mut first = true;
    let mut h = Harness::builder().with_size(egui::vec2(900.0, 700.0)).build_ui_state(
        move |ui, s| {
            if std::mem::take(&mut first) {
                ecad_ui::fonts::install(ui.ctx(), egui::FontDefinitions::default());
            }
            ecad_ui::style::apply(ui.ctx());
            app(ui, s);
        },
        state,
    );
    h.run();
    h
}

/// Every stroke width in the last frame's shapes that is drawn in `color`.
pub fn strokes_in(h: &Harness<'_, impl Sized>, color: egui::Color32) -> Vec<f32> {
    let mut out = Vec::new();
    for clipped in &h.output().shapes {
        collect(&clipped.shape, color, &mut out);
    }
    out
}

fn collect(shape: &egui::Shape, color: egui::Color32, out: &mut Vec<f32>) {
    use egui::Shape;
    match shape {
        Shape::Vec(v) => v.iter().for_each(|s| collect(s, color, out)),
        Shape::Rect(r) if r.stroke.color == color && r.stroke.width > 0.0 => out.push(r.stroke.width),
        Shape::LineSegment { stroke, .. } if stroke.color == color => out.push(stroke.width),
        _ => {}
    }
}
