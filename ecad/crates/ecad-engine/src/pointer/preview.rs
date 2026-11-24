//! What a click-based form draws while it waits: preview entities in the canvas colours, rings,
//! lines and preview text. Previews are never entities of the drawing.

use cadcraft_engine::Session;
use cadcraft_engine::doc::color::Color as CColor;
use cadcraft_engine::doc::{Circle, Entity, EntityKind, Handle, Line, Text};
use cadcraft_engine::geom::Vec3;
use ecad_model::Pt;

use super::pick::{Capture, pixel};

/// Canvas colours (design tokens, ACI on the dark canvas).
pub(crate) const CONNECTION_POINT_ACI: u8 = 4;
pub(crate) const ATTACHED_ACI: u8 = 3;
/// A neutral colour for loose ends, not red: light grey.
pub(crate) const LOOSE_ACI: u8 = 9;
/// "Will be refused" (`ecad.refuse-preview`, ACI 1, red).
pub(crate) const REFUSE_ACI: u8 = 1;

/// Glyph sizes in screen pixels (design tokens).
pub(crate) const CONNECTION_POINT_RING_PX: f64 = 8.0;
pub(crate) const RING_PX: f64 = 16.0;
const PREVIEW_TEXT_PX: f64 = 12.0;

/// A preview entity drawn in ACI colour `aci` (its own properties, not the current layer's).
pub(crate) fn preview_entity(s: &Session, kind: EntityKind, aci: u8) -> Entity {
    let mut common = s.current_common();
    common.color = CColor::Index(aci);
    Entity { handle: Handle(0), common, kind }
}

fn v3(p: Pt) -> Vec3 {
    Vec3::new(p.x, p.y, 0.0)
}

/// A circle of `diameter_px` screen pixels around `at`.
pub(crate) fn ring_kind(at: Pt, diameter_px: f64, px: f64) -> EntityKind {
    EntityKind::Circle(Circle { center: v3(at), radius: diameter_px * px / 2.0 })
}

pub(crate) fn line_kind(a: Pt, b: Pt) -> EntityKind {
    EntityKind::Line(Line { a: v3(a), b: v3(b) })
}

/// The "will attach" preview of a capture: a bold ring around the terminal and its
/// name beside it. The drafting font has no arrow, so the name reads `-> A1`.
pub(crate) fn capture_preview(s: &Session, c: &Capture, aci: u8) -> Vec<Entity> {
    let Some(px) = pixel(s) else { return Vec::new() };
    let mut out: Vec<Entity> = [RING_PX - 1.0, RING_PX, RING_PX + 1.0].into_iter().map(|d| preview_entity(s, ring_kind(c.at, d, px), aci)).collect();
    let at = Pt::new(c.at.x + 10.0 * px + RING_PX * px / 2.0, c.at.y + 10.0 * px);
    out.push(preview_text(s, at, px, format!("-> {}", c.name), aci));
    out
}

/// A preview text line (`ecad.preview-text`: 12 px high) at `at`, in ACI colour `aci`.
pub(crate) fn preview_text(s: &Session, at: Pt, px: f64, value: String, aci: u8) -> Entity {
    let text = Text {
        insert: v3(at),
        align_pt: None,
        height: PREVIEW_TEXT_PX * px,
        value,
        rotation: 0.0,
        width_factor: 1.0,
        oblique: 0.0,
        style: "Standard".into(),
        halign: Default::default(),
        valign: Default::default(),
    };
    preview_entity(s, EntityKind::Text(text), aci)
}
