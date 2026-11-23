//! The words check run once when a project is opened.
//!
//! A typed field with an unknown word already fails its record's decode. This check finds the
//! words inside values that stay text: a stored value of a defaults choice that is not one of its
//! words, a defaults switch ([`Kind::Flag`]) or a yes/no key ([`keys::ValueSpec::Flag`]) holding anything but `yes` or `no`, a
//! `tag_prefixes` key that is not a built-in symbol, and a property that became a struct field
//! ([`ecad_model::naming::keys::RESERVED`]). Typed input may say `true`, `1` or `on`; a file holds only the
//! words the program writes. Anything found opens the project read-only, naming the
//! word, the setting and where it is; the file is never changed.
//!
//! It reads the whole document, so it runs on open (and on reload) only, never per commit. It
//! changes nothing and excludes values that are not words: an out-of-range length or a malformed
//! style ID still reads as absent.

use ecad_model::document::strict::{UnknownWord, WordPlace};
use ecad_model::{Document, Properties, RepresentationId};
use {ecad_model::naming::keys, ecad_model::naming::words};

use ecad_model::registry::data::Registry;
use ecad_model::registry::{Entry, Kind, Level, ProjectSource};

/// Every unknown word in `doc`, in a stable order (by place, then setting).
///
/// # Example
/// ```
/// use ecad_model::Document;
/// use ecad_model::registry::data::Registry;
/// static REGISTRY: Registry = Registry::new(&[&ecad_electrical::defaults::data::DATA]);
/// let mut doc = Document::default();
/// assert!(ecad_electrical::checks::words_check::check(&doc, &REGISTRY).is_empty());
/// doc.edit_parts(|parts| parts.display.insert("wire.join".into(), "maybe".into()));
/// let found = ecad_electrical::checks::words_check::check(&doc, &REGISTRY);
/// assert_eq!(found.len(), 1);
/// assert!(found[0].to_string().contains("`maybe`"));
/// ```
pub fn check(doc: &Document, registry: &Registry) -> Vec<UnknownWord> {
    let mut out = Vec::new();
    bag(registry, doc.display(), WordPlace::Project, Level::Project, &mut out);
    if let Some(std) = &doc.standard() {
        bag(registry, &std.values, WordPlace::Standard, Level::Standard, &mut out);
    }
    for (id, p) in doc.pages() {
        bag(registry, &p.display, WordPlace::Page(*id), Level::Page, &mut out);
    }
    for (id, c) in doc.devices() {
        bag(registry, &c.properties, WordPlace::Device(*id), Level::Object, &mut out);
        old_kind(&c.properties, WordPlace::Device(*id), &mut out);
    }
    for (id, c) in doc.wires() {
        bag(registry, &c.display, WordPlace::Wire(*id), Level::Object, &mut out);
    }
    for (id, c) in doc.cables() {
        reserved(&c.properties, WordPlace::Cable(*id), &mut out);
    }
    // A starter's prefix names a starter of the catalogue (the key's form is read
    // strictly with the file); a project symbol's prefix names a project symbol the project holds.
    for key in doc.settings().tag_prefixes.keys() {
        let known = match key {
            ecad_model::naming::prefix::PrefixOf::Starter(w) => crate::symbols::catalogue::by_key(w).is_some(),
            ecad_model::naming::prefix::PrefixOf::Project(k) => crate::symbols::project::has_project_key(doc, k),
        };
        if !known {
            out.push(UnknownWord::Value { place: WordPlace::Settings, setting: "tag_prefixes".into(), word: key.to_string(), allowed: Vec::new() });
        }
    }
    out.sort();
    out
}

/// The unknown words of one property bag held at `level`.
fn bag(registry: &Registry, props: &Properties, place: WordPlace, level: Level, out: &mut Vec<UnknownWord>) {
    reserved(props, place, out);
    flags(props, place, out);
    for (key, value) in props {
        let Some((entry, level)) = entry_of(registry, key, level) else { continue };
        let words = match entry.kind {
            Kind::Choice(words) => words,
            // Stored strictly as `yes` or `no`.
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

/// The property a device's kind was kept in before the current model: a device has none now (drawing
/// aids, loose parts and routes are their own types).
const OLD_KIND_KEY: &str = "kind";

/// A device's stored `kind`, any word: a junction went with routes (a T is a route
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
        WordPlace::Settings | WordPlace::Standard | WordPlace::File => None,
    };
    let reserved = |k: &str| holder.map_or_else(|| keys::is_reserved(k), |h| keys::is_reserved_on(h, k));
    out.extend(props.keys().filter(|k| reserved(k)).map(|k| UnknownWord::ReservedKey { place, key: k.clone() }));
}

/// The registry entry stored under `key` at `level`: the entry itself, or on an object a
/// symbol's own value of it (`text.<rep>.tag.show`, [`ecad_model::registry::symbol_key`]).
fn entry_of(registry: &Registry, key: &str, level: Level) -> Option<(&'static Entry, Level)> {
    if let Some(e) = registry.find(key) {
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
    registry.find(&generic).map(|e| (e, Level::Object))
}
