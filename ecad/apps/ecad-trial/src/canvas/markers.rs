//! The markers ECAD paints over the page (brief 040, steps 2, 3 and 5), in the shapes and colours
//! of `ecad-tokens` by meaning: an error is a filled disc with "!", a warning a filled triangle
//! with "!", a provisional tag gets a dotted underline, a device's other placements on the page a
//! dashed outline in the selection colour. Markers keep their screen size at every zoom; outlines
//! follow the sheet's line weights, with a 1 px floor. (Wire colours are ink, drawn by the page's
//! projection: `ecad_cadcraft::wires::colour_routes`.)
//!
//! None of it is an entity: it is painted over CADCraft's drawing and never reaches a plot.

use cadcraft_engine::doc::Handle;
use cadcraft_engine::geom::Vec2;
use cadcraft_ui_egui::canvas::Xf;
use ecad_cadcraft::look::Painted;
use eframe::egui::{self, Pos2, Rect, Stroke, pos2, vec2};

use super::PageData;
use crate::Trial;

/// The error disc's and warning triangle's size on screen, px.
pub const MARKER_PX: f32 = 12.0;
/// The dotted underline: dot length and gap, px.
const DOT_PX: f32 = 1.5;
const DOT_GAP_PX: f32 = 2.0;
/// The outline's dash and gap, px.
const DASH_PX: f32 = 5.0;
const DASH_GAP_PX: f32 = 3.0;
/// Room between an object and its outline, px.
const OUTLINE_PAD_PX: f32 = 3.0;

fn screen(xf: &Xf, p: ecad_model::Pt) -> Pos2 {
    xf.to_screen(Vec2::new(p.x, p.y))
}

/// A canvas line weight (mm) on screen, with the 1 px floor.
fn weight(xf: &Xf, mm: f32) -> f32 {
    ((f64::from(mm) * xf.scale) as f32).max(ecad_ui::tokens::line::SCREEN_FLOOR_PX)
}

/// Paint the page's markers.
pub fn paint(p: &egui::Painter, xf: &Xf, sheet: &ecad_ui::tokens::Sheet, t: &Trial, page: &PageData) {
    for m in &page.painted {
        match *m {
            Painted::Error(at) => error_disc(p, screen(xf, at), ecad_ui::color(sheet.error)),
            Painted::Warning(at) => warning_triangle(p, screen(xf, at), ecad_ui::color(sheet.warning)),
        }
    }
    let Ok(d) = t.cad.session.doc() else { return };
    let bounds = |h: &Handle| {
        let e = d.entity(*h)?;
        let b = cadcraft_engine::doc::extent_bounds(d, e);
        let r = Rect::from_two_pos(xf.to_screen(b.min), xf.to_screen(b.max));
        (r.is_finite() && r.width() >= 0.0).then_some(r)
    };
    let provisional = ecad_ui::color(sheet.provisional);
    for r in page.provisional.iter().filter_map(&bounds) {
        dotted(p, pos2(r.min.x, r.max.y + 1.5), pos2(r.max.x, r.max.y + 1.5), Stroke::new(1.0, provisional));
    }
    // Other placements of the selected device on this page: one dashed outline each.
    let stroke = Stroke::new(weight(xf, ecad_ui::tokens::line::SELECTION_MM).min(2.0), ecad_ui::color(sheet.selection));
    for group in &page.related.here {
        let Some(r) = group.iter().filter_map(&bounds).reduce(|a, b| a.union(b)) else { continue };
        dashed_rect(p, r.expand(OUTLINE_PAD_PX), stroke);
    }
}

/// "!" drawn as a bar and a dot, in `colour`, centred on `at`, `h` tall.
fn exclamation(p: &egui::Painter, at: Pos2, h: f32, colour: egui::Color32) {
    p.line_segment([at - vec2(0.0, h * 0.5), at + vec2(0.0, h * 0.1)], Stroke::new(h * 0.18, colour));
    p.circle_filled(at + vec2(0.0, h * 0.38), h * 0.1, colour);
}

/// An error: a filled disc with "!".
pub fn error_disc(p: &egui::Painter, at: Pos2, colour: egui::Color32) {
    p.circle_filled(at, MARKER_PX / 2.0, colour);
    exclamation(p, at, MARKER_PX * 0.6, ecad_ui::color(ecad_ui::tokens::WHITE));
}

/// A warning: a filled triangle with "!".
pub fn warning_triangle(p: &egui::Painter, at: Pos2, colour: egui::Color32) {
    let s = MARKER_PX;
    let pts = vec![at + vec2(0.0, -s * 0.6), at + vec2(s * 0.6, s * 0.45), at + vec2(-s * 0.6, s * 0.45)];
    p.add(egui::Shape::convex_polygon(pts, colour, Stroke::NONE));
    exclamation(p, at + vec2(0.0, s * 0.08), s * 0.5, ecad_ui::color(ecad_ui::tokens::WHITE));
}

/// A dotted line from `a` to `b` ("dotted means not final yet").
fn dotted(p: &egui::Painter, a: Pos2, b: Pos2, stroke: Stroke) {
    p.extend(egui::Shape::dashed_line(&[a, b], stroke, DOT_PX, DOT_GAP_PX));
}

/// A dashed rectangle ("dashed means a link or a relation").
fn dashed_rect(p: &egui::Painter, r: Rect, stroke: Stroke) {
    let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
    p.extend(egui::Shape::dashed_line(&pts, stroke, DASH_PX, DASH_GAP_PX));
}
