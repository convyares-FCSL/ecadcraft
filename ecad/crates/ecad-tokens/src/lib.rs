//! ecad-tokens: the one table of ECAD's interface tokens (brief 037, design `ui-architecture.md` §3.1).
//!
//! Every colour, size, radius, line weight, type style and icon name of ECAD's interface is a
//! constant here, named as in the UI design spec §4 (`text-2` → [`shell::Shell::text_2`]). The
//! table is documented in `ecad/docs/design/tokens.md`, and a test compares every row of that
//! file's token tables with these constants, so the doc and the code cannot drift.
//!
//! - [`shell`]: the Graphite shell colours, with the contrast of every stated text pair.
//! - [`ui_type`]: the UI type ramp (px).
//! - [`space`], [`size`], [`radius`], [`line`], [`wheel`], [`shadow`]: geometry.
//! - [`icon`]: the names of the shell's icons (painted by `ecad-ui`).
//!
//! This crate **excludes** any drawing, any toolkit type and the drawing style (sheet colours,
//! sheet font and text, wire colours, markers, line weights in mm: `ecad-drawing-style`, design
//! 065 §1.1, whose [`Color`] it shares). It is plain `const` data, used by `ecad-ui` (the shell's
//! widgets).
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod icon;
pub mod line;
pub mod radius;
pub mod shadow;
pub mod shell;
pub mod size;
pub mod space;
pub mod ui_type;
pub mod wheel;

pub use icon::Icon;
pub use shell::{GRAPHITE, LIGHT, Shell};

pub use ecad_drawing_style::Color;

/// White, for text on the accent (spec §4: "white text 4.9:1").
pub const WHITE: Color = Color::hex(0xFFFFFF);
/// Black, the shadow colour before its alpha.
pub const BLACK: Color = Color::hex(0x000000);
