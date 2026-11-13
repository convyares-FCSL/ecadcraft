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
    /// The CADCraft text style an entity of this kind names: [`STYLE_REGULAR`] or [`STYLE_BOLD`],
    /// or, for a kind with a font of its own (`text.<kind>.font`), `ECAD-<KIND>` /
    /// `ECAD-<KIND>-BOLD` ([`kind_style_name`]). Setting only this name on an entity is not enough
    /// for bold in the stroke-font fallback: use [`apply`].
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

    /// The registry entry of this kind's own font (owner, 2026-11-12).
    pub fn font_entry(self) -> &'static Entry {
        match self {
            TextKind::Tag => &d::TAG_FONT,
            TextKind::PointLabel => &d::POINT_LABEL_FONT,
            TextKind::Description => &d::DESCRIPTION_FONT,
            TextKind::PartNumber => &d::PART_NUMBER_FONT,
            TextKind::ContinuationName => &d::CONTINUATION_NAME_FONT,
            TextKind::Reference => &d::REFERENCE_FONT,
            TextKind::WireLabel => &d::WIRE_LABEL_FONT,
            TextKind::CableText => &d::CABLE_TEXT_FONT,
            TextKind::LocationLabel => &d::LOCATION_FONT,
            TextKind::TitleValue => &d::TITLE_VALUE_FONT,
            TextKind::TitleCaption => &d::TITLE_CAPTION_FONT,
            TextKind::TitleMain => &d::TITLE_MAIN_FONT,
            TextKind::Grid => &d::GRID_FONT,
            TextKind::Table => &d::TABLE_FONT,
            TextKind::TableHeader => &d::TABLE_HEADER_FONT,
            TextKind::Free => &d::FREE_FONT,
            TextKind::TitlePage => &d::TITLE_PAGE_FONT,
            TextKind::TitleRev => &d::TITLE_REV_FONT,
            TextKind::TableHeading => &d::TABLE_HEADING_FONT,
            TextKind::Cover => &d::COVER_FONT,
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
    let style_name = match kind_font(doc, kind) {
        Some(_) => kind_style_name(kind, weight),
        None => weight.style_name(),
    };
    TextStyle { height, weight, style_name }
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
    if let (Weight::Bold, Some(lw)) = (s.weight, bold_lineweight) {
        e.common.lineweight = cadcraft_engine::doc::Lineweight::Mm100(lw);
    }
}

/// The CADCraft text styles of kinds with a font of their own: `ECAD-TAG`, `ECAD-TAG-BOLD`…, in
/// the order of [`TextKind::ALL`].
const KIND_STYLES: [(&str, &str); TextKind::ALL.len()] = [
    ("ECAD-TAG", "ECAD-TAG-BOLD"),
    ("ECAD-POINT-LABEL", "ECAD-POINT-LABEL-BOLD"),
    ("ECAD-DESCRIPTION", "ECAD-DESCRIPTION-BOLD"),
    ("ECAD-PART-NUMBER", "ECAD-PART-NUMBER-BOLD"),
    ("ECAD-CONTINUATION-NAME", "ECAD-CONTINUATION-NAME-BOLD"),
    ("ECAD-REFERENCE", "ECAD-REFERENCE-BOLD"),
    ("ECAD-WIRE-LABEL", "ECAD-WIRE-LABEL-BOLD"),
    ("ECAD-CABLE-TEXT", "ECAD-CABLE-TEXT-BOLD"),
    ("ECAD-LOCATION", "ECAD-LOCATION-BOLD"),
    ("ECAD-TITLE-VALUE", "ECAD-TITLE-VALUE-BOLD"),
    ("ECAD-TITLE-CAPTION", "ECAD-TITLE-CAPTION-BOLD"),
    ("ECAD-TITLE-MAIN", "ECAD-TITLE-MAIN-BOLD"),
    ("ECAD-GRID", "ECAD-GRID-BOLD"),
    ("ECAD-TABLE", "ECAD-TABLE-BOLD"),
    ("ECAD-TABLE-HEADER", "ECAD-TABLE-HEADER-BOLD"),
    ("ECAD-FREE", "ECAD-FREE-BOLD"),
    ("ECAD-TITLE-PAGE", "ECAD-TITLE-PAGE-BOLD"),
    ("ECAD-TITLE-REV", "ECAD-TITLE-REV-BOLD"),
    ("ECAD-TABLE-HEADING", "ECAD-TABLE-HEADING-BOLD"),
    ("ECAD-COVER", "ECAD-COVER-BOLD"),
];

/// The style a kind with its own font names, for `weight`.
pub fn kind_style_name(kind: TextKind, weight: Weight) -> &'static str {
    let i = TextKind::ALL.iter().position(|k| *k == kind).unwrap_or(0);
    let (regular, bold) = KIND_STYLES.get(i).copied().unwrap_or((STYLE_REGULAR, STYLE_BOLD));
    match weight {
        Weight::Regular => regular,
        Weight::Bold => bold,
    }
}

/// A kind's own font family (`text.<kind>.font`), when the project sets one that differs from the
/// project font; `None` when the kind uses the project font.
pub fn kind_font(doc: &Document, kind: TextKind) -> Option<&str> {
    let r = defaults::resolve(doc, kind.font_entry(), Scope::project());
    let defaults::Value::Text(f) = r.value else { return None };
    let project = match defaults::resolve(doc, &d::FONT, Scope::project()).value {
        defaults::Value::Text(t) => t,
        _ => default_family(),
    };
    (r.from != defaults::Source::Builtin && !f.eq_ignore_ascii_case(project)).then_some(f)
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
        self.styles.get(i).copied().unwrap_or(TextStyle { height: 1.0, weight: Weight::Regular, style_name: STYLE_REGULAR })
    }

    /// Give a text entity `kind`'s style name (the project font or the kind's own) and, for bold
    /// in the stroke font, its lineweight (§2.2), as [`apply`] does from a page's resolved look.
    /// Its height is left as it is (derived texts set their own).
    pub fn apply(&self, e: &mut cadcraft_engine::doc::Entity, kind: TextKind) {
        style_entity(e, self.style(kind), self.fonts_for(kind).bold_lineweight);
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

// ---------------------------------------------------------------- Show Hidden Text (§5.7)

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

// ---------------------------------------------------------------- Justification and boxes

/// A two-letter justification code (`L`/`C`/`R` then `T`/`M`/`B`; baseline counts as `B`).
pub fn justify_code(h: cadcraft_engine::doc::HAlign, v: cadcraft_engine::doc::VAlign) -> String {
    use cadcraft_engine::doc::{HAlign, VAlign};
    let a = match h {
        HAlign::Center | HAlign::Middle => 'C',
        HAlign::Right => 'R',
        _ => 'L',
    };
    let b = match v {
        VAlign::Top => 'T',
        VAlign::Middle => 'M',
        VAlign::Bottom | VAlign::Baseline => 'B',
    };
    format!("{a}{b}")
}

/// The alignment a two-letter code names (any case); `None` for anything else.
pub fn parse_justify(code: &str) -> Option<(cadcraft_engine::doc::HAlign, cadcraft_engine::doc::VAlign)> {
    use cadcraft_engine::doc::{HAlign, VAlign};
    let c = code.trim().to_ascii_uppercase();
    let mut it = c.chars();
    let h = match it.next()? {
        'L' => HAlign::Left,
        'C' => HAlign::Center,
        'R' => HAlign::Right,
        _ => return None,
    };
    let v = match it.next()? {
        'T' => VAlign::Top,
        'M' => VAlign::Middle,
        'B' => VAlign::Bottom,
        _ => return None,
    };
    it.next().is_none().then_some((h, v))
}

/// The box of a one-line text of `width` and `height` at `at` with this justification.
pub fn text_box(
    width: f64, height: f64, at: ecad_model::Pt, h: cadcraft_engine::doc::HAlign, v: cadcraft_engine::doc::VAlign,
) -> (ecad_model::Pt, ecad_model::Pt) {
    use cadcraft_engine::doc::{HAlign, VAlign};
    let x0 = match h {
        HAlign::Center | HAlign::Middle => at.x - width / 2.0,
        HAlign::Right => at.x - width,
        _ => at.x,
    };
    let y0 = match v {
        VAlign::Top => at.y - height,
        VAlign::Middle => at.y - height / 2.0,
        VAlign::Bottom | VAlign::Baseline => at.y,
    };
    (ecad_model::Pt::new(x0, y0), ecad_model::Pt::new(x0 + width, y0 + height))
}

// ---------------------------------------------------------------- Symbol keys (§6.4, FP5)

/// The prefix of a symbol's own text keys on its component: `text.<rep>.`.
pub fn symbol_prefix(rep: RepresentationId) -> String {
    format!("text.{}.", rep.hex())
}

/// A symbol's own text keys and values on its component (`text.<rep>.*`).
pub fn symbol_text_keys(props: &ecad_model::Properties, rep: RepresentationId) -> Vec<(String, String)> {
    let prefix = symbol_prefix(rep);
    props.range(prefix.clone()..).take_while(|(k, _)| k.starts_with(&prefix)).map(|(k, v)| (k.clone(), v.clone())).collect()
}

/// Whether `key` is a symbol's own text key (`text.<32 hex digits>.…`).
pub fn is_symbol_key(key: &str) -> bool {
    key.strip_prefix("text.").and_then(|r| r.split_once('.')).is_some_and(|(id, _)| id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// The properties of a copy of symbol `old` that is now symbol `new` (FP5): every symbol key of the
/// source component is dropped, and `old`'s own keys come back under `new`.
pub fn copy_symbol_keys(props: &mut ecad_model::Properties, old: RepresentationId, new: RepresentationId) {
    let mine = symbol_text_keys(props, old);
    props.retain(|k, _| !is_symbol_key(k));
    for (k, v) in mine {
        props.insert(rename_symbol_key(&k, old, new), v);
    }
}

/// `key` of symbol `old` as the same key of symbol `new`.
pub fn rename_symbol_key(key: &str, old: RepresentationId, new: RepresentationId) -> String {
    match key.strip_prefix(&symbol_prefix(old)) {
        Some(rest) => format!("{}{rest}", symbol_prefix(new)),
        None => key.to_string(),
    }
}

// ---------------------------------------------------------------- Placement (§6)

/// Most tag blocks on a page that search for a free place; beyond it a block takes its first
/// candidate (a guard on hostile pages).
const MAX_PLACED_BLOCKS: usize = 5_000;
/// Most obstacles on a page that are looked at; beyond it blocks take their first candidate.
const MAX_OBSTACLES: usize = 50_000;
/// The side of a cell of the obstacle grid, millimetres (a performance guard, not a preference).
const CELL: f64 = 20.0;
/// Most cells one box is filed under (a guard: a huge box is filed under none and never collides).
const MAX_CELLS_PER_BOX: i64 = 400;

/// An axis-aligned box `(lo, hi)`.
pub type TextBox = (ecad_model::Pt, ecad_model::Pt);

fn finite_box(b: &TextBox) -> bool {
    b.0.is_finite() && b.1.is_finite()
}

fn overlap_area(a: &TextBox, b: &TextBox) -> f64 {
    let w = a.1.x.min(b.1.x) - a.0.x.max(b.0.x);
    let h = a.1.y.min(b.1.y) - a.0.y.max(b.0.y);
    if w > 0.0 && h > 0.0 { w * h } else { 0.0 }
}

/// Obstacles filed in a uniform grid of [`CELL`] cells.
#[derive(Default)]
struct Obstacles {
    boxes: Vec<TextBox>,
    cells: std::collections::HashMap<(i64, i64), Vec<usize>>,
}

impl Obstacles {
    fn cell_range(b: &TextBox) -> Option<(i64, i64, i64, i64)> {
        let c = |v: f64| (v / CELL).floor();
        let (x0, y0, x1, y1) = (c(b.0.x), c(b.0.y), c(b.1.x), c(b.1.y));
        if ![x0, y0, x1, y1].iter().all(|v| v.is_finite() && v.abs() < 1e12) {
            return None;
        }
        let (x0, y0, x1, y1) = (x0 as i64, y0 as i64, x1 as i64, y1 as i64);
        ((x1 - x0 + 1).saturating_mul(y1 - y0 + 1) <= MAX_CELLS_PER_BOX).then_some((x0, y0, x1, y1))
    }

    fn add(&mut self, b: TextBox) {
        if !finite_box(&b) || self.boxes.len() >= MAX_OBSTACLES {
            return;
        }
        let i = self.boxes.len();
        self.boxes.push(b);
        if let Some((x0, y0, x1, y1)) = Self::cell_range(&b) {
            for x in x0..=x1 {
                for y in y0..=y1 {
                    self.cells.entry((x, y)).or_default().push(i);
                }
            }
        }
    }

    /// The area of `b` that obstacles cover (overlaps counted per obstacle).
    fn overlap(&self, b: &TextBox) -> f64 {
        let Some((x0, y0, x1, y1)) = Self::cell_range(b) else { return 0.0 };
        let mut seen = std::collections::BTreeSet::new();
        let mut area = 0.0;
        for x in x0..=x1 {
            for y in y0..=y1 {
                for i in self.cells.get(&(x, y)).into_iter().flatten() {
                    if seen.insert(*i)
                        && let Some(o) = self.boxes.get(*i)
                    {
                        area += overlap_area(b, o);
                    }
                }
            }
        }
        area
    }
}

/// Where a symbol's tag block goes: its box and its horizontal justification (the lines are
/// stacked down from the box's top).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockPlace {
    pub rect: TextBox,
    pub halign: cadcraft_engine::doc::HAlign,
    /// Stored by Move Text (§6.4).
    pub moved: bool,
    /// No candidate was free (§6.3): the block overlaps something.
    pub overlaps: bool,
}

impl BlockPlace {
    /// The alignment point of a line whose top is at `top`: left, centre or right of the box.
    pub fn line_at(&self, top: f64) -> ecad_model::Pt {
        use cadcraft_engine::doc::HAlign;
        let x = match self.halign {
            HAlign::Right => self.rect.1.x,
            HAlign::Center | HAlign::Middle => (self.rect.0.x + self.rect.1.x) / 2.0,
            _ => self.rect.0.x,
        };
        ecad_model::Pt::new(x, top)
    }

    /// The justification code of the block (its halign and the vertical anchor `v`).
    pub fn code(&self, v: char) -> String {
        let h = match self.halign {
            cadcraft_engine::doc::HAlign::Right => 'R',
            cadcraft_engine::doc::HAlign::Center | cadcraft_engine::doc::HAlign::Middle => 'C',
            _ => 'L',
        };
        format!("{h}{v}")
    }
}

/// The box of a block `w` × `h` anchored at `at` with a two-letter justification.
pub fn block_rect(at: ecad_model::Pt, w: f64, h: f64, halign: cadcraft_engine::doc::HAlign, v: cadcraft_engine::doc::VAlign) -> TextBox {
    use cadcraft_engine::doc::{HAlign, VAlign};
    let x0 = match halign {
        HAlign::Right => at.x - w,
        HAlign::Center | HAlign::Middle => at.x - w / 2.0,
        _ => at.x,
    };
    let y1 = match v {
        VAlign::Top => at.y,
        VAlign::Middle => at.y + h / 2.0,
        VAlign::Bottom | VAlign::Baseline => at.y + h,
    };
    (ecad_model::Pt::new(x0, y1 - h), ecad_model::Pt::new(x0 + w, y1))
}

/// The candidates of design 031 §6.2 for a block `w` × `h` beside a symbol box `(lo, hi)`, in
/// order: (box, halign).
pub fn candidates(lo: ecad_model::Pt, hi: ecad_model::Pt, vertical: bool, w: f64, h: f64, gap: f64) -> [(TextBox, cadcraft_engine::doc::HAlign); 5] {
    use cadcraft_engine::doc::HAlign;
    use ecad_model::Pt;
    let cx = (lo.x + hi.x) / 2.0;
    let cy = (lo.y + hi.y) / 2.0;
    let r = |x0: f64, y0: f64| (Pt::new(x0, y0), Pt::new(x0 + w, y0 + h));
    let left_top = (r(lo.x - gap - w, hi.y - h), HAlign::Right);
    let left_mid = (r(lo.x - gap - w, cy - h / 2.0), HAlign::Right);
    let right_top = (r(hi.x + gap, hi.y - h), HAlign::Left);
    let right_mid = (r(hi.x + gap, cy - h / 2.0), HAlign::Left);
    let above = (r(cx - w / 2.0, hi.y + gap), HAlign::Center);
    let above_left = (r(lo.x, hi.y + gap), HAlign::Left);
    let below = (r(cx - w / 2.0, lo.y - gap - h), HAlign::Center);
    if vertical { [left_top, left_mid, right_top, above, below] } else { [above, above_left, below, left_mid, right_mid] }
}

/// A stored text place (`"x,y,J"`, design 031 §6.4): the point in the symbol's own frame and the
/// justification. `None` for anything that does not parse.
pub fn parse_at(v: &str) -> Option<(ecad_model::Pt, cadcraft_engine::doc::HAlign, cadcraft_engine::doc::VAlign)> {
    let mut it = v.split(',');
    let x: f64 = it.next()?.trim().parse().ok()?;
    let y: f64 = it.next()?.trim().parse().ok()?;
    let (h, v) = parse_justify(it.next()?)?;
    if it.next().is_some() || !x.is_finite() || !y.is_finite() {
        return None;
    }
    Some((ecad_model::Pt::new(x, y), h, v))
}

/// The stored form of a text place.
pub fn at_text(p: ecad_model::Pt, h: cadcraft_engine::doc::HAlign, v: cadcraft_engine::doc::VAlign) -> String {
    let r = |x: f64| (x * 1e6).round() / 1e6;
    format!("{},{},{}", r(p.x), r(p.y), justify_code(h, v))
}

/// A page point in a placement's local frame (the inverse of `Placement::apply`).
pub fn to_local(pl: &ecad_model::Placement, p: ecad_model::Pt) -> Option<ecad_model::Pt> {
    if !pl.is_valid() || !p.is_finite() {
        return None;
    }
    let (dx, dy) = (p.x - pl.origin.x, p.y - pl.origin.y);
    let (s, c) = pl.rotation.sin_cos();
    let (x, y) = ((c * dx + s * dy) / pl.scale, (-s * dx + c * dy) / pl.scale);
    let x = if pl.mirror { -x } else { x };
    let q = ecad_model::Pt::new(x, y);
    q.is_finite().then_some(q)
}

/// One tag block to place: its symbol, the symbol's box, orientation and placement, and the
/// block's size.
#[derive(Debug, Clone)]
pub struct Block {
    pub rep: RepresentationId,
    pub component: ComponentId,
    pub lo: ecad_model::Pt,
    pub hi: ecad_model::Pt,
    pub vertical: bool,
    pub placement: ecad_model::Placement,
    pub w: f64,
    pub h: f64,
}

/// The obstacles of `page` that never move (§6.3 step 1): symbol boxes, conductor routes inflated
/// by `text.wire_clearance`, and the boxes of `fixed` texts (connection point labels, location
/// labels). The frame's inside edge is checked separately ([`place_blocks`]).
fn page_obstacles(doc: &Document, page: PageId, fixed: &[TextBox]) -> Obstacles {
    let mut obs = Obstacles::default();
    let clear = defaults::resolve(doc, &d::WIRE_CLEARANCE, Scope::project()).length(doc);
    for (_, r) in doc.representations_on(page) {
        match r.subject {
            ecad_model::Subject::Component(_) if r.form == ecad_model::Form::Symbol => {
                if let Some((def, pl)) = crate::devices::rep_definition(doc, r)
                    && let Some(b) = crate::devices::placed_box(def, &pl)
                {
                    obs.add(b);
                }
            }
            ecad_model::Subject::Conductor(_) => {
                for item in r.items.iter().filter_map(|i| doc.items.get(i)) {
                    let pts: Vec<ecad_model::Pt> = match &item.kind {
                        ecad_model::ItemKind::Primitive(ecad_model::Primitive::Line { a, b }) => vec![*a, *b],
                        ecad_model::ItemKind::Primitive(ecad_model::Primitive::Polyline { vertices, .. }) => vertices.clone(),
                        _ => Vec::new(),
                    };
                    for s in pts.windows(2) {
                        if let [a, b] = s {
                            obs.add((
                                ecad_model::Pt::new(a.x.min(b.x) - clear, a.y.min(b.y) - clear),
                                ecad_model::Pt::new(a.x.max(b.x) + clear, a.y.max(b.y) + clear),
                            ));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    for b in fixed {
        obs.add(*b);
    }
    obs
}

/// Place every tag block of `page` (design 031 §6.3): moved blocks at their stored places, then
/// the others in reading order, each at its first free candidate (else the one with the least
/// overlap), never crossing the frame's inside edge when a candidate inside it exists. Each placed
/// block becomes an obstacle. Deterministic: the result does not depend on op order.
pub fn place_blocks(doc: &Document, page: PageId, blocks: &[Block], fixed: &[TextBox]) -> std::collections::BTreeMap<RepresentationId, BlockPlace> {
    let mut out = std::collections::BTreeMap::new();
    let mut obs = page_obstacles(doc, page, fixed);
    let grid = ecad_model::sheet::Grid::of(doc, page);
    let frame =
        (ecad_model::Pt::new(grid.frame_left(), grid.frame_bottom()), ecad_model::Pt::new(grid.frame_left() + grid.area_width(), grid.frame_top()));
    let inside = |b: &TextBox| !finite_box(&frame) || (b.0.x >= frame.0.x && b.0.y >= frame.0.y && b.1.x <= frame.1.x && b.1.y <= frame.1.y);
    let gap_factor = defaults::resolve(doc, &d::TAG_GAP, Scope::project()).number();
    let tag_height = text_style(doc, TextKind::Tag).height;
    let mut free: Vec<&Block> = Vec::new();
    // Moved blocks first (§6.4).
    for b in blocks {
        let stored = doc
            .components
            .get(&b.component)
            .and_then(|c| c.properties.get(&format!("{}tag.at", symbol_prefix(b.rep))))
            .and_then(|v| parse_at(v))
            .and_then(|(p, h, v)| {
                let at = b.placement.apply(p);
                at.is_finite().then(|| (block_rect(at, b.w, b.h, h, v), h))
            })
            .filter(|(r, _)| finite_box(r));
        match stored {
            Some((rect, halign)) => {
                obs.add(rect);
                out.insert(b.rep, BlockPlace { rect, halign, moved: true, overlaps: false });
            }
            None => free.push(b),
        }
    }
    // Reading order: box top (highest first), then box left, then representation ID.
    free.sort_by(|a, b| b.hi.y.total_cmp(&a.hi.y).then(a.lo.x.total_cmp(&b.lo.x)).then(a.rep.cmp(&b.rep)));
    for (i, b) in free.into_iter().enumerate() {
        let gap = gap_factor * tag_height;
        let cands = candidates(b.lo, b.hi, b.vertical, b.w, b.h, gap);
        let search = i < MAX_PLACED_BLOCKS && obs.boxes.len() < MAX_OBSTACLES;
        let mut best: Option<(f64, usize)> = None;
        if search {
            for (k, (rect, _)) in cands.iter().enumerate() {
                if !finite_box(rect) {
                    continue;
                }
                // Outside the frame counts as fully covered: used only when nothing inside is.
                let area = if inside(rect) { obs.overlap(rect) } else { f64::MAX };
                if best.is_none_or(|(a, _)| area < a) {
                    best = Some((area, k));
                }
                if area == 0.0 {
                    break;
                }
            }
        }
        let (area, k) = best.unwrap_or((0.0, 0));
        let Some((rect, halign)) = cands.get(k).copied() else { continue };
        if !finite_box(&rect) {
            continue;
        }
        obs.add(rect);
        out.insert(b.rep, BlockPlace { rect, halign, moved: false, overlaps: search && area > 0.0 });
    }
    out
}

// ---------------------------------------------------------------- Runs (§5.3)

/// Most symbols on a page for which runs are worked out; beyond it every symbol counts as the
/// first of its own run (a guard on hostile pages: the grouping is quadratic in a line's symbols).
const MAX_RUN_SYMBOLS: usize = 20_000;

/// One symbol on a page, as the run grouping sees it.
#[derive(Debug, Clone, Copy)]
struct Placed {
    rep: RepresentationId,
    component: ComponentId,
    /// The run class: the same starter kind (strip terminals, PLC channels, or the starter itself).
    class: u64,
    centre: (f64, f64),
    lo: (f64, f64),
    hi: (f64, f64),
}

/// The symbols of `page` that are the first of their run (design 031 §5.3): leftmost of a row,
/// topmost of a column, or a run of one. A run is a maximal set of symbols of one device, of one
/// starter kind, whose centres are within `text.run.align` across the line, whose neighbouring
/// boxes are at most `text.run.gap` apart along it, with no symbol of another device between
/// them. Rows are tried before columns.
pub fn run_firsts(doc: &Document, page: PageId) -> std::collections::BTreeSet<RepresentationId> {
    use std::collections::BTreeSet;
    let align = defaults::resolve(doc, &d::RUN_ALIGN, Scope::project()).length(doc);
    let gap = defaults::resolve(doc, &d::RUN_GAP, Scope::project()).length(doc);
    let mut all: Vec<Placed> = Vec::new();
    for (rid, r) in doc.representations_on(page) {
        let ecad_model::Subject::Component(c) = r.subject else { continue };
        if r.form != ecad_model::Form::Symbol {
            continue;
        }
        let Some((def, pl)) = crate::devices::rep_definition(doc, r) else { continue };
        let Some((lo, hi)) = crate::devices::placed_box(def, &pl) else { continue };
        let class = match crate::devices::run_rule(doc, r) {
            RunRule::Strip => 1,
            RunRule::Plc => 2,
            RunRule::None => {
                // Other symbols: their starter (a definition ID otherwise), so only like with like.
                let key =
                    crate::devices::rep_starter(doc, r).map_or(0, |s| s.key.bytes().fold(7u64, |h, b| h.wrapping_mul(31).wrapping_add(u64::from(b))));
                3 + key
            }
        };
        all.push(Placed { rep: rid, component: c, class, centre: ((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0), lo: (lo.x, lo.y), hi: (hi.x, hi.y) });
    }
    if all.len() > MAX_RUN_SYMBOLS {
        return all.iter().map(|p| p.rep).collect();
    }
    let mut placed_in_row: BTreeSet<RepresentationId> = BTreeSet::new();
    let mut firsts: BTreeSet<RepresentationId> = BTreeSet::new();
    // Groups: one device, one class.
    let mut groups: std::collections::BTreeMap<(ComponentId, u64), Vec<Placed>> = std::collections::BTreeMap::new();
    for p in &all {
        groups.entry((p.component, p.class)).or_default().push(*p);
    }
    for axis in [Axis::Row, Axis::Column] {
        for members in groups.values() {
            let free: Vec<Placed> = members.iter().copied().filter(|p| !placed_in_row.contains(&p.rep)).collect();
            for run in runs_along(&free, &all, axis, align, gap) {
                if run.len() < 2 {
                    continue;
                }
                if let Some(first) = run.first() {
                    firsts.insert(first.rep);
                }
                placed_in_row.extend(run.iter().map(|p| p.rep));
            }
        }
    }
    // Runs of one.
    for p in &all {
        if !placed_in_row.contains(&p.rep) {
            firsts.insert(p.rep);
        }
    }
    firsts
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Axis {
    /// A horizontal row: same y; first is the leftmost.
    Row,
    /// A vertical column: same x; first is the topmost.
    Column,
}

/// The runs of `members` (one device, one class) along `axis`, each in order from its first.
fn runs_along(members: &[Placed], all: &[Placed], axis: Axis, align: f64, gap: f64) -> Vec<Vec<Placed>> {
    // (across, along) coordinates; along grows from the first of a run.
    let key = |p: &Placed| match axis {
        Axis::Row => (p.centre.1, p.centre.0),
        Axis::Column => (p.centre.0, -p.centre.1),
    };
    // The box's extent along the line, in the same direction.
    let span = |p: &Placed| match axis {
        Axis::Row => (p.lo.0, p.hi.0),
        Axis::Column => (-p.hi.1, -p.lo.1),
    };
    let mut sorted: Vec<Placed> = members.to_vec();
    sorted.sort_by(|a, b| key(a).0.total_cmp(&key(b).0).then(key(a).1.total_cmp(&key(b).1)));
    // Lines: consecutive (by across) members within `align` of the line's first.
    let mut lines: Vec<Vec<Placed>> = Vec::new();
    for p in sorted {
        match lines.last_mut() {
            Some(line) if line.first().is_some_and(|f| (key(&p).0 - key(f).0).abs() <= align) => line.push(p),
            _ => lines.push(vec![p]),
        }
    }
    let mut out = Vec::new();
    for mut line in lines {
        line.sort_by(|a, b| key(a).1.total_cmp(&key(b).1));
        let across = line.first().map_or(0.0, |f| key(f).0);
        let Some(component) = line.first().map(|f| f.component) else { continue };
        // Other devices' symbols on this line, by their position along it.
        let mut others: Vec<f64> = all.iter().filter(|o| o.component != component && (key(o).0 - across).abs() <= align).map(|o| key(o).1).collect();
        others.sort_by(f64::total_cmp);
        let mut run: Vec<Placed> = Vec::new();
        for p in line {
            let joins = run.last().is_some_and(|prev| {
                let open = span(&p).0 - span(prev).1;
                let (a, b) = (key(prev).1, key(&p).1);
                let between = others.partition_point(|x| *x <= a) < others.partition_point(|x| *x < b);
                open <= gap && !between
            });
            if !joins && !run.is_empty() {
                out.push(std::mem::take(&mut run));
            }
            run.push(p);
        }
        if !run.is_empty() {
            out.push(run);
        }
    }
    out
}

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
