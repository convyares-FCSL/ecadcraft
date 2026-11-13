//! Brief 045b, channels (design 045 §4, §6, §7, §9 b-rows `channel-*`, §13 F, G, H, K, L, M, T;
//! FP2, FP3, FP6, FP7).

use ecad_model::ComponentId;
use serde_json::json;

use super::*;
use crate::common::id;

// ------------------------------------------------------------------------------- F (AC 1)

/// F: a fence across the eight input drops makes `-A03` channels 0–7, pins 1–8, keys `.<i>.in`, in
/// one transaction; `u`/redo exact; the echo names card, channels, wires and pins.
#[test]
#[ignore = "tester 045: awaiting build"]
fn f_fence_makes_channels_0_to_7_in_one_transaction() {
    let (mut p, c) = with_cards(0x7045_f101);
    let a03 = c["-A03"];
    p.h.activate(p.inputs);
    step(&mut p.h, "plcchannels", json!({"card": a03.hex(), "from": [90, 200], "to": [180, 200]}));
    let d = p.h.doc();
    let pts = channel_points(&d, a03);
    let got: Vec<(String, String)> = pts.iter().map(|(k, (l, _))| (k.clone(), l.clone())).collect();
    let want: Vec<(String, String)> = (0..8).map(|i| (format!(".{i}.in"), format!("{}", i + 1))).collect();
    let mut got_sorted = got.clone();
    got_sorted.sort_by_key(|(k, _)| k.split('.').nth(1).and_then(|x| x.parse::<u32>().ok()));
    assert_eq!(got_sorted, want, "FP2: key .<i>.in, label = pin (role layout)");
    let e = echo(&p.h, 3);
    assert!(e.contains("Placed -A03 channels 0\u{2013}7 on 8 wires (pins 1\u{2013}8)."), "{e}");
    // Each channel point sits on its own drop, left to right.
    let mut xs: Vec<(u32, f64)> = Vec::new();
    for i in 0..8 {
        let (_, t) = pts[&format!(".{i}.in")].clone();
        xs.push((i, p.h.terminal_pos(p.inputs, t).x));
    }
    for (i, x) in &xs {
        assert!((x - IN_XS[*i as usize]).abs() < 1e-6, "channel {i} at x {x}");
    }
    p.h.check();
}

/// F: Draw remaining channels (`rest`) draws 8–15 to the right as `Spare` cells, no wires, one
/// transaction; `u` undoes it alone.
#[test]
#[ignore = "tester 045: awaiting build"]
fn f_rest_draws_the_remaining_channels_as_spare() {
    let (mut p, c) = with_cards(0x7045_f102);
    let a03 = c["-A03"];
    fence_inputs(&mut p, a03);
    let wires = p.h.doc().conductors.len();
    step(&mut p.h, "plcchannels", json!({"card": a03.hex(), "rest": true}));
    let d = p.h.doc();
    assert_eq!(drawn_channels(&d, a03), (0..16).collect::<Vec<_>>());
    assert_eq!(d.conductors.len(), wires, "rest draws no wires");
    assert_eq!(pin(&d, a03, 15, "in").as_deref(), Some("16"));
    let e = echo(&p.h, 3);
    assert!(e.contains("Drew -A03 channels 8\u{2013}15 (8 spare)."), "{e}");
    let spares = text_values(&mut p.h, p.inputs).iter().filter(|t| *t == "Spare").count();
    assert_eq!(spares, 8, "a drawn spare cell shows plc.spare.text");
    // The rest cells continue to the right of the last fenced cell.
    let x15 = p.h.terminal_pos(p.inputs, channel_points(&d, a03)[".15.in"].1).x;
    assert!(x15 > IN_XS[7], "{x15}");
    // Nothing left: rest is refused or a no-op, never a second set.
    let head = p.h.head();
    let _ = p.h.run("plcchannels", json!({"card": a03.hex(), "rest": true}));
    assert!(p.h.head() <= head + 1);
    assert_eq!(channel_points(&p.h.doc(), a03).len(), 16);
    // Counts: 8 used (wired to the strip), 8 spare.
    let row = slot_row(&mut p.h, "-A03").expect("-A03 in plcracks");
    assert_eq!((num(&row["used"]), num(&row["spare"]), num(&row["reserved"])), (Some(8), Some(8), Some(0)), "{row}");
    p.h.check();
}

/// F (rest with `at`): the cells go where asked, on another page too.
#[test]
#[ignore = "tester 045: awaiting build"]
fn f_rest_at_a_point() {
    let (mut p, c) = with_cards(0x7045_f103);
    p.h.activate(p.outputs);
    step(&mut p.h, "plcchannels", json!({"card": c["-A07"].hex(), "rest": true, "at": [300, 100]}));
    assert_eq!(drawn_channels(&p.h.doc(), c["-A07"]).len(), 16);
}

/// F: a fence with Card New and a type makes a new card (prefix A), no rack (design §4.4 rule 2).
#[test]
#[ignore = "tester 045: awaiting build"]
fn f_fence_new_card() {
    let mut p = fixture_p(0x7045_f104);
    p.h.activate(p.inputs);
    let v = step(&mut p.h, "plcchannels", json!({"card": null, "type": "DI", "from": [90, 200], "to": [180, 200]}));
    let card: ComponentId = id(&v, "card");
    let d = p.h.doc();
    assert!(tag(&d, card).starts_with("-A"));
    assert_eq!(drawn_channels(&d, card), (0..8).collect::<Vec<_>>());
    assert_eq!(prop(&d, card, "plc.rack"), None);
}

// ------------------------------------------------------------------------------- G (AC 2)

/// G: `chai` twice with Device `-A05`: channels 0 and 1, pins `+` 1, 2 and `-` 5, 6 (role layout).
#[test]
#[ignore = "tester 045: awaiting build"]
fn g_ai_pins_role_layout() {
    let (mut p, c) = with_cards(0x7045_1e001);
    let a05 = c["-A05"];
    p.h.activate(p.analogue);
    step(&mut p.h, "place", json!({"symbol": "chai", "at": [100, 200], "device": a05.hex()}));
    step(&mut p.h, "place", json!({"symbol": "chai", "at": [120, 200], "device": a05.hex()}));
    let d = p.h.doc();
    assert_eq!(
        [pin(&d, a05, 0, "+"), pin(&d, a05, 0, "-"), pin(&d, a05, 1, "+"), pin(&d, a05, 1, "-")],
        [Some("1".to_string()), Some("5".to_string()), Some("2".to_string()), Some("6".to_string())]
    );
    p.h.check();
}

/// G: with `plc.pins channel` on `-A05` first: 1, 2 / 3, 4. A drawn pin is never recomputed when
/// the layout changes afterwards (Q4).
#[test]
#[ignore = "tester 045: awaiting build"]
fn g_ai_pins_channel_layout_and_never_rewritten() {
    let (mut p, c) = with_cards(0x7045_1e002);
    let a05 = c["-A05"];
    p.h.run("setdefault", json!({"key": "plc.pins", "level": "object", "component": a05.hex(), "value": "channel"})).unwrap();
    p.h.activate(p.analogue);
    p.h.run("place", json!({"symbol": "chai", "at": [100, 200], "device": a05.hex()})).unwrap();
    p.h.run("place", json!({"symbol": "chai", "at": [120, 200], "device": a05.hex()})).unwrap();
    let d = p.h.doc();
    assert_eq!(
        [pin(&d, a05, 0, "+"), pin(&d, a05, 0, "-"), pin(&d, a05, 1, "+"), pin(&d, a05, 1, "-")],
        [Some("1".to_string()), Some("2".to_string()), Some("3".to_string()), Some("4".to_string())]
    );
    let before = channel_points(&d, a05);
    p.h.run("setdefault", json!({"key": "plc.pins", "level": "object", "component": a05.hex(), "value": "role"})).unwrap();
    p.h.run("setprop", json!({"component": a05.hex(), "key": "plc.channels", "value": "8"})).unwrap();
    assert_eq!(channel_points(&p.h.doc(), a05), before, "drawn pins stay");
}

/// G: RTD and IO-Link cells carry their four and three roles (design §4.1), role layout pins.
#[test]
#[ignore = "tester 045: awaiting build"]
fn g_rtd_and_iolink_roles() {
    let (mut p, c) = with_cards(0x7045_1e003);
    p.h.activate(p.iolink);
    p.h.run("place", json!({"symbol": "chiol", "at": [100, 200], "device": c["-A06"].hex()})).unwrap();
    let d = p.h.doc();
    let keys: Vec<String> = channel_points(&d, c["-A06"]).keys().cloned().collect();
    assert_eq!(keys.len(), 3, "{keys:?}");
    for k in [".0.L+", ".0.C/Q", ".0.L-"] {
        assert!(keys.iter().any(|x| x == k), "{k}: {keys:?}");
    }
    assert_eq!(
        [pin(&d, c["-A06"], 0, "L+"), pin(&d, c["-A06"], 0, "C/Q"), pin(&d, c["-A06"], 0, "L-")],
        [Some("1".to_string()), Some("5".to_string()), Some("9".to_string())]
    );
    // An RTD card in no rack, placed by its first cell.
    let v = p.h.run("place", json!({"symbol": "chrtd", "at": [200, 200]})).unwrap();
    let rtd: ComponentId = id(&v, "component");
    let keys: Vec<String> = channel_points(&p.h.doc(), rtd).keys().cloned().collect();
    assert_eq!(keys.len(), 4, "{keys:?}");
    for k in [".0.I+", ".0.S+", ".0.S-", ".0.I-"] {
        assert!(keys.iter().any(|x| x == k), "{k}: {keys:?}");
    }
}

// ------------------------------------------------------------------------------- H (AC 2)

/// H: wrong type, a taken channel, a full card and a rack are each refused with their sentence; a
/// compact `di` on `-A03` (0–7 drawn) joins as channel 8 with pin 9.
#[test]
#[ignore = "tester 045: awaiting build"]
fn h_refusals_and_compact_di_joins_as_next_channel() {
    let (mut p, c) = with_cards(0x7045_2e001);
    let a03 = c["-A03"];
    fence_inputs(&mut p, a03);
    p.h.activate(p.analogue);
    refused(
        &mut p.h,
        "place",
        json!({"symbol": "chai", "at": [100, 100], "device": a03.hex()}),
        &["-A03 is a DI card; pick a DI channel (chdi, di), or another card."],
    );
    refused(&mut p.h, "place", json!({"symbol": "chdi", "at": [100, 100], "device": p.rack.hex()}), &["-A01 is a CPU; channels go on its cards."]);
    refused(&mut p.h, "place", json!({"symbol": "chdi", "at": [100, 100], "device": c["-A02"].hex()}), &["channels go on its cards"]);
    refused(
        &mut p.h,
        "place",
        json!({"symbol": "chdi", "at": [100, 100], "device": a03.hex(), "channel": 3}),
        &["Channel 3 of -A03 is drawn on", "Enter for the next free one (8)."],
    );
    let v = step(&mut p.h, "place", json!({"symbol": "di", "at": [100, 100], "device": a03.hex()}));
    let d = p.h.doc();
    assert_eq!(pin(&d, a03, 8, "in").as_deref(), Some("9"), "compact di joins as channel 8: {v}");
    // A named channel.
    step(&mut p.h, "place", json!({"symbol": "chdi", "at": [140, 100], "device": a03.hex(), "channel": 12}));
    assert!(drawn_channels(&p.h.doc(), a03).contains(&12));
    // Full card.
    p.h.run("plcchannels", json!({"card": a03.hex(), "rest": true})).unwrap();
    refused(
        &mut p.h,
        "place",
        json!({"symbol": "di", "at": [160, 100], "device": a03.hex()}),
        &["-A03's 16 channels are all drawn. Raise its channel count in Properties, or pick another card."],
    );
    p.h.check();
}

/// H: a compact `do` on a DQ card joins as its next channel, role `out`; a lone `di` makes a new
/// DI card (`-A` prefix) with channel 0, pin 1, key `.0.in` (FP7: not `I0.0`).
#[test]
#[ignore = "tester 045: awaiting build"]
fn h_compact_symbols_follow_fp7() {
    let (mut p, c) = with_cards(0x7045_2e002);
    p.h.activate(p.outputs);
    p.h.run("place", json!({"symbol": "do", "at": [100, 200], "device": c["-A07"].hex()})).unwrap();
    assert_eq!(pin(&p.h.doc(), c["-A07"], 0, "out").as_deref(), Some("1"));
    let v = p.h.run("place", json!({"symbol": "di", "at": [300, 100]})).unwrap();
    let card: ComponentId = id(&v, "component");
    let d = p.h.doc();
    assert!(tag(&d, card).starts_with("-A"));
    assert_eq!(channel_points(&d, card).keys().cloned().collect::<Vec<_>>(), [".0.in"]);
    assert_eq!(pin(&d, card, 0, "in").as_deref(), Some("1"));
}

// ------------------------------------------------------------------------------- FP2: old keys

/// FP2: an old `di` terminal (key and label `I1.3`, no leading `.`) reads as channel 11 of its
/// card: the next `di` skips it, a new channel 11 is refused as taken, and the label stays as
/// stored; a hand-made file opens.
#[test]
#[ignore = "tester 045: awaiting build"]
fn fp2_old_di_keys_read_as_a_times_8_plus_b() {
    let (mut p, c) = with_cards(0x7045_5e001);
    let a04 = c["-A04"];
    p.h.activate(p.analogue);
    // Channels 0 and 1 by `di`, then rewritten by hand to the old form I0.0 and I1.3.
    p.h.run("place", json!({"symbol": "di", "at": [100, 200], "device": a04.hex()})).unwrap();
    p.h.run("place", json!({"symbol": "di", "at": [120, 200], "device": a04.hex()})).unwrap();
    let pts = channel_points(&p.h.doc(), a04);
    rekey_raw(&mut p.h, pts[".0.in"].1, Some("I0.0"), Some("I0.0"));
    rekey_raw(&mut p.h, pts[".1.in"].1, Some("I1.3"), Some("I1.3"));
    let mut h = reload(p.h, "old di", 0x7045_5e101);
    read_everything(&mut h);
    let d = h.doc();
    let a04 = by_tag(&d, "-A04");
    let labels: Vec<String> = d.terminals.values().filter(|t| t.component == a04).filter_map(|t| t.label.clone()).collect();
    assert!(labels.contains(&"I1.3".to_string()), "the label stays as stored: {labels:?}");
    // Next free channel: 1 (0 and 11 are drawn).
    h.run("place", json!({"symbol": "di", "at": [140, 200], "device": a04.hex()})).unwrap();
    assert!(channel_points(&h.doc(), a04).contains_key(".1.in"), "channel 1 is the lowest free one");
    refused(&mut h, "place", json!({"symbol": "chdi", "at": [160, 200], "device": a04.hex(), "channel": 11}), &["Channel 11 of -A04 is drawn on"]);
    let old_do = h.run("place", json!({"symbol": "do", "at": [180, 200], "device": by_tag(&d, "-A07").hex()})).unwrap();
    assert!(old_do["terminals"].to_string().contains(".0.out"), "{old_do}");
    h.check();
}

/// FP2: an old `do` key `Q2.7` reads as channel 23: on a 16-channel DQ that is `channel-over`.
#[test]
#[ignore = "tester 045: awaiting build"]
fn fp2_old_do_key_beyond_the_count_is_channel_over() {
    let (mut p, c) = with_cards(0x7045_5e002);
    let a07 = c["-A07"];
    p.h.activate(p.outputs);
    p.h.run("place", json!({"symbol": "do", "at": [300, 200], "device": a07.hex()})).unwrap();
    let t = channel_points(&p.h.doc(), a07)[".0.out"].1;
    rekey_raw(&mut p.h, t, Some("Q2.7"), Some("Q2.7"));
    let mut h = reload(p.h, "old do", 0x7045_5e102);
    let rows = checks_of(&mut h, "channel-over");
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(rows[0].to_string().contains("-A07 has channel 23 drawn; it has 16 channels"), "{}", rows[0]);
}

// ------------------------------------------------------------------------------- K, §4.6 (AC 4)

/// K: `plcchannel` text and typed address: one transaction each, both shown on the cell; the
/// description reads upwards (rotation 90) by default; `null` clears; FP3 keys on the card.
#[test]
#[ignore = "tester 045: awaiting build"]
fn k_plcchannel_text_and_typed_address() {
    let (mut p, c) = with_cards(0x7045_5e003);
    let a03 = c["-A03"];
    fence_inputs(&mut p, a03);
    step(&mut p.h, "plcchannel", json!({"card": a03.hex(), "channel": 3, "text": "Door closed", "address": "I9.9"}));
    let d = p.h.doc();
    assert_eq!(prop(&d, a03, "plc.ch.3.text").as_deref(), Some("Door closed"));
    assert_eq!(prop(&d, a03, "plc.ch.3.address").as_deref(), Some("I9.9"));
    let texts = text_values(&mut p.h, p.inputs);
    assert!(texts.iter().any(|t| t == "I9.9"), "the typed address on the cell: {texts:?}");
    assert!(!texts.iter().any(|t| t == "I0.3"), "the typed address replaces the automatic one: {texts:?}");
    let row = text_row(&mut p.h, p.inputs, "Door closed").expect("the description on the cell");
    assert!(upright(&row), "rotated 90 by default: {row}");
    let e = echo(&p.h, 3);
    assert!(e.contains("-A03 channel 3"), "{e}");
    step(&mut p.h, "plcchannel", json!({"card": a03.hex(), "channel": 3, "address": null}));
    assert_eq!(prop(&p.h.doc(), a03, "plc.ch.3.address"), None);
    assert!(text_values(&mut p.h, p.inputs).iter().any(|t| t == "I0.3"), "automatic again");
    // `setprop` goes through the same rule.
    step(&mut p.h, "setprop", json!({"component": a03.hex(), "key": "plc.ch.4.text", "value": "Pump 1 running"}));
    refused(&mut p.h, "setprop", json!({"component": a03.hex(), "key": "plc.ch.4.text", "value": "x".repeat(81)}), &[]);
    refused(&mut p.h, "plcchannel", json!({"card": a03.hex(), "channel": 4, "address": "x".repeat(33)}), &[]);
    p.h.check();
}

/// §4.6 pins: relabel a channel point (Remove + Add of the same ID and key, FP6); a duplicate,
/// a space or 17 characters is refused with its sentence.
#[test]
#[ignore = "tester 045: awaiting build"]
fn k_plcchannel_relabels_pins() {
    let (mut p, c) = with_cards(0x7045_5e004);
    let a05 = c["-A05"];
    p.h.activate(p.analogue);
    p.h.run("place", json!({"symbol": "chai", "at": [100, 200], "device": a05.hex()})).unwrap();
    let before = channel_points(&p.h.doc(), a05);
    step(&mut p.h, "plcchannel", json!({"card": a05.hex(), "channel": 0, "pins": {"+": "13"}}));
    let after = channel_points(&p.h.doc(), a05);
    assert_eq!(after[".0.+"].0, "13");
    assert_eq!(after[".0.+"].1, before[".0.+"].1, "same terminal ID");
    let ops = p.h.p(|p| p.ledger.log().last().map(|t| t.ops.clone())).unwrap_or_default();
    assert!(
        ops.iter().any(|o| matches!(o, ecad_model::Op::RemoveTerminal { .. })) && ops.iter().any(|o| matches!(o, ecad_model::Op::AddTerminal { .. })),
        "{ops:?}"
    );
    refused(
        &mut p.h,
        "plcchannel",
        json!({"card": a05.hex(), "channel": 0, "pins": {"-": "13"}}),
        &["-A05 already has pin 13 (channel 0, +). Pick another label."],
    );
    refused(&mut p.h, "plcchannel", json!({"card": a05.hex(), "channel": 0, "pins": {"-": "1 3"}}), &[]);
    refused(&mut p.h, "plcchannel", json!({"card": a05.hex(), "channel": 0, "pins": {"-": "x".repeat(17)}}), &[]);
    refused(&mut p.h, "plcchannel", json!({"card": a05.hex(), "channel": 0, "pins": {"nope": "20"}}), &[]);
}

// ------------------------------------------------------------------------------- §6 spare and reserved

/// §6: state is derived: a fenced channel on a net with the strip is `used`; an undrawn or bare
/// channel is `spare`; one with a description and no wire is `reserved` and shows its description,
/// not `Spare`.
#[test]
#[ignore = "tester 045: awaiting build"]
fn spare_and_reserved_are_derived() {
    let (mut p, c) = with_cards(0x7045_9e001);
    let a03 = c["-A03"];
    fence_inputs(&mut p, a03);
    p.h.run("plcchannels", json!({"card": a03.hex(), "rest": true})).unwrap();
    p.h.run("plcchannel", json!({"card": a03.hex(), "channel": 9, "text": "Spare sensor"})).unwrap();
    let row = slot_row(&mut p.h, "-A03").unwrap();
    assert_eq!((num(&row["used"]), num(&row["reserved"]), num(&row["spare"])), (Some(8), Some(1), Some(7)), "{row}");
    let texts = text_values(&mut p.h, p.inputs);
    assert_eq!(texts.iter().filter(|t| *t == "Spare").count(), 7);
    assert!(texts.iter().any(|t| t == "Spare sensor"));
    // An undrawn card's channels count too (what is bought, §2.3).
    let row = slot_row(&mut p.h, "-A04").unwrap();
    assert_eq!((num(&row["used"]), num(&row["spare"])), (Some(0), Some(16)), "{row}");
    // No stored state: no `plc.ch.*.state` or similar key appears.
    let d = p.h.doc();
    assert!(d.components[&a03].properties.keys().all(|k| !k.contains("state") && !k.contains("spare")), "{:?}", d.components[&a03].properties);
    // `plc.spare.text`.
    p.h.run("setdefault", json!({"key": "plc.spare.text", "level": "project", "value": "Reserve"})).unwrap();
    assert_eq!(text_values(&mut p.h, p.inputs).iter().filter(|t| *t == "Reserve").count(), 7);
}

// ------------------------------------------------------------------------------- M (b-rows, channels)

/// M: `channel-over`: channels drawn beyond `plc.channels`; **Raise to n** (`setprop`) clears it.
#[test]
#[ignore = "tester 045: awaiting build"]
fn m_channel_over_and_raise() {
    let (mut p, c) = with_cards(0x7045_7e201);
    let a03 = c["-A03"];
    fence_inputs(&mut p, a03);
    p.h.run("setprop", json!({"component": a03.hex(), "key": "plc.channels", "value": "6"})).unwrap();
    let rows = checks_of(&mut p.h, "channel-over");
    assert!(!rows.is_empty(), "drawing beyond the count is a row, never silent");
    assert!(rows.iter().any(|r| r.to_string().contains("-A03 has channel 7 drawn; it has 6 channels")), "{rows:?}");
    rows_have_a_way_out(&mut p.h, "channel-over");
    step(&mut p.h, "setprop", json!({"component": a03.hex(), "key": "plc.channels", "value": "8"}));
    assert!(checks_of(&mut p.h, "channel-over").is_empty());
}

/// M: `channel-twice` (a merge or a hand-made file draws channel 3 twice): an Error naming both
/// places; go-to on the second cell; moving one to a free channel clears it.
#[test]
#[ignore = "tester 045: awaiting build"]
fn m_channel_twice_from_a_hand_made_file() {
    let (mut p, c) = with_cards(0x7045_7e202);
    let a03 = c["-A03"];
    fence_inputs(&mut p, a03);
    let t4 = channel_points(&p.h.doc(), a03)[".4.in"].1;
    rekey_raw(&mut p.h, t4, None, Some(".3.in"));
    let mut h = reload(p.h, "channel twice", 0x7045_7e302);
    let rows = checks_of(&mut h, "channel-twice");
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(rows[0].to_string().contains("-A03 channel 3 is drawn twice"), "{}", rows[0]);
    assert_eq!(rows[0]["severity"], "error");
    rows_have_a_way_out(&mut h, "channel-twice");
    read_everything(&mut h);
}

/// M: `channel-type` is a known kind (constructing it needs a hand-made file whose ops the design
/// does not name; phase 2 adds a case if the build gives a simple one).
#[test]
#[ignore = "tester 045: awaiting build"]
fn m_channel_type_kind_is_known() {
    let (mut p, _) = with_cards(0x7045_7e203);
    assert!(p.h.run("checks", json!({"kinds": ["channel-type"]})).is_ok());
    assert!(checks_of(&mut p.h, "channel-type").is_empty());
}

// ------------------------------------------------------------------------------- L (AC 5): texts

/// L: `plc.text.address.show off` on the project hides every address; a card with its own `on`
/// keeps them; `plc.text.channel.rotation 0` lays descriptions flat; the caption pattern
/// `{rack} Slot{slot:02}` gives `-A01 Slot02`; `plc.text.caption.show off` on a card hides its
/// caption only.
#[test]
#[ignore = "tester 045: awaiting build"]
fn l_registry_switches_change_only_inheriting_cards() {
    let (mut p, c) = with_cards(0x7045_6e001);
    fence_inputs(&mut p, c["-A03"]);
    p.h.run("plcchannels", json!({"card": c["-A04"].hex(), "rest": true, "at": [300, 100]})).unwrap();
    p.h.run("plcchannel", json!({"card": c["-A03"].hex(), "channel": 0, "text": "Door closed"})).unwrap();
    let t = text_values(&mut p.h, p.inputs);
    assert!(t.iter().any(|x| x == "I0.0") && t.iter().any(|x| x == "I2.0"), "{t:?}");
    p.h.run("setdefault", json!({"key": "plc.text.address.show", "level": "object", "component": c["-A04"].hex(), "value": "on"})).unwrap();
    step(&mut p.h, "setdefault", json!({"key": "plc.text.address.show", "level": "project", "value": "off"}));
    let t = text_values(&mut p.h, p.inputs);
    assert!(!t.iter().any(|x| x == "I0.0"), "inheriting -A03 hides: {t:?}");
    assert!(t.iter().any(|x| x == "I2.0"), "-A04's own on stays: {t:?}");
    step(&mut p.h, "setdefault", json!({"key": "plc.text.channel.rotation", "level": "project", "value": "0"}));
    let row = text_row(&mut p.h, p.inputs, "Door closed").expect("description");
    assert!(!upright(&row), "flat at 0: {row}");
    step(&mut p.h, "setdefault", json!({"key": "plc.card.caption", "level": "project", "value": "{rack} Slot{slot:02}"}));
    let cap = text_values(&mut p.h, p.cpu_page);
    assert!(cap.iter().any(|x| x == "-A01 Slot02"), "{cap:?}");
    step(&mut p.h, "setdefault", json!({"key": "plc.text.caption.show", "level": "object", "component": c["-A03"].hex(), "value": "off"}));
    let cap = text_values(&mut p.h, p.cpu_page);
    assert!(!cap.iter().any(|x| x == "-A01 Slot02") && cap.iter().any(|x| x == "-A01 Slot03"), "{cap:?}");
    p.h.check();
}

/// L: `plc.channel.first 1` on a card shifts the shown channel number, never a terminal key.
#[test]
#[ignore = "tester 045: awaiting build"]
fn l_channel_first_changes_no_terminal() {
    let (mut p, c) = with_cards(0x7045_6e002);
    fence_inputs(&mut p, c["-A03"]);
    let before = channel_points(&p.h.doc(), c["-A03"]);
    p.h.run("setdefault", json!({"key": "plc.channel.first", "level": "object", "component": c["-A03"].hex(), "value": "1"})).unwrap();
    assert_eq!(channel_points(&p.h.doc(), c["-A03"]), before, "keys are 0-based for good");
    // The shown number at the named-channel prompt follows: `channel: 1` is index 0 (taken).
    refused(
        &mut p.h,
        "place",
        json!({"symbol": "chdi", "at": [300, 100], "device": c["-A03"].hex(), "channel": 1}),
        &["Channel 1 of -A03 is drawn on"],
    );
}

// ------------------------------------------------------------------------------- T (b)

/// T (b): hostile channel keys in a hand-made file, hostile counts and texts, a fence across 300
/// wires (more than 256 crossings): a refusal in words or a Checks row, never a panic.
#[test]
#[ignore = "tester 045: awaiting build"]
fn t_hostile_channel_keys_and_texts() {
    let (mut p, c) = with_cards(0x7045_ae201);
    let a03 = c["-A03"];
    fence_inputs(&mut p, a03);
    let pts = channel_points(&p.h.doc(), a03);
    for (i, k) in [".999.in", ".-1.in", ".3.nope", ".3", ".", ".99999999999999999999.in", ".1x.in"].iter().enumerate() {
        if let Some((_, t)) = pts.get(&format!(".{i}.in")) {
            rekey_raw(&mut p.h, *t, None, Some(k));
        }
    }
    raw_setprop(&mut p.h, a03, "plc.ch.0.text", Some(&"x".repeat(10_000)));
    raw_setprop(&mut p.h, a03, "plc.ch.99999999999999999999.text", Some("far"));
    raw_setprop(&mut p.h, a03, "plc.ch.-1.address", Some("I0.0"));
    let mut h = reload(p.h, "hostile b", 0x7045_ae301);
    read_everything(&mut h);
    let a03 = by_tag(&h.doc(), "-A03");
    for v in [json!(-1), json!(256), json!(1e300), json!("3"), json!(null)] {
        survives(&mut h, "plcchannel", json!({"card": a03.hex(), "channel": v, "text": "x"}));
    }
    survives(&mut h, "plcchannel", json!({"card": a03.hex(), "channel": 0, "text": "x".repeat(10_000)}));
    survives(&mut h, "plcchannel", json!({"card": a03.hex(), "channel": 0, "pins": "not an object"}));
    survives(&mut h, "plcchannels", json!({"card": a03.hex(), "from": [f64::MAX, 0], "to": [0, 0]}));
    survives(&mut h, "plcchannels", json!({"card": "zz", "rest": true}));
    survives(&mut h, "plcchannels", json!({"card": null, "type": "XX", "from": [0, 0], "to": [10, 0]}));
    h.check();
}

/// T (b): a fence across 300 wires is refused like 028's Place Terminals (at most 256 crossings)
/// and writes nothing.
#[test]
#[ignore = "tester 045: awaiting build"]
fn t_fence_across_300_wires_is_refused() {
    let mut p = fixture_p(0x7045_ae202);
    let page = p.h.doc().page_order[0];
    let wires: Vec<(f64, f64)> = (0..300).map(|i| (1000.0 + 2.0 * f64::from(i), 0.0)).collect();
    p.h.activate(page);
    for (x, _) in &wires {
        p.h.s.execute("wire", &json!({"points": [[x, 50.0], [x, -50.0]]})).unwrap();
    }
    let head = p.h.head();
    let r = p.h.s.execute("plcchannels", &json!({"card": null, "type": "DI", "from": [990, 0], "to": [1700, 0]}));
    assert!(r.is_err(), "more than 256 crossings is refused");
    assert_eq!(p.h.head(), head);
}
