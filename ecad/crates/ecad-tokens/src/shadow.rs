//! The one shadow style (spec §4): popovers and the sheet only.

use crate::{BLACK, Color};

/// Horizontal offset, px.
pub const OFFSET_X: f32 = 0.0;
/// Vertical offset, px.
pub const OFFSET_Y: f32 = 8.0;
/// Blur, px.
pub const BLUR: f32 = 24.0;
/// Colour: black at 45 %.
pub const COLOR: Color = BLACK.at_percent(45);
