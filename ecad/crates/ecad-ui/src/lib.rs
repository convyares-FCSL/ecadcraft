//! ecad-ui: ECAD's interface kit (brief 037, design `ui-architecture.md` §3–§4).
//!
//! - [`style`]: egui's visuals, spacing and text styles from `ecad-tokens` (the Graphite shell).
//! - [`fonts`]: IBM Plex Sans (interface) and Barlow Semi Condensed (sheet), embedded, registered
//!   on top of the font set the host app gives.
//! - [`prim`]: the nine primitives (button, icon button, pill, chip, badge, field, segmented
//!   control, key cap, row) and [`prim::read_value`], [`prim::provisional`].
//! - [`icons`]: the shell's icons, painted in code.
//! - [`showcase`]: one page drawing every primitive in every state (for screenshots).
//!
//! From brief 037 on, ECAD UI code takes colours, sizes and fonts from here, never locally
//! (`cargo xtask layers` checks it).
//!
//! This crate **excludes** anything about projects, documents or commands, and any `cadcraft-*`
//! dependency (layer class UiKit): it draws widgets from tokens and nothing else.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod fonts;
pub mod icons;
pub mod prim;
pub mod showcase;
pub mod style;

pub use ecad_tokens as tokens;
pub use ecad_tokens::Icon;

/// An `ecad-tokens` colour as egui's.
///
/// # Example
/// ```
/// let c = ecad_ui::color(ecad_tokens::GRAPHITE.accent_soft);
/// assert_eq!(c.a(), ecad_tokens::GRAPHITE.accent_soft.a);
/// ```
pub fn color(c: ecad_tokens::Color) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

/// The shell's colour set (the one appearance, Graphite).
pub const SHELL: ecad_tokens::Shell = ecad_tokens::GRAPHITE;
