//! Text styles (brief 031, design 031 §3): the kinds of text ECAD draws, their resolved height,
//! weight and CADCraft style name, and whether a text is shown.
//!
//! **The shared contract** (lanes W and P read it): [`text_style`], [`TextStyle`], [`Weight`] and
//! [`TextKind`]. The name, signature and the three fields do not change in v0.15; kinds may be
//! added. [`text_shown`] is public too (lane W reads wire-label visibility through it).
//!
//! Every value comes from the defaults registry ([`crate::defaults`], entries in
//! `defaults/text.rs`); nothing here writes a size or a weight as a literal. Both readers are a
//! few map lookups and never fail: a hostile stored value counts as absent.
//!
//! This module draws nothing and stores nothing.

use ecad_model::{ComponentId, ConductorId, Document, PageId, RepresentationId};

use crate::defaults::{self, Entry, Scope, text as d};

/// The CADCraft text style of regular ECAD text (the project font, regular).
pub const STYLE_REGULAR: &str = "Standard";
/// The CADCraft text style of bold ECAD text (the project font, bold).
pub const STYLE_BOLD: &str = "ECAD-BOLD";

/// One row of the style table, resolved for this project.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    /// Height in drawing units (mm, or inches in an inch project); always finite and in range.
    pub height: f64,
    pub weight: Weight,
    /// The CADCraft text style an entity of this kind names: [`STYLE_REGULAR`] or [`STYLE_BOLD`].
    pub style_name: &'static str,
}

/// A text weight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weight {
    Regular,
    Bold,
}

impl Weight {
    /// The CADCraft text style for this weight.
    pub fn style_name(self) -> &'static str {
        match self {
            Weight::Regular => STYLE_REGULAR,
            Weight::Bold => STYLE_BOLD,
        }
    }
}

/// A kind of text: one row of the style table (design 031 §3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TextKind {
    /// A device tag beside a symbol.
    Tag,
    /// A connection point label (`A1`, `13`, strip `1`).
    PointLabel,
    /// A device's description under its tag.
    Description,
    /// A device's part number under its tag.
    PartNumber,
    /// A continuation's name.
    ContinuationName,
    /// Cross-reference texts.
    Reference,
    /// Conductor labels (height stored in `settings.wiring.label_height`).
    WireLabel,
    /// Cable look: core labels, wire numbers at the slash, label box lines.
    CableText,
    /// A location box's `+A1`.
    LocationLabel,
    /// Title block values.
    TitleValue,
    /// Title block box captions.
    TitleCaption,
    /// Project and page title in the title block.
    TitleMain,
    /// Frame column and row labels.
    Grid,
    /// Report table cells and contact-table rows.
    Table,
    /// Report column headers.
    TableHeader,
    /// Free text: its font only (a typed height is the user's).
    Free,
    /// The title block's page number box (lane P, design 033 §1.2).
    TitlePage,
    /// The title block's revision box and pending marker (lane P).
    TitleRev,
    /// Location and strip headings, `(continued)` (lane P).
    TableHeading,
    /// Cover lines (lane P scales them from this height).
    Cover,
}

impl TextKind {
    /// Every kind, in the order of the style table.
    pub const ALL: [TextKind; 20] = [
        TextKind::Tag,
        TextKind::PointLabel,
        TextKind::Description,
        TextKind::PartNumber,
        TextKind::ContinuationName,
        TextKind::Reference,
        TextKind::WireLabel,
        TextKind::CableText,
        TextKind::LocationLabel,
        TextKind::TitleValue,
        TextKind::TitleCaption,
        TextKind::TitleMain,
        TextKind::Grid,
        TextKind::Table,
        TextKind::TableHeader,
        TextKind::Free,
        TextKind::TitlePage,
        TextKind::TitleRev,
        TextKind::TableHeading,
        TextKind::Cover,
    ];

    /// The key word (`tag`, `point_label`…), as in `text.<word>.height` and the commands.
    pub fn word(self) -> &'static str {
        match self {
            TextKind::Tag => "tag",
            TextKind::PointLabel => "point_label",
            TextKind::Description => "description",
            TextKind::PartNumber => "part_number",
            TextKind::ContinuationName => "continuation_name",
            TextKind::Reference => "reference",
            TextKind::WireLabel => "wire_label",
            TextKind::CableText => "cable_text",
            TextKind::LocationLabel => "location",
            TextKind::TitleValue => "title_value",
            TextKind::TitleCaption => "title_caption",
            TextKind::TitleMain => "title_main",
            TextKind::Grid => "grid",
            TextKind::Table => "table",
            TextKind::TableHeader => "table_header",
            TextKind::Free => "free",
            TextKind::TitlePage => "title_page",
            TextKind::TitleRev => "title_rev",
            TextKind::TableHeading => "table_heading",
            TextKind::Cover => "cover",
        }
    }

    /// The kind with this key word (exact, lower case).
    pub fn from_word(word: &str) -> Option<TextKind> {
        TextKind::ALL.into_iter().find(|k| k.word() == word)
    }

    /// The registry entry of this kind's height.
    pub fn height_entry(self) -> &'static Entry {
        match self {
            TextKind::Tag => &d::TAG_HEIGHT,
            TextKind::PointLabel => &d::POINT_LABEL_HEIGHT,
            TextKind::Description => &d::DESCRIPTION_HEIGHT,
            TextKind::PartNumber => &d::PART_NUMBER_HEIGHT,
            TextKind::ContinuationName => &d::CONTINUATION_NAME_HEIGHT,
            TextKind::Reference => &d::REFERENCE_HEIGHT,
            TextKind::WireLabel => &d::WIRE_LABEL_HEIGHT,
            TextKind::CableText => &d::CABLE_TEXT_HEIGHT,
            TextKind::LocationLabel => &d::LOCATION_HEIGHT,
            TextKind::TitleValue => &d::TITLE_VALUE_HEIGHT,
            TextKind::TitleCaption => &d::TITLE_CAPTION_HEIGHT,
            TextKind::TitleMain => &d::TITLE_MAIN_HEIGHT,
            TextKind::Grid => &d::GRID_HEIGHT,
            TextKind::Table => &d::TABLE_HEIGHT,
            TextKind::TableHeader => &d::TABLE_HEADER_HEIGHT,
            TextKind::Free => &d::FREE_HEIGHT,
            TextKind::TitlePage => &d::TITLE_PAGE_HEIGHT,
            TextKind::TitleRev => &d::TITLE_REV_HEIGHT,
            TextKind::TableHeading => &d::TABLE_HEADING_HEIGHT,
            TextKind::Cover => &d::COVER_HEIGHT,
        }
    }

    /// The registry entry of this kind's weight.
    pub fn weight_entry(self) -> &'static Entry {
        match self {
            TextKind::Tag => &d::TAG_WEIGHT,
            TextKind::PointLabel => &d::POINT_LABEL_WEIGHT,
            TextKind::Description => &d::DESCRIPTION_WEIGHT,
            TextKind::PartNumber => &d::PART_NUMBER_WEIGHT,
            TextKind::ContinuationName => &d::CONTINUATION_NAME_WEIGHT,
            TextKind::Reference => &d::REFERENCE_WEIGHT,
            TextKind::WireLabel => &d::WIRE_LABEL_WEIGHT,
            TextKind::CableText => &d::CABLE_TEXT_WEIGHT,
            TextKind::LocationLabel => &d::LOCATION_WEIGHT,
            TextKind::TitleValue => &d::TITLE_VALUE_WEIGHT,
            TextKind::TitleCaption => &d::TITLE_CAPTION_WEIGHT,
            TextKind::TitleMain => &d::TITLE_MAIN_WEIGHT,
            TextKind::Grid => &d::GRID_WEIGHT,
            TextKind::Table => &d::TABLE_WEIGHT,
            TextKind::TableHeader => &d::TABLE_HEADER_WEIGHT,
            TextKind::Free => &d::FREE_WEIGHT,
            TextKind::TitlePage => &d::TITLE_PAGE_WEIGHT,
            TextKind::TitleRev => &d::TITLE_REV_WEIGHT,
            TextKind::TableHeading => &d::TABLE_HEADING_WEIGHT,
            TextKind::Cover => &d::COVER_WEIGHT,
        }
    }

    /// The registry entry of this kind's visibility, for the kinds that have one (design 031
    /// §5.1); `None` for kinds that are always shown.
    pub fn show_entry(self) -> Option<&'static Entry> {
        match self {
            TextKind::Tag => Some(&d::TAG_SHOW),
            TextKind::PointLabel => Some(&d::POINT_LABEL_SHOW),
            TextKind::Description => Some(&d::DESCRIPTION_SHOW),
            TextKind::PartNumber => Some(&d::PART_NUMBER_SHOW),
            TextKind::ContinuationName => Some(&d::CONTINUATION_NAME_SHOW),
            TextKind::Reference => Some(&d::REFERENCE_SHOW),
            TextKind::LocationLabel => Some(&d::LOCATION_SHOW),
            TextKind::WireLabel => Some(&d::WIRE_LABEL_SHOW),
            _ => None,
        }
    }
}

/// The refusal of a `text.` key through `projectprops`, `pageprops` or `setprop` (design 031
/// §10.6).
pub const RESERVED_KEY: &str = "\"text.…\" is set with Text styles (textstyle, showtext, movetext).";

/// Whether `key` is one of the text commands' keys (`text.` prefix, any case).
pub fn is_reserved_key(key: &str) -> bool {
    key.trim().get(..5).is_some_and(|p| p.eq_ignore_ascii_case("text."))
}

/// The style of `kind` in this project: the project's `text.<kind>.*` keys (and, for
/// [`TextKind::WireLabel`], `settings.wiring.label_height`), else the defaults of design 031 §3.2.
/// Never fails; a hostile stored value gives the default.
///
/// # Example
///
/// ```
/// use ecad_cadcraft::text_style::{text_style, TextKind, Weight};
/// let doc = ecad_model::Document::default();
/// let tag = text_style(&doc, TextKind::Tag);
/// assert_eq!((tag.height, tag.weight, tag.style_name), (2.5, Weight::Bold, "ECAD-BOLD"));
/// ```
pub fn text_style(doc: &Document, kind: TextKind) -> TextStyle {
    let height = defaults::resolve(doc, kind.height_entry(), Scope::project()).length(doc);
    let weight = match defaults::resolve(doc, kind.weight_entry(), Scope::project()).word() {
        "bold" => Weight::Bold,
        _ => Weight::Regular,
    };
    TextStyle { height, weight, style_name: weight.style_name() }
}

/// Whether a text is shown (design 031 §5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shown {
    On,
    Off,
    /// Once per run (§5.3): on the first symbol of each run, hidden on the others. Tags only.
    First,
}

impl Shown {
    /// The stored word: `on`, `off` or `first`.
    pub fn word(self) -> &'static str {
        match self {
            Shown::On => "on",
            Shown::Off => "off",
            Shown::First => "first",
        }
    }

    fn from_word(w: &str) -> Shown {
        match w {
            "off" => Shown::Off,
            "first" => Shown::First,
            _ => Shown::On,
        }
    }
}

/// The built-in run rule of a symbol (design 031 §5.3): strip terminals and PLC channels show
/// their tag once per run unless something narrower says otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RunRule {
    /// Any other symbol.
    #[default]
    None,
    /// A strip terminal (starter kind `Terminal`).
    Strip,
    /// A PLC channel (starter kind `Plc`).
    Plc,
}

/// What a text belongs to, for [`text_shown`]: a symbol (its representation, component and page),
/// a conductor, a page or the project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextTarget {
    pub page: Option<PageId>,
    pub component: Option<ComponentId>,
    pub representation: Option<RepresentationId>,
    pub conductor: Option<ConductorId>,
    /// The symbol's run rule (only for a tag).
    pub run: RunRule,
}

impl TextTarget {
    /// The project only.
    pub fn project() -> Self {
        TextTarget::default()
    }

    /// A page.
    pub fn page(page: PageId) -> Self {
        TextTarget { page: Some(page), ..TextTarget::default() }
    }

    /// A symbol: `rep`, its component and its page (looked up in `doc`), with its run rule.
    pub fn symbol(doc: &Document, rep: RepresentationId, run: RunRule) -> Self {
        let s = Scope::symbol(doc, rep);
        TextTarget { page: s.page, component: s.component, representation: Some(rep), conductor: None, run }
    }

    /// A conductor drawn on `page` (lane W: wire labels).
    pub fn conductor(conductor: ConductorId, page: Option<PageId>) -> Self {
        TextTarget { page, conductor: Some(conductor), ..TextTarget::default() }
    }

    fn scope(self) -> Scope {
        Scope { page: self.page, component: self.component, rep: self.representation, conductor: self.conductor }
    }
}

/// Whether a text of `kind` on `target` is shown (design 031 §5.2): the narrowest level that has a
/// value wins (symbol, device or conductor, page, project); then, for a tag with a run rule, the
/// project's `text.tag.show.strip` / `.plc`; then the built-in. `First` is only ever returned for
/// a tag. Kinds with no visibility entry are always `On`. Never fails.
pub fn text_shown(doc: &Document, target: TextTarget, kind: TextKind) -> Shown {
    let Some(entry) = kind.show_entry() else { return Shown::On };
    let r = defaults::resolve(doc, entry, target.scope());
    if r.from == defaults::Source::Builtin && kind == TextKind::Tag {
        let rule = match target.run {
            RunRule::None => None,
            RunRule::Strip => Some(&d::TAG_SHOW_STRIP),
            RunRule::Plc => Some(&d::TAG_SHOW_PLC),
        };
        if let Some(rule) = rule {
            return Shown::from_word(defaults::resolve(doc, rule, Scope::project()).word());
        }
    }
    Shown::from_word(r.word())
}

// ---------------------------------------------------------------- Fonts (§2)

/// The family CADCraft's resolver looks for when the project's family is missing: the built-in of
/// `text.font` (§2.2 step 3).
fn default_family() -> &'static str {
    match d::FONT.builtin {
        defaults::Value::Text(t) => t,
        _ => "",
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
    /// Resolve `doc`'s font on this machine. Never fails: a missing font falls back (§2.2).
    pub fn resolve(doc: &Document) -> Fonts {
        let wanted = match defaults::resolve(doc, &d::FONT, Scope::project()).value {
            defaults::Value::Text(t) => t.to_string(),
            _ => default_family().to_string(),
        };
        let found = |name: &str| cadcraft_engine::fonts::ttf::find(name).is_some();
        let family = if found(&wanted) {
            Some(wanted.clone())
        } else if found(default_family()) {
            Some(default_family().to_string())
        } else {
            None
        };
        match family {
            Some(f) => {
                let bold_name = format!("{f} Bold");
                let bold = if found(&bold_name) { bold_name } else { f.clone() };
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

/// Write ECAD's two text styles into `d` (design 031 §2.3): `Standard` (the project family,
/// regular) and `ECAD-BOLD` (bold, or as §2.2). Any drawing ECAD builds calls it; a drawing that
/// already has the same styles is unchanged.
pub fn install(d: &mut cadcraft_engine::doc::Drawing, doc: &Document) {
    install_fonts(d, &Fonts::resolve(doc));
}

/// [`install`] with fonts already resolved.
pub fn install_fonts(d: &mut cadcraft_engine::doc::Drawing, fonts: &Fonts) {
    for (name, font) in [(STYLE_REGULAR, &fonts.regular), (STYLE_BOLD, &fonts.bold)] {
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
}

/// Every style of a page, resolved once per draw, and the fonts: what a derived text entity needs.
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    styles: [TextStyle; TextKind::ALL.len()],
    pub fonts: Fonts,
}

impl Look {
    /// Resolve every kind's style and the fonts of `doc`.
    pub fn new(doc: &Document) -> Look {
        Look { styles: TextKind::ALL.map(|k| text_style(doc, k)), fonts: Fonts::resolve(doc) }
    }

    /// The style of `kind`.
    pub fn style(&self, kind: TextKind) -> TextStyle {
        let i = TextKind::ALL.iter().position(|k| *k == kind).unwrap_or(0);
        self.styles.get(i).copied().unwrap_or(TextStyle { height: 1.0, weight: Weight::Regular, style_name: STYLE_REGULAR })
    }

    /// Give a text entity `kind`'s style name, and, for bold in the stroke font, its lineweight
    /// (§2.2). Its height is left as it is.
    pub fn apply(&self, e: &mut cadcraft_engine::doc::Entity, kind: TextKind) {
        let s = self.style(kind);
        if let cadcraft_engine::doc::EntityKind::Text(t) = &mut e.kind {
            t.style = s.style_name.to_string();
        }
        if let (Weight::Bold, Some(lw)) = (s.weight, self.fonts.bold_lineweight) {
            e.common.lineweight = cadcraft_engine::doc::Lineweight::Mm100(lw);
        }
    }
}

/// The width of `value` at `kind`'s height in the font it is drawn with (§2.4), falling back to
/// the stroke font's width; a non-finite or negative result counts as `chars × height × 0.8`.
pub fn text_width(doc: &Document, kind: TextKind, value: &str) -> f64 {
    let s = text_style(doc, kind);
    let fonts = Fonts::resolve(doc);
    width_in(&fonts, s, value)
}

/// [`text_width`] with the style and fonts already resolved.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::defaults::Value;
    use ecad_model::{Component, LengthUnit, Page, Representation};

    /// Design 031 §3.2, written out here so a reader and its entry cannot drift.
    const TABLE: [(TextKind, f64, Weight); 20] = [
        (TextKind::Tag, 2.5, Weight::Bold),
        (TextKind::PointLabel, 1.5, Weight::Regular),
        (TextKind::Description, 1.6, Weight::Regular),
        (TextKind::PartNumber, 1.6, Weight::Regular),
        (TextKind::ContinuationName, 2.0, Weight::Regular),
        (TextKind::Reference, 2.0, Weight::Regular),
        (TextKind::WireLabel, ecad_model::wiring::DEFAULT_LABEL_HEIGHT, Weight::Regular),
        (TextKind::CableText, 1.6, Weight::Regular),
        (TextKind::LocationLabel, 1.6, Weight::Regular),
        (TextKind::TitleValue, 2.5, Weight::Regular),
        (TextKind::TitleCaption, 1.8, Weight::Regular),
        (TextKind::TitleMain, 3.5, Weight::Bold),
        (TextKind::Grid, 3.0, Weight::Regular),
        (TextKind::Table, 2.5, Weight::Regular),
        (TextKind::TableHeader, 2.5, Weight::Bold),
        (TextKind::Free, 2.5, Weight::Regular),
        (TextKind::TitlePage, 7.0, Weight::Regular),
        (TextKind::TitleRev, 5.0, Weight::Regular),
        (TextKind::TableHeading, 3.5, Weight::Regular),
        (TextKind::Cover, 10.0, Weight::Regular),
    ];

    #[test]
    fn an_empty_project_gets_the_built_in_of_every_kind() {
        let doc = Document::default();
        for (kind, h, w) in TABLE {
            let s = text_style(&doc, kind);
            assert_eq!(s, TextStyle { height: h, weight: w, style_name: w.style_name() }, "{kind:?}");
            assert_eq!(kind.height_entry().builtin, Value::Length(h), "{kind:?}");
            assert_eq!(TextKind::from_word(kind.word()), Some(kind));
            assert!(kind.height_entry().key.starts_with(&format!("text.{}.", kind.word())));
            assert!(kind.weight_entry().key.starts_with(&format!("text.{}.", kind.word())));
        }
    }

    #[test]
    fn hostile_heights_give_the_default() {
        for bad in ["NaN", "abc", "0.1", "11", "1e308", "-2", ""] {
            let mut doc = Document::default();
            doc.properties.insert("text.tag.height".into(), bad.into());
            doc.properties.insert("text.tag.weight".into(), "heavy".into());
            assert_eq!(text_style(&doc, TextKind::Tag), TextStyle { height: 2.5, weight: Weight::Bold, style_name: STYLE_BOLD }, "{bad:?}");
        }
    }

    #[test]
    fn project_values_and_inch_projects() {
        let mut doc = Document::default();
        doc.properties.insert("text.tag.height".into(), "3".into());
        doc.properties.insert("text.tag.weight".into(), "Regular".into());
        assert_eq!(text_style(&doc, TextKind::Tag), TextStyle { height: 3.0, weight: Weight::Regular, style_name: STYLE_REGULAR });
        doc.settings.length_unit = LengthUnit::Inch;
        assert!((text_style(&doc, TextKind::Tag).height - 3.0 / 25.4).abs() < 1e-12);
        doc.settings.wiring.label_height = 2.0;
        assert!((text_style(&doc, TextKind::WireLabel).height - 2.0 / 25.4).abs() < 1e-12);
    }

    #[test]
    fn shown_resolves_symbol_device_page_project_and_runs() {
        let mut doc = Document::default();
        let page = PageId(1);
        let comp = ComponentId(2);
        let rep = RepresentationId(3);
        doc.pages.insert(page, Page::new("p"));
        doc.components.insert(comp, Component::default());
        doc.representations.insert(
            rep,
            Representation {
                subject: ecad_model::Subject::Component(comp),
                page,
                form: ecad_model::Form::Symbol,
                role: String::new(),
                items: Vec::new(),
                connection_points: Default::default(),
            },
        );
        let plain = TextTarget::symbol(&doc, rep, RunRule::None);
        let strip = TextTarget::symbol(&doc, rep, RunRule::Strip);
        assert_eq!(plain.component, Some(comp));
        assert_eq!(plain.page, Some(page));
        assert_eq!(text_shown(&doc, plain, TextKind::Tag), Shown::On);
        assert_eq!(text_shown(&doc, strip, TextKind::Tag), Shown::First);
        assert_eq!(text_shown(&doc, plain, TextKind::Description), Shown::Off);
        assert_eq!(text_shown(&doc, plain, TextKind::ContinuationName), Shown::Off);
        assert_eq!(text_shown(&doc, plain, TextKind::Grid), Shown::On);
        // A project-wide tag rule overrides the run rule; `first` is refused for other kinds.
        doc.properties.insert("text.tag.show".into(), "off".into());
        doc.properties.insert("text.description.show".into(), "first".into());
        assert_eq!(text_shown(&doc, strip, TextKind::Tag), Shown::Off);
        assert_eq!(text_shown(&doc, plain, TextKind::Description), Shown::Off);
        doc.pages.get_mut(&page).map(|p| p.properties.insert("text.tag.show".into(), "on".into()));
        assert_eq!(text_shown(&doc, plain, TextKind::Tag), Shown::On);
        doc.components.get_mut(&comp).map(|c| c.properties.insert("text.tag.show".into(), "off".into()));
        assert_eq!(text_shown(&doc, plain, TextKind::Tag), Shown::Off);
        doc.components.get_mut(&comp).map(|c| c.properties.insert(defaults::symbol_key("text.tag.show", rep), "on".into()));
        assert_eq!(text_shown(&doc, plain, TextKind::Tag), Shown::On);
        // The project may turn the strip run rule off.
        let mut doc2 = Document::default();
        doc2.properties.insert("text.tag.show.strip".into(), "on".into());
        assert_eq!(text_shown(&doc2, TextTarget { run: RunRule::Strip, ..TextTarget::project() }, TextKind::Tag), Shown::On);
    }
}
