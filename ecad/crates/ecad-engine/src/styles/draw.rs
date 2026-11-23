//! The CADCraft side of text styles: the engine's text styles and
//! fonts a page installs, styling a text entity, measuring text in the font it is drawn with, and
//! drawing hidden texts. The rules (kinds, resolved heights, own styles, placement) are
//! [`crate::text_style`].

use ecad_model::Document;

use ecad_electrical::styles::text_style::fonts::{self, TextMetrics};
use ecad_electrical::styles::text_style::*;
use ecad_model::look::{OwnStyle, TextKind, TextStyle};
use ecad_model::objects::styles::text::Weight;

/// Write the two CADCraft text styles of `family` into `d` (an object's own text style's font).
pub(crate) fn install_family(d: &mut cadcraft_engine::doc::Drawing, doc: &Document, family: &str) {
    let f = Fonts::for_family(doc, family.to_string());
    put_style(d, &family_style_name(family, Weight::Regular), &f.regular);
    put_style(d, &family_style_name(family, Weight::Bold), &f.bold);
}

/// Style a text entity of `kind` as this project draws it (the one entry point for the other modules):
/// its height, its style name (the project font, or the kind's own font), and
/// for bold drawn in the stroke-font fallback the bold lineweight. The drawing must hold the
/// styles: [`install`] (or [`install_draw`]) writes them. A caller styling many entities holds a
/// [`DrawStyles`] and calls [`DrawApply::apply`] instead (one resolution per page, not per entity).
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
    // A style's colour (ink otherwise; a mono print prints ink regardless).
    if let Some(rgb) = s.colour.and_then(|c| crate::cables::draw::text_colour_rgb(c, crate::cables::draw::projection_sheet())) {
        e.common.color = cadcraft_engine::doc::color::Color::True(rgb);
    }
    if let (Weight::Bold, Some(lw)) = (s.weight, bold_lineweight) {
        e.common.lineweight = cadcraft_engine::doc::Lineweight::Mm100(lw);
    }
}

/// The sheet font's faces as given to CADCraft's font table.
static SHEET_FACES: [cadcraft_engine::fonts::ttf::EmbeddedFace; 2] = [
    cadcraft_engine::fonts::ttf::EmbeddedFace {
        family: ecad_model::palette::sheet_font::FAMILY,
        style: "Regular",
        bytes: ecad_model::palette::sheet_font::REGULAR,
    },
    cadcraft_engine::fonts::ttf::EmbeddedFace {
        family: ecad_model::palette::sheet_font::FAMILY,
        style: "SemiBold",
        bytes: ecad_model::palette::sheet_font::SEMIBOLD,
    },
];

/// Give CADCraft's font table the sheet font, Barlow Semi Condensed Regular and SemiBold,
/// so every page ECAD draws or prints, in the window or headless (MCP,
/// Print Book, release), uses it. Called by [`crate::install`] and before a print; idempotent.
/// Whether the faces are registered (the same faces, or another embedder's, given first).
pub fn register_sheet_font() -> bool {
    cadcraft_engine::fonts::ttf::register_faces(&SHEET_FACES).is_ok() || !cadcraft_engine::fonts::ttf::embedder_faces().is_empty()
}

/// CADCraft's fonts as the rules' [`TextMetrics`]: a font is found when CADCraft's font table
/// has it, and a text's width is CADCraft's measure of it in that font.
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

/// The rules' text look on this machine: their text styles, measured with CADCraft's fonts.
pub static LOOK: fonts::FontLook<EngineMetrics> = fonts::FontLook(EngineMetrics);

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
    /// Resolve `doc`'s font on this machine. Never fails: a missing font falls back.
    pub fn resolve(doc: &Document) -> Fonts {
        Fonts(fonts::Fonts::resolve_with(doc, &EngineMetrics))
    }

    /// Resolve the family `wanted` on this machine, with the fallbacks.
    pub fn for_family(doc: &Document, wanted: String) -> Fonts {
        Fonts(fonts::Fonts::for_family_with(doc, wanted, &EngineMetrics))
    }
}

/// Write ECAD's text styles into `d`: `Standard` (the project family, regular)
/// and `ECAD-BOLD` (bold, or as the fallbacks give it), and the two styles of every kind with a font of its own
/// (`ECAD-TAG`, `ECAD-TAG-BOLD`…). Any drawing ECAD builds calls it; a drawing that already has
/// the same styles is unchanged.
pub fn install(d: &mut cadcraft_engine::doc::Drawing, doc: &Document) {
    install_draw(d, &DrawStyles::new(doc));
}

/// [`install`] with the look already resolved.
pub(crate) fn install_draw(d: &mut cadcraft_engine::doc::Drawing, draw: &fonts::DrawStyles) {
    install_fonts(d, &draw.fonts);
    for (kind, fonts) in TextKind::ALL.iter().zip(draw.kind_fonts.iter()) {
        if let Some(f) = fonts {
            put_style(d, kind_style_name(*kind, Weight::Regular), &f.regular);
            put_style(d, kind_style_name(*kind, Weight::Bold), &f.bold);
        }
    }
}

/// Write `Standard` and `ECAD-BOLD` with `fonts` into `d`.
pub(crate) fn install_fonts(d: &mut cadcraft_engine::doc::Drawing, f: &fonts::Fonts) {
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

/// [`fonts::DrawStyles`] resolved on this machine with CADCraft's fonts ([`EngineMetrics`]), and how a
/// derived text entity takes it; it reads as the rules' type ([`std::ops::Deref`]).
#[derive(Debug, Clone, PartialEq)]
pub struct DrawStyles(pub fonts::DrawStyles);

impl std::ops::Deref for DrawStyles {
    type Target = fonts::DrawStyles;

    fn deref(&self) -> &fonts::DrawStyles {
        &self.0
    }
}

impl DrawStyles {
    /// Resolve every kind's style and the fonts of `doc`.
    pub fn new(doc: &Document) -> DrawStyles {
        DrawStyles(fonts::DrawStyles::with(doc, &EngineMetrics))
    }
}

/// Styling a derived text entity from a page's resolved look.
pub(crate) trait DrawApply {
    /// Give a text entity `kind`'s style name (the project font or the kind's own) and, for bold
    /// in the stroke font, its lineweight, as [`apply`] does from a page's resolved look.
    /// Its height is left as it is (derived texts set their own).
    fn apply(&self, e: &mut cadcraft_engine::doc::Entity, kind: TextKind);

    /// [`DrawApply::apply`] with an object's own text style: its weight, colour and
    /// the CADCraft style naming its font.
    fn apply_own(&self, e: &mut cadcraft_engine::doc::Entity, kind: TextKind, own: &OwnStyle);
}

impl DrawApply for fonts::DrawStyles {
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

/// The width of `value` at `kind`'s height in the font it is drawn with on this machine.
pub fn text_width(doc: &Document, kind: TextKind, value: &str) -> f64 {
    ecad_model::look::Look::text_width(&LOOK, doc, kind, value)
}

/// [`text_width`] with the style and fonts already resolved (from a [`DrawStyles`] the caller holds).
pub fn width_in(f: &fonts::Fonts, s: TextStyle, value: &str) -> f64 {
    fonts::width_in(f, s, value, &EngineMetrics)
}

/// The layer hidden derived texts are drawn on while Show Hidden Text is on: never plotted.
pub const HIDDEN_LAYER: &str = "ECAD-HIDDEN-TEXT";

/// `ecad.hidden-text`: ACI 8.
pub(crate) const HIDDEN_ACI: u8 = 8;

/// Add [`HIDDEN_LAYER`] to `d` when it is missing (not plotted).
pub(crate) fn add_hidden_layer(d: &mut cadcraft_engine::doc::Drawing) {
    if d.layer(HIDDEN_LAYER).is_none() {
        let mut l = cadcraft_engine::doc::Layer::new(HIDDEN_LAYER);
        l.plot = false;
        l.color = cadcraft_engine::doc::color::Color::Index(HIDDEN_ACI);
        d.layers.push(l);
    }
}

/// Draw a derived text entity as a hidden one: on [`HIDDEN_LAYER`], in `ecad.hidden-text`.
pub(crate) fn dim_hidden(e: &mut cadcraft_engine::doc::Entity) {
    e.common.layer = HIDDEN_LAYER.to_string();
    e.common.color = cadcraft_engine::doc::color::Color::Index(HIDDEN_ACI);
}
