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
        let x = 100.0 + 20.0 * f64::from(i);
        placed(&mut h, "lamp", x, 260.0);
        h.run("wire", json!({"points": pts(&[(x, 250.0), (x, 150.0)])})).unwrap();
    }
    h.run("plcchannels", json!({"card": di.hex(), "from": [95.0, 200.0], "to": [255.0, 200.0]})).unwrap();
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
    assert_eq!(
        (tiles[1].head.as_str(), tiles[1].tag.as_str(), tiles[1].what.as_str()),
        ("Slot 1", "-A02", "DI 16"),
        "the body name placing wrote is not the user's description"
    );
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

// ---------------------------------------------------------------- the card on the page (§2)

/// The frame parts of `page`: (lines, texts).
fn frame(h: &mut H, page: PageId) -> (Vec<(ecad_model::Pt, ecad_model::Pt)>, Vec<String>) {
    let d = h.doc();
    let mut lines = Vec::new();
    let mut texts = Vec::new();
    for p in ecad_cadcraft::plc::look::parts_on_page(&d, page) {
        match p.shape {
            ecad_cadcraft::xref::Shape::Line(a, b) => lines.push((a, b)),
            ecad_cadcraft::xref::Shape::Text { value, .. } => texts.push(value),
        }
    }
    (lines, texts)
}

/// Whether a line is slanted (a break edge).
fn slanted(l: &(ecad_model::Pt, ecad_model::Pt)) -> bool {
    (l.0.x - l.1.x).abs() > 1.0 && (l.0.y - l.1.y).abs() > 1.0
}

#[test]
fn a_d_e_one_frame_per_run_with_break_edges_where_the_card_goes_on() {
    let F { mut h, di, .. } = fixture(5120);
    let page = h.first_page();
    let runs = ecad_cadcraft::plc::look::runs_on_page(&h.doc(), page);
    let di_runs: Vec<_> = runs.iter().filter(|r| r.card == di).collect();
    assert_eq!(di_runs.len(), 2, "the fenced cells and the body elsewhere on the page");
    let cells = di_runs.iter().find(|r| r.first == Some(0)).expect("the cells' run");
    assert_eq!((cells.first, cells.last), (Some(0), Some(7)));
    assert!((cells.right - cells.left - 160.0).abs() < 1e-6, "eight 20 mm cells abut");
    let (lines, texts) = frame(&mut h, page);
    assert!(lines.iter().filter(|l| slanted(l)).count() >= 1, "channels 8–15 and the body go on: a break edge");
    assert!(texts.iter().any(|t| t == "-A02") && texts.iter().any(|t| t == "-A01 slot 1"), "{texts:?}");
    assert!(texts.iter().any(|t| t == "DI 16"), "{texts:?}");
    // The rest drawn after the run, the body moved to its end: one run, straight sides.
    h.run("plcchannels", json!({"card": di.hex(), "rest": true})).unwrap();
    let d = h.doc();
    let body = d
        .representations_of(ecad_model::Subject::Component(di))
        .find(|(_, r)| ecad_cadcraft::devices::rep_starter(&d, r).is_some_and(|s| s.key == "plcdi"))
        .map(|(_, r)| r.items[0])
        .unwrap();
    // Cells' points are at y 200; the body's at its origin + 25; cell 15 ends at x 100 + 15·20 + 10.
    let item = d.items[&body].clone();
    let ecad_model::ItemKind::Symbol(mut s) = item.kind.clone() else { panic!() };
    s.placement.origin = ecad_model::Pt::new(100.0 + 16.0 * 20.0, 200.0 - 25.0);
    let after = ecad_model::Item { kind: ecad_model::ItemKind::Symbol(s), ..item.clone() };
    h.cfg(|p| {
        p.ledger.commit(ecad_model::Author::User, "move body", vec![ecad_model::Op::ModifyItem { id: body, before: item, after }]).unwrap();
    });
    let runs = ecad_cadcraft::plc::look::runs_on_page(&h.doc(), page);
    let di_runs: Vec<_> = runs.iter().filter(|r| r.card == di).collect();
    assert_eq!(di_runs.len(), 1, "{di_runs:?}");
    assert!(!di_runs[0].right_cell, "the body is the run's power end");
    let (lines, _) = frame(&mut h, page);
    assert!(!lines.iter().any(slanted), "the whole card: straight sides");
}

#[test]
fn b_a_fence_across_wires_closer_than_a_cell_is_refused() {
    let mut h = H::new(5121);
    let di = placed(&mut h, "plcdi", 100.0, 400.0);
    for x in [100.0, 110.0] {
        h.run("wire", json!({"points": pts(&[(x, 250.0), (x, 150.0)])})).unwrap();
    }
    let head = h.head();
    let e = h.run("plcchannels", json!({"card": di.hex(), "from": [95.0, 200.0], "to": [115.0, 200.0]})).unwrap_err();
    assert!(e.contains("are 10 mm apart; a DI cell is 20 mm wide. Space the wires 20 mm apart, or place compact di symbols."), "{e}");
    assert!(!e.starts_with("Wires wire"), "{e}");
    assert_eq!(h.head(), head, "nothing written");
}

#[test]
fn f_a_card_at_180_keeps_its_name_block_upright_above() {
    let F { mut h, dq, .. } = fixture(5122);
    let v = h.run("plcpage", json!({"card": dq.hex()})).unwrap();
    let page: PageId = id(&v, "page");
    let runs = ecad_cadcraft::plc::look::runs_on_page(&h.doc(), page);
    assert!(!runs.is_empty());
    assert!(runs.iter().all(|r| r.dir > 0.0), "outputs at 180°: the frame above the points");
    let d = h.doc();
    let parts = ecad_cadcraft::plc::look::parts_on_page(&d, page);
    let tag = parts
        .iter()
        .find_map(|p| match &p.shape {
            ecad_cadcraft::xref::Shape::Text { value, at, valign, .. } if value == "-A04" => Some((*at, *valign)),
            _ => None,
        })
        .expect("the tag in the name block");
    assert_eq!(tag.1, cadcraft_engine::doc::VAlign::Top, "top left, under the far edge");
    assert!(tag.0.y > runs[0].top, "above the points");
}

#[test]
fn h_i_text_styles_and_name_block_lines() {
    let F { mut h, di, .. } = fixture(5123);
    let d = h.doc();
    let ts = |k| ecad_cadcraft::text_style::text_style(&d, k).height;
    assert_eq!((ts(ecad_cadcraft::text_style::TextKind::ChannelText), ts(ecad_cadcraft::text_style::TextKind::Address)), (2.5, 2.0));
    let page = h.first_page();
    h.run("setprop", json!({"component": di.hex(), "key": "maker", "value": "Maker A"})).unwrap();
    h.run("setprop", json!({"component": di.hex(), "key": "part_number", "value": "DI-16"})).unwrap();
    let (_, t) = frame(&mut h, page);
    for w in ["-A02", "-A01 slot 1", "Maker A", "DI-16"] {
        assert!(t.iter().any(|x| x == w), "{w}: {t:?}");
    }
    h.run("setdefault", json!({"key": "plc.text.caption.show", "level": "object", "component": di.hex(), "value": "off"})).unwrap();
    let (_, t) = frame(&mut h, page);
    assert!(!t.iter().any(|x| x == "-A01 slot 1"), "{t:?}");
    h.run("setdefault", json!({"key": "plc.frame.show", "level": "object", "component": di.hex(), "value": "off"})).unwrap();
    let (_, t) = frame(&mut h, page);
    assert!(!t.iter().any(|x| x == "-A02"), "no frame: {t:?}");
}
