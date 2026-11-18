//! Lane 072 B (design 072 §2.3, acceptance A7 and A8): a device's type comes from its symbol and
//! agrees with what today's predicates say it is.

use ecad_cadcraft::symbols::{STARTERS, StarterKey};
use ecad_model::device::DeviceGroup;
use ecad_model::{DeviceId, Document, Word};
use serde_json::json;

use crate::common::{H, id};

/// The group today's predicates imply for device `c`, where one of them speaks (design 072 A7).
fn predicates_say(doc: &Document, c: DeviceId) -> Option<DeviceGroup> {
    use ecad_cadcraft::{bom, connectors, continuations, locations, plc, strips};
    if strips::is_fuse_device(doc, c) || bom::is_fuse_terminal(doc, c) {
        // The owner's answer (design 072 §2.2): a fuse terminal device is Protection; a fuse
        // position in a strip (`termfuse`) is the strip's.
        return Some(DeviceGroup::Protection);
    }
    if strips::is_strip(doc, c) {
        return Some(DeviceGroup::TerminalStrip);
    }
    if connectors::is_connector(doc, c) {
        return Some(DeviceGroup::Connector);
    }
    // Lane C (design 072 §3) removed the PLC predicates; the oracle reads the card body drawn.
    let body = doc
        .representations_of(ecad_model::Subject::Device(c))
        .find_map(|(_, r)| ecad_cadcraft::devices::rep_starter(doc, r).and_then(plc::rack::CardType::of_symbol));
    match body {
        Some(t) if t.is_rack() => return Some(DeviceGroup::PlcRack),
        Some(_) => return Some(DeviceGroup::PlcModule),
        None => {}
    }
    if continuations::is_continuation(doc, c) || locations::is_box(doc, c) || bom::is_item(doc, c) {
        return Some(DeviceGroup::General);
    }
    None
}

/// A7: one device per starter that places on its own; each has the group its starter declares,
/// and wherever a predicate speaks, the predicate's group.
#[test]
fn types_match_the_old_predicates() {
    let mut h = H::new(0x72_b7);
    let mut placed = Vec::new();
    for (n, s) in STARTERS.iter().enumerate() {
        let at = [20.0 + 30.0 * (n % 12) as f64, 30.0 + 40.0 * (n / 12) as f64];
        if let Ok(v) = h.run("place", json!({"symbol": s.key.word(), "at": at}))
            && v.get("component").is_some()
        {
            placed.push((s.key, id::<DeviceId>(&v, "component")));
        }
    }
    assert!(placed.len() >= 80, "most starters place on their own: {}", placed.len());
    let doc = h.doc();
    let mut spoke = 0;
    for (key, c) in placed {
        let group = doc.devices[&c].device_type.group();
        assert_eq!(group, key.device_group(), "{key}: the device has its starter's type");
        if let Some(want) = predicates_say(&doc, c) {
            spoke += 1;
            assert_eq!(group, want, "{key}: the predicates say {want:?}");
        }
    }
    assert!(spoke >= 40, "the predicates spoke for {spoke} devices");
}

/// A8: placing a symbol as a new device gives the symbol's type; adding another kind of symbol to
/// that device keeps it.
#[test]
fn the_symbol_declares_the_type_and_joining_keeps_it() {
    let mut h = H::new(0x72_b8);
    let k = id::<DeviceId>(&h.run("place", json!({"symbol": "coil", "at": [40, 40], "tag": "-K01"})).unwrap(), "component");
    let q = id::<DeviceId>(&h.run("place", json!({"symbol": "mcb1", "at": [80, 40]})).unwrap(), "component");
    let doc = h.doc();
    assert_eq!(doc.devices[&k].device_type.group(), DeviceGroup::Switching);
    assert_eq!(doc.devices[&q].device_type.group(), DeviceGroup::Protection);
    // A float switch's contact placed with -K01's tag joins -K01, which stays Switching.
    let joined = id::<DeviceId>(&h.run("place", json!({"symbol": "floatno", "at": [120, 40], "tag": "-K01"})).unwrap(), "component");
    assert_eq!(joined, k);
    assert_eq!(h.doc().devices[&k].device_type.group(), DeviceGroup::Switching);
    // The starter's definition declares it.
    let lamp = STARTERS.iter().find(|s| s.key == StarterKey::Lamp).unwrap();
    assert_eq!(lamp.definition().unwrap().device_type.group(), DeviceGroup::Indicator);
}

/// Design 072 F5 (A2 hand-over): a project symbol's tag prefix follows its key when an edit
/// renames it and its old version goes; a prefix naming no project symbol is a read error.
#[test]
fn a_project_prefix_follows_a_renamed_symbol_key() {
    use ecad_model::prefix::PrefixOf;
    let mut h = H::new(0x72_b9);
    let shape = json!({"kind": "polyline", "vertices": [[-2.5, -7.5], [2.5, -7.5], [2.5, 7.5], [-2.5, 7.5]], "closed": true});
    let symbol = |key: &str| {
        json!({
            "name": "Float switch", "key": key, "graphics": [shape.clone()],
            "points": [{"key": "1", "at": [0, 5]}, {"key": "2", "at": [0, -5]}],
            "slots": [{"text": "tag", "at": [-4, 0]}],
            "defaults": {"prefix": "B", "description": "Float switch", "join": "device"},
        })
    };
    let def: ecad_model::DefinitionId = id(&h.run("symbolnew", symbol("float1")).unwrap(), "definition");
    h.run("setdefault", json!({"key": "symbol.float1.prefix", "value": "S"})).unwrap();
    assert!(h.doc().settings.tag_prefixes.contains_key(&PrefixOf::Project("float1".into())));
    let mut edit = symbol("float2");
    edit["definition"] = json!(def.hex());
    h.run("symboledit", edit).unwrap();
    let prefixes = h.doc().settings.tag_prefixes;
    assert!(!prefixes.contains_key(&PrefixOf::Project("float1".into())), "{prefixes:?}");
    assert_eq!(prefixes.get(&PrefixOf::Project("float2".into())).map(|p| p.to_string()), Some("S".into()));
    // Nothing names a project symbol that is not there: the open check finds nothing.
    assert!(ecad_electrical::words_check::check(&h.doc()).is_empty());
}

/// Step 2b (design 072 §2.1): a device's description, part number, maker, article number and type
/// text are written as one `ModifyDevice` (`setprop`, `article`, `bomitem`), never into its bag.
#[test]
fn text_fields_are_written_as_modify_device() {
    use ecad_model::{Op, TextField};
    let mut h = H::new(7202);
    let c: DeviceId = id(&crate::common::place(&mut h, "coil", (100.0, 100.0)), "component");
    let placed = h.doc().devices[&c].clone();
    assert_eq!(placed.text(TextField::Description), Some("Contactor / relay coil"), "placing writes the symbol's name as the description");
    for f in TextField::ALL {
        h.run("setprop", json!({"component": c.hex(), "key": f.key(), "value": format!("v-{}", f.key())})).unwrap();
        let ops = h.p(|p| p.ledger.log().last().unwrap().ops.clone());
        assert!(matches!(ops.as_slice(), [Op::ModifyDevice { .. }]), "{}: {ops:?}", f.key());
    }
    let d = h.doc().devices[&c].clone();
    assert!(TextField::ALL.iter().all(|f| d.text(*f).is_some()) && d.properties.is_empty(), "{d:?}");
    // `article` writes the device's article number on every part with that part number.
    h.run("article", json!({"part_number": "v-part_number", "article_number": "A-77"})).unwrap();
    assert_eq!(h.doc().devices[&c].text(TextField::ArticleNumber), Some("A-77"));
    // An item's description is a field too, and can't be cleared.
    let item: DeviceId = id(&h.run("bomitem", json!({"description": "Trunking", "qty": "2"})).unwrap(), "item");
    assert_eq!(h.doc().devices[&item].text(TextField::Description), Some("Trunking"));
    h.run("bomitem", json!({"item": item.hex(), "set": {"maker": "Lapp"}})).unwrap();
    assert_eq!(h.doc().devices[&item].text(TextField::Manufacturer), Some("Lapp"));
    assert!(h.run("setprop", json!({"component": item.hex(), "key": "description", "value": null})).is_err());
    // Too long for the field: refused with its bound.
    let e = h.run("setprop", json!({"component": c.hex(), "key": "part_number", "value": "9".repeat(65)})).unwrap_err();
    assert!(e.contains("at most 64"), "{e}");
}

/// Step 2c (design 072 §6.3): a typed location or function is written as one `ModifyDevice`
/// with `fixed` set; a box's proposal for an unset device writes it unfixed; the bag holds neither.
#[test]
fn location_and_function_are_written_as_modify_device() {
    use ecad_model::Op;
    let mut h = H::new(7203);
    let c: DeviceId = id(&crate::common::place(&mut h, "coil", (100.0, 160.0)), "component");
    for key in ["location", "function"] {
        let value = if key == "location" { "+CP1" } else { "=F1" };
        h.run("setprop", json!({"component": c.hex(), "key": key, "value": value})).unwrap();
        let ops = h.p(|p| p.ledger.log().last().unwrap().ops.clone());
        assert!(matches!(ops.as_slice(), [Op::ModifyProject { .. }, Op::ModifyDevice { .. }]), "{key}: {ops:?}");
    }
    let d = h.doc().devices[&c].clone();
    assert!(d.location.is_some() && d.location_fixed && d.function.is_some() && d.function_fixed, "{d:?}");
    assert!(!d.properties.contains_key("location") && !d.properties.contains_key("function"), "{d:?}");
    // Clearing clears the flag too.
    h.run("setprop", json!({"component": c.hex(), "key": "function", "value": null})).unwrap();
    let d = h.doc().devices[&c].clone();
    assert_eq!((d.function, d.function_fixed), (None, false));
    // A box drawn around an unset device proposes its value: set, not fixed.
    let other: DeviceId = id(&crate::common::place(&mut h, "lamp", (100.0, 100.0)), "component");
    h.run("locationbox", json!({"location": "+CP2", "corners": [[20, 60], [200, 220]]})).unwrap();
    let o = h.doc().devices[&other].clone();
    assert!(o.location.is_some() && !o.location_fixed, "{o:?}");
    // The device typed +CP1 keeps it (fixed) and is asked about, not moved.
    assert!(h.doc().devices[&c].location_fixed);
    assert_ne!(h.doc().devices[&c].location, o.location);
}

/// Step 3, Connector (design 072 §2.2): placing a connector stores its symbol's family; `setprop`
/// of its pins, pin names, a half's part and an ignored row writes one `ModifyDevice` each; a bad
/// value is refused with words.
#[test]
fn connector_fields_are_written_as_modify_device() {
    use ecad_model::Op;
    use ecad_model::device::{ConnectorCheck, ConnectorFamily, DeviceType};
    let mut h = H::new(7204);
    let c: DeviceId = id(&crate::common::place(&mut h, "dsub9s", (100.0, 100.0)), "component");
    let family = |h: &mut H| match &h.doc().devices[&c].device_type {
        DeviceType::Connector(x) => Some(x.family),
        _ => None,
    };
    assert_eq!(family(&mut h), Some(ConnectorFamily::Dsub), "the symbol's family is stored");
    for (key, value) in
        [("pins", "8"), ("pin_names", "A,B,C"), ("half.s.maker", "Acme"), ("half.p.part_number", "PLG-9"), ("ignore.connector-size", "yes")]
    {
        h.run("setprop", json!({"component": c.hex(), "key": key, "value": value})).unwrap();
        let ops = h.p(|p| p.ledger.log().last().unwrap().ops.clone());
        assert!(matches!(ops.as_slice(), [Op::ModifyDevice { .. }]), "{key}: {ops:?}");
        assert_eq!(h.doc().devices[&c].value(key).as_deref(), Some(value), "{key}");
    }
    let d = h.doc().devices[&c].clone();
    assert!(d.properties.is_empty(), "{d:?}");
    let DeviceType::Connector(x) = &d.device_type else { panic!("{d:?}") };
    assert!(x.ignored.contains(&ConnectorCheck::Size));
    for (key, value, want) in [
        ("pins", "65", "1 to 64"),
        ("pin_names", "A,a", "twice"),
        ("half.x.maker", "Acme", "no half"),
        ("half.s.colour", "red", "field is one of"),
        ("ignore.wire-colour", "yes", "connector ignores"),
    ] {
        let e = h.run("setprop", json!({"component": c.hex(), "key": key, "value": value})).unwrap_err();
        assert!(e.contains(want), "{key}: {e}");
    }
    // Not a connector: refused.
    let k: DeviceId = id(&crate::common::place(&mut h, "coil", (200.0, 100.0)), "component");
    assert!(h.run("setprop", json!({"component": k.hex(), "key": "pins", "value": "4"})).is_err());
}

/// Step 3, Protection (design 072 §2.2, "Ratings"): `setprop rating` reads the text into the
/// group's typed fields and says what it read; ambiguous text is asked, never guessed; text that is
/// no rating is the note, and the device then has the Checks row `no-rating`.
#[test]
fn a_rating_is_read_into_typed_fields() {
    use ecad_model::Op;
    use ecad_model::device::{DeviceType, Protection, TripCurve};
    let mut h = H::new(7205);
    let q: DeviceId = id(&crate::common::place(&mut h, "mcb1", (100.0, 100.0)), "component");
    let r = h.run("setprop", json!({"component": q.hex(), "key": "rating", "value": "B16"})).unwrap();
    assert_eq!(r["message"], json!("Read: curve B, 16 A."), "{r}");
    assert_eq!(r["value"], json!("curve B, 16 A"));
    let ops = h.p(|p| p.ledger.log().last().unwrap().ops.clone());
    assert!(matches!(ops.as_slice(), [Op::ModifyDevice { .. }]), "{ops:?}");
    let DeviceType::Protection(Protection::Mcb(b)) = &h.doc().devices[&q].device_type else { panic!() };
    assert_eq!((b.curve, b.rated_current.map(|a| a.0)), (Some(TripCurve::B), Some(16.0)));
    assert!(h.doc().devices[&q].properties.is_empty());
    // A bare number on a breaker asks for the curve; a residual current is refused.
    let e = h.run("setprop", json!({"component": q.hex(), "key": "rating", "value": "16"})).unwrap_err();
    assert!(e.contains("curve"), "{e}");
    let e = h.run("setprop", json!({"component": q.hex(), "key": "rating", "value": "C16 30mA"})).unwrap_err();
    assert!(e.contains("RCBO"), "{e}");
    // No rating: a note, and a warning.
    let no_rating = |h: &mut H| h.run("checks", json!({})).unwrap().to_string().contains("no-rating");
    assert!(!no_rating(&mut h));
    let r = h.run("setprop", json!({"component": q.hex(), "key": "rating", "value": "see datasheet"})).unwrap();
    assert!(r["message"].as_str().unwrap().contains("note"), "{r}");
    assert_eq!(h.doc().devices[&q].rating_note.as_ref().map(|n| n.to_string()).as_deref(), Some("see datasheet"));
    assert!(no_rating(&mut h));
    // A fuse bank's fuse.
    let f: DeviceId = id(&crate::common::place(&mut h, "fusebank4", (200.0, 100.0)), "component");
    let r = h.run("setprop", json!({"component": f.hex(), "key": "rating.2", "value": "6.3A T"})).unwrap();
    assert_eq!(r["value"], json!("6.3 A, T"), "{r}");
    assert!(h.run("setprop", json!({"component": f.hex(), "key": "rating.9", "value": "4 A"})).is_err());
    assert!(h.run("setprop", json!({"component": f.hex(), "key": "rating", "value": "4 A"})).is_err());
}

/// Step 3, Switching (design 072 §2.2, "Ratings"): the symbol says what operates the device;
/// `setprop rating` reads a coil's voltage and power and the AC-3 rating into typed fields; a
/// voltage without AC or DC, or a current without AC-3, is asked; a manual switch has no coil.
#[test]
fn a_switching_rating_is_read_into_typed_fields() {
    use ecad_model::Op;
    use ecad_model::device::{DeviceType, OperatorKind};
    use ecad_model::units::Current;
    let mut h = H::new(7206);
    let k: DeviceId = id(&crate::common::place(&mut h, "coil", (100.0, 100.0)), "component");
    let kind = |h: &mut H, c: DeviceId| match &h.doc().devices[&c].device_type {
        DeviceType::Switching(s) => Some(s.operator_kind()),
        _ => None,
    };
    assert_eq!(kind(&mut h, k), Some(OperatorKind::Coil));
    // An unrated coil has the Checks row `no-rating`; a rated one does not.
    let no_rating = |h: &mut H| h.run("checks", json!({})).unwrap().to_string().contains("no-rating");
    assert!(no_rating(&mut h));
    let r = h.run("setprop", json!({"component": k.hex(), "key": "rating", "value": "24VDC AC-3 9A"})).unwrap();
    assert_eq!(r["message"], json!("Read: coil 24 V DC, AC-3 9 A."), "{r}");
    assert_eq!(r["value"], json!("coil 24 V DC, AC-3 9 A"));
    let ops = h.p(|p| p.ledger.log().last().unwrap().ops.clone());
    assert!(matches!(ops.as_slice(), [Op::ModifyDevice { .. }]), "{ops:?}");
    let DeviceType::Switching(s) = &h.doc().devices[&k].device_type else { panic!() };
    assert_eq!(s.control().and_then(|c| c.voltage).map(|v| (v.volts.0, v.current)), Some((24.0, Current::Dc)));
    assert_eq!(s.ac3_current.map(|a| a.0), Some(9.0));
    assert!(h.doc().devices[&k].properties.is_empty());
    assert!(!no_rating(&mut h));
    // Ambiguous text is asked, never guessed.
    let e = h.run("setprop", json!({"component": k.hex(), "key": "rating", "value": "24 V"})).unwrap_err();
    assert!(e.contains("AC or DC"), "{e}");
    let e = h.run("setprop", json!({"component": k.hex(), "key": "rating", "value": "9 A"})).unwrap_err();
    assert!(e.contains("AC-3"), "{e}");
    // A push button is manual: no coil, and unrated is no warning.
    let s: DeviceId = id(&crate::common::place(&mut h, "pbno", (200.0, 100.0)), "component");
    assert_eq!(kind(&mut h, s), Some(OperatorKind::Manual));
    assert!(!no_rating(&mut h));
    let e = h.run("setprop", json!({"component": s.hex(), "key": "rating", "value": "24 V DC"})).unwrap_err();
    assert!(e.contains("manual switch has no coil"), "{e}");
    let r = h.run("setprop", json!({"component": s.hex(), "key": "rating", "value": "AC-3 25 A"})).unwrap();
    assert_eq!(r["value"], json!("AC-3 25 A"), "{r}");
    // An SSR's input.
    let u: DeviceId = id(&crate::common::place(&mut h, "ssrin", (300.0, 100.0)), "component");
    assert_eq!(kind(&mut h, u), Some(OperatorKind::SolidState));
    let r = h.run("setprop", json!({"component": u.hex(), "key": "rating", "value": "24 V DC"})).unwrap();
    assert_eq!(r["value"], json!("input 24 V DC"), "{r}");
}

/// Step 3, Sensor (design 072 §2.2 row 4, "Ratings"): the symbol says what a sensor senses and
/// outputs; `setprop rating` reads a PNP sensor's supply into a typed field; a voltage without AC
/// or DC is asked; a contact or a thermocouple keeps its text as the note and is not warned about.
#[test]
fn a_sensor_rating_is_read_into_typed_fields() {
    use ecad_model::Op;
    use ecad_model::device::{DeviceType, Output, Sensing};
    use ecad_model::units::Current;
    let mut h = H::new(7207);
    let kind = |h: &mut H, c: DeviceId| match &h.doc().devices[&c].device_type {
        DeviceType::Sensor(s) => Some((s.sensing, s.output)),
        _ => None,
    };
    for (symbol, want) in [
        ("prox3", (Sensing::Proximity, Output::Pnp)),
        ("floatno", (Sensing::Float, Output::Contact)),
        ("rollernc", (Sensing::Limit, Output::Contact)),
        ("interlockno", (Sensing::Interlock, Output::Contact)),
        ("tc", (Sensing::Temperature, Output::Thermocouple)),
    ] {
        let mut g = H::new(7208);
        let c: DeviceId = id(&crate::common::place(&mut g, symbol, (100.0, 100.0)), "component");
        assert_eq!(kind(&mut g, c), Some(want), "{symbol}");
    }
    let no_rating = |h: &mut H| h.run("checks", json!({})).unwrap().to_string().contains("no-rating");
    // A contact and a thermocouple are not warned about; their text is the note.
    let f: DeviceId = id(&crate::common::place(&mut h, "floatno", (200.0, 100.0)), "component");
    let t: DeviceId = id(&crate::common::place(&mut h, "tc", (300.0, 100.0)), "component");
    assert!(!no_rating(&mut h));
    let r = h.run("setprop", json!({"component": f.hex(), "key": "rating", "value": "250 V AC 5 A"})).unwrap();
    assert_eq!(r["value"], json!("250 V AC 5 A"), "{r}");
    let r = h.run("setprop", json!({"component": t.hex(), "key": "rating", "value": "K"})).unwrap();
    assert_eq!(r["value"], json!("K"), "{r}");
    assert!(h.doc().devices[&t].rating_note.is_some());
    // An unrated PNP sensor has the Checks row `no-rating`; a rated one does not.
    let b: DeviceId = id(&crate::common::place(&mut h, "prox3", (100.0, 100.0)), "component");
    assert!(no_rating(&mut h));
    let r = h.run("setprop", json!({"component": b.hex(), "key": "rating", "value": "24VDC"})).unwrap();
    assert_eq!(r["message"], json!("Read: 24 V DC."), "{r}");
    assert_eq!(r["value"], json!("24 V DC"));
    let ops = h.p(|p| p.ledger.log().last().unwrap().ops.clone());
    assert!(matches!(ops.as_slice(), [Op::ModifyDevice { .. }]), "{ops:?}");
    let DeviceType::Sensor(s) = &h.doc().devices[&b].device_type else { panic!() };
    assert_eq!(s.supply.map(|v| (v.volts.0, v.current)), Some((24.0, Current::Dc)));
    assert!(h.doc().devices[&b].properties.is_empty());
    assert!(!no_rating(&mut h));
    // Ambiguous text is asked, never guessed.
    let e = h.run("setprop", json!({"component": b.hex(), "key": "rating", "value": "24 V"})).unwrap_err();
    assert!(e.contains("AC or DC"), "{e}");
    let e = h.run("setprop", json!({"component": b.hex(), "key": "rating", "value": "24"})).unwrap_err();
    assert!(e.contains("a unit for 24"), "{e}");
}

/// Step 3, TerminalStrip (design 072 §2.5): `setprop rating.<p>` reads a fuse position's rating
/// into the strip's typed field (one `ModifyDevice`, nothing in its properties); text that is no
/// fuse rating, or a position that is no fuse position, is refused; the Checks row reads the
/// field; clearing works.
#[test]
fn a_strip_fuse_position_rating_is_typed() {
    use ecad_model::Op;
    use ecad_model::device::{DeviceType, FuseClass};
    let mut h = H::new(7209);
    for i in 1..=3 {
        let x = 10.0 * f64::from(i);
        h.run("wire", json!({"points": [[x, 0.0], [x, 40.0]]})).unwrap();
    }
    let f = h.run("terminals", json!({"type": "FUSE", "level": "1", "from": [0, 20], "to": [40, 20]})).unwrap();
    let x1: DeviceId = id(&f, "strip");
    let no_fuse_rating = |h: &mut H| h.run("checks", json!({})).unwrap().to_string().contains("no-fuse-rating");
    assert!(no_fuse_rating(&mut h));
    let r = h.run("setprop", json!({"component": x1.hex(), "key": "rating.2", "value": "6,3A T"})).unwrap();
    assert_eq!(r["message"], json!("Read: 6.3 A, T."), "{r}");
    let ops = h.p(|p| p.ledger.log().last().unwrap().ops.clone());
    assert!(matches!(ops.as_slice(), [Op::ModifyDevice { .. }]), "{ops:?}");
    let strip = h.doc().devices[&x1].clone();
    let DeviceType::TerminalStrip(s) = &strip.device_type else { panic!("{:?}", strip.device_type) };
    let fuse = s.fuse("2").unwrap();
    assert_eq!((fuse.rated_current.map(|a| a.0), fuse.class), (Some(6.3), Some(FuseClass::T)));
    assert!(strip.properties.keys().all(|k| !k.starts_with("rating.")), "{:?}", strip.properties);
    assert_eq!(strip.value("rating.2").as_deref(), Some("6.3 A, T"));
    // Text that is no fuse rating, and a position that is no fuse position, are refused.
    let e = h.run("setprop", json!({"component": x1.hex(), "key": "rating.1", "value": "see datasheet"})).unwrap_err();
    assert!(e.contains("is not a fuse rating"), "{e}");
    let e = h.run("setprop", json!({"component": x1.hex(), "key": "rating.1", "value": "B16"})).unwrap_err();
    assert!(e.contains("rating 'B16'"), "{e}");
    let e = h.run("setprop", json!({"component": x1.hex(), "key": "rating.9", "value": "4A"})).unwrap_err();
    assert!(e.contains("is not a fuse position"), "{e}");
    // Every fuse position rated: no Checks row.
    for p in ["1", "3"] {
        h.run("setprop", json!({"component": x1.hex(), "key": format!("rating.{p}"), "value": "4 A"})).unwrap();
    }
    assert!(!no_fuse_rating(&mut h));
    // Clearing works, also on a label that is no fuse position.
    h.run("setprop", json!({"component": x1.hex(), "key": "rating.3", "value": ""})).unwrap();
    h.run("setprop", json!({"component": x1.hex(), "key": "rating.9", "value": ""})).unwrap();
    assert_eq!(h.doc().devices[&x1].value("rating.3"), None);
    assert!(no_fuse_rating(&mut h));
}

/// Step 3, Motor (design 072 §2.2 row 5, "Ratings"): the symbol says a motor's phases;
/// `setprop rating` reads its nameplate into typed fields; a bare number is asked, a voltage
/// refused, and other text kept as the note; an unrated motor is warned about.
#[test]
fn a_motor_rating_is_read_into_typed_fields() {
    use ecad_model::Op;
    use ecad_model::device::{DeviceType, Phases, Starting};
    use ecad_model::units::{Amps, Watts};
    let mut h = H::new(7209);
    let phases = |h: &mut H, c: DeviceId| match &h.doc().devices[&c].device_type {
        DeviceType::Motor(m) => Some(m.phases),
        _ => None,
    };
    let no_rating = |h: &mut H| h.run("checks", json!({})).unwrap().to_string().contains("no-rating");
    let m1: DeviceId = id(&crate::common::place(&mut h, "motor1", (200.0, 100.0)), "component");
    assert_eq!(phases(&mut h, m1), Some(Phases::Single));
    let m: DeviceId = id(&crate::common::place(&mut h, "motor3", (100.0, 100.0)), "component");
    assert_eq!(phases(&mut h, m), Some(Phases::Three));
    assert!(no_rating(&mut h));
    let r = h.run("setprop", json!({"component": m.hex(), "key": "rating", "value": "7,5kW 15.2A Y-D"})).unwrap();
    assert_eq!(r["message"], json!("Read: 7.5 kW, 15.2 A, star-delta."), "{r}");
    assert_eq!(r["value"], json!("7.5 kW, 15.2 A, star-delta"));
    let ops = h.p(|p| p.ledger.log().last().unwrap().ops.clone());
    assert!(matches!(ops.as_slice(), [Op::ModifyDevice { .. }]), "{ops:?}");
    let DeviceType::Motor(got) = &h.doc().devices[&m].device_type else { panic!() };
    assert_eq!((got.rated_power, got.rated_current, got.starting), (Some(Watts(7500.0)), Some(Amps(15.2)), Some(Starting::StarDelta)));
    assert!(h.doc().devices[&m].properties.is_empty());
    let r = h.run("setprop", json!({"component": m1.hex(), "key": "rating", "value": "0.37 kW"})).unwrap();
    assert_eq!(r["value"], json!("0.37 kW"), "{r}");
    assert!(!no_rating(&mut h));
    // Ambiguous text is asked, never guessed; a voltage is refused; other text is the note.
    let e = h.run("setprop", json!({"component": m.hex(), "key": "rating", "value": "7.5"})).unwrap_err();
    assert!(e.contains("kW (rated power) or A"), "{e}");
    let e = h.run("setprop", json!({"component": m.hex(), "key": "rating", "value": "7.5 kW 400 V"})).unwrap_err();
    assert!(e.contains("no voltage"), "{e}");
    let r = h.run("setprop", json!({"component": m.hex(), "key": "rating", "value": "IE3"})).unwrap();
    assert_eq!(r["value"], json!("IE3"), "{r}");
    assert!(h.doc().devices[&m].rating_note.is_some());
    assert!(no_rating(&mut h));
}

/// Step 3, Load (design 072 §2.2 row 6, "Ratings"): heaters and valves are loads; `setprop rating`
/// reads the rated power into its typed field; a bare number is asked, a voltage refused, and other
/// text kept as the note; an unrated load is warned about.
#[test]
fn a_load_rating_is_read_into_typed_fields() {
    use ecad_model::Op;
    use ecad_model::device::{DeviceType, Load};
    use ecad_model::units::Watts;
    let mut h = H::new(7210);
    let no_rating = |h: &mut H| h.run("checks", json!({})).unwrap().to_string().contains("no-rating");
    let v: DeviceId = id(&crate::common::place(&mut h, "valve", (200.0, 100.0)), "component");
    assert_eq!(h.doc().devices[&v].device_type, DeviceType::Load(Load::default()));
    let e: DeviceId = id(&crate::common::place(&mut h, "heater", (100.0, 100.0)), "component");
    assert!(no_rating(&mut h));
    let r = h.run("setprop", json!({"component": e.hex(), "key": "rating", "value": "3000 W"})).unwrap();
    assert_eq!(r["message"], json!("Read: 3 kW."), "{r}");
    assert_eq!(r["value"], json!("3 kW"));
    let ops = h.p(|p| p.ledger.log().last().unwrap().ops.clone());
    assert!(matches!(ops.as_slice(), [Op::ModifyDevice { .. }]), "{ops:?}");
    assert_eq!(h.doc().devices[&e].device_type, DeviceType::Load(Load { rated_power: Some(Watts(3000.0)) }));
    assert!(h.doc().devices[&e].properties.is_empty());
    let r = h.run("setprop", json!({"component": v.hex(), "key": "rating", "value": "5W"})).unwrap();
    assert_eq!(r["value"], json!("5 W"), "{r}");
    assert!(!no_rating(&mut h));
    // Ambiguous text is asked, never guessed; a voltage is refused; other text is the note.
    let err = h.run("setprop", json!({"component": v.hex(), "key": "rating", "value": "5"})).unwrap_err();
    assert!(err.contains("W or kW"), "{err}");
    let err = h.run("setprop", json!({"component": v.hex(), "key": "rating", "value": "24 V DC"})).unwrap_err();
    assert!(err.contains("no voltage"), "{err}");
    let r = h.run("setprop", json!({"component": v.hex(), "key": "rating", "value": "see datasheet"})).unwrap();
    assert_eq!(r["value"], json!("see datasheet"), "{r}");
    assert!(h.doc().devices[&v].rating_note.is_some());
    assert!(no_rating(&mut h));
}
