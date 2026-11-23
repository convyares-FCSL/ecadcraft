//! What the symbol builder offers for drawn geometry: a connection point at every
//! free line end on the point grid, and a Tag slot for text that reads like a tag. Rules, so they live here and
//! not in the screen that shows the offers.

use ecad_model::{Primitive, Pt};
use serde::{Deserialize, Serialize};

use crate::symbols::graphics::POINT_GRID_MM;

/// What the builder offers and does not apply until accepted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Offer {
    /// A free line end on the point grid: a connection point there.
    Point { at: Pt },
    /// A text that reads like a tag: a Tag slot in its place (the text then goes).
    Slot { text: String, at: Pt, graphic: usize },
}

/// Whether `p` is on the point grid.
pub fn on_point_grid(p: Pt) -> bool {
    let on = |x: f64| ((x / POINT_GRID_MM).round() * POINT_GRID_MM - x).abs() < 1e-6;
    on(p.x) && on(p.y)
}

/// Whether two points are the same, to a micron.
pub fn same(a: Pt, b: Pt) -> bool {
    (a.x - b.x).abs() < 1e-6 && (a.y - b.y).abs() < 1e-6
}

/// Whether `text` reads like a tag (`-` then letters then digits), so it is offered as the Tag slot.
pub(crate) fn looks_like_tag(text: &str) -> bool {
    let Some(rest) = text.trim().strip_prefix('-') else { return false };
    let letters = rest.chars().take_while(char::is_ascii_alphabetic).count();
    let digits = rest.chars().skip(letters).take_while(char::is_ascii_digit).count();
    letters > 0 && digits > 0 && letters + digits == rest.chars().count()
}

/// The offers for geometry `graphics`: a connection point at every free
/// line end on the point grid that no other shape touches, and a Tag slot for the first text that
/// reads like a tag.
pub fn offers_for(graphics: &[Primitive]) -> Vec<Offer> {
    let mut out = Vec::new();
    let mut ends: Vec<(usize, Pt)> = Vec::new();
    for (i, g) in graphics.iter().enumerate() {
        if let Some((a, b)) = g.path_ends()
            && !matches!(g, Primitive::Polyline { closed: true, .. })
        {
            ends.extend([(i, a), (i, b)]);
        }
    }
    for (i, at) in &ends {
        let touched = graphics.iter().enumerate().any(|(j, g)| j != *i && touches(g, *at));
        if on_point_grid(*at) && !touched && !out.iter().any(|o| matches!(o, Offer::Point { at: x } if same(*x, *at))) {
            out.push(Offer::Point { at: *at });
        }
    }
    if let Some((i, at, value)) = graphics.iter().enumerate().find_map(|(i, g)| match g {
        Primitive::Text { at, value, .. } if looks_like_tag(value) => Some((i, *at, value.clone())),
        Primitive::Line { .. }
        | Primitive::Polyline { .. }
        | Primitive::Arc { .. }
        | Primitive::Circle { .. }
        | Primitive::Text { .. }
        | Primitive::Point { .. } => None,
    }) {
        out.push(Offer::Slot { text: value, at, graphic: i });
    }
    out
}

/// Whether shape `g` passes through `p`.
fn touches(g: &Primitive, p: Pt) -> bool {
    let on_seg = |a: Pt, b: Pt| {
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let len2 = dx * dx + dy * dy;
        if len2 < 1e-12 {
            return same(a, p);
        }
        let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0);
        let (qx, qy) = (a.x + t * dx, a.y + t * dy);
        ((qx - p.x).powi(2) + (qy - p.y).powi(2)).sqrt() < 1e-6
    };
    match g {
        Primitive::Line { a, b } => on_seg(*a, *b),
        Primitive::Polyline { vertices, closed } => {
            let n = vertices.len();
            let segs = if *closed { n } else { n.saturating_sub(1) };
            (0..segs).any(|k| match (vertices.get(k), vertices.get((k + 1) % n.max(1))) {
                (Some(a), Some(b)) => on_seg(*a, *b),
                _ => false,
            })
        }
        Primitive::Circle { center, radius } | Primitive::Arc { center, radius, .. } => {
            (((p.x - center.x).powi(2) + (p.y - center.y).powi(2)).sqrt() - radius).abs() < 1e-6
        }
        Primitive::Text { .. } | Primitive::Point { .. } => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_ends_on_the_grid_and_a_tag_text_are_offered() {
        let line = |a: (f64, f64), b: (f64, f64)| Primitive::Line { a: Pt::new(a.0, a.1), b: Pt::new(b.0, b.1) };
        let circle = Primitive::Circle { center: Pt::new(0.0, 15.0), radius: 5.0 };
        let g = vec![line((0.0, -10.0), (0.0, -5.0)), line((0.0, 20.0), (0.0, 25.0)), line((10.0, 15.0), (5.0, 15.0)), circle];
        let o = offers_for(&g);
        let points: Vec<Pt> = o.iter().filter_map(|x| if let Offer::Point { at } = x { Some(*at) } else { None }).collect();
        assert!(points.contains(&Pt::new(0.0, -10.0)) && points.contains(&Pt::new(0.0, 25.0)));
        assert!(!points.contains(&Pt::new(5.0, 15.0)), "an end on the circle is not free");
    }

    #[test]
    fn looks_like_a_tag() {
        assert!(looks_like_tag("-B03") && looks_like_tag("-LS1"));
        assert!(!looks_like_tag("Level high") && !looks_like_tag("-B") && !looks_like_tag("B03"));
    }
}
