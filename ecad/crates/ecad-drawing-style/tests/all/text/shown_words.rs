//! The unknown-word check over the whole registry: a stored Shown word that is not one its
//! setting allows is named, at every level it is stored.

use ecad_model::document::strict::{UnknownWord, WordPlace};
use ecad_model::{Device, DeviceId, Document, PageId, RepresentationId};

fn check(doc: &Document) -> Vec<UnknownWord> {
    ecad_electrical::checks::words_check::check(doc, &ecad_drawing_style::REGISTRY)
}

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
            comp.properties.insert(ecad_model::registry::symbol_key("text.tag.show", rep), "never".into());
        }
    });
    let places: Vec<WordPlace> = check(&d).iter().map(UnknownWord::place).collect();
    assert_eq!(places, [WordPlace::Page(page), WordPlace::Device(c)]);
}
