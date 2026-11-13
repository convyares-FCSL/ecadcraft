//! Brief 045c (design 045 §8, §13 N–Q): the I/O list as `iolist`, the generated page and the
//! release CSV; the update map of its rows.

use ecad_model::{ComponentId, PageId};
use serde_json::{Value, json};

use super::{placed, prop};
use crate::common::{H, id, pts};

/// A CPU, a DI card wired to eight lamps through a fence, an AI card and a DQ card.
struct F {
    h: H,
    di: ComponentId,
    lamps: Vec<ComponentId>,
}

fn fixture(seed: u64) -> F {
    let mut h = H::new(seed);
    placed(&mut h, "plccpu", 50.0, 400.0);
    let di = placed(&mut h, "plcdi", 100.0, 400.0);
    placed(&mut h, "plcai", 140.0, 400.0);
    placed(&mut h, "plcdq", 180.0, 400.0);
    let mut lamps = Vec::new();
    for i in 0..8 {
        let x = 100.0 + 10.0 * f64::from(i);
        lamps.push(placed(&mut h, "lamp", x, 260.0));
        h.run("wire", json!({"points": pts(&[(x, 250.0), (x, 150.0)])})).unwrap();
    }
    h.run("plcchannels", json!({"card": di.hex(), "from": [95.0, 200.0], "to": [175.0, 200.0]})).unwrap();
    F { h, di, lamps }
}

fn rows(h: &mut H, params: Value) -> Vec<Value> {
    h.run("iolist", params).unwrap()["rows"].as_array().cloned().unwrap_or_default()
}

#[test]
fn n_one_row_per_channel_of_every_card_with_states_targets_and_wires() {
    let F { mut h, di, .. } = fixture(4541);
    h.run("plcchannel", json!({"card": di.hex(), "channel": 9, "text": "Spare sensor"})).unwrap();
    let head = h.head();
    let v = h.run("iolist", json!({"limit": 2000})).unwrap();
    assert_eq!(h.head(), head, "a read records nothing");
    let all = v["rows"].as_array().unwrap();
    assert_eq!(all.len(), 16 + 4 + 16, "DI 16, AI 4, DQ 16");
    assert_eq!(v["cards"].as_array().unwrap().len(), 3);
    assert_eq!(v["cards"][0]["used"], 8);
    assert_eq!(v["cards"][0]["reserved"], 1);
    assert_eq!(v["cards"][0]["spare"], 7);
    let r0 = &all[0];
    assert_eq!(
        (r0["rack"].clone(), r0["slot"].clone(), r0["tag"].clone(), r0["channel"].clone()),
        (json!("-A01"), json!(1), json!("-A02"), json!(0))
    );
    assert_eq!(r0["address"], "I0.0");
    assert_eq!(r0["state"], "used");
    assert_eq!(r0["pins"], json!([{"role": "in", "pin": "1"}]));
    assert_eq!(r0["drawn"], true);
    assert!(r0["place"].is_string(), "{r0}");
    let t = r0["targets"].as_array().unwrap();
    assert!(t.iter().any(|x| x["point"] == "X2"), "wired to its lamp: {r0}");
    assert!(r0["wire"].is_string(), "the wire number: {r0}");
    assert_eq!(all[9]["state"], "reserved");
    assert_eq!(all[9]["text"], "Spare sensor");
    assert_eq!(all[10]["state"], "spare");
    assert_eq!(all[10]["drawn"], false);
    assert!(r0["editable"].as_array().is_some_and(|e| e.len() == 3));
    // Filters.
    assert_eq!(rows(&mut h, json!({"state": "spare", "limit": 2000})).len(), 7 + 4 + 16);
    assert_eq!(rows(&mut h, json!({"card": "-A02", "limit": 2000})).len(), 16);
    let e = h.run("iolist", json!({"state": "broken"})).unwrap_err();
    assert!(e.contains("used, reserved or spare"), "{e}");
}

#[test]
fn o_the_generated_page_heads_each_card_and_marks_typed_addresses() {
    let F { mut h, di, .. } = fixture(4542);
    h.run("plcchannel", json!({"card": di.hex(), "channel": 3, "address": "I9.9"})).unwrap();
    let page: PageId = id(&h.run("newpage", json!({"generated": "iolist"})).unwrap(), "page");
    let d = h.doc();
    let (sheets, lines) = h.cfg(|p| {
        p.outputs();
        let o = p.outputs.as_ref().unwrap();
        (ecad_cadcraft::generated::sheets(&d, page, Some(o)), ecad_cadcraft::plc::iolist::page_lines(&d, &o.plc))
    });
    // Brief 051 (design 051 §4.1, W7): cards are packed on sheets; these 44 lines fit one.
    assert_eq!(sheets, 1, "cards packed on one sheet");
    let heads: Vec<&String> = lines.iter().filter(|(h, _)| *h).filter_map(|(_, c)| c.first()).collect();
    assert_eq!(heads.len(), 3);
    assert!(heads[0].starts_with("-A01 slot 1  -A02  DI 16"), "{heads:?}");
    assert!(heads[0].contains("8 used, 0 reserved, 8 spare"), "{heads:?}");
    let rows: Vec<&Vec<String>> = lines.iter().filter(|(h, _)| !*h).map(|(_, c)| c).collect();
    assert_eq!(rows[3][3], "I9.9*", "typed address marked");
    assert_eq!(rows[10][4], "Spare");
}

#[test]
fn p_the_release_csv_and_its_missing_reason() {
    let F { mut h, .. } = fixture(4543);
    let slot = ecad_cadcraft::release::LISTS.iter().find(|s| s.id == "io-list").expect("an io-list slot");
    assert_eq!(slot.suffix, " IO list.csv");
    let d = h.doc();
    let t = h.cfg(|p| (slot.build)(&d, p.outputs()).unwrap()).expect("a table");
    assert_eq!(t.headers.first(), Some(&"Rack"));
    assert_eq!(t.rows.len(), 36);
    let csv = ecad_cadcraft::outputs::csv(&t, true);
    assert!(csv.contains("-A02"));
    let mut empty = H::new(4544);
    let d = empty.doc();
    assert!(empty.cfg(|p| (slot.build)(&d, p.outputs()).unwrap()).is_none());
    assert_eq!(slot.missing_why, "No PLC cards with channels.");
}

#[test]
fn q_renaming_a_target_patches_only_its_cards_rows() {
    let F { mut h, di, lamps } = fixture(4545);
    h.cfg(|p| {
        p.outputs();
    });
    h.run("setprop", json!({"component": lamps[0].hex(), "key": "tag", "value": "-P99"})).unwrap();
    let changed = h.cfg(|p| p.outputs().plc.rows_changed.clone());
    assert_eq!(changed.into_iter().collect::<Vec<_>>(), [di], "only the DI card's rows");
    let r = rows(&mut h, json!({"card": "-A02"}));
    assert!(r[0]["targets"].as_array().unwrap().iter().any(|t| t["device"] == "-P99"), "{}", r[0]);
    // A commit touching no PLC data leaves the list alone.
    placed(&mut h, "coil", 400.0, 100.0);
    assert!(h.cfg(|p| p.outputs().plc.rows_changed.is_empty()));
    assert!(prop(&mut h, di, "plc.rack").is_some());
}
