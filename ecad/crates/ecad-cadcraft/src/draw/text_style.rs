//! The CADCraft side of text styles (brief 031; design 065 §3, cut 4): the engine's text styles and
//! fonts a page installs, styling a text entity, measuring text in the font it is drawn with, and
//! drawing hidden texts. The rules (kinds, resolved heights, own styles, placement) are
//! [`crate::text_style`].

use ecad_model::Document;

use crate::text_style::fonts::{self, TextMetrics};
use crate::text_style::*;

/// Write the two CADCraft text styles of `family` into `d` (an object's own text style's font).
pub fn install_family(d: &mut cadcraft_engine::doc::Drawing, doc: &Document, family: &str) {
    let f = Fonts::for_family(doc, family.to_string());
    put_style(d, &family_style_name(family, Weight::Regular), &f.regular);
    put_style(d, &family_style_name(family, Weight::Bold), &f.bold);
}

/// Style a text entity of `kind` as this project draws it (the one entry point for other lanes,
/// design 031 §3.3): its height, its style name (the project font, or the kind's own font), and
/// for bold drawn in the stroke-font fallback the bold lineweight. The drawing must hold the
/// styles: [`install`] (or [`install_look`]) writes them. A caller styling many entities holds a
/// [`Look`] and calls [`LookApply::apply`] instead (one resolution per page, not per entity).
pub fn apply(doc: &Document, e: &mut cadcraft_engine::doc::Entity, kind: TextKind) {
    let s = text_style(doc, kind);
    if let cadcraft_engine::doc::EntityKind::Text(t) = &mut e.kind {
        t.height = s.height;
    }
    let bold_lineweight = fonts::Fonts::of_kind_with(doc, kind, &EngineMetrics).bold_lineweight;
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

/// CADCraft's fonts as the rules' [`TextMetrics`]: a font is found when CADCraft's font table
/// has it, and a text's width is CADCraft's measure of it in that font (design 065 §3, cut 10).
#[derive(Debug, Clone, Copy, Default)]
pub struct EngineMetrics;

impl TextMetrics for EngineMetrics {
    fn found(&self, name: &str) -> bool {
        cadcraft_engine::fonts::ttf::find(name).is_some()
    }

    fn width(&self, name: &str, text: &str, height: f64) -> f64 {
        let font = cadcraft_engine::fonts::TextFont::resolve(name);
        cadcraft_engine::fonts::text_width(&font, text, height, 1.0)
    }

    fn stroke_font(&self) -> &str {
        cadcraft_engine::fonts::BUILTIN_FONT
    }
}

/// [`fonts::Fonts`] resolved on this machine with CADCraft's fonts ([`EngineMetrics`]); it reads
/// as the rules' type ([`std::ops::Deref`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Fonts(pub fonts::Fonts);

impl std::ops::Deref for Fonts {
    type Target = fonts::Fonts;

    fn deref(&self) -> &fonts::Fonts {
        &self.0
    }
}

impl Fonts {
    /// Resolve `doc`'s font on this machine. Never fails: a missing font falls back (§2.2).
    pub fn resolve(doc: &Document) -> Fonts {
        Fonts(fonts::Fonts::resolve_with(doc, &EngineMetrics))
    }

    /// Resolve the family `wanted` on this machine, with the fallbacks of §2.2.
    pub fn for_family(doc: &Document, wanted: String) -> Fonts {
        Fonts(fonts::Fonts::for_family_with(doc, wanted, &EngineMetrics))
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
pub fn install_look(d: &mut cadcraft_engine::doc::Drawing, look: &fonts::Look) {
    install_fonts(d, &look.fonts);
    for (kind, fonts) in TextKind::ALL.iter().zip(look.kind_fonts.iter()) {
        if let Some(f) = fonts {
            put_style(d, kind_style_name(*kind, Weight::Regular), &f.regular);
            put_style(d, kind_style_name(*kind, Weight::Bold), &f.bold);
        }
    }
}

/// Write `Standard` and `ECAD-BOLD` with `fonts` into `d`.
pub fn install_fonts(d: &mut cadcraft_engine::doc::Drawing, f: &fonts::Fonts) {
    put_style(d, STYLE_REGULAR, &f.regular);
    put_style(d, STYLE_BOLD, &f.bold);
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

/// [`fonts::Look`] resolved on this machine with CADCraft's fonts ([`EngineMetrics`]), and how a
/// derived text entity takes it; it reads as the rules' type ([`std::ops::Deref`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Look(pub fonts::Look);

impl std::ops::Deref for Look {
    type Target = fonts::Look;

    fn deref(&self) -> &fonts::Look {
        &self.0
    }
}

impl Look {
    /// Resolve every kind's style and the fonts of `doc`.
    pub fn new(doc: &Document) -> Look {
        Look(fonts::Look::with(doc, &EngineMetrics))
    }
}

/// Styling a derived text entity from a page's resolved look.
pub trait LookApply {
    /// Give a text entity `kind`'s style name (the project font or the kind's own) and, for bold
    /// in the stroke font, its lineweight (§2.2), as [`apply`] does from a page's resolved look.
    /// Its height is left as it is (derived texts set their own).
    fn apply(&self, e: &mut cadcraft_engine::doc::Entity, kind: TextKind);

    /// [`LookApply::apply`] with an object's own text style (brief 050a): its weight, colour and
    /// the CADCraft style naming its font.
    fn apply_own(&self, e: &mut cadcraft_engine::doc::Entity, kind: TextKind, own: &OwnStyle);
}

impl LookApply for fonts::Look {
    fn apply(&self, e: &mut cadcraft_engine::doc::Entity, kind: TextKind) {
        style_entity(e, self.style(kind), self.fonts_for(kind).bold_lineweight);
    }

    fn apply_own(&self, e: &mut cadcraft_engine::doc::Entity, kind: TextKind, own: &OwnStyle) {
        style_entity(e, own.style, self.fonts_for(kind).bold_lineweight);
        if let cadcraft_engine::doc::EntityKind::Text(t) = &mut e.kind {
            t.style.clone_from(&own.name);
        }
    }
}

/// The width of `value` at `kind`'s height in the font it is drawn with on this machine (§2.4):
/// [`fonts::text_width`] with CADCraft's fonts.
pub fn text_width(doc: &Document, kind: TextKind, value: &str) -> f64 {
    fonts::text_width(doc, kind, value, &EngineMetrics)
}

/// [`text_width`] with the style and fonts already resolved (from a [`Look`] the caller holds).
pub fn width_in(f: &fonts::Fonts, s: TextStyle, value: &str) -> f64 {
    fonts::width_in(f, s, value, &EngineMetrics)
}

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
