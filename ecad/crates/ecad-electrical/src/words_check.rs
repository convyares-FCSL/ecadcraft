//! The words check run once when a project is opened (design 072 §9.4, F11).
//!
//! A typed field with an unknown word already fails its record's decode. This check finds the
//! words inside values that stay text: a stored value of a defaults choice that is not one of its
//! words, a defaults switch ([`Kind::Flag`]) or a yes/no key ([`keys::ValueSpec::Flag`]) holding anything but `yes` or `no`, a
//! `tag_prefixes` key that is not a built-in symbol, and a property that became a struct field
//! ([`ecad_model::keys::RESERVED`]). Typed input may say `true`, `1` or `on`; a file holds only the
//! words the program writes. Anything found opens the project read-only, naming the
//! word, the setting and where it is; the file is never changed.
//!
//! It reads the whole document, so it runs on open (and on reload) only, never per commit. It
//! changes nothing and excludes values that are not words: an out-of-range length or a malformed
//! style ID still reads as absent (design 066).

use ecad_model::strict::{UnknownWord, WordPlace};
use ecad_model::{Document, Properties, RepresentationId};
use ecad_model::{keys, words};

use crate::defaults::{self, Entry, Kind, Level, ProjectSource};

/// Every unknown word in `doc`, in a stable order (by place, then setting).
///
/// # Example
/// ```
/// use ecad_model::Document;
/// let mut doc = Document::default();
/// assert!(ecad_electrical::words_check::check(&doc).is_empty());
/// doc.properties.insert("text.tag.show".into(), "maybe".into());
/// let found = ecad_electrical::words_check::check(&doc);
/// assert_eq!(found.len(), 1);
/// assert!(found[0].to_string().contains("`maybe`"));
/// ```
pub fn check(doc: &Document) -> Vec<UnknownWord> {
    let mut out = Vec::new();
    bag(&doc.properties, WordPlace::Project, Level::Project, &mut out);
    if let Some(std) = &doc.standard {
        bag(&std.values, WordPlace::Standard, Level::Standard, &mut out);
    }
    for (id, p) in &doc.pages {
        bag(&p.display, WordPlace::Page(*id), Level::Page, &mut out);
    }
    for (id, c) in &doc.devices {
        bag(&c.properties, WordPlace::Device(*id), Level::Object, &mut out);
        old_kind(&c.properties, WordPlace::Device(*id), &mut out);
    }
    for (id, c) in &doc.wires {
        bag(&c.display, WordPlace::Wire(*id), Level::Object, &mut out);
    }
    for (id, c) in &doc.cables {
        reserved(&c.properties, WordPlace::Cable(*id), &mut out);
        flags(&c.properties, WordPlace::Cable(*id), &mut out);
    }
    // Design 072 F5: a starter's prefix names a starter of the catalogue (the key's form is read
    // strictly with the file); a project symbol's prefix names a project symbol the project holds.
    for key in doc.settings.tag_prefixes.keys() {
        let known = match key {
            ecad_model::prefix::PrefixOf::Starter(w) => crate::symbols::by_key(w).is_some(),
            ecad_model::prefix::PrefixOf::Project(k) => crate::symbols::has_project_key(doc, k),
        };
        if !known {
            out.push(UnknownWord::Value { place: WordPlace::Settings, setting: "tag_prefixes".into(), word: key.to_string(), allowed: Vec::new() });
        }
    }
    out.sort();
    out
}

/// The unknown words of one property bag held at `level`.
fn bag(props: &Properties, place: WordPlace, level: Level, out: &mut Vec<UnknownWord>) {
    reserved(props, place, out);
    flags(props, place, out);
    for (key, value) in props {
        let Some((entry, level)) = entry_of(key, level) else { continue };
        let words = match entry.kind {
            Kind::Choice(words) => words,
            // Stored strictly as `yes` or `no` (F4).
            Kind::Flag => words::FLAG_WORDS,
            Kind::Length { .. } | Kind::Number { .. } | Kind::Text { .. } | Kind::Parsed { .. } | Kind::Style(_) => continue,
        };
        // A project value an entry keeps in the settings is typed there, not stored as a word.
        let in_settings = level == Level::Project && matches!(entry.project, ProjectSource::Settings(_));
        let stored_here = entry.allows(level) && !in_settings;
        if stored_here && entry.parse(value).is_none() {
            out.push(UnknownWord::Value { place, setting: key.clone(), word: value.clone(), allowed: words.to_vec() });
        }
    }
}

/// The property a device's kind was kept in before design 072: a device has none now (drawing
/// aids, loose parts and routes are their own types, §6).
const OLD_KIND_KEY: &str = "kind";

/// A device's stored `kind`, any word: a junction went with routes (design 072 D: a T is a route
/// landing on a route), so a file holding one opens read-only (pre-v1, no conversion). The old
/// device forms of drawing aids and loose parts are read errors in ecad-file.
fn old_kind(props: &Properties, place: WordPlace, out: &mut Vec<UnknownWord>) {
    if let Some(kind) = props.get(OLD_KIND_KEY) {
        out.push(UnknownWord::Value { place, setting: OLD_KIND_KEY.into(), word: kind.clone(), allowed: Vec::new() });
    }
}

/// The yes/no keys of [`keys::KEYS`] holding anything but the written words `yes` and `no`.
fn flags(props: &Properties, place: WordPlace, out: &mut Vec<UnknownWord>) {
    let written = [words::flag_word(true), words::flag_word(false)];
    for (key, value) in props {
        let flag = keys::spec(key).is_some_and(|s| matches!(s.value, keys::ValueSpec::Flag));
        if flag && !written.contains(&value.as_str()) {
            out.push(UnknownWord::Value { place, setting: key.clone(), word: value.clone(), allowed: written.to_vec() });
        }
    }
}

/// A property that became a struct field.
fn reserved(props: &Properties, place: WordPlace, out: &mut Vec<UnknownWord>) {
    let holder = match place {
        WordPlace::Project => Some(keys::Holder::Project),
        WordPlace::Page(_) => Some(keys::Holder::Page),
        WordPlace::Device(_) => Some(keys::Holder::Device),
        WordPlace::Wire(_) => Some(keys::Holder::Wire),
        WordPlace::Cable(_) => Some(keys::Holder::Cable),
        WordPlace::Settings | WordPlace::Standard => None,
    };
    let reserved = |k: &str| holder.map_or_else(|| keys::is_reserved(k), |h| keys::is_reserved_on(h, k));
    out.extend(props.keys().filter(|k| reserved(k)).map(|k| UnknownWord::ReservedKey { place, key: k.clone() }));
}

/// The registry entry stored under `key` at `level`: the entry itself, or on an object a
/// symbol's own value of it (`text.<rep>.tag.show`, [`defaults::symbol_key`]).
fn entry_of(key: &str, level: Level) -> Option<(&'static Entry, Level)> {
    if let Some(e) = defaults::find(key) {
        return Some((e, level));
    }
    if level != Level::Object {
        return None;
    }
    let mut parts = key.splitn(3, '.');
    let (area, rep, rest) = (parts.next()?, parts.next()?, parts.next());
    RepresentationId::parse(rep).filter(|_| rep.len() == 32)?;
    let generic = match rest {
        Some(rest) => format!("{area}.{rest}"),
        None => area.to_string(),
    };
    defaults::find(&generic).map(|e| (e, Level::Object))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ecad_model::{Device, DeviceId, PageId};

    fn doc() -> Document {
        Document::default()
    }

    #[test]
    fn known_words_pass_and_unknown_ones_are_named() {
        let mut d = doc();
        d.properties.insert("text.tag.show".into(), "first".into());
        assert!(check(&d).is_empty(), "a known word passes");
        d.properties.insert("text.tag.show".into(), " FIRST ".into());
        assert!(check(&d).is_empty(), "case and spaces are read as today");
        d.properties.insert("text.tag.show".into(), "maybe".into());
        let found = check(&d);
        assert_eq!(found.len(), 1);
        let text = found[0].to_string();
        assert!(text.contains("`maybe`") && text.contains("text.tag.show") && text.contains("project"), "{text}");
        assert!(matches!(&found[0], UnknownWord::Value { allowed, .. } if allowed.contains(&"first")));
    }

    #[test]
    fn a_stored_junction_is_an_unknown_kind() {
        let mut d = doc();
        let j = DeviceId(4);
        let mut junction = Device::default();
        junction.properties.insert(OLD_KIND_KEY.into(), "junction".into());
        d.devices.insert(j, junction);
        let found = check(&d);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].place(), WordPlace::Device(j));
        let text = found[0].to_string();
        assert!(text.contains("`junction`") && text.contains("`kind`"), "{text}");
    }

    #[test]
    fn values_that_are_not_words_are_left_to_their_readers() {
        let mut d = doc();
        d.properties.insert("text.tag.height".into(), "huge".into());
        d.properties.insert("my_own_field".into(), "anything".into());
        assert!(check(&d).is_empty());
    }

    #[test]
    fn every_level_is_checked() {
        let mut d = doc();
        let page = PageId(1);
        d.pages.insert(page, ecad_model::Page::titled("p").unwrap());
        if let Some(p) = d.pages.get_mut(&page) {
            p.display.insert("text.tag.show".into(), "sometimes".into());
        }
        let c = DeviceId(2);
        d.devices.insert(c, Device::default());
        let rep = RepresentationId(0xab);
        if let Some(comp) = d.devices.get_mut(&c) {
            comp.properties.insert(defaults::symbol_key("text.tag.show", rep), "never".into());
        }
        let places: Vec<WordPlace> = check(&d).iter().map(UnknownWord::place).collect();
        assert_eq!(places, [WordPlace::Page(page), WordPlace::Device(c)]);
    }

    #[test]
    fn stored_flags_hold_only_yes_or_no() {
        let mut d = doc();
        // A cable's bag still holds `tag_fixed` (a device's is a field, design 072 §2.1).
        let c = ecad_model::CableId(3);
        d.cables.insert(c, ecad_model::Cable::default());
        d.properties.insert(keys::NUMBERING_PROVISIONAL.as_str().into(), "yes".into());
        if let Some(cable) = d.cables.get_mut(&c) {
            cable.properties.insert(keys::TAG_FIXED.as_str().into(), "no".into());
        }
        assert!(check(&d).is_empty(), "the written words pass");
        for typed in ["on", "true", "1", "off", "Yes", " no"] {
            if let Some(cable) = d.cables.get_mut(&c) {
                cable.properties.insert(keys::TAG_FIXED.as_str().into(), typed.into());
            }
            let found = check(&d);
            assert_eq!(found.len(), 1, "{typed:?} in a file is refused");
            let text = found[0].to_string();
            assert!(text.contains("tag_fixed") && text.contains(&format!("`{typed}`")), "{text}");
        }
    }

    #[test]
    fn tag_prefixes_name_built_in_symbols() {
        let mut d = doc();
        let q = ecad_model::prefix::TagPrefix::typed("Q").unwrap();
        d.settings.tag_prefixes.insert(ecad_model::prefix::PrefixOf::Starter("nosuch".into()), q.clone());
        // A project symbol's prefix names a project symbol the project holds: none here.
        d.settings.tag_prefixes.insert(ecad_model::prefix::PrefixOf::Project("valve-x".into()), q);
        let found = check(&d);
        assert_eq!(found.len(), 2);
        assert!(found.iter().any(|f| f.to_string().contains("tag_prefixes") && f.to_string().contains("starter:nosuch")));
        assert!(found.iter().any(|f| f.to_string().contains("project:valve-x")));
    }

    #[test]
    fn registry_and_key_list_are_disjoint() {
        for e in defaults::all() {
            assert!(keys::spec(e.key).is_none(), "{} is both a registry entry and in keys::KEYS", e.key);
            // A page's type is a page-level-only entry whose value is the page's field (design 072
            // §5.1): `setdefault` writes it with `ModifyPage`, so a bag holding it is an error.
            let field = e.key == ecad_model::sheet::PAGE_TYPE_KEY;
            assert!(field || !keys::is_reserved(e.key), "{} is both a registry entry and reserved", e.key);
            let area = keys::area_of(e.key);
            assert!(area.is_registry(), "{} classifies to {area:?}, not a registry area", e.key);
        }
        let words: Vec<&str> = keys::Area::REGISTRY.iter().map(|a| ecad_model::Word::word(*a)).collect();
        assert_eq!(words, defaults::AREAS, "Area::REGISTRY has the registry's words in its order");
    }
}
