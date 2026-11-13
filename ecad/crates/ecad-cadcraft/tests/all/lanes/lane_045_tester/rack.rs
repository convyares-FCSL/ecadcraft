//! Brief 045a (design 045 §2, §3, §9 a-rows, §12 a-entries, §13 A–E, M, T; FP1's four
//! conditions).

use cadcraft_engine::Input;
use ecad_model::{ComponentId, Op};
use serde_json::json;

use super::*;
use crate::common::id;

// ------------------------------------------------------------------------------- A (AC 1)

/// A: six card bodies by JSON form take `-A01` and slots 1…6 by default, one transaction each;
/// `plcracks` lists them in slot order with their types; the card keys hold the rack's ID.
#[test]
fn a_six_cards_take_the_rack_and_the_next_free_slot() {
    let mut p = fixture_p(0x7045_a001);
    p.h.activate(p.cpu_page);
    let rack_hex = p.rack.hex();
    for (i, (sym, t, slot)) in CARDS.iter().enumerate() {
        let v = step(&mut p.h, "place", json!({"symbol": sym, "at": [140.0 + 40.0 * i as f64, 200]}));
        let c: ComponentId = id(&v, "component");
        let d = p.h.doc();
        assert_eq!(tag(&d, c), *t);
        assert_eq!(prop(&d, c, "plc.rack").as_deref(), Some(rack_hex.as_str()), "FP1: the rack's 32-hex ID");
        assert_eq!(prop(&d, c, "plc.slot"), Some(slot.to_string()), "{t}");
        let e = echo(&p.h, 3);
        assert!(e.contains(&format!("in -A01 slot {slot}")), "echo names the rack by tag: {e}");
        assert!(!e.contains(&rack_hex), "echo never shows the hex ID: {e}");
    }
    let rows = slots(&mut p.h, "-A01");
    let got: Vec<(u64, String)> = rows.iter().filter(|r| !r["tag"].is_null()).map(|r| (num(&r["slot"]).unwrap_or(0), s(&r["tag"]))).collect();
    assert_eq!(got, [(1, "-A02".into()), (2, "-A03".into()), (3, "-A04".into()), (4, "-A05".into()), (5, "-A06".into()), (6, "-A07".into())]);
    let types: Vec<String> = rows.iter().filter(|r| !r["tag"].is_null()).map(|r| s(&r["type"]).to_uppercase()).collect();
    assert_eq!(types, ["CM", "DI", "DI", "AI", "IOL", "DQ"]);
    let ch: Vec<Option<u64>> = rows.iter().filter(|r| !r["tag"].is_null()).map(|r| num(&r["channels"])).collect();
    assert_eq!(ch, [Some(0), Some(16), Some(16), Some(4), Some(4), Some(16)], "channel counts from the registry (§2.3)");
    p.h.check();
}

/// A: placing a `plccpu` or `plcim` makes a rack, no prompt; `plcim`, `plcpm`, `plccm` exist as
/// starters (design §3.1).
#[test]
fn a_new_bodies_exist_and_a_head_module_is_a_rack() {
    let mut h = H::new(0x7045_a002);
    for key in ["plcim", "plcpm", "plccm"] {
        let v = h.run("symbols", json!({"query": key})).unwrap();
        assert!(v["symbols"].as_array().is_some_and(|a| a.iter().any(|x| x["key"] == key)), "{key} is a starter: {v}");
    }
    let im = h.run("place", json!({"symbol": "plcim", "at": [100, 200]})).unwrap();
    let v = h.run("plcracks", json!({})).unwrap();
    let racks = v["racks"].as_array().cloned().unwrap_or_default();
    assert_eq!(racks.len(), 1, "{v}");
    assert_eq!(racks[0]["tag"], im["tag"], "{v}");
    h.check();
}

/// A (interactive, design §2.4 and Q17): typed at the command line, Place Symbol of a card body
/// asks Rack (default the only rack) and Slot (default the next free); Enter takes both; Esc writes
/// nothing; `None` places it out of any rack.
#[test]
fn a_place_prompts_rack_then_slot() {
    let mut p = fixture_p(0x7045_a003);
    p.h.activate(p.cpu_page);
    let prompt = |h: &H| h.s.current_prompt().map(|p| p.display()).unwrap_or_default();
    let head = p.h.head();
    p.h.s.cmdline("place").unwrap();
    p.h.s.cmdline("plcdi").unwrap();
    let q = prompt(&p.h);
    assert!(q.starts_with("Rack") && q.contains("None") && q.contains("<-A01>"), "{q}");
    p.h.s.input(Input::Cancel).unwrap();
    assert!(p.h.s.running.is_none());
    assert_eq!(p.h.head(), head, "Esc writes nothing");

    p.h.s.cmdline("place").unwrap();
    p.h.s.cmdline("plcdi").unwrap();
    p.h.s.input(Input::Enter).unwrap();
    let q = prompt(&p.h);
    assert!(q.starts_with("Slot") && q.contains("<1>"), "{q}");
    p.h.s.input(Input::Enter).unwrap();
    p.h.s.cmdline("140,200").unwrap();
    assert_eq!(p.h.head(), head + 1, "one transaction");
    let d = p.h.doc();
    let c = by_tag(&d, "-A02");
    assert_eq!(prop(&d, c, "plc.slot").as_deref(), Some("1"));
    let _ = p.h.s.input(Input::Cancel);
    // Next card: the slot default moves on.
    p.h.s.cmdline("place").unwrap();
    p.h.s.cmdline("plcdq").unwrap();
    p.h.s.input(Input::Enter).unwrap();
    assert!(prompt(&p.h).contains("<2>"), "{}", prompt(&p.h));
    p.h.s.input(Input::Cancel).unwrap();
    // `None`: out of any rack, no Slot prompt.
    p.h.s.cmdline("place").unwrap();
    p.h.s.cmdline("plcdq").unwrap();
    p.h.s.cmdline("None").unwrap();
    assert!(!prompt(&p.h).starts_with("Slot"), "{}", prompt(&p.h));
    p.h.s.cmdline("220,200").unwrap();
    let _ = p.h.s.input(Input::Cancel);
    let d = p.h.doc();
    let c = by_tag(&d, "-A03");
    assert_eq!((prop(&d, c, "plc.rack"), prop(&d, c, "plc.slot")), (None, None));
    p.h.check();
}

/// A: placing a card body when the project has no rack asks nothing and leaves it in no rack.
#[test]
fn a_no_rack_no_prompt() {
    let mut h = H::new(0x7045_a004);
    let v = step(&mut h, "place", json!({"symbol": "plcdi", "at": [100, 200]}));
    let c: ComponentId = id(&v, "component");
    let d = h.doc();
    assert_eq!((prop(&d, c, "plc.rack"), prop(&d, c, "plc.slot")), (None, None));
    let v = h.run("plcracks", json!({})).unwrap();
    assert!(v["unracked"].to_string().contains("-A01"), "cards in no rack in a last group: {v}");
}

// ------------------------------------------------------------------------------- B (AC 2)

/// B: a taken slot is refused naming the holder and the next free slot; `swap: true` swaps the two
/// in one transaction; `rack: null` takes a card out; `u` undoes each.
#[test]
fn b_plcslot_refuses_swaps_and_takes_out() {
    let (mut p, c) = with_cards(0x7045_b001);
    let (a03, a04) = (c["-A03"], c["-A04"]);
    let e = refused(
        &mut p.h,
        "plcslot",
        json!({"card": a04.hex(), "slot": 2}),
        &["Slot 2 of -A01 holds -A03", "Pick another slot, or Enter for the next free one (7)."],
    );
    assert!(!e.contains(&p.rack.hex()), "{e}");
    step(&mut p.h, "plcslot", json!({"card": a04.hex(), "slot": 2, "swap": true}));
    let d = p.h.doc();
    assert_eq!((prop(&d, a04, "plc.slot").as_deref(), prop(&d, a03, "plc.slot").as_deref()), (Some("2"), Some("3")), "swapped");
    // Out of the rack.
    step(&mut p.h, "plcslot", json!({"card": a03.hex(), "rack": null}));
    let d = p.h.doc();
    assert_eq!((prop(&d, a03, "plc.rack"), prop(&d, a03, "plc.slot")), (None, None));
    // Back in, named slot.
    step(&mut p.h, "plcslot", json!({"card": a03.hex(), "rack": p.rack.hex(), "slot": 12}));
    assert_eq!(prop(&p.h.doc(), a03, "plc.slot").as_deref(), Some("12"));
    p.h.check();
}

/// B (design §2.5: "`slot` absent: the next free slot"): `plcslot {card}` on a card already in a
/// slot moves it to the next free slot (7 in fixture P after the swap).
#[test]
fn b_plcslot_without_slot_takes_the_next_free_one() {
    let (mut p, c) = with_cards(0x7045_b004);
    let a03 = c["-A03"];
    let v = p.h.run("plcslot", json!({"card": a03.hex()})).unwrap();
    assert_eq!(prop(&p.h.doc(), a03, "plc.slot").as_deref(), Some("7"), "{v}");
}

/// B: with two racks, `plcslot` without `rack` on a card in no rack is refused naming both.
#[test]
fn b_two_racks_need_a_name() {
    let mut p = fixture_p(0x7045_b002);
    p.h.activate(p.cpu_page);
    p.h.run("place", json!({"symbol": "plcim", "at": [400, 200], "tag": "-A20"})).unwrap();
    let v = place_card(&mut p.h, "plcdi", 140.0, json!({"rack": null}));
    let card: ComponentId = id(&v, "component");
    refused(&mut p.h, "plcslot", json!({"card": card.hex()}), &["Name a rack: the project has 2 (-A01, -A20)."]);
}

/// B: the echo of a move counts the cards whose automatic addresses change (design §2.5).
#[test]
fn b_plcslot_echo_names_tags_and_counts() {
    let (mut p, c) = with_cards(0x7045_b003);
    p.h.run("plcslot", json!({"card": c["-A07"].hex(), "slot": 9})).unwrap();
    let e = echo(&p.h, 3);
    assert!(e.contains("-A07: -A01 slot 6 \u{2192}") && e.contains("slot 9"), "{e}");
    // -A07 is the last card: no automatic address moves, so no count (045b's J checks the count).
    assert!(!e.contains(&p.rack.hex()), "{e}");
}

// ------------------------------------------------------------------------------- C, E, M (AC 3, 5)

/// C: `slot-range`: a card in slot 40 of a 32-slot rack; **Raise -A01 to 40 slots** (object-level
/// `plc.slots`) clears it; **Move to slot 5** (`plcslot`) clears it too.
#[test]
fn c_slot_range_row_and_both_fixes() {
    let (mut p, c) = with_cards(0x7045_c001);
    p.h.run("setprop", json!({"component": c["-A04"].hex(), "key": "plc.slot", "value": "40"})).unwrap();
    let rows = checks_of(&mut p.h, "slot-range");
    assert_eq!(rows.len(), 1, "{rows:?}");
    let t = rows[0].to_string();
    assert!(t.contains("-A04 is in slot 40; -A01 has slots 1\u{2013}32") || t.contains("-A04 is in slot 40"), "{t}");
    assert!(!t.contains(&p.rack.hex()), "{t}");
    assert_eq!(rows[0]["severity"], "warning");
    rows_have_a_way_out(&mut p.h, "slot-range");
    step(&mut p.h, "setdefault", json!({"key": "plc.slots", "level": "object", "component": p.rack.hex(), "value": 40}));
    assert!(checks_of(&mut p.h, "slot-range").is_empty());
    p.h.cmdline("u");
    assert_eq!(checks_of(&mut p.h, "slot-range").len(), 1);
    step(&mut p.h, "plcslot", json!({"card": c["-A04"].hex(), "slot": 7}));
    assert!(checks_of(&mut p.h, "slot-range").is_empty());
}

/// C / FP1 (3): a `plc.rack` that is random hex, a deleted rack's ID or a DI card's ID is a
/// `rack-missing` row (never a refusal on open, never a crash); **Put in rack…** and **Take out of
/// rack** both clear it.
#[test]
fn c_rack_missing_from_a_hand_made_file() {
    let (mut p, c) = with_cards(0x7045_c002);
    raw_setprop(&mut p.h, c["-A03"], "plc.rack", Some("0123456789abcdef0123456789abcdef"));
    raw_setprop(&mut p.h, c["-A04"], "plc.rack", Some(&c["-A05"].hex())); // a card, not a rack
    raw_setprop(&mut p.h, c["-A05"], "plc.rack", Some("not hex at all"));
    let mut h = reload(p.h, "rack missing", 0x7045_c102);
    let rows = checks_of(&mut h, "rack-missing");
    assert_eq!(rows.len(), 3, "{rows:?}");
    assert!(rows.iter().any(|r| r.to_string().contains("-A03 names a rack that is not in the project")), "{rows:?}");
    rows_have_a_way_out(&mut h, "rack-missing");
    read_everything(&mut h);
    let d = h.doc();
    let rack = by_tag(&d, "-A01");
    let (a03, a04) = (by_tag(&d, "-A03"), by_tag(&d, "-A04"));
    step(&mut h, "plcslot", json!({"card": a03.hex(), "rack": rack.hex(), "slot": 20}));
    step(&mut h, "plcslot", json!({"card": a04.hex(), "rack": null}));
    let d = h.doc();
    assert_eq!((prop(&d, a04, "plc.rack"), prop(&d, a04, "plc.slot")), (None, None));
    assert_eq!(checks_of(&mut h, "rack-missing").len(), 1, "only -A05 left");
    h.check();
}

/// M: `slot-twice` (two cards in one slot, as a merge leaves it): an Error with the second card's
/// go-to; **Move -A07 to slot n** (next free) clears it.
#[test]
fn m_slot_twice_row_and_fix() {
    let (mut p, c) = with_cards(0x7045_7e001);
    raw_setprop(&mut p.h, c["-A07"], "plc.slot", Some("3"));
    let mut h = reload(p.h, "slot twice", 0x7045_7e101);
    let rows = checks_of(&mut h, "slot-twice");
    assert_eq!(rows.len(), 1, "{rows:?}");
    let t = rows[0].to_string();
    assert!(t.contains("-A01 slot 3 holds -A04 and -A07"), "{t}");
    assert_eq!(rows[0]["severity"], "error");
    rows_have_a_way_out(&mut h, "slot-twice");
    let a07 = by_tag(&h.doc(), "-A07");
    step(&mut h, "plcslot", json!({"card": a07.hex()}));
    assert!(checks_of(&mut h, "slot-twice").is_empty());
}

/// E (AC 5): 034-era card bodies (placed with no rack keys) open as cards in no rack: one `unracked`
/// Info count row; **Put in rack…** (`plcslot`) works.
#[test]
fn e_034_bodies_are_cards_in_no_rack() {
    let (mut p, c) = with_cards(0x7045_e001);
    for t in ["-A03", "-A04", "-A05"] {
        raw_setprop(&mut p.h, c[t], "plc.rack", None);
        raw_setprop(&mut p.h, c[t], "plc.slot", None);
    }
    let mut h = reload(p.h, "034 era", 0x7045_e101);
    let rows = checks_of(&mut h, "unracked");
    assert_eq!(rows.len(), 1, "one count row: {rows:?}");
    assert!(rows[0].to_string().contains("3 PLC cards are in no rack"), "{}", rows[0]);
    assert_eq!(rows[0]["severity"], "info");
    let v = h.run("plcracks", json!({})).unwrap();
    assert!(v["unracked"].as_array().is_some_and(|a| a.len() == 3), "{v}");
    let d = h.doc();
    step(&mut h, "plcslot", json!({"card": by_tag(&d, "-A03").hex(), "rack": by_tag(&d, "-A01").hex()}));
    assert!(checks_of(&mut h, "unracked")[0].to_string().contains("2 PLC cards"));
}

// ------------------------------------------------------------------------------- D, FP1 (AC 4, 6, 10)

/// D / FP1 (4): save, reopen, compare: racks, slots, channel counts and `plc.rack` survive exactly;
/// retagging the rack changes no card property and every caption follows.
#[test]
fn d_round_trip_and_retag() {
    let (mut p, c) = with_cards(0x7045_d001);
    p.h.run("setprop", json!({"component": c["-A04"].hex(), "key": "plc.channels", "value": "32"})).unwrap();
    let before = p.h.doc();
    let rows_before = slots(&mut p.h, "-A01");
    let cpu_page = p.cpu_page;
    let mut h = reload(p.h, "round trip", 0x7045_d101);
    let after = h.doc();
    assert_eq!(after.components, before.components, "every property, plc.rack included, round-trips");
    assert_eq!(slots(&mut h, "-A01"), rows_before);
    let rack = by_tag(&after, "-A01");
    let props_before: Vec<_> = c.values().map(|x| after.components[x].properties.clone()).collect();
    h.activate(cpu_page);
    let caps: Vec<String> = text_values(&mut h, cpu_page).into_iter().filter(|t| t.starts_with("-A01 slot ")).collect();
    assert_eq!(caps.len(), 6, "a caption per card in a rack (§7.2): {caps:?}");
    step(&mut h, "setprop", json!({"component": rack.hex(), "key": "tag", "value": "-A10"}));
    let d = h.doc();
    let props_after: Vec<_> = c.values().map(|x| d.components[x].properties.clone()).collect();
    assert_eq!(props_before, props_after, "retagging the rack touches no card");
    assert_eq!(slots(&mut h, "-A10").len(), rows_before.len());
    let texts = text_values(&mut h, cpu_page);
    assert!(texts.iter().any(|t| t == "-A10 slot 2"), "{texts:?}");
    assert!(!texts.iter().any(|t| t.starts_with("-A01 slot")), "{texts:?}");
    assert!(!texts.iter().any(|t| t.contains(&rack.hex())), "{texts:?}");
    h.check();
}

/// FP1 (2) / AC 6: deleting a rack clears `plc.rack` and `plc.slot` on its cards in the same
/// transaction, says so, and `u` restores both.
#[test]
fn fp1_deleting_a_rack_takes_its_cards_out_in_one_transaction() {
    let (mut p, c) = with_cards(0x7045_f001);
    let before = p.h.doc();
    let head = p.h.head();
    p.h.run("devicedelete", json!({"component": p.rack.hex()})).unwrap();
    assert_eq!(p.h.head(), head + 1, "one transaction");
    let d = p.h.doc();
    assert!(!d.components.contains_key(&p.rack));
    for x in c.values() {
        assert!(d.components.contains_key(x), "cards stay");
        assert_eq!((prop(&d, *x, "plc.rack"), prop(&d, *x, "plc.slot")), (None, None), "{}", tag(&d, *x));
    }
    let ops = p.h.p(|p| p.ledger.log().last().map(|t| t.ops.clone())).unwrap_or_default();
    let cleared = ops.iter().filter(|o| matches!(o, Op::SetProperty { key, after: None, .. } if key == "plc.rack")).count();
    assert_eq!(cleared, 6, "the same transaction clears each card's key");
    let e = echo(&p.h, 3);
    assert!(e.contains("Deleted -A01; 6 cards are now in no rack."), "{e}");
    assert!(checks_of(&mut p.h, "rack-missing").is_empty(), "no dangling ID is left behind");
    p.h.cmdline("u");
    assert_eq!(model_parts(&p.h.doc()), model_parts(&before), "u restores the rack and the keys");
    p.h.check();
}

/// FP1 (2): erasing the rack's last symbol (018's delete) also clears the keys when the device goes.
#[test]
fn fp1_erasing_the_rack_body_never_leaves_a_dangling_id() {
    let (mut p, c) = with_cards(0x7045_f002);
    let item = p.h.symbol_item(p.rack);
    let handle = p.h.handle_of(p.cpu_page, item);
    let head = p.h.head();
    p.h.run("erase", json!({"handles": [handle.hex()]})).unwrap();
    assert_eq!(p.h.head(), head + 1);
    let d = p.h.doc();
    if !d.components.contains_key(&p.rack) {
        for x in c.values() {
            assert_eq!(prop(&d, *x, "plc.rack"), None, "{}", tag(&d, *x));
        }
    }
    assert!(checks_of(&mut p.h, "rack-missing").is_empty());
}

/// FP1 (1), inspector: Properties on a card body shows the rack's tag, never the hex ID; on a
/// rack body the Rack section's rows.
#[test]
fn fp1_inspector_shows_the_rack_tag() {
    let (mut p, c) = with_cards(0x7045_f003);
    let hex = p.rack.hex();
    let item = p.h.symbol_item(c["-A04"]);
    let handle = p.h.handle_of(p.cpu_page, item);
    let page = p.cpu_page;
    let view = p.h.p(|pr| ecad_cadcraft::props::view(pr, Some(page), &[handle]));
    let rows: Vec<_> = view.sections.iter().flat_map(|s| s.rows.iter().map(move |r| (s.title.clone(), r.clone()))).collect();
    for (_, r) in &rows {
        assert!(!r.value.to_lowercase().contains(&hex.to_lowercase()), "the inspector shows hex: {r:?}");
        assert!(r.choices.iter().all(|c| !c.to_lowercase().contains(&hex.to_lowercase())), "choices show hex: {r:?}");
    }
    // As built the Card rows join the device's section (design §2.6 names a "Card" section).
    let card: Vec<_> = rows.iter().filter(|(_, r)| r.key.starts_with("plc.")).map(|(_, r)| r).collect();
    let rack_row = card.iter().find(|r| r.label == "Rack").unwrap_or_else(|| panic!("Properties: Rack row: {rows:?}"));
    assert_eq!(rack_row.value, "-A01");
    assert!(rack_row.choices.iter().any(|c| c == "None") && rack_row.choices.iter().any(|c| c == "-A01"), "{rack_row:?}");
    assert!(card.iter().any(|r| r.label == "Slot" && r.value == "3"), "{card:?}");
    assert!(card.iter().any(|r| r.label == "Channels"), "{card:?}");
    // The rack body: Properties ▸ Rack.
    let item = p.h.symbol_item(p.rack);
    let handle = p.h.handle_of(page, item);
    let view = p.h.p(|pr| ecad_cadcraft::props::view(pr, Some(page), &[handle]));
    let rack: Vec<_> = view.sections.iter().flat_map(|s| s.rows.iter()).collect();
    for l in ["Slots", "First slot"] {
        assert!(rack.iter().any(|r| r.label == l), "{l}: {rack:?}");
    }
}

/// FP1 (1), the history: each `plcslot` transaction's label never holds the hex ID.
#[test]
fn fp1_history_labels_name_the_tag() {
    let (mut p, c) = with_cards(0x7045_f004);
    p.h.run("plcslot", json!({"card": c["-A04"].hex(), "slot": 9})).unwrap();
    p.h.run("plcslot", json!({"card": c["-A05"].hex(), "rack": null})).unwrap();
    let hex = p.rack.hex();
    let labels: Vec<String> = p.h.p(|p| p.ledger.log().iter().map(|t| t.label.clone()).collect());
    for l in &labels {
        assert!(!l.contains(&hex), "{l}");
    }
}

// ------------------------------------------------------------------------------- registry (§12 a)

/// §12: `defaults {group: "PLC"}` lists the a-entries; `plc.slots` and `plc.slot.first` at rack
/// level change `plcracks` and the next free slot; a `plc.di.channels` project value changes the
/// count of cards with no own value only.
#[test]
fn registry_a_entries() {
    let (mut p, c) = with_cards(0x7045_8e001);
    let d = p.h.run("defaults", json!({"group": "PLC"})).unwrap();
    let keys: Vec<String> = d["defaults"].as_array().map(|a| a.iter().map(|r| s(&r["key"])).collect()).unwrap_or_default();
    for k in [
        "plc.slots",
        "plc.slot.first",
        "plc.di.channels",
        "plc.dq.channels",
        "plc.ai.channels",
        "plc.ao.channels",
        "plc.rtd.channels",
        "plc.iol.channels",
        "plc.card.caption",
        "plc.text.caption.show",
    ] {
        assert!(keys.iter().any(|x| x == k), "{k} missing from {keys:?}");
    }
    p.h.run("setprop", json!({"component": c["-A04"].hex(), "key": "plc.channels", "value": "8"})).unwrap();
    step(&mut p.h, "setdefault", json!({"key": "plc.di.channels", "level": "project", "value": 32}));
    assert_eq!(slot_row(&mut p.h, "-A03").and_then(|r| num(&r["channels"])), Some(32));
    assert_eq!(slot_row(&mut p.h, "-A04").and_then(|r| num(&r["channels"])), Some(8), "own value kept");
    step(&mut p.h, "setdefault", json!({"key": "plc.slot.first", "level": "object", "component": p.rack.hex(), "value": "0"}));
    let v = place_card(&mut p.h, "plcdq", 500.0, json!({}));
    assert_eq!(prop(&p.h.doc(), id(&v, "component"), "plc.slot").as_deref(), Some("0"), "slot 0 is free once slots start at 0");
}

// ------------------------------------------------------------------------------- T (a)

/// T (a): hostile slots, counts, racks and captions: a refusal in words or a Checks row, never a
/// panic; every read still works.
#[test]
fn t_hostile_slots_racks_and_captions() {
    let (mut p, c) = with_cards(0x7045_ae001);
    let a03 = c["-A03"].hex();
    for v in ["-1", "1000", "99999999999999999999", "", " 3", "3.5", "NaN", "\u{0663}"] {
        survives(&mut p.h, "setprop", json!({"component": a03, "key": "plc.slot", "value": v}));
        survives(&mut p.h, "plcslot", json!({"card": a03, "slot": v}));
    }
    for v in [json!(-1), json!(1000), json!(1e300), json!(u64::MAX), json!("x"), json!([1])] {
        survives(&mut p.h, "plcslot", json!({"card": a03, "slot": v}));
    }
    for v in ["257", "-1", "99999999999999999999", "1e9"] {
        survives(&mut p.h, "setprop", json!({"component": a03, "key": "plc.channels", "value": v}));
    }
    for v in [json!("{rack}{nope}"), json!("x".repeat(10_000)), json!("{slot:99}"), json!("{")] {
        survives(&mut p.h, "setdefault", json!({"key": "plc.card.caption", "level": "project", "value": v}));
    }
    for v in [json!(0), json!(1000), json!(-5), json!("many")] {
        survives(&mut p.h, "setdefault", json!({"key": "plc.slots", "level": "object", "component": p.rack.hex(), "value": v}));
    }
    // A card naming itself, and a rack naming a rack.
    raw_setprop(&mut p.h, c["-A03"], "plc.rack", Some(&c["-A03"].hex()));
    raw_setprop(&mut p.h, p.rack, "plc.rack", Some(&p.rack.hex()));
    raw_setprop(&mut p.h, c["-A04"], "plc.slot", Some("99999999999999999999"));
    raw_setprop(&mut p.h, c["-A05"], "plc.channels", Some("4294967296"));
    let mut h = reload(p.h, "hostile a", 0x7045_ae101);
    read_everything(&mut h);
    survives(&mut h, "plcslot", json!({"card": "zz", "slot": 1}));
    survives(&mut h, "plcslot", json!({"card": null}));
    survives(&mut h, "plcracks", json!({"rack": "0123"}));
    h.check();
}

/// Swap, take-out and delete all keep undo/redo/replay exact (the harness checks replay and the
/// projection after every step).
#[test]
fn a_undo_redo_replay_across_rack_edits() {
    let (mut p, c) = with_cards(0x7045_be001);
    let start = p.h.doc();
    p.h.run("plcslot", json!({"card": c["-A03"].hex(), "slot": 4, "swap": true})).unwrap();
    p.h.run("plcslot", json!({"card": c["-A06"].hex(), "rack": null})).unwrap();
    p.h.run("devicedelete", json!({"component": p.rack.hex()})).unwrap();
    for _ in 0..3 {
        p.h.cmdline("u");
    }
    assert_eq!(model_parts(&p.h.doc()), model_parts(&start));
    p.h.check();
}
