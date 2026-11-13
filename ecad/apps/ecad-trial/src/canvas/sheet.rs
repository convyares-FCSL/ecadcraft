//! The sheet (brief 040, step 1): the area around the sheet frame in the shell's colour with the
//! spec's one shadow, so the page reads as paper on a desk, and the 0.25 mm dot grid every 5 mm
//! inside the frame only. Worked out from the view each frame; nothing is read from the project.

use cadcraft_engine::geom::Vec2;
use cadcraft_ui_egui::canvas::Xf;
use eframe::egui::{self, Rect, pos2};

/// The grid's pitch and dot size on paper, mm (spec §4 "Canvas colours").
pub const GRID_MM: f64 = 5.0;
pub const DOT_MM: f64 = 0.25;
/// Dots closer than this on screen are not drawn (the grid would be a tint), px.
pub const MIN_PITCH_PX: f64 = 10.0;
/// Most dots drawn in one frame; above it the grid is left out.
pub const MAX_DOTS: usize = 20_000;
/// How much of the drawing outside the sheet still shows through the surround, % opaque.
const SURROUND_PERCENT: u8 = 88;

/// A drawing rectangle on screen.
pub fn screen_rect(xf: &Xf, lo: ecad_model::Pt, hi: ecad_model::Pt) -> Rect {
    let a = xf.to_screen(Vec2::new(lo.x, lo.y));
    let b = xf.to_screen(Vec2::new(hi.x, hi.y));
    Rect::from_two_pos(a, b)
}

/// Paint the surround, the shadow and the grid of the sheet `frames` (drawing units) in `rect`.
pub fn paint(p: &egui::Painter, xf: &Xf, sheet: &ecad_ui::tokens::Sheet, frames: &[(ecad_model::Pt, ecad_model::Pt)], rect: Rect) {
    let sheets: Vec<Rect> = frames.iter().map(|(lo, hi)| screen_rect(xf, *lo, *hi)).filter(|r| r.is_positive() && r.is_finite()).collect();
    let Some(outer) = sheets.iter().copied().reduce(|a, b| a.union(b)) else { return };
    surround(p, rect, outer);
    for s in &sheets {
        dots(p, xf, sheet, *s);
    }
}

/// The shell's colour around the sheet, and the shadow beneath it, outside the sheet only.
fn surround(p: &egui::Painter, rect: Rect, sheet: Rect) {
    let colour = ecad_ui::color(ecad_ui::SHELL.bg.at_percent(SURROUND_PERCENT));
    let shadow = egui::epaint::Shadow {
        offset: [ecad_ui::tokens::shadow::OFFSET_X as i8, ecad_ui::tokens::shadow::OFFSET_Y as i8],
        blur: ecad_ui::tokens::shadow::BLUR as u8,
        spread: 0,
        color: ecad_ui::color(ecad_ui::tokens::shadow::COLOR),
    };
    // Four bands around the sheet: above, below, left and right of it.
    let bands = [
        Rect::from_min_max(rect.min, pos2(rect.max.x, sheet.min.y)),
        Rect::from_min_max(pos2(rect.min.x, sheet.max.y), rect.max),
        Rect::from_min_max(pos2(rect.min.x, sheet.min.y), pos2(sheet.min.x, sheet.max.y)),
        Rect::from_min_max(pos2(sheet.max.x, sheet.min.y), pos2(rect.max.x, sheet.max.y)),
    ];
    for band in bands.into_iter().map(|b| b.intersect(rect)).filter(|b| b.is_positive()) {
        let clipped = p.with_clip_rect(band);
        clipped.rect_filled(band, 0.0, colour);
        clipped.add(shadow.as_shape(sheet, 0.0));
    }
}

/// The dot grid inside `sheet` (screen), clipped to what shows.
fn dots(p: &egui::Painter, xf: &Xf, tokens: &ecad_ui::tokens::Sheet, sheet: Rect) {
    let pitch = GRID_MM * xf.scale;
    if !pitch.is_finite() || pitch < MIN_PITCH_PX || xf.twist != 0.0 {
        return;
    }
    let visible = sheet.intersect(p.clip_rect());
    if !visible.is_positive() {
        return;
    }
    // The grid runs from the sheet's lower left corner (its origin).
    let origin = xf.to_world(pos2(sheet.min.x, sheet.max.y));
    let lo = xf.to_world(pos2(visible.min.x, visible.max.y));
    let hi = xf.to_world(pos2(visible.max.x, visible.min.y));
    let first = |a: f64, o: f64| ((a - o) / GRID_MM).ceil();
    let (i0, j0) = (first(lo.x, origin.x), first(lo.y, origin.y));
    let (ni, nj) = (((hi.x - lo.x) / GRID_MM).floor() + 1.0, ((hi.y - lo.y) / GRID_MM).floor() + 1.0);
    if !(ni.is_finite() && nj.is_finite()) || ni <= 0.0 || nj <= 0.0 || ni * nj > MAX_DOTS as f64 {
        return;
    }
    let r = ((DOT_MM * xf.scale / 2.0) as f32).max(0.5);
    let colour = ecad_ui::color(tokens.grid);
    let mut shapes = Vec::with_capacity((ni * nj) as usize);
    for i in 0..ni as i64 {
        for j in 0..nj as i64 {
            let w = Vec2::new(origin.x + (i0 + i as f64) * GRID_MM, origin.y + (j0 + j as f64) * GRID_MM);
            let at = xf.to_screen(w);
            if visible.contains(at) {
                shapes.push(egui::Shape::circle_filled(at, r, colour));
            }
        }
    }
    p.extend(shapes);
}
