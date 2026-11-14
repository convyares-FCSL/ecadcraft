//! Brief 050a (design 050 §12, checks 1–9): the standard, the style catalogues and their
//! commands, what the page draws from them, the style rows of Checks, the update map, and a
//! hostile document.

use crate::common::{H, id};

use cadcraft_engine::doc::EntityKind;
use cadcraft_engine::doc::color::Color;
use ecad_cadcraft::defaults::{self, Scope, Source};
use ecad_cadcraft::styles;
use ecad_cadcraft::text_style::{TextKind, text_style};
use ecad_model::style::{StyleLayer, StyleRecord, StyleRef};
use ecad_model::{ComponentId, ObjectRef, Op, PageId};
use serde_json::json;

const SEED: u64 = 50;

fn fixture() -> (H, PageId, ComponentId) {
    let mut h = H::new(SEED);
    let page = h.first_page();
    let r = h.run("place", json!({"symbol": "coil", "at": [100, 100]})).unwrap();
    (h, page, id(&r, "component"))
}

/// The page's tag texts of `c` (their heights and colours).
fn tag_texts(h: &mut H, page: PageId, tag_prefix: &str) -> Vec<(f64, Color)> {
    let ents = h.pv(|p, pg| pg.entities_of(p, page));
    ents.iter()
        .filter(|e| e.common.layer == ecad_cadcraft::devices::TAG_LAYER)
        .filter_map(|e| match &e.kind {
            EntityKind::Text(t) if t.value.starts_with(tag_prefix) => Some((t.height, e.common.color)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_new_project_uses_iec_and_lists_the_built_in_styles() {
    let (mut h, ..) = fixture();
    assert_eq!(h.run("standard", json!({})).unwrap(), json!({"name": "IEC (built in)", "id": null}));
    let text = h.run("styles", json!({"catalogue": "text"})).unwrap();
    // Brief 051 (FP1): two more, `channel_text` and `address`.
    assert_eq!(text["styles"].as_array().unwrap().len(), 23, "one built-in style per 031/049 kind and 051's two");
    let tag = &text["styles"][0];
    assert_eq!((tag["id"].as_str(), tag["layer"].as_str()), (Some("tag"), Some("builtin")));
    assert_eq!(tag["fields"]["height"]["value"], json!(2.5));
    assert_eq!(tag["fields"]["weight"]["value"], json!("bold"));
    assert_eq!(tag["fields"]["colour"]["value"], json!("none"));
    for (cat, id) in [("location", "box"), ("frame", "sheet")] {
        let v = h.run("styles", json!({"catalogue": cat})).unwrap();
        assert_eq!(v["styles"][0]["id"], json!(id), "{cat}");
    }
    // 031's drift test, kept: an empty project's text styles are the built-ins (text_style).
    let doc = h.doc();
    assert_eq!(text_style(&doc, TextKind::Tag).height, 2.5);
    assert_eq!(text_style(&doc, TextKind::Cover).height, 10.0);
}

#[test]
fn a_standard_value_reaches_new_devices_and_undoes_back_to_iec() {
    let (mut h, ..) = fixture();
    let refused = h.run("setdefault", json!({"key": "symbol.coil.prefix", "level": "standard", "value": "Q"})).unwrap_err();
    assert!(refused.contains("Make a standard first"), "{refused}");
    let v = h.run("standard", json!({"new": "House"})).unwrap();
    assert_eq!(v["standard"]["name"], json!("House"));
    h.run("setdefault", json!({"key": "symbol.coil.prefix", "level": "standard", "value": "Q"})).unwrap();
    let e = defaults::find("symbol.coil.prefix").unwrap();
    let doc = h.doc();
    let chain = defaults::chain(&doc, e, Scope::project());
    assert!(chain.iter().any(|(s, v)| *s == Source::Standard && v.is_some()), "{chain:?}");
    assert_eq!(defaults::resolve(&doc, e, Scope::project()).from, Source::Standard);
    let r = h.run("place", json!({"symbol": "coil", "at": [150, 100]})).unwrap();
    let c: ComponentId = id(&r, "component");
    let tag = h.p(|p| p.ledger.document().components[&c].properties.get("tag").cloned()).unwrap();
    assert!(tag.starts_with("-Q"), "{tag}");
    // Undo the place, the value and the standard: IEC (built in) again.
    for _ in 0..3 {
        h.cmdline("u");
    }
    assert!(h.doc().standard.is_none());
}

#[test]
fn a_text_style_change_restyles_every_tag_in_one_transaction() {
    let (mut h, page, _) = fixture();
    let before = h.head();
    h.run("setstyle", json!({"catalogue": "text", "id": "tag", "fields": {"colour": "RD", "height": 3}})).unwrap();
    assert_eq!(h.head(), before + 1, "one transaction");
    let tags = tag_texts(&mut h, page, "-K");
    assert!(!tags.is_empty());
    assert!(tags.iter().all(|(ht, c)| (*ht - 3.0).abs() < 1e-9 && matches!(c, Color::True(_))), "{tags:?}");
    // Mono print prints ink; a colour print keeps the tag's colour.
    let doc = h.doc();
    let fan = Default::default();
    let (mono, _) = ecad_cadcraft::printbook::build_pending(&doc, &[page], &fan, 0.01, None, false).unwrap();
    assert!(mono.model.iter().all(|e| !matches!(e.common.color, Color::True(_))), "mono is ink");
    let (colour, _) = ecad_cadcraft::printbook::build_in(&doc, &[page], &fan, 0.01, None, false, true).unwrap();
    assert!(
        colour.model.iter().any(|e| matches!(&e.kind, EntityKind::Text(t) if t.value.starts_with("-K")) && matches!(e.common.color, Color::True(_)))
    );
    // Refusals name the field's range.
    let e = h.run("setstyle", json!({"catalogue": "text", "id": "tag", "fields": {"height": 20}})).unwrap_err();
    assert!(e.contains("height is 0.5 to 10 mm (cap height)"), "{e}");
    let e = h.run("setstyle", json!({"catalogue": "location", "id": "box", "fields": {"line_type": "wavy"}})).unwrap_err();
    assert!(e.contains("line_type is one of dashed, continuous, dashdot"), "{e}");
    h.cmdline("u");
    assert!(tag_texts(&mut h, page, "-K").iter().all(|(ht, c)| (*ht - 2.5).abs() < 1e-9 && !matches!(c, Color::True(_))));
}

#[test]
fn an_own_style_for_one_device_then_deleted_with_a_replacement() {
    let (mut h, page, k1) = fixture();
    let r = h.run("place", json!({"symbol": "coil", "at": [200, 100]})).unwrap();
    let k2: ComponentId = id(&r, "component");
    let v = h.run("setstyle", json!({"catalogue": "text", "new": true, "name": "Tag large", "copy_of": "tag", "fields": {"height": 5}})).unwrap();
    let large = v["id"].as_str().unwrap().to_string();
    assert!(large.starts_with('#'));
    h.run("setdefault", json!({"key": "text.tag.style", "level": "object", "component": k1.hex(), "value": "Tag large"})).unwrap();
    let tag_of = |h: &mut H, c: ComponentId| h.p(|p| p.ledger.document().components[&c].properties["tag"].clone());
    let (t1, t2) = (tag_of(&mut h, k1), tag_of(&mut h, k2));
    let tags = tag_texts(&mut h, page, "-K");
    let height = |t: &str, h: &mut H| tag_texts(h, page, t).first().map(|x| x.0);
    assert_eq!(height(&t1, &mut h), Some(5.0), "{tags:?}");
    assert_eq!(height(&t2, &mut h), Some(2.5));
    // Deleting a style in use needs a replacement.
    let e = h.run("deletestyle", json!({"catalogue": "text", "name": "Tag large"})).unwrap_err();
    assert!(e.contains("1 device uses Tag large; give replace_with"), "{e}");
    let before = h.head();
    h.run("deletestyle", json!({"catalogue": "text", "name": "Tag large", "replace_with": "tag"})).unwrap();
    assert_eq!(h.head(), before + 1);
    assert_eq!(height(&t1, &mut h), Some(2.5));
    let doc = h.doc();
    assert_eq!(doc.components[&k1].properties.get("text.tag.style").map(String::as_str), Some("tag"));
    assert!(!styles::exists(&doc, "text", &StyleRef::parse(&large).unwrap()));
    // Built-in styles stay.
    assert!(h.run("deletestyle", json!({"catalogue": "text", "id": "tag"})).unwrap_err().contains("cannot be deleted"));
}

#[test]
fn promote_moves_a_project_style_to_the_standard_with_the_same_id() {
    let (mut h, page, k1) = fixture();
    let v = h.run("setstyle", json!({"catalogue": "text", "new": true, "name": "Tag large", "fields": {"height": 5}})).unwrap();
    let large = v["id"].as_str().unwrap().to_string();
    h.run("setdefault", json!({"key": "text.tag.style", "level": "object", "component": k1.hex(), "value": large})).unwrap();
    assert!(h.run("promote", json!({"catalogue": "text", "id": large})).unwrap_err().contains("Make a standard first"));
    h.run("standard", json!({"new": "House"})).unwrap();
    h.run("promote", json!({"catalogue": "text", "id": large})).unwrap();
    let doc = h.doc();
    let id = StyleRef::parse(&large).unwrap();
    assert!(styles::record(&doc, StyleLayer::Standard, "text", &id).is_some());
    assert!(styles::record(&doc, StyleLayer::Project, "text", &id).is_none());
    let tag = doc.components[&k1].properties["tag"].clone();
    assert_eq!(tag_texts(&mut h, page, &tag).first().map(|x| x.0), Some(5.0), "K1 still uses it");
    // A project value moves too; the standard's own differing value is refused.
    h.run("setdefault", json!({"key": "symbol.coil.prefix", "level": "project", "value": "KA"})).unwrap();
    h.run("promote", json!({"key": "symbol.coil.prefix"})).unwrap();
    let doc = h.doc();
    assert_eq!(doc.standard.as_ref().unwrap().values.get("symbol.coil.prefix").map(String::as_str), Some("KA"));
    assert_eq!(defaults::resolve(&doc, defaults::find("symbol.coil.prefix").unwrap(), Scope::project()).from, Source::Standard);
}

#[test]
fn back_to_iec_needs_a_replacement_for_a_standard_only_style_in_use() {
    let (mut h, _, k1) = fixture();
    h.run("standard", json!({"new": "House"})).unwrap();
    let v = h.run("setstyle", json!({"catalogue": "text", "new": true, "name": "Std tag", "layer": "standard"})).unwrap();
    let sid = v["id"].as_str().unwrap().to_string();
    h.run("setdefault", json!({"key": "text.tag.style", "level": "object", "component": k1.hex(), "value": sid})).unwrap();
    h.run("setdefault", json!({"key": "symbol.coil.prefix", "level": "standard", "value": "Q"})).unwrap();
    let e = h.run("standard", json!({"remove": true})).unwrap_err();
    assert!(e.contains("1 device uses Std tag"), "{e}");
    let before = h.head();
    let v = h.run("standard", json!({"remove": true, "replace_with": {"text": "tag"}})).unwrap();
    assert_eq!(h.head(), before + 1, "one transaction");
    assert_eq!(v["message"], json!("1 values and 1 styles went back to IEC; project values stay."));
    assert!(h.doc().standard.is_none());
    h.cmdline("u");
    assert_eq!(h.doc().standard.as_ref().map(|s| s.values.len()), Some(1), "undo restores the values");
}

#[test]
fn a_location_style_restyles_boxes_and_a_page_frame_style_one_page() {
    let (mut h, page, _) = fixture();
    h.run("locationbox", json!({"location": "CP1", "corners": [[20, 20], [140, 90]]})).unwrap();
    let boxes = |h: &mut H| {
        let ents = h.pv(|p, pg| pg.entities_of(p, page));
        ents.iter()
            .filter(|e| e.common.layer == ecad_cadcraft::locations::LAYER && !matches!(e.kind, EntityKind::Text(_)))
            .map(|e| (e.common.linetype.clone(), e.common.lineweight))
            .collect::<Vec<_>>()
    };
    assert_eq!(boxes(&mut h).first().map(|b| b.0.as_str()), Some("DASHED"), "nothing moves on landing");
    h.run("setstyle", json!({"catalogue": "location", "id": "box", "fields": {"line_type": "continuous", "line_weight": 0.5}})).unwrap();
    let b = boxes(&mut h);
    assert_eq!(b.first().map(|b| b.0.as_str()), Some("CONTINUOUS"), "{b:?}");
    assert_eq!(b.first().map(|b| b.1), Some(cadcraft_engine::doc::Lineweight::Mm100(50)));
    // The page still verifies (a reopened page is not unavailable).
    let ok = h.pv(|p, pg| {
        let d = pg.drawing_of(p, page).unwrap();
        ecad_cadcraft::verify::verify_page(p.ledger.document(), page, &d, &p.pages[&page].map).ok()
    });
    assert!(ok);
    // An own frame style chosen on one page changes only that page.
    let v = h
        .run("setstyle", json!({"catalogue": "frame", "new": true, "name": "Heavy", "fields": {"border_weight": 1.0, "grid_labels": "off"}}))
        .unwrap();
    let heavy = v["id"].as_str().unwrap().to_string();
    let p2: PageId = id(&h.run("newpage", json!({"title": "Two"})).unwrap(), "page");
    h.run("setdefault", json!({"key": "print.frame.style", "level": "page", "page": page.hex(), "value": heavy})).unwrap();
    let doc = h.doc();
    assert_eq!(styles::frame::frame_look(&doc, page).border, 100);
    assert!(!styles::frame::frame_look(&doc, page).grid_labels);
    assert_eq!(styles::frame::frame_look(&doc, p2).border, 50);
    let parts = ecad_cadcraft::frame::parts(&ecad_cadcraft::frame::FrameInput::of(&doc, page, &ecad_model::book::Book::of(&doc)));
    assert!(parts.iter().any(|p| p.weight == Some(100)));
}

#[test]
fn checks_report_unknown_styles_and_old_settings() {
    let (mut h, _, k1) = fixture();
    // An older build's text sizes: one row per kind of key, cleared by clearing the key.
    h.cfg(|p| {
        let ops = vec![
            Op::SetProperty { object: ObjectRef::Project, key: "text.tag.height".into(), before: None, after: Some("3".into()) },
            Op::SetProperty { object: ObjectRef::Project, key: "text.grid.height".into(), before: None, after: Some("4".into()) },
            Op::SetProperty {
                object: ObjectRef::Component(k1),
                key: "text.tag.style".into(),
                before: None,
                after: Some(format!("#{:032x}", 0xdead)),
            },
        ];
        p.ledger.commit(ecad_model::Author::User, "old", ops).unwrap();
    });
    let doc = h.doc();
    let rows = ecad_cadcraft::checks::all(&doc, 0.01);
    let old: Vec<_> = rows.iter().filter(|c| c.kind == ecad_cadcraft::checks::Kind::IgnoredOldSetting).collect();
    assert_eq!(old.len(), 1, "one row for heights");
    assert!(old[0].which.contains("Text heights set in an older build"), "{}", old[0].which);
    let unknown: Vec<_> = rows.iter().filter(|c| c.kind == ecad_cadcraft::checks::Kind::StyleUnknown).collect();
    assert_eq!(unknown.len(), 1);
    assert!(unknown[0].which.contains("not found; drawn as Tag"), "{}", unknown[0].which);
    assert_eq!(text_style(&doc, TextKind::Tag).height, 2.5, "the old value is ignored");
    h.run("projectprops", json!({"props": {"text.tag.height": null, "text.grid.height": null}})).unwrap();
    let rows = ecad_cadcraft::checks::all(&h.doc(), 0.01);
    assert!(!rows.iter().any(|c| c.kind == ecad_cadcraft::checks::Kind::IgnoredOldSetting));
}

#[test]
fn a_style_change_marks_only_the_pages_of_its_users() {
    let (mut h, page, k1) = fixture();
    let v = h.run("setstyle", json!({"catalogue": "text", "new": true, "name": "Tag large"})).unwrap();
    let large = StyleRef::parse(v["id"].as_str().unwrap()).unwrap();
    let doc = h.doc();
    let set = |id: StyleRef| Op::SetStyle {
        layer: StyleLayer::Project,
        catalogue: "text".into(),
        id,
        before: None,
        after: Some(StyleRecord { name: String::new(), fields: [("height".to_string(), "4".to_string())].into() }),
    };
    // Nobody uses it: no page.
    assert_eq!(styles::pages_touched(&doc, &set(large.clone())), Some(Default::default()));
    // The project's tag style: every page.
    assert_eq!(styles::pages_touched(&doc, &set(StyleRef::Builtin("tag".into()))), None);
    // One device uses it: that device's page.
    h.run("setdefault", json!({"key": "text.tag.style", "level": "object", "component": k1.hex(), "value": large.text()})).unwrap();
    let doc = h.doc();
    assert_eq!(styles::pages_touched(&doc, &set(large)), Some([page].into()));
    // The standard's header changes no page.
    assert_eq!(styles::pages_touched(&doc, &Op::SetStandard { before: None, after: None }), Some(Default::default()));
}

/// Design 050 §12 check 8 (hostile): 2,000 styles, 100 fields, a 10,000-character name, a
/// dangling reference and an unknown catalogue open, draw and report, with no panic.
#[test]
fn a_hostile_style_document_opens_draws_and_reports() {
    let (mut h, page, k1) = fixture();
    let long = "n".repeat(10_000);
    h.cfg(|p| {
        let mut ops = Vec::new();
        for i in 0..2_000u128 {
            ops.push(Op::SetStyle {
                layer: StyleLayer::Project,
                catalogue: "text".into(),
                id: StyleRef::Own(i + 1),
                before: None,
                after: Some(StyleRecord { name: format!("S{i}"), fields: Default::default() }),
            });
        }
        let fields = (0..100).map(|i| (format!("f{i}"), "x".repeat(300))).collect();
        ops.push(Op::SetStyle {
            layer: StyleLayer::Project,
            catalogue: "location".into(),
            id: StyleRef::Own(9),
            before: None,
            after: Some(StyleRecord { name: long.clone(), fields }),
        });
        ops.push(Op::SetStyle {
            layer: StyleLayer::Project,
            catalogue: "cable".into(),
            id: StyleRef::Own(10),
            before: None,
            after: Some(StyleRecord { name: "Future".into(), fields: Default::default() }),
        });
        ops.push(Op::SetProperty {
            object: ObjectRef::Component(k1),
            key: "text.tag.style".into(),
            before: None,
            after: Some(format!("#{:032x}", 0xbeef)),
        });
        ops.push(Op::SetProperty { object: ObjectRef::Project, key: "location.style".into(), before: None, after: Some("nonsense".into()) });
        p.ledger.commit(ecad_model::Author::User, "hostile", ops).unwrap();
    });
    let doc = h.doc();
    let cat = styles::catalogue("text").unwrap();
    assert!(styles::rows(&doc, cat).len() <= 23 + ecad_model::style::MAX_STYLES_PER_CATALOGUE);
    let rows = ecad_cadcraft::checks::all(&doc, 0.01);
    assert!(rows.iter().any(|c| c.kind == ecad_cadcraft::checks::Kind::StyleLimits));
    assert!(rows.iter().filter(|c| c.kind == ecad_cadcraft::checks::Kind::StyleUnknown).count() >= 2);
    let name = styles::name(&doc, "location", &StyleRef::Own(9));
    assert!(name.chars().count() <= 64);
    // It draws (the page projected from the document, as a print does) and the reads answer.
    let fan = Default::default();
    let (d, _) = ecad_cadcraft::printbook::build_pending(&doc, &[page], &fan, 0.01, None, false).unwrap();
    assert!(!d.model.is_empty());
    assert!(h.run("styles", json!({})).unwrap()["unknown_catalogues"].as_array().unwrap().iter().any(|c| c["catalogue"] == "cable"));
    assert_eq!(text_style(&doc, TextKind::Tag).height, 2.5);
}

// ------------------------------------------------------------------------- 050b: wire categories

fn wire(h: &mut H, y: f64, potential: Option<&str>) -> ecad_model::ConductorId {
    let mut p = json!({"points": [[10, y], [60, y]]});
    if let Some(pot) = potential {
        p["potential"] = json!(pot);
    }
    id(&h.run("wire", p).unwrap(), "conductor")
}

fn label_of(h: &mut H, page: PageId, c: ecad_model::ConductorId) -> Option<String> {
    h.p(|p| ecad_cadcraft::wires::labels_on_page(p.ledger.document(), page).into_iter().find(|l| l.conductor == c).map(|l| l.text))
}

#[test]
fn a_24v_wire_takes_the_24_v_dc_category() {
    let (mut h, page, _) = fixture();
    let v = h.run("styles", json!({"catalogue": "wire"})).unwrap();
    let ids: Vec<&str> = v["styles"].as_array().unwrap().iter().map(|s| s["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["l", "l1", "l2", "l3", "n", "pe", "dc24", "dc0", "signal", "analogue"]);
    let w = wire(&mut h, 150.0, Some("24V"));
    h.run("renumberwires", json!({})).unwrap();
    let label = label_of(&mut h, page, w).unwrap();
    assert!(label.ends_with(" 0.5 BU") && label.starts_with("24V-"), "{label}");
    let doc = h.doc();
    assert_eq!(ecad_cadcraft::look::conductor_colour(&doc, w).as_deref(), Some("BU"), "drawn blue (line colour: wire)");
    let rows = h.run("wirelist", json!({})).unwrap();
    assert!(rows.to_string().contains("24 V DC"), "{rows}");
    // A power wire is drawn in ink, heavier.
    let l = wire(&mut h, 170.0, Some("L1"));
    let doc = h.doc();
    assert_eq!(ecad_cadcraft::look::conductor_colour(&doc, l), None, "power in ink");
    assert!((styles::wire::wire_look(&doc, l).line_weight - 0.6).abs() < 1e-9);
}

#[test]
fn a_category_change_reaches_its_wires_but_not_an_own_colour() {
    let (mut h, page, _) = fixture();
    let a = wire(&mut h, 150.0, Some("24V"));
    let b = wire(&mut h, 170.0, Some("24V"));
    h.run("setprop", json!({"conductor": b.hex(), "key": "colour", "value": "WH"})).unwrap();
    h.run("setstyle", json!({"catalogue": "wire", "id": "dc24", "fields": {"colour": "RD"}})).unwrap();
    let doc = h.doc();
    assert_eq!(ecad_cadcraft::look::conductor_colour(&doc, a).as_deref(), Some("RD"));
    assert_eq!(ecad_cadcraft::look::conductor_colour(&doc, b).as_deref(), Some("WH"), "its own colour stays");
    assert!(label_of(&mut h, page, a).unwrap().ends_with(" RD"));
    let look = styles::wire::wire_look(&doc, b);
    assert_eq!(look.colour.map(|c| c.1), Some(Source::Object), "overridden");
    // Reset: it follows the category again.
    h.run("setprop", json!({"conductor": b.hex(), "key": "colour", "value": null})).unwrap();
    assert_eq!(ecad_cadcraft::look::conductor_colour(&h.doc(), b).as_deref(), Some("RD"));
}

#[test]
fn choosing_a_category_sets_a_missing_potential_and_keeps_another() {
    let (mut h, _, _) = fixture();
    let a = wire(&mut h, 150.0, Some("24V"));
    let before = h.head();
    let v = h.run("category", json!({"conductor": a.hex(), "value": "signal"})).unwrap();
    assert_eq!(h.head(), before + 1);
    let doc = h.doc();
    assert_eq!(styles::wire::wire_look(&doc, a).category, Some(StyleRef::Builtin("signal".into())));
    let net = doc.connectivity().net_of_conductor(a).and_then(|n| doc.connectivity().net(n)).unwrap();
    assert_eq!(ecad_model::wiring::net_potential(&doc, net).name.as_deref(), Some("24V"), "the net keeps 24V");
    assert!(v["message"].is_null(), "signal names no potential: {v}");
    // A wire with no potential: the category's potential comes with it, in one transaction.
    let b = wire(&mut h, 170.0, None);
    let before = h.head();
    h.run("category", json!({"conductor": b.hex(), "value": "24 V DC"})).unwrap();
    assert_eq!(h.head(), before + 1);
    let doc = h.doc();
    let net = doc.connectivity().net_of_conductor(b).and_then(|n| doc.connectivity().net(n)).unwrap();
    assert_eq!(ecad_model::wiring::net_potential(&doc, net).name.as_deref(), Some("24V"));
    // A net with another potential keeps it, and says so.
    let v = h.run("category", json!({"conductor": a.hex(), "value": "Power L1"})).unwrap();
    assert_eq!(v["message"], json!("Category set; the net keeps potential 24V."));
    // `wire {category}` gives the new wire its category in the same transaction.
    let before = h.head();
    let r = h.run("wire", json!({"points": [[10, 190], [60, 190]], "category": "analogue"})).unwrap();
    assert_eq!(h.head(), before + 1);
    let c: ecad_model::ConductorId = id(&r, "conductor");
    assert_eq!(styles::wire::wire_look(&h.doc(), c).category, Some(StyleRef::Builtin("analogue".into())));
}

#[test]
fn one_category_per_potential_and_old_potential_colours_are_refused() {
    let (mut h, _, _) = fixture();
    let e = h.run("setstyle", json!({"catalogue": "wire", "id": "pe", "fields": {"potential": "24V"}})).unwrap_err();
    assert!(e.contains("24 V DC is already the default for 24V; clear it there first."), "{e}");
    let mut w = h.run("wiresettings", json!({})).unwrap();
    let pots = w["potentials"].as_array_mut().unwrap();
    pots[0]["colour"] = json!("RD");
    let e = h.run("wiresettings", json!({"potentials": pots.clone()})).unwrap_err();
    assert!(e.contains("set on wire categories now"), "{e}");
    // An older file's potential colours: one Checks row, cleared by writing the table.
    h.cfg(|p| {
        let before = p.ledger.document().settings.clone();
        let mut after = before.clone();
        after.wiring.potentials[0].legacy_colour = Some("BN".into());
        p.ledger.commit(ecad_model::Author::User, "old", vec![Op::SetProjectSettings { before, after }]).unwrap();
    });
    let rows = ecad_cadcraft::checks::all(&h.doc(), 0.01);
    assert!(rows.iter().any(|c| c.kind == ecad_cadcraft::checks::Kind::IgnoredOldSetting && c.which.contains("Potential colours")));
    let w = h.run("wiresettings", json!({})).unwrap();
    h.run("wiresettings", json!({"potentials": w["potentials"].clone(), "digits": 3})).unwrap();
    assert!(h.doc().settings.wiring.potentials.iter().all(|p| p.legacy_colour.is_none()));
}

#[test]
fn a_category_with_a_part_number_puts_wire_on_the_bom() {
    let (mut h, _, _) = fixture();
    for y in [150.0, 160.0, 170.0] {
        wire(&mut h, y, Some("24V"));
    }
    h.run("setstyle", json!({"catalogue": "wire", "id": "dc24", "fields": {"part_number": "W-05-BU"}})).unwrap();
    let rows = ecad_cadcraft::checks::all(&h.doc(), 0.01);
    assert!(rows.iter().any(|c| c.kind == ecad_cadcraft::checks::Kind::WireAllowance), "no allowance: the BOM check");
    h.run("setstyle", json!({"catalogue": "wire", "id": "dc24", "fields": {"allowance": 1.5}})).unwrap();
    let bom = h.cfg(|p| p.outputs().wire_bom());
    assert_eq!(bom.len(), 1);
    assert_eq!(bom[0].part_number.as_deref(), Some("W-05-BU"));
    assert_eq!(bom[0].qty_text(), "4.5 m", "3 wires × 1.5 m");
    assert!(bom[0].description.as_deref().is_some_and(|d| d.contains("estimate")));
}

#[test]
fn mono_prints_wires_in_ink_and_colour_prints_their_line_colours() {
    let (mut h, page, _) = fixture();
    wire(&mut h, 150.0, Some("24V"));
    wire(&mut h, 170.0, Some("L1"));
    let doc = h.doc();
    let fan = Default::default();
    let (mono, _) = ecad_cadcraft::printbook::build_pending(&doc, &[page], &fan, 0.01, None, false).unwrap();
    assert!(mono.model.iter().all(|e| !matches!(e.common.color, Color::True(_))));
    let (colour, _) = ecad_cadcraft::printbook::build_in(&doc, &[page], &fan, 0.01, None, false, true).unwrap();
    assert_eq!(colour.model.iter().filter(|e| matches!(e.common.color, Color::True(_))).count(), 1, "only the 24V wire (power is ink)");
    // The heavier power line prints in mono too (weight is not colour).
    assert!(mono.model.iter().any(|e| e.common.lineweight == cadcraft_engine::doc::Lineweight::Mm100(60)));
}
