//! Every icon paints inside its rect with 1.5 px strokes (brief 037, acceptance criterion 2).

use ecad_tokens::{Icon, line};
use egui::{Color32, Rect, Shape, pos2, vec2};

/// The stroke widths of `shape`'s outlines (none for a filled dot).
fn widths(shape: &Shape, out: &mut Vec<f32>) {
    match shape {
        Shape::Vec(v) => v.iter().for_each(|s| widths(s, out)),
        Shape::LineSegment { stroke, .. } => out.push(stroke.width),
        Shape::Path(p) => out.push(p.stroke.width),
        Shape::Circle(c) if c.stroke.width > 0.0 => out.push(c.stroke.width),
        Shape::Circle(c) => assert!(c.fill != Color32::TRANSPARENT, "a dot is filled"),
        other => panic!("icons use lines, arcs and circles only, not {other:?}"),
    }
}

#[test]
fn every_icon_paints_inside_its_rect_with_the_icon_stroke() {
    for side in [16.0, 20.0, 12.0] {
        let rect = Rect::from_min_size(pos2(100.0, 50.0), vec2(side, side));
        for icon in Icon::ALL {
            let shapes = ecad_ui::icons::shapes(rect, icon, Color32::WHITE);
            assert!(!shapes.is_empty(), "{icon:?} paints");
            let mut w = Vec::new();
            for s in &shapes {
                let bounds = s.visual_bounding_rect();
                assert!(rect.expand(0.01).contains_rect(bounds), "{icon:?} at {side} px: {bounds:?} outside {rect:?}");
                widths(s, &mut w);
            }
            assert!(!w.is_empty(), "{icon:?} has strokes");
            assert!(w.iter().all(|x| *x == line::ICON_STROKE_PX), "{icon:?}: stroke widths {w:?}");
        }
    }
}

/// Icons scale to the shorter side and stay centred in a non-square rect.
#[test]
fn icons_fit_a_wide_rect() {
    let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(40.0, 16.0));
    for icon in Icon::ALL {
        for s in ecad_ui::icons::shapes(rect, icon, Color32::WHITE) {
            let b = s.visual_bounding_rect();
            assert!(b.left() >= 12.0 - 0.01 && b.right() <= 28.0 + 0.01, "{icon:?}: {b:?}");
        }
    }
}
