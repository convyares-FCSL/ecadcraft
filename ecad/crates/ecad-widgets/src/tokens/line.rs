//! Screen line weights (spec §4, "Line weights"), in px. Canvas lines are in mm on paper: they are
//! part of the drawing style (`ecad_drawing_style::line`), and are drawn no thinner than
//! [`SCREEN_FLOOR_PX`] on screen.

/// UI hairline, px.
pub const HAIRLINE_PX: f32 = 1.0;
/// UI icon stroke, px, on a 16 px grid with round caps and joins.
pub const ICON_STROKE_PX: f32 = 1.5;
/// Focus ring, px, in `accent_text`.
pub const FOCUS_RING_PX: f32 = 2.0;
/// The thinnest a canvas line is drawn on screen, px.
pub const SCREEN_FLOOR_PX: f32 = 1.0;

/// Every screen token as `(name, px)`, names as in `tokens.md`.
pub const PX: [(&str, f32); 4] = [
    ("line.hairline", HAIRLINE_PX),
    ("line.icon_stroke", ICON_STROKE_PX),
    ("line.focus_ring", FOCUS_RING_PX),
    ("line.screen_floor", SCREEN_FLOOR_PX),
];
