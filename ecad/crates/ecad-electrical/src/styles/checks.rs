//! The style rows of the Checks list.
//!
//! - `style-unknown` (Info): an object's (or the project's, or the standard's) style choice names
//!   no style of its catalogue; the object is drawn with the next level's style. A cable's
//!   `cable_type` naming no cable type is one too.
//! - `style-limits` (Info): style records past the bounds, which are ignored.
//! - `defaults-damaged` (Warning): a built-in default failed to load.
//!
//! # Update map
//! The rows are worked out on open, reload or first show, and again only after an effect
//! [`touches`] names: a style record or the standard, a style choice on any object (a wire's or a
//! cable's type included, a location box's own style), or what the page type and folder
//! rows read (`crate::pages::page_types::touches_checks`: a page added, removed, renumbered, moved to
//! another folder or given another type, a folder, the page number step). Every other effect leaves them. Working them out reads the
//! style-choice index (explicit choices only), the location boxes' own styles, the project's
//! properties and the style records.

use ecad_model::document::effect::Effect;
use ecad_model::objects::styles::{MAX_FIELD_CHARS, MAX_FIELDS_PER_STYLE, MAX_STYLES_PER_CATALOGUE, StyleLayer, StyleRef, is_choice_key, is_name};
use ecad_model::{Document, ObjectRef, Properties};

use crate::checks::{Check, Fix, Kind, Object};

/// Whether `effect` can change the style rows: its own rows ([`touches_own`]) or the page type and
/// folder rows they include ([`crate::pages::page_types::touches_checks`]).
pub fn touches(effect: &Effect<'_>) -> bool {
    touches_own(effect) || crate::pages::page_types::touches_checks(effect)
}

/// Whether `effect` can change the style rows other than the page type and folder rows.
fn touches_own(effect: &Effect<'_>) -> bool {
    match effect {
        Effect::Style { .. } | Effect::Standard { .. } => true,
        // A potential's name is shown in the category-potential-twice row.
        Effect::Settings { before, after } => before.wiring.potentials != after.wiring.potentials,
        Effect::Prop { key, .. } => is_choice_key(key.raw),
        // A deleted object takes its choices with it.
        Effect::Device { before: Some(component), after: None, .. } => component.properties.keys().any(|k| is_choice_key(k)),
        Effect::Wire { before: Some(wire), after: None, .. } => wire.wire_type.is_some() || wire.display.keys().any(|k| is_choice_key(k)),
        Effect::Wire { before: Some(before), after: Some(after), .. } => before.wire_type != after.wire_type || before.display != after.display,
        // A removed page takes its choices with it; a page's type is a choice.
        Effect::Page { before: Some(page), after: None, .. } => page.page_type.is_some() || page.display.keys().any(|k| is_choice_key(k)),
        Effect::Page { before: Some(before), after: Some(after), .. } => before.page_type != after.page_type,
        // A cable's type (a dangling one is a row).
        Effect::Cable { before: Some(before), after: Some(after), .. } => before.cable_type != after.cable_type,
        Effect::Cable { before, after, .. } => before.or(*after).is_some_and(|c| c.cable_type.is_some()),
        // A location box's own style.
        Effect::Aid { before, after, .. } => {
            before.and_then(|a| a.as_box()).and_then(|b| b.style.as_ref()) != after.and_then(|a| a.as_box()).and_then(|b| b.style.as_ref())
        }
        Effect::Page { .. }
        | Effect::Layer { .. }
        | Effect::Item { .. }
        | Effect::Device { .. }
        | Effect::DeviceDefinition { .. }
        | Effect::ConnectionPoint { .. }
        | Effect::Rep { .. }
        | Effect::Wire { .. }
        | Effect::Route { .. }
        | Effect::Definition { .. }
        | Effect::Version { .. }
        | Effect::Folder { .. }
        | Effect::Project { .. }
        | Effect::LoosePart { .. }
        | Effect::DeviceLink { .. } => false,
    }
}

/// The catalogue a choice key names a style of, for the keys this check covers (the wire
/// style's `wire.category` has its own check).
fn catalogue_of(key: &str) -> Option<ecad_model::StyleCatalogue> {
    match key {
        "location.style" => Some(ecad_model::StyleCatalogue::Location),
        crate::styles::cable::CABLE_TYPE => Some(ecad_model::StyleCatalogue::Cable),
        "print.frame.style" => Some(ecad_model::StyleCatalogue::Frame),
        k if ecad_model::naming::keys::in_area(k, ecad_model::naming::keys::Area::Text) && k.ends_with(".style") => {
            Some(ecad_model::StyleCatalogue::Text)
        }
        _ => None,
    }
}

/// Whether a stored choice is dangling.
fn dangling(doc: &Document, catalogue: ecad_model::StyleCatalogue, value: &str) -> bool {
    StyleRef::parse(value).is_none_or(|id| !ecad_model::registry::styles::exists(doc, crate::styles::catalogue(catalogue), &id))
}

/// `Not found: drawn as Tag` words for a dangling choice of `key`.
fn words(doc: &Document, key: &str, value: &str, whose: &str) -> String {
    let next = crate::defaults::find(key).and_then(|e| {
        let id = ecad_model::registry::styles::chosen(doc, e, ecad_model::registry::Scope::project());
        catalogue_of(key).map(|c| ecad_model::registry::styles::name(doc, crate::styles::catalogue(c), &id))
    });
    let one = catalogue_of(key).map_or("Style", |c| crate::styles::catalogue(c).one);
    match next {
        Some(n) => format!("{whose}: {one} {value:?} not found; drawn as {n}"),
        None => format!("{whose}: {one} {value:?} not found"),
    }
}

/// The style rows as a [`crate::checks::CheckSource`]: kept on pages with checks off.
pub(crate) struct StyleRows;

impl crate::checks::CheckSource for StyleRows {
    fn key(&self) -> crate::checks::Key {
        crate::checks::Key::Styles
    }
    fn touched(&self, scope: &crate::checks::Scope) -> bool {
        scope.styles
    }
    fn kept_on_pages_off(&self) -> bool {
        true
    }
    fn checks(&self, doc: &Document) -> Vec<Check> {
        rows(doc)
    }
}

/// Every style row of `doc`.
pub fn rows(doc: &Document) -> Vec<Check> {
    let mut out = Vec::new();
    // One drawing order for the rows' go-to places: the book is built at most once.
    let mut places = ecad_model::naming::numbering::Places::new(doc);
    // A built-in default that failed to load. Never in a tested build.
    if let Some(first) = crate::defaults::data::DATA.problems().first() {
        out.push(Check {
            kind: Kind::DefaultsDamaged,
            page: None,
            at: None,
            object: Object::None,
            which: format!("Built-in defaults damaged ({first}): safe values used"),
            fix: None,
        });
    }
    // Two categories default for one potential.
    for (pot, ids) in &crate::styles::wire::WireDraws::new(doc).twice {
        let names: Vec<String> =
            ids.iter().map(|id| ecad_model::registry::styles::name(doc, crate::styles::catalogue(ecad_model::StyleCatalogue::Wire), id)).collect();
        let shown = doc.settings().wiring.potential(pot).map_or_else(|| pot.clone(), |p| p.name.clone());
        out.push(Check {
            kind: Kind::CategoryPotentialTwice,
            page: None,
            at: None,
            object: Object::None,
            which: format!("Two wire categories are the default for {shown} ({}); the first is used", names.join(", ")),
            fix: Some(Fix::OpenStandards(ecad_model::StyleCatalogue::Wire)),
        });
    }
    // A category on the BOM (a part number) with no allowance.
    for row in ecad_model::registry::styles::rows(doc, &crate::styles::wire::CATALOGUE) {
        let f = |k: &str| ecad_model::registry::styles::field_of(doc, crate::styles::catalogue(ecad_model::StyleCatalogue::Wire), &row.id, k, None);
        let has_part = f("part_number").is_some_and(|r| crate::styles::wire::filled(&r).is_some());
        if has_part && f("allowance").is_some_and(|r| r.number() <= 0.0) {
            out.push(Check {
                kind: Kind::WireAllowance,
                page: None,
                at: None,
                object: Object::None,
                which: format!("Wire length not set for {} (set an allowance in Standards)", row.name),
                fix: Some(Fix::OpenStandards(ecad_model::StyleCatalogue::Wire)),
            });
        }
    }
    // Dangling choices held by objects (through the index).
    for (value, key, o) in doc.style_choices() {
        if key == ecad_model::connectivity::wiring::WIRE_CATEGORY {
            if let ObjectRef::Wire(c) = o
                && dangling(doc, ecad_model::StyleCatalogue::Wire, value)
            {
                let page = ecad_model::naming::numbering::rep_anchor_in(&mut places, ecad_model::Subject::Wire(c)).map(|(p, _)| p);
                out.push(Check { kind: Kind::CategoryUnknown, page, at: None, object: Object::Wire(c), which: String::new(), fix: None });
            }
            continue;
        }
        // A page's own type has its own row (`crate::pages::page_types::check_rows`).
        if key == ecad_model::document::sheet::PAGE_TYPE_KEY {
            continue;
        }
        let Some(cat) = catalogue_of(key) else { continue };
        if !dangling(doc, cat, value) {
            continue;
        }
        let (object, page, whose) = match o {
            ObjectRef::Device(c) => {
                let page = crate::symbols::reps::goto_place_in(&mut places, c).map(|(p, _)| p);
                (Object::Device(c), page, crate::devices::tags::tag_words(doc, c))
            }
            ObjectRef::Wire(c) => {
                let page = ecad_model::naming::numbering::rep_anchor_in(&mut places, ecad_model::Subject::Wire(c)).map(|(p, _)| p);
                (Object::Wire(c), page, "Wire".to_string())
            }
            ObjectRef::Page(p) => (Object::None, Some(p), format!("Page {}", ecad_model::document::book::page_number(doc, p))),
            ObjectRef::Cable(k) => (Object::Cable(k), None, crate::cables::cable_title(doc, k)),
            _ => continue,
        };
        out.push(Check { kind: Kind::StyleUnknown, page, at: None, object, which: words(doc, key, value, &whose), fix: None });
    }
    // A location box's own style naming no location style (boxes are few: one pass).
    for (a, x) in doc.aids() {
        let Some(value) = x.as_box().and_then(|b| b.style.as_ref()).map(StyleRef::text) else { continue };
        if !dangling(doc, ecad_model::StyleCatalogue::Location, &value) {
            continue;
        }
        let page = doc.representations_of(ecad_model::Subject::Aid(*a)).next().map(|(_, r)| r.page);
        let which = words(doc, "location.style", &value, "A location box");
        out.push(Check { kind: Kind::StyleUnknown, page, at: None, object: Object::Aid(*a), which, fix: None });
    }
    // Dangling choices of the project and the standard.
    let level = |props: &Properties, whose: &str, out: &mut Vec<Check>| {
        for (key, value) in props.iter().filter(|(k, _)| is_choice_key(k)) {
            let Some(cat) = catalogue_of(key) else { continue };
            if dangling(doc, cat, value) {
                out.push(Check {
                    kind: Kind::StyleUnknown,
                    page: None,
                    at: None,
                    object: Object::None,
                    which: words(doc, key, value, whose),
                    fix: Some(Fix::OpenStandards(cat)),
                });
            }
        }
    };
    level(doc.display(), "Project", &mut out);
    if let Some(s) = &doc.standard() {
        level(&s.values, "Standard", &mut out);
    }
    // Bounds.
    let mut past = 0usize;
    let mut cats: Vec<ecad_model::StyleCatalogue> = Vec::new();
    for layer in [StyleLayer::Standard, StyleLayer::Project] {
        let Some(layer_cats) = ecad_model::objects::styles::layer_of(doc, layer) else { continue };
        for (cat, styles) in layer_cats {
            let mut bad = styles.len().saturating_sub(MAX_STYLES_PER_CATALOGUE);
            for r in styles.values().take(MAX_STYLES_PER_CATALOGUE) {
                let named_badly = r.name.as_ref().is_some_and(|n| !is_name(n.as_str()));
                // A record's fields are typed and bounded by their types.
                bad += usize::from(named_badly);
            }
            if bad > 0 {
                past += bad;
                if !cats.contains(cat) {
                    cats.push(*cat);
                }
            }
        }
    }
    // Page types and folder start numbers.
    out.extend(crate::pages::page_types::check_rows(doc));
    if past > 0 {
        let first = cats.first().copied().unwrap_or(ecad_model::StyleCatalogue::Text);
        out.push(Check {
            kind: Kind::StyleLimits,
            page: None,
            at: None,
            object: Object::None,
            which: format!(
                "{past} style record{} past the limits ignored ({}): at most {MAX_STYLES_PER_CATALOGUE} styles per catalogue, {MAX_FIELDS_PER_STYLE} fields, 64-character names, {MAX_FIELD_CHARS}-character values",
                if past == 1 { "" } else { "s" },
                cats.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")
            ),
            fix: Some(Fix::OpenStandards(first)),
        });
    }
    out
}
