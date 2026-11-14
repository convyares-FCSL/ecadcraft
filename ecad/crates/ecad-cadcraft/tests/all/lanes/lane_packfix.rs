//! The drawing-pack re-run's bugs (2026-11-14), one test or more per bug, each from a generic
//! repro: project symbols, tags, contact tables, references and PLC card drawing.

use ecad_model::{ComponentId, Document, PageId};
use serde_json::{Value, json};

use crate::common::{H, id};

/// A one-point earth symbol whose point has an empty label, prefix PE.
fn earth(h: &mut H) {
    h.run(
        "symbolnew",
        json!({
            "name": "Earth", "key": "e1",
            "graphics": [{"kind": "line", "a": [-3, 0], "b": [3, 0]}],
            "points": [{"key": "PE", "label": "", "at": [0, 0]}],
            "slots": [{"text": "tag", "at": [4, 0]}],
            "defaults": {"prefix": "PE"},
        }),
    )
    .unwrap();
}

fn placed(h: &mut H, params: Value) -> ComponentId {
    id(&h.run("place", params).unwrap(), "component")
}

fn tag(d: &Document, c: ComponentId) -> String {
    d.components.get(&c).and_then(|x| x.properties.get("tag").cloned()).unwrap_or_default()
}

/// Every text value derived on `page`, shown or not.
fn page_texts(h: &mut H, page: PageId) -> Vec<String> {
    let d = h.doc();
    ecad_cadcraft::devices::all_texts_on_page(&d, page).into_iter().map(|t| t.text.value).collect()
}

fn is_hex_id(s: &str) -> bool {
    s.len() == 32 && s.chars().all(|c| c.is_ascii_hexdigit())
}

// ---------------------------------------------------------------- bug 1

#[test]
fn bug_1_an_unlabelled_point_never_prints_its_id_after_a_merge() {
    let mut h = H::new(9101);
    let page = h.first_page();
    earth(&mut h);
    let a = placed(&mut h, json!({"symbol": "e1", "at": [50, 50]}));
    let b = placed(&mut h, json!({"symbol": "e1", "at": [80, 50]}));
    let ta = tag(&h.doc(), a);
    h.run("setprop", json!({"component": b.hex(), "key": "tag", "value": ta, "merge": true})).unwrap();
    let d = h.doc();
    assert!(!d.components.contains_key(&b), "the second earth merged into the first");
    for t in page_texts(&mut h, page) {
        assert!(!is_hex_id(&t), "a text prints a 32-digit ID: {t}");
    }
    // Both points are still named by their key, unlabelled.
    let terms: Vec<_> = d.terminals_of(a).map(|(_, t)| (t.key.clone(), t.label.clone())).collect();
    assert_eq!(terms.len(), 2);
    assert!(terms.iter().all(|(k, l)| l.is_none() && k.as_deref().is_some_and(|k| !is_hex_id(k))), "{terms:?}");
}

// ---------------------------------------------------------------- bug 2

#[test]
fn bug_2_place_names_a_new_project_symbol_device_by_its_typed_tag() {
    let mut h = H::new(9102);
    h.run(
        "symbolnew",
        json!({
            "name": "Sensor", "key": "s1",
            "graphics": [{"kind": "line", "a": [-3, 0], "b": [3, 0]}],
            "points": [{"key": "1", "at": [-5, 0]}, {"key": "2", "at": [5, 0]}],
            "slots": [{"text": "tag", "at": [0, 4]}],
            "defaults": {"prefix": "B"},
        }),
    )
    .unwrap();
    let a = placed(&mut h, json!({"symbol": "s1", "at": [100, 100], "tag": "B07"}));
    assert_eq!(tag(&h.doc(), a), "-B07");
    // Without a tag: the next free number, as before.
    let b = placed(&mut h, json!({"symbol": "s1", "at": [120, 100]}));
    assert_ne!(tag(&h.doc(), b), "-B07");
    // A tag a device holds is that device, as for built-in symbols: this symbol would join it, and
    // its points 1 and 2 are taken there.
    let e = h.run("place", json!({"symbol": "s1", "at": [140, 100], "tag": "b7"})).unwrap_err();
    assert!(e.contains("-B07 already has a connection point"), "{e}");
    // `tag` and `device` together are refused.
    assert!(h.run("place", json!({"symbol": "s1", "at": [160, 100], "tag": "B9", "device": a.hex()})).is_err());
}
