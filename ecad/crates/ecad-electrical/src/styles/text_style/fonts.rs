//! The fonts a project's texts are drawn with, and their widths (design 031 §2.2 and §2.4; design
//! 065 §3, cut 10, option F2).
//!
//! Which fonts a machine has and how wide a text is drawn are the drawing engine's knowledge. It
//! reaches the rules through one trait, [`TextMetrics`]: is a font found, and how wide is a text in
//! it. ecad-engine implements it over CADCraft's fonts and keeps wrappers with the engine-free
//! signatures (`text_style::Fonts::resolve(doc)`, `text_style::Look::new(doc)`, `width_in`,
//! `text_width`). [`FixedWidth`] is a fixed-width implementation for tests.
//!
//! This module looks nothing up itself and makes no OS call.

use ecad_model::Document;

use crate::defaults::{self, Scope, text as d};
use crate::styles::text_style::{TextKind, TextStyle, Weight, default_family, kind_font, text_style};

/// What the drawing engine knows about fonts on this machine: whether a font is found, and the
/// width of a text drawn in it.
pub trait TextMetrics {
    /// Whether the font `name` (a family, or a face such as `Arial Bold`) is drawn on this
    /// machine.
    fn found(&self, name: &str) -> bool;
    /// The width of `text` at `height` in the font `name`, falling back to the stroke font when
    /// `name` is not found. May be non-finite; [`width_in`] guards it.
    fn width(&self, name: &str, text: &str, height: f64) -> f64;
    /// The name of the stroke font, drawn when no family is found.
    fn stroke_font(&self) -> &str;
}

/// A fixed-width [`TextMetrics`] for tests: every font is found, and a character is
/// [`FixedWidth::SHARE`] of the height wide.
#[derive(Debug, Clone, Copy, Default)]
pub struct FixedWidth;

impl FixedWidth {
    /// A character's width as a share of the height.
    pub const SHARE: f64 = 0.6;
}

impl TextMetrics for FixedWidth {
    fn found(&self, _name: &str) -> bool {
        true
    }

    fn width(&self, _name: &str, text: &str, height: f64) -> f64 {
        text.chars().count() as f64 * height * Self::SHARE
    }

    fn stroke_font(&self) -> &str {
        "stroke"
    }
}

/// The fonts a project's texts are drawn with on this machine (design 031 §2.2).
#[derive(Debug, Clone, PartialEq)]
pub struct Fonts {
    /// The family the project asks for (`text.font`, or the default).
    pub wanted: String,
    /// The font name the regular style names: the wanted family, the default family, or the
    /// stroke font.
    pub regular: String,
    /// The font name the bold style names: the family's bold face, else [`Fonts::regular`].
    pub bold: String,
    /// What the texts are drawn with, in words for `textstyle {}` and the status line: a family
    /// name, or `stroke`.
    pub drawn_with: String,
    /// Bold drawn in the stroke font: the lineweight (hundredths of a millimetre) that makes it
    /// bold (§2.2 step 4, `text.bold_stroke_weight`); `None` with an outline font.
    pub bold_lineweight: Option<u16>,
}

impl Fonts {
    /// Resolve `doc`'s font with `m`. Never fails: a missing font falls back (§2.2).
    pub fn resolve_with(doc: &Document, m: &dyn TextMetrics) -> Fonts {
        let wanted = match defaults::resolve(doc, &d::FONT, Scope::project()).value {
            defaults::Value::Text(t) => t.to_string(),
            _ => default_family().to_string(),
        };
        Fonts::for_family_with(doc, wanted, m)
    }

    /// Resolve the family `wanted` with `m`, with the fallbacks of §2.2.
    pub fn for_family_with(doc: &Document, wanted: String, m: &dyn TextMetrics) -> Fonts {
        // Brief 040: the wanted family, the sheet font (registered by the app), then 031's
        // built-in font, then the stroke font.
        let family = [wanted.as_str(), default_family(), d::FALLBACK_FONT].into_iter().find(|f| m.found(f)).map(str::to_string);
        match family {
            Some(f) => {
                // Bold is the family's Bold face, else its SemiBold (the sheet font's 600).
                let bold = [format!("{f} Bold"), format!("{f} SemiBold")].into_iter().find(|n| m.found(n)).unwrap_or_else(|| f.clone());
                Fonts { wanted, regular: f.clone(), bold, drawn_with: f, bold_lineweight: None }
            }
            None => {
                let w = defaults::resolve(doc, &d::BOLD_STROKE_WEIGHT, Scope::project()).number();
                // A lineweight is hundredths of a millimetre, at most 2.11 mm (the entry's range).
                let lw = (w * 100.0).round().clamp(0.0, 211.0) as u16;
                let stroke = m.stroke_font().to_string();
                Fonts { wanted, regular: stroke.clone(), bold: stroke, drawn_with: "stroke".into(), bold_lineweight: Some(lw) }
            }
        }
    }

    /// The fonts `kind` is drawn with: its own family, else the project's.
    pub fn of_kind_with(doc: &Document, kind: TextKind, m: &dyn TextMetrics) -> Fonts {
        match kind_font(doc, kind) {
            Some(f) => Fonts::for_family_with(doc, f.to_string(), m),
            None => Fonts::resolve_with(doc, m),
        }
    }

    /// Whether the project's family is the one drawn.
    pub fn is_wanted(&self) -> bool {
        self.drawn_with == self.wanted
    }

    /// The status words of §4.1: `drawn with <family>`, or why another font is used.
    pub fn status(&self) -> String {
        if self.is_wanted() {
            format!("drawn with {}", self.drawn_with)
        } else if self.drawn_with == "stroke" {
            format!("{} is not available here; drawn with the stroke font", self.wanted)
        } else {
            format!("{} is not available here; drawn with {}", self.wanted, self.drawn_with)
        }
    }
}

/// Every style of a page, resolved once per draw, and the fonts: what a derived text entity needs.
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    styles: [TextStyle; TextKind::ALL.len()],
    pub fonts: Fonts,
    /// The fonts of kinds with a font of their own, in the order of [`TextKind::ALL`].
    pub kind_fonts: [Option<Fonts>; TextKind::ALL.len()],
}

impl Look {
    /// Resolve every kind's style and the fonts of `doc` with `m`.
    pub fn with(doc: &Document, m: &dyn TextMetrics) -> Look {
        Look {
            styles: TextKind::ALL.map(|k| text_style(doc, k)),
            fonts: Fonts::resolve_with(doc, m),
            kind_fonts: TextKind::ALL.map(|k| kind_font(doc, k).map(|f| Fonts::for_family_with(doc, f.to_string(), m))),
        }
    }

    /// The fonts `kind` is drawn with: its own, else the project's.
    pub fn fonts_for(&self, kind: TextKind) -> &Fonts {
        let i = TextKind::ALL.iter().position(|k| *k == kind).unwrap_or(0);
        self.kind_fonts.get(i).and_then(Option::as_ref).unwrap_or(&self.fonts)
    }

    /// The style of `kind`.
    pub fn style(&self, kind: TextKind) -> TextStyle {
        let i = TextKind::ALL.iter().position(|k| *k == kind).unwrap_or(0);
        self.styles.get(i).copied().unwrap_or(TextStyle {
            height: 1.0,
            weight: Weight::Regular,
            style_name: crate::styles::text_style::STYLE_REGULAR,
            colour: None,
        })
    }

    /// The width of `value` in `kind`'s font and `s` (a style derived from `kind`'s).
    pub fn width(&self, kind: TextKind, s: TextStyle, value: &str, m: &dyn TextMetrics) -> f64 {
        width_in(self.fonts_for(kind), s, value, m)
    }
}

/// The width of `value` at `kind`'s height in the font it is drawn with (§2.4). Resolves only
/// `kind`'s style and font; a caller measuring many texts holds a [`Look`] and calls
/// [`width_in`] instead.
pub fn text_width(doc: &Document, kind: TextKind, value: &str, m: &dyn TextMetrics) -> f64 {
    width_in(&Fonts::of_kind_with(doc, kind, m), text_style(doc, kind), value, m)
}

/// The width of `value` in `fonts` and `s`; a non-finite or negative width counts as
/// `chars × height × 0.8`.
pub fn width_in(fonts: &Fonts, s: TextStyle, value: &str, m: &dyn TextMetrics) -> f64 {
    let name = match s.weight {
        Weight::Bold => &fonts.bold,
        Weight::Regular => &fonts.regular,
    };
    let w = m.width(name, value, s.height);
    if w.is_finite() && w >= 0.0 { w } else { value.chars().count() as f64 * s.height * ROUGH_WIDTH }
}

/// A character's width as a share of the height, for a width no font could give (a guard).
const ROUGH_WIDTH: f64 = 0.8;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fixed_width_text_is_its_characters_times_the_share() {
        let doc = Document::default();
        let w = text_width(&doc, TextKind::Tag, "ABC", &FixedWidth);
        let h = text_style(&doc, TextKind::Tag).height;
        assert!((w - 3.0 * h * FixedWidth::SHARE).abs() < 1e-9);
        assert_eq!(Fonts::resolve_with(&doc, &FixedWidth).bold_lineweight, None);
    }
}
