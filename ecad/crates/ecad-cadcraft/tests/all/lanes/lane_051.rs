//! Brief 051 (PLC visuals and the rack overview, design 051 §10.1): the tile model, `plcpage`, the
//! ruled I/O sheet and the `plcrack` sheet.

use cadcraft_engine::doc::EntityKind;
use ecad_cadcraft::plc::overview::{self, Square, TileKind};
use ecad_cadcraft::plc::sheets::{self, Seg};
use ecad_model::{ComponentId, PageId};
use serde_json::{Value, json};

use super::lane_045::placed;
use crate::common::{H, id, pts};

/// A CPU with a DI card in slot 1 wired to eight lamps through a fence, an AI card in slot 2 and a
/// DQ card in slot 3, none of the AI's or DQ's channels drawn (045's I/O fixture).
struct F {
    h: H,
    cpu: ComponentId,
    di: ComponentId,
    ai: ComponentId,
    dq: ComponentId,
}

fn fixture(seed: u64) -> F {
    let mut h = H::new(seed);
    let cpu = placed(&mut h, "plccpu", 50.0, 400.0);
    let di = placed(&mut h, "plcdi", 100.0, 400.0);
    let ai = placed(&mut h, "plcai", 140.0, 400.0);
    let dq = placed(&mut h, "plcdq", 180.0, 400.0);
    for i in 0..8 {
        let x = 100.0 + 10.0 * f64::from(i);
        placed(&mut h, "lamp", x, 260.0);
        h.run("wire", json!({"points": pts(&[(x, 250.0), (x, 150.0)])})).unwrap();
    }
    h.run("plcchannels", json!({"card": di.hex(), "from": [95.0, 200.0], "to": [175.0, 200.0]})).unwrap();
    F { h, cpu, di, ai, dq }
}

/// Every text value drawn on `page`.
fn texts(h: &mut H, page: PageId) -> Vec<String> {
    let d = h.doc();
    h.cfg(|p| {
        p.outputs();
        let o = p.outputs.as_ref().unwrap();
        ecad_cadcraft::generated::parts_of(&d, page, Some(o))
            .into_iter()
            .filter_map(|x| match x.kind {
                EntityKind::Text(t) => Some(t.value),
                _ => None,
            })
            .collect()
    })
}

fn sheets_of(h: &mut H, page: PageId) -> usize {
    let d = h.doc();
    h.cfg(|p| {
        p.outputs();
        ecad_cadcraft::generated::sheets(&d, page, p.outputs.as_ref())
    })
}

// ---------------------------------------------------------------- packing (§4.1)

#[test]
fn n_groups_pack_on_sheets_and_a_long_one_splits_continued() {
    let seg = |group, start, end, continued| Seg { group, start, end, continued };
    // (rows of each group, sheet height in lines) -> sheets.
    let cases: Vec<(Vec<usize>, f64, Vec<Vec<Seg>>)> = vec![
        (vec![], 44.0, vec![vec![]]),
        (vec![16, 4, 16], 44.0, vec![vec![seg(0, 0, 16, false), seg(1, 0, 4, false), seg(2, 0, 16, false)]]),
        // The third group does not fit in what is left: it goes to the next sheet whole.
        (vec![16, 16, 16], 44.0, vec![vec![seg(0, 0, 16, false), seg(1, 0, 16, false)], vec![seg(2, 0, 16, false)]]),
        // Longer than a whole sheet: filled, then continued.
        (vec![4, 60], 44.0, vec![vec![seg(0, 0, 4, false), seg(1, 0, 35, false)], vec![seg(1, 35, 60, true)]]),
        // A card with no channels still has its heading.
        (vec![0], 44.0, vec![vec![seg(0, 0, 0, false)]]),
    ];
    for (rows, height, want) in cases {
        assert_eq!(sheets::pack(&rows, 2.0, 1.0, 1.0, height), want, "{rows:?}");
    }
    // Hostile: a sheet too small for a heading still makes progress, one row a sheet.
    let tiny = sheets::pack(&[3], 2.0, 1.0, 1.0, 1.0);
    assert_eq!(tiny.len(), 3);
    assert!(tiny.iter().flatten().all(|s| s.end - s.start == 1));
}

#[test]
fn n_the_io_sheet_is_a_ruled_table_per_card_with_its_heading_above() {
    let F { mut h, di, .. } = fixture(5101);
    h.run("plcchannel", json!({"card": di.hex(), "channel": 3, "address": "I9.9"})).unwrap();
    let page: PageId = id(&h.run("newpage", json!({"generated": "iolist"})).unwrap(), "page");
    // 16 + 4 + 16 rows, three headings and header rows, two gaps: 44 lines, one sheet (W7: was 3).
    assert_eq!(sheets_of(&mut h, page), 1);
    let t = texts(&mut h, page);
    let heads: Vec<&String> = t.iter().filter(|x| x.contains(" used, ")).collect();
    assert_eq!(heads.len(), 3, "{heads:?}");
    assert!(heads[0].starts_with("-A01 slot 1 \u{b7} -A02 \u{b7} DI 16 \u{b7} "), "{heads:?}");
    assert!(heads[0].ends_with("8 used, 0 reserved, 8 spare"), "{heads:?}");
    // Slot and Card are only in the heading; the header row is the new column set.
    assert!(!t.iter().any(|x| x == "Slot" || x == "Card"), "{t:?}");
    for c in ["Ch", "Address", "Pins", "Description", "State", "Wired to", "Wire", "Place"] {
        // The title block has a Description too.
        assert!(t.iter().filter(|x| *x == c).count() >= 3, "{c} once per card");
    }
    assert!(t.iter().any(|x| x == "I9.9*"), "typed address marked");
    // A 60-channel card splits with `(continued)` on the next sheet; the DQ card follows it there
    // only if it fits (here it takes a third).
    h.run("setprop", json!({"component": di.hex(), "key": "plc.channels", "value": "60"})).unwrap();
    assert_eq!(sheets_of(&mut h, page), 3);
}

#[test]
fn n_the_io_sheet_count_is_the_drawn_count() {
    let F { mut h, di, .. } = fixture(5102);
    h.run("setprop", json!({"component": di.hex(), "key": "plc.channels", "value": "60"})).unwrap();
    let page: PageId = id(&h.run("newpage", json!({"generated": "iolist"})).unwrap(), "page");
    let n = sheets_of(&mut h, page);
    assert_eq!(n, 3, "60 rows split over two sheets; 4 and 16 after them");
    let t = texts(&mut h, page);
    assert!(t.iter().any(|x| x.ends_with("(continued)")), "{t:?}");
}

// ---------------------------------------------------------------- tiles (§3.3, §3.4)

#[test]
fn j_tiles_of_a_rack_slots_squares_and_totals() {
    let F { mut h, cpu, di, ai, dq } = fixture(5103);
    h.run("plcchannel", json!({"card": ai.hex(), "channel": 1, "text": "Tank level"})).unwrap();
    let d = h.doc();
    let (tiles, totals, chips) = h.cfg(|p| {
        let plc = &p.outputs().plc;
        let rack = plc.racks.get(&cpu).unwrap().clone();
        (overview::rack_tiles(&d, plc, &rack), overview::totals(&plc.cards_of(cpu)), overview::chips(plc))
    });
    let kinds: Vec<TileKind> = tiles.iter().map(|t| t.kind).collect();
    assert_eq!(kinds, [TileKind::Rack(cpu), TileKind::Card(di), TileKind::Card(ai), TileKind::Card(dq), TileKind::Free]);
    assert_eq!(tiles[0].range, "32 slots, 3 used");
    assert_eq!((tiles[1].head.as_str(), tiles[1].tag.as_str(), tiles[1].what.as_str()), ("Slot 1", "-A02", "PLC digital input card"));
    assert_eq!(tiles[1].range, "I0.0\u{2013}I1.7");
    assert_eq!(tiles[1].squares.len(), 16);
    assert_eq!(tiles[1].squares.iter().filter(|s| **s == Square::Used).count(), 8);
    assert_eq!(tiles[2].squares[1], Square::Reserved);
    assert_eq!(tiles[4].head, "Slot 4");
    assert_eq!(tiles[4].element(), "free.4");
    assert!(!tiles[1].pages.is_empty(), "drawn");
    assert_eq!(overview::totals_words(totals), "36 channels, 8 in use");
    assert_eq!(chips.len(), 1);
    assert_eq!(chips[0].words(), "-A01 CPU");
}

#[test]
fn j_a_channel_with_a_check_is_red_and_counted() {
    let F { mut h, cpu, di, .. } = fixture(5104);
    // Channel 3 typed with channel 0's address: an `address-twice` row names it.
    h.run("plcchannel", json!({"card": di.hex(), "channel": 3, "address": "I0.0"})).unwrap();
    let d = h.doc();
    let tile = h.cfg(|p| {
        let plc = &p.outputs().plc;
        let rack = plc.racks.get(&cpu).unwrap().clone();
        overview::rack_tiles(&d, plc, &rack).into_iter().find(|t| t.kind == TileKind::Card(di)).unwrap()
    });
    assert_eq!(tile.squares[3], Square::Check);
    assert!(tile.checks >= 1);
}

#[test]
fn q_plcracks_returns_the_tile_data_and_records_nothing() {
    let F { mut h, .. } = fixture(5105);
    let head = h.head();
    let v = h.run("plcracks", json!({})).unwrap();
    assert_eq!(h.head(), head);
    let di = &v["racks"][0]["cards"][0];
    assert_eq!(di["tag"], "-A02");
    assert_eq!(di["range"], json!({"inputs": "I0.0\u{2013}I1.7", "outputs": null}));
    assert_eq!(di["checks"], 0);
    assert_eq!(di["pages"].as_array().map(Vec::len), Some(1));
    let dq = &v["racks"][0]["cards"][2];
    assert!(dq["range"]["outputs"].is_string(), "{dq}");
}

// ---------------------------------------------------------------- plcpage (§3.6)

#[test]
fn l_make_its_page_draws_every_channel_and_undo_removes_it() {
    let F { mut h, ai, .. } = fixture(5106);
    let pages = h.doc().page_order.len();
    let head = h.head();
    let v = h.run("plcpage", json!({"card": ai.hex()})).unwrap();
    assert_eq!(h.head(), head + 1, "one transaction");
    assert_eq!(v["title"], "-A03 AI 4");
    assert_eq!(v["channels"], json!([0, 1, 2, 3]));
    assert_eq!(v["body"], false, "the body is drawn already");
    assert!(v["message"].as_str().unwrap().starts_with("Made page "), "{v}");
    let page: PageId = id(&v, "page");
    let d = h.doc();
    assert_eq!(d.page_order.len(), pages + 1);
    assert_eq!(d.page_order.iter().position(|p| *p == page), Some(1), "after the rack's last card page");
    assert_eq!(d.representations_on(page).count(), 4);
    // Every channel drawn: refused, and it says where.
    let e = h.run("plcpage", json!({"card": ai.hex()})).unwrap_err();
    assert!(e.starts_with("All 4 channels of -A03 are drawn ("), "{e}");
    h.cmdline("u");
    assert_eq!(h.doc().page_order.len(), pages);
    let tile_pages = h.cfg(|p| p.outputs().plc.cards.get(&ai).map(|c| c.chans.len()));
    assert_eq!(tile_pages, Some(0), "the channels went with the page");
}

#[test]
fn l_make_its_page_draws_a_card_with_no_body_and_places_it() {
    let mut h = H::new(5107);
    placed(&mut h, "plccpu", 50.0, 400.0);
    // A card drawn only by a channel cell (no body), then its page draws the rest and the body.
    let v = h.run("place", json!({"symbol": "chdq", "at": [100.0, 100.0]})).unwrap();
    let card: ComponentId = id(&v, "component");
    let r = h.run("plcpage", json!({"card": card.hex()})).unwrap();
    assert_eq!(r["body"], true);
    assert_eq!(r["channels"].as_array().map(Vec::len), Some(15));
    let page: PageId = id(&r, "page");
    let d = h.doc();
    let reps: Vec<_> = d.representations_on(page).collect();
    assert_eq!(reps.len(), 16, "15 cells and the body");
}

#[test]
fn r_make_its_page_refuses_in_words() {
    let F { mut h, cpu, ai, .. } = fixture(5108);
    let head = h.head();
    for (params, want) in [
        (json!({"card": cpu.hex()}), "is a rack"),
        (json!({"card": ai.hex(), "after": "999999999999"}), "No page 999999999999."),
        (json!({"card": ai.hex(), "after": "x".repeat(10_000)}), "`after` is a page number"),
        (json!({"card": ai.hex(), "after": [1]}), "`after` is a page number"),
        (json!({"card": "-Z99"}), ""),
        (json!({}), ""),
    ] {
        let e = h.run("plcpage", params.clone()).unwrap_err();
        assert!(e.contains(want), "{params}: {e}");
    }
    h.run("devicedelete", json!({"component": ai.hex()})).unwrap();
    assert!(h.run("plcpage", json!({"card": ai.hex()})).is_err());
    assert_eq!(h.head(), head + 1, "only the delete was recorded");
}

#[test]
fn r_the_page_title_pattern_is_bounded() {
    let F { mut h, ai, .. } = fixture(5109);
    for bad in ["x".repeat(10_000), "{nope}".to_string(), "{tag".to_string()] {
        assert!(h.run("setdefault", json!({"key": "plc.page.title", "value": bad})).is_err(), "{bad}");
    }
    h.run("setdefault", json!({"key": "plc.page.title", "value": "{rack} slot {slot:02} {type}"})).unwrap();
    let v = h.run("plcpage", json!({"card": ai.hex()})).unwrap();
    assert_eq!(v["title"], "-A01 slot 02 AI");
}

// ---------------------------------------------------------------- the rack sheet (§4.2)

#[test]
fn o_the_rack_sheet_draws_a_heading_and_tiles_per_rack() {
    let F { mut h, .. } = fixture(5110);
    let page: PageId = id(&h.run("newpage", json!({"generated": "plcrack"})).unwrap(), "page");
    assert_eq!(h.doc().pages.get(&page).map(|p| p.title.clone()).as_deref(), Some("PLC racks"));
    assert_eq!(sheets_of(&mut h, page), 1);
    let t = texts(&mut h, page);
    assert!(t.iter().any(|x| x == "-A01 \u{b7} CPU \u{b7} 32 slots, 3 used"), "{t:?}");
    for w in ["Rack", "Slot 1", "Slot 2", "Slot 3", "Slot 4", "Free", "-A02", "-A03", "-A04"] {
        assert!(t.iter().any(|x| x == w), "{w} in {t:?}");
    }
    assert!(t.iter().any(|x| x.starts_with('\u{2192}')), "a place: {t:?}");
    assert_eq!(sheets::tiles_per_row(390.0), 9, "9 per A3 row");
}

#[test]
fn o_the_rack_sheet_without_racks_says_so() {
    let mut h = H::new(5111);
    let page: PageId = id(&h.run("newpage", json!({"generated": "plcrack"})).unwrap(), "page");
    assert_eq!(sheets_of(&mut h, page), 1);
    let t = texts(&mut h, page);
    assert!(t.iter().any(|x| x == sheets::NO_RACKS), "{t:?}");
}

#[test]
fn r_a_rack_of_many_tiles_wraps_and_continues_on_sheets() {
    let mut h = H::new(5112);
    let cpu = placed(&mut h, "plccpu", 50.0, 400.0);
    h.run("setprop", json!({"component": cpu.hex(), "key": "plc.slots", "value": "999"})).unwrap();
    let card = placed(&mut h, "plcdi", 100.0, 400.0);
    h.run("plcslot", json!({"card": card.hex(), "slot": 999})).unwrap();
    let page: PageId = id(&h.run("newpage", json!({"generated": "plcrack"})).unwrap(), "page");
    // 999 slots shown (998 free and the card), the rack tile: 1000 tiles, 9 a row, 112 rows.
    let n = sheets_of(&mut h, page);
    assert!(n > 10, "{n}");
    let t = texts(&mut h, page);
    assert!(!t.is_empty());
    let _: Value = h.run("plcracks", json!({})).unwrap();
}
