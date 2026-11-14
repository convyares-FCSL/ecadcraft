//! Brief 056 (the PLC signal list, design 056 §13.1, with the owner's change of 2026-11-14: no
//! sync, export and a one-way import): tags and safe states, Drives, Make tags, the generic CSV tag
//! table, Import an I/O list, Export tags, the card type, the rack page in the book, Checks, and
//! hostile files.

use ecad_cadcraft::outputs::Outputs;
use ecad_cadcraft::plc::tagtable::{self, Caps, TagFormat};
use ecad_model::ComponentId;
use serde_json::{Value, json};

use crate::common::H;

/// The generic tag table of fixture P56 (design 056 §13): `%Q2.2` is `Valve_In`, `%Q3.0` is
/// missing, `%Q3.4` is `Spare_Out4`, `%I8.0` is `Door_Sw`, and 300 memory tags.
pub const P56_TAGS: &str = include_str!("../../../../../tests/fixtures/plc/P56-tags.csv");
/// The same as a spreadsheet saves it: `;`, a BOM, capitalised headers.
pub const P56_TAGS_B: &str = include_str!("../../../../../tests/fixtures/plc/P56-tags-b.csv");

/// -A03's channels 0–8 as board 2.03: (tag, description).
pub const A03: [(&str, &str); 9] = [
    ("Fan_On", "Fan contactor on"),
    ("Heater_On", "Heater relay on"),
    ("Valve_Inlet", "Inlet valve open"),
    ("Valve_Drain", "Drain valve open"),
    ("Lamp_Fault", "Fault lamp"),
    ("Lamp_Auto", "Automatic mode lamp"),
    ("Horn", "Alarm horn"),
    ("Pump2_Run", "Standby pump contactor"),
    ("Dosing_On", "Dosing pump on"),
];

/// Fixture P56's command lines (design 056 §13): a CPU `-A01` with a DI card `-A02` in slot 1, the
/// DQ card `-A03` in slot 2 with channels 0–8 wired to lamps (channel 2 through terminal `-X01`),
/// an AI, an AO and an RTD card; the rack in the `percent` style with outputs from byte 2, so
/// `-A03` is `%Q2.0–%Q3.7`; -A03's tags and descriptions as the board.
pub fn p56_lines() -> Vec<String> {
    let mut v = vec![r#"place {"symbol":"plccpu","at":[30,250]}"#.to_string()];
    for (i, key) in ["plcdi", "plcdq", "plcai", "plcao", "plcrtd"].iter().enumerate() {
        v.push(format!(r#"place {{"symbol":"{key}","at":[{},250]}}"#, 70 + 40 * i));
    }
    v.push(r#"plcaddress {"rack":"-A01","style":"percent","output_start":2}"#.into());
    for i in 0..9 {
        let x = 60 + 20 * i;
        v.push(format!(r#"place {{"symbol":"lamp","at":[{x},160]}}"#));
        v.push(format!(r#"wire {{"points":[[{x},150],[{x},60]]}}"#));
    }
    v.push(r#"place {"symbol":"terminal","at":[100,130]}"#.into());
    v.push(r#"plcchannels {"card":"-A03","from":[55,100],"to":[225,100]}"#.into());
    for (ch, (tag, text)) in A03.iter().enumerate() {
        v.push(format!(r#"plcchannel {{"card":"-A03","channel":{ch},"tag":"{tag}","text":"{text}"}}"#));
    }
    v
}

/// Run a `cmd {json}` line.
fn line(h: &mut H, l: &str) -> Result<Value, String> {
    let (cmd, p) = l.split_once(' ').unwrap_or((l, "{}"));
    h.run(cmd, serde_json::from_str(p).unwrap_or(json!({})))
}

struct F {
    h: H,
    a03: ComponentId,
}

fn p56(seed: u64) -> F {
    let mut h = H::new(seed);
    for l in p56_lines() {
        line(&mut h, &l).unwrap_or_else(|e| panic!("{l}: {e}"));
    }
    let a03 = *h.doc().components_tagged("-A03").first().unwrap();
    F { h, a03 }
}

fn rows(h: &mut H, card: &str) -> Vec<Value> {
    h.run("iolist", json!({"card": card, "limit": 2000})).unwrap()["rows"].as_array().cloned().unwrap()
}

fn same_as_built(h: &mut H) {
    let (kept, built) = h.cfg(|p| (p.outputs().plc.clone(), Outputs::build(p.ledger.document()).plc));
    assert!(kept.same_as(&built), "the kept PLC model differs from a rebuild");
}

fn import(h: &mut H, text: &str, extra: Value) -> Result<Value, String> {
    let mut p = json!({"text": text, "name": "P56-tags.csv"});
    if let (Some(o), Some(e)) = (p.as_object_mut(), extra.as_object()) {
        o.extend(e.clone());
    }
    h.run("plcimport", p)
}

// ---------------------------------------------------------------- A, B: the rows

#[test]
fn a_b_rows_carry_tag_safe_drives_and_test() {
    let F { mut h, .. } = p56(5601);
    let r = rows(&mut h, "-A03");
    assert_eq!(r.len(), 16);
    assert_eq!(r[0]["address"], "%Q2.0", "the percent style");
    assert_eq!(r[8]["address"], "%Q3.0");
    assert_eq!(r[2]["plc_tag"], "Valve_Inlet");
    assert_eq!(r[2]["safe"], "off");
    assert_eq!(r[2]["safe_default"], true);
    assert_eq!(r[2]["test"], Value::Null);
    assert_eq!(r[9]["plc_tag"], Value::Null);
    assert_eq!(r[9]["state"], "spare");
    let di = rows(&mut h, "-A02");
    assert_eq!(di[0]["safe"], Value::Null, "inputs have no safe state");
    let racks = h.run("plcracks", json!({})).unwrap();
    let a03 = racks["racks"][0]["cards"].as_array().unwrap().iter().find(|c| c["tag"] == "-A03").cloned().unwrap();
    assert_eq!(a03["kind_words"], "16 outputs");
    assert_eq!(a03["in_use"], 9);
}

// ---------------------------------------------------------------- C: tags

#[test]
fn c_a_tag_is_one_transaction_and_a_duplicate_is_refused_in_words() {
    let F { mut h, a03 } = p56(5602);
    let head = h.head();
    h.run("plcchannel", json!({"card": "-A03", "channel": 9, "tag": "Spare_9"})).unwrap();
    assert_eq!(h.head(), head + 1);
    let e = h.run("plcchannel", json!({"card": "-A03", "channel": 10, "tag": "valve_INLET"})).unwrap_err();
    assert_eq!(e, "Tag \"valve_INLET\" is already -A03 channel 2 (%Q2.2).");
    assert_eq!(h.head(), head + 1, "refused: nothing recorded");
    // The same tag on another PLC's card is fine: none here, so another rack is not needed; the
    // same channel keeping its own tag is not a duplicate.
    h.run("plcchannel", json!({"card": "-A03", "channel": 2, "tag": "Valve_Inlet"})).unwrap();
    h.cmdline("u");
    assert!(!h.doc().components[&a03].properties.contains_key("plc.ch.9.tag"), "u undoes the tag");
    let e = h.run("plcchannel", json!({"card": "-A03", "channel": 9, "tag": "x".repeat(129)})).unwrap_err();
    assert!(e.contains("1 to 128 characters"), "{e}");
    let e = h.run("plcchannel", json!({"card": "-A03", "channel": 9, "tag": "bad\u{7}tag"})).unwrap_err();
    assert!(e.contains("control"), "{e}");
    same_as_built(&mut h);
}

// ---------------------------------------------------------------- D: safe states

#[test]
fn d_safe_states_default_and_follow_setdefault() {
    let F { mut h, .. } = p56(5603);
    h.run("plcchannel", json!({"card": "-A03", "channel": 4, "safe": "On"})).unwrap();
    let r = rows(&mut h, "-A03");
    assert_eq!((r[4]["safe"].clone(), r[4]["safe_default"].clone()), (json!("on"), json!(false)));
    assert_eq!((r[1]["safe"].clone(), r[1]["safe_default"].clone()), (json!("off"), json!(true)));
    h.run("setdefault", json!({"key": "plc.safe.dq", "level": "project", "value": "on"})).unwrap();
    let r = rows(&mut h, "-A03");
    assert_eq!(r[1]["safe"], "on", "an unset channel follows the default");
    h.run("setdefault", json!({"key": "plc.safe.dq", "level": "project", "value": "hold"})).unwrap();
    let r = rows(&mut h, "-A03");
    assert_eq!(r[4]["safe"], "on", "a set channel keeps its own");
    assert_eq!(r[1]["safe"], "hold");
    let e = h.run("plcchannel", json!({"card": "-A03", "channel": 4, "safe": "4 mA"})).unwrap_err();
    assert!(e.contains("Off, On or Hold"), "{e}");
    let e = h.run("plcchannel", json!({"card": "-A02", "channel": 0, "safe": "off"})).unwrap_err();
    assert!(e.contains("only outputs"), "{e}");
    h.run("plcchannel", json!({"card": "-A05", "channel": 0, "safe": " 4 mA "})).unwrap();
    assert_eq!(rows(&mut h, "-A05")[0]["safe"], "4 mA");
    let e = h.run("plcchannel", json!({"card": "-A05", "channel": 0, "safe": "9999 mA and more"})).unwrap();
    assert_eq!(e["changed"], true, "16 characters is the limit: {e}");
    let e = h.run("plcchannel", json!({"card": "-A05", "channel": 0, "safe": "9999 mA and more!"})).unwrap_err();
    assert!(e.contains("1 to 16"), "{e}");
    same_as_built(&mut h);
}

// ---------------------------------------------------------------- E: Drives

#[test]
fn e_drives_reaches_the_end_device_through_a_terminal() {
    let F { mut h, .. } = p56(5604);
    let r = rows(&mut h, "-A03");
    let d = r[2]["drives"].as_array().unwrap();
    assert_eq!(d.len(), 1, "the terminal is left out: {d:?}");
    assert_eq!(d[0]["device"], "-H03");
    let t = r[2]["targets"].as_array().unwrap();
    assert!(t.iter().any(|x| x["device"] == "-X01"), "the terminal is still a target: {t:?}");
    assert_eq!(r[0]["drives"][0]["device"], "-H01");
}

// ---------------------------------------------------------------- F, G: import

#[test]
fn i_export_then_import_round_trips_and_counts_untagged() {
    let F { mut h, a03 } = p56(5608);
    h.run("plcchannel", json!({"card": "-A03", "channel": 10, "text": "Reserved, no tag"})).unwrap();
    let head = h.head();
    let v = h.run("plcexport", json!({"rack": "-A01"})).unwrap();
    assert_eq!(h.head(), head, "export records nothing");
    assert_eq!(v["rows"], 9);
    assert_eq!(v["left_out"]["no_tag"], 1, "{v}");
    assert!(v["message"].as_str().unwrap().contains("left out 1 without a tag"), "{v}");
    let text = v["text"].as_str().unwrap().to_string();
    assert!(text.starts_with("name,address,type,comment,card,channel,safe,drives,page\r\n"), "{text}");
    // Device tags start with `-`, which a spreadsheet reads as a formula: written as text.
    assert!(text.contains("Valve_Inlet,%Q2.2,Bool,Inlet valve open,'-A03,2,Off,'-H03:X2,"), "{text}");
    // Importing the project's own export changes nothing.
    let v = import(&mut h, &text, json!({})).unwrap();
    assert_eq!(v["written"], false, "{v}");
    assert_eq!(h.head(), head, "nothing to change: no transaction");
    // Clear two tags and a description; the export brings them back in one transaction.
    h.run("plcchannel", json!({"card": "-A03", "channel": 2, "tag": null, "text": null})).unwrap();
    h.run("plcchannel", json!({"card": "-A03", "channel": 3, "tag": null})).unwrap();
    let head = h.head();
    let v = import(&mut h, &text, json!({})).unwrap();
    assert_eq!(h.head(), head + 1);
    assert_eq!((v["tags"].clone(), v["descriptions"].clone()), (json!(2), json!(1)), "{v}");
    let p = h.doc().components[&a03].properties.clone();
    assert_eq!(p.get("plc.ch.2.tag").map(String::as_str), Some("Valve_Inlet"));
    assert_eq!(p.get("plc.ch.2.text").map(String::as_str), Some("Inlet valve open"));
    same_as_built(&mut h);
}

#[test]
fn f_g_import_an_io_list_fills_tags_and_descriptions_one_way() {
    let F { mut h, a03 } = p56(5605);
    let head = h.head();
    let v = import(&mut h, P56_TAGS, json!({"dry_run": true})).unwrap();
    assert_eq!(h.head(), head, "a dry run records nothing");
    assert_eq!((v["read"].clone(), v["io"].clone()), (json!(310), json!(10)), "{v}");
    assert_eq!(v["skipped"]["not_io"], 300);
    assert_eq!(v["tags"], 2, "Valve_In at %Q2.2 and Spare_Out4 at %Q3.4: {v}");
    assert_eq!(v["no_card"], json!(["%I8.0"]));
    let m = v["message"].as_str().unwrap();
    assert!(m.starts_with("P56-tags.csv would set: 2 tags and 2 descriptions on -A03"), "{m}");

    let v = import(&mut h, P56_TAGS, json!({})).unwrap();
    assert_eq!(h.head(), head + 1, "one transaction");
    assert_eq!(v["written"], true);
    let label = h.p(|p| p.ledger.log().last().map(|t| t.label.clone()));
    assert_eq!(label.as_deref(), Some("Import an I/O list"));
    let p = h.doc().components[&a03].properties.clone();
    assert_eq!(p.get("plc.ch.2.tag").map(String::as_str), Some("Valve_In"));
    assert_eq!(p.get("plc.ch.2.text").map(String::as_str), Some("Inlet valve"));
    assert_eq!(p.get("plc.ch.12.tag").map(String::as_str), Some("Spare_Out4"));
    assert_eq!(p.get("plc.ch.8.tag").map(String::as_str), Some("Dosing_On"), "a channel not in the list keeps its tag");
    assert!(!h.doc().properties.keys().any(|k| k.starts_with("plc.sync.")), "nothing of the table is kept");
    same_as_built(&mut h);
    h.cmdline("u");
    let p = h.doc().components[&a03].properties.clone();
    assert_eq!(p.get("plc.ch.2.tag").map(String::as_str), Some("Valve_Inlet"), "u undoes the import whole");
    assert!(!p.contains_key("plc.ch.12.tag"));
    // `what: tags` leaves descriptions alone; the `;` variant reads the same.
    let v = import(&mut h, P56_TAGS_B, json!({"what": "tags"})).unwrap();
    assert_eq!((v["tags"].clone(), v["descriptions"].clone()), (json!(2), json!(0)), "{v}");
}

#[test]
fn j_an_import_that_would_give_two_channels_one_tag_is_refused_whole() {
    let F { mut h, .. } = p56(5609);
    let head = h.head();
    // Two names that differ only in case, at two addresses.
    let e = import(&mut h, "name,address\nPump,%Q3.1\nPUMP,%Q3.2\n", json!({})).unwrap_err();
    assert!(e.starts_with("The import would give two channels the tag \"PUMP\": -A03 channel 9 (%Q3.1) and -A03 channel 10 (%Q3.2)."), "{e}");
    // A name a channel outside the list already holds.
    let e = import(&mut h, "name,address\nHorn,%Q3.1\n", json!({})).unwrap_err();
    assert!(e.contains("\"Horn\"") && e.contains("-A03 channel 6") && e.ends_with("Nothing was imported."), "{e}");
    assert_eq!(h.head(), head, "nothing recorded");
    // A swap of two tags in one list is fine.
    import(&mut h, "name,address\nHorn,%Q2.0\nFan_On,%Q2.6\n", json!({})).unwrap();
    assert_eq!(h.head(), head + 1);
}

#[test]
fn j_other_files_and_a_missing_column_are_refused_in_words() {
    let F { mut h, .. } = p56(5618);
    let head = h.head();
    let e = import(&mut h, "PK\u{3}\u{4}\u{14}\u{0}\u{6}\u{0}[Content_Types].xml", json!({})).unwrap_err();
    assert_eq!(e, tagtable::UNKNOWN);
    let e = import(&mut h, "Tag,Logical Address,Data Type\nA,%Q0.0,Bool\n", json!({})).unwrap_err();
    assert_eq!(e, tagtable::UNKNOWN);
    let e = import(&mut h, "name,type,comment\nA,Bool,x\n", json!({})).unwrap_err();
    assert_eq!(e, "The file has no \"address\" column.", "tester 056, F2");
    let e = h.run("plcimport", json!({"text": "name,address\n", "format": "tia"})).unwrap_err();
    assert!(e.contains("No tag table format \"tia\""), "{e}");
    // The review of #112, 1: `name` is bounded.
    let e = h.run("plcimport", json!({"text": P56_TAGS, "name": "x".repeat(256)})).unwrap_err();
    assert!(e.contains("1 to 255 characters"), "{e}");
    let e = h.run("plcimport", json!({"text": P56_TAGS, "name": "a\u{7}b"})).unwrap_err();
    assert!(e.contains("control"), "{e}");
    let e = h.run("plcimport", json!({"text": P56_TAGS, "what": "everything"})).unwrap_err();
    assert!(e.contains("both, tags or descriptions"), "{e}");
    assert_eq!(h.head(), head, "refusals record nothing");
}

#[test]
fn exported_fields_never_run_as_formulas_and_read_back() {
    let F { mut h, a03 } = p56(5619);
    h.run("plcchannel", json!({"card": "-A03", "channel": 0, "text": "=HYPERLINK(\"http://x\")"})).unwrap();
    h.run("plcchannel", json!({"card": "-A03", "channel": 1, "text": "@SUM(A1)"})).unwrap();
    let text = h.run("plcexport", json!({})).unwrap()["text"].as_str().unwrap().to_string();
    assert!(text.contains("\"'=HYPERLINK(\"\"http://x\"\")\""), "{text}");
    assert!(text.contains(",'@SUM(A1),"), "{text}");
    h.run("plcchannel", json!({"card": "-A03", "channel": 0, "text": null})).unwrap();
    import(&mut h, &text, json!({})).unwrap();
    let p = h.doc().components[&a03].properties.clone();
    assert_eq!(p.get("plc.ch.0.text").map(String::as_str), Some("=HYPERLINK(\"http://x\")"), "read back as written");
}

#[test]
fn m_the_first_rack_brings_the_rack_page_into_the_book() {
    let mut h = H::new(5620);
    let racks = |h: &mut H| {
        let d = h.doc();
        d.page_order.iter().filter(|p| ecad_cadcraft::generated::kind_of(&d, **p) == Some(ecad_cadcraft::generated::GenKind::PlcRack)).count()
    };
    assert_eq!(racks(&mut h), 0);
    let head = h.head();
    line(&mut h, r#"place {"symbol":"plccpu","at":[30,250]}"#).unwrap();
    assert_eq!(h.head(), head + 1, "the rack and its page in one transaction");
    assert_eq!(racks(&mut h), 1, "tester 056, F1");
    line(&mut h, r#"place {"symbol":"plcdq","at":[70,250]}"#).unwrap();
    line(&mut h, r#"place {"symbol":"plccpu","at":[30,350]}"#).unwrap();
    assert_eq!(racks(&mut h), 1, "one page for every rack");
    h.cmdline("u");
    h.cmdline("u");
    h.cmdline("u");
    assert_eq!(racks(&mut h), 0, "undoing the first rack takes its page too");
}

#[test]
fn the_card_type_changes_the_body_in_one_transaction() {
    let F { mut h, a03 } = p56(5621);
    h.run("plcchannel", json!({"card": "-A03", "channel": 4, "safe": "on"})).unwrap();
    let head = h.head();
    let v = h.run("plccardtype", json!({"card": "-A03", "type": "AO"})).unwrap();
    assert_eq!(h.head(), head + 1, "{v}");
    assert_eq!(v["message"], "-A03 is now a AO card.");
    let ty = h.cfg(|p| ecad_cadcraft::plc::card_type_of(p.ledger.document(), a03));
    assert_eq!(ty, Some(ecad_cadcraft::plc::rack::CardType::Ao));
    assert!(!h.doc().components[&a03].properties.contains_key("plc.ch.4.safe"), "`on` is not an analogue safe state");
    same_as_built(&mut h);
    h.cmdline("u");
    let ty = h.cfg(|p| ecad_cadcraft::plc::card_type_of(p.ledger.document(), a03));
    assert_eq!(ty, Some(ecad_cadcraft::plc::rack::CardType::Dq), "u undoes it");
    let e = h.run("plccardtype", json!({"card": "-A03", "type": "CPU"})).unwrap_err();
    assert!(e.contains("DI, DQ, AI, AO, RTD or IOL"), "{e}");
    let e = h.run("plccardtype", json!({"card": "-A01", "type": "DI"})).unwrap_err();
    assert!(e.contains("not a card"), "{e}");
    let v = h.run("plccardtype", json!({"card": "-A03", "type": "dq"})).unwrap();
    assert_eq!(v["changed"], false);
}

#[test]
fn checks_a_tag_twice_names_the_other_channel() {
    let F { mut h, a03 } = p56(5613);
    // Only a raw setprop or a merge can do this; plcchannel refuses it.
    h.run("setprop", json!({"component": a03.hex(), "key": "plc.ch.3.tag", "value": "VALVE_INLET"})).unwrap();
    let rows = h.run("checks", json!({"kinds": ["plc-tag-twice"]})).unwrap()["checks"].as_array().cloned().unwrap();
    assert_eq!(rows.len(), 2, "one row per channel: {rows:?}");
    let which: Vec<String> = rows.iter().map(|r| r["which"].as_str().unwrap_or_default().to_string()).collect();
    assert_ne!(which[0], which[1], "each row names its own channel: {which:?}");
}

#[test]
fn make_tags_fills_empty_tags_from_descriptions_uniquely() {
    let F { mut h, a03 } = p56(5612);
    h.run("plcchannel", json!({"card": "-A03", "channel": 9, "text": "Inlet valve open"})).unwrap();
    h.run("plcchannel", json!({"card": "-A03", "channel": 10, "text": "7-segment display!"})).unwrap();
    h.run("plcchannel", json!({"card": "-A03", "channel": 11, "text": "Fan_On"})).unwrap();
    let head = h.head();
    let v = h.run("plcmaketags", json!({"card": "-A03"})).unwrap();
    assert_eq!(h.head(), head + 1);
    assert_eq!(v["made"], 3, "{v}");
    assert_eq!(v["message"], "Made 3 tags on -A03.");
    let p = h.doc().components[&a03].properties.clone();
    assert_eq!(p.get("plc.ch.9.tag").map(String::as_str), Some("Inlet_valve_open"));
    assert_eq!(p.get("plc.ch.10.tag").map(String::as_str), Some("_7_segment_display"));
    assert_eq!(p.get("plc.ch.11.tag").map(String::as_str), Some("Fan_On_2"), "unique in the PLC");
    let v = h.run("plcmaketags", json!({"rack": "-A01"})).unwrap();
    assert_eq!(v["changed"], false, "{v}");
}

// ---------------------------------------------------------------- Checks (§9)

#[test]
fn the_percent_style_writes_percent_addresses() {
    let F { mut h, .. } = p56(5614);
    let r = rows(&mut h, "-A02");
    assert_eq!(r[0]["address"], "%I0.0");
    let ai = rows(&mut h, "-A04");
    assert!(ai[0]["address"].as_str().unwrap().starts_with("%IW"), "{}", ai[0]);
    assert_eq!(tagtable::key_of("%IX0.0"), tagtable::key_of("I0.0"), "every style matches");
}

// ---------------------------------------------------------------- O: hostile

#[test]
fn o_hostile_files_are_refused_skipped_or_counted_never_a_panic() {
    let F { mut h, .. } = p56(5615);
    let head = h.head();
    let mut many = String::from("name,address\n");
    for k in 0..30_000u32 {
        many.push_str(&format!("T{k},%Q{}.{}\n", k / 8, k % 8));
    }
    let e = import(&mut h, &many, json!({})).unwrap_err();
    assert_eq!(e, "The file has 30000 I/O tags; at most 20000 are read.");
    let long =
        format!("name,address\n{},%Q2.2\nOk,%Q2.9\nOk2,%Q-1.0\nOk3,%Q99999.0\nA,%Q2.3\nA,%Q2.3\nB\u{1},%Q2.4\n\"open,%Q2.5\n", "n".repeat(300));
    let v = import(&mut h, &long, json!({"dry_run": true})).unwrap();
    let p = &v;
    assert_eq!(p["io"], 1, "{p}");
    assert_eq!(p["skipped"]["bad_name"], 2, "{p}");
    assert_eq!(p["skipped"]["bad_address"], 4, "three bad addresses and the unbalanced quote: {p}");
    assert_eq!(p["skipped"]["twice"], 1, "{p}");
    for bad in ["", "\u{0}\u{1}\u{2}garbage", &"x".repeat(1 << 16)] {
        let _ = import(&mut h, bad, json!({}));
    }
    let big = "a".repeat(tagtable::MAX_BYTES + 1);
    let e = import(&mut h, &big, json!({})).unwrap_err();
    assert!(e.contains("at most 16 MB"), "{e}");
    assert_eq!(h.head(), head, "nothing recorded");
    let r = tagtable::GenericCsv.read(b"name,address\r\n\"a\"\"b\",%Q0.0", &Caps::default()).unwrap();
    assert_eq!(r.rows[0].name, "a\"b");
}
