//! Brief 045b, addressing (design 045 §5, §9 b-rows `address-*`, `no-address`, §13 I, J, K, M, T).
//!
//! Addresses are read where a person reads them in 045b: the `Address` text on each drawn cell
//! (`pagetexts`), and `plcracks`' per-card `inputs`/`outputs` (an IO-Link port's cell shows its
//! input address only, §5.2). The I/O list (045c) is not used here.

use std::collections::BTreeSet;

use ecad_model::ComponentId;
use serde_json::{Value, json};

use super::*;

/// Fixture P with check F's fence on `-A03` and every channel of every card drawn (rest), so each
/// card's addresses are on a page.
fn all_drawn(seed: u64) -> (P, BTreeMap<&'static str, ComponentId>) {
    let (mut p, c) = with_cards(seed);
    fence_inputs(&mut p, c["-A03"]);
    p.h.run("plcchannels", json!({"card": c["-A03"].hex(), "rest": true})).unwrap();
    p.h.run("plcchannels", json!({"card": c["-A04"].hex(), "rest": true, "at": [100, 50]})).unwrap();
    p.h.activate(p.analogue);
    p.h.run("plcchannels", json!({"card": c["-A05"].hex(), "rest": true, "at": [100, 200]})).unwrap();
    p.h.activate(p.iolink);
    p.h.run("plcchannels", json!({"card": c["-A06"].hex(), "rest": true, "at": [100, 200]})).unwrap();
    p.h.activate(p.outputs);
    p.h.run("plcchannels", json!({"card": c["-A07"].hex(), "rest": true, "at": [100, 50]})).unwrap();
    (p, c)
}

/// Every shown text on every page.
fn all_texts(p: &mut P) -> BTreeSet<String> {
    let pages = p.h.doc().page_order.clone();
    pages.into_iter().flat_map(|pg| text_values(&mut p.h, pg)).collect()
}

fn has_all(t: &BTreeSet<String>, want: &[String], what: &str) {
    let missing: Vec<&String> = want.iter().filter(|w| !t.contains(*w)).collect();
    assert!(missing.is_empty(), "{what}: missing {missing:?}");
}

fn digital(prefix: &str, bytes: std::ops::RangeInclusive<u32>) -> Vec<String> {
    bytes.flat_map(|b| (0..8).map(move |i| format!("{prefix}{b}.{i}"))).collect()
}

fn words(prefix: &str, at: &[u32]) -> Vec<String> {
    at.iter().map(|b| format!("{prefix}{b}")).collect()
}

/// The first whole number in `v` (`QW0` → 0, `Q8.0` → 8, `%QX8.0` → 8, `8` → 8).
fn first_number(v: &Value) -> Option<u64> {
    let t = s(v);
    let d: String = t.chars().skip_while(|c| !c.is_ascii_digit()).take_while(char::is_ascii_digit).collect();
    d.parse().ok()
}

/// The byte where a card's inputs or outputs start, per `plcracks` (design §2.5).
fn block_start(p: &mut P, card: &str, dir: &str) -> Option<u64> {
    let r = slot_row(&mut p.h, card)?;
    first_number(&r[dir][0])
}

// ------------------------------------------------------------------------------- I (AC 3)

/// I: `byte.bit` (built-in): `-A03` I0.0–I1.7, `-A04` I2.0–I3.7, `-A05` IW4…IW10, `-A06` inputs
/// IW12…IW18 and outputs from QW0, `-A07` Q8.0–Q9.7; the CM takes no address.
#[test]
#[ignore = "tester 045: awaiting build"]
fn i_byte_bit_addresses() {
    let (mut p, _) = all_drawn(0x7045_i001);
    let t = all_texts(&mut p);
    has_all(&t, &digital("I", 0..=1), "-A03");
    has_all(&t, &digital("I", 2..=3), "-A04");
    has_all(&t, &words("IW", &[4, 6, 8, 10]), "-A05");
    has_all(&t, &words("IW", &[12, 14, 16, 18]), "-A06 inputs");
    has_all(&t, &digital("Q", 8..=9), "-A07");
    assert!(!t.contains("IW20") && !t.contains("Q10.0") && !t.contains("I4.0"), "nothing beyond the blocks");
    assert_eq!(block_start(&mut p, "-A06", "outputs"), Some(0), "IO-Link outputs QW0 (§5.1 IOL in and out)");
    assert_eq!(block_start(&mut p, "-A07", "outputs"), Some(8));
    assert_eq!(block_start(&mut p, "-A05", "inputs"), Some(4));
    p.h.check();
}

/// I: `iec` and `slot` styles, then `none`, each one transaction by `plcaddress {rack, style}`;
/// every drawn address follows (live, never stored).
#[test]
#[ignore = "tester 045: awaiting build"]
fn i_each_address_style() {
    let (mut p, c) = all_drawn(0x7045_i002);
    let rack = p.rack.hex();
    step(&mut p.h, "plcaddress", json!({"rack": rack, "style": "iec"}));
    let t = all_texts(&mut p);
    has_all(&t, &digital("%IX", 0..=3), "iec digital inputs");
    has_all(&t, &words("%IW", &[4, 6, 8, 10, 12, 14, 16, 18]), "iec words");
    has_all(&t, &digital("%QX", 8..=9), "iec digital outputs");
    assert!(!t.contains("I0.0"), "byte.bit gone");
    step(&mut p.h, "plcaddress", json!({"rack": rack, "style": "slot"}));
    let t = all_texts(&mut p);
    let a03: Vec<String> = (0..16).map(|n| format!("I:2/{n:02}")).collect();
    let a05: Vec<String> = (0..4).map(|n| format!("I:4/{n:02}")).collect();
    let a07: Vec<String> = (0..16).map(|n| format!("O:6/{n:02}")).collect();
    has_all(&t, &a03, "slot -A03");
    has_all(&t, &a05, "slot -A05 (analogue: the same)");
    has_all(&t, &a07, "slot -A07");
    assert!(t.contains("I:5/00"), "slot -A06");
    // `n` is the channel number shown: plc.channel.first 1 on -A03.
    p.h.run("setdefault", json!({"key": "plc.channel.first", "level": "object", "component": c["-A03"].hex(), "value": "1"})).unwrap();
    let t = all_texts(&mut p);
    assert!(t.contains("I:2/16") && !t.contains("I:2/00"), "slot numbers follow the shown channel number");
    step(&mut p.h, "plcaddress", json!({"rack": rack, "style": "none"}));
    let t = all_texts(&mut p);
    assert!(!t.iter().any(|x| x.starts_with("I:") || x.starts_with("%I") || x == "I0.0"), "style none: typed addresses only");
    // The style is the rack's object-level registry value (setdefault), shown with its source.
    let d = p.h.run("defaults", json!({"key": "plc.address.style", "component": rack})).unwrap();
    assert!(d.to_string().contains("none"), "{d}");
    p.h.check();
}

/// I: the rack's input and output starts move every block (`plcaddress {rack, input_start,
/// output_start}`).
#[test]
#[ignore = "tester 045: awaiting build"]
fn i_rack_starts() {
    let (mut p, _) = all_drawn(0x7045_i003);
    step(&mut p.h, "plcaddress", json!({"rack": p.rack.hex(), "input_start": 100, "output_start": 200}));
    let t = all_texts(&mut p);
    has_all(&t, &digital("I", 100..=101), "-A03 from 100");
    has_all(&t, &words("IW", &[104]), "-A05 from 104");
    has_all(&t, &digital("Q", 208..=209), "-A07 after the IOL outputs from 200");
}

/// I: a card's size override (`plcaddress {card, size}`) moves the cards after it.
#[test]
#[ignore = "tester 045: awaiting build"]
fn i_size_override() {
    let (mut p, c) = all_drawn(0x7045_i004);
    step(&mut p.h, "plcaddress", json!({"card": c["-A03"].hex(), "size": 4}));
    let t = all_texts(&mut p);
    has_all(&t, &digital("I", 4..=5), "-A04 after a 4-byte -A03");
    assert_eq!(prop(&p.h.doc(), c["-A03"], "plc.addr.size").as_deref(), Some("4"), "FP3");
}

// ------------------------------------------------------------------------------- J (AC 3)

/// J: moving `-A03` to slot 9 shifts every later input card and the echo counts the 4 cards whose
/// addresses change; after `plcaddress fix` the same move changes no address; `clear` makes them
/// automatic again.
#[test]
#[ignore = "tester 045: awaiting build"]
fn j_live_addresses_fix_and_clear() {
    let (mut p, c) = all_drawn(0x7045_j001);
    let rack = p.rack.hex();
    let before = all_texts(&mut p);
    p.h.run("plcslot", json!({"card": c["-A03"].hex(), "slot": 9})).unwrap();
    let e = echo(&p.h, 3);
    assert!(e.contains("addresses of 4 cards change"), "-A03, -A04, -A05, -A06 move; -A07's outputs do not: {e}");
    let t = all_texts(&mut p);
    has_all(&t, &digital("I", 0..=1), "-A04 now first");
    has_all(&t, &digital("I", 18..=19), "-A03 after -A06 (bytes 2–17)");
    has_all(&t, &digital("Q", 8..=9), "-A07 unchanged");
    p.h.cmdline("u");
    assert_eq!(all_texts(&mut p), before, "u puts every address back");
    // Fix: one transaction, a pinned start on each card.
    step(&mut p.h, "plcaddress", json!({"rack": rack, "fix": true}));
    let e = echo(&p.h, 3);
    assert!(e.contains("cards pinned at their current addresses"), "{e}");
    let d = p.h.doc();
    assert_eq!(prop(&d, c["-A04"], "plc.addr.start").as_deref(), Some("2"));
    assert_eq!(prop(&d, c["-A05"], "plc.addr.start").as_deref(), Some("4"));
    p.h.run("plcslot", json!({"card": c["-A03"].hex(), "slot": 9})).unwrap();
    assert_eq!(all_texts(&mut p), before, "pinned: a slot move changes no address");
    // Clear: automatic again, -A03 now after -A06.
    step(&mut p.h, "plcaddress", json!({"rack": rack, "clear": true}));
    let d = p.h.doc();
    assert!(c.values().all(|x| prop(&d, *x, "plc.addr.start").is_none()), "every pin removed");
    has_all(&all_texts(&mut p), &digital("I", 18..=19), "automatic again");
    p.h.check();
}

/// §5.1: a pinned start keeps a card's addresses and the cards after it follow it; `null` unpins.
#[test]
#[ignore = "tester 045: awaiting build"]
fn j_pinned_start_on_one_card() {
    let (mut p, c) = all_drawn(0x7045_j002);
    step(&mut p.h, "plcaddress", json!({"card": c["-A05"].hex(), "start": 20}));
    let e = echo(&p.h, 3);
    assert!(e.contains("-A05 pinned at"), "{e}");
    let t = all_texts(&mut p);
    has_all(&t, &words("IW", &[20, 22, 24, 26]), "-A05 at 20");
    has_all(&t, &words("IW", &[28, 30, 32, 34]), "-A06 follows it");
    has_all(&t, &digital("I", 0..=3), "cards before it stay");
    // Moving an earlier card does not move the pinned one.
    p.h.run("plcslot", json!({"card": c["-A03"].hex(), "slot": 10})).unwrap();
    has_all(&all_texts(&mut p), &words("IW", &[20]), "-A05 keeps its start");
    step(&mut p.h, "plcaddress", json!({"card": c["-A05"].hex(), "start": null}));
    assert_eq!(prop(&p.h.doc(), c["-A05"], "plc.addr.start"), None);
}

// ------------------------------------------------------------------------------- K, M (AC 4, 6)

/// K / M: a typed address equal to another channel's automatic address in the same rack is
/// `address-twice` (go-to the typed one); **Clear the typed address** clears it; the same address in
/// another rack raises nothing (Q8).
#[test]
#[ignore = "tester 045: awaiting build"]
fn k_address_twice_within_a_rack_only() {
    let (mut p, c) = all_drawn(0x7045_k101);
    p.h.run("plcchannel", json!({"card": c["-A03"].hex(), "channel": 3, "address": "I2.3"})).unwrap();
    let rows = checks_of(&mut p.h, "address-twice");
    assert_eq!(rows.len(), 1, "{rows:?}");
    let t = rows[0].to_string();
    assert!(t.contains("I2.3 is the address of") && t.contains("(typed)"), "{t}");
    assert_eq!(rows[0]["severity"], "warning");
    rows_have_a_way_out(&mut p.h, "address-twice");
    step(&mut p.h, "plcchannel", json!({"card": c["-A03"].hex(), "channel": 3, "address": null}));
    assert!(checks_of(&mut p.h, "address-twice").is_empty());
    // A second station: its own I0.0 does not clash with -A01's.
    p.h.activate(p.cpu_page);
    let im = p.h.run("place", json!({"symbol": "plcim", "at": [600, 200]})).unwrap();
    let card = place_card(&mut p.h, "plcdi", 640.0, json!({"rack": im["component"].clone(), "slot": 1}));
    p.h.run("plcchannels", json!({"card": card["component"].clone(), "rest": true, "at": [600, 50]})).unwrap();
    assert!(checks_of(&mut p.h, "address-twice").is_empty(), "racks are separate stations");
}

/// M: `address-overlap`: `-A05` pinned at 0 overlaps `-A03`; **Unpin -A05** clears it.
#[test]
#[ignore = "tester 045: awaiting build"]
fn m_address_overlap_and_unpin() {
    let (mut p, c) = all_drawn(0x7045_m401);
    p.h.run("plcaddress", json!({"card": c["-A05"].hex(), "start": 0})).unwrap();
    let rows = checks_of(&mut p.h, "address-overlap");
    assert!(!rows.is_empty());
    assert!(rows.iter().any(|r| r.to_string().contains("-A05 inputs") && r.to_string().contains("overlap -A03 inputs")), "{rows:?}");
    assert!(rows.iter().all(|r| !r.to_string().contains(&p.rack.hex())), "{rows:?}");
    rows_have_a_way_out(&mut p.h, "address-overlap");
    step(&mut p.h, "plcaddress", json!({"card": c["-A05"].hex(), "start": null}));
    assert!(checks_of(&mut p.h, "address-overlap").is_empty());
}

/// M / §5.5: a block passing byte 65535 gets no address and an `address-overflow` row; **Unpin**
/// clears it; never a panic.
#[test]
#[ignore = "tester 045: awaiting build"]
fn m_address_overflow() {
    let (mut p, c) = all_drawn(0x7045_m402);
    p.h.run("plcaddress", json!({"card": c["-A04"].hex(), "start": 65535, "size": 4096})).unwrap();
    let rows = checks_of(&mut p.h, "address-overflow");
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(rows[0].to_string().contains("-A04's block passes byte 65535"), "{}", rows[0]);
    rows_have_a_way_out(&mut p.h, "address-overflow");
    read_everything(&mut p.h);
    let t = all_texts(&mut p);
    assert!(!t.iter().any(|x| x.starts_with("I65535.") || x.starts_with("I65536")), "no address past the limit: {t:?}");
    p.h.run("plcaddress", json!({"card": c["-A04"].hex(), "start": null})).unwrap();
    assert!(checks_of(&mut p.h, "address-overflow").is_empty());
    p.h.check();
}

/// M: `no-address`: style `none` with used channels: one Info count row.
#[test]
#[ignore = "tester 045: awaiting build"]
fn m_no_address_count_row() {
    let (mut p, _) = all_drawn(0x7045_m403);
    p.h.run("plcaddress", json!({"rack": p.rack.hex(), "style": "none"})).unwrap();
    let rows = checks_of(&mut p.h, "no-address");
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(rows[0].to_string().contains("8 used channels have no address (style none)"), "{}", rows[0]);
    assert_eq!(rows[0]["severity"], "info");
    // Spare channels raise nothing.
    assert!(checks_of(&mut p.h, "address-twice").is_empty() && checks_of(&mut p.h, "address-overlap").is_empty());
}

// ------------------------------------------------------------------------------- T (b): addresses

/// T: hostile address params and stored values: refusal or row, never a panic.
#[test]
#[ignore = "tester 045: awaiting build"]
fn t_hostile_addresses() {
    let (mut p, c) = all_drawn(0x7045_t401);
    let a03 = c["-A03"].hex();
    let rack = p.rack.hex();
    for v in [json!(-1), json!(65536), json!(1e300), json!("x"), json!(u64::MAX)] {
        survives(&mut p.h, "plcaddress", json!({"card": a03, "start": v}));
        survives(&mut p.h, "plcaddress", json!({"card": a03, "size": v}));
        survives(&mut p.h, "plcaddress", json!({"rack": rack, "input_start": v}));
    }
    survives(&mut p.h, "plcaddress", json!({"rack": rack, "style": "vendor"}));
    survives(&mut p.h, "plcaddress", json!({"rack": rack, "fix": true, "clear": true}));
    survives(&mut p.h, "plcaddress", json!({"rack": a03, "fix": true}));
    survives(&mut p.h, "plcaddress", json!({"card": rack, "start": 1}));
    survives(&mut p.h, "plcaddress", json!({}));
    for v in ["0", "4097", "99999999999999999999", "-3"] {
        survives(&mut p.h, "setprop", json!({"component": a03, "key": "plc.addr.size", "value": v}));
        survives(&mut p.h, "setprop", json!({"component": a03, "key": "plc.addr.start", "value": v}));
    }
    raw_setprop(&mut p.h, c["-A04"], "plc.addr.start", Some("99999999999999999999"));
    raw_setprop(&mut p.h, c["-A05"], "plc.addr.size", Some("-7"));
    raw_setprop(&mut p.h, p.rack, "plc.address.style", Some("\u{0}"));
    raw_setprop(&mut p.h, p.rack, "plc.address.start.input", Some("65535"));
    let mut h = reload(p.h, "hostile addresses", 0x7045_t402);
    read_everything(&mut h);
    h.check();
}
