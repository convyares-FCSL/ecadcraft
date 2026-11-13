//! Brief 045a (design 045 §2, §3, §9 a-rows, §11, §13 A–E, M, T): racks, slots and cards.

use ecad_cadcraft::checks::{Fix, Kind};
use ecad_cadcraft::outputs::accessory_fix_commands;
use ecad_cadcraft::{Persistence, ProjectHandle};
use ecad_model::{Author, ComponentId, ObjectRef, Op};
use serde_json::{Value, json};

use super::{checks_of, place, placed, prop, same_as_built, tag};
use crate::common::{H, id};

/// A CPU and six cards (fixture P's rack, design 045 §13 A): `plccm`, `plcdi`, `plcdi`, `plcai`,
/// `plciol`, `plcdq`, each placed with Enter's defaults.
struct Rack {
    h: H,
    cpu: ComponentId,
    cards: Vec<ComponentId>,
}

fn rack(seed: u64) -> Rack {
    let mut h = H::new(seed);
    let cpu = placed(&mut h, "plccpu", 50.0, 200.0);
    let mut cards = Vec::new();
    for (i, key) in ["plccm", "plcdi", "plcdi", "plcai", "plciol", "plcdq"].iter().enumerate() {
        cards.push(placed(&mut h, key, 100.0 + 40.0 * i as f64, 200.0));
    }
    Rack { h, cpu, cards }
}

fn slot(h: &mut H, c: ComponentId) -> Option<String> {
    prop(h, c, "plc.slot")
}

fn rack_of(h: &mut H, c: ComponentId) -> Option<String> {
    prop(h, c, "plc.rack")
}

fn hand_made(h: &mut H, ops: Vec<Op>) {
    h.cfg(|p| p.ledger.commit(Author::User, "hand-made", ops).map(|_| ()).unwrap());
}

fn set(c: ComponentId, key: &str, before: Option<String>, after: Option<&str>) -> Op {
    Op::SetProperty { object: ObjectRef::Component(c), key: key.into(), before, after: after.map(str::to_string) }
}

/// The rows of `kind` a fresh build finds (hand-made files bypass the kept model's commit path).
fn built_rows(h: &mut H, kind: Kind) -> Vec<ecad_cadcraft::checks::Check> {
    ecad_cadcraft::checks::all(&h.doc(), 1e-6).into_iter().filter(|c| c.kind == kind).collect()
}

/// Run fix button `i` of `fix` as the Checks drawer does.
fn run_fix(h: &mut H, fix: Fix, i: usize) -> Value {
    let (cmd, params) = h
        .cfg(|p| accessory_fix_commands(p.outputs(), fix).into_iter().nth(i).map(|(_, c, v)| (c.to_string(), v)))
        .unwrap_or_else(|| panic!("no button {i} for {fix:?}"));
    h.run(&cmd, params).unwrap_or_else(|e| panic!("{cmd}: {e}"))
}

#[test]
fn a1_cards_take_the_next_free_slot_of_the_only_rack_one_transaction_each() {
    let mut h = H::new(4501);
    let cpu = placed(&mut h, "plccpu", 50.0, 200.0);
    assert_eq!(tag(&mut h, cpu), "-A01");
    let mut cards = Vec::new();
    for (i, key) in ["plccm", "plcdi", "plcdi", "plcai", "plciol", "plcdq"].iter().enumerate() {
        let before = h.head();
        let v = place(&mut h, key, 100.0 + 40.0 * i as f64, 200.0, json!({})).unwrap();
        assert_eq!(h.head(), before + 1, "one transaction for {key}");
        assert_eq!(v["rack"], json!(cpu.hex()), "{v}");
        assert_eq!(v["slot"], json!(i + 1), "{v}");
        let c: ComponentId = id(&v, "component");
        assert!(v["message"].as_str().unwrap().contains(&format!("in -A01 slot {}", i + 1)), "{v}");
        cards.push(c);
    }
    // The CPU itself takes no slot and no prompt.
    assert_eq!(rack_of(&mut h, cpu), None);
    let r = h.run("plcracks", json!({})).unwrap();
    let racks = r["racks"].as_array().unwrap();
    assert_eq!(racks.len(), 1);
    assert_eq!(racks[0]["tag"], "-A01");
    let listed: Vec<Value> = racks[0]["cards"].as_array().unwrap().iter().filter(|s| s["free"] == false).cloned().collect();
    let types: Vec<&str> = listed.iter().map(|c| c["type"].as_str().unwrap()).collect();
    assert_eq!(types, ["CM", "DI", "DI", "AI", "IOL", "DQ"]);
    let slots: Vec<u64> = listed.iter().map(|c| c["slot"].as_u64().unwrap()).collect();
    assert_eq!(slots, [1, 2, 3, 4, 5, 6]);
    assert_eq!(listed[1]["channels"], 16);
    assert_eq!(listed[3]["channels"], 4);
    assert_eq!(racks[0]["cards"].as_array().unwrap().len(), 32, "every slot listed, free ones too");
    assert!(r["unracked"].as_array().unwrap().is_empty());
    same_as_built(&mut h);
    // Named rack and slot, and `rack: null`.
    let v = place(&mut h, "plcdi", 400.0, 200.0, json!({"rack": "-A01", "slot": 9})).unwrap();
    assert_eq!(v["slot"], 9);
    let v = place(&mut h, "plcdi", 440.0, 200.0, json!({"rack": null})).unwrap();
    assert!(v.get("rack").is_none());
    let c: ComponentId = id(&v, "component");
    assert_eq!(rack_of(&mut h, c), None);
    // A taken slot is refused with the way out.
    let e = place(&mut h, "plcdi", 480.0, 200.0, json!({"slot": 2})).unwrap_err();
    assert!(e.contains("Slot 2 of -A01 holds -A03 (DI 16)") && e.contains("next free one (7)"), "{e}");
}

#[test]
fn a2_plcslot_refuses_a_taken_slot_swaps_takes_out_and_undoes() {
    let Rack { mut h, cards, cpu } = rack(4502);
    let (a03, a04) = (cards[1], cards[2]);
    assert_eq!(tag(&mut h, a04), "-A04");
    let before = h.head();
    let e = h.run("plcslot", json!({"card": "-A04", "slot": 2})).unwrap_err();
    assert!(e.contains("Slot 2 of -A01 holds -A03") && e.contains("next free one (7)"), "{e}");
    assert_eq!(h.head(), before, "a refusal records nothing");
    let v = h.run("plcslot", json!({"card": a04.hex(), "slot": 2, "swap": true})).unwrap();
    assert_eq!(h.head(), before + 1, "one transaction");
    assert_eq!((slot(&mut h, a04), slot(&mut h, a03)), (Some("2".into()), Some("3".into())), "{v}");
    assert!(v["message"].as_str().unwrap().starts_with("-A04: -A01 slot 3 → -A01 slot 2"), "{v}");
    same_as_built(&mut h);
    h.cmdline("u");
    assert_eq!((slot(&mut h, a04), slot(&mut h, a03)), (Some("3".into()), Some("2".into())));
    // Take out, then put back in the next free slot.
    h.run("plcslot", json!({"card": "-A04", "rack": null})).unwrap();
    assert_eq!((rack_of(&mut h, a04), slot(&mut h, a04)), (None, None));
    h.run("plcslot", json!({"card": "-A04"})).unwrap();
    assert_eq!((rack_of(&mut h, a04), slot(&mut h, a04)), (Some(cpu.hex()), Some("3".into())), "the lowest free slot");
    h.cmdline("u");
    h.cmdline("u");
    assert_eq!(slot(&mut h, a04), Some("3".into()));
    // A rack is not a card; a second rack must be named.
    let e = h.run("plcslot", json!({"card": "-A01", "slot": 2})).unwrap_err();
    assert!(e.contains("is a CPU"), "{e}");
    placed(&mut h, "plcim", 50.0, 400.0);
    let v = h.run("plcslot", json!({"card": "-A04", "rack": "-A08", "slot": 1})).unwrap();
    assert!(v["message"].as_str().unwrap().contains("-A08 slot 1"), "{v}");
    h.run("plcslot", json!({"card": "-A04", "rack": null})).unwrap();
    let e = h.run("plcslot", json!({"card": "-A04"})).unwrap_err();
    assert!(e.contains("Name a rack: the project has 2 (-A01, -A08)"), "{e}");
    same_as_built(&mut h);
}

#[test]
fn a3_slot_rows_appear_and_every_fix_works() {
    let Rack { mut h, cards, cpu } = rack(4503);
    // slot-range: a card in slot 40 of a 32-slot rack.
    h.run("setprop", json!({"component": cards[1].hex(), "key": "plc.slot", "value": "40"})).unwrap();
    let rows = checks_of(&mut h, "slot-range");
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(rows[0]["which"].as_str().unwrap().contains("-A03 is in slot 40; -A01 has slots 1–32"), "{rows:?}");
    let fix = Fix::Plc(cards[1], ecad_cadcraft::plc::PlcFix::SlotRange);
    run_fix(&mut h, fix, 0);
    assert_eq!(prop(&mut h, cpu, "plc.slots"), Some("40".into()), "Raise -A01 to 40 slots");
    assert!(checks_of(&mut h, "slot-range").is_empty());
    h.cmdline("u");
    run_fix(&mut h, fix, 1);
    assert_eq!(slot(&mut h, cards[1]), Some("2".into()), "Move to slot 2 (its old slot is the lowest free one)");
    assert!(checks_of(&mut h, "slot-range").is_empty());
    // slot-twice: two cards in slot 2.
    h.run("setprop", json!({"component": cards[2].hex(), "key": "plc.slot", "value": "4"})).unwrap();
    let rows = checks_of(&mut h, "slot-twice");
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["severity"], "error");
    assert!(rows[0]["which"].as_str().unwrap().contains("-A01 slot 4 holds -A04 and -A05"), "{rows:?}");
    let c: ComponentId = ComponentId::parse(rows[0]["object"]["component"].as_str().unwrap()).unwrap();
    run_fix(&mut h, Fix::Plc(c, ecad_cadcraft::plc::PlcFix::NextFreeSlot), 0);
    assert!(checks_of(&mut h, "slot-twice").is_empty());
    same_as_built(&mut h);
    // unracked: a card in no rack, one count row.
    h.run("plcslot", json!({"card": cards[5].hex(), "rack": null})).unwrap();
    let rows = checks_of(&mut h, "unracked");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["which"], "1 PLC card is in no rack");
    run_fix(&mut h, Fix::Plc(ComponentId(0), ecad_cadcraft::plc::PlcFix::Unracked), 0);
    assert!(checks_of(&mut h, "unracked").is_empty());
    assert_eq!(rack_of(&mut h, cards[5]), Some(cpu.hex()));
    same_as_built(&mut h);
}

#[test]
fn a3_a_dangling_rack_id_is_a_rack_missing_row_with_two_fixes() {
    let Rack { mut h, cards, .. } = rack(4504);
    let a03 = cards[1];
    let before = rack_of(&mut h, a03);
    hand_made(&mut h, vec![set(a03, "plc.rack", before, Some("0123456789abcdef0123456789abcdef"))]);
    let rows = built_rows(&mut h, Kind::RackMissing);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].which, "-A03 names a rack that is not in the project");
    // The kept model, rebuilt from the file as a reopen would.
    h.cfg(|p| p.outputs = None);
    run_fix(&mut h, Fix::Plc(a03, ecad_cadcraft::plc::PlcFix::RackMissing), 0);
    assert_eq!(slot(&mut h, a03), Some("2".into()), "Put in rack: the only rack's lowest free slot");
    assert!(checks_of(&mut h, "rack-missing").is_empty());
    h.cmdline("u");
    run_fix(&mut h, Fix::Plc(a03, ecad_cadcraft::plc::PlcFix::RackMissing), 1);
    assert_eq!((rack_of(&mut h, a03), slot(&mut h, a03)), (None, None), "Take out of rack");
    // A value that is no ID at all, and the rack of a DI card.
    hand_made(&mut h, vec![set(a03, "plc.rack", None, Some("not an id"))]);
    assert_eq!(built_rows(&mut h, Kind::RackMissing).len(), 1);
    let di = cards[2].hex();
    hand_made(&mut h, vec![set(a03, "plc.rack", Some("not an id".into()), Some(&di))]);
    assert_eq!(built_rows(&mut h, Kind::RackMissing).len(), 1);
}

fn tmp(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ecad t045 {tag} {}", ecad_store::disk::unique()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a4_save_and_reopen_keep_racks_slots_and_the_rack_reference() {
    let Rack { mut h, cpu, cards } = rack(4505);
    h.run("setprop", json!({"component": cards[1].hex(), "key": "plc.channels", "value": "32"})).unwrap();
    let path = tmp("roundtrip").join("plc.ecad").display().to_string();
    h.cfg(|p| p.persistence = Persistence::File);
    h.run("saveas", json!({"path": path})).unwrap();
    let saved = h.doc();
    while !h.s.docs.is_empty() {
        h.s.execute("close", &json!({"index": 0})).unwrap();
    }
    drop(h);
    let mut s = cadcraft_engine::Session::empty();
    let (project, _) = ProjectHandle::open_file(&mut s, &path, 4505).unwrap();
    let mut h = H { s, project, trail: Vec::new() };
    let d = h.doc();
    for c in &cards {
        assert_eq!(d.components.get(c).map(|x| &x.properties), saved.components.get(c).map(|x| &x.properties), "{c:?}");
    }
    assert_eq!(rack_of(&mut h, cards[0]), Some(cpu.hex()), "plc.rack round-trips as the rack's ID");
    assert_eq!(prop(&mut h, cards[1], "plc.channels"), Some("32".into()));
    let r = h.run("plcracks", json!({})).unwrap();
    assert_eq!(r["racks"][0]["cards"][1]["channels"], 32);
    // Retag the rack: no card property changes; captions follow.
    let props: Vec<_> = cards.iter().map(|c| d.components.get(c).map(|x| x.properties.clone())).collect();
    h.run("setprop", json!({"component": cpu.hex(), "key": "tag", "value": "-A10"})).unwrap();
    let d = h.doc();
    for (c, p) in cards.iter().zip(props) {
        let now = d.components.get(c).map(|x| x.properties.clone());
        assert_eq!(now, p, "a retag changes no card property");
    }
    let page = h.first_page();
    let captions: Vec<String> = ecad_cadcraft::devices::all_texts_on_page(&d, page)
        .into_iter()
        .filter(|t| t.text.of == ecad_cadcraft::devices::TextOf::Caption)
        .map(|t| t.text.value)
        .collect();
    assert_eq!(captions.len(), 6, "{captions:?}");
    assert!(captions.contains(&"-A10 slot 2".to_string()), "{captions:?}");
    let r = h.run("plcracks", json!({})).unwrap();
    assert_eq!(r["racks"][0]["cards"][0]["rack"], "-A10", "lists show the tag, never the ID");
}

#[test]
fn a4_the_caption_pattern_and_show_rule_come_from_the_registry() {
    let Rack { mut h, cards, .. } = rack(4506);
    let page = h.first_page();
    let caption = |h: &mut H, c: ComponentId| {
        let d = h.doc();
        let rep = d.representations_of(ecad_model::Subject::Component(c)).next().map(|(r, _)| r);
        ecad_cadcraft::devices::all_texts_on_page(&d, page)
            .into_iter()
            .find(|t| t.text.of == ecad_cadcraft::devices::TextOf::Caption && Some(t.text.rep) == rep)
            .map(|t| (t.text.value, t.shown))
    };
    assert_eq!(caption(&mut h, cards[1]), Some(("-A01 slot 2".into(), true)));
    h.run("setdefault", json!({"key": "plc.card.caption", "level": "project", "value": "{rack} Slot{slot:02}"})).unwrap();
    assert_eq!(caption(&mut h, cards[1]), Some(("-A01 Slot02".into(), true)));
    let e = h.run("setdefault", json!({"key": "plc.card.caption", "level": "project", "value": "{rack}{nope}"})).unwrap_err();
    assert!(!e.is_empty());
    let e = h.run("setdefault", json!({"key": "plc.card.caption", "level": "project", "value": "x".repeat(10_000)})).unwrap_err();
    assert!(!e.is_empty());
    h.run("setdefault", json!({"key": "plc.text.caption.show", "level": "object", "component": cards[1].hex(), "value": "off"})).unwrap();
    assert_eq!(caption(&mut h, cards[1]), Some(("-A01 Slot02".into(), false)));
    // A card in no rack has no caption.
    h.run("plcslot", json!({"card": cards[2].hex(), "rack": null})).unwrap();
    assert_eq!(caption(&mut h, cards[2]), None);
}

#[test]
fn a5_card_bodies_placed_before_a_rack_are_cards_in_no_rack() {
    let mut h = H::new(4507);
    let di = placed(&mut h, "plcdi", 100.0, 200.0);
    assert_eq!(rack_of(&mut h, di), None, "no rack in the project: no prompt, no slot");
    let rows = checks_of(&mut h, "unracked");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["severity"], "info");
    let cpu = placed(&mut h, "plccpu", 50.0, 200.0);
    run_fix(&mut h, Fix::Plc(ComponentId(0), ecad_cadcraft::plc::PlcFix::Unracked), 0);
    assert_eq!((rack_of(&mut h, di), slot(&mut h, di)), (Some(cpu.hex()), Some("1".into())));
    assert!(checks_of(&mut h, "unracked").is_empty());
    same_as_built(&mut h);
}

#[test]
fn a6_deleting_a_rack_takes_its_cards_out_in_the_same_transaction() {
    let Rack { mut h, cpu, cards } = rack(4508);
    let before = h.head();
    let v = h.run("ecaddelete", json!({"component": cpu.hex()})).unwrap();
    assert_eq!(h.head(), before + 1, "one transaction");
    assert_eq!(v["message"], "Deleted -A01; 6 cards are now in no rack.");
    for c in &cards {
        assert_eq!((rack_of(&mut h, *c), slot(&mut h, *c)), (None, None));
    }
    assert!(checks_of(&mut h, "rack-missing").is_empty(), "no dangling ID is left");
    assert_eq!(checks_of(&mut h, "unracked").len(), 1);
    same_as_built(&mut h);
    h.cmdline("u");
    assert!(h.doc().components.contains_key(&cpu));
    for c in &cards {
        assert_eq!(rack_of(&mut h, *c), Some(cpu.hex()), "undo restores both");
    }
    same_as_built(&mut h);
    // Erasing the rack's last symbol does the same.
    let page = h.first_page();
    let handles = h.rep_handles(page, cpu);
    h.run("erase", json!({"handles": handles})).unwrap();
    assert!(!h.doc().components.contains_key(&cpu));
    for c in &cards {
        assert_eq!(rack_of(&mut h, *c), None);
    }
    same_as_built(&mut h);
}

#[test]
fn a8_an_op_the_update_map_does_not_list_recomputes_nothing() {
    let Rack { mut h, cards, .. } = rack(4509);
    h.cfg(|p| {
        p.outputs();
    });
    let n = h.cfg(|p| p.outputs().plc.recomputed);
    // A wire, a coil, a page property: nothing PLC.
    h.run("wire", json!({"points": [[0.0, 0.0], [0.0, 50.0]]})).unwrap();
    placed(&mut h, "coil", 300.0, 300.0);
    assert_eq!(h.cfg(|p| p.outputs().plc.recomputed), n, "no PLC recompute for unrelated edits");
    // A slot move recomputes the card (and the rack's group), not the whole model.
    h.run("plcslot", json!({"card": cards[1].hex(), "slot": 20})).unwrap();
    let work = h.cfg(|p| p.outputs().plc.last_work);
    assert!((1..=2).contains(&work), "work {work}");
    same_as_built(&mut h);
}

#[test]
fn a9_hostile_slots_channels_and_racks_are_refused_in_words() {
    let Rack { mut h, cards, .. } = rack(4510);
    let c = cards[1].hex();
    for bad in ["-1", "1000", "99999999999999999999", "2.5", "x"] {
        let e = h.run("setprop", json!({"component": c, "key": "plc.slot", "value": bad})).unwrap_err();
        assert!(e.contains("A slot is a whole number from 0 to 999"), "{bad}: {e}");
    }
    let e = h.run("setprop", json!({"component": c, "key": "plc.channels", "value": "257"})).unwrap_err();
    assert!(e.contains("0 to 256"), "{e}");
    let e = h.run("setprop", json!({"component": c, "key": "plc.rack", "value": cards[2].hex()})).unwrap_err();
    assert!(e.contains("is not a rack"), "{e}");
    let e = h.run("plcslot", json!({"card": c, "slot": 99_999_999_999u64})).unwrap_err();
    assert!(e.contains("whole number"), "{e}");
    let e = h.run("plcslot", json!({"card": c, "slot": 50})).unwrap_err();
    assert!(e.contains("has slots 1–32"), "{e}");
    // Hand-made values never panic: they are Checks rows.
    hand_made(
        &mut h,
        vec![
            set(cards[1], "plc.slot", Some("2".into()), Some("99999999999999999999")),
            set(cards[2], "plc.channels", None, Some("100000")),
            set(cards[3], "plc.slot", Some("4".into()), Some("-1")),
        ],
    );
    let d = h.doc();
    let _ = ecad_cadcraft::checks::all(&d, 1e-6);
    let built = Outputs::build(&d);
    assert_eq!(built.plc.cards.get(&cards[2]).map(|x| x.channels), Some(16), "a hostile count reads as the registry's");
    assert_eq!(built.plc.cards.get(&cards[1]).and_then(|x| x.slot), None);
}

#[test]
fn fp1_merge_and_diff_words_show_the_racks_tag_never_its_id() {
    let Rack { mut h, cpu, .. } = rack(4511);
    let d = h.doc();
    assert_eq!(ecad_model::merge::value_words(&[&d], &cpu.hex()), "-A01");
    assert_eq!(ecad_model::merge::value_words(&[&d], "plain text"), "plain text");
    assert_eq!(ecad_model::merge::value_words(&[&d], "0123456789abcdef0123456789abcdef"), "0123456789abcdef0123456789abcdef");
    // The inspector's Rack row shows the tag.
    let page = h.first_page();
    let a03 = placed_card(&mut h);
    let item = h.symbol_item(a03);
    let handle = h.handle_of(page, item);
    let v = h.pv(|p, _| ecad_cadcraft::props::view(p, Some(page), &[handle]));
    let row = v.sections.iter().flat_map(|s| s.rows.iter()).find(|r| r.key == "plc.rack").cloned().expect("a Rack row");
    assert_eq!(row.value, "-A01");
    assert!(row.choices.contains(&"-A01".to_string()) && row.choices.contains(&"None".to_string()));
    let line = ecad_cadcraft::props::edit_line(&ecad_cadcraft::props::Target::Devices(vec![a03]), "plc.rack", Some("None"));
    assert!(line.starts_with("plcslot ") && line.contains("\"rack\":null"), "{line}");
}

fn placed_card(h: &mut H) -> ComponentId {
    placed(h, "plcdi", 600.0, 200.0)
}

use ecad_cadcraft::outputs::Outputs;

fn prompt(h: &H) -> String {
    h.s.current_prompt().map(|p| p.display()).unwrap_or_default()
}

fn typed(h: &mut H, text: &str) {
    if text.is_empty() {
        h.s.input(cadcraft_engine::Input::Enter).unwrap();
    } else {
        h.s.cmdline(text).unwrap();
    }
    h.check();
}

#[test]
fn a1_place_symbol_asks_rack_and_slot_for_a_card_and_enter_takes_both() {
    let mut h = H::new(4512);
    let cpu = placed(&mut h, "plccpu", 50.0, 200.0);
    h.s.start("place").unwrap();
    typed(&mut h, "plcdi");
    assert_eq!(prompt(&h), "Rack or [-A01/None] <-A01>:");
    typed(&mut h, "");
    assert_eq!(prompt(&h), "Slot <1>:");
    typed(&mut h, "");
    assert!(prompt(&h).starts_with("Specify insertion point"), "{}", prompt(&h));
    typed(&mut h, "100,200");
    // The next card goes in the same rack's next free slot; Rack changes it. A taken slot is
    // refused with the way out.
    assert_eq!(prompt(&h), "Specify insertion point (new device -A03 in -A01 slot 2, rotation 0) or [Rotate/Mirror/Device/Tag/Rack]:");
    typed(&mut h, "Rack");
    assert_eq!(prompt(&h), "Rack or [-A01/None] <-A01>:");
    typed(&mut h, "");
    assert_eq!(prompt(&h), "Slot <2>:");
    typed(&mut h, "1");
    assert_eq!(prompt(&h), "Slot <2>:", "a taken slot keeps the prompt");
    assert!(h.s.log.iter().any(|l| l.contains("Slot 1 of -A01 holds -A02")), "{:?}", h.s.log.last());
    typed(&mut h, "4");
    typed(&mut h, "140,200");
    typed(&mut h, "Rack");
    typed(&mut h, "None");
    typed(&mut h, "180,200");
    typed(&mut h, "");
    assert!(h.s.running.is_none());
    let d = h.doc();
    let slots: Vec<(String, Option<String>, Option<String>)> = d
        .components
        .values()
        .filter(|c| c.properties.get("description").is_some_and(|x| x.contains("digital input card")))
        .map(|c| (c.properties.get("tag").cloned().unwrap_or_default(), c.properties.get("plc.rack").cloned(), c.properties.get("plc.slot").cloned()))
        .collect();
    let mut slots = slots;
    slots.sort();
    assert_eq!(
        slots,
        [
            ("-A02".to_string(), Some(cpu.hex()), Some("1".to_string())),
            ("-A03".to_string(), Some(cpu.hex()), Some("4".to_string())),
            ("-A04".to_string(), None, None),
        ]
    );
}
