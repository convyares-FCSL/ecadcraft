//! Canvas markers: the shape of each meaning and, for the screen-size markers, their size (spec
//! §4, "Canvas colours"). Markers are drawn at a fixed screen size and do not scale with zoom;
//! the line-like marks (terminal link, selection outline, agent's last write) are in millimetres
//! and scale with the sheet ([`crate::line`]).

use crate::sheet::Meaning;

/// How a meaning is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    /// No marker: the meaning colours a drawing (sheet, ink, grid dots).
    None,
    /// A hollow ring.
    Ring,
    /// A solid dot.
    Dot,
    /// A hollow square.
    Square,
    /// An oblique slash with a dot per core; the tag in the same colour.
    Slash,
    /// A filled disc with "!". A refused preview is a red outline with hatching.
    Disc,
    /// A filled triangle with "!".
    Triangle,
    /// Text in the colour with a dotted underline.
    DottedUnderline,
    /// A dashed bar.
    DashedBar,
    /// A translucent fill and an outline (selection: four grips; other placements of the same
    /// device dashed).
    Outline,
    /// A dashed outline with a name chip.
    DashedOutline,
}

/// A marker: its shape and, when it is drawn at a fixed screen size, that size in px.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Marker {
    pub shape: Shape,
    /// Outer size on screen at 100 % UI scale (ring and dot diameter, square side).
    pub size_px: Option<f32>,
}

/// Connection point ring, 8 px.
pub const CONNECTION_POINT_PX: f32 = 8.0;
/// Attached wire end dot, 5 px.
pub const WIRE_END_ATTACHED_PX: f32 = 5.0;
/// Loose wire end square, 7 px.
pub const LOOSE_END_PX: f32 = 7.0;

/// The marker of a meaning.
pub const fn marker(meaning: Meaning) -> Marker {
    let (shape, size_px) = match meaning {
        Meaning::Sheet | Meaning::Ink | Meaning::Ink2 | Meaning::Grid => (Shape::None, None),
        Meaning::ConnectionPoint => (Shape::Ring, Some(CONNECTION_POINT_PX)),
        Meaning::WireEndAttached => (Shape::Dot, Some(WIRE_END_ATTACHED_PX)),
        Meaning::LooseEnd => (Shape::Square, Some(LOOSE_END_PX)),
        Meaning::Cable => (Shape::Slash, None),
        Meaning::Error => (Shape::Disc, None),
        Meaning::Warning | Meaning::MissingData => (Shape::Triangle, None),
        Meaning::Provisional => (Shape::DottedUnderline, None),
        Meaning::TerminalLink => (Shape::DashedBar, None),
        Meaning::Selection | Meaning::PlacementPreview => (Shape::Outline, None),
        Meaning::AgentWrite => (Shape::DashedOutline, None),
    };
    Marker { shape, size_px }
}
