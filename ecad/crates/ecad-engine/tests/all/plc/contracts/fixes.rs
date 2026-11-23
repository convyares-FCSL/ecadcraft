//! Fixes to PLC racks: merging a rack keeps the rack's conditions,
//! the I/O list follows target changes that are not a tag or a wire property, kept-model paths
//! walk no project, and `plcslot {card}` takes the next free slot.

use ecad_model::{DeviceId, PageId, Subject};
use serde_json::{Value, json};

use crate::common::{H, id};
use crate::plc::contracts::{placed, prop};

fn rack_of(h: &mut H, c: DeviceId) -> Option<String> {
    prop(h, c, "plc.rack")
}

/// A CPU `-A01` with two DI cards in slots 1 and 2, and a contact-only device `-K01` (no head).
fn rack_and_contact(seed: u64) -> (H, DeviceId, Vec<DeviceId>, DeviceId) {
    let mut h = H::new(seed);
    let cpu = placed(&mut h, "plccpu", 50.0, 400.0);
    let cards = vec![placed(&mut h, "plcdi", 100.0, 400.0), placed(&mut h, "plcdi", 140.0, 400.0)];
    let k = placed(&mut h, "no", 300.0, 400.0);
    (h, cpu, cards, k)
}

#[test]
fn fp1_merging_a_rack_by_tag_re_points_its_cards_in_the_same_transaction() {
    let (mut h, cpu, cards, k) = rack_and_contact(4581);
    let ktag = crate::plc::contracts::tag(&mut h, k);
    let before = h.head();
    h.run("setprop", json!({"component": cpu.hex(), "key": "tag", "value": ktag, "merge": true})).unwrap();
    assert_eq!(h.head(), before + 1, "one transaction");
    let d = h.doc();
    assert!(!d.devices().contains_key(&cpu), "the rack device merged away");
    for c in &cards {
        assert_eq!(rack_of(&mut h, *c), Some(k.hex()), "re-pointed to the device that took the rack body");
    }
    assert!(crate::plc::contracts::checks_of(&mut h, "rack-missing").is_empty());
    let r = h.run("plcracks", json!({})).unwrap();
    assert_eq!(r["racks"][0]["tag"], json!(ktag));
    crate::plc::contracts::same_as_built(&mut h);
    h.cmdline("u");
    for c in &cards {
        assert_eq!(rack_of(&mut h, *c), Some(cpu.hex()), "undo puts both back");
    }
}

#[test]
fn fp1_make_them_one_re_points_the_cards_too() {
    // Make them one (026, a branch merge's tag clash) runs 018's `plan_merge`: the same ops.
    let (mut h, cpu, cards, k) = rack_and_contact(4582);
    let d = h.doc();
    let ops = h.cfg(|p| ecad_electrical::devices::merge::plan_merge(&d, &mut p.ids, cpu, k)).unwrap();
    for c in &cards {
        let repointed = ops.iter().any(|op| {
            matches!(op, ecad_model::Op::ModifyDevice { id, after, .. }
                if id == c && matches!(&after.device_type, ecad_model::DeviceType::PlcModule(m) if m.rack == Some(k)))
        });
        assert!(repointed, "card re-pointed in the merge's ops");
    }
    let mut after = d.clone();
    for op in &ops {
        op.apply(&mut after).unwrap();
    }
    assert!(ecad_engine::checks::all(&after, 1e-6).iter().all(|c| c.kind.name() != "rack-missing"), "no dangling ID");
}

/// A DI card `-A02` whose channel 0 is wired through strip terminal `-X01` to lamp `-P01`.
fn wired_through_a_strip(seed: u64) -> (H, DeviceId, DeviceId, PageId) {
    let mut h = H::new(seed);
    placed(&mut h, "plccpu", 50.0, 400.0);
    let di = placed(&mut h, "plcdi", 100.0, 400.0);
    placed(&mut h, "lamp", 100.0, 260.0);
    h.wire_pp(&[(100.0, 250.0), (100.0, 150.0)]).unwrap();
    let v = h.run("terminalblocks", json!({"from": [95.0, 230.0], "to": [105.0, 230.0]})).unwrap();
    let strip: DeviceId = id(&v, "strip");
    h.run("plcchannels", json!({"card": di.hex(), "from": [95.0, 200.0], "to": [115.0, 200.0]})).unwrap();
    let page: PageId = id(&h.run("newpage", json!({"generated": "iolist"})).unwrap(), "page");
    (h, di, strip, page)
}

fn targets(h: &mut H) -> Vec<String> {
    let v = h.run("iolist", json!({"card": "-A02"})).unwrap();
    v["rows"][0]["targets"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|t| format!("{}:{}", t["device"].as_str().unwrap_or(""), t["point"].as_str().unwrap_or("")))
        .collect()
}

#[test]
fn review_2_renumbering_a_strip_patches_the_cards_io_rows() {
    let (mut h, di, strip, _page) = wired_through_a_strip(4583);
    let stag = crate::plc::contracts::tag(&mut h, strip);
    assert!(targets(&mut h).contains(&format!("{stag}:1")), "{:?}", targets(&mut h));
    h.cfg(|p| {
        p.reports();
    });
    h.run("renumber", json!({"kind": "terminal_blocks", "scope": "all", "start": 5})).unwrap();
    let changed = h.cfg(|p| p.reports().plc.rows_changed.clone());
    assert!(changed.contains(&di), "the card's rows are patched");
    assert!(targets(&mut h).contains(&format!("{stag}:5")), "{:?}", targets(&mut h));
}

#[test]
fn review_2_renaming_a_continuation_patches_the_cards_io_rows() {
    let mut h = H::new(4584);
    placed(&mut h, "plccpu", 50.0, 400.0);
    let di = placed(&mut h, "plcdi", 100.0, 400.0);
    // Channel 0 on a wire ending at continuation A; a lamp wired to continuation B.
    h.run("place", json!({"symbol": "continuation", "at": [100, 250], "name": "A"})).unwrap();
    h.wire_pp(&[(100.0, 250.0), (100.0, 150.0)]).unwrap();
    h.run("plcchannels", json!({"card": di.hex(), "from": [95.0, 200.0], "to": [115.0, 200.0]})).unwrap();
    let b = h.run("place", json!({"symbol": "continuation", "at": [300, 250], "name": "B"})).unwrap();
    let b: ecad_model::AidId = id(&b, "aid");
    placed(&mut h, "lamp", 300.0, 160.0);
    h.wire_pp(&[(300.0, 250.0), (300.0, 170.0)]).unwrap();
    h.run("newpage", json!({"generated": "iolist"})).unwrap();
    assert!(!targets(&mut h).iter().any(|t| t.ends_with(":X1")), "{:?}", targets(&mut h));
    h.cfg(|p| {
        p.reports();
    });
    h.run("setprop", json!({"aids": [b.hex()], "key": "name", "value": "A"})).unwrap();
    let changed = h.cfg(|p| p.reports().plc.rows_changed.clone());
    assert!(changed.contains(&di), "the card's rows are patched");
    assert!(targets(&mut h).iter().any(|t| t.ends_with(":X1")), "the lamp is a target now: {:?}", targets(&mut h));
}

#[test]
fn review_3_kept_model_paths_walk_no_project() {
    let mut h = H::new(4585);
    placed(&mut h, "plccpu", 50.0, 400.0);
    let di = placed(&mut h, "plcdi", 100.0, 400.0);
    placed(&mut h, "plcai", 140.0, 400.0);
    for i in 0..4 {
        let x = 100.0 + 20.0 * f64::from(i);
        h.wire_pp(&[(x, 250.0), (x, 150.0)]).unwrap();
    }
    h.run("plcchannels", json!({"card": di.hex(), "from": [95.0, 200.0], "to": [175.0, 200.0]})).unwrap();
    h.cfg(|p| {
        p.reports();
    });
    // A channel's text (its page with cells redrawn) and a card's channel count (its rack's pages).
    // Run straight on the session: the harness's own check projects pages without the kept model.
    h.s.execute("plcchannel", &json!({"card": di.hex(), "channel": 0, "text": "Pump"})).unwrap();
    h.s.execute("setprop", &json!({"component": di.hex(), "key": "plc.channels", "value": "8"})).unwrap();
    // The inspector on a cell.
    let page = h.first_page();
    let d = h.doc();
    let cell = d
        .representations_of(Subject::Device(di))
        .find(|(_, r)| r.role == ecad_model::Role::Part(ecad_model::PartRole::Channel))
        .and_then(|(_, r)| r.items.first().copied())
        .expect("a cell");
    let handle = h.handle_of(page, cell);
    let v = h.pv(|p, _| ecad_engine::properties::props::view(p, Some(page), &[handle]));
    let row = v.sections.iter().flat_map(|s| s.rows.iter()).find(|r| r.label == "Address").cloned().expect("an Address row");
    assert!(row.value.starts_with("I0."), "{}", row.value);
    // A station's modules come from the index (design 072 §3): no walk to count.
    h.check();
}

#[test]
fn review_3_the_io_sheet_count_comes_from_kept_row_counts() {
    let mut h = H::new(4586);
    placed(&mut h, "plccpu", 50.0, 400.0);
    for (i, key) in ["plcdi", "plcai", "plcdq"].iter().enumerate() {
        placed(&mut h, key, 100.0 + 40.0 * i as f64, 400.0);
    }
    let di = h.doc().devices().iter().find(|(_, c)| c.tag() == Some("-A02")).map(|(c, _)| *c).unwrap();
    for n in ["16", "64", "120"] {
        h.run("setprop", json!({"component": di.hex(), "key": "plc.channels", "value": n})).unwrap();
        let (kept, laid) = h.cfg(|p| {
            p.reports();
            let o = p.kept_reports().unwrap();
            // Brief 051 (design 051 §4.1): the packing of the rows as made.
            let d = p.ledger.document();
            let made: Vec<usize> = ecad_electrical::plc::iolist::cards_in_order(&o.plc)
                .iter()
                .map(|c| ecad_electrical::plc::iolist::rows_of(d, &o.plc, c).len())
                .collect();
            let per = ecad_electrical::pages::generated::rows_per_sheet();
            (o.plc.iolist_sheets(per), ecad_electrical::plc::sheets::pack(&made, 2.0, 1.0, 1.0, per as f64).len())
        });
        assert_eq!(kept, laid, "{n} channels");
    }
}

#[test]
fn tester_045a_plcslot_without_a_slot_moves_to_the_next_free_slot() {
    let mut h = H::new(4587);
    placed(&mut h, "plccpu", 50.0, 400.0);
    let a = placed(&mut h, "plcdi", 100.0, 400.0);
    placed(&mut h, "plcdi", 140.0, 400.0);
    let v: Value = h.run("plcslot", json!({"card": a.hex()})).unwrap();
    assert_eq!(v["slot"], 3, "{v}");
}
