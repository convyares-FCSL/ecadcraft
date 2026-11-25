//! ecad-widgets: ECAD's interface kit: generic controls and screen tokens, with no ECAD knowledge.
//!
//! - [`tokens`]: the one table of interface tokens: shell colours (Graphite and light), spacing, type, sizes.
//! - [`style`]: egui's visuals, spacing and text styles from the tokens (the shell in use: [`set_theme`]).
//! - [`control`]: the component set for values (text entry, number with unit, selector, toggle, value box, read
//!   value, link), one per kind of control, every state from tokens.
//! - [`prim`]: the primitives (button, icon button, pill, chip, badge, field, segmented control, key cap, row),
//!   [`prim::read_value`] and [`prim::provisional`].
//! - [`panel`]: a right-hand panel's section headers, value rows, status pill and rules.
//! - [`table`]: the one table every manager uses: sort, filter, group, multi-select, inline edit, go-to, change
//!   tint, CSV; virtualised.
//! - [`dialog`], [`splitter`], [`wheel`]: a dialog over the dimmed page, the handle between two panes, and the
//!   radial wheel.
//! - [`fonts`]: IBM Plex Sans (interface) and Barlow Semi Condensed (sheet), embedded, registered on top of the
//!   font set the host app gives.
//! - [`icons`]: the shell's icons, painted in code.
//! - [`showcase`]: one page drawing every component in every state (for screenshots).
//!
//! ECAD UI code takes colours, sizes and fonts from here, never locally (`cargo xtask layers` checks it).
//!
//! This crate **excludes** anything about projects, documents or commands, and any `cadcraft-*` dependency: it
//! draws widgets from tokens and nothing else.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod control;
pub mod dialog;
pub mod fonts;
pub mod icons;
pub mod panel;
pub mod prim;
pub mod showcase;
pub mod splitter;
pub mod style;
pub mod table;
pub mod tokens;
pub mod wheel;

pub use crate::tokens::Icon;

/// An `ecad-tokens` colour as egui's.
///
/// # Example
/// ```
/// let c = ecad_widgets::color(ecad_widgets::tokens::GRAPHITE.accent_soft);
/// assert_eq!(c.a(), ecad_widgets::tokens::GRAPHITE.accent_soft.a);
/// ```
pub fn color(c: ecad_model::palette::Color) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

/// An image from premultiplied RGBA bytes, `size` = `[width, height]` (a rasterised page preview).
pub fn rgba_image(size: [usize; 2], rgba: &[u8]) -> egui::ColorImage {
    egui::ColorImage::from_rgba_premultiplied(size, rgba)
}

/// The interface theme: which shell colour set [`SHELL`] reads.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Theme {
    /// Graphite, the default.
    #[default]
    Dark,
    /// The light shell.
    Light,
}

impl Theme {
    /// The shell colour set of this theme.
    pub const fn shell(self) -> &'static crate::tokens::Shell {
        match self {
            Theme::Dark => &crate::tokens::GRAPHITE,
            Theme::Light => &crate::tokens::LIGHT,
        }
    }
}

thread_local! {
    /// The theme of the interface drawn on this thread. Per thread, so tests that draw in parallel
    /// never see each other's theme; the app draws on one thread.
    static THEME: std::cell::Cell<Theme> = const { std::cell::Cell::new(Theme::Dark) };
}

/// Choose the theme [`SHELL`] reads on this thread. Re-apply [`style::apply`] after a change so
/// egui's own widgets follow.
pub fn set_theme(theme: Theme) {
    THEME.with(|t| t.set(theme));
}

/// The theme in use on this thread.
pub fn theme() -> Theme {
    THEME.with(std::cell::Cell::get)
}

/// The shell's colours in use: a handle that reads the current theme's set ([`set_theme`]), so
/// `SHELL.text` is always the colour of the theme on screen.
///
/// # Example
/// ```
/// ecad_widgets::set_theme(ecad_widgets::Theme::Light);
/// assert_eq!(ecad_widgets::SHELL.panel, ecad_widgets::tokens::LIGHT.panel);
/// ecad_widgets::set_theme(ecad_widgets::Theme::Dark);
/// assert_eq!(ecad_widgets::SHELL.panel, ecad_widgets::tokens::GRAPHITE.panel);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CurrentShell;

impl std::ops::Deref for CurrentShell {
    type Target = crate::tokens::Shell;

    fn deref(&self) -> &crate::tokens::Shell {
        theme().shell()
    }
}

/// The shell's colour set in use (Graphite or light, [`set_theme`]).
pub const SHELL: CurrentShell = CurrentShell;

/// The plural ending for a count: `""` for one, `"s"` otherwise (`format!("{n} page{}", plural(n))`).
///
/// # Semantics
/// Only the regular English `s`; words with another plural spell it out where they are written.
///
/// # Example
/// ```
/// assert_eq!(ecad_widgets::plural(1_u64), "");
/// assert_eq!(ecad_widgets::plural(2_usize), "s");
/// ```
pub fn plural<N: PartialEq + From<u8>>(n: N) -> &'static str {
    if n == N::from(1) { "" } else { "s" }
}
