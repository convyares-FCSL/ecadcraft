//! Text styles: the kinds of text ECAD draws, their resolved height,
//! weight and CADCraft style name, and whether a text is shown.
//!
//! Every value comes from the kind's text style ([`crate::catalogues::text`]): the
//! registry's `text.<kind>.style` choice says which style, and [`ecad_model::registry::styles::field`] what it
//! says. Nothing here writes a size or a weight as a literal. The readers are a few map lookups
//! and never fail: a hostile stored value counts as absent.
//!
//! This module draws nothing and stores nothing.

use ecad_model::{Document, StyleRef};

use crate::defaults::text as d;
use ecad_model::look::{OwnStyle, RunRule, Shown, TextColour, TextKind, TextStyle, TextTarget};
use ecad_model::objects::styles::text::Weight;
use ecad_model::registry::{Entry, Scope};

pub mod commands;
pub mod fonts;

/// The CADCraft text style of regular ECAD text (the project font, regular).
pub const STYLE_REGULAR: &str = "Standard";
/// The CADCraft text style of bold ECAD text (the project font, bold).
pub const STYLE_BOLD: &str = "ECAD-BOLD";
/// The style name a text entity is built with before its kind's style is applied:
/// every drawn text then takes its kind's style through `DrawApply`; symbol line art, which is
/// exempt, keeps it).
pub const PLACEHOLDER_STYLE: &str = STYLE_REGULAR;

/// A colour field's value as a [`TextColour`]: `None` for `none` (ink) and anything unknown.
pub(crate) fn text_colour(value: &str) -> Option<TextColour> {
    let t = value.trim();
    if t.eq_ignore_ascii_case("none") {
        return None;
    }
    if let Some((r, g, b)) = ecad_model::registry::styles::hex_rgb(t) {
        return Some(TextColour::Rgb(r, g, b));
    }
    ecad_electrical::cables::words::colour_code(t).map(|(first, _)| TextColour::Code(first))
}

/// The CADCraft text style of a weight.
pub(crate) trait WeightStyle {
    /// The CADCraft text style for this weight.
    fn style_name(self) -> &'static str;
}

impl WeightStyle for Weight {
    fn style_name(self) -> &'static str {
        match self {
            Weight::Regular => STYLE_REGULAR,
            Weight::Bold => STYLE_BOLD,
        }
    }
}

/// The registry entry choosing `kind`'s text style (`text.<kind>.style`).
pub fn style_entry(kind: TextKind) -> &'static Entry {
    let i = TextKind::ALL.iter().position(|k| *k == kind).unwrap_or(0);
    crate::defaults::styles::TEXT.get(i).copied().unwrap_or(&crate::defaults::styles::TAG)
}

/// The registry entry of `kind`'s visibility, for the kinds that have one; `None` for kinds
/// that are always shown.
pub fn show_entry(kind: TextKind) -> Option<&'static Entry> {
    match kind {
        TextKind::Tag => Some(&d::TAG_SHOW),
        TextKind::PointLabel => Some(&d::POINT_LABEL_SHOW),
        TextKind::Description => Some(&d::DESCRIPTION_SHOW),
        TextKind::PartNumber => Some(&d::PART_NUMBER_SHOW),
        TextKind::ContinuationName => Some(&d::CONTINUATION_NAME_SHOW),
        TextKind::Reference => Some(&d::REFERENCE_SHOW),
        TextKind::ContinuationCaption => Some(&d::CONTINUATION_CAPTION_SHOW),
        TextKind::LocationLabel => Some(&d::LOCATION_SHOW),
        TextKind::WireLabel => Some(&d::WIRE_LABEL_SHOW),
        _ => None,
    }
}

/// The style of `kind` in this project: its text style (the project's `text.<kind>.style`, else
/// the standard's, else the kind's built-in style) and that style's fields. The
/// wire label's height is `settings.wiring.label_height`. Never fails; a hostile stored
/// value gives the built-in.
///
/// # Example
///
/// ```
/// use ecad_drawing_style::text::text_style;
/// use ecad_model::look::TextKind;
/// use ecad_model::objects::styles::text::Weight;
/// let doc = ecad_model::Document::default();
/// let tag = text_style(&doc, TextKind::Tag);
/// assert_eq!((tag.height, tag.weight, tag.style_name), (2.5, Weight::Bold, "ECAD-BOLD"));
/// ```
pub fn text_style(doc: &Document, kind: TextKind) -> TextStyle {
    text_style_at(doc, kind, Scope::project())
}

/// [`text_style`] at `at`: an object's own `text.<kind>.style`
/// (S P O) chooses its style. The CADCraft style name and font stay the kind's project style's (an
/// object's style changes height, weight and colour).
pub fn text_style_at(doc: &Document, kind: TextKind, at: Scope) -> TextStyle {
    let id = style_of(doc, kind, at);
    let f = |k: &str| ecad_model::registry::styles::field_of(doc, &crate::catalogues::text::CATALOGUE, &id, k, None);
    let height = f("height").map_or_else(|| builtin_height(doc), |r| r.length(doc));
    let weight = match f("weight").map(|r| r.word()) {
        Some("bold") => Weight::Bold,
        _ => Weight::Regular,
    };
    let style_name = match kind_font(doc, kind) {
        Some(_) => kind_style_name(kind, weight),
        None => weight.style_name(),
    };
    let colour = f("colour").and_then(|r| text_colour(&r.text()));
    TextStyle { height, weight, style_name, colour }
}

/// The CADCraft text style of an own text style drawn in `family`: `ECAD-FONT-<FAMILY>` and its
/// `-BOLD` twin (letters and digits kept, the rest `-`).
pub fn family_style_name(family: &str, weight: Weight) -> String {
    let w: String = family.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_uppercase() } else { '-' }).take(48).collect();
    match weight {
        Weight::Regular => format!("ECAD-FONT-{w}"),
        Weight::Bold => format!("ECAD-FONT-{w}-BOLD"),
    }
}

/// The own text style of `kind` that the object with `props` chooses (its `text.<kind>.style`),
/// resolved at `at`; `None` when it has no valid choice of its own.
pub(crate) fn own_style(doc: &Document, kind: TextKind, at: Scope, props: &ecad_model::Properties) -> Option<OwnStyle> {
    let e = crate::text::style_entry(kind);
    props.get(e.key).and_then(|v| e.parse_in(doc, v))?;
    let style = text_style_at(doc, kind, at);
    let id = style_of(doc, kind, at);
    Some(own_with(doc, style, &id))
}

/// The text style `id` of the `text` catalogue as drawn: its height, weight and
/// colour, and the CADCraft style naming its font, as [`own_style`] gives an object's own style.
/// For a text whose style is chosen by something other than its kind (a location box's label).
pub fn own_style_of(doc: &Document, id: &StyleRef) -> OwnStyle {
    let f = |k: &str| ecad_model::registry::styles::field_of(doc, &crate::catalogues::text::CATALOGUE, id, k, None);
    let height = f("height").map_or_else(|| builtin_height(doc), |r| r.length(doc));
    let weight = match f("weight").map(|r| r.word()) {
        Some("bold") => Weight::Bold,
        _ => Weight::Regular,
    };
    let colour = f("colour").and_then(|r| text_colour(&r.text()));
    own_with(doc, TextStyle { height, weight, style_name: weight.style_name(), colour }, id)
}

/// The text catalogue's built-in cap height in drawing units (only when a field cannot be read).
fn builtin_height(doc: &Document) -> f64 {
    let mm = match crate::catalogues::text::CATALOGUE.field("height").map(|f| f.builtin()) {
        Some(ecad_model::registry::Value::Length(x)) => x,
        _ => 0.0,
    };
    ecad_model::registry::Resolved { value: ecad_model::registry::Value::Length(mm), from: ecad_model::registry::Source::Builtin }.length(doc)
}

/// [`OwnStyle`] for `style` of text style `id`: its font family when not the project font.
fn own_with(doc: &Document, style: TextStyle, id: &StyleRef) -> OwnStyle {
    let font =
        ecad_model::registry::styles::field_of(doc, &crate::catalogues::text::CATALOGUE, id, "font", None).map(|r| r.text()).unwrap_or_default();
    let project = match ecad_model::registry::resolve(doc, &d::FONT, Scope::project()).value {
        ecad_model::registry::Value::Text(t) => t.to_string(),
        _ => default_family().to_string(),
    };
    let family = (!font.is_empty() && !font.eq_ignore_ascii_case(crate::catalogues::text::PROJECT_FONT) && !font.eq_ignore_ascii_case(&project))
        .then_some(font);
    let name = match &family {
        Some(f) => family_style_name(f, style.weight),
        None => style.weight.style_name().to_string(),
    };
    OwnStyle { style, name, family }
}

/// The text style `kind` uses at `at`.
pub fn style_of(doc: &Document, kind: TextKind, at: Scope) -> StyleRef {
    ecad_model::registry::styles::chosen(doc, crate::text::style_entry(kind), at)
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
    ("ECAD-CONTINUATION-CAPTION", "ECAD-CONTINUATION-CAPTION-BOLD"),
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
    ("ECAD-CHANNEL-TEXT", "ECAD-CHANNEL-TEXT-BOLD"),
    ("ECAD-ADDRESS", "ECAD-ADDRESS-BOLD"),
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

/// A kind's own font family (its text style's `font`), when it names a family that
/// differs from the project font; `None` when the kind uses the project font (`project`, the
/// built-in).
pub(crate) fn kind_font(doc: &Document, kind: TextKind) -> Option<&str> {
    let id = style_of(doc, kind, Scope::project());
    let r = ecad_model::registry::styles::field_of(doc, &crate::catalogues::text::CATALOGUE, &id, "font", None)?;
    let ecad_model::registry::Value::Text(f) = r.value else { return None };
    if f.eq_ignore_ascii_case(crate::catalogues::text::PROJECT_FONT) {
        return None;
    }
    let project = match ecad_model::registry::resolve(doc, &d::FONT, Scope::project()).value {
        ecad_model::registry::Value::Text(t) => t,
        _ => default_family(),
    };
    (!f.eq_ignore_ascii_case(project)).then_some(f)
}

/// Whether a text of `kind` on `target` is shown: the narrowest level that has a
/// value wins (symbol, device or conductor, page, project); then, for a tag with a run rule, the
/// project's `text.tag.show.strip` / `.plc`; then the built-in. `First` is only ever returned for
/// a tag. Kinds with no visibility entry are always `On`. Never fails.
pub(crate) fn text_shown(doc: &Document, target: TextTarget, kind: TextKind) -> Shown {
    let Some(entry) = crate::text::show_entry(kind) else { return Shown::On };
    let r = ecad_model::registry::resolve(doc, entry, target.scope());
    if r.from == ecad_model::registry::Source::Builtin && kind == TextKind::Tag {
        let rule = match target.run {
            RunRule::None => None,
            RunRule::Strip => Some(&d::TAG_SHOW_STRIP),
            RunRule::Plc => Some(&d::TAG_SHOW_PLC),
        };
        if let Some(rule) = rule {
            return <Shown as ecad_model::Word>::parse(ecad_model::registry::resolve(doc, rule, Scope::project()).word()).unwrap_or_default();
        }
    }
    <Shown as ecad_model::Word>::parse(r.word()).unwrap_or_default()
}

// ---------------------------------------------------------------- Fonts

/// The family CADCraft's resolver looks for when the project's family is missing: the built-in of
/// `text.font`.
pub(crate) fn default_family() -> &'static str {
    match d::FONT.builtin() {
        ecad_model::registry::Value::Text(t) => t,
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ecad_model::Word;
    use ecad_model::registry::Value;
    use ecad_model::{Device, DeviceId, LengthUnit, Page, PageId, Representation, RepresentationId};

    /// The built-in table, written out here so a reader and its entry cannot drift.
    const TABLE: [(TextKind, f64, Weight); 23] = [
        (TextKind::Tag, 2.5, Weight::Bold),
        (TextKind::PointLabel, 1.5, Weight::Regular),
        (TextKind::Description, 1.6, Weight::Regular),
        (TextKind::PartNumber, 1.6, Weight::Regular),
        (TextKind::ContinuationName, 2.0, Weight::Regular),
        (TextKind::Reference, 2.5, Weight::Regular),
        (TextKind::ContinuationCaption, 2.5, Weight::Regular),
        (TextKind::WireLabel, 2.5, Weight::Regular),
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
        (TextKind::ChannelText, 2.5, Weight::Regular),
        (TextKind::Address, 2.0, Weight::Regular),
    ];

    #[test]
    fn an_empty_project_gets_the_built_in_of_every_kind() {
        let doc = Document::default();
        for (kind, h, w) in TABLE {
            let s = text_style(&doc, kind);
            assert_eq!(s, TextStyle { height: h, weight: w, style_name: w.style_name(), colour: None }, "{kind:?}");
            // The kind's built-in text style carries exactly these values.
            let b = crate::catalogues::text::CATALOGUE.builtin(&kind.builtin_style());
            assert_eq!(b.and_then(|b| b.value("height")), Some(Value::Length(h)), "{kind:?}");
            assert_eq!(<TextKind as ecad_model::Word>::parse(kind.word()), Some(kind));
            assert_eq!(crate::text::style_entry(kind).key, format!("text.{}.style", kind.word()));
        }
    }

    #[test]
    fn hostile_heights_give_the_default() {
        // A stored height is a number; one outside the field's range counts as absent.
        for bad in [f64::NAN, 0.1, 11.0, 1e308, -2.0, f64::INFINITY] {
            let mut doc = Document::default();
            let body = ecad_model::objects::styles::TextStyle { height: Some(ecad_model::geometry::units::Mm(bad)), ..Default::default() };
            let rec = ecad_model::StyleRecord::new("", ecad_model::objects::styles::StyleBody::Text(body));
            doc.edit_parts(|parts| {
                parts.styles.entry(ecad_model::StyleCatalogue::Text).or_default().insert(TextKind::Tag.builtin_style(), rec);
            });
            let want = TextStyle { height: 2.5, weight: Weight::Bold, style_name: STYLE_BOLD, colour: None };
            assert_eq!(text_style(&doc, TextKind::Tag), want, "{bad:?}");
        }
    }

    #[test]
    fn old_031_properties_are_ignored() {
        // The old project keys are not read.
        let mut doc = Document::default();
        doc.edit_parts(|parts| {
            parts.display.insert("text.tag.height".into(), "3".into());
        });
        doc.edit_parts(|parts| {
            parts.display.insert("text.tag.weight".into(), "regular".into());
        });
        assert_eq!(text_style(&doc, TextKind::Tag), TextStyle { height: 2.5, weight: Weight::Bold, style_name: STYLE_BOLD, colour: None });
    }

    #[test]
    fn project_values_and_inch_projects() {
        let mut doc = Document::default();
        let body = ecad_model::objects::styles::TextStyle {
            height: Some(ecad_model::geometry::units::Mm(3.0)),
            weight: Some(Weight::Regular),
            ..Default::default()
        };
        let rec = ecad_model::StyleRecord::new("", ecad_model::objects::styles::StyleBody::Text(body));
        doc.edit_parts(|parts| {
            parts.styles.entry(ecad_model::StyleCatalogue::Text).or_default().insert(TextKind::Tag.builtin_style(), rec);
        });
        assert_eq!(text_style(&doc, TextKind::Tag), TextStyle { height: 3.0, weight: Weight::Regular, style_name: STYLE_REGULAR, colour: None });
        doc.edit_parts(|parts| {
            parts.settings.length_unit = LengthUnit::Inch;
        });
        assert!((text_style(&doc, TextKind::Tag).height - 3.0 / 25.4).abs() < 1e-12);
        doc.edit_parts(|parts| {
            parts.settings.wiring.label_height = Some(2.0);
        });
        assert!((text_style(&doc, TextKind::WireLabel).height - 2.0 / 25.4).abs() < 1e-12);
    }

    #[test]
    fn shown_resolves_symbol_device_page_project_and_runs() {
        let mut doc = Document::default();
        let page = PageId(1);
        let comp = DeviceId(2);
        let rep = RepresentationId(3);
        doc.edit_parts(|parts| {
            parts.pages.insert(page, Page::titled("p").unwrap());
        });
        doc.edit_parts(|parts| {
            parts.devices.insert(comp, Device::default());
        });
        doc.edit_parts(|parts| {
            parts.representations.insert(
                rep,
                Representation {
                    subject: ecad_model::Subject::Device(comp),
                    page,
                    form: ecad_model::Form::Symbol,
                    role: ecad_model::Role::default(),
                    items: Vec::new(),
                    connection_points: Default::default(),
                },
            );
        });
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
        doc.edit_parts(|parts| {
            parts.display.insert("text.tag.show".into(), "no".into());
        });
        doc.edit_parts(|parts| {
            parts.display.insert("text.description.show".into(), "first".into());
        });
        assert_eq!(text_shown(&doc, strip, TextKind::Tag), Shown::Off);
        assert_eq!(text_shown(&doc, plain, TextKind::Description), Shown::Off);
        doc.edit_parts(|parts| {
            parts.pages.get_mut(&page).map(|p| p.display.insert("text.tag.show".into(), "yes".into()));
        });
        assert_eq!(text_shown(&doc, plain, TextKind::Tag), Shown::On);
        doc.edit_parts(|parts| {
            parts.devices.get_mut(&comp).map(|c| c.properties.insert("text.tag.show".into(), "no".into()));
        });
        assert_eq!(text_shown(&doc, plain, TextKind::Tag), Shown::Off);
        doc.edit_parts(|parts| {
            parts.devices.get_mut(&comp).map(|c| c.properties.insert(ecad_model::registry::symbol_key("text.tag.show", rep), "yes".into()));
        });
        assert_eq!(text_shown(&doc, plain, TextKind::Tag), Shown::On);
        // The project may turn the strip run rule off.
        let mut doc2 = Document::default();
        doc2.edit_parts(|parts| {
            parts.display.insert("text.tag.show.strip".into(), "yes".into());
        });
        assert_eq!(text_shown(&doc2, TextTarget { run: RunRule::Strip, ..TextTarget::default() }, TextKind::Tag), Shown::On);
    }
}
