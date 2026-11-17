//! Lane 072 G (design 072 §8, §4.6, F30, F36; acceptance A2, A4, A9): style records hold a typed
//! body of their own catalogue. A wire type reads and writes its typed fields, refuses an unknown
//! field or a value in a form it does not store, and cannot be filed under another catalogue.

use ecad_model::style::wire::{Insulation, LineColour};
use ecad_model::style::{StyleBody, WireType};
use ecad_model::units::Mm;
use ecad_model::wire::{Colours, WireSize};
use ecad_model::{Author, Ledger, Op, StyleCatalogue, StyleLayer, StyleRecord, StyleRef};

fn earth() -> WireType {
    WireType {
        potential: Some(ecad_model::text::Text::new("PE").unwrap()),
        size: WireSize::parse("6"),
        colours: Colours::parse("GN/YE"),
        insulation: Some(Insulation::Pvc),
        line_colour: Some(LineColour::Wire),
        line_weight: Some(Mm(0.35)),
        ..WireType::default()
    }
}

#[test]
fn a_wire_type_round_trips_as_its_typed_fields() {
    let rec = StyleRecord::new("Earth 6", StyleBody::Wire(earth()));
    let v = serde_json::to_value(&rec).unwrap();
    assert_eq!(v["body"]["wire"]["colours"], "GN/YE", "{v}");
    assert_eq!(v["body"]["wire"]["insulation"], "pvc", "{v}");
    assert_eq!(v["body"]["wire"]["size"], "6", "{v}");
    let back: StyleRecord = serde_json::from_value(v).unwrap();
    assert_eq!(back, rec);
    assert_eq!(back.catalogue(), StyleCatalogue::Wire);
}

#[test]
fn a_wire_type_with_an_unknown_field_or_word_does_not_read() {
    let good = serde_json::to_value(StyleRecord::new("Earth", StyleBody::Wire(earth()))).unwrap();
    let cases = [
        ("an unknown field", serde_json::json!({"colour": "BU"})),
        ("an unknown insulation", serde_json::json!({"insulation": "paper"})),
        ("a colour not in its stored form", serde_json::json!({"line_colour": "blue"})),
        ("text with a hidden character", serde_json::json!({"potential": "P\u{200b}E"})),
    ];
    for (what, patch) in cases {
        let mut v = good.clone();
        for (k, x) in patch.as_object().unwrap() {
            v["body"]["wire"][k] = x.clone();
        }
        assert!(serde_json::from_value::<StyleRecord>(v).is_err(), "{what} is refused");
    }
    // A body of no catalogue is refused too.
    let mut v = good;
    v["body"] = serde_json::json!({"stamp": {}});
    assert!(serde_json::from_value::<StyleRecord>(v).is_err());
}

#[test]
fn a_record_is_filed_only_under_its_own_catalogue() {
    let mut l = Ledger::new(72);
    let id = StyleRef::Own(7);
    let set = |catalogue| Op::SetStyle {
        layer: StyleLayer::Project,
        catalogue,
        id: id.clone(),
        before: None,
        after: Some(StyleRecord::new("Earth", StyleBody::Wire(earth()))),
    };
    assert!(l.commit(Author::User, "wrong", vec![set(StyleCatalogue::Text)]).is_err(), "a wire type is no text style");
    l.commit(Author::User, "right", vec![set(StyleCatalogue::Wire)]).unwrap();
    let r = ecad_model::style::record(l.document(), StyleLayer::Project, StyleCatalogue::Wire, &id).unwrap();
    assert_eq!(r.body.wire().and_then(|w| w.insulation), Some(Insulation::Pvc));
}
