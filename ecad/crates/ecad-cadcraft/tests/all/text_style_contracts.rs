//! Brief 031 Q1 (design 031 §2-§4, §13 T1-T4, T12): the drawn sizes and styles of derived
//! texts, the styles installed in every page drawing, and `textstyle` redrawing the open page.

use crate::common::{H, id};

use cadcraft_engine::doc::{Entity, EntityKind, Lineweight};
use ecad_cadcraft::text_style::{self, STYLE_BOLD, STYLE_REGULAR, TextKind};
use ecad_model::{ComponentId, PageId};
use serde_json::json;

const SEED: u64 = 31;

/// The texts on `page`'s drawing whose value is `value`: (height, style, lineweight).
fn drawn(h: &mut H, page: PageId, value: &str) -> Vec<(f64, String, Lineweight)> {
    h.pv(|p, pg| pg.entities_of(p, page))
        .iter()
        .filter_map(|e| match &e.kind {
            EntityKind::Text(t) if t.value == value => Some((t.height, t.style.clone(), e.common.lineweight)),
            _ => None,
        })
        .collect()
}

fn texts(h: &mut H, page: PageId) -> Vec<Entity> {
    h.pv(|p, pg| pg.entities_of(p, page)).iter().map(|e| (**e).clone()).collect()
}

#[test]
fn a_new_project_draws_tags_bold_and_labels_small() {
    let mut h = H::new(SEED);
    let page = h.first_page();
    let r = h.run("place", json!({"symbol": "coil", "at": [100, 100]})).unwrap();
    let c: ComponentId = id(&r, "component");
    let tag = h.p(|p| ecad_cadcraft::devices::tag_words(p.ledger.document(), c));
    let fonts = h.p(|p| text_style::Fonts::resolve(p.ledger.document()));
    let bold_lw = fonts.bold_lineweight.map_or(Lineweight::ByLayer, Lineweight::Mm100);
    assert_eq!(drawn(&mut h, page, &tag), vec![(2.5, STYLE_BOLD.to_string(), bold_lw)]);
    // A coil's connection point labels: 1.5 mm, regular.
    for l in ["A1", "A2"] {
        assert_eq!(drawn(&mut h, page, l), vec![(1.5, STYLE_REGULAR.to_string(), Lineweight::ByLayer)], "{l}");
    }
    // A location box label: 1.6 mm.
    h.run("locationbox", json!({"location": "+JB1", "corners": [[80, 80], [160, 130]]})).unwrap();
    assert_eq!(drawn(&mut h, page, "+JB1"), vec![(1.6, STYLE_REGULAR.to_string(), Lineweight::ByLayer)]);
    // Both styles are in the page's drawing, naming the resolved fonts.
    let d = h.pv(|p, pg| pg.drawing_of(p, page)).unwrap();
    assert_eq!(d.text_style(STYLE_REGULAR).map(|s| s.font.clone()), Some(fonts.regular.clone()));
    assert_eq!(d.text_style(STYLE_BOLD).map(|s| s.font.clone()), Some(fonts.bold.clone()));
    h.check();
}

#[test]
fn without_craft_fonts_bold_is_a_heavier_stroke() {
    // Test builds do not embed craft-fonts; when the default family is not on the machine either,
    // the stroke font draws and bold gets `text.bold_stroke_weight` (§2.2 step 4).
    let doc = ecad_model::Document::default();
    let f = text_style::Fonts::resolve(&doc);
    if f.drawn_with == "stroke" {
        assert_eq!(f.bold_lineweight, Some(35));
        assert_eq!(f.regular, cadcraft_engine::fonts::BUILTIN_FONT);
        assert!(f.status().contains("drawn with the stroke font"), "{}", f.status());
    } else {
        assert_eq!(f.bold_lineweight, None);
        assert_eq!(f.drawn_with, "BIZ UDPGothic");
    }
    // A family nobody has falls back the same way and says so.
    let mut doc = ecad_model::Document::default();
    doc.properties.insert("text.font".into(), "NoSuchFont 031".into());
    let g = text_style::Fonts::resolve(&doc);
    assert_eq!((g.wanted.as_str(), g.drawn_with.as_str()), ("NoSuchFont 031", f.drawn_with.as_str()));
    assert!(g.status().starts_with("NoSuchFont 031 is not available here; drawn with"), "{}", g.status());
}

#[test]
fn textstyle_redraws_the_open_page_in_one_transaction() {
    let mut h = H::new(SEED);
    let page = h.first_page();
    let r = h.run("place", json!({"symbol": "coil", "at": [100, 100]})).unwrap();
    let c: ComponentId = id(&r, "component");
    let tag = h.p(|p| ecad_cadcraft::devices::tag_words(p.ledger.document(), c));
    let head = h.head();
    h.run("textstyle", json!({"kind": "tag", "height": 3, "weight": "regular"})).unwrap();
    assert_eq!(h.head(), head + 1, "one transaction");
    assert_eq!(drawn(&mut h, page, &tag), vec![(3.0, STYLE_REGULAR.to_string(), Lineweight::ByLayer)]);
    h.run("textstyle", json!({"kind": "point_label", "height": 2})).unwrap();
    assert!(drawn(&mut h, page, "A1").iter().all(|d| d.0 == 2.0));
    // Undo restores both fields of the first change.
    h.cmdline("u");
    h.cmdline("u");
    let doc = h.doc();
    assert_eq!(text_style::text_style(&doc, TextKind::Tag).height, 2.5);
    assert_eq!(drawn(&mut h, page, &tag)[0].0, 2.5);
    assert_eq!(drawn(&mut h, page, &tag)[0].1, STYLE_BOLD);
}

#[test]
fn a_font_change_is_not_a_user_edit_of_the_style_table() {
    let mut h = H::new(SEED);
    let page = h.first_page();
    h.run("textstyle", json!({"font": "NoSuchFont 031"})).unwrap();
    // An ordinary edit after it is accepted (translate compares the table with the projection).
    let head = h.head();
    h.run("line", json!({"points": [[10, 10], [50, 10]]})).unwrap();
    assert_eq!(h.head(), head + 1, "the line is recorded");
    assert!(!texts(&mut h, page).is_empty());
    h.check();
}

#[test]
fn text_with_justify_is_recorded_with_its_alignment() {
    let mut h = H::new(SEED);
    let page = h.first_page();
    let head = h.head();
    h.run("text", json!({"at": [100, 100], "text": "X", "height": 2.5, "justify": "MC"})).unwrap();
    assert_eq!(h.head(), head + 1, "recorded, not refused");
    let doc = h.doc();
    let item = doc
        .items
        .values()
        .find_map(|i| match &i.kind {
            ecad_model::ItemKind::Primitive(ecad_model::Primitive::Text { value, halign, valign, at, .. }) if value == "X" => {
                Some((*halign, *valign, *at))
            }
            _ => None,
        })
        .expect("the text item");
    assert_eq!(item, (ecad_model::TextHAlign::Center, ecad_model::TextVAlign::Middle, ecad_model::Pt::new(100.0, 100.0)));
    let e = texts(&mut h, page).into_iter().find(|e| matches!(&e.kind, EntityKind::Text(t) if t.value == "X")).unwrap();
    let EntityKind::Text(t) = &e.kind else { unreachable!() };
    assert_eq!((t.halign, t.valign), (cadcraft_engine::doc::HAlign::Center, cadcraft_engine::doc::VAlign::Middle));
    h.check();
}
