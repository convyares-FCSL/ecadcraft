//! The canvas palette: how an ECAD page's colours, markers and line weights are defined, and the sheet
//! font's bytes. The engine draws with it, the UI kit and the app paint the canvas with its colours, and
//! the drawing style resolves text and frames on top of it. A test compares every row of the "Drawing
//! style" section of the design tokens document with these constants.
//!
//! - [`sheet`]: the paper and dark sheet colour sets, and which meanings print.
//! - [`marker`]: the shape and screen size of each canvas marker.
//! - [`wire`]: default wire category colours and the two-colour dash.
//! - [`sheet_font`]: the embedded sheet font.
//! - [`line`]: line weights on the page, in mm.
//!
//! This module excludes any drawing, any toolkit type and the screen's own look (shell colours, spacing,
//! UI type and icons are `ecad-widgets`' tokens). It is plain `const` data and the font bytes.

pub mod line;
pub mod marker;
pub mod sheet;
pub mod sheet_font;
pub mod wire;

pub use sheet::{Meaning, Sheet};

/// An sRGB colour with straight (not premultiplied) alpha.
///
/// # Example
/// ```
/// use ecad_model::palette::Color;
/// let c = Color::hex(0x2F6BE0);
/// assert_eq!((c.r, c.g, c.b, c.a), (0x2F, 0x6B, 0xE0, 255));
/// assert_eq!(c.to_hex(), "#2F6BE0");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    /// Straight alpha, 255 opaque.
    pub a: u8,
}

impl Color {
    /// An opaque colour from `0xRRGGBB` (higher bits are ignored).
    pub const fn hex(rgb: u32) -> Self {
        Self { r: ((rgb >> 16) & 0xFF) as u8, g: ((rgb >> 8) & 0xFF) as u8, b: (rgb & 0xFF) as u8, a: 255 }
    }

    /// The same colour at `percent` % opacity (clamped to 0–100, rounded to the nearest step).
    pub const fn at_percent(self, percent: u8) -> Self {
        let p = if percent > 100 { 100 } else { percent } as u32;
        Self { a: ((255 * p + 50) / 100) as u8, ..self }
    }

    /// `#RRGGBB`, upper case; the alpha is not written (see [`Color::to_token`]).
    pub fn to_hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }

    /// How `tokens.md` writes the colour: `#RRGGBB`, or `#RRGGBB at N %` when it is translucent.
    pub fn to_token(self) -> String {
        if self.a == 255 { self.to_hex() } else { format!("{} at {} %", self.to_hex(), (u32::from(self.a) * 100 + 127) / 255) }
    }

    /// WCAG 2 relative luminance of the opaque colour, 0 (black) to 1 (white).
    pub fn luminance(self) -> f32 {
        let lin = |c: u8| {
            let c = f32::from(c) / 255.0;
            if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * lin(self.r) + 0.7152 * lin(self.g) + 0.0722 * lin(self.b)
    }

    /// WCAG 2 contrast ratio between two opaque colours, 1 to 21.
    ///
    /// # Example
    /// ```
    /// use ecad_model::palette::Color;
    /// assert!((Color::hex(0xFFFFFF).contrast(Color::hex(0x000000)) - 21.0).abs() < 0.01);
    /// ```
    pub fn contrast(self, other: Color) -> f32 {
        let (a, b) = (self.luminance(), other.luminance());
        let (hi, lo) = if a >= b { (a, b) } else { (b, a) };
        (hi + 0.05) / (lo + 0.05)
    }
}
