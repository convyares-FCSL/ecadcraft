//! The rules of the unknown-word check (design 072 §5): a stored word that is not one of the
//! words its setting allows is named, and so is a key a typed field has taken over.

use ecad_electrical::checks::words_check::check;
use ecad_electrical::defaults;
use ecad_model::document::strict::{UnknownWord, WordPlace};
use ecad_model::naming::keys;
use ecad_model::{Device, DeviceId, Document, PageId, RepresentationId};

fn doc() -> Document {
    Document::default()
}

#[test]
fn known_words_pass_and_unknown_ones_are_named() {
    let mut d = doc();
    d.edit_parts(|parts| {
        parts.display.insert("text.tag.show".into(), "first".into());
    });
    assert!(check(&d).is_empty(), "a known word passes");
    d.edit_parts(|parts| {
        parts.display.insert("text.tag.show".into(), " FIRST ".into());
    });
    assert!(check(&d).is_empty(), "case and spaces are read as today");
    d.edit_parts(|parts| {
        parts.display.insert("text.tag.show".into(), "maybe".into());
    });
    let found = check(&d);
    assert_eq!(found.len(), 1);
    let text = found[0].to_string();
    assert!(text.contains("`maybe`") && text.contains("text.tag.show") && text.contains("project"), "{text}");
    assert!(matches!(&found[0], UnknownWord::Value { allowed, .. } if allowed.contains(&"first")));
}

#[test]
fn an_old_style_cable_holding_its_tag_or_description_in_its_bag_is_named() {
    let mut d = doc();
    let c = ecad_model::CableId(5);
    let mut cable = ecad_model::Cable::default();
    for key in ["tag", "description"] {
        cable.properties.insert(key.into(), "x".into());
    }
    d.edit_parts(|parts| {
        parts.cables.insert(c, cable);
    });
    let found = check(&d);
    let keys: Vec<&str> = found
        .iter()
        .filter_map(|f| if let UnknownWord::ReservedKey { place: WordPlace::Cable(k), key } = f { (*k == c).then_some(key.as_str()) } else { None })
        .collect();
    assert_eq!(keys.len(), 2, "{found:?}");
    for key in ["tag", "description"] {
        assert!(keys.contains(&key), "{key} is named: {keys:?}");
    }
}

#[test]
fn a_stored_junction_is_an_unknown_kind() {
    let mut d = doc();
    let j = DeviceId(4);
    let mut junction = Device::default();
    junction.properties.insert("kind".into(), "junction".into());
    d.edit_parts(|parts| {
        parts.devices.insert(j, junction);
    });
    let found = check(&d);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].place(), WordPlace::Device(j));
    let text = found[0].to_string();
    assert!(text.contains("`junction`") && text.contains("`kind`"), "{text}");
}

#[test]
fn values_that_are_not_words_are_left_to_their_readers() {
    let mut d = doc();
    d.edit_parts(|parts| {
        parts.display.insert("text.tag.height".into(), "huge".into());
    });
    d.edit_parts(|parts| {
        parts.display.insert("my_own_field".into(), "anything".into());
    });
    assert!(check(&d).is_empty());
}

#[test]
fn every_level_is_checked() {
    let mut d = doc();
    let page = PageId(1);
    d.edit_parts(|parts| {
        parts.pages.insert(page, ecad_model::Page::titled("p").unwrap());
    });
    d.edit_parts(|parts| {
        if let Some(p) = parts.pages.get_mut(&page) {
            p.display.insert("text.tag.show".into(), "sometimes".into());
        }
    });
    let c = DeviceId(2);
    d.edit_parts(|parts| {
        parts.devices.insert(c, Device::default());
    });
    let rep = RepresentationId(0xab);
    d.edit_parts(|parts| {
        if let Some(comp) = parts.devices.get_mut(&c) {
            comp.properties.insert(defaults::symbol_key("text.tag.show", rep), "never".into());
        }
    });
    let places: Vec<WordPlace> = check(&d).iter().map(UnknownWord::place).collect();
    assert_eq!(places, [WordPlace::Page(page), WordPlace::Device(c)]);
}

#[test]
fn tag_prefixes_name_built_in_symbols() {
    let mut d = doc();
    let q = ecad_model::naming::prefix::TagPrefix::typed("Q").unwrap();
    d.edit_parts(|parts| {
        parts.settings.tag_prefixes.insert(ecad_model::naming::prefix::PrefixOf::Starter("nosuch".into()), q.clone());
    });
    // A project symbol's prefix names a project symbol the project holds: none here.
    d.edit_parts(|parts| {
        parts.settings.tag_prefixes.insert(ecad_model::naming::prefix::PrefixOf::Project("valve-x".into()), q);
    });
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
        let field = e.key == ecad_model::document::sheet::PAGE_TYPE_KEY;
        assert!(field || !keys::is_reserved(e.key), "{} is both a registry entry and reserved", e.key);
        let area = keys::area_of(e.key);
        assert!(area.is_registry(), "{} classifies to {area:?}, not a registry area", e.key);
    }
    let words: Vec<&str> = keys::Area::REGISTRY.iter().map(|a| ecad_model::Word::word(*a)).collect();
    assert_eq!(words, defaults::AREAS, "Area::REGISTRY has the registry's words in its order");
}
