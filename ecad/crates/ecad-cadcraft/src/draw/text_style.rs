//! The CADCraft side of text styles (brief 031; design 065 §3, cut 4): the engine's text styles and
//! fonts a page installs, styling a text entity, measuring text in the font it is drawn with, and
//! drawing hidden texts. The rules (kinds, resolved heights, own styles, placement) are
//! [`crate::text_style`].

use ecad_model::Document;

use crate::defaults::{self, Scope, text as d};
use crate::text_style::*;

/// Write the two CADCraft text styles of `family` into `d` (an object's own text style's font).
pub fn install_family(d: &mut cadcraft_engine::doc::Drawing, doc: &Document, family: &str) {
    let fonts = Fonts::for_family(doc, family.to_string());
    put_style(d, &family_style_name(family, Weight::Regular), &fonts.regular);
    put_style(d, &family_style_name(family, Weight::Bold), &fonts.bold);
}

/// Style a text entity of `kind` as this project draws it (the one entry point for other lanes,
/// design 031 §3.3): its height, its style name (the project font, or the kind's own font), and
/// for bold drawn in the stroke-font fallback the bold lineweight. The drawing must hold the
/// styles: [`install`] (or [`install_look`]) writes them. A caller styling many entities holds a
/// [`Look`] and calls [`Look::apply`] instead (one resolution per page, not per entity).
pub fn apply(doc: &Document, e: &mut cadcraft_engine::doc::Entity, kind: TextKind) {
    let s = text_style(doc, kind);
    if let cadcraft_engine::doc::EntityKind::Text(t) = &mut e.kind {
        t.height = s.height;
    }
    let bold_lineweight = match kind_font(doc, kind) {
        Some(f) => Fonts::for_family(doc, f.to_string()).bold_lineweight,
        None => Fonts::resolve(doc).bold_lineweight,
    };
    style_entity(e, s, bold_lineweight);
}

/// Set `s`'s style name on a text entity, and the bold lineweight when bold is drawn in the
/// stroke font.
fn style_entity(e: &mut cadcraft_engine::doc::Entity, s: TextStyle, bold_lineweight: Option<u16>) {
    if let cadcraft_engine::doc::EntityKind::Text(t) = &mut e.kind {
        t.style = s.style_name.to_string();
    }
    // Brief 050a: a style's colour (ink otherwise; a mono print prints ink regardless).
    if let Some(rgb) = s.colour.and_then(|c| crate::look::text_colour_rgb(c, crate::look::projection_sheet())) {
        e.common.color = cadcraft_engine::doc::color::Color::True(rgb);
    }
    if let (Weight::Bold, Some(lw)) = (s.weight, bold_lineweight) {
        e.common.lineweight = cadcraft_engine::doc::Lineweight::Mm100(lw);
    }
}

/// The sheet font's faces as given to CADCraft's font table.
static SHEET_FACES: [cadcraft_engine::fonts::ttf::EmbeddedFace; 2] = [
    cadcraft_engine::fonts::ttf::EmbeddedFace {
        family: ecad_drawing_style::sheet_font::FAMILY,
        style: "Regular",
        bytes: ecad_drawing_style::sheet_font::REGULAR,
    },
    cadcraft_engine::fonts::ttf::EmbeddedFace {
        family: ecad_drawing_style::sheet_font::FAMILY,
        style: "SemiBold",
        bytes: ecad_drawing_style::sheet_font::SEMIBOLD,
    },
];

/// Give CADCraft's font table the sheet font, Barlow Semi Condensed Regular and SemiBold (brief
/// 040, upstream seam US3), so every page ECAD draws or prints, in the window or headless (MCP,
/// Print Book, release), uses it. Called by [`crate::install`] and before a print; idempotent.
/// Whether the faces are registered (the same faces, or another embedder's, given first).
pub fn register_sheet_font() -> bool {
    cadcraft_engine::fonts::ttf::register_faces(&SHEET_FACES).is_ok() || !cadcraft_engine::fonts::ttf::embedder_faces().is_empty()
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
    /// Resolve `doc`'s font on this machine. Never fails: a missing font falls back (§2.2).
    pub fn resolve(doc: &Document) -> Fonts {
        let wanted = match defaults::resolve(doc, &d::FONT, Scope::project()).value {
            defaults::Value::Text(t) => t.to_string(),
            _ => default_family().to_string(),
        };
        Fonts::for_family(doc, wanted)
    }

    /// Resolve the family `wanted` on this machine, with the fallbacks of §2.2.
    pub fn for_family(doc: &Document, wanted: String) -> Fonts {
        let found = |name: &str| cadcraft_engine::fonts::ttf::find(name).is_some();
        // Brief 040: the wanted family, the sheet font (registered by the app), then 031's
        // built-in font, then the stroke font.
        let family = [wanted.as_str(), default_family(), d::FALLBACK_FONT].into_iter().find(|f| found(f)).map(str::to_string);
        match family {
            Some(f) => {
                // Bold is the family's Bold face, else its SemiBold (the sheet font's 600).
                let bold = [format!("{f} Bold"), format!("{f} SemiBold")].into_iter().find(|n| found(n)).unwrap_or_else(|| f.clone());
                Fonts { wanted, regular: f.clone(), bold, drawn_with: f, bold_lineweight: None }
            }
            None => {
                let w = defaults::resolve(doc, &d::BOLD_STROKE_WEIGHT, Scope::project()).number();
                // A lineweight is hundredths of a millimetre, at most 2.11 mm (the entry's range).
                let lw = (w * 100.0).round().clamp(0.0, 211.0) as u16;
                let stroke = cadcraft_engine::fonts::BUILTIN_FONT.to_string();
                Fonts { wanted, regular: stroke.clone(), bold: stroke, drawn_with: "stroke".into(), bold_lineweight: Some(lw) }
            }
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

/// Write ECAD's text styles into `d` (design 031 §2.3): `Standard` (the project family, regular)
/// and `ECAD-BOLD` (bold, or as §2.2), and the two styles of every kind with a font of its own
/// (`ECAD-TAG`, `ECAD-TAG-BOLD`…). Any drawing ECAD builds calls it; a drawing that already has
/// the same styles is unchanged.
pub fn install(d: &mut cadcraft_engine::doc::Drawing, doc: &Document) {
    install_look(d, &Look::new(doc));
}

/// [`install`] with the look already resolved.
pub fn install_look(d: &mut cadcraft_engine::doc::Drawing, look: &Look) {
    install_fonts(d, &look.fonts);
    for (kind, fonts) in TextKind::ALL.iter().zip(look.kind_fonts.iter()) {
        if let Some(f) = fonts {
            put_style(d, kind_style_name(*kind, Weight::Regular), &f.regular);
            put_style(d, kind_style_name(*kind, Weight::Bold), &f.bold);
        }
    }
}

/// Write `Standard` and `ECAD-BOLD` with `fonts` into `d`.
pub fn install_fonts(d: &mut cadcraft_engine::doc::Drawing, fonts: &Fonts) {
    put_style(d, STYLE_REGULAR, &fonts.regular);
    put_style(d, STYLE_BOLD, &fonts.bold);
}

fn put_style(d: &mut cadcraft_engine::doc::Drawing, name: &str, font: &String) {
    match d.text_styles.iter_mut().find(|s| s.name.eq_ignore_ascii_case(name)) {
        Some(s) => {
            if s.font != *font {
                s.font.clone_from(font);
            }
        }
        None => d.text_styles.push(cadcraft_engine::doc::TextStyle {
            name: name.to_string(),
            font: font.clone(),
            ..cadcraft_engine::doc::TextStyle::default()
        }),
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
    /// Resolve every kind's style and the fonts of `doc`.
    pub fn new(doc: &Document) -> Look {
        Look {
            styles: TextKind::ALL.map(|k| text_style(doc, k)),
            fonts: Fonts::resolve(doc),
            kind_fonts: TextKind::ALL.map(|k| kind_font(doc, k).map(|f| Fonts::for_family(doc, f.to_string()))),
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
        self.styles.get(i).copied().unwrap_or(TextStyle { height: 1.0, weight: Weight::Regular, style_name: STYLE_REGULAR, colour: None })
    }

    /// Give a text entity `kind`'s style name (the project font or the kind's own) and, for bold
    /// in the stroke font, its lineweight (§2.2), as [`apply`] does from a page's resolved look.
    /// Its height is left as it is (derived texts set their own).
    pub fn apply(&self, e: &mut cadcraft_engine::doc::Entity, kind: TextKind) {
        style_entity(e, self.style(kind), self.fonts_for(kind).bold_lineweight);
    }

    /// [`Look::apply`] with an object's own text style (brief 050a): its weight, colour and the
    /// CADCraft style naming its font.
    pub fn apply_own(&self, e: &mut cadcraft_engine::doc::Entity, kind: TextKind, own: &OwnStyle) {
        style_entity(e, own.style, self.fonts_for(kind).bold_lineweight);
        if let cadcraft_engine::doc::EntityKind::Text(t) = &mut e.kind {
            t.style.clone_from(&own.name);
        }
    }
}

/// The width of `value` at `kind`'s height in the font it is drawn with (§2.4), falling back to
/// the stroke font's width; a non-finite or negative result counts as `chars × height × 0.8`.
/// Resolves only `kind`'s style and font. A caller measuring many texts holds a [`Look`] and calls
/// [`width_in`] with [`Look::style`] and [`Look::fonts_for`] instead.
pub fn text_width(doc: &Document, kind: TextKind, value: &str) -> f64 {
    let s = text_style(doc, kind);
    let fonts = match kind_font(doc, kind) {
        Some(f) => Fonts::for_family(doc, f.to_string()),
        None => Fonts::resolve(doc),
    };
    width_in(&fonts, s, value)
}

/// [`text_width`] with the style and fonts already resolved (from a [`Look`] the caller holds).
pub fn width_in(fonts: &Fonts, s: TextStyle, value: &str) -> f64 {
    let name = match s.weight {
        Weight::Bold => &fonts.bold,
        Weight::Regular => &fonts.regular,
    };
    let font = cadcraft_engine::fonts::TextFont::resolve(name);
    let w = cadcraft_engine::fonts::text_width(&font, value, s.height, 1.0);
    if w.is_finite() && w >= 0.0 { w } else { value.chars().count() as f64 * s.height * ROUGH_WIDTH }
}

/// A character's width as a share of the height, for a width no font could give (a guard).
const ROUGH_WIDTH: f64 = 0.8;

/// The layer hidden derived texts are drawn on while Show Hidden Text is on: never plotted.
pub const HIDDEN_LAYER: &str = "ECAD-HIDDEN-TEXT";

/// `ecad.hidden-text`: ACI 8.
pub const HIDDEN_ACI: u8 = 8;

/// Add [`HIDDEN_LAYER`] to `d` when it is missing (not plotted).
pub fn add_hidden_layer(d: &mut cadcraft_engine::doc::Drawing) {
    if d.layer(HIDDEN_LAYER).is_none() {
        let mut l = cadcraft_engine::doc::Layer::new(HIDDEN_LAYER);
        l.plot = false;
        l.color = cadcraft_engine::doc::color::Color::Index(HIDDEN_ACI);
        d.layers.push(l);
    }
}

/// Draw a derived text entity as a hidden one: on [`HIDDEN_LAYER`], in `ecad.hidden-text`.
pub fn dim_hidden(e: &mut cadcraft_engine::doc::Entity) {
    e.common.layer = HIDDEN_LAYER.to_string();
    e.common.color = cadcraft_engine::doc::color::Color::Index(HIDDEN_ACI);
}
