//! Brief 018 (design 018 §12): devices, tags and starter symbols, from outside, through the
//! commands' JSON and interactive forms.

use crate::common;

use cadcraft_engine::Input;
use cadcraft_engine::geom::Vec2;
use common::{H, id};
use ecad_cadcraft::devices::{self, TextOf};
use ecad_cadcraft::symbols::{self, STARTERS};
use ecad_model::{ComponentId, ConductorId, End, ItemId, Op, PageId, Subject, TerminalId};
use serde_json::{Value, json};

const SEED: u64 = 18;

fn new_h() -> H {
    let mut h = H::new(SEED);
    h.s.viewport_px = (1280.0, 800.0);
    h
}

fn place(h: &mut H, symbol: &str, at: [f64; 2]) -> Value {
    h.run("place", json!({"symbol": symbol, "at": at})).unwrap()
}

fn join(h: &mut H, symbol: &str, device: ComponentId, at: [f64; 2]) -> Value {
    h.run("place", json!({"symbol": symbol, "at": at, "device": device.hex()})).unwrap()
}

fn labels(v: &Value) -> Vec<String> {
    v["terminals"].as_array().unwrap().iter().map(|t| t["label"].as_str().unwrap().to_string()).collect()
}

fn tag(h: &mut H, c: ComponentId) -> String {
    h.p(|p| devices::tag_words(p.ledger.document(), c))
}

/// The derived tag texts drawn on `page` (as the projection draws them).
fn tag_texts(h: &mut H, page: PageId) -> Vec<String> {
    h.p(|p| {
        devices::texts_on_page(p.ledger.document(), page)
            .into_iter()
            .filter(|t| t.of == TextOf::Tag)
            .map(|t| t.value)
            .collect()
    })
}

fn new_page(h: &mut H, title: &str) -> PageId {
    id(&h.run("newpage", json!({"title": title})).unwrap(), "page")
}

fn last_ops(h: &mut H) -> Vec<Op> {
    h.p(|p| p.ledger.log().last().unwrap().ops.clone())
}

fn prompt(h: &H) -> String {
    h.s.current_prompt().map(|p| p.display()).unwrap_or_default()
}

fn typed(h: &mut H, text: &str) {
    if text.is_empty() {
        h.s.input(Input::Enter).unwrap();
    } else {
        h.s.cmdline(text).unwrap();
    }
    h.check();
}

/// Fixture B: `-K01`'s coil on page 1, its NO and NC contacts on page 2.
struct K1 {
    h: H,
    p1: PageId,
    p2: PageId,
    k1: ComponentId,
    no: Value,
    nc: Value,
}

fn k1() -> K1 {
    let mut h = new_h();
    let p1 = h.first_page();
    let coil = place(&mut h, "coil", [0.0, 0.0]);
    let k1: ComponentId = id(&coil, "component");
    let p2 = new_page(&mut h, "Control");
    let no = join(&mut h, "no", k1, [0.0, 0.0]);
    let nc = join(&mut h, "nc", k1, [50.0, 0.0]);
    K1 { h, p1, p2, k1, no, nc }
}

// ---------------------------------------------------------------- A

#[test]
fn a_every_starter_symbol_gets_its_prefix_and_the_next_number() {
    let mut h = new_h();
    let want = [
        ("coil", "-K01"),
        ("no", "-K02"),
        ("nc", "-K03"),
        ("power3", "-K04"),
        ("mcb1", "-Q01"),
        ("mcb2", "-Q02"),
        ("fuse", "-F01"),
        ("psu", "-G01"),
        ("pbno", "-S01"),
        ("pbnc", "-S02"),
        ("sel", "-S03"),
        ("lamp", "-H01"),
        ("motor3", "-M01"),
        ("motor1", "-M02"),
        ("trafo", "-T01"),
        ("terminal", "-X01"),
        ("di", "-A01"),
        ("do", "-A02"),
    ];
    assert_eq!(want.len(), STARTERS.len());
    for (i, (key, t)) in want.iter().enumerate() {
        let x = 60.0 * (i % 6) as f64;
        let y = -60.0 * (i / 6) as f64;
        let r = place(&mut h, key, [x, y]);
        assert_eq!(r["tag"], json!(t), "{key}");
        assert_eq!(r["joined"], json!(false));
        let s = symbols::by_key(key).unwrap();
        assert_eq!(labels(&r), s.labels(), "{key}: the definition's terminal names");
    }
    // Each definition is embedded once, with its origin; a second coil reuses it.
    let r = place(&mut h, "coil", [400.0, 0.0]);
    assert_eq!(r["tag"], json!("-K05"));
    let doc = h.doc();
    assert_eq!(doc.definitions.len(), STARTERS.len());
    for s in STARTERS {
        let d = symbols::embedded(&doc, s).unwrap();
        let o = doc.definitions[&d].origin.clone().unwrap();
        assert_eq!((o.library, o.definition, o.library_seq), (symbols::STARTER_LIBRARY, s.id, 1));
    }
    assert!(!last_ops(&mut h).iter().any(|o| matches!(o, Op::EmbedDefinition { .. })));
    // Every symbol shows its tag beside it.
    let page = h.first_page();
    assert_eq!(tag_texts(&mut h, page).len(), STARTERS.len() + 1);
}

// ---------------------------------------------------------------- B, C

#[test]
fn b_a_coil_and_two_contacts_on_two_pages_are_one_device() {
    let mut f = k1();
    assert_eq!(
        (labels(&f.no), labels(&f.nc)),
        (vec!["13".to_string(), "14".into()], vec!["21".to_string(), "22".into()])
    );
    assert_eq!((f.no["joined"].clone(), f.nc["joined"].clone()), (json!(true), json!(true)));
    assert_eq!(f.no["tag"], json!("-K01"));
    assert_eq!(f.nc["pages"], json!([1, 2]));
    let label = f.h.p(|p| p.ledger.log().last().unwrap().label.clone());
    assert_eq!(label, "Add symbol to -K01");
    // The drawing shows -K01 beside all three.
    assert_eq!(tag_texts(&mut f.h, f.p1), ["-K01"]);
    assert_eq!(tag_texts(&mut f.h, f.p2), ["-K01", "-K01"]);
    let d = f.h.run("devices", json!({})).unwrap();
    let reps = d["devices"][0]["representations"].as_array().unwrap().clone();
    assert_eq!(reps.len(), 3);
    assert_eq!(reps.iter().map(|r| r["page_number"].clone()).collect::<Vec<_>>(), [json!(1), json!(2), json!(2)]);
    f.h.check();
}

#[test]
fn c_renaming_the_tag_updates_every_symbol_in_one_undoable_transaction() {
    let mut f = k1();
    let r = f.h.run("setprop", json!({"component": f.k1.hex(), "key": "tag", "value": "k5"})).unwrap();
    assert_eq!(r["value"], json!("-K05"));
    let ops = last_ops(&mut f.h);
    assert!(matches!(ops.as_slice(), [Op::SetProperty { key, .. }] if key == "tag"), "{ops:?}");
    assert_eq!(f.h.p(|p| p.ledger.log().last().unwrap().label.clone()), "Set tag");
    assert_eq!(tag_texts(&mut f.h, f.p2), ["-K05", "-K05"]);
    f.h.activate(f.p1);
    assert_eq!(tag_texts(&mut f.h, f.p1), ["-K05"]);
    f.h.run("u", Value::Null).unwrap();
    assert_eq!(tag(&mut f.h, f.k1), "-K01");
    f.h.check();
}

// ---------------------------------------------------------------- D (duplicates: never a dead end)

#[test]
fn d_a_duplicate_tag_names_the_device_and_offers_the_ways_forward() {
    let mut f = k1();
    f.h.activate(f.p1);
    let k2: ComponentId = id(&place(&mut f.h, "coil", [60.0, 0.0]), "component");
    assert_eq!(tag(&mut f.h, k2), "-K02");
    let head = f.h.head();
    for typed in ["-K1", "K01", "k001"] {
        let e = f.h.run("setprop", json!({"component": k2.hex(), "key": "tag", "value": typed})).unwrap_err();
        assert!(e.contains("Tag -K01 is already used by -K01 (Contactor / relay coil, page 1"), "{e}");
        assert!(e.contains("`merge`") && e.contains("next free one is -K03"), "{e}");
    }
    assert_eq!(f.h.head(), head, "nothing written");
    // Any letters are accepted (IEC is the default, never the law).
    for (typed, stored) in [("sr1", "-SR01"), ("TB01", "-TB01"), ("N01", "-N01"), ("-K1A", "-K1A")] {
        f.h.run("setprop", json!({"component": k2.hex(), "key": "tag", "value": typed})).unwrap();
        assert_eq!(tag(&mut f.h, k2), stored);
    }
    // The search lists devices by tag.
    let s = f.h.run("symbols", json!({"query": "-k"})).unwrap();
    let tags: Vec<&str> = s["devices"].as_array().unwrap().iter().map(|d| d["tag"].as_str().unwrap()).collect();
    assert_eq!(tags, ["-K01", "-K1A"]);
    let q = f.h.run("symbols", json!({"query": "-Q"})).unwrap();
    let keys: Vec<&str> = q["symbols"].as_array().unwrap().iter().map(|d| d["key"].as_str().unwrap()).collect();
    assert_eq!(keys, ["mcb1", "mcb2"]);
}

#[test]
fn d_placing_with_an_existing_tag_adds_the_symbol_to_that_device() {
    let mut f = k1();
    let r = f.h.run("place", json!({"symbol": "no", "at": [100, 0], "tag": "K1"})).unwrap();
    assert_eq!((r["joined"].clone(), r["component"].clone()), (json!(true), json!(f.k1.hex())));
    assert_eq!(labels(&r), ["33", "34"]);
    f.h.check();
}

#[test]
fn d_merging_a_renamed_device_moves_its_symbols_terminals_and_wires() {
    let mut f = k1();
    // A NO contact placed as its own device (-K02) on page 2, wired at its 13.
    let k2v = place(&mut f.h, "no", [100.0, 0.0]);
    let k2: ComponentId = id(&k2v, "component");
    assert_eq!(labels(&k2v), ["13", "14"]);
    let w: ConductorId = id(&f.h.run("wire", json!({"points": [[100, 10], [140, 10]]})).unwrap(), "conductor");
    assert!(f.h.doc().conductors[&w].end(End::A).is_some());
    let r = f.h.run("setprop", json!({"component": k2.hex(), "key": "tag", "value": "-K01", "merge": true})).unwrap();
    assert_eq!(r["merged"], json!(true));
    let doc = f.h.doc();
    assert!(!doc.components.contains_key(&k2), "the merged device is gone");
    let reps = doc.representations_of(Subject::Component(f.k1)).count();
    assert_eq!(reps, 4);
    let t = doc.conductors[&w].end(End::A).unwrap();
    assert_eq!(doc.terminals[&t].component, f.k1, "the wire stays attached, now to -K01");
    assert_eq!(doc.terminals[&t].label.as_deref(), Some("33"), "renumbered as when joining");
    assert!(f.h.p(|p| p.ledger.log().last().unwrap().label.clone()).starts_with("Merge into -K01"));
    f.h.check();
    // One undo brings -K02 back.
    f.h.run("u", Value::Null).unwrap();
    assert!(f.h.doc().components.contains_key(&k2));
    f.h.check();
}

#[test]
fn d_next_number_is_the_highest_plus_one_and_deleted_tags_are_not_reused() {
    let mut h = new_h();
    let k1: ComponentId = id(&place(&mut h, "coil", [0.0, 0.0]), "component");
    let k2: ComponentId = id(&place(&mut h, "coil", [50.0, 0.0]), "component");
    h.run("ecaddelete", json!({"component": k2.hex()})).unwrap();
    assert_eq!(place(&mut h, "coil", [100.0, 0.0])["tag"], json!("-K03"));
    h.run("ecaddelete", json!({"component": k1.hex()})).unwrap();
    assert_eq!(place(&mut h, "coil", [150.0, 0.0])["tag"], json!("-K04"));
}

// ---------------------------------------------------------------- E

#[test]
fn e_save_and_reopen_keep_tags_devices_and_embedded_definitions() {
    let mut f = k1();
    f.h.run("setprop", json!({"component": f.k1.hex(), "key": "maker", "value": "Acme"})).unwrap();
    f.h.cfg(|p| p.persistence = ecad_cadcraft::Persistence::File);
    let dir = std::env::temp_dir().join(format!("ecad-018-{}", ecad_store::disk::unique()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("devices.ecad").display().to_string();
    f.h.run("saveas", json!({"path": path})).unwrap();
    let live = f.h.doc();
    while !f.h.s.docs.is_empty() {
        f.h.s.execute("close", &json!({"index": 0})).unwrap();
    }
    let mut s = cadcraft_engine::Session::empty();
    let (project, _) = ecad_cadcraft::ProjectHandle::open_file(&mut s, &path, 77).unwrap();
    let mut h2 = H { s, project, trail: Vec::new() };
    let doc = h2.doc();
    assert_eq!(doc, live, "tags, properties, the three symbols of -K01 and their labels");
    assert!(h2.p(|p| p.ledger.replay_matches().unwrap()));
    assert!(doc.definitions.values().all(|d| d.origin.is_some()));
    assert_eq!(doc.components[&f.k1].properties["maker"], "Acme");
    h2.check();
    drop(h2);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Review of PR #41 (S2): the project reads its history for tag numbers once when it opens and
/// then only what each change adds, so the prompt's preview, which never reads the log, already
/// knows a deleted tag after reopening.
#[test]
fn e_a_reopened_project_previews_the_next_tag_from_its_history() {
    let mut h = new_h();
    place(&mut h, "coil", [0.0, 0.0]);
    let k2: ComponentId = id(&place(&mut h, "coil", [50.0, 0.0]), "component");
    h.run("ecaddelete", json!({"component": k2.hex()})).unwrap();
    let coil = symbols::by_key("coil").unwrap();
    assert_eq!(h.p(|p| devices::next_tag_preview(p, coil)).as_deref(), Some("-K03"));
    h.cfg(|p| p.persistence = ecad_cadcraft::Persistence::File);
    let dir = std::env::temp_dir().join(format!("ecad-018-s2-{}", ecad_store::disk::unique()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("tags.ecad").display().to_string();
    h.run("saveas", json!({"path": path})).unwrap();
    while !h.s.docs.is_empty() {
        h.s.execute("close", &json!({"index": 0})).unwrap();
    }
    let mut s = cadcraft_engine::Session::empty();
    let (project, _) = ecad_cadcraft::ProjectHandle::open_file(&mut s, &path, 77).unwrap();
    let mut h2 = H { s, project, trail: Vec::new() };
    assert_eq!(h2.p(|p| devices::next_tag_preview(p, coil)).as_deref(), Some("-K03"), "-K02 was used once");
    // The preview uses nothing up.
    assert_eq!(h2.p(|p| devices::next_tag_preview(p, coil)).as_deref(), Some("-K03"));
    drop(h2);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Review of PR #41 (W3): a symbol that is linked to none of its device's connection points (an
/// old or hand-edited file) is refused before merging, with words that say what to do, not with
/// a model invariant.
#[test]
fn d_merging_a_device_whose_symbol_shows_no_connection_points_is_refused_clearly() {
    let mut f = k1();
    let k2: ComponentId = id(&place(&mut f.h, "no", [100.0, 0.0]), "component");
    let mut doc = f.h.doc();
    let (rid, _) = doc.representations_of(Subject::Component(k2)).next().unwrap();
    doc.representations.get_mut(&rid).unwrap().connection_points.clear();
    let ts: Vec<TerminalId> = doc.terminals_of(k2).map(|(t, _)| t).collect();
    for t in ts {
        doc.terminals.get_mut(&t).unwrap().key = Some(format!("old-{}", t.short()));
    }
    let mut ids = ecad_model::IdAllocator::seeded(5);
    let e = devices::plan_merge(&doc, &mut ids, k2, f.k1).unwrap_err();
    assert!(e.starts_with("Cannot merge -K02: its symbol on page"), "{e}");
    assert!(e.contains("not linked to any of its connection points"), "{e}");
}

// ---------------------------------------------------------------- G (interactive)

#[test]
fn g_typed_search_then_device_then_click_records_the_json_join() {
    let mut h = new_h();
    let k1: ComponentId = id(&place(&mut h, "coil", [0.0, 0.0]), "component");
    h.s.start("place").unwrap();
    assert_eq!(prompt(&h), "Enter symbol or [List/Rotate/Mirror] <coil>:");
    typed(&mut h, "cont");
    assert!(h.s.log.iter().any(|l| l.starts_with("\"cont\" matches")), "several matches are listed");
    typed(&mut h, "no");
    assert_eq!(prompt(&h), "Specify insertion point (new device -K02, rotation 0) or [Rotate/Mirror/Device/Tag]:");
    typed(&mut h, "D");
    assert_eq!(prompt(&h), "Enter device to join or [-K01/New] <-K01>:");
    typed(&mut h, "");
    assert_eq!(prompt(&h), "Specify insertion point (joins -K01 as 13/14, rotation 0) or [Rotate/Mirror/Device/Tag]:");
    typed(&mut h, "R");
    typed(&mut h, "100,0");
    typed(&mut h, "");
    assert!(h.s.running.is_none());
    let ops = last_ops(&mut h);
    assert!(ops.iter().any(
        |o| matches!(o, Op::LinkRepresentation { rep, .. } if rep.role == "no" && rep.connection_points.len() == 2)
    ));
    let doc = h.doc();
    let labels: Vec<String> = doc.terminals_of(k1).filter_map(|(_, t)| t.label.clone()).collect();
    assert!(labels.contains(&"13".to_string()) && labels.contains(&"14".to_string()));
    // Rotated: the tag sits above the contact, centred.
    let page = h.first_page();
    let above = h.p(|p| {
        devices::texts_on_page(p.ledger.document(), page)
            .into_iter()
            .any(|t| t.of == TextOf::Tag && t.halign == cadcraft_engine::doc::HAlign::Center)
    });
    assert!(above);
}

#[test]
fn g_a_typed_tag_that_exists_offers_to_add_the_symbol_to_that_device() {
    let mut h = new_h();
    let k1: ComponentId = id(&place(&mut h, "coil", [0.0, 0.0]), "component");
    h.s.start("place").unwrap();
    typed(&mut h, "nc");
    typed(&mut h, "T");
    assert_eq!(prompt(&h), "Enter tag for the new device <-K02>:");
    typed(&mut h, "k1");
    assert_eq!(
        prompt(&h),
        "-K01 exists: -K01 (Contactor / relay coil, page 1 \"1\"). Add this NC contact to it? [Yes/Choose another] <Yes>:"
    );
    typed(&mut h, "C");
    assert_eq!(prompt(&h), "Enter tag for the new device <-K02>:", "Choose another goes back to the tag");
    typed(&mut h, "k1");
    typed(&mut h, "Y");
    typed(&mut h, "60,0");
    typed(&mut h, "");
    assert_eq!(h.doc().representations_of(Subject::Component(k1)).count(), 2);
    // A tag nobody has makes a new device with it.
    h.s.start("place").unwrap();
    typed(&mut h, "coil");
    typed(&mut h, "D");
    typed(&mut h, "SR7");
    assert_eq!(prompt(&h), "No device -SR07. Make it a new device with that tag? [Yes/Choose another] <Yes>:");
    typed(&mut h, "");
    typed(&mut h, "120,0");
    typed(&mut h, "");
    assert!(h.doc().components.values().any(|c| c.properties.get("tag").map(String::as_str) == Some("-SR07")));
}

#[test]
fn g_device_properties_renames_and_asks_before_merging() {
    let mut f = k1();
    f.h.activate(f.p1);
    let k2: ComponentId = id(&place(&mut f.h, "no", [100.0, 0.0]), "component");
    let item: ItemId =
        id(&f.h.run("devices", json!({"tag": "-K02"})).unwrap()["devices"][0]["representations"][0], "representation");
    let _ = item;
    f.h.s.set_selection(vec![]);
    f.h.s.start("deviceprops").unwrap();
    assert_eq!(prompt(&f.h), "Select a device:");
    let sym = f.h.symbol_item(k2);
    let at = {
        let doc = f.h.doc();
        match &doc.items[&sym].kind {
            ecad_model::ItemKind::Symbol(s) => s.placement.origin,
            _ => unreachable!(),
        }
    };
    let h = f.h.handle_of(f.p1, sym);
    f.h.s.input(Input::Pick(vec![h])).unwrap();
    assert!(f.h.s.log.iter().any(|l| l == "Tag: -K02"));
    assert!(
        prompt(&f.h).starts_with("Enter property to change or [Tag/Description/Maker/Part/Location/Function]"),
        "{}: {:?}",
        prompt(&f.h),
        f.h.s.log
    );
    typed(&mut f.h, "Maker");
    typed(&mut f.h, "Acme");
    assert!(f.h.s.log.iter().any(|l| l == "Maker of -K02 set to \"Acme\"."));
    typed(&mut f.h, "Tag");
    assert_eq!(prompt(&f.h), "New tag <-K02>:");
    let head = f.h.head();
    typed(&mut f.h, "K01");
    assert!(
        prompt(&f.h).contains("Make this the same device? [Yes/Choose another] <Choose another>"),
        "{}",
        prompt(&f.h)
    );
    typed(&mut f.h, "");
    assert_eq!(f.h.head(), head, "Choose another writes nothing");
    assert_eq!(prompt(&f.h), "New tag <-K02>:", "and asks for another tag");
    typed(&mut f.h, "K01");
    typed(&mut f.h, "Y");
    assert!(!f.h.doc().components.contains_key(&k2), "merged into -K01");
    typed(&mut f.h, "");
    assert!(f.h.s.running.is_none());
    let _ = at;
    f.h.check();
}

// ---------------------------------------------------------------- H (refusals, hostile input)

#[test]
fn h_refusals_say_why_and_write_nothing() {
    let mut f = k1();
    let head = f.h.head();
    let cases: Vec<(Value, &str)> = vec![
        (json!({"symbol": "coil", "at": [0, 0], "device": f.k1.hex()}), "-K01 already has a coil."),
        (json!({"symbol": "xyz", "at": [0, 0]}), "No symbol \"xyz\". Type List to see them."),
        (json!({"symbol": "no", "at": [0, 0], "device": "123456"}), "No such device."),
        (json!({"symbol": "no", "at": [0, 0], "device": f.k1.hex(), "tag": "-K9"}), "`tag` is for a new device"),
        (json!({"symbol": "no", "at": [f64::MAX, 0]}), "Point out of range."),
        (json!({"symbol": "no", "at": [0, 0], "tag": "a\u{7}"}), "A tag is 1 to 40"),
        (json!({"symbol": "no", "at": [0, 0], "tag": "K".repeat(10_000)}), "A tag is 1 to 40"),
        (json!({"symbol": "no", "definition": "1", "at": [0, 0]}), "mutually exclusive"),
    ];
    for (params, want) in cases {
        let e = f.h.run("place", params.clone()).unwrap_err();
        assert!(e.contains(want), "{params}: {e}");
    }
    let e = f.h.run("setprop", json!({"component": f.k1.hex(), "key": "maker", "value": "m".repeat(201)})).unwrap_err();
    assert!(e.contains("at most 200 characters"), "{e}");
    assert_eq!(f.h.head(), head);
    // Nine contacts at most.
    for i in 0..7 {
        join(&mut f.h, "no", f.k1, [100.0 + 10.0 * f64::from(i), 0.0]);
    }
    let e = f.h.run("place", json!({"symbol": "no", "at": [0, 50], "device": f.k1.hex()})).unwrap_err();
    assert_eq!(e, "-K01 already has 9 contacts.");
    // A long number is a free-form tag, never a panic.
    let r = f.h.run("place", json!({"symbol": "lamp", "at": [0, 90], "tag": format!("-K{}", "9".repeat(20))})).unwrap();
    assert_eq!(r["joined"], json!(false));
    f.h.check();
}

// ---------------------------------------------------------------- I (erase, delete, move, copy)

#[test]
fn i_erasing_one_symbol_keeps_the_device_and_the_last_one_deletes_it() {
    let mut f = k1();
    let nc_item: ItemId = id(&f.nc, "item");
    let h = f.h.handle_of(f.p2, nc_item);
    f.h.run("erase", json!({"handles": [h.hex()]})).unwrap();
    let doc = f.h.doc();
    assert!(doc.components.contains_key(&f.k1));
    let labels: Vec<String> = doc.terminals_of(f.k1).filter_map(|(_, t)| t.label.clone()).collect();
    assert_eq!(labels.len(), 4, "A1, A2, 13, 14 stay; 21, 22 went: {labels:?}");
    assert!(!labels.contains(&"21".to_string()));
    assert!(f.h.s.log.iter().any(|l| l.starts_with("Removed -K01's NC contact 21/22 (page 2)")), "{:?}", f.h.s.log);
    // Erasing the NO contact's tag text erases that symbol.
    let no_rep = id::<ecad_model::RepresentationId>(&f.no, "representation");
    let text = f.h.p(|p| p.pages[&f.p2].map.tag_text(no_rep, TextOf::Tag)).unwrap();
    f.h.run("erase", json!({"handles": [text.hex()]})).unwrap();
    assert_eq!(f.h.doc().representations_of(Subject::Component(f.k1)).count(), 1);
    // The last symbol: the device goes, and U brings it back.
    f.h.activate(f.p1);
    let coil = f.h.symbol_item(f.k1);
    let ch = f.h.handle_of(f.p1, coil);
    f.h.run("erase", json!({"handles": [ch.hex()]})).unwrap();
    assert!(!f.h.doc().components.contains_key(&f.k1));
    assert!(f.h.s.log.iter().any(|l| l == "Deleted -K01 with its last symbol; U brings it back."));
    f.h.run("u", Value::Null).unwrap();
    assert!(f.h.doc().components.contains_key(&f.k1));
    f.h.check();
}

#[test]
fn i_delete_device_asks_first() {
    let mut f = k1();
    let item: ItemId = id(&f.no, "item");
    let h = f.h.handle_of(f.p2, item);
    let head = f.h.head();
    f.h.s.set_selection(vec![h]);
    f.h.s.start("devicedelete").unwrap();
    assert_eq!(prompt(&f.h), "Delete -K01 and its 3 symbols on pages 1, 2? [Yes/No] <No>:");
    typed(&mut f.h, "N");
    assert!(f.h.s.log.iter().any(|l| l == "Nothing deleted."));
    assert_eq!(f.h.head(), head);
    f.h.s.set_selection(vec![h]);
    f.h.s.start("devicedelete").unwrap();
    typed(&mut f.h, "Y");
    assert!(!f.h.doc().components.contains_key(&f.k1));
    assert_eq!(f.h.head(), head + 1, "one transaction");
    f.h.check();
}

#[test]
fn i_moving_a_contact_moves_its_terminals_and_copying_a_device_gives_the_next_tag() {
    let mut f = k1();
    let w: ConductorId = id(&f.h.run("wire", json!({"points": [[0, 10], [0, 40]]})).unwrap(), "conductor");
    let t13: TerminalId = id(&f.no["terminals"][0], "terminal");
    assert_eq!(f.h.doc().conductors[&w].end(End::A), Some(t13));
    let item: ItemId = id(&f.no, "item");
    let h = f.h.handle_of(f.p2, item);
    f.h.run("move", json!({"handles": [h.hex()], "delta": [5, 0]})).unwrap();
    let a = f.h.route_ends(f.p2, w).0;
    assert!(a.near(ecad_model::Pt::new(5.0, 10.0), 1e-9), "the wire end followed its terminal: {a:?}");
    f.h.activate(f.p1);
    let coil = f.h.symbol_item(f.k1);
    let ch = f.h.handle_of(f.p1, coil);
    f.h.run("copy", json!({"handles": [ch.hex()], "delta": [0, 60]})).unwrap();
    let doc = f.h.doc();
    let tags: Vec<&str> = doc.components.values().filter_map(|c| c.properties.get("tag").map(String::as_str)).collect();
    assert!(tags.contains(&"-K02"), "the copy is a new device with the next tag: {tags:?}");
    f.h.check();
}

#[test]
fn hover_lens_names_the_device_its_symbols_and_wires() {
    let mut f = k1();
    let rep = id::<ecad_model::RepresentationId>(&f.no, "representation");
    f.h.run("setprop", json!({"component": f.k1.hex(), "key": "maker", "value": "Acme"})).unwrap();
    let lines = f.h.p(|p| devices::hover_lines(p.ledger.document(), f.k1, rep));
    assert_eq!(lines[0], "Device -K01   Contactor / relay coil");
    assert_eq!(lines[1], "Maker: Acme   Part number: ?");
    assert_eq!(lines[2], "This symbol: NO contact 13/14, page 2 \"Control\"");
    assert_eq!(lines[3], "Also: coil A1/A2 page 1 \"1\"; NC contact 21/22 page 2 \"Control\"");
    assert_eq!(lines[4], "  13 (not connected)        14 (not connected)");
    let _ = Vec2::ZERO;
}

// ---------------------------------------------------------------- project settings (owner, 2026-11-08)

#[test]
fn tag_settings_change_padding_and_prefixes_and_survive_reopening() {
    let mut h = new_h();
    let r = h.run("tagsettings", json!({})).unwrap();
    assert_eq!(
        (r["changed"].clone(), r["padding"].clone(), r["prefixes"]["coil"].clone()),
        (json!(false), json!(2), json!("-K"))
    );
    let head = h.head();
    let r = h.run("tagsettings", json!({"padding": 3, "prefixes": {"coil": "-ka", "mcb1": "F"}})).unwrap();
    assert_eq!(r["changed"], json!(true));
    assert_eq!(h.head(), head + 1, "one transaction");
    assert!(matches!(last_ops(&mut h).as_slice(), [Op::SetProjectSettings { .. }]));
    assert_eq!(place(&mut h, "coil", [0.0, 0.0])["tag"], json!("-KA001"));
    assert_eq!(place(&mut h, "mcb1", [40.0, 0.0])["tag"], json!("-F001"));
    assert_eq!(place(&mut h, "no", [80.0, 0.0])["tag"], json!("-K001"), "other symbols keep the built-in prefix");
    // Typed tags are padded the same way.
    let c: ComponentId = id(&place(&mut h, "lamp", [120.0, 0.0]), "component");
    h.run("setprop", json!({"component": c.hex(), "key": "tag", "value": "h7"})).unwrap();
    assert_eq!(tag(&mut h, c), "-H007");
    // null goes back to the built-in prefix; refusals.
    h.run("tagsettings", json!({"prefixes": {"coil": null}})).unwrap();
    assert_eq!(place(&mut h, "coil", [0.0, 40.0])["tag"], json!("-K002"));
    for (params, want) in [
        (json!({"padding": 0}), "`padding` must be a whole number from 1 to 6"),
        (json!({"padding": 7}), "`padding` must be a whole number from 1 to 6"),
        (json!({"prefixes": {"coil": "K1"}}), "A prefix is 1 to 10 letters"),
        (json!({"prefixes": {"xyz": "K"}}), "No symbol \"xyz\""),
    ] {
        let e = h.run("tagsettings", params.clone()).unwrap_err();
        assert!(e.contains(want), "{params}: {e}");
    }
    // Undo takes the settings back with everything else.
    h.run("u", Value::Null).unwrap();
    h.run("u", Value::Null).unwrap();
    assert_eq!(h.doc().settings.tag_prefixes.get("coil").map(String::as_str), Some("KA"));
    h.check();
}

/// Brief 021 (overseer answer C): the tag high-water mark travels with the snapshot. Delete -K05,
/// save (the save writes a snapshot at the head), reopen from that snapshot: the next tag is -K06,
/// never -K05, although the history before the snapshot is not read.
#[test]
fn a_tag_deleted_before_the_snapshot_is_not_given_again_after_reopening() {
    let mut h = new_h();
    let mut k5 = None;
    for i in 0..5 {
        k5 = Some(id(&place(&mut h, "coil", [50.0 * f64::from(i), 0.0]), "component"));
    }
    let k5: ComponentId = k5.unwrap();
    h.run("ecaddelete", json!({"component": k5.hex()})).unwrap();
    h.cfg(|p| p.persistence = ecad_cadcraft::Persistence::File);
    let dir = std::env::temp_dir().join(format!("ecad-021-c-{}", ecad_store::disk::unique()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("k05.ecad").display().to_string();
    h.run("saveas", json!({"path": path})).unwrap();
    let head = h.head();
    while !h.s.docs.is_empty() {
        h.s.execute("close", &json!({"index": 0})).unwrap();
    }
    let mut s = cadcraft_engine::Session::empty();
    let (project, _) = ecad_cadcraft::ProjectHandle::open_file(&mut s, &path, 78).unwrap();
    let mut h2 = H { s, project, trail: Vec::new() };
    assert_eq!(h2.p(|p| (p.ledger.base(), p.ledger.log().len())), (head, 0), "opened from the snapshot");
    let coil = symbols::by_key("coil").unwrap();
    assert_eq!(h2.p(|p| devices::next_tag_preview(p, coil)).as_deref(), Some("-K06"));
    let placed = place(&mut h2, "coil", [400.0, 0.0]);
    let c: ComponentId = id(&placed, "component");
    let tag = h2.p(|p| p.ledger.document().components[&c].properties.get("tag").cloned());
    assert_eq!(tag.as_deref(), Some("-K06"));
    drop(h2);
    let _ = std::fs::remove_dir_all(&dir);
}
