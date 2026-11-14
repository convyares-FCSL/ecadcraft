//! ecad-ui: ECAD's interface kit (brief 037, design `ui-architecture.md` §3–§4).
//!
//! - [`style`]: egui's visuals, spacing and text styles from `ecad-tokens` (the shell in use,
//!   Graphite or light: [`set_theme`]).
//! - [`fonts`]: IBM Plex Sans (interface) and Barlow Semi Condensed (sheet), embedded, registered
//!   on top of the font set the host app gives.
//! - [`prim`]: the nine primitives (button, icon button, pill, chip, badge, field, segmented
//!   control, key cap, row) and [`prim::read_value`], [`prim::provisional`].
//! - [`icons`]: the shell's icons, painted in code.
//! - [`showcase`]: one page drawing every primitive in every state (for screenshots).
//! - [`table`]: the one table every manager uses (brief 039): sort, filter, group, multi-select,
//!   inline edit, go-to, change tint, CSV; virtualised.
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
pub mod table;
pub mod wheel;

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

/// An image from premultiplied RGBA bytes, `size` = `[width, height]` (a rasterised page preview).
pub fn rgba_image(size: [usize; 2], rgba: &[u8]) -> egui::ColorImage {
    egui::ColorImage::from_rgba_premultiplied(size, rgba)
}

/// The interface theme (UI rev 3, boards 1.17 and 1.18): which shell colour set [`SHELL`] reads.
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
    pub const fn shell(self) -> &'static ecad_tokens::Shell {
        match self {
            Theme::Dark => &ecad_tokens::GRAPHITE,
            Theme::Light => &ecad_tokens::LIGHT,
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
/// ecad_ui::set_theme(ecad_ui::Theme::Light);
/// assert_eq!(ecad_ui::SHELL.panel, ecad_tokens::LIGHT.panel);
/// ecad_ui::set_theme(ecad_ui::Theme::Dark);
/// assert_eq!(ecad_ui::SHELL.panel, ecad_tokens::GRAPHITE.panel);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CurrentShell;

impl std::ops::Deref for CurrentShell {
    type Target = ecad_tokens::Shell;

    fn deref(&self) -> &ecad_tokens::Shell {
        theme().shell()
    }
}

/// The shell's colour set in use (Graphite or light, [`set_theme`]).
pub const SHELL: CurrentShell = CurrentShell;
