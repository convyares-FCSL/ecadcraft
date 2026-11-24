//! The rating line on the drawing: a group's template (`rating.template.<group>`) says which of
//! its set fields show and in what order; the built-in template is the plate and design fields.

use ecad_model::objects::devices::DeviceGroup;
use ecad_model::{DeviceId, Word};
use serde_json::json;

use crate::common::{H, id};
use cadcraft_engine::doc::EntityKind;
use ecad_engine::history::event_log::EventKind;

fn line(h: &mut H, c: DeviceId) -> Option<String> {
    let doc = h.doc();
    ecad_electrical::devices::ratings::line(&doc, &doc.devices()[&c])
}

/// Every group's built-in template is its plate and design fields, in the model's order. A group
/// whose fields all come from a datasheet (a connector) has no rating line and no template.
#[test]
fn the_built_in_templates_are_the_plate_and_design_fields() {
    let mut h = H::new(0x00d4_0300);
    let doc = h.doc();
    for g in <DeviceGroup as Word>::ALL.iter().copied().filter(|g| !g.default_template().is_empty()) {
        let key = format!("rating.template.{}", g.word());
        let e = ecad_drawing_style::REGISTRY.find(&key).unwrap_or_else(|| panic!("{key} is registered"));
        let built_in = ecad_model::registry::resolve(&doc, e, ecad_model::registry::Scope::project()).text();
        assert_eq!(built_in, g.default_template().join(", "), "{key}");
    }
}

/// A project template picks and orders the fields the drawing shows; the stored rating and every
/// answer keep all of them; a word the group has no field for is refused.
#[test]
fn a_template_picks_and_orders_the_fields_the_drawing_shows() {
    let mut h = H::new(0x00d4_0301);
    let m: DeviceId = id(&crate::common::place(&mut h, "motor3", (100.0, 100.0)), "component");
    h.run("setprop", json!({"component": m.hex(), "key": "rating", "value": "7.5 kW 400 V 15.2 A 50 Hz DOL"})).unwrap();
    assert_eq!(line(&mut h, m).as_deref(), Some("7.5 kW, 400 V, 15.2 A, 50 Hz, DOL"));
    h.run("setdefault", json!({"key": "rating.template.motor", "level": "project", "value": "current, power"})).unwrap();
    assert_eq!(line(&mut h, m).as_deref(), Some("15.2 A, 7.5 kW"));
    assert_eq!(h.doc().devices()[&m].value("rating").as_deref(), Some("7.5 kW, 400 V, 15.2 A, 50 Hz, DOL"), "the answer keeps every field");
    // Only unset fields named: nothing to show.
    h.run("setdefault", json!({"key": "rating.template.motor", "level": "project", "value": "efficiency"})).unwrap();
    assert_eq!(line(&mut h, m), None);
    for bad in ["power, rpm", "power, power", ""] {
        let e = h.run("setdefault", json!({"key": "rating.template.motor", "level": "project", "value": bad}));
        assert!(e.is_err(), "{bad:?} is refused");
    }
    // No typed rating: the note shows, whatever the template.
    let n: DeviceId = id(&crate::common::place(&mut h, "motor3", (200.0, 100.0)), "component");
    h.run("setprop", json!({"component": n.hex(), "key": "rating", "value": "see datasheet"})).unwrap();
    assert_eq!(line(&mut h, n).as_deref(), Some("see datasheet"));
}

/// The drawn texts of `page` whose value is `value`: their handles.
fn drawn(h: &mut H, page: ecad_model::PageId, value: &str) -> Vec<cadcraft_engine::doc::Handle> {
    h.pv(|p, pg| pg.entities_of(p, page)).iter().filter(|e| matches!(&e.kind, EntityKind::Text(t) if t.value == value)).map(|e| e.handle).collect()
}

/// A built-in motor and lamp draw their rating line beside the symbol. Changing the group's template
/// redraws it through the commit's update path: the same text, rewritten in place (its handle
/// kept), no page rebuilt; undo brings the old line back.
#[test]
fn a_template_change_redraws_the_rating_line_in_place() {
    let mut h = H::new(0x00d4_0302);
    let page = h.first_page();
    h.project.open_page(&mut h.s, page).unwrap();
    let m: DeviceId = id(&crate::common::place(&mut h, "motor3", (100.0, 100.0)), "component");
    h.run("setprop", json!({"component": m.hex(), "key": "rating", "value": "7.5 kW 400 V 15.2 A 50 Hz DOL"})).unwrap();
    let l: DeviceId = id(&crate::common::place(&mut h, "lamp", (200.0, 100.0)), "component");
    h.run("setprop", json!({"component": l.hex(), "key": "rating", "value": "green flashing 24 V DC"})).unwrap();
    let lamp_line = line(&mut h, l).unwrap();
    assert_eq!(drawn(&mut h, page, &lamp_line).len(), 1, "the lamp's line {lamp_line:?} is drawn");
    let before = drawn(&mut h, page, "7.5 kW, 400 V, 15.2 A, 50 Hz, DOL");
    assert_eq!(before.len(), 1, "the motor's line is drawn once");
    let rebuilt = h.p(|p| p.events.count(EventKind::ProjectionRebuilt));
    let passes = ecad_electrical::devices::texts::text_passes(page);
    h.s.execute("setdefault", &json!({"key": "rating.template.motor", "level": "project", "value": "current, power"})).unwrap();
    assert!(ecad_electrical::devices::texts::text_passes(page) > passes, "the page's texts were worked out again");
    assert_eq!(drawn(&mut h, page, "15.2 A, 7.5 kW"), before, "the same text, rewritten");
    assert!(drawn(&mut h, page, "7.5 kW, 400 V, 15.2 A, 50 Hz, DOL").is_empty());
    assert_eq!(drawn(&mut h, page, &lamp_line).len(), 1, "another group's line stays");
    assert_eq!(h.p(|p| p.events.count(EventKind::ProjectionRebuilt)), rebuilt, "no page rebuilt");
    h.s.execute("undo", &json!({})).unwrap();
    assert_eq!(drawn(&mut h, page, "7.5 kW, 400 V, 15.2 A, 50 Hz, DOL"), before);
    h.check();
}
