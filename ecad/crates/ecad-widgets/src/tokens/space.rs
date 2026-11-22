//! Spacing (spec §4, "Spacing, sizes, radii"), px at 100 % UI scale.

/// The spacing steps; nothing in the shell uses a gap that is not one of these.
pub const STEPS: [f32; 7] = [2.0, 4.0, 6.0, 8.0, 12.0, 16.0, 24.0];
pub const XXS: f32 = 2.0;
pub const XS: f32 = 4.0;
pub const S: f32 = 6.0;
pub const M: f32 = 8.0;
pub const L: f32 = 12.0;
pub const XL: f32 = 16.0;
pub const XXL: f32 = 24.0;
/// Padding inside a panel.
pub const PANEL_PADDING: f32 = 12.0;

/// Every token as `(name, px)`, names as in `tokens.md`.
pub const ALL: [(&str, f32); 8] = [
    ("space.xxs", XXS),
    ("space.xs", XS),
    ("space.s", S),
    ("space.m", M),
    ("space.l", L),
    ("space.xl", XL),
    ("space.xxl", XXL),
    ("space.panel_padding", PANEL_PADDING),
];
