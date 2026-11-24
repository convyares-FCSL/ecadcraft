//! One rating field at a time: the Device tab's row per field of the device's kind
//! (`rating.<word>`), `setprop rating.<word>` read through the field's type and refused with the
//! field's label, and `deviceprops`' `rating_fields`.

use std::collections::BTreeSet;

use ecad_model::DeviceId;
use ecad_model::objects::devices::fields::FieldType;
use serde_json::json;

use crate::common::{H, id};

/// The Device tab's rating rows of `c`: key, value and whether it can be edited.
fn rows(h: &mut H, c: DeviceId) -> Vec<(String, String, bool)> {
    let doc = h.doc();
    let s = ecad_electrical::properties::device_section(&doc, &ecad_drawing_style::REGISTRY, None, &BTreeSet::from([c]), None);
    s.rows.iter().filter(|r| r.key.starts_with("rating.")).map(|r| (r.key.clone(), r.value.clone(), r.editable)).collect()
}

/// Every field a placed device's kind has is a row; each one typed by itself takes its type's
/// example, shows it, and clears again; one of another type is refused with the field's label.
#[test]
fn every_rating_field_is_a_row_set_through_its_type() {
    let mut h = H::new(0x00d4_0401);
    let mut seen = BTreeSet::new();
    for (n, symbol) in ["motor3", "lamp", "heater", "fuse", "mcb1", "rcbo2", "psu", "trafo", "coil", "prox3", "fusebank4"].into_iter().enumerate() {
        let c: DeviceId = id(&crate::common::place(&mut h, symbol, (40.0 + 40.0 * n as f64, 100.0)), "component");
        let group = h.doc().devices()[&c].device_type.group();
        let shown = rows(&mut h, c);
        for f in group.rating_fields() {
            let key = format!("rating.{}", f.word);
            let Some((_, _, editable)) = shown.iter().find(|(k, _, _)| *k == key) else {
                // Not on this kind: setprop says so in the field's words.
                if f.ty != FieldType::Combined {
                    let e = h.run("setprop", json!({"component": c.hex(), "key": key, "value": f.ty.example()})).unwrap_err();
                    assert!(e.contains(&f.label.to_lowercase()), "{symbol} {key}: {e}");
                }
                continue;
            };
            assert_eq!(*editable, f.ty != FieldType::Combined, "{symbol} {key}");
            if f.ty == FieldType::Combined {
                let e = h.run("setprop", json!({"component": c.hex(), "key": key, "value": "x"})).unwrap_err();
                assert!(e.contains(f.label) && e.contains("whole rating"), "{symbol} {key}: {e}");
                continue;
            }
            seen.insert((group, f.word));
            h.run("setprop", json!({"component": c.hex(), "key": key, "value": f.ty.example()})).unwrap_or_else(|e| panic!("{symbol} {key}: {e}"));
            let value = rows(&mut h, c).into_iter().find(|(k, _, _)| *k == key).map(|(_, v, _)| v).unwrap_or_default();
            let doc = h.doc();
            assert_eq!(value, (f.get)(&doc.devices()[&c].device_type).unwrap_or_default(), "{symbol} {key}");
            // What the row shows reads back to the same value.
            let before = h.doc().devices()[&c].device_type.clone();
            if !value.is_empty() {
                h.run("setprop", json!({"component": c.hex(), "key": key, "value": value})).unwrap_or_else(|e| panic!("{symbol} {key} {value}: {e}"));
                assert_eq!(h.doc().devices()[&c].device_type, before, "{symbol} {key} {value}");
            }
            let e = h.run("setprop", json!({"component": c.hex(), "key": key, "value": "zz"})).unwrap_err();
            assert!(e.starts_with(&format!("{} takes ", f.label)), "{symbol} {key}: {e}");
            h.run("setprop", json!({"component": c.hex(), "key": key, "value": ""})).unwrap();
            assert_eq!((f.get)(&h.doc().devices()[&c].device_type), None, "{symbol} {key} clears");
        }
    }
    // Every plain field of every group was reached by some built-in symbol.
    for g in <ecad_model::objects::devices::DeviceGroup as ecad_model::Word>::ALL {
        for f in g.rating_fields().iter().filter(|f| f.ty != FieldType::Combined) {
            assert!(seen.contains(&(*g, f.word)), "{} {} set by no symbol here", g.label(), f.word);
        }
    }
}

/// A value out of the group's range is refused with the field's label; a field set by itself
/// shows on the rating line in the template's place.
#[test]
fn a_field_out_of_range_is_refused_and_a_set_field_draws_on_the_line() {
    let mut h = H::new(0x00d4_0402);
    let m: DeviceId = id(&crate::common::place(&mut h, "motor3", (100.0, 100.0)), "component");
    let e = h.run("setprop", json!({"component": m.hex(), "key": "rating.frequency", "value": "5000 Hz"})).unwrap_err();
    assert!(e.starts_with("Rated frequency: ") && e.contains("out of range"), "{e}");
    h.run("setprop", json!({"component": m.hex(), "key": "rating.power", "value": "7500"})).unwrap();
    h.run("setprop", json!({"component": m.hex(), "key": "rating.starting", "value": "star-delta"})).unwrap();
    let doc = h.doc();
    assert_eq!(ecad_electrical::devices::ratings::line(&doc, &doc.devices()[&m]).as_deref(), Some("7.5 kW, star-delta"));
    let v = h.run("deviceprops", json!({"component": m.hex()})).unwrap();
    let fields = v["rating_fields"].as_array().cloned().unwrap_or_default();
    let words: Vec<&str> = fields.iter().filter_map(|f| f["key"].as_str()).collect();
    assert_eq!(
        words,
        ["rating.power", "rating.voltage", "rating.current", "rating.frequency", "rating.power_factor", "rating.efficiency", "rating.starting"]
    );
    assert_eq!(fields[0]["value"], json!("7.5 kW"));
    assert_eq!(fields[0]["source"], json!("nameplate"));
    assert_eq!(fields[6]["source"], json!("design"));
    assert_eq!(fields[6]["choices"], json!(["direct", "star_delta", "soft", "vfd"]));
    assert_eq!(fields[1]["value"], json!(null));
}

/// Every nameplate field of a group's list is read by that group's rating reader: a device with
/// each of them set, its rating typed back as the drawing shows it, has the same fields. A field
/// added to a list without its reader words fails here.
#[test]
fn the_rating_reader_fills_every_nameplate_field() {
    use ecad_model::objects::devices::FieldSource;
    let mut h = H::new(0x00d4_0403);
    let mut read = BTreeSet::new();
    for (n, symbol) in ["motor3", "lamp", "heater", "fuse", "mcb1", "rcbo2", "psu", "trafo", "coil", "prox3"].into_iter().enumerate() {
        let c: DeviceId = id(&crate::common::place(&mut h, symbol, (40.0 + 40.0 * n as f64, 160.0)), "component");
        let group = h.doc().devices()[&c].device_type.group();
        let plate: Vec<_> = group.rating_fields().iter().filter(|f| f.source == FieldSource::Plate && f.ty != FieldType::Combined).collect();
        for f in &plate {
            // A field this kind does not have is refused; the others take their example.
            let _ = h.run("setprop", json!({"component": c.hex(), "key": format!("rating.{}", f.word), "value": f.ty.example()}));
        }
        let before = h.doc().devices()[&c].device_type.clone();
        let text = before.rating().unwrap_or_else(|| panic!("{symbol}: rated"));
        h.run("setprop", json!({"component": c.hex(), "key": "rating", "value": ""})).unwrap();
        h.run("setprop", json!({"component": c.hex(), "key": "rating", "value": text})).unwrap_or_else(|e| panic!("{symbol} {text}: {e}"));
        let after = h.doc().devices()[&c].device_type.clone();
        for f in &plate {
            let want = (f.get)(&before);
            assert_eq!((f.get)(&after), want, "{symbol}: {} read from {text:?}", f.word);
            if want.is_some() {
                read.insert((group, f.word));
            }
        }
    }
    for g in <ecad_model::objects::devices::DeviceGroup as ecad_model::Word>::ALL {
        for f in g.rating_fields().iter().filter(|f| f.source == FieldSource::Plate && f.ty != FieldType::Combined) {
            assert!(read.contains(&(*g, f.word)), "{} {}: no symbol here has it", g.label(), f.word);
        }
    }
}

/// A datasheet field is set by itself and kept when the whole rating is typed or cleared; it shows
/// on the rating line only when the group's template adds it.
#[test]
fn a_datasheet_field_is_kept_through_the_rating_and_drawn_when_the_template_adds_it() {
    let mut h = H::new(0x00d4_0404);
    let lamp: DeviceId = id(&crate::common::place(&mut h, "lamp", (100.0, 100.0)), "component");
    h.run("setprop", json!({"component": lamp.hex(), "key": "rating.current", "value": "0.02 A"})).unwrap();
    h.run("setprop", json!({"component": lamp.hex(), "key": "rating.ip_rating", "value": "ip 65"})).unwrap();
    h.run("setprop", json!({"component": lamp.hex(), "key": "rating", "value": "RD 24VDC 2W"})).unwrap();
    let line = |h: &mut H| {
        let doc = h.doc();
        ecad_electrical::devices::ratings::line(&doc, &doc.devices()[&lamp])
    };
    assert_eq!(line(&mut h).as_deref(), Some("red, 24 V DC, 2 W"), "built in, the datasheet fields do not show");
    let rows = rows(&mut h, lamp);
    let value = |key: &str| rows.iter().find(|(k, _, _)| k == key).map(|(_, v, _)| v.clone());
    assert_eq!((value("rating.current"), value("rating.ip_rating")), (Some("20 mA".into()), Some("IP65".into())));
    // The datasheet rows come after the nameplate and design rows.
    assert_eq!(rows.iter().map(|(k, _, _)| k.as_str()).collect::<Vec<_>>()[4..], ["rating.current", "rating.ip_rating"]);
    h.run("setdefault", json!({"key": "rating.template.indicator", "level": "project", "value": "colour, voltage, current, ip_rating"})).unwrap();
    assert_eq!(line(&mut h).as_deref(), Some("red, 24 V DC, 20 mA, IP65"));
    h.run("setprop", json!({"component": lamp.hex(), "key": "rating", "value": ""})).unwrap();
    assert_eq!(line(&mut h).as_deref(), Some("20 mA, IP65"), "clearing the rating keeps the datasheet fields");
    let e = h.run("setprop", json!({"component": lamp.hex(), "key": "rating.current", "value": "20 A"})).unwrap_err();
    assert!(e.starts_with("Rated current: ") && e.contains("out of range"), "{e}");
    let e = h.run("setprop", json!({"component": lamp.hex(), "key": "rating.ip_rating", "value": "IP75"})).unwrap_err();
    assert_eq!(e, "IP rating takes an IP code, such as IP65 or IPX4.");
}
