//! The electrical rules' defaults: their registry entries, one file per area, and their built-in
//! data ([`data`]).
//!
//! Each area owns one file and its own `ENTRIES` list; [`data::LISTS`] chains them. The registry's
//! mechanism (entries, kinds, levels, resolution) is [`ecad_model::registry`]: read a value with
//! [`ecad_model::registry::resolve`] or a typed reader over it, never by writing the number where
//! it is used. The read-only `defaults` command and Settings ▸ Standards list the entries;
//! `setdefault` writes them at a level.

use ecad_model::Document;
use ecad_model::registry::{Entry, Kind, Level, Scope, resolve, wire_value};

pub mod bom;
pub mod connectors;
pub mod continuations;
pub mod data;
pub mod pages;
pub mod plc;
pub mod print;
pub mod rating;
pub mod settings;
pub mod strip_pages;
pub mod styles;
pub mod symbols;
pub mod terminal_blocks;
pub mod text;
pub mod wires;

/// The electrical rules' entry with this key (the whole registry's is
/// [`ecad_model::registry::data::Registry::find`]).
pub fn find(key: &str) -> Option<&'static Entry> {
    data::DATA.entries().find(|e| e.key == key)
}

/// What [`users`] counts for `entry`: `"devices"`, `"wires"` or `"pages"`.
pub fn users_noun(entry: &Entry) -> &'static str {
    let area = entry.key.split('.').next().unwrap_or_default();
    // A page style is counted by page, though its key is in the `connection_point` area.
    if entry.key == strip_pages::STYLE.key {
        return "pages";
    }
    match area {
        "text" | "symbol" | "terminal_block" | "tags" if entry.key != text::WIRE_LABEL_HEIGHT.key => "devices",
        "wire" | "wiring" | "text" => "wires",
        _ => "pages",
    }
}

/// How many objects use the project's value of `entry`:
/// for a tag prefix, the devices whose tag starts with the prefix the project resolves to; for
/// another entry counted by device or wire ([`users_noun`]), the devices or conductors with no
/// value of their own; otherwise the pages. Read only; one pass over the project's devices,
/// conductors or pages (asked for one entry at a time, never per frame).
pub fn users(doc: &Document, entry: &Entry) -> usize {
    if entry.key.ends_with(".prefix") && matches!(entry.kind, Kind::Parsed { .. }) {
        let px = resolve(doc, entry, Scope::project()).text();
        let px = px.trim().trim_start_matches('-').to_ascii_uppercase();
        return doc
            .devices()
            .values()
            .filter_map(|c| c.tag())
            .filter(|t| {
                let t = t.trim_start_matches('-').to_ascii_uppercase();
                t.strip_prefix(px.as_str()).is_some_and(|rest| rest.chars().next().is_some_and(|c| c.is_ascii_digit() || c == '?'))
            })
            .count();
    }
    // A style choice: the users of the style the project resolves to.
    if let Kind::Style(cat) = entry.kind
        && let Some(id) = ecad_model::StyleRef::parse(&resolve(doc, entry, Scope::project()).text())
    {
        let c = crate::styles::users(doc, cat, &id);
        return c.explicit + c.implicit;
    }
    let own = |props: &ecad_model::Properties| entry.allows(Level::Object) && props.get(entry.key).is_some_and(|v| entry.parse(v.as_str()).is_some());
    match users_noun(entry) {
        "devices" => doc.devices().values().filter(|c| !own(&c.properties)).count(),
        "wires" => doc.wires().values().filter(|c| !(entry.allows(Level::Object) && wire_value(doc, entry, c).is_some())).count(),
        _ => doc.pages().len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ecad_model::registry::{AREAS, Resolved, Source, Value, symbol_key};
    use ecad_model::{Device, Page};
    use ecad_model::{DeviceId, PageId, RepresentationId};

    fn key_ok(key: &str) -> bool {
        let parts: Vec<&str> = key.split('.').collect();
        parts.len() >= 2
            && AREAS.contains(&parts[0])
            && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'))
    }

    #[test]
    fn keys_are_unique_and_well_formed() {
        let mut seen = std::collections::BTreeSet::new();
        for e in data::DATA.entries() {
            assert!(seen.insert(e.key), "duplicate key {}", e.key);
            assert!(key_ok(e.key), "bad key {}", e.key);
            assert!(!e.label.is_empty() && !e.group.is_empty(), "{} lacks words", e.key);
        }
    }

    #[test]
    fn every_builtin_is_valid_for_its_kind() {
        // Levels are per entry (an entry may declare fewer); only the built-in is always there.
        for e in data::DATA.entries() {
            assert!(e.accepts(&e.builtin()), "{}: built-in {:?} is not a valid {:?}", e.key, e.builtin(), e.kind);
            // The built-in, written as text, parses back to itself.
            let r = Resolved { value: e.builtin(), from: Source::Builtin };
            assert_eq!(e.parse(&r.text()), Some(e.builtin()), "{}", e.key);
        }
    }

    #[test]
    fn hostile_stored_values_fall_through_to_the_next_level() {
        let tag = &text::WIRE_LABEL_HEIGHT;
        let font = &text::FONT;
        let show = &text::TAG_SHOW;
        let long = "x".repeat(10_000);
        for bad in ["NaN", "nan", "inf", "-inf", "1e308", "0.1", "11", "abc", "", "  ", "2.5mm", long.as_str()] {
            assert_eq!(tag.parse(bad), None, "height {bad:?}");
            let mut doc = Document::default();
            doc.edit_parts(|parts| {
                parts.display.insert(tag.key.into(), bad.into());
            });
            assert_eq!(resolve(&doc, tag, Scope::project()), Resolved { value: tag.builtin(), from: Source::Builtin });
        }
        for bad in ["", "a\u{7}b", "line\nbreak", &"f".repeat(65), long.as_str()] {
            assert_eq!(font.parse(bad), None, "font {bad:?}");
        }
        for bad in ["maybe", "first ", "1", ""] {
            if bad == "first " {
                // trimmed: a valid word
                assert_eq!(show.parse(bad), Some(Value::Choice("first")));
            } else {
                assert_eq!(show.parse(bad), None, "show {bad:?}");
            }
        }
        // A hostile page value under a good project value: the project wins.
        let mut doc = Document::default();
        let page = PageId(7);
        doc.edit_parts(|parts| {
            parts.pages.insert(page, Page::titled("p").unwrap());
        });
        doc.edit_parts(|parts| {
            parts.display.insert(show.key.into(), "no".into());
        });
        doc.edit_parts(|parts| {
            if let Some(p) = parts.pages.get_mut(&page) {
                p.display.insert(show.key.into(), "sideways".into());
            }
        });
        assert_eq!(resolve(&doc, show, Scope::page(page)), Resolved { value: Value::Choice("no"), from: Source::Project });
    }

    #[test]
    fn the_narrowest_allowed_level_wins() {
        let show = &text::TAG_SHOW;
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
        let at = Scope { page: Some(page), component: Some(comp), rep: Some(rep), conductor: None };
        assert_eq!(resolve(&doc, show, at).from, Source::Builtin);
        doc.edit_parts(|parts| {
            parts.display.insert(show.key.into(), "no".into());
        });
        assert_eq!(resolve(&doc, show, at), Resolved { value: Value::Choice("no"), from: Source::Project });
        doc.edit_parts(|parts| {
            parts.pages.get_mut(&page).map(|p| p.display.insert(show.key.into(), "yes".into()));
        });
        assert_eq!(resolve(&doc, show, at), Resolved { value: Value::Choice("yes"), from: Source::Page });
        doc.edit_parts(|parts| {
            parts.devices.get_mut(&comp).map(|c| c.properties.insert(show.key.into(), "no".into()));
        });
        assert_eq!(resolve(&doc, show, at), Resolved { value: Value::Choice("no"), from: Source::Object });
        doc.edit_parts(|parts| {
            parts.devices.get_mut(&comp).map(|c| c.properties.insert(symbol_key(show.key, rep), "first".into()));
        });
        assert_eq!(resolve(&doc, show, at), Resolved { value: Value::Choice("first"), from: Source::Symbol });
        // The project-only scope still sees only the project.
        assert_eq!(resolve(&doc, show, Scope::project()).from, Source::Project);
        // An entry that does not allow the page level ignores a page value.
        let h = &text::RUN_ALIGN;
        doc.edit_parts(|parts| {
            parts.pages.get_mut(&page).map(|p| p.display.insert(h.key.into(), "4".into()));
        });
        assert_eq!(resolve(&doc, h, at).from, Source::Builtin);
    }

    #[test]
    fn symbol_keys_put_the_representation_after_the_area() {
        let rep = RepresentationId(0xab);
        assert_eq!(symbol_key("text.tag.show", rep), format!("text.{:032x}.tag.show", 0xab));
    }

    #[test]
    fn settings_sources_resolve_as_the_project_level() {
        let e = &text::WIRE_LABEL_HEIGHT;
        let mut doc = Document::default();
        assert_eq!(resolve(&doc, e, Scope::project()).from, Source::Builtin);
        doc.edit_parts(|parts| {
            parts.settings.wiring.label_height = Some(2.0);
        });
        assert_eq!(resolve(&doc, e, Scope::project()), Resolved { value: Value::Length(2.0), from: Source::Project });
        doc.edit_parts(|parts| {
            parts.settings.wiring.label_height = Some(f64::NAN);
        });
        assert_eq!(resolve(&doc, e, Scope::project()).from, Source::Builtin);
    }
}

#[cfg(test)]
mod kind_tests {
    use super::*;
    use ecad_model::registry::data::Slot;
    use ecad_model::registry::{ProjectSource, Resolved, Source, Value};
    use ecad_model::{PageId, ProjectSettings};

    /// Each choice declared as a word enum keeps the words, in the order, of the
    /// list it replaced (the first word is the entry's fallback).
    #[test]
    fn choice_enums_keep_their_words() {
        let table: &[(&str, &[&str])] = &[
            ("connector.maker.halves", &["socket", "plug", "both"]),
            ("connector.maker.family", &["plain", "dsub", "m12", "rj45", "usba", "usbb"]),
            ("connector.maker.names", &["numbers", "letters", "list"]),
            ("connector.maker.layout", &["row", "pins"]),
            ("plc.slot.first", &["0", "1"]),
            ("plc.channel.first", &["0", "1"]),
            ("plc.pins", &["role", "channel"]),
            ("plc.address.style", &["byte.bit", "iec", "percent", "slot", "none"]),
            ("plc.text.channel.rotation", &["90", "0"]),
            ("plc.safe.dq", &["off", "on", "hold"]),
            ("print.continuation_numbers", &["sequence"]),
            ("print.missing_mark", &["boxed"]),
            ("print.contact_table_fallback", &["right_then_lift"]),
            (crate::releases::revisions::PENDING_KEY, &["star", "draft", "off"]),
            ("print.colour", &["mono", "colour"]),
            ("rating.system", &["ac1", "ac3", "dc"]),
            ("terminal_block.plan.style", &["table", "graphic"]),
            ("bom.lines", &["flat", "nested"]),
        ];
        for (key, want) in table {
            let e = find(key);
            assert!(matches!(e.map(|e| e.kind), Some(Kind::Choice(w)) if w == *want), "{key}");
        }
        assert_eq!(<crate::styles::frame::BlockAt as ecad_model::Word>::WORDS, ["bottom_full", "bottom_right"]);
        assert_eq!(<ecad_model::objects::styles::text::Weight as ecad_model::Word>::WORDS, ["regular", "bold"]);
        assert_eq!(<crate::styles::wire::LineType as ecad_model::Word>::WORDS, ["continuous", "dashed", "dashdot"]);
        assert_eq!(<crate::styles::wire::RouteLabels as ecad_model::Word>::WORDS, ["project", "net", "each", "none"]);
        assert_eq!(<ecad_model::objects::styles::wire::ConductorClass as ecad_model::Word>::WORDS, ["1", "2", "5", "6"]);
        assert_eq!(<ecad_model::objects::styles::wire::Material as ecad_model::Word>::WORDS, ["cu"]);
    }

    fn prefix(s: &str) -> bool {
        s.chars().all(|c| c.is_ascii_uppercase())
    }

    static PARSED: Entry = Entry {
        key: "tags.test.prefix",
        label: "Test prefix",
        group: "Test",

        kind: Kind::Parsed { max_chars: 3, parse: prefix },
        slot: Slot::new(&crate::defaults::data::DATA),
        levels: &[Level::Project],
        project: ProjectSource::Property,
    };

    fn rule(s: &ProjectSettings) -> Option<Value<'_>> {
        (s.tag_padding == Some(3)).then_some(Value::Choice("each"))
    }

    static CHOICE: Entry = Entry {
        key: "wiring.test.rule",
        label: "Test rule",
        group: "Test",

        kind: Kind::Choice(&["net", "each"]),
        slot: Slot::new(&crate::defaults::data::DATA),
        levels: &[Level::Standard, Level::Project],
        project: ProjectSource::Settings(rule),
    };

    /// An entry with no project level ignores a project value.
    static PAGE_ONLY: Entry = Entry {
        key: "print.test.page",
        label: "Test page only",
        group: "Test",

        kind: Kind::Length { min: 0.0, max: 10.0 },
        slot: Slot::new(&crate::defaults::data::DATA),
        levels: &[Level::Page],
        project: ProjectSource::Property,
    };

    #[test]
    fn a_parsed_text_kind_checks_its_parser() {
        assert_eq!(PARSED.parse("KA"), Some(Value::Text("KA")));
        assert_eq!(PARSED.parse("ka"), None);
        assert_eq!(PARSED.parse("KAAA"), None);
        let mut doc = Document::default();
        doc.edit_parts(|parts| {
            parts.display.insert(PARSED.key.into(), "x1".into());
        });
        assert_eq!(resolve(&doc, &PARSED, Scope::project()).from, Source::Builtin);
    }

    #[test]
    fn a_word_valued_setting_is_the_project_level() {
        let mut doc = Document::default();
        assert_eq!(resolve(&doc, &CHOICE, Scope::project()), Resolved { value: Value::Choice("net"), from: Source::Builtin });
        // A property of the same key is never a second project path.
        doc.edit_parts(|parts| {
            parts.display.insert(CHOICE.key.into(), "each".into());
        });
        assert_eq!(resolve(&doc, &CHOICE, Scope::project()).from, Source::Builtin);
        doc.edit_parts(|parts| {
            parts.settings.tag_padding = Some(3);
        });
        assert_eq!(resolve(&doc, &CHOICE, Scope::project()), Resolved { value: Value::Choice("each"), from: Source::Project });
    }

    #[test]
    fn an_entry_may_declare_fewer_levels() {
        let mut doc = Document::default();
        doc.edit_parts(|parts| {
            parts.display.insert(PAGE_ONLY.key.into(), "5".into());
        });
        assert_eq!(resolve(&doc, &PAGE_ONLY, Scope::project()).from, Source::Builtin);
        let page = PageId(9);
        doc.edit_parts(|parts| {
            parts.pages.insert(page, ecad_model::Page::titled("p").unwrap());
        });
        doc.edit_parts(|parts| {
            parts.pages.get_mut(&page).map(|p| p.display.insert(PAGE_ONLY.key.into(), "5".into()));
        });
        assert_eq!(resolve(&doc, &PAGE_ONLY, Scope::page(page)), Resolved { value: Value::Length(5.0), from: Source::Page });
    }

    #[test]
    fn an_entry_with_no_data_takes_its_safe_value() {
        // Not in any file: the kind's safe value, never a crash.
        assert_eq!(CHOICE.builtin(), Value::Choice("net"));
        assert_eq!(PAGE_ONLY.builtin(), Value::Length(0.0));
    }
}
