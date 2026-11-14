//! Tester 056 (brief 056, design 056 §13.1 A–O; written from the brief and design only, before the
//! implementation): PLC tag and safe state, Drives, Make tags, the `percent` style, the generic CSV
//! tag table (export, dry run, import as one undoable transaction), the sync states and the chip
//! count, Use PLC's, a card move, the `plcrack` page in the book, update maps and hostile files.
//!
//! The fixture is the tester's own (not the worker's P56 files): a CPU `-A01`, DI `-A02` (slot 1),
//! DQ `-A03` (slot 2) and AO `-A04` (slot 3); eight lamps wired down through a fence on `-A03`, so
//! its channels 0–7 are used and 8–15 free; terminal `-X10` sits on channel 2's wire between the
//! channel and its lamp. Tag tables are synthetic CSV text built here, from the live addresses.
//!
//! Observed only through commands (`iolist`, `plcsync`, `plcexport`, `plcimport`, `checks`) and the
//! stored keys of design §4.3, §4.4 and §6.2, so the test does not lean on the worker's types.

use ecad_model::ComponentId;
use serde_json::{Value, json};

use crate::common::{H, id, pts};
use crate::plc_contracts::{checks_of, placed, prop, same_as_built};

struct F {
    h: H,
    dq: ComponentId,
    ao: ComponentId,
    lamps: Vec<ComponentId>,
}

/// The tags of `-A03`'s used channels 0–7 (all tagged, so only the planned rows differ).
const TAGS: [&str; 8] = ["Fan_On", "Lamp_1", "Valve_Inlet", "Lamp_3", "Lamp_4", "Lamp_5", "Lamp_6", "Dosing_On"];

fn fixture(seed: u64) -> F {
    let mut h = H::new(seed);
    placed(&mut h, "plccpu", 50.0, 400.0);
    placed(&mut h, "plcdi", 100.0, 400.0);
    let dq = placed(&mut h, "plcdq", 140.0, 400.0);
    let ao = placed(&mut h, "plcao", 180.0, 400.0);
    let mut lamps = Vec::new();
    for i in 0..8 {
        let x = 100.0 + 20.0 * f64::from(i);
        lamps.push(placed(&mut h, "lamp", x, 260.0));
        h.run("wire", json!({"points": pts(&[(x, 250.0), (x, 150.0)])})).unwrap();
    }
    h.run("plcchannels", json!({"card": dq.hex(), "from": [95.0, 200.0], "to": [255.0, 200.0]})).unwrap();
    // Channel 2's wire is x = 140: a terminal between the channel (y 200) and the lamp (y 250).
    let x = h.run("place", json!({"symbol": "terminal", "at": [140, 225], "tag": "-X10"})).unwrap();
    assert_eq!(x["joined"], true, "fixture: -X10 lands on channel 2's wire: {x}");
    F { h, dq, ao, lamps }
}

/// `fixture` with every used `-A03` channel tagged.
fn tagged(seed: u64) -> F {
    let mut f = fixture(seed);
    for (i, t) in TAGS.iter().enumerate() {
        f.h.run("plcchannel", json!({"card": "-A03", "channel": i, "tag": t})).unwrap();
    }
    f
}

fn rows(h: &mut H, card: &str) -> Vec<Value> {
    h.run("iolist", json!({"card": card, "limit": 2000})).unwrap()["rows"].as_array().cloned().unwrap_or_default()
}

fn row(h: &mut H, card: &str, ch: u64) -> Value {
    rows(h, card).into_iter().find(|r| r["channel"] == json!(ch)).unwrap_or_else(|| panic!("no row {card} {ch}"))
}

/// The live address of a channel as `iolist` gives it (the project's style).
fn addr(h: &mut H, card: &str, ch: u64) -> String {
    row(h, card, ch)["address"].as_str().unwrap_or_else(|| panic!("{card} {ch} has an address")).to_string()
}

/// `I0.2` / `%IX0.2` / `%I0.2` → `I0.2` (the test's own reading of design §6.3).
fn bare(a: &str) -> String {
    let a = a.trim().trim_start_matches('%').to_uppercase();
    let mut c = a.chars();
    match (c.next(), c.next()) {
        (Some(d @ ('I' | 'Q')), Some('X')) => format!("{d}{}", &a[2..]),
        _ => a,
    }
}

/// The German mnemonic of a bare address (`Q0.2` → `A0.2`, `I1.0` → `E1.0`).
fn german(a: &str) -> String {
    let b = bare(a);
    match b.chars().next() {
        Some('Q') => format!("A{}", &b[1..]),
        Some('I') => format!("E{}", &b[1..]),
        _ => b,
    }
}

fn csv(rows: &[(String, String, &str, &str)]) -> String {
    let mut s = String::from("name,address,type,comment\n");
    for (n, a, t, c) in rows {
        s.push_str(&format!("{n},{a},{t},{c}\n"));
    }
    s
}

/// The tester's P56-like table for `tagged`: ch 0 (German mnemonic, no `%`) and 1, 3–6 in sync,
/// ch 2 `Valve_In` (differs), ch 7 absent (not in PLC), ch 12 `Spare_Out4` (only in PLC), `%I8.0`
/// `Door_Sw` (no card), and 300 memory tags (skipped).
fn table(h: &mut H) -> String {
    let mut r: Vec<(String, String, &str, &str)> = Vec::new();
    r.push(("Fan_On".into(), german(&addr(h, "-A03", 0)), "Bool", "Fan contactor on"));
    for i in [1u64, 3, 4, 5, 6] {
        let i_us = usize::try_from(i).unwrap();
        r.push((TAGS[i_us].into(), format!("%{}", bare(&addr(h, "-A03", i))), "Bool", ""));
    }
    let a2 = bare(&addr(h, "-A03", 2));
    r.push(("Valve_In".into(), format!("%{}X{}", &a2[..1], &a2[1..]), "Bool", "Inlet valve"));
    r.push(("Spare_Out4".into(), format!("%{}", bare(&addr(h, "-A03", 12))), "Bool", "Spare out"));
    r.push(("Door_Sw".into(), "%I8.0".into(), "Bool", "Door switch"));
    for i in 0..300 {
        r.push((format!("Mem_{i}"), format!("%M{}.{}", i / 8, i % 8), "Bool", ""));
    }
    csv(&r)
}

/// `plcsync` rows of `card` (all states), by channel.
fn sync_rows(h: &mut H, card: &str) -> Vec<Value> {
    let v = h.run("plcsync", json!({"card": card})).unwrap();
    v["rows"].as_array().cloned().unwrap_or_default()
}

fn state(h: &mut H, card: &str, ch: u64) -> String {
    sync_rows(h, card)
        .into_iter()
        .find(|r| r["channel"] == json!(ch))
        .map(|r| r["state"].as_str().unwrap_or("?").to_string())
        .unwrap_or_else(|| "none".into())
}

const COUNTED: [&str; 4] = ["differs", "moved", "not_in_plc", "only_in_plc"];

/// The chip's count for a card: counted states (design §6.3, §6.4).
fn differences(h: &mut H, card: &str) -> usize {
    sync_rows(h, card).iter().filter(|r| r["state"].as_str().is_some_and(|s| COUNTED.contains(&s))).count()
}

fn ptag(h: &mut H, c: ComponentId, ch: u64) -> Option<String> {
    prop(h, c, &format!("plc.ch.{ch}.tag"))
}

fn last_label(h: &mut H) -> String {
    h.p(|p| p.ledger.log().last().map(|t| t.label.clone()).unwrap_or_default())
}

fn project_prop(h: &mut H, k: &str) -> Option<String> {
    h.doc().properties.get(k).cloned()
}

fn import(h: &mut H, text: &str, extra: Value) -> Result<Value, String> {
    let mut p = json!({"text": text});
    if let (Some(o), Some(e)) = (p.as_object_mut(), extra.as_object()) {
        o.extend(e.clone());
    }
    h.run("plcimport", p)
}

// ---------------------------------------------------------------- C: Tag (§4.3)

#[test]
#[ignore = "tester 056: awaiting build"]
fn c_a_tag_is_one_transaction_and_a_duplicate_is_refused_case_insensitive() {
    let F { mut h, dq, .. } = fixture(5601);
    let head = h.head();
    h.run("plcchannel", json!({"card": "-A03", "channel": 2, "tag": "Valve_Inlet"})).unwrap();
    assert_eq!(h.head(), head + 1, "one transaction");
    assert_eq!(ptag(&mut h, dq, 2).as_deref(), Some("Valve_Inlet"), "stored as plc.ch.2.tag (FP1)");
    let e = h.run("plcchannel", json!({"card": "-A03", "channel": 3, "tag": "valve_INLET"})).unwrap_err();
    assert!(
        e.starts_with("Tag \"valve_INLET\" is already -A03 channel 2 (") || e.starts_with("Tag \"Valve_Inlet\" is already -A03 channel 2 ("),
        "{e}"
    );
    assert_eq!(h.head(), head + 1, "a refusal records nothing");
    // The DI card is the same PLC: refused there too.
    assert!(h.run("plcchannel", json!({"card": "-A02", "channel": 0, "tag": "VALVE_inlet"})).is_err());
    h.cmdline("u");
    assert_eq!(ptag(&mut h, dq, 2), None, "u undoes the tag");
    // Trimmed; null clears; 128 characters fit, 129 do not.
    h.run("plcchannel", json!({"card": "-A03", "channel": 2, "tag": "  Valve_Inlet  "})).unwrap();
    assert_eq!(ptag(&mut h, dq, 2).as_deref(), Some("Valve_Inlet"));
    h.run("plcchannel", json!({"card": "-A03", "channel": 2, "tag": null})).unwrap();
    assert_eq!(ptag(&mut h, dq, 2), None);
    h.run("plcchannel", json!({"card": "-A03", "channel": 4, "tag": "T".repeat(128)})).unwrap();
    let n = h.head();
    assert!(h.run("plcchannel", json!({"card": "-A03", "channel": 5, "tag": "T".repeat(129)})).is_err());
    assert_eq!(h.head(), n);
    same_as_built(&mut h);
}

#[test]
#[ignore = "tester 056: awaiting build"]
fn c_another_plc_may_reuse_a_tag() {
    let F { mut h, .. } = fixture(5602);
    h.run("plcchannel", json!({"card": "-A03", "channel": 0, "tag": "Pump_On"})).unwrap();
    let cpu2: ComponentId = id(&h.run("place", json!({"symbol": "plccpu", "at": [400, 400], "tag": "-A20"})).unwrap(), "component");
    let v = h.run("place", json!({"symbol": "plcdq", "at": [440, 400], "rack": cpu2.hex(), "slot": 1})).unwrap();
    let dq2: ComponentId = id(&v, "component");
    h.run("plcchannel", json!({"card": dq2.hex(), "channel": 0, "tag": "Pump_On"})).expect("a tag is unique per PLC, not per project");
    same_as_built(&mut h);
}

/// A tag a format would leave out is a Check, not a refusal (§4.3, §7.4, §9 `plc-tag-chars`).
#[test]
#[ignore = "tester 056: awaiting build"]
fn c_a_tag_with_a_control_character_is_kept_and_checked() {
    let F { mut h, .. } = fixture(5603);
    let r = h.run("plcchannel", json!({"card": "-A03", "channel": 1, "tag": "Bad\u{7}Tag"}));
    if r.is_ok() {
        let rows = checks_of(&mut h, "plc-tag-chars");
        assert_eq!(rows.len(), 1, "{rows:?}");
        let v = h.run("plcexport", json!({"rack": "-A01"})).unwrap();
        assert!(!v["text"].as_str().unwrap_or_default().contains("Bad"), "left out of the export: {v}");
    } else {
        // Refusing a control character in words is acceptable too; never a panic.
        assert!(!r.unwrap_err().is_empty());
    }
}

// ---------------------------------------------------------------- D: Safe (§4.4, §11)

#[test]
#[ignore = "tester 056: awaiting build"]
fn d_safe_state_set_here_default_and_setdefault() {
    let F { mut h, dq, ao, .. } = fixture(5604);
    let head = h.head();
    h.run("plcchannel", json!({"card": "-A03", "channel": 4, "safe": "on"})).unwrap();
    assert_eq!(h.head(), head + 1);
    assert_eq!(prop(&mut h, dq, "plc.ch.4.safe").as_deref(), Some("on"));
    let r1 = row(&mut h, "-A03", 1);
    assert_eq!(r1["safe"], "off", "unset shows plc.safe.dq: {r1}");
    assert_eq!(r1["safe_default"], true, "{r1}");
    assert_eq!(row(&mut h, "-A03", 4)["safe_default"], false);
    h.run("setdefault", json!({"key": "plc.safe.dq", "level": "project", "value": "hold"})).unwrap();
    assert_eq!(row(&mut h, "-A03", 1)["safe"], "hold", "follows setdefault");
    assert_eq!(row(&mut h, "-A03", 4)["safe"], "on", "set here keeps its value");
    // Setting the default's own value writes it: the badge goes.
    h.run("plcchannel", json!({"card": "-A03", "channel": 5, "safe": "hold"})).unwrap();
    assert_eq!(row(&mut h, "-A03", 5)["safe_default"], false);
    // null clears back to the default.
    h.run("plcchannel", json!({"card": "-A03", "channel": 4, "safe": null})).unwrap();
    assert_eq!(prop(&mut h, dq, "plc.ch.4.safe"), None);
    // Inputs have no safe state; a DQ takes only off/on/hold.
    let n = h.head();
    assert!(h.run("plcchannel", json!({"card": "-A02", "channel": 0, "safe": "off"})).is_err(), "an input has no safe state");
    assert!(h.run("plcchannel", json!({"card": "-A03", "channel": 0, "safe": "4 mA"})).is_err());
    assert!(h.run("plcchannel", json!({"card": "-A03", "channel": 0, "safe": "open"})).is_err());
    assert_eq!(h.head(), n, "refusals record nothing");
    // AO: hold by default; min, max or a value of at most 16 characters, trimmed.
    assert_eq!(row(&mut h, "-A04", 0)["safe"], "hold");
    h.run("plcchannel", json!({"card": "-A04", "channel": 0, "safe": "min"})).unwrap();
    h.run("plcchannel", json!({"card": "-A04", "channel": 1, "safe": "9999 mA "})).unwrap();
    assert_eq!(prop(&mut h, ao, "plc.ch.1.safe").as_deref(), Some("9999 mA"), "trimmed");
    let n = h.head();
    assert!(h.run("plcchannel", json!({"card": "-A04", "channel": 2, "safe": "x".repeat(17)})).is_err());
    assert!(h.run("plcchannel", json!({"card": "-A04", "channel": 2, "safe": "on"})).is_err(), "on is a DQ word");
    assert_eq!(h.head(), n);
    same_as_built(&mut h);
}

// ---------------------------------------------------------------- E: Drives (§4.5)

#[test]
#[ignore = "tester 056: awaiting build"]
fn e_drives_reads_through_a_terminal_to_the_lamp() {
    let F { mut h, lamps, .. } = fixture(5605);
    let tags: Vec<String> = lamps.iter().map(|l| crate::plc_contracts::tag(&mut h, *l)).collect();
    let r2 = row(&mut h, "-A03", 2);
    let d = r2["drives"].as_array().cloned().unwrap_or_default();
    assert!(!d.is_empty(), "{r2}");
    assert_eq!(d[0]["device"], json!(tags[2]), "the end device, not -X10: {r2}");
    assert!(d.iter().all(|x| x["device"] != "-X10"), "a pass-through is dropped when an end device exists: {r2}");
    assert!(d[0]["point"].is_string() && d[0]["place"].is_string(), "{r2}");
    // A direct wire: the lamp itself.
    let r0 = row(&mut h, "-A03", 0);
    assert_eq!(r0["drives"][0]["device"], json!(tags[0]), "{r0}");
    // A free channel drives nothing.
    let r9 = row(&mut h, "-A03", 9);
    assert!(r9["drives"].as_array().is_none_or(Vec::is_empty), "{r9}");
    // Test column designed, empty in 056 (§4.6).
    assert_eq!(r0["test"], Value::Null);
}

/// A pass-through loop (two terminals on one net, bridged) is cycle-safe and bounded (§4.5 step 3).
#[test]
#[ignore = "tester 056: awaiting build"]
fn e_drives_is_bounded_on_a_chain_of_terminals() {
    let mut h = H::new(5606);
    placed(&mut h, "plccpu", 50.0, 400.0);
    let dq = placed(&mut h, "plcdq", 100.0, 400.0);
    placed(&mut h, "lamp", 100.0, 300.0);
    h.run("wire", json!({"points": pts(&[(100.0, 290.0), (100.0, 0.0)])})).unwrap();
    h.run("plcchannels", json!({"card": dq.hex(), "from": [95.0, 20.0], "to": [105.0, 20.0]})).unwrap();
    // Twelve terminals in series: more than the 8 hops; Drives ends, never hangs or panics.
    for i in 0..12 {
        let y = 40.0 + 20.0 * f64::from(i);
        let _ = h.run("place", json!({"symbol": "terminal", "at": [100, y], "tag": "-X20"}));
    }
    let r = row(&mut h, "-A02", 0);
    let shown = r["drives"].to_string();
    assert!(shown.contains("-X20") || shown.contains("-H"), "the last pass-through or the lamp: {r}");
}

// ---------------------------------------------------------------- Make tags (§4.3)

#[test]
#[ignore = "tester 056: awaiting build"]
fn make_tags_fills_empty_tags_from_descriptions_in_one_transaction() {
    let F { mut h, dq, .. } = fixture(5607);
    for (i, t) in [(0, "Inlet valve open"), (1, "2nd pump - run"), (2, "Inlet valve open"), (3, "Größe #1"), (4, "Inlet valve open")] {
        h.run("plcchannel", json!({"card": "-A03", "channel": i, "text": t})).unwrap();
    }
    h.run("plcchannel", json!({"card": "-A03", "channel": 5, "tag": "Kept"})).unwrap();
    h.run("plcchannel", json!({"card": "-A03", "channel": 5, "text": "Something else"})).unwrap();
    // A reserved (free, described) channel is filled too; a free channel with no text is not.
    h.run("plcchannel", json!({"card": "-A03", "channel": 10, "text": "Reserve out"})).unwrap();
    let head = h.head();
    let v = h.run("plcmaketags", json!({"card": "-A03"})).unwrap();
    assert_eq!(h.head(), head + 1, "one transaction");
    assert_eq!(ptag(&mut h, dq, 0).as_deref(), Some("Inlet_valve_open"));
    let t1 = ptag(&mut h, dq, 1).unwrap_or_default();
    assert!(t1.starts_with("_2nd_pump"), "a digit first gets _ before it; spaces and dashes become _: {t1}");
    assert!(t1.chars().all(|c| c.is_alphanumeric() || c == '_'), "{t1}");
    assert_eq!(ptag(&mut h, dq, 2).as_deref(), Some("Inlet_valve_open_2"), "duplicates get _2");
    assert_eq!(ptag(&mut h, dq, 4).as_deref(), Some("Inlet_valve_open_3"));
    let t3 = ptag(&mut h, dq, 3).unwrap_or_default();
    assert!(!t3.contains('#') && !t3.contains(' '), "other characters dropped: {t3}");
    assert_eq!(ptag(&mut h, dq, 5).as_deref(), Some("Kept"), "a set tag is not replaced");
    assert_eq!(ptag(&mut h, dq, 10).as_deref(), Some("Reserve_out"), "reserved channels too");
    assert_eq!(ptag(&mut h, dq, 11), None, "free without text: left alone");
    let n = (0..16u64).filter(|i| *i != 5).filter(|i| ptag(&mut h, dq, *i).is_some()).count();
    assert!(v["message"].as_str().unwrap_or_default().starts_with(&format!("Made {n} tags on -A03.")), "{v}");
    h.cmdline("u");
    assert_eq!(ptag(&mut h, dq, 0), None, "one u undoes all of them");
    assert_eq!(ptag(&mut h, dq, 5).as_deref(), Some("Kept"));
    // By rack.
    let head = h.head();
    h.run("plcmaketags", json!({"rack": "-A01"})).unwrap();
    assert_eq!(h.head(), head + 1);
    same_as_built(&mut h);
}

// ---------------------------------------------------------------- percent (§11, Q5)

#[test]
#[ignore = "tester 056: awaiting build"]
fn percent_style_and_matching_across_styles() {
    let F { mut h, .. } = tagged(5608);
    let t = table(&mut h);
    import(&mut h, &t, json!({})).unwrap();
    let before: Vec<String> = (0..16).map(|i| state(&mut h, "-A03", i)).collect();
    h.run("plcaddress", json!({"rack": "-A01", "style": "percent"})).unwrap();
    assert_eq!(addr(&mut h, "-A02", 0), "%I0.0");
    assert!(addr(&mut h, "-A03", 0).starts_with("%Q"), "{}", addr(&mut h, "-A03", 0));
    assert!(addr(&mut h, "-A04", 0).starts_with("%QW"), "{}", addr(&mut h, "-A04", 0));
    let percent: Vec<String> = (0..16).map(|i| state(&mut h, "-A03", i)).collect();
    assert_eq!(percent, before, "states do not depend on the style");
    h.run("plcaddress", json!({"rack": "-A01", "style": "iec"})).unwrap();
    let iec: Vec<String> = (0..16).map(|i| state(&mut h, "-A03", i)).collect();
    assert_eq!(iec, before);
    // The built-in default stays byte.bit.
    let v = h.run("defaults", json!({"group": "PLC"})).unwrap().to_string();
    assert!(v.contains("percent"), "percent is a value of plc.address.style");
    h.run("plcaddress", json!({"rack": "-A01", "style": "slot"})).unwrap();
    let s = state(&mut h, "-A03", 0);
    assert!(!COUNTED.contains(&s.as_str()), "slot style: no address, not counted: {s}");
}

// ---------------------------------------------------------------- F, G: import (§6.2, §6.7)

#[test]
#[ignore = "tester 056: awaiting build"]
fn f_the_dry_run_counts_and_writes_nothing() {
    let F { mut h, .. } = tagged(5609);
    let t = table(&mut h);
    let head = h.head();
    let v = import(&mut h, &t, json!({"dry_run": true})).unwrap();
    assert_eq!(h.head(), head, "the preview writes nothing");
    assert_eq!(v["read"], 310, "{v}");
    assert_eq!(v["io"], 10, "{v}");
    assert_eq!(project_prop(&mut h, "plc.sync.table"), None);
    let s0 = state(&mut h, "-A03", 2);
    assert!(!COUNTED.contains(&s0.as_str()), "no snapshot: nothing to compare: {s0}");
    let rows = v["rows"].as_array().cloned().unwrap_or_default();
    assert!(rows.iter().any(|r| r.to_string().contains("Valve_In")), "the preview lists the differences: {v}");
}

#[test]
#[ignore = "tester 056: awaiting build"]
fn g_import_is_one_transaction_with_the_states_and_u_undoes_it_whole() {
    let F { mut h, .. } = tagged(5610);
    assert_eq!(differences(&mut h, "-A03"), 0, "not imported: nothing counted");
    let t = table(&mut h);
    let head = h.head();
    let v = import(&mut h, &t, json!({})).unwrap();
    assert_eq!(h.head(), head + 1, "one transaction");
    assert_eq!(last_label(&mut h), "Import PLC tags");
    let snap = project_prop(&mut h, "plc.sync.table").expect("the snapshot (FP2)");
    assert!(snap.starts_with("name,address,type,comment"), "canonical CSV: {snap}");
    assert_eq!(snap.lines().count(), 1 + 10, "I/O tags only; memory tags not kept");
    assert!(!snap.contains("Mem_"));
    let src = project_prop(&mut h, "plc.sync.source").unwrap_or_default();
    assert!(!src.contains('/') && !src.contains('\\'), "no directory: {src}");
    let at = project_prop(&mut h, "plc.sync.at").unwrap_or_default();
    assert!(chrono::DateTime::parse_from_rfc3339(&at).is_ok(), "RFC 3339: {at}");
    assert_eq!(v["differences"].as_u64().map(|_| ()), Some(()), "{v}");
    for (ch, want) in [(0, "in_sync"), (1, "in_sync"), (2, "differs"), (7, "not_in_plc"), (12, "only_in_plc"), (13, "free")] {
        let s = state(&mut h, "-A03", ch);
        if want == "free" {
            assert!(!COUNTED.contains(&s.as_str()) && s != "in_sync", "ch {ch}: {s}");
        } else {
            assert_eq!(s, want, "ch {ch}");
        }
    }
    assert_eq!(differences(&mut h, "-A03"), 3, "the chip: differs, not in PLC, only in PLC");
    let all = h.run("plcsync", json!({})).unwrap();
    assert!(all.to_string().contains("Door_Sw"), "a tag on no card is listed: {all}");
    let nc = h.run("plcsync", json!({"state": "no_card"})).unwrap();
    assert!(nc.to_string().contains("I8.0"), "{nc}");
    // Checks (§9).
    assert_eq!(checks_of(&mut h, "plc-differs").len(), 2, "differs + only in PLC");
    assert_eq!(checks_of(&mut h, "plc-not-in-plc").len(), 1);
    assert_eq!(checks_of(&mut h, "plc-no-card").len(), 1);
    // u undoes the whole import.
    h.cmdline("u");
    assert_eq!(project_prop(&mut h, "plc.sync.table"), None);
    assert_eq!(project_prop(&mut h, "plc.sync.source"), None);
    assert_eq!(differences(&mut h, "-A03"), 0);
    assert!(checks_of(&mut h, "plc-differs").is_empty(), "sync checks exist only with a snapshot");
    // A second import replaces the first (one snapshot per project), and u brings back the first.
    import(&mut h, &t, json!({})).unwrap();
    let first = project_prop(&mut h, "plc.sync.table");
    let t2 = csv(&[("Fan_On".into(), format!("%{}", bare(&addr(&mut h, "-A03", 0))), "Bool", "")]);
    import(&mut h, &t2, json!({})).unwrap();
    assert_eq!(project_prop(&mut h, "plc.sync.table").map(|s| s.lines().count()), Some(2));
    h.cmdline("u");
    assert_eq!(project_prop(&mut h, "plc.sync.table"), first, "undo restores the previous snapshot");
    same_as_built(&mut h);
}

#[test]
#[ignore = "tester 056: awaiting build"]
fn g_import_taking_names_is_still_one_transaction() {
    let F { mut h, dq, .. } = tagged(5611);
    let t = table(&mut h);
    let head = h.head();
    import(&mut h, &t, json!({"take": "names"})).unwrap();
    assert_eq!(h.head(), head + 1);
    assert_eq!(ptag(&mut h, dq, 2).as_deref(), Some("Valve_In"), "differs: name taken");
    assert_eq!(ptag(&mut h, dq, 12).as_deref(), Some("Spare_Out4"), "only in PLC: name taken");
    assert_eq!(prop(&mut h, dq, "plc.ch.12.text").as_deref(), Some("Spare out"), "and the comment as description");
    assert_eq!(ptag(&mut h, dq, 7).as_deref(), Some("Dosing_On"), "not in PLC: ours stays");
    assert_eq!(differences(&mut h, "-A03"), 1, "only not in PLC is left");
    h.cmdline("u");
    assert_eq!(ptag(&mut h, dq, 2).as_deref(), Some("Valve_Inlet"), "u restores names and snapshot together");
    assert_eq!(ptag(&mut h, dq, 12), None);
    assert_eq!(project_prop(&mut h, "plc.sync.table"), None);
}

/// A taken name another channel already has: the import does not fail; `plc-tag-twice` shows it.
#[test]
#[ignore = "tester 056: awaiting build"]
fn g_taking_a_name_that_is_already_used_gives_tag_twice_not_a_panic() {
    let F { mut h, .. } = tagged(5612);
    let a2 = bare(&addr(&mut h, "-A03", 2));
    let t = csv(&[("lamp_1".into(), format!("%{a2}"), "Bool", "")]);
    let r = import(&mut h, &t, json!({"take": "names"}));
    match r {
        Ok(_) => {
            let rows = checks_of(&mut h, "plc-tag-twice");
            assert!(rows.len() == 1 || state(&mut h, "-A03", 2) == "differs", "a duplicate is a Checks row or the name is not taken: {rows:?}");
        }
        Err(e) => assert!(!e.is_empty()),
    }
}

// ---------------------------------------------------------------- H: Use PLC's (§6.5)

#[test]
#[ignore = "tester 056: awaiting build"]
fn h_use_plcs_takes_one_name_then_all() {
    let F { mut h, dq, .. } = tagged(5613);
    let t = table(&mut h);
    import(&mut h, &t, json!({})).unwrap();
    let head = h.head();
    h.run("plctake", json!({"items": [{"card": "-A03", "channel": 2}]})).unwrap();
    assert_eq!(h.head(), head + 1);
    assert_eq!(ptag(&mut h, dq, 2).as_deref(), Some("Valve_In"));
    assert_eq!(state(&mut h, "-A03", 2), "in_sync");
    assert_eq!(differences(&mut h, "-A03"), 2);
    h.run("plctake", json!({"card": "-A03"})).unwrap();
    assert_eq!(h.head(), head + 2);
    assert_eq!(ptag(&mut h, dq, 12).as_deref(), Some("Spare_Out4"));
    assert_eq!(differences(&mut h, "-A03"), 1, "not in PLC has nothing to take");
    h.cmdline("u");
    h.cmdline("u");
    assert_eq!(ptag(&mut h, dq, 2).as_deref(), Some("Valve_Inlet"));
    let head = h.head();
    h.run("plctake", json!({"all": true})).unwrap();
    assert_eq!(h.head(), head + 1, "for all: one transaction");
    // Nothing to take: refused or a no-op, never an empty transaction.
    let head = h.head();
    let _ = h.run("plctake", json!({"items": [{"card": "-A03", "channel": 0}]}));
    assert_eq!(h.head(), head, "in sync: nothing to take");
    same_as_built(&mut h);
}

// ---------------------------------------------------------------- I: round trip (§6.6, §7.3)

#[test]
#[ignore = "tester 056: awaiting build"]
fn i_export_then_import_round_trips_and_leaves_out_untagged() {
    let F { mut h, .. } = fixture(5614);
    for (i, t) in [(0, "Fan_On"), (2, "Valve, \"Inlet\""), (3, "Dosing;On")] {
        h.run("plcchannel", json!({"card": "-A03", "channel": i, "tag": t})).unwrap();
    }
    h.run("plcchannel", json!({"card": "-A03", "channel": 0, "text": "Fan, contactor \"K3\""})).unwrap();
    h.run("plcchannel", json!({"card": "-A04", "channel": 0, "tag": "Speed_Ref"})).unwrap();
    let head = h.head();
    let v = h.run("plcexport", json!({"rack": "-A01"})).unwrap();
    assert_eq!(h.head(), head, "export writes nothing");
    let text = v["text"].as_str().unwrap().to_string();
    let mut lines = text.lines();
    assert_eq!(lines.next().map(|l| l.trim_start_matches('\u{feff}')), Some("name,address,type,comment,card,channel,safe,drives,page"));
    assert_eq!(v["rows"], 4, "{v}");
    assert!(v["left_out"].to_string().contains('5'), "5 used -A03 channels without a tag are counted: {v}");
    assert!(text.contains("\"Valve, \"\"Inlet\"\"\""), "RFC 4180 quoting: {text}");
    assert!(text.contains("\"Dosing;On\""), "a ; is quoted: {text}");
    assert!(text.contains("\"Fan, contactor \"\"K3\"\"\""), "{text}");
    let speed = text.lines().find(|l| l.starts_with("Speed_Ref")).unwrap_or_default().to_string();
    assert!(speed.contains(",Int,"), "AO → Int: {speed}");
    let fan = text.lines().find(|l| l.starts_with("Fan_On")).unwrap_or_default().to_string();
    assert!(fan.contains(",Bool,") && fan.contains(",%Q"), "DQ → Bool, % form: {fan}");
    // Reimport: every exported row is in sync, and exporting again gives the same file.
    import(&mut h, &text, json!({})).unwrap();
    let snap1 = project_prop(&mut h, "plc.sync.table");
    for ch in [0, 2, 3] {
        assert_eq!(state(&mut h, "-A03", ch), "in_sync", "ch {ch}");
    }
    let again = h.run("plcexport", json!({"rack": "-A01"})).unwrap();
    assert_eq!(again["text"], v["text"], "export does not change with the sync state");
    import(&mut h, again["text"].as_str().unwrap(), json!({})).unwrap();
    assert_eq!(project_prop(&mut h, "plc.sync.table"), snap1, "same snapshot");
}

/// Export to a path with spaces and non-ASCII letters; import from it (Windows paths).
#[test]
#[ignore = "tester 056: awaiting build"]
fn i_export_and_import_by_path() {
    let F { mut h, .. } = tagged(5615);
    let dir = std::env::temp_dir().join(format!("t056 Schaltpläne {}", ecad_store::disk::unique()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("Pump station PLC tags.csv");
    let head = h.head();
    let v = h.run("plcexport", json!({"path": path.display().to_string()})).unwrap();
    assert_eq!(h.head(), head);
    let m = v["message"].as_str().unwrap_or_default();
    assert!(m.starts_with("Wrote 8 tags") && m.contains("Pump station PLC tags.csv"), "{v}");
    let bytes = std::fs::read(&path).unwrap();
    assert!(!bytes.is_empty());
    h.run("plcimport", json!({"path": path.display().to_string()})).unwrap();
    assert!(project_prop(&mut h, "plc.sync.source").unwrap_or_default().contains("Pump station PLC tags.csv"));
    assert_eq!(differences(&mut h, "-A03"), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------- J: `;`, BOM, CRLF, refusals (§7.1, §7.3)

#[test]
#[ignore = "tester 056: awaiting build"]
fn j_semicolons_with_a_bom_and_crlf_give_the_same_states() {
    let F { mut h, .. } = tagged(5616);
    let t = table(&mut h);
    import(&mut h, &t, json!({})).unwrap();
    let comma: Vec<String> = (0..16).map(|i| state(&mut h, "-A03", i)).collect();
    h.cmdline("u");
    let semi = format!("\u{feff}{}", t.replace(',', ";").replace('\n', "\r\n"));
    import(&mut h, &semi, json!({})).unwrap();
    let s: Vec<String> = (0..16).map(|i| state(&mut h, "-A03", i)).collect();
    assert_eq!(s, comma);
}

#[test]
#[ignore = "tester 056: awaiting build"]
fn j_other_files_are_refused_with_words_and_write_nothing() {
    let F { mut h, .. } = tagged(5617);
    let head = h.head();
    let e = import(&mut h, "Name;Path;Data Type;Logical Address\nA;B;Bool;%Q0.0\n", json!({})).unwrap_err();
    assert!(e.contains("\"address\""), "a missing column is named: {e}");
    let e = import(&mut h, "foo,bar\n1,2\n", json!({})).unwrap_err();
    assert!(e.contains("address") || e.contains("Only the generic CSV tag table can be read yet"), "{e}");
    let e = import(&mut h, "PK\u{3}\u{4}\u{14}\u{0}\u{6}\u{0}[Content_Types].xml", json!({})).unwrap_err();
    assert_eq!(e, "Only the generic CSV tag table can be read yet (name, address, type, comment).");
    let e = h.run("plcimport", json!({"text": "name,address\nx,%Q0.0\n", "format": "tia"})).unwrap_err();
    assert!(!e.is_empty());
    assert_eq!(h.head(), head, "refusals write nothing");
}

// ---------------------------------------------------------------- K: card move (§6.3, §12.2)

#[test]
#[ignore = "tester 056: awaiting build"]
fn k_a_card_move_moves_addresses_and_states_and_nothing_else() {
    let F { mut h, dq, .. } = tagged(5618);
    let t = table(&mut h);
    import(&mut h, &t, json!({})).unwrap();
    let old = addr(&mut h, "-A03", 0);
    let snap = project_prop(&mut h, "plc.sync.table");
    let before = h.doc();
    let head = h.head();
    h.run("plcslot", json!({"card": dq.hex(), "slot": 5})).unwrap();
    assert_eq!(h.head(), head + 1);
    let new = addr(&mut h, "-A03", 0);
    assert_ne!(bare(&old), bare(&new), "fixture: the AO's words come first now");
    assert_eq!(project_prop(&mut h, "plc.sync.table"), snap, "the snapshot is not touched");
    for i in 0..8u64 {
        assert_eq!(ptag(&mut h, dq, i), before.components.get(&dq).and_then(|c| c.properties.get(&format!("plc.ch.{i}.tag")).cloned()));
    }
    // Ch 0's tag is at its old address in the PLC: moved.
    assert_eq!(state(&mut h, "-A03", 0), "moved");
    let r = sync_rows(&mut h, "-A03").into_iter().find(|r| r["channel"] == 0).unwrap();
    assert!(r.to_string().contains(&bare(&old)), "the PLC address is shown: {r}");
    // Use PLC's address types it (and so the state is in sync again).
    h.run("plctake", json!({"items": [{"card": "-A03", "channel": 0}], "what": "address"})).unwrap();
    assert_eq!(state(&mut h, "-A03", 0), "in_sync");
    assert!(prop(&mut h, dq, "plc.ch.0.address").is_some(), "a typed address");
    same_as_built(&mut h);
}

// ---------------------------------------------------------------- M: the book (§8)

fn plcrack_pages(h: &mut H) -> usize {
    let d = h.doc();
    d.page_order
        .iter()
        .filter(|p| d.pages.get(p).and_then(|x| x.properties.get(ecad_cadcraft::generated::KEY)).is_some_and(|k| k == "plcrack"))
        .count()
}

#[test]
#[ignore = "tester 056: awaiting build"]
fn m_the_rack_overview_is_in_the_book_with_a_rack_and_goes_with_the_last() {
    let mut h = H::new(5619);
    assert_eq!(plcrack_pages(&mut h), 0);
    let cpu = placed(&mut h, "plccpu", 50.0, 400.0);
    placed(&mut h, "plcdq", 100.0, 400.0);
    assert_eq!(plcrack_pages(&mut h), 1, "always in the book with a rack");
    let cpu2 = placed(&mut h, "plccpu", 300.0, 400.0);
    assert_eq!(plcrack_pages(&mut h), 1, "one overview, not one per rack");
    for c in [cpu2, cpu] {
        let page = h.first_page();
        let hs = h.rep_handles(page, c);
        h.run("erase", json!({"handles": hs})).unwrap();
    }
    assert_eq!(plcrack_pages(&mut h), 0, "goes with the last rack");
    h.cmdline("u");
    assert_eq!(plcrack_pages(&mut h), 1);
}

// ---------------------------------------------------------------- update maps (§12)

#[test]
#[ignore = "tester 056: awaiting build"]
fn update_maps_tag_edit_one_card_unrelated_commit_nothing() {
    let F { mut h, lamps, .. } = tagged(5620);
    let t = table(&mut h);
    import(&mut h, &t, json!({})).unwrap();
    let rec = |h: &mut H| h.cfg(|p| p.outputs().plc.recomputed);
    let n = rec(&mut h);
    h.run("plcchannel", json!({"card": "-A03", "channel": 2, "tag": "Valve_In"})).unwrap();
    assert!(rec(&mut h) - n <= 1, "a tag edit recomputes at most its card");
    assert_eq!(differences(&mut h, "-A03"), 2);
    let n = rec(&mut h);
    h.run("setprop", json!({"component": lamps[0].hex(), "key": "description", "value": "Lamp"})).unwrap();
    assert_eq!(rec(&mut h), n, "a commit with no PLC data recomputes nothing");
    same_as_built(&mut h);
}

// ---------------------------------------------------------------- Checks (§9)

#[test]
#[ignore = "tester 056: awaiting build"]
fn checks_tag_missing_only_with_a_snapshot_and_make_tags_fixes_it() {
    let F { mut h, .. } = fixture(5621);
    assert!(checks_of(&mut h, "plc-tag-missing").is_empty(), "no snapshot, never exported");
    import(&mut h, "name,address,type,comment\nX,%I8.0,Bool,\n", json!({})).unwrap();
    assert_eq!(checks_of(&mut h, "plc-tag-missing").len(), 8, "-A03's eight used channels");
    h.run("plcchannel", json!({"card": "-A03", "channel": 0, "text": "Fan"})).unwrap();
    h.run("plcmaketags", json!({"card": "-A03"})).unwrap();
    assert_eq!(checks_of(&mut h, "plc-tag-missing").len(), 7, "Make tags fills from descriptions only");
}

// ---------------------------------------------------------------- O: hostile (§13.1 O)

#[test]
#[ignore = "tester 056: awaiting build"]
fn o_hostile_tables_are_refused_skipped_or_checked_never_a_panic() {
    let F { mut h, .. } = tagged(5622);
    let q = bare(&addr(&mut h, "-A03", 0));
    let cases: Vec<String> = vec![
        String::new(),
        "\u{0}\u{1}\u{2}\u{ff}garbage\u{fffd}".into(),
        "name,address,type,comment".into(),
        "name,address,type,comment\n\"unbalanced,%Q0.0,Bool,\n".into(),
        format!("name,address,type,comment\n{},%{q},Bool,\n", "N".repeat(300)),
        "name,address,type,comment\nA,%Q99999.9,Bool,\nB,%Q2.9,Bool,\nC,%Q-1.0,Bool,\nD,%M0.0,Bool,\nE,,Bool,\nF,%QW99999999999999999999,Int,\n"
            .into(),
        format!("name,address,type,comment\nDup,%{q},Bool,\nDup,%I8.1,Bool,\ndup,%I8.2,Bool,\n"),
        format!("name,address,type,comment\nCtl\u{7}Tag,%{q},Bool,\n"),
        format!("name,address,type,comment\nX,%{q},Bool,{}\n", "c".repeat(10_000)),
        format!("name,address,type,comment,{}\n", "x,".repeat(5000)),
        "name,address,type,comment\n\n\n\n,,,\n;;;\n".into(),
        "\u{feff}\u{feff}name,address,type,comment\nA,%Q0.0,Bool,\n".into(),
    ];
    for (i, c) in cases.iter().enumerate() {
        let head = h.head();
        let r = import(&mut h, c, json!({}));
        match r {
            Ok(v) => assert!(h.head() <= head + 1, "case {i}: {v}"),
            Err(e) => {
                assert!(!e.is_empty(), "case {i}: words");
                assert_eq!(h.head(), head, "case {i}: a refusal writes nothing");
            }
        }
        let _ = h.run("plcsync", json!({}));
        let _ = h.run("iolist", json!({"limit": 50}));
        let _ = h.run("checks", json!({}));
        let _ = h.run("plcexport", json!({}));
    }
    // The bad addresses are skipped and counted.
    let v = import(&mut h, &cases[5], json!({"dry_run": true})).unwrap();
    assert!(v["skipped"].to_string().chars().any(|c| c.is_ascii_digit() && c != '0'), "skipped and counted: {v}");
    same_as_built(&mut h);
}

#[test]
#[ignore = "tester 056: awaiting build"]
fn o_more_than_20000_io_tags_are_refused_with_the_count() {
    let F { mut h, .. } = fixture(5623);
    let mut s = String::from("name,address,type,comment\n");
    for i in 0..30_000u32 {
        s.push_str(&format!("T{i},%I{}.{},Bool,\n", i / 8, i % 8));
    }
    let head = h.head();
    let e = import(&mut h, &s, json!({})).unwrap_err();
    assert!(e.contains("30 000") || e.contains("30000"), "{e}");
    assert!(e.contains("20 000") || e.contains("20000"), "{e}");
    assert_eq!(h.head(), head);
    // Exactly 20 000 is read (and fits a property value: FP2's check).
    let mut s = String::from("name,address,type,comment\n");
    for i in 0..20_000u32 {
        s.push_str(&format!("T{i},%I{}.{},Bool,\n", i / 8, i % 8));
    }
    import(&mut h, &s, json!({})).expect("20 000 I/O tags fit");
    assert_eq!(project_prop(&mut h, "plc.sync.table").map(|t| t.lines().count()), Some(20_001));
}

#[test]
#[ignore = "tester 056: awaiting build"]
fn o_a_16_mb_single_line_is_refused_not_read_whole() {
    let F { mut h, .. } = fixture(5624);
    let big = format!("name,address,type,comment{}", "x".repeat(16 * 1024 * 1024 + 1));
    let head = h.head();
    assert!(import(&mut h, &big, json!({})).is_err());
    assert_eq!(h.head(), head);
}

/// A saved project with a hand-edited hostile snapshot (FP2) opens; sync reads it bounded.
#[test]
#[ignore = "tester 056: awaiting build"]
fn o_a_hostile_stored_snapshot_never_panics() {
    let F { mut h, .. } = tagged(5625);
    for v in ["", "garbage", "name,address,type,comment\n\"x,%Q0.0", "name,address,type,comment\nA,%Q0.0,Bool,\nA,%Q0.0,Bool,\n"] {
        let r = h.run("setprop", json!({"project": true, "key": "plc.sync.table", "value": v}));
        if r.is_ok() {
            let _ = h.run("plcsync", json!({}));
            let _ = h.run("checks", json!({}));
            let _ = differences(&mut h, "-A03");
        }
    }
}
