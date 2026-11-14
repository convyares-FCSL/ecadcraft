//! Brief 050a in the trial, headless (egui_kittest; design 050 §7, §9): the standard header makes
//! a standard after saying nothing is written until Enter; the catalogue strip shows the text,
//! location and frame tables; a style's detail says the consequence first and applies a field in
//! one transaction, patching one row (no rebuild); the inspector's Style row lists the text styles
//! and chooses one for the device; the status bar names the standard.

use ecad_cadcraft::defaults::Source;
use ecad_model::StyleRef;
use ecad_trial::lists::style_catalogues as sc;
use ecad_trial::settings::standards;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

use crate::mouse_tester::{doc, harness_with, head, ok};

type H = Harness<'static, ecad_trial::Trial>;

fn click(h: &mut H, label: &str) {
    h.get_by_label(label).click();
    h.run_steps(3);
}

fn fixture() -> H {
    let mut h = harness_with(None);
    h.state_mut().set_cad_chrome(false);
    ok(&mut h, r#"place {"symbol":"coil","at":[100,100]}"#);
    h
}

#[test]
fn the_header_makes_a_standard_and_the_status_bar_names_it() {
    let mut h = fixture();
    h.state_mut().open_standards(None);
    h.run_steps(3);
    assert!(h.query_by_label(standards::HEADER).is_some());
    click(&mut h, standards::MAKE);
    assert!(h.query_by_label(standards::HEADER_NAME).is_some(), "an inline name field, no dialog");
    let before = head(&mut h);
    h.state_mut().settings.standards.header = standards::HeaderMode::Make("House".into());
    h.run_steps(2);
    assert_eq!(head(&mut h), before, "nothing written while typing");
    click(&mut h, standards::HEADER_CONFIRM);
    assert_eq!(head(&mut h), before + 1, "one transaction");
    assert_eq!(doc(&mut h).standard.map(|s| s.header.name), Some("House".into()));
    assert_eq!(h.state_mut().standard_words(), "Standard: House");
    assert!(h.query_by_label(standards::BACK_TO_IEC).is_some() && h.query_by_label(standards::RENAME).is_some());
    // Back to IEC says its consequence first, then goes on the confirm.
    click(&mut h, standards::BACK_TO_IEC);
    assert!(h.query_by_label(standards::HEADER_CONSEQUENCE).is_some());
    assert_eq!(head(&mut h), before + 1, "said, not applied");
    click(&mut h, standards::HEADER_CONFIRM);
    assert!(doc(&mut h).standard.is_none());
}

#[test]
fn a_style_field_applies_in_one_transaction_and_patches_one_row() {
    let mut h = fixture();
    h.state_mut().open_standards(None);
    h.run_steps(3);
    for (_, tab, _) in standards::CATALOGUES {
        assert!(h.query_by_label(tab).is_some(), "{tab}");
    }
    click(&mut h, standards::CATALOGUE_STYLES);
    let v = &h.state().settings.standards;
    assert_eq!(v.catalogue, 1);
    let (text, _) = &v.cats[0];
    let tag = text.key_of(&StyleRef::Builtin("tag".into())).unwrap();
    let rows = ecad_ui::table::TableSource::keys(text).len();
    assert_eq!(rows, 21, "one row per text style");
    h.state_mut().settings.standards.style_focus = Some(tag);
    h.run_steps(3);
    assert!(h.query_by_label("ecad.standards.style.field.colour").is_some());
    let (rebuilds, patches) = (h.state().settings.standards.rebuilds, h.state().settings.standards.patches);
    // Typing a colour says the consequence first; Apply writes one transaction.
    let before = head(&mut h);
    let st = &mut h.state_mut().settings.standards.style_detail;
    st.field = Some("colour");
    st.edit = "RD".into();
    h.run_steps(2);
    assert!(h.query_by_label(sc::CONSEQUENCE).is_some(), "the consequence line comes first");
    assert_eq!(head(&mut h), before);
    h.query_all_by_label(sc::APPLY).find(|n| !n.accesskit_node().is_disabled()).unwrap().click();
    h.run_steps(3);
    let err = h.state().settings.standards.style_detail.error.clone();
    assert_eq!(head(&mut h), before + 1, "one transaction ({err:?})");
    let v = &h.state().settings.standards;
    assert_eq!((v.rebuilds, v.patches), (rebuilds, patches + 1), "patched, not rebuilt");
    let row = v.cats[0].0.row(tag).unwrap();
    assert!(row.overridden, "the badge's dot");
    assert!(row.values.iter().any(|(val, from)| val == "RD" && *from == Source::Project), "{:?}", row.values);
    // A commit touching no style or default leaves every table alone.
    ok(&mut h, r#"wire {"points":[[0,150],[40,150]]}"#);
    h.run_steps(2);
    let v = &h.state().settings.standards;
    assert_eq!((v.rebuilds, v.patches), (rebuilds, patches + 1));
}

#[test]
fn the_inspector_chooses_a_text_style_for_the_device() {
    let mut h = fixture();
    ok(&mut h, r#"setstyle {"catalogue":"text","new":true,"name":"Tag large","fields":{"height":5}}"#);
    let d = doc(&mut h);
    let rep = *d.representations.keys().next().unwrap();
    let c = *d.components.keys().next().unwrap();
    h.state_mut().goto_symbols(&[rep]).unwrap();
    h.run_steps(3);
    let rows = &h.state().settings.inspector.rows;
    let style = rows.iter().find(|r| r.key == "text.tag.style").expect("a Tag style row");
    assert!(style.styles.iter().any(|(_, n)| n == "Tag large"), "{:?}", style.styles);
    assert_eq!(style.value, "Tag");
    assert!(h.query_by_label("ecad.inspector.style.text.tag.style").is_some());
    let large = style.styles.iter().find(|(_, n)| n == "Tag large").unwrap().0.clone();
    let line = ecad_cadcraft::default_cmds::line(
        "text.tag.style",
        ecad_cadcraft::defaults::Level::Object,
        ecad_cadcraft::default_cmds::Target::Component(c),
        Some(&large),
    );
    h.state_mut().inspector_action(ecad_trial::shell::right::InspectorAction::Run("text.tag.style", line));
    h.run_steps(3);
    assert_eq!(doc(&mut h).components[&c].properties.get("text.tag.style"), Some(&large));
    let rows = &h.state().settings.inspector.rows;
    let style = rows.iter().find(|r| r.key == "text.tag.style").unwrap();
    assert_eq!((style.value.as_str(), style.from), ("Tag large", Source::Object));
    assert!(h.query_by_label("ecad.inspector.reset.text.tag.style").is_some(), "Reset clears it");
}

/// Screenshots for the owner's look check (written when `ECAD_TESTER_SHOTS` names a folder and a
/// wgpu adapter exists; never committed): Settings ▸ Standards on the text and wire catalogues
/// with a style's detail, a 24 V wire's Category row in the inspector, and one page printed mono
/// and in colour (PDF, beside the PNGs).
#[test]
#[ignore = "slow: a 1920×1080 trial and two prints; run by name for the screenshots"]
fn look_check_screenshots() {
    use std::sync::Arc;
    let deltas: crate::mouse_tester::Deltas = Arc::default();
    let mut h = harness_with(Some(deltas.clone()));
    h.state_mut().set_cad_chrome(false);
    ok(&mut h, r#"place {"symbol":"coil","at":[100,100]}"#);
    for (y, pot) in [(150, "L1"), (140, "N"), (130, "PE"), (120, "24V"), (110, "0V")] {
        ok(&mut h, &format!(r#"wire {{"points":[[40,{y}],[160,{y}]],"potential":"{pot}"}}"#));
    }
    ok(&mut h, r#"standard {"new":"House"}"#);
    ok(&mut h, r#"setstyle {"catalogue":"text","id":"tag","fields":{"colour":"RD"}}"#);
    let out = std::env::var_os("ECAD_TESTER_SHOTS").map(std::path::PathBuf::from);
    let save = |h: &H, name: &str| {
        if let Some(dir) = &out
            && std::fs::create_dir_all(dir).is_ok()
            && crate::mouse_tester::shot(h, &deltas, &dir.join(name)).is_none()
        {
            eprintln!("no wgpu adapter: {name} skipped");
        }
    };

    // Settings ▸ Standards: the text styles with Tag's detail, then the wire categories with 24 V DC's.
    h.state_mut().open_standards(None);
    h.run_steps(3);
    click(&mut h, standards::CATALOGUE_STYLES);
    let tag = h.state().settings.standards.cats[0].0.key_of(&StyleRef::Builtin("tag".into())).unwrap();
    h.state_mut().settings.standards.style_focus = Some(tag);
    h.run_steps(4);
    save(&h, "050-standards-text.png");
    click(&mut h, "ecad.standards.catalogue.wire");
    let v = &h.state().settings.standards;
    let wire = standards::CATALOGUES.iter().position(|c| c.0 == "wire").unwrap();
    let dc24 = v.cats[wire].0.key_of(&StyleRef::Builtin("dc24".into())).unwrap();
    h.state_mut().settings.standards.style_focus = Some(dc24);
    h.run_steps(4);
    save(&h, "050-standards-wire.png");
    h.state_mut().close_settings();
    h.run_steps(3);

    // The 24 V wire selected: the inspector's Category row says where its category comes from.
    let handle = {
        let t = h.state_mut();
        let p = t.project().unwrap().clone();
        p.read(&mut t.cad.session, |p, _| {
            let doc = p.ledger.document();
            let page = doc.page_order[0];
            let w = doc.conductors.keys().find(|c| ecad_model::wiring::label(doc, **c).is_some_and(|l| l.text.starts_with("24V"))).copied().unwrap();
            let (item, _) = ecad_model::derive::route_on_page(doc, w, page).unwrap();
            p.pages[&page].map.item(item).unwrap()
        })
        .unwrap()
    };
    h.state_mut().cad.session.set_selection(vec![handle]);
    h.run_steps(4);
    let rows = &h.state().settings.inspector.rows;
    let cat = rows.iter().find(|r| r.key == "wire.category").expect("a Category row");
    assert!(cat.value.contains("24 V DC") && cat.value.contains("from potential"), "{}", cat.value);
    save(&h, "050-inspector-category.png");

    // One page printed mono and in colour.
    if let Some(dir) = &out {
        ecad_cadcraft::printbook::install_io();
        for (word, file) in [("mono", "050-print-mono.pdf"), ("colour", "050-print-colour.pdf")] {
            let path = dir.join(file).to_string_lossy().into_owned();
            let r = h.state_mut().cad.session.execute("printbook", &serde_json::json!({"scope": {"pages": "1"}, "path": path, "colour": word}));
            if let Err(e) = r {
                eprintln!("printbook {word}: {e}");
            }
        }
    }
}
