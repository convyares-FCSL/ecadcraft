//! The clipboard keeps the page objects were copied from: a paste reads that copy, not the live project.

use crate::common::{H, id};
use ecad_model::{DeviceId, Subject};
use serde_json::json;

/// A symbol copied to the clipboard and then erased from the page still pastes as a device of the same definition, with its
/// connection points.
#[test]
fn a_symbol_erased_after_it_was_copied_still_pastes_whole() {
    let mut h = H::new(0xC11B);
    let p1 = h.first_page();
    // A second one keeps the symbol's drawing block on the page once the first is erased.
    h.run("place", json!({"at": [200, 10]})).unwrap();
    let placed: DeviceId = id(&h.run("place", json!({"at": [100, 10]})).unwrap(), "component");
    let definition = h.doc().devices()[&placed].definition;
    let item = h.symbol_item(placed);
    let sym = h.handle_of(p1, item);
    h.run("copyclip", json!({"handles": [sym.hex()]})).unwrap();
    h.run("erase", json!({"handles": [sym.hex()]})).unwrap();
    let before = h.doc();
    assert_eq!(before.representations_of(Subject::Device(placed)).count(), 0, "the symbol is erased from the page");
    let base = h.s.clipboard_base;
    h.run("pasteclip", json!({"at": [base.x, base.y - 60.0]})).unwrap();
    let doc = h.doc();
    let pasted: Vec<&DeviceId> = doc.devices().keys().filter(|d| !before.devices().contains_key(d)).collect();
    assert_eq!(pasted.len(), 1, "one device pasted");
    assert_eq!(doc.devices()[pasted[0]].definition, definition, "same definition");
    assert_eq!(doc.connection_points_of(*pasted[0]).count(), 2, "with its connection points");
}
