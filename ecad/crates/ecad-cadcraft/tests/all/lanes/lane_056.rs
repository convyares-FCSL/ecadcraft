//! Brief 056 (the PLC signal list and PLC sync, design 056 §13.1): tags and safe states, Drives,
//! Make tags, the generic CSV tag table, import, Compare's take, export, the sync states and their
//! update map, Checks, and hostile files.

use ecad_cadcraft::outputs::Outputs;
use ecad_cadcraft::plc::sync::State;
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
    assert!(kept.same_as(&built), "the kept PLC model (sync included) differs from a rebuild");
}

fn state(h: &mut H, c: ComponentId, i: u32) -> State {
    h.cfg(|p| p.outputs().plc.sync.state(c, i))
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
fn f_g_the_preview_writes_nothing_and_the_import_is_one_transaction() {
    let F { mut h, a03 } = p56(5605);
    let head = h.head();
    let v = import(&mut h, P56_TAGS, json!({"dry_run": true})).unwrap();
    assert_eq!(h.head(), head, "a dry run records nothing");
    let p = &v["preview"];
    assert_eq!((p["read"].clone(), p["io"].clone()), (json!(310), json!(10)));
    assert_eq!(p["skipped"]["not_io"], 300);
    assert_eq!(p["differences"], 4, "3 on -A03 and 1 on no card");
    let s = p["summary"].as_str().unwrap();
    assert!(s.starts_with("310 tags read; 10 with an I/O address. 300 memory and data-block tags skipped."), "{s}");
    assert!(s.contains("3 differences on -A03, 1 not on any card"), "{s}");
    assert_eq!(state(&mut h, a03, 2), State::Nothing, "nothing imported yet");

    import(&mut h, P56_TAGS, json!({})).unwrap();
    assert_eq!(h.head(), head + 1, "one transaction");
    let label = h.p(|p| p.ledger.log().last().map(|t| t.label.clone()));
    assert_eq!(label.as_deref(), Some("Import PLC tags"));
    assert_eq!(state(&mut h, a03, 0), State::InSync);
    assert_eq!(state(&mut h, a03, 2), State::Differs("Valve_In".into()));
    assert_eq!(state(&mut h, a03, 8), State::NotInPlc);
    assert_eq!(state(&mut h, a03, 12), State::OnlyInPlc("Spare_Out4".into()));
    assert_eq!(state(&mut h, a03, 9), State::Free);
    let (n, total, none) = h.cfg(|p| {
        let s = &p.outputs().plc.sync;
        (s.card_count(a03), s.total(), s.no_card.clone())
    });
    assert_eq!((n, total), (3, 4));
    assert_eq!(none, ["I8.0"]);
    let checks = h.run("checks", json!({"kinds": ["plc-differs", "plc-not-in-plc", "plc-no-card"]})).unwrap();
    assert_eq!(checks["checks"].as_array().unwrap().len(), 4, "{checks}");
    same_as_built(&mut h);
    h.cmdline("u");
    assert_eq!(state(&mut h, a03, 2), State::Nothing, "u undoes the import whole");
    assert!(!h.doc().properties.contains_key("plc.sync.table"));
    same_as_built(&mut h);
}

#[test]
fn f_import_with_take_names_takes_differs_and_only_in_plc() {
    let F { mut h, a03 } = p56(5606);
    let head = h.head();
    import(&mut h, P56_TAGS, json!({"take": "names"})).unwrap();
    assert_eq!(h.head(), head + 1, "snapshot and names in one transaction");
    let props = h.doc().components[&a03].properties.clone();
    assert_eq!(props.get("plc.ch.2.tag").map(String::as_str), Some("Valve_In"));
    assert_eq!(props.get("plc.ch.12.tag").map(String::as_str), Some("Spare_Out4"));
    assert_eq!(props.get("plc.ch.12.text").map(String::as_str), Some("Spare output 4"), "an only-in-PLC row takes the comment too");
    assert_eq!(h.cfg(|p| p.outputs().plc.sync.card_count(a03)), 1, "only `not in PLC` is left");
}

// ---------------------------------------------------------------- H: take

#[test]
fn h_use_the_plcs_name_is_one_transaction_and_for_all_too() {
    let F { mut h, a03 } = p56(5607);
    import(&mut h, P56_TAGS, json!({})).unwrap();
    let head = h.head();
    h.run("plctake", json!({"items": [{"card": "-A03", "channel": 2}]})).unwrap();
    assert_eq!(h.head(), head + 1);
    assert_eq!(state(&mut h, a03, 2), State::InSync);
    assert_eq!(h.cfg(|p| p.outputs().plc.sync.card_count(a03)), 2);
    h.run("plctake", json!({"card": "-A03"})).unwrap();
    assert_eq!(h.head(), head + 2, "for all: one transaction");
    assert_eq!(state(&mut h, a03, 12), State::InSync);
    let v = h.run("plctake", json!({"all": true})).unwrap();
    assert_eq!(v["changed"], false, "nothing left to take: {v}");
    assert_eq!(h.head(), head + 2);
    same_as_built(&mut h);
}

// ---------------------------------------------------------------- I, J: export and formats

#[test]
fn i_export_then_reimport_round_trips_and_counts_untagged() {
    let F { mut h, .. } = p56(5608);
    h.run("plcchannel", json!({"card": "-A03", "channel": 10, "text": "Reserved, no tag"})).unwrap();
    let head = h.head();
    let v = h.run("plcexport", json!({"rack": "-A01"})).unwrap();
    assert_eq!(h.head(), head, "export records nothing");
    assert_eq!(v["rows"], 9);
    assert_eq!(v["left_out"]["no_tag"], 1, "{v}");
    assert!(v["message"].as_str().unwrap().contains("left out 1 without a tag"), "{v}");
    let text = v["text"].as_str().unwrap().to_string();
    assert!(text.starts_with("name,address,type,comment,card,channel,safe,drives,page\r\n"), "{text}");
    assert!(text.contains("Valve_Inlet,%Q2.2,Bool,Inlet valve open,-A03,2,Off,-H03:X2,"), "{text}");
    import(&mut h, &text, json!({})).unwrap();
    let first = h.doc().properties.get("plc.sync.table").cloned().unwrap();
    let v2 = h.run("plcsync", json!({})).unwrap();
    assert_eq!(v2["differences"], 0, "an export of the project is in sync: {v2}");
    // A second export and import gives the same snapshot.
    let again = h.run("plcexport", json!({"rack": "-A01"})).unwrap()["text"].as_str().unwrap().to_string();
    import(&mut h, &again, json!({})).unwrap();
    assert_eq!(h.doc().properties.get("plc.sync.table"), Some(&first), "the same snapshot");
}

#[test]
fn j_the_spreadsheet_variant_gives_the_same_states_and_other_files_are_refused() {
    let F { mut h, a03 } = p56(5609);
    import(&mut h, P56_TAGS_B, json!({})).unwrap();
    assert_eq!(state(&mut h, a03, 2), State::Differs("Valve_In".into()));
    assert_eq!(state(&mut h, a03, 12), State::OnlyInPlc("Spare_Out4".into()));
    assert_eq!(h.cfg(|p| p.outputs().plc.sync.total()), 4);
    let head = h.head();
    let e = import(&mut h, "PK\u{3}\u{4}\u{14}\u{0}\u{6}\u{0}[Content_Types].xml", json!({})).unwrap_err();
    assert_eq!(e, tagtable::UNKNOWN);
    let e = import(&mut h, "Name,Logical Address,Data Type\nA,%Q0.0,Bool\n", json!({})).unwrap_err();
    assert_eq!(e, tagtable::UNKNOWN);
    let e = import(&mut h, "name,type\nA,Bool\n", json!({"format": "csv"})).unwrap_err();
    assert_eq!(e, tagtable::UNKNOWN, "no address column: not the generic header");
    let e = h.run("plcimport", json!({"text": "name,address\n", "format": "tia"})).unwrap_err();
    assert!(e.contains("No tag table format \"tia\""), "{e}");
    assert_eq!(h.head(), head, "refusals record nothing");
}

// ---------------------------------------------------------------- K: a card move

#[test]
fn k_a_card_move_moves_addresses_and_states_only() {
    let F { mut h, a03 } = p56(5610);
    import(&mut h, P56_TAGS, json!({})).unwrap();
    let before = h.cfg(|p| p.outputs().plc.sync.builds);
    // Behind the AO card in slot order: the DQ card's block moves.
    h.run("plcslot", json!({"card": "-A03", "slot": 9})).unwrap();
    let r = rows(&mut h, "-A03");
    let a0 = r[0]["address"].as_str().unwrap().to_string();
    assert_ne!(a0, "%Q2.0", "the DQ card's block moved: {a0}");
    assert_eq!(state(&mut h, a03, 0), State::Moved("Q2.0".into()), "Fan_On is in the PLC at its old address");
    let after = h.cfg(|p| p.outputs().plc.sync.builds);
    assert_eq!(after, before, "a card move works out cards, never the whole index");
    same_as_built(&mut h);
}

// ---------------------------------------------------------------- update map (§12)

#[test]
fn update_map_a_tag_edit_works_out_one_card_and_an_unrelated_edit_none() {
    let F { mut h, a03 } = p56(5611);
    import(&mut h, P56_TAGS, json!({})).unwrap();
    let (b0, u0) = h.cfg(|p| (p.outputs().plc.sync.builds, p.outputs().plc.sync.card_updates));
    h.run("plcchannel", json!({"card": "-A03", "channel": 2, "tag": "Valve_In"})).unwrap();
    let (b1, u1, changed) = h.cfg(|p| {
        let plc = &p.outputs().plc;
        (plc.sync.builds, plc.sync.card_updates, plc.rows_changed.clone())
    });
    assert_eq!(b1, b0, "no rebuild");
    assert_eq!(u1 - u0, 1, "one card worked out");
    assert_eq!(changed.into_iter().collect::<Vec<_>>(), [a03], "one card's rows");
    assert_eq!(state(&mut h, a03, 2), State::InSync);
    h.run("place", json!({"symbol": "lamp", "at": [400, 400]})).unwrap();
    let (b2, u2) = h.cfg(|p| (p.outputs().plc.sync.builds, p.outputs().plc.sync.card_updates));
    assert_eq!((b2, u2), (b1, u1), "a commit with no PLC data works out nothing");
    // A new snapshot: one build.
    import(&mut h, P56_TAGS_B, json!({})).unwrap();
    let b3 = h.cfg(|p| p.outputs().plc.sync.builds);
    assert_eq!(b3, 1, "a fresh index, built once");
    same_as_built(&mut h);
}

// ---------------------------------------------------------------- Make tags

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
fn checks_tag_twice_after_take_and_tag_missing_with_a_snapshot() {
    let F { mut h, .. } = p56(5613);
    assert!(h.run("checks", json!({"kinds": ["plc-tag-missing"]})).unwrap()["checks"].as_array().unwrap().is_empty(), "no snapshot: no row");
    h.run("plcchannel", json!({"card": "-A03", "channel": 0, "tag": null})).unwrap();
    import(&mut h, P56_TAGS, json!({})).unwrap();
    let rows = h.run("checks", json!({"kinds": ["plc-tag-missing"]})).unwrap()["checks"].as_array().cloned().unwrap();
    assert_eq!(rows.len(), 1, "{rows:?}");
    // Taking `Valve_In` while channel 3 already holds it makes two channels share a tag.
    h.run("plcchannel", json!({"card": "-A03", "channel": 3, "tag": "VALVE_IN"})).unwrap();
    h.run("plctake", json!({"items": [{"card": "-A03", "channel": 2}]})).unwrap();
    let rows = h.run("checks", json!({"kinds": ["plc-tag-twice"]})).unwrap()["checks"].as_array().cloned().unwrap();
    assert_eq!(rows.len(), 2, "one row per channel: {rows:?}");
}

// ---------------------------------------------------------------- the percent style

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
    let p = &v["preview"];
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

#[test]
fn o_a_snapshot_too_large_for_one_property_is_refused_in_words() {
    let F { mut h, .. } = p56(5616);
    let mut t = String::from("name,address,type,comment\n");
    for k in 0..20_000u32 {
        t.push_str(&format!("Tag_{k:05},%Q{}.{},Bool,{}\n", k / 8, k % 8, "c".repeat(60)));
    }
    let e = import(&mut h, &t, json!({})).unwrap_err();
    assert!(e.contains("at most 1.0 MB is kept"), "{e}");
}

#[test]
fn an_unchanged_snapshot_is_not_recorded_again() {
    let F { mut h, .. } = p56(5617);
    import(&mut h, P56_TAGS, json!({})).unwrap();
    let first = h.p(|p| p.ledger.log().last().map(|t| t.ops.clone())).unwrap();
    let table_ops =
        |ops: &[ecad_model::Op]| ops.iter().filter(|op| matches!(op, ecad_model::Op::SetProperty { key, .. } if key == "plc.sync.table")).count();
    assert_eq!(table_ops(&first), 1, "the first import records the snapshot");
    // The same table again (the `;` variant reads to the same canonical text): the snapshot is
    // not recorded twice, so history does not grow by its size.
    let n = h.p(|p| p.ledger.log().len());
    import(&mut h, P56_TAGS_B, json!({})).unwrap();
    let added: Vec<ecad_model::Op> = h.p(|p| p.ledger.log().iter().skip(n).flat_map(|t| t.ops.clone()).collect());
    assert_eq!(table_ops(&added), 0, "at most the time and source are recorded: {added:?}");
}
