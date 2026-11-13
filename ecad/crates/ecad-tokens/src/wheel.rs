//! The radial wheel's geometry (spec §4), px at 100 % UI scale; the widget itself is U4b.

/// Outer radius.
pub const OUTER_RADIUS: f32 = 150.0;
/// Inner radius.
pub const INNER_RADIUS: f32 = 54.0;
/// Number of slots.
pub const SLOTS: usize = 8;
/// Angle of one slot, degrees.
pub const SLOT_DEGREES: f32 = 41.0;
/// Gap between slots, degrees.
pub const GAP_DEGREES: f32 = 4.0;
