//! The style rows of the Checks list (brief 050a, design 050 §3.2, §3.4, §5.7).
//!
//! - `style-unknown` (Info): an object's (or the project's, or the standard's) style choice names
//!   no style of its catalogue; the object is drawn with the next level's style.
//! - `ignored-old-setting` (Info, overseer ruling on FP4): one row per kind of 031 key
//!   (`text.<kind>.height`, `.weight`, `.font`) still in the project's properties.
//! - `style-limits` (Info): style records past the bounds, which are ignored.
//!
//! # Update map
//! The rows are worked out on open, reload or first show, and again only after an op
//! [`touches`] names: a style record or the standard, a style choice on any object, or a
//! project property of the old kinds. Every other op leaves them. Working them out reads the
//! style-choice index (explicit choices only), the project's properties and the style records.

use ecad_model::style::{MAX_FIELD_CHARS, MAX_FIELDS_PER_STYLE, MAX_STYLES_PER_CATALOGUE, StyleLayer, StyleRef, is_choice_key, is_name};
use ecad_model::{Document, ObjectRef, Op, Properties};

use crate::checks::{Check, Fix, Kind, Object};

/// The kinds of old 031 key (FP4): the field word, and the row's words.
const OLD_KEYS: [(&str, &str); 3] = [
    ("height", "Text heights set in an older build, now ignored: set them in Standards"),
    ("weight", "Text weights set in an older build, now ignored: set them in Standards"),
    ("font", "Text fonts set in an older build, now ignored: set them in Standards"),
];

/// Whether project property `key` is one of 031's old text keys of field `field`
/// (`text.<kind>.<field>` for a kind word).
pub fn is_old_text_key(key: &str, field: &str) -> bool {
    key.strip_prefix("text.")
        .and_then(|r| r.strip_suffix(field))
        .and_then(|r| r.strip_suffix('.'))
        .is_some_and(|kind| crate::text_style::TextKind::from_word(kind).is_some())
}

/// Whether `key` is any of 031's old text keys.
pub fn is_old_key(key: &str) -> bool {
    OLD_KEYS.iter().any(|(f, _)| is_old_text_key(key, f))
}

/// Whether `op` can change the style rows.
pub fn touches(op: &Op) -> bool {
    match op {
        Op::SetStyle { .. } | Op::SetStandard { .. } => true,
        // 050b: a potentials table written again clears its older colours.
        Op::SetProjectSettings { before, after } => before.wiring.potentials != after.wiring.potentials,
        Op::SetProperty { object: ObjectRef::Project, key, .. } => is_choice_key(key) || is_old_key(key),
        Op::SetProperty { key, .. } => is_choice_key(key),
        // A deleted object takes its choices with it.
        Op::DeleteComponent { component, .. } => component.properties.keys().any(|k| is_choice_key(k)),
        Op::DeleteConductor { conductor, .. } => conductor.properties.keys().any(|k| is_choice_key(k)),
        Op::RemovePage { page, .. } => page.properties.keys().any(|k| is_choice_key(k)),
        _ => false,
    }
}

/// The catalogue a choice key names a style of, for the keys this check covers (050b's
/// `wire.category` has its own check).
fn catalogue_of(key: &str) -> Option<&'static str> {
    match key {
        "location.style" => Some("location"),
        "print.frame.style" => Some("frame"),
        k if k.starts_with("text.") && k.ends_with(".style") => Some("text"),
        _ => None,
    }
}

/// Whether a stored choice is dangling.
fn dangling(doc: &Document, catalogue: &str, value: &str) -> bool {
    StyleRef::parse(value).is_none_or(|id| !crate::styles::exists(doc, catalogue, &id))
}

/// `Not found: drawn as Tag` words for a dangling choice of `key`.
fn words(doc: &Document, key: &str, value: &str, whose: &str) -> String {
    let next = crate::defaults::find(key).map(|e| {
        let id = crate::styles::chosen(doc, e, crate::defaults::Scope::project());
        crate::styles::name(doc, catalogue_of(key).unwrap_or_default(), &id)
    });
    let one = catalogue_of(key).and_then(crate::styles::catalogue).map_or("Style", |c| c.one);
    match next {
        Some(n) => format!("{whose}: {one} {value:?} not found; drawn as {n}"),
        None => format!("{whose}: {one} {value:?} not found"),
    }
}

/// Every style row of `doc`.
pub fn rows(doc: &Document) -> Vec<Check> {
    let mut out = Vec::new();
    // Old 031 keys: one row per kind found.
    for (field, text) in OLD_KEYS {
        if doc.properties.keys().any(|k| is_old_text_key(k, field)) {
            out.push(Check {
                kind: Kind::IgnoredOldSetting,
                page: None,
                at: None,
                object: Object::None,
                which: text.into(),
                fix: Some(Fix::OpenStandards("text")),
            });
        }
    }
    // 050b (FP6 ruling): potential colours or cross-sections an older build stored.
    if doc.settings.wiring.potentials.iter().any(|p| p.legacy_colour.is_some() || p.legacy_cross_section.is_some()) {
        out.push(Check {
            kind: Kind::IgnoredOldSetting,
            page: None,
            at: None,
            object: Object::None,
            which: "Potential colours and cross-sections set in an older build, now ignored: set them in Standards ▸ Wire categories".into(),
            fix: Some(Fix::OpenStandards("wire")),
        });
    }
    // 050b: two categories default for one potential.
    for (pot, ids) in &super::wire::WireLooks::new(doc).twice {
        let names: Vec<String> = ids.iter().map(|id| super::name(doc, "wire", id)).collect();
        let shown = doc.settings.wiring.potential(pot).map_or_else(|| pot.clone(), |p| p.name.clone());
        out.push(Check {
            kind: Kind::CategoryPotentialTwice,
            page: None,
            at: None,
            object: Object::None,
            which: format!("Two wire categories are the default for {shown} ({}); the first is used", names.join(", ")),
            fix: Some(Fix::OpenStandards("wire")),
        });
    }
    // 050b: a category on the BOM (a part number) with no allowance.
    for row in super::rows(doc, &super::wire::CATALOGUE) {
        let f = |k: &str| super::field_of(doc, "wire", &row.id, k, None);
        let has_part = f("part_number").is_some_and(|r| super::wire::filled(&r).is_some());
        if has_part && f("allowance").is_some_and(|r| r.number() <= 0.0) {
            out.push(Check {
                kind: Kind::WireAllowance,
                page: None,
                at: None,
                object: Object::None,
                which: format!("Wire length not set for {} (set an allowance in Standards)", row.name),
                fix: Some(Fix::OpenStandards("wire")),
            });
        }
    }
    // Dangling choices held by objects (through the index).
    for (value, key, o) in doc.style_choices() {
        if key == ecad_model::wiring::WIRE_CATEGORY {
            if let ObjectRef::Conductor(c) = o
                && dangling(doc, "wire", value)
            {
                let page = ecad_model::numbering::rep_anchor(doc, ecad_model::Subject::Conductor(c)).map(|(p, _)| p);
                out.push(Check { kind: Kind::CategoryUnknown, page, at: None, object: Object::Conductor(c), which: String::new(), fix: None });
            }
            continue;
        }
        let Some(cat) = catalogue_of(key) else { continue };
        if !dangling(doc, cat, value) {
            continue;
        }
        let (object, page, whose) = match o {
            ObjectRef::Component(c) => {
                let page = crate::reps::goto_place(doc, c).map(|(p, _)| p);
                (Object::Component(c), page, crate::devices::tag_words(doc, c))
            }
            ObjectRef::Conductor(c) => {
                let page = ecad_model::numbering::rep_anchor(doc, ecad_model::Subject::Conductor(c)).map(|(p, _)| p);
                (Object::Conductor(c), page, "Wire".to_string())
            }
            ObjectRef::Page(p) => (Object::None, Some(p), format!("Page {}", ecad_model::book::page_number(doc, p))),
            _ => continue,
        };
        out.push(Check { kind: Kind::StyleUnknown, page, at: None, object, which: words(doc, key, value, &whose), fix: None });
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
    level(&doc.properties, "Project", &mut out);
    if let Some(s) = &doc.standard {
        level(&s.values, "Standard", &mut out);
    }
    // Bounds (design 050 §3.2).
    let mut past = 0usize;
    let mut cats: Vec<&str> = Vec::new();
    for layer in [StyleLayer::Standard, StyleLayer::Project] {
        let Some(layer_cats) = ecad_model::style::layer_of(doc, layer) else { continue };
        for (cat, styles) in layer_cats {
            let mut bad = styles.len().saturating_sub(MAX_STYLES_PER_CATALOGUE);
            for r in styles.values().take(MAX_STYLES_PER_CATALOGUE) {
                let named_badly = !r.name.is_empty() && !is_name(&r.name);
                let fields_badly = r.fields.len() > MAX_FIELDS_PER_STYLE || r.fields.values().any(|v| v.chars().count() > MAX_FIELD_CHARS);
                bad += usize::from(named_badly || fields_badly);
            }
            if bad > 0 {
                past += bad;
                if !cats.contains(&cat.as_str()) {
                    cats.push(cat.as_str());
                }
            }
        }
    }
    if past > 0 {
        let first = cats.first().and_then(|c| crate::styles::catalogue(c)).map_or("text", |c| c.word);
        out.push(Check {
            kind: Kind::StyleLimits,
            page: None,
            at: None,
            object: Object::None,
            which: format!(
                "{past} style record{} past the limits ignored ({}): at most {MAX_STYLES_PER_CATALOGUE} styles per catalogue, {MAX_FIELDS_PER_STYLE} fields, 64-character names, {MAX_FIELD_CHARS}-character values",
                if past == 1 { "" } else { "s" },
                cats.join(", ")
            ),
            fix: Some(Fix::OpenStandards(first)),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_keys_are_031_kind_fields_only() {
        assert!(is_old_key("text.tag.height"));
        assert!(is_old_key("text.wire_label.weight"));
        assert!(is_old_key("text.cover.font"));
        assert!(!is_old_key("text.font"));
        assert!(!is_old_key("text.tag.style"));
        assert!(!is_old_key("text.tag.show"));
        assert!(!is_old_key("text.nothing.height"));
    }
}
