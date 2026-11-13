//! Brief 046 (design 046 §11): graphical terminal-strip pages. Fixture G is built through the
//! commands' JSON forms: `-X01` at `+CP1` with four feed-through positions on fixture T's wires
//! (028), one PE and one fuse position drawn, an offer adding spares of every kind, `-W01`
//! (`YSLY-JZ 7G1.5`) with four cores, `link.1 = 1-4`, a wire bridge 3–4, end stops at both ends, a
//! partition after 2 and markers on 1–3.

use std::collections::BTreeSet;

use cadcraft_engine::doc::EntityKind;
use ecad_cadcraft::frame::Part;
use ecad_cadcraft::generated;
use ecad_cadcraft::outputs::Outputs;
use ecad_cadcraft::strip_page::{self, StripPages};
use ecad_model::{CableId, ComponentId, PageId, TerminalId};
use serde_json::{Value, json};

use crate::common::{H, id, pts};

/// Fixture G's objects.
struct G {
    h: H,
    schematic: PageId,
    x1: ComponentId,
    cable: CableId,
    terms: Vec<TerminalId>,
    m1: ComponentId,
}

fn place(h: &mut H, symbol: &str, at: (f64, f64)) -> Value {
    h.run("place", json!({"symbol": symbol, "at": [at.0, at.1]})).unwrap()
}

fn fixture(seed: u64) -> G {
    let mut h = H::new(seed);
    let schematic = h.first_page();
    h.run("locationbox", json!({"location": "+CP1", "corners": [[20, 150], [400, 280]]})).unwrap();
    h.run("locationbox", json!({"location": "+FIELD", "corners": [[20, 20], [400, 140]]})).unwrap();
    let k1: ComponentId = id(&place(&mut h, "coil", (100.0, 220.0)), "component");
    let k2: ComponentId = id(&place(&mut h, "coil", (150.0, 220.0)), "component");
    h.run("place", json!({"symbol": "no", "at": [200, 220], "device": k1.hex()})).unwrap();
    h.run("place", json!({"symbol": "no", "at": [250, 220], "device": k2.hex()})).unwrap();
    let m1: ComponentId = id(&place(&mut h, "motor3", (110.0, 50.0)), "component");
    h.run("setprop", json!({"component": m1.hex(), "key": "description", "value": "Pump"})).unwrap();
    place(&mut h, "pbno", (250.0, 60.0));
    for r in [
        vec![(100.0, 210.0), (100.0, 65.0)],
        vec![(150.0, 210.0), (150.0, 100.0), (110.0, 100.0), (110.0, 65.0)],
        vec![(200.0, 210.0), (200.0, 95.0), (120.0, 95.0), (120.0, 65.0)],
        vec![(250.0, 210.0), (250.0, 70.0)],
    ] {
        h.run("wire", json!({"points": pts(&r)})).unwrap();
    }
    let v = h.run("cableslash", json!({"from": [90, 120], "to": [260, 120], "tag": "-W01"})).unwrap();
    let cable: CableId = id(&v, "cable");
    h.run("setprop", json!({"cable": cable.hex(), "key": "type", "value": "YSLY-JZ 7G1.5"})).unwrap();
    let v = h.run("terminals", json!({"from": [90, 170], "to": [260, 170]})).unwrap();
    let x1: ComponentId = id(&v, "strip");
    let terms: Vec<TerminalId> = v["terminals"].as_array().unwrap().iter().map(|x| id(x, "terminal")).collect();
    // A PE and a fuse position on wires of their own.
    for x in [300.0, 340.0] {
        h.run("wire", json!({"points": pts(&[(x, 260.0), (x, 160.0)])})).unwrap();
    }
    h.run("terminals", json!({"strip": x1.hex(), "type": "PE", "level": "All", "from": [290, 200], "to": [310, 200]})).unwrap();
    let f = h.run("terminals", json!({"strip": x1.hex(), "type": "FUSE", "level": "All", "from": [330, 200], "to": [350, 200]})).unwrap();
    let fuse_label = f["terminals"][0]["label"].as_str().unwrap_or("6").to_string();
    let s = x1.hex();
    for (k, v) in [("terminals", "12+2PE+3LNPE+2FUSE"), ("link.1", "1-4")] {
        h.run("setprop", json!({"component": s, "key": k, "value": v})).unwrap();
    }
    h.run("setprop", json!({"component": s, "key": format!("rating.{fuse_label}"), "value": "2A"})).unwrap();
    h.run("bridge", json!({"from": terms[2].hex(), "to": terms[3].hex()})).unwrap();
    h.run("accessory", json!({"strip": s, "at": "start", "items": ["end_stop"]})).unwrap();
    h.run("accessory", json!({"strip": s, "at": "end", "items": ["end_stop"]})).unwrap();
    h.run("accessory", json!({"strip": s, "at": "2", "items": ["partition"]})).unwrap();
    h.run("accessory", json!({"strip": s, "at": ["1", "2", "3"], "on": true, "items": ["marker"]})).unwrap();
    G { h, schematic, x1, cable, terms, m1 }
}

/// The kept models, built on first show.
fn outputs(h: &mut H) -> Outputs {
    h.cfg(|p| p.outputs().clone())
}

/// The page's parts as Print Book and the window draw them.
fn parts(h: &mut H, page: PageId) -> Vec<Part> {
    let o = outputs(h);
    let doc = h.doc();
    generated::parts_of(&doc, page, Some(&o))
}

fn texts(parts: &[Part]) -> Vec<(String, f64)> {
    parts
        .iter()
        .filter_map(|p| match &p.kind {
            EntityKind::Text(t) => Some((t.value.clone(), t.height)),
            _ => None,
        })
        .collect()
}

fn has(parts: &[Part], s: &str) -> bool {
    texts(parts).iter().any(|(t, _)| t == s)
}

/// The kept graphic layout equals a rebuild from the kept strips (design 046 §9, verify mode).
fn same_as_built(h: &mut H) {
    let (kept, built) = h.cfg(|p| {
        let o = p.outputs().clone();
        let built = StripPages::build(p.ledger.document(), &o.strips);
        (o.strip_pages, built)
    });
    assert!(kept.same_as(&built), "strip pages differ from a rebuild");
}

/// Run `cmd` on the session; the window projections it caused and the pages it names.
fn counted(h: &mut H, cmd: &str, params: Value) -> (u64, Option<BTreeSet<PageId>>) {
    let (before, n) = h.p(|p| (p.projections, p.change_count()));
    h.s.execute(cmd, &params).unwrap();
    h.p(|p| (p.projections - before, p.changes_since(n)))
}

fn strip_page(g: &mut G) -> PageId {
    let head = g.h.head();
    let v = g.h.run("newpage", json!({"generated": "terminals", "style": "graphic", "strip": g.x1.hex()})).unwrap();
    assert_eq!(g.h.head(), head + 1, "one transaction");
    id(&v, "page")
}

/// §11 #1–#4: the page draws the tag box, the columns, leads, texts, the cable box and the far
/// device; spares are dashed, the fuse shows its rating.
#[test]
fn a_graphic_page_draws_the_strip_as_wired() {
    let mut g = fixture(0x46_01);
    let page = strip_page(&mut g);
    let doc = g.h.doc();
    assert_eq!(doc.pages[&page].title, strip_page::TITLE);
    assert_eq!(strip_page::style_of(&doc, page), strip_page::Style::Graphic);
    let p = parts(&mut g.h, page);
    let o = outputs(&mut g.h);
    let s = o.strips.get(g.x1).unwrap();
    // Every plan row's shown label is drawn (spares too).
    for r in &s.rows {
        assert!(texts(&p).iter().any(|(t, _)| t == &r.shown || t.starts_with(&format!("{} ", r.shown))), "label {} missing", r.shown);
    }
    for want in ["-X01", "+CP1", "-K01:A2", "-M01:U1", "-W01", "YSLY-JZ 7G1.5", "-M01 Pump", "links \u{25b8}", "bridges \u{25b8}"] {
        assert!(has(&p, want), "{want} not drawn: {:?}", texts(&p));
    }
    // Wire numbers, inside and outside, and core labels on -W01's cores.
    let r0 = &s.rows[0];
    if let Some(w) = r0.inside.first().and_then(|c| c.wire.clone()) {
        assert!(texts(&p).iter().filter(|(t, _)| *t == w).count() >= 2, "wire number {w} above and below");
    }
    for c in s.rows.iter().flat_map(|r| &r.outside).filter(|c| c.cable.is_some()) {
        if let Some(core) = &c.core {
            assert!(has(&p, core), "core {core}");
        }
    }
    // The fuse's rating beside its label.
    assert!(texts(&p).iter().any(|(t, _)| t.ends_with(" 2A")), "fuse rating");
    // The title names the strip.
    let titles: Vec<String> = texts(&p).into_iter().map(|(t, _)| t).filter(|t| t.starts_with("Terminal strips: -X01")).collect();
    assert!(!titles.is_empty(), "sheet title");
    same_as_built(&mut g.h);
}

/// §11 #2: one bar under -W01's cores (drawn heavier than the leads) and one box.
#[test]
fn one_cable_bar_and_one_box_per_cable() {
    let mut g = fixture(0x46_02);
    let page = strip_page(&mut g);
    let p = parts(&mut g.h, page);
    let heavy = p.iter().filter(|x| matches!(x.kind, EntityKind::Line(_)) && x.weight == Some(ecad_cadcraft::look::CABLE_WEIGHT * 2)).count();
    assert_eq!(heavy, 1, "one cable bar");
    assert_eq!(texts(&p).iter().filter(|(t, _)| t == "-W01").count(), 1, "one cable box");
    let _ = (g.cable, g.m1, g.schematic);
}

/// §11 #5: 45 positions run onto a second sheet; no position split; per-strip titles; the book's
/// Total counts both; a link over the break draws its stub on both sheets.
#[test]
fn a_long_strip_runs_onto_a_second_sheet() {
    let mut g = fixture(0x46_03);
    let s = g.x1.hex();
    g.h.run("setprop", json!({"component": s, "key": "terminals", "value": "45"})).unwrap();
    g.h.run("setprop", json!({"component": s, "key": "link.2", "value": "30-45"})).unwrap();
    let page = strip_page(&mut g);
    let o = outputs(&mut g.h);
    let doc = g.h.doc();
    assert_eq!(generated::sheets(&doc, page, Some(&o)), 2);
    let before = generated::book_sheets(&doc, Some(&o));
    assert_eq!(before, doc.page_order.len() + 1, "Total counts both sheets");
    let p = parts(&mut g.h, page);
    // `<page title>: <tag> <description> (i/n)` (design 046 §6.3).
    let titles: Vec<String> = texts(&p).into_iter().map(|(t, _)| t).filter(|t| t.starts_with("Terminal strips: -X01")).collect();
    assert!(titles.len() == 2 && titles[0].ends_with(" (1/2)") && titles[1].ends_with(" (2/2)"), "{titles:?}");
    // No position split: every position's levels are on one sheet (the layout is by block).
    let strip = o.strips.get(g.x1).unwrap();
    let geometry = strip_page::Geometry::of(&doc);
    let (cols, blocks) = strip_page::columns(strip, &geometry);
    let widths: Vec<f64> = blocks.iter().map(|b| b.2).collect();
    let sheets = strip_page::paginate(&widths, geometry.available(&ecad_model::sheet::sheet_of(&doc, page)));
    assert_eq!(sheets.len(), 2);
    assert_eq!(blocks.iter().map(|b| b.1 - b.0).sum::<usize>(), cols.len(), "blocks cover every column");
    // link.2 (30-45) crosses the break: a stub past the first column of sheet 2 and past the last
    // of sheet 1. Each stub is a 4 mm line ending at a column centre.
    let stubs = p
        .iter()
        .filter_map(|x| match &x.kind {
            EntityKind::Line(l) => Some((l.a, l.b)),
            _ => None,
        })
        .filter(|(a, b)| (a.y - b.y).abs() < 1e-9 && ((b.x - a.x).abs() - 4.0).abs() < 1e-6)
        .count();
    assert!(stubs >= 2, "continuation stubs on both sheets ({stubs})");
    same_as_built(&mut g.h);
}

/// §11 #6: text heights follow the text styles.
#[test]
fn text_heights_follow_the_text_styles() {
    let mut g = fixture(0x46_04);
    let page = strip_page(&mut g);
    let height = |p: &[Part]| texts(p).into_iter().find(|(t, _)| t == "+CP1").map(|(_, h)| h).unwrap();
    let before = height(&parts(&mut g.h, page));
    g.h.run("setdefault", json!({"key": "text.table.height", "level": "project", "value": "3.5"})).unwrap();
    let after = height(&parts(&mut g.h, page));
    assert!((after - 3.5).abs() < 1e-9 && (before - 2.5).abs() < 1e-9, "{before} → {after}");
}

/// §11 #7–#9: the update map. A wire on -X01:3 redraws the shown page once; an edit no strip shows
/// projects nothing; a cable type change lays out only the strips its cores are on.
#[test]
fn the_shown_strip_page_redraws_only_for_its_strip() {
    let mut g = fixture(0x46_05);
    let page = strip_page(&mut g);
    // An edit no strip shows.
    let other: ComponentId = id(&g.h.run("newpage", json!({"title": "P2"})).unwrap(), "page");
    let _ = other;
    let m2: ComponentId = id(&place(&mut g.h, "coil", (100.0, 200.0)), "component");
    g.h.activate(page);
    let (n, named) = counted(&mut g.h, "setprop", json!({"component": m2.hex(), "key": "maker", "value": "Acme"}));
    assert_eq!(n, 0, "an edit no strip shows projected the strip page {n} time(s)");
    assert!(!named.expect("pages known").contains(&page));
    assert!(outputs(&mut g.h).strip_pages.last_changed.is_empty());
    // A strip change: -X01:3's wire gets a number.
    let doc = g.h.doc();
    let w3 = doc.ends_at(g.terms[2]).into_iter().next().map(|(w, _)| w).unwrap();
    let (n, named) = counted(&mut g.h, "wirenumber", json!({"conductor": w3.hex(), "value": "W-77"}));
    assert_eq!(n, 1, "the strip page redraws once");
    assert!(named.expect("pages known").contains(&page));
    let o = outputs(&mut g.h);
    assert_eq!(o.strip_pages.last_work, 1, "one strip laid out");
    same_as_built(&mut g.h);
    // The cable's type: only -X01 is marked, the box shows the new type.
    let (n, _) = counted(&mut g.h, "setprop", json!({"cable": g.cable.hex(), "key": "type", "value": "5G1.5"}));
    assert_eq!(n, 1, "the strip page redraws for its cable's type");
    let o = outputs(&mut g.h);
    assert_eq!(o.strip_pages.last_changed, BTreeSet::from([g.x1]));
    assert!(has(&parts(&mut g.h, page), "5G1.5"));
    // The far device's description.
    let (n, _) = counted(&mut g.h, "setprop", json!({"component": g.m1.hex(), "key": "description", "value": "Fan"}));
    assert_eq!(n, 1, "the strip page redraws for its far device's description");
    assert!(has(&parts(&mut g.h, page), "-M01 Fan"));
    same_as_built(&mut g.h);
}

/// §11 #10: switching the style is one transaction each way; undo takes it back.
#[test]
fn switching_the_style_is_one_transaction() {
    let mut g = fixture(0x46_06);
    let page = strip_page(&mut g);
    let head = g.h.head();
    g.h.run("setdefault", json!({"key": "terminal.plan.style", "level": "page", "page": page.hex(), "value": "table"})).unwrap();
    assert_eq!(g.h.head(), head + 1);
    let p = parts(&mut g.h, page);
    assert!(has(&p, "Inside target"), "the table");
    g.h.cmdline("u");
    let p = parts(&mut g.h, page);
    assert!(!has(&p, "Inside target") && has(&p, "-W01"), "the graphic again");
    // The project default reaches a page with no style of its own.
    let plain: PageId = id(&g.h.run("newpage", json!({"generated": "terminals"})).unwrap(), "page");
    assert!(has(&parts(&mut g.h, plain), "Inside target"));
    g.h.run("setdefault", json!({"key": "terminal.plan.style", "level": "project", "value": "graphic"})).unwrap();
    assert!(!has(&parts(&mut g.h, plain), "Inside target"));
    same_as_built(&mut g.h);
}

/// §11 #11: `style` on another kind is refused with §8.1's words; nothing recorded.
#[test]
fn style_is_only_for_a_terminal_plan_page() {
    let mut g = fixture(0x46_07);
    let head = g.h.head();
    let e = g.h.run("newpage", json!({"generated": "cover", "style": "graphic"})).unwrap_err();
    assert!(e.contains("`style` is for a terminal plan page (`generated: \"terminals\"`)"), "{e}");
    let e = g.h.run("newpage", json!({"generated": "terminals", "style": "wavy"})).unwrap_err();
    assert!(e.contains("`style` is \"table\" or \"graphic\""), "{e}");
    assert_eq!(g.h.head(), head, "nothing recorded");
}

/// §11 #12: Print Book draws the same parts as the page (built fresh from the document), in ink.
#[test]
fn print_book_prints_what_the_page_shows() {
    let mut g = fixture(0x46_08);
    let page = strip_page(&mut g);
    let shown = parts(&mut g.h, page);
    let doc = g.h.doc();
    let printed = generated::parts_of(&doc, page, None);
    assert_eq!(shown, printed);
    let content: Vec<Part> = shown.iter().filter(|p| p.layer == generated::LAYER).cloned().collect();
    let flagged = content.iter().filter(|p| p.aci.is_some()).count();
    let missing = texts(&content).iter().filter(|(t, _)| t == "?").count();
    assert!(flagged <= missing * 2, "only `?` marks are coloured ({flagged} coloured, {missing} `?`)");
}

/// §11 #13: a terminals page with no style (a 036 project) still draws the table.
#[test]
fn an_old_terminals_page_still_draws_the_table() {
    let mut g = fixture(0x46_09);
    let page: PageId = id(&g.h.run("newpage", json!({"generated": "terminals"})).unwrap(), "page");
    let doc = g.h.doc();
    assert_eq!(strip_page::style_of(&doc, page), strip_page::Style::Table);
    assert!(has(&parts(&mut g.h, page), "Inside target"));
}

/// Hostile and empty states (§8.2): no strips; a `strip` naming a deleted device; a huge offer.
#[test]
fn empty_and_hostile_states_draw_without_crashing() {
    let mut h = H::new(0x46_0a);
    let page: PageId = id(&h.run("newpage", json!({"generated": "terminals", "style": "graphic"})).unwrap(), "page");
    assert!(has(&parts(&mut h, page), strip_page::NO_STRIPS));
    let mut g = fixture(0x46_0b);
    g.h.run("setprop", json!({"component": g.x1.hex(), "key": "terminals", "value": "999"})).unwrap();
    let page = strip_page(&mut g);
    let o = outputs(&mut g.h);
    let n = generated::sheets(&g.h.doc(), page, Some(&o));
    assert!((20..=40).contains(&n), "999 positions at about 40 a sheet: {n}");
    let _ = parts(&mut g.h, page);
    // The page's `strip` names a device that is then deleted.
    let mut doc = g.h.doc();
    doc.components.remove(&g.x1);
    let p = generated::parts_of(&doc, page, None);
    assert!(texts(&p).iter().any(|(t, _)| t == "?"), "a flagged `?`");
}

/// Paging never splits a block and puts a block wider than the sheet on a sheet of its own.
#[test]
fn paginate_keeps_blocks_whole() {
    assert_eq!(strip_page::paginate(&[], 100.0), vec![(0, 0)]);
    assert_eq!(strip_page::paginate(&[40.0, 40.0, 40.0], 100.0), vec![(0, 2), (2, 3)]);
    assert_eq!(strip_page::paginate(&[150.0, 10.0], 100.0), vec![(0, 1), (1, 2)]);
    assert_eq!(strip_page::paginate(&[f64::NAN, 10.0], 100.0).len(), 1);
}
