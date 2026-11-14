//! Brief 045b (design 045 §4–§7, §9 b-rows, §13 F–M, T): channels, pins, addressing and texts.

use ecad_cadcraft::checks::Fix;
use ecad_cadcraft::devices::TextOf;
use ecad_cadcraft::outputs::accessory_fix_commands;
use ecad_cadcraft::plc::PlcFix;
use ecad_model::{Author, ComponentId, ObjectRef, Op, Subject, Terminal, TerminalId};
use serde_json::{Value, json};

use super::{checks_of, place, placed, prop, same_as_built};
use crate::common::{H, id, pts};

/// Fixture P, reduced (design 045 §13): a CPU `-A01` with DI `-A02` (slot 1), DI `-A03` (2), AI
/// `-A04` (3), IOL `-A05` (4) and DQ `-A06` (5), and eight vertical drops 10 mm apart.
struct P {
    h: H,
    cpu: ComponentId,
    di: ComponentId,
    di2: ComponentId,
    ai: ComponentId,
    iol: ComponentId,
    dq: ComponentId,
}

fn fixture(seed: u64) -> P {
    let mut h = H::new(seed);
    let cpu = placed(&mut h, "plccpu", 50.0, 400.0);
    let di = placed(&mut h, "plcdi", 100.0, 400.0);
    let di2 = placed(&mut h, "plcdi", 140.0, 400.0);
    let ai = placed(&mut h, "plcai", 180.0, 400.0);
    let iol = placed(&mut h, "plciol", 220.0, 400.0);
    let dq = placed(&mut h, "plcdq", 260.0, 400.0);
    // Eight drops from lamps (so each channel is used once it lands).
    for i in 0..8 {
        let x = 100.0 + 10.0 * f64::from(i);
        placed(&mut h, "lamp", x, 260.0);
        h.run("wire", json!({"points": pts(&[(x, 250.0), (x, 150.0)])})).unwrap();
    }
    P { h, cpu, di, di2, ai, iol, dq }
}

fn fence(h: &mut H, card: ComponentId) -> Value {
    h.run("plcchannels", json!({"card": card.hex(), "from": [95.0, 200.0], "to": [175.0, 200.0]})).unwrap()
}

/// (key, label) of card `c`'s channel points, by key.
fn points(h: &mut H, c: ComponentId) -> Vec<(String, String)> {
    let d = h.doc();
    let mut v: Vec<(String, String)> = d
        .terminals_of(c)
        .filter(|(_, t)| t.key.as_deref().is_some_and(|k| k.starts_with('.')))
        .map(|(_, t)| (t.key.clone().unwrap_or_default(), t.label.clone().unwrap_or_default()))
        .collect();
    v.sort_by_key(|(k, _)| {
        let i: u32 = k.trim_start_matches('.').split('.').next().and_then(|n| n.parse().ok()).unwrap_or(0);
        (i, k.clone())
    });
    v
}

fn addr(h: &mut H, c: ComponentId, i: u32) -> Option<String> {
    h.cfg(|p| {
        p.outputs();
        let o = p.outputs.as_ref().unwrap();
        o.plc.address_of(p.ledger.document(), c, i).0
    })
}

fn out_addr(h: &mut H, c: ComponentId, i: u32) -> Option<String> {
    h.cfg(|p| {
        p.outputs();
        let o = p.outputs.as_ref().unwrap();
        o.plc.automatic(p.ledger.document(), c, i, true)
    })
}

fn run_fix(h: &mut H, fix: Fix, i: usize) -> Value {
    let (cmd, params) = h
        .cfg(|p| accessory_fix_commands(p.outputs(), fix).into_iter().nth(i).map(|(_, c, v)| (c.to_string(), v)))
        .unwrap_or_else(|| panic!("no button {i} for {fix:?}"));
    h.run(&cmd, params).unwrap_or_else(|e| panic!("{cmd}: {e}"))
}

/// The texts of `kind` on page 1 for card `c`, (value, shown).
fn texts(h: &mut H, c: ComponentId, kind: fn(TextOf) -> bool) -> Vec<(String, bool)> {
    let d = h.doc();
    let page = d.page_order[0];
    let reps: Vec<_> = d.representations_of(Subject::Component(c)).map(|(r, _)| r).collect();
    ecad_cadcraft::devices::all_texts_on_page(&d, page)
        .into_iter()
        .filter(|t| reps.contains(&t.text.rep) && kind(t.text.of))
        .map(|t| (t.text.value, t.shown))
        .collect()
}

#[test]
fn f_a_fence_across_eight_wires_makes_channels_0_to_7_and_rest_draws_the_spares() {
    let P { mut h, di, .. } = fixture(4521);
    let before = h.head();
    let v = fence(&mut h, di);
    assert_eq!(h.head(), before + 1, "one transaction");
    assert_eq!(v["message"], "Placed -A02 channels 0–7 on 8 wires (pins 1–8).", "{v}");
    let pts = points(&mut h, di);
    assert_eq!(pts.len(), 8);
    assert_eq!(pts[0], (".0.in".to_string(), "1".to_string()));
    assert_eq!(pts[7], (".7.in".to_string(), "8".to_string()));
    // Each point is on its wire.
    let d = h.doc();
    for (t, _) in d.terminals_of(di).filter(|(_, t)| t.key.as_deref().is_some_and(|k| k.starts_with('.'))) {
        assert!(!d.ends_at(t).is_empty(), "channel point on its wire");
    }
    let v = h.run("plcchannels", json!({"card": "-A02", "rest": true})).unwrap();
    assert_eq!(v["message"], "Drew -A02 channels 8–15 (8 spare).", "{v}");
    assert_eq!(points(&mut h, di).len(), 16);
    // Drawn spares print Spare; wired channels print no description.
    let descs = texts(&mut h, di, |o| matches!(o, TextOf::ChannelText { .. }));
    assert_eq!(descs.iter().filter(|(v, _)| v == "Spare").count(), 8, "{descs:?}");
    let e = h.run("plcchannels", json!({"card": "-A02", "rest": true})).unwrap_err();
    assert!(e.contains("Every channel of -A02 is drawn (16)"), "{e}");
    same_as_built(&mut h);
    h.cmdline("u");
    assert_eq!(points(&mut h, di).len(), 8);
    h.cmdline("u");
    assert_eq!(points(&mut h, di).len(), 0);
}

/// A channel cell has one point, not a way through (screenshot 045-schematic-cells: the half
/// below each point ran on through the cell box and its number covered the channel number). A
/// fence keeps only the half of each wire that reaches something; a wire attached at both ends
/// keeps both halves.
#[test]
fn f_a_cell_ends_a_loose_wire_at_its_point() {
    let mut h = H::new(4532);
    let di = placed(&mut h, "plcdi", 100.0, 400.0);
    // A lamp's X2 is 10 mm below its origin, its X1 10 mm above.
    for x in [100.0, 110.0, 120.0] {
        placed(&mut h, "lamp", x, 260.0);
    }
    placed(&mut h, "lamp", 120.0, 140.0);
    // From the lamp down to a loose end; drawn the other way, loose end first; lamp to lamp.
    h.run("wire", json!({"points": pts(&[(100.0, 250.0), (100.0, 150.0)])})).unwrap();
    h.run("wire", json!({"points": pts(&[(110.0, 150.0), (110.0, 250.0)])})).unwrap();
    h.run("wire", json!({"points": pts(&[(120.0, 250.0), (120.0, 150.0)])})).unwrap();
    let wires = h.doc().conductors.len();
    let v = h.run("plcchannels", json!({"card": di.hex(), "from": [95.0, 200.0], "to": [125.0, 200.0]})).unwrap();
    assert_eq!(v["message"], "Placed -A01 channels 0–2 on 3 wires (pins 1–3).", "{v}");
    let d = h.doc();
    assert_eq!(d.conductors.len(), wires + 1, "only the lamp-to-lamp wire is split in two");
    let page = d.page_order[0];
    let mut cells: Vec<(String, TerminalId)> =
        d.terminals_of(di).filter_map(|(t, x)| x.key.as_ref().filter(|k| k.starts_with('.')).map(|k| (k.clone(), t))).collect();
    cells.sort();
    let ends: Vec<usize> = cells.iter().map(|(_, t)| d.ends_at(*t).len()).collect();
    assert_eq!(ends, [1, 1, 2], "{cells:?}");
    for (_, t) in cells.iter().take(2) {
        let (c, _) = d.ends_at(*t)[0];
        let wire = &d.conductors[&c];
        assert!(wire.ends.iter().all(Option::is_some), "the kept half runs from the lamp to the point: {wire:?}");
        let (_, _, route) = ecad_cadcraft::wires::route_vertices(&d, c, page).unwrap();
        assert!(route.iter().all(|q| q.y >= 200.0 - 1e-9), "nothing past the point, into the cell: {route:?}");
    }
    same_as_built(&mut h);
    h.cmdline("u");
    assert_eq!(h.doc().conductors.len(), wires);
    same_as_built(&mut h);
}

#[test]
fn g_analogue_pins_follow_the_role_or_channel_layout() {
    let P { mut h, ai, .. } = fixture(4522);
    for x in [300.0, 330.0] {
        place(&mut h, "chai", x, 300.0, json!({"device": ai.hex()})).unwrap();
    }
    assert_eq!(points(&mut h, ai), [(".0.+", "1"), (".0.-", "5"), (".1.+", "2"), (".1.-", "6")].map(|(k, l)| (k.to_string(), l.to_string())));
    let ai2 = placed(&mut h, "plcai", 400.0, 400.0);
    h.run("setdefault", json!({"key": "plc.pins", "level": "object", "component": ai2.hex(), "value": "channel"})).unwrap();
    for x in [300.0, 330.0] {
        place(&mut h, "chai", x, 250.0, json!({"device": ai2.hex()})).unwrap();
    }
    assert_eq!(points(&mut h, ai2), [(".0.+", "1"), (".0.-", "2"), (".1.+", "3"), (".1.-", "4")].map(|(k, l)| (k.to_string(), l.to_string())));
    // A named channel, and one already drawn.
    let v = place(&mut h, "chai", 360.0, 300.0, json!({"device": ai.hex(), "channel": 3})).unwrap();
    assert_eq!(v["channel"], 3);
    let e = place(&mut h, "chai", 390.0, 300.0, json!({"device": ai.hex(), "channel": 0})).unwrap_err();
    assert!(e.contains("Channel 0 of -A04 is drawn on") && e.contains("next free one (2)"), "{e}");
    place(&mut h, "chai", 390.0, 300.0, json!({"device": ai.hex()})).unwrap();
    let e = place(&mut h, "chai", 420.0, 300.0, json!({"device": ai.hex()})).unwrap_err();
    assert!(e.contains("-A04's 4 channels are all drawn"), "{e}");
    same_as_built(&mut h);
}

#[test]
fn h_a_wrong_type_is_refused_and_a_compact_di_joins_as_the_next_channel() {
    let P { mut h, di, cpu, .. } = fixture(4523);
    fence(&mut h, di);
    let e = place(&mut h, "chai", 300.0, 300.0, json!({"device": di.hex()})).unwrap_err();
    assert_eq!(e, "-A02 is a DI card; pick a DI channel (chdi, di), or another card.");
    let e = place(&mut h, "chdi", 300.0, 300.0, json!({"device": cpu.hex()})).unwrap_err();
    assert!(e.starts_with("-A01 is a CPU; channels go on its cards."), "{e}");
    let v = place(&mut h, "di", 300.0, 300.0, json!({"device": di.hex()})).unwrap();
    assert_eq!((v["channel"].clone(), v["terminals"][0]["label"].clone(), v["terminals"][0]["key"].clone()), (json!(8), json!("9"), json!(".8.in")));
    // A cell on its own makes a new card of its type, in no rack.
    let v = place(&mut h, "chrtd", 300.0, 100.0, json!({})).unwrap();
    let c: ComponentId = id(&v, "component");
    assert_eq!(ecad_cadcraft::plc::card_type_of(&h.doc(), c), Some(ecad_cadcraft::plc::rack::CardType::Rtd));
    assert_eq!(v["terminals"].as_array().map(Vec::len), Some(4));
}

#[test]
fn i_addresses_in_three_styles() {
    let P { mut h, cpu, di, di2, ai, iol, dq } = fixture(4524);
    assert_eq!(addr(&mut h, di, 0).as_deref(), Some("I0.0"));
    assert_eq!(addr(&mut h, di, 15).as_deref(), Some("I1.7"));
    assert_eq!(addr(&mut h, di2, 0).as_deref(), Some("I2.0"));
    assert_eq!(addr(&mut h, di2, 15).as_deref(), Some("I3.7"));
    let ais: Vec<Option<String>> = (0..4).map(|i| addr(&mut h, ai, i)).collect();
    assert_eq!(ais, ["IW4", "IW6", "IW8", "IW10"].map(|s| Some(s.to_string())));
    assert_eq!(addr(&mut h, iol, 0).as_deref(), Some("IW12"));
    assert_eq!(addr(&mut h, iol, 3).as_deref(), Some("IW18"));
    assert_eq!(out_addr(&mut h, iol, 0).as_deref(), Some("QW0"));
    assert_eq!(out_addr(&mut h, iol, 3).as_deref(), Some("QW6"));
    assert_eq!(addr(&mut h, dq, 0).as_deref(), Some("Q8.0"));
    assert_eq!(addr(&mut h, dq, 15).as_deref(), Some("Q9.7"));
    h.run("plcaddress", json!({"rack": cpu.hex(), "style": "iec"})).unwrap();
    assert_eq!(addr(&mut h, di, 0).as_deref(), Some("%IX0.0"));
    assert_eq!(addr(&mut h, ai, 1).as_deref(), Some("%IW6"));
    h.run("plcaddress", json!({"rack": "-A01", "style": "slot"})).unwrap();
    assert_eq!(addr(&mut h, di2, 4).as_deref(), Some("I:2/04"));
    assert_eq!(addr(&mut h, dq, 1).as_deref(), Some("O:5/01"));
    h.run("plcaddress", json!({"rack": "-A01", "style": "none"})).unwrap();
    assert_eq!(addr(&mut h, di, 0), None);
    let e = h.run("plcaddress", json!({"rack": "-A01", "style": "vendor"})).unwrap_err();
    assert!(e.contains("is not a value of plc.address.style"), "{e}");
    same_as_built(&mut h);
}

#[test]
fn j_a_slot_move_shifts_later_cards_until_the_rack_is_fixed() {
    let P { mut h, di, di2, ai, .. } = fixture(4525);
    let v = h.run("plcslot", json!({"card": di.hex(), "slot": 9})).unwrap();
    assert!(v["message"].as_str().unwrap().contains("(addresses of 3 cards change)"), "{v}");
    assert_eq!(addr(&mut h, di2, 0).as_deref(), Some("I0.0"));
    assert_eq!(addr(&mut h, ai, 0).as_deref(), Some("IW2"));
    assert_eq!(addr(&mut h, di, 0).as_deref(), Some("I18.0"), "after the IOL inputs");
    h.cmdline("u");
    let v = h.run("plcaddress", json!({"rack": "-A01", "fix": true})).unwrap();
    assert!(v["message"].as_str().unwrap().contains("5 cards pinned at their current addresses"), "{v}");
    assert_eq!(prop(&mut h, ai, "plc.addr.start"), Some("4".into()));
    let v = h.run("plcslot", json!({"card": di.hex(), "slot": 9})).unwrap();
    assert!(!v["message"].as_str().unwrap().contains("addresses of"), "{v}");
    assert_eq!(addr(&mut h, di2, 0).as_deref(), Some("I2.0"));
    assert_eq!(addr(&mut h, di, 0).as_deref(), Some("I0.0"), "pinned at its start");
    h.run("plcaddress", json!({"rack": "-A01", "clear": true})).unwrap();
    assert_eq!(prop(&mut h, ai, "plc.addr.start"), None);
    same_as_built(&mut h);
}

#[test]
fn k_typed_address_and_description_show_on_the_cell_and_a_duplicate_is_flagged() {
    let P { mut h, di, di2, .. } = fixture(4526);
    fence(&mut h, di);
    let v = h.run("plcchannel", json!({"card": "-A02", "channel": 3, "text": "Door closed", "address": "I9.9"})).unwrap();
    assert!(v["message"].as_str().unwrap().starts_with("-A02 channel 3: text (none) → \"Door closed\""), "{v}");
    assert_eq!(prop(&mut h, di, "plc.ch.3.text"), Some("Door closed".into()));
    let addrs = texts(&mut h, di, |o| o == TextOf::Address);
    assert!(addrs.contains(&("I9.9".to_string(), true)), "{addrs:?}");
    assert!(addrs.contains(&("I0.0".to_string(), true)));
    let descs = texts(&mut h, di, |o| o == TextOf::ChannelText { rotated: true });
    assert!(descs.contains(&("Door closed".to_string(), true)), "{descs:?}");
    // A typed address another channel has: address-twice, fixed by clearing the typed one.
    h.run("plcchannel", json!({"card": di2.hex(), "channel": 0, "address": "I0.1"})).unwrap();
    let rows = checks_of(&mut h, "address-twice");
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(rows[0]["which"].as_str().unwrap().starts_with("I0.1 is the address of"), "{rows:?}");
    run_fix(&mut h, Fix::Plc(di2, PlcFix::ClearTyped(0)), 0);
    assert!(checks_of(&mut h, "address-twice").is_empty());
    assert_eq!(prop(&mut h, di2, "plc.ch.0.address"), None);
    // Pins relabel; a taken pin is refused with its holder.
    h.run("plcchannel", json!({"card": "-A02", "channel": 0, "pins": {"in": "X1"}})).unwrap();
    let e = h.run("plcchannel", json!({"card": "-A02", "channel": 1, "pins": {"in": "X1"}})).unwrap_err();
    assert_eq!(e, "-A02 already has pin X1 (channel 0, in). Pick another label.");
    let e = h.run("plcchannel", json!({"card": "-A02", "channel": 1, "text": "x".repeat(81)})).unwrap_err();
    assert!(e.contains("1 to 80 characters"), "{e}");
    same_as_built(&mut h);
}

#[test]
fn l_registry_switches_change_the_texts() {
    let P { mut h, di, .. } = fixture(4527);
    fence(&mut h, di);
    h.run("plcchannel", json!({"card": "-A02", "channel": 0, "text": "Pump 1 running"})).unwrap();
    h.run("setdefault", json!({"key": "plc.text.address.show", "level": "project", "value": "off"})).unwrap();
    assert!(texts(&mut h, di, |o| o == TextOf::Address).iter().all(|(_, shown)| !shown));
    h.run("setdefault", json!({"key": "plc.text.channel.rotation", "level": "project", "value": "0"})).unwrap();
    assert!(texts(&mut h, di, |o| o == TextOf::ChannelText { rotated: false }).contains(&("Pump 1 running".into(), true)));
    // A project default reaches only the cards that inherit it.
    let n = h.cfg(|p| p.outputs().plc.recomputed);
    h.run("setdefault", json!({"key": "plc.channel.first", "level": "object", "component": di.hex(), "value": "1"})).unwrap();
    let one = h.cfg(|p| p.outputs().plc.recomputed) - n;
    assert!(one <= 2, "an object value recomputes its card only ({one})");
    let nums = texts(&mut h, di, |o| o == TextOf::ChannelNo);
    assert!(nums.iter().any(|(v, _)| v == "1") && !nums.iter().any(|(v, _)| v == "0"), "{nums:?}");
    same_as_built(&mut h);
}

#[test]
fn m_channel_and_address_rows_have_working_fixes() {
    let P { mut h, di, ai, .. } = fixture(4528);
    fence(&mut h, di);
    // channel-over: 8 drawn on a card of 4.
    h.run("setprop", json!({"component": di.hex(), "key": "plc.channels", "value": "4"})).unwrap();
    let rows = checks_of(&mut h, "channel-over");
    assert_eq!(rows[0]["which"], "-A02 has channel 7 drawn; it has 4 channels");
    run_fix(&mut h, Fix::Plc(di, PlcFix::ChannelOver(7)), 0);
    assert_eq!(prop(&mut h, di, "plc.channels"), Some("8".into()));
    assert!(checks_of(&mut h, "channel-over").is_empty());
    // address-overlap: the AI pinned onto the DI's bytes.
    h.run("plcaddress", json!({"card": ai.hex(), "start": 0})).unwrap();
    let rows = checks_of(&mut h, "address-overlap");
    assert_eq!(rows.len(), 1, "{rows:?}");
    run_fix(&mut h, Fix::Plc(ai, PlcFix::Overlap(0)), 1);
    assert!(checks_of(&mut h, "address-overlap").is_empty(), "Unpin");
    // address-overflow: a block past byte 65535.
    h.run("plcaddress", json!({"card": ai.hex(), "start": 65535, "size": 4096})).unwrap();
    assert_eq!(checks_of(&mut h, "address-overflow").len(), 1);
    run_fix(&mut h, Fix::Plc(ai, PlcFix::Unpin), 0);
    assert!(checks_of(&mut h, "address-overflow").is_empty());
    // no-address: used channels with style none.
    h.run("plcaddress", json!({"rack": "-A01", "style": "none"})).unwrap();
    let rows = checks_of(&mut h, "no-address");
    assert_eq!(rows.len(), 1, "{rows:?}");
    // channel-twice: a hand-made second copy of channel 0's key.
    let d = h.doc();
    let (t1, _) = d.terminals_of(di).find(|(_, t)| t.key.as_deref() == Some(".1.in")).unwrap();
    let term = d.terminals.get(&t1).cloned().unwrap();
    let after = Terminal { key: Some(".0.in".into()), ..term.clone() };
    h.cfg(|p| {
        p.ledger
            .commit(Author::User, "hand-made", vec![Op::RemoveTerminal { id: t1, terminal: term }, Op::AddTerminal { id: t1, terminal: after }])
            .map(|_| ())
            .unwrap()
    });
    let mut h = reload(h, 4528);
    let rows = checks_of(&mut h, "channel-twice");
    assert_eq!(rows.len(), 1, "{rows:?}");
    run_fix(&mut h, Fix::Plc(di, PlcFix::ChannelTwice(0)), 0);
    assert!(checks_of(&mut h, "channel-twice").is_empty());
    same_as_built(&mut h);
}

#[test]
fn t_hostile_keys_counts_and_values_never_panic() {
    let P { mut h, di, .. } = fixture(4529);
    fence(&mut h, di);
    // Commands refuse in words.
    let e = h.run("plcchannels", json!({"card": "-A02", "from": [0.0, 0.0], "to": [1e12, 0.0]})).unwrap_err();
    assert!(!e.is_empty());
    let e = h.run("plcchannel", json!({"card": "-A02", "channel": 99_999})).unwrap_err();
    assert!(e.contains("whole number"), "{e}");
    let e = h.run("setprop", json!({"component": di.hex(), "key": "plc.addr.start", "value": "70000"})).unwrap_err();
    assert!(e.contains("0 to 65535"), "{e}");
    let d = h.doc();
    let ts: Vec<(TerminalId, Terminal)> =
        d.terminals_of(di).filter(|(_, t)| t.key.as_deref().is_some_and(|k| k.starts_with('.'))).map(|(i, t)| (i, t.clone())).take(4).collect();
    let mut ops = Vec::new();
    for ((t, term), k) in ts.into_iter().zip([".999.in", ".-1.in", ".3.nope", ".3"]) {
        ops.push(Op::RemoveTerminal { id: t, terminal: term.clone() });
        ops.push(Op::AddTerminal { id: t, terminal: Terminal { key: Some(k.into()), ..term } });
    }
    ops.push(Op::SetProperty { object: ObjectRef::Component(di), key: "plc.ch.0.text".into(), before: None, after: Some("x".repeat(10_000)) });
    ops.push(Op::SetProperty { object: ObjectRef::Component(di), key: "plc.addr.start".into(), before: None, after: Some("65535".into()) });
    ops.push(Op::SetProperty { object: ObjectRef::Component(di), key: "plc.addr.size".into(), before: None, after: Some("4096".into()) });
    h.cfg(|p| p.ledger.commit(Author::User, "hand-made", ops).map(|_| ()).unwrap());
    let d = h.doc();
    let _ = ecad_cadcraft::checks::all(&d, 1e-6);
    let _ = ecad_cadcraft::devices::all_texts_on_page(&d, d.page_order[0]);
}

#[test]
#[ignore = "slow: draws 260 wires through commands"]
fn t_a_fence_across_too_many_wires_is_refused() {
    let mut h = H::new(4530);
    for i in 0..260 {
        let x = 10.0 + 1.0 * f64::from(i);
        h.run("wire", json!({"points": pts(&[(x, 250.0), (x, 150.0)])})).unwrap();
    }
    let e = h.run("plcchannels", json!({"type": "DI", "from": [5.0, 200.0], "to": [300.0, 200.0]})).unwrap_err();
    assert!(e.contains("crosses 260 wires; at most 256"), "{e}");
}

/// Save the project as it is (after a hand-made commit) and open it as a user would.
fn reload(mut h: H, seed: u64) -> H {
    let dir = std::env::temp_dir().join(format!("ecad t045 hand made {}", ecad_store::disk::unique()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("hand made.ecad").display().to_string();
    h.cfg(|p| p.persistence = ecad_cadcraft::Persistence::File);
    h.s.execute("saveas", &json!({"path": path})).unwrap();
    while !h.s.docs.is_empty() {
        h.s.execute("close", &json!({"index": 0})).unwrap();
    }
    drop(h);
    let mut s = cadcraft_engine::Session::empty();
    let (project, _) = ecad_cadcraft::ProjectHandle::open_file(&mut s, &path, seed).unwrap();
    let mut h = H { s, project, trail: Vec::new() };
    let d = h.doc();
    h.trail.push(d);
    h
}
