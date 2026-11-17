//! Brief 062: `test_support::ProjectBuilder` builds what the commands build, and a test can start
//! from it in one expression.

use ecad_cadcraft::test_support::ProjectBuilder;
use ecad_model::{Document, Subject};
use serde_json::json;

use crate::common::H;

fn model_parts(d: &Document) -> impl PartialEq + std::fmt::Debug + '_ {
    (&d.devices, &d.terminals, &d.wires, &d.cables, &d.pages, &d.page_order, &d.representations, &d.items, &d.properties)
}

/// Two pages, two tagged coils, a wire between their first connection points and a cable that is
/// not drawn: the harness's invariants hold and the ids are the ones the build returned.
#[test]
fn the_builder_makes_pages_devices_a_wire_and_a_cable() {
    let b = ProjectBuilder::new(6201)
        .page_titled("Second")
        .coil("-K1", (50.0, 50.0))
        .coil("-K2", (150.0, 50.0))
        .wire("-K1", "-K2")
        .cable(json!({"cores": "4G"}))
        .build();
    let (k1, k2) = (b.device("-K1"), b.device("-K2"));
    let (wire, cable, added) = (b.wires[0], b.cables[0], b.added_pages.clone());
    let mut h = H::from_built(b);
    let doc = h.doc();
    assert_eq!(doc.page_order.len(), 2);
    assert_eq!(added, vec![doc.page_order[1]]);
    assert!(doc.devices.contains_key(&k1) && doc.devices.contains_key(&k2));
    assert!(doc.wires[&wire].ends.iter().all(Option::is_some), "both ends attached to the coils");
    assert!(doc.cables[&cable].members.is_empty(), "a cable made by `cablenew` is not drawn");
    assert_eq!(doc.representations_of(Subject::Device(k1)).count(), 1);
}

/// The builder runs the same commands, so the same seed gives the same model as running them by hand.
#[test]
fn the_builder_equals_the_commands_run_by_hand() {
    let mut a = H::from_built(ProjectBuilder::new(6202).coil("-K1", (50.0, 50.0)).wire((0.0, 0.0), (30.0, 0.0)).build());

    let mut b = H::new(6202);
    b.run("place", json!({"symbol": "coil", "at": [50.0, 50.0], "tag": "-K1"})).unwrap();
    b.run("wire", json!({"points": [[0.0, 0.0], [30.0, 0.0]]})).unwrap();

    let (da, db) = (a.doc(), b.doc());
    assert!(model_parts(&da) == model_parts(&db), "the builder's project differs from the hand-run one");
}

/// A step the command refuses fails the build with the command and its message.
#[test]
#[should_panic(expected = "builder step `place")]
fn a_refused_step_names_the_command() {
    let _ = ProjectBuilder::new(6203).place("no-such-symbol", "-K1", (0.0, 0.0)).build();
}

/// Any other command goes through `command`, in order between the steps.
#[test]
fn other_commands_run_in_order_between_the_steps() {
    let b = ProjectBuilder::new(6204).coil("-K1", (50.0, 50.0)).command("newpage", json!({"title": "Extra"})).coil("-K2", (150.0, 50.0)).build();
    let mut h = H::from_built(b);
    assert_eq!(h.doc().page_order.len(), 2);
    assert_eq!(h.doc().devices.len(), 2);
}
