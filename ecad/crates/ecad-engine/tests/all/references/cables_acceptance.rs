//! Independent tester's acceptance checks for brief 010 (cable references across pages), written
//! from the brief and design 010 only (fixture R of design §11, the refusals of §4.4, the JSON
//! forms of §7 and the existing-command table of §9).
//!
//! Every step runs through the harness in `common`, which after each command asserts that replay
//! equals the live document, every open page drawing equals a fresh projection, the handle map is
//! one-to-one and no attached end is away from its terminal. So "the drawing shows the live text"
//! is checked on every page after every step, not only where a test reads it.
//!
//! The JSON forms are driven through `Session::execute`, so the rows can move to brief 011's MCP
//! harness unchanged (brief scope 7).

use crate::common;

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::common::close_all;
use cadcraft_engine::doc::{Drawing, EntityKind};
use cadcraft_engine::ext::MarkShape;
use cadcraft_engine::geom::Vec2;
use cadcraft_engine::{Input, Session};
use common::{H, id};
use ecad_engine::interact::{hover_lines, overlay_marks};
use ecad_engine::references::{diagnostics, markers_on_page, plan_continue};
use ecad_engine::{HandleMap, Persistence, ProjectHandle, project_page};
use ecad_model::connectivity::derive::{cable_references, nets};
use ecad_model::{
    CableId, Core, Document, Form, Item, ItemId, ItemKind, Op, Page, PageId, Primitive, Pt, Representation, RepresentationId, Subject, ConnectionPointId,
    WireId,
};
use serde_json::{Value, json};

/// `wire` point to point (design 072: a wire runs between two connection points): a connection
/// point at each end that has none first ([`H::wire_pp`]).
fn wire_pp(h: &mut H, points: Value) -> Result<Value, String> {
    let p: Vec<(f64, f64)> = points.as_array().into_iter().flatten().filter_map(|q| Some((q[0].as_f64()?, q[1].as_f64()?))).collect();
    h.wire_pp(&p)
}

const SEED: u64 = 10_910;

// ---------------------------------------------------------------- fixture R (design §11)

struct R {
    h: H,
    supply: PageId,
    field: PageId,
    c1: CableId,
    w: [WireId; 3],
    k2: [ConnectionPointId; 3],
    /// The continued stubs on Field (design 072 §4.3: routes), with the free end of each.
    stubs: Vec<(ecad_model::RouteId, String)>,
}

fn set_tag(h: &mut H, key: &str, id: &str, value: &str) {
    // A wire has no tag (design 072 §4.2): it is named by its wire number.
    if key == "conductor" {
        h.run("wirenumber", json!({"conductor": id, "value": value})).unwrap();
    } else {
        h.run("setprop", json!({key: id, "key": "tag", "value": value})).unwrap();
    }
}

fn component(h: &mut H, page: PageId, outline: &str, at: [(f64, f64); 3], name: &str) -> [ConnectionPointId; 3] {
    let before: BTreeSet<_> = h.lines_on(page).into_iter().collect();
    h.cmdline(outline);
    let lines: Vec<_> = h.lines_on(page).into_iter().filter(|l| !before.contains(l)).collect();
    let v = h.run("convertcomponent", json!({"handles": lines.iter().map(|x| x.hex()).collect::<Vec<_>>()})).unwrap();
    let c = v["component"].as_str().unwrap().to_string();
    set_tag(h, "component", &c, name);
    at.map(|(x, y)| id(&h.run("addterminal", json!({"handle": lines[0].hex(), "at": [x, y]})).unwrap(), "terminal"))
}

/// Fixture R up to (not including) `cableref`. `attached`: wires start on K1's terminals (end A
/// attached) or start in free space (both ends loose).
fn fixture(seed: u64, attached: bool) -> R {
    let mut h = H::new(seed);
    let supply = h.first_page();
    h.run("renamepage", json!({"page": supply.hex(), "title": "Supply"})).unwrap();
    component(&mut h, supply, "line -10,-5 0,-5 0,15 -10,15 c", [(0.0, 0.0), (0.0, 5.0), (0.0, 10.0)], "K1");
    let x0 = if attached { 0.0 } else { 5.0 };
    let mut w = Vec::new();
    // Loose lines leaving the page (design 072 §4.4, F4): no wire until each is continued through
    // the cable's references and lands on K2; until then a core is named by its line.
    for y in [0.0, 5.0, 10.0] {
        let v = h.run("wire", json!({"points": [[x0, y], [60.0, y]]})).unwrap();
        let line: ecad_model::RouteId = id(&v, "route");
        w.push(WireId::parse(&line.hex()).unwrap());
    }
    let c1: CableId = id(&h.run("cableslash", json!({"from": [28, -3], "to": [32, 18], "tag": "C1"})).unwrap(), "cable");
    let field: PageId = id(&h.run("newpage", json!({"title": "Field"})).unwrap(), "page");
    let k2 = component(&mut h, field, "line 100,-5 110,-5 110,15 100,15 c", [(100.0, 0.0), (100.0, 5.0), (100.0, 10.0)], "K2");
    h.activate(supply);
    R { h, supply, field, c1, w: [w[0], w[1], w[2]], k2, stubs: Vec::new() }
}

fn referenced(seed: u64) -> R {
    let mut r = fixture(seed, true);
    r.h.run("cableref", json!({"cable": r.c1.hex(), "at": [65, 5]})).unwrap();
    r
}

/// [`referenced`] with C1's other reference end on Field at (20,5), written as `cablecontinue`
/// writes it ([`H::reference_end`]) but without its stubs: a continuation needs wires leaving the
/// page through the reference, which form only with lane F step F4. For tests about the
/// references themselves (texts, orphans, hostile documents, bounded derivation), not the wires.
fn paired(seed: u64) -> R {
    let mut r = referenced(seed);
    let field = r.field;
    r.h.reference_end(r.c1, field, (20.0, 5.0));
    r
}

fn continued(seed: u64) -> R {
    let mut r = referenced(seed);
    r.h.activate(r.field);
    let v = r.h.run("cablecontinue", json!({"cable": r.c1.hex(), "at": [20, 5], "toward": [40, 5]})).unwrap();
    r.stubs = v["routes"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|s| (id(s, "route"), s["free_end"].as_str().unwrap_or("B").to_string()))
        .collect();
    r
}

fn connected(seed: u64) -> R {
    let mut r = continued(seed);
    for ((s, end), t) in r.stubs.clone().into_iter().zip(r.k2) {
        r.h.run("connect", json!({"route": s.hex(), "end": end, "terminal": t.hex()})).unwrap();
    }
    // Now each core is a wire, from K1 to K2 across the pages: name them W0, W5, W10.
    let doc = r.h.doc();
    let wires: Vec<WireId> = doc.cables[&r.c1].cores.iter().filter_map(|c| c.wire()).collect();
    for (i, (w, y)) in wires.iter().zip(["0", "5", "10"]).enumerate() {
        set_tag(&mut r.h, "conductor", &w.hex(), &format!("W{y}"));
        r.w[i] = *w;
    }
    r
}

// ---------------------------------------------------------------- helpers

fn ref_on(h: &mut H, page: PageId) -> Option<RepresentationId> {
    let doc = h.doc();
    cable_references(&doc).into_iter().find(|x| x.page == page).map(|x| x.reference)
}

/// The text the page drawing actually shows for its reference (the Text entity mapped to it).
fn drawn_text(h: &mut H, page: PageId) -> Option<String> {
    // What `page` shows once it is the active page (brief 016: one page window). The page that
    // was active is made active again afterwards.
    let prev = h.p(|p| p.active_page());
    h.activate(page);
    let rid = ref_on(h, page);
    let hd = rid.and_then(|rid| h.p(|p| p.pages.get(&page).and_then(|v| v.map.ref_text(rid))));
    let text = hd.and_then(|hd| {
        h.pv(|p, pg| pg.entities_of(p, page)).into_iter().find(|e| e.handle == hd).and_then(|e| match &e.kind {
            EntityKind::Text(t) => Some(t.value.clone()),
            _ => None,
        })
    });
    if let Some(prev) = prev {
        h.activate(prev);
    }
    text
}

/// The derived text of `page`'s reference, from `cablerefs` (the agent's view).
fn listed_text(h: &mut H, page: PageId) -> String {
    let v = h.run("cablerefs", json!({})).unwrap();
    v["references"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["page"] == json!(page.hex()))
        .map(|x| x["text"].as_str().unwrap().to_string())
        .unwrap_or_default()
}

/// The page window's document, with `page` made the active page (brief 016).
fn uid(h: &mut H, page: PageId) -> u64 {
    h.activate(page);
    h.p(|p| p.pages[&page].uid).expect("page open")
}

fn last_ops(h: &mut H) -> Vec<Op> {
    h.p(|p| p.ledger.log().last().map(|t| t.ops.clone()).unwrap_or_default())
}

fn click(h: &mut H, x: f64, y: f64) {
    let at = Vec2::new(x, y);
    h.s.cursor = at;
    let _ = h.s.input(Input::Point(at));
}

fn prompt(h: &H) -> String {
    h.s.current_prompt().map(|p| p.display()).unwrap_or_default()
}

/// Handles of everything drawn on `page` for the cable's continuation there (stubs, slash,
/// reference point and text), not K2.
fn continuation_handles(h: &mut H, page: PageId) -> Vec<String> {
    let doc = h.doc();
    let map = h.p(|p| p.pages[&page].map.clone());
    let mut out = Vec::new();
    for (rid, r) in doc.representations.iter().filter(|(_, r)| r.page == page) {
        if matches!(r.subject, Subject::Device(_)) {
            continue;
        }
        out.extend(r.items.iter().filter_map(|i| map.item(*i)).map(|x| x.hex()));
        if let Some(t) = map.ref_text(*rid) {
            out.push(t.hex());
        }
    }
    out
}

struct TempDir(PathBuf);

impl TempDir {
    /// A folder with a space and non-ASCII letters, as Windows user folders often have.
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("ecad tester 010 Fält {name}-{}", ecad_store::disk::unique()));
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
    fn path(&self, name: &str) -> String {
        self.0.join(name).display().to_string()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn no_internal_error(e: &str, what: &str) {
    assert!(!e.to_lowercase().contains("internal error"), "{what}: a panic was caught by the guard: {e}");
}

// ---------------------------------------------------------------- AC1

/// AC1: one cable, members split between its drawings, `derive::nets` unchanged by splitting it.
/// Table over how the wires are attached and which members are listed; undo returns the exact
/// document before, redo the exact document after.
#[test]
fn ac1_splitting_a_cable_keeps_its_id_members_conductors_and_nets() {
    // (row, wires start on K1, `members` listed, routes expected on Field)
    let rows: Vec<(&str, bool, Option<[usize; 2]>, usize)> = vec![
        ("attached at A, all members", true, None, 3),
        ("both ends loose, all members", false, None, 3),
        ("attached at A, two members listed out of order", true, Some([2, 0]), 2),
    ];
    for (row, attached, members, want) in rows {
        let mut r = fixture(SEED + 1, attached);
        r.h.run("cableref", json!({"cable": r.c1.hex(), "at": [65, 5]})).unwrap();
        r.h.activate(r.field);
        let before = r.h.doc();
        let head = r.h.head();
        let mut params = json!({"cable": r.c1.hex(), "at": [20, 5], "toward": [40, 5]});
        if let Some(m) = members {
            params["members"] = json!(m.iter().map(|i| r.w[*i].hex()).collect::<Vec<_>>());
        }
        let v = r.h.run("cablecontinue", params).unwrap();
        assert_eq!(r.h.head(), head + 1, "{row}: one transaction");
        let after = r.h.doc();
        let positions = |d: &Document| d.cables[&r.c1].cores.iter().map(|c| c.position).collect::<Vec<_>>();
        assert_eq!(positions(&after), positions(&before), "{row}: same cable, same ID, same cores where they were");
        assert_eq!(after.wires, before.wires, "{row}: no conductor or end changed");
        assert_eq!(nets(&after), nets(&before), "{row}: nets unchanged by the split");
        let routes = v["routes"].as_array().unwrap();
        assert_eq!(routes.len(), want, "{row}");
        if let Some(m) = members {
            let cores = &after.cables[&r.c1].cores;
            let got: Vec<String> = routes
                .iter()
                .map(|x| {
                    let c = &cores[x["core"].as_u64().unwrap() as usize - 1];
                    c.wire().map(|w| w.hex()).or(c.route().map(|l| l.hex())).unwrap()
                })
                .collect();
            let order: Vec<String> = m.iter().map(|i| r.w[*i].hex()).collect();
            assert_eq!(got, order, "{row}: routes in the order given");
        }
        // Every continued core now has a line on both pages, joined through its two named points
        // (design 072 §4.4, F4).
        for x in routes {
            let stub: ecad_model::RouteId = id(x, "route");
            assert_eq!(after.routes[&stub].page, r.field, "{row}");
            let core = &after.cables[&r.c1].cores[x["core"].as_u64().unwrap() as usize - 1];
            assert_eq!(core.through.len(), 2, "{row}: {core:?}");
            let line = core.route().and_then(|l| after.routes.get(&l)).map(|l| l.page);
            assert_eq!(line, Some(r.supply), "{row}: {core:?}");
        }
        // Loose wires continue from the end nearer the Supply reference (B, at x = 60).
        if !attached {
            assert!(routes.iter().all(|x| x["free_end"] == "B"), "{row}: {routes:?}");
        }
        // New items, representations and stubs; besides them only the named points (design 072
        // §4.4, F4): this cable's cores listing them, Supply's reference placing them, and the
        // continued cores' Supply lines landing on them, their points unchanged.
        let ops = last_ops(&mut r.h);
        let source = ref_on(&mut r.h, r.supply).unwrap();
        for o in &ops {
            match o {
                Op::AddItem { .. } | Op::LinkRepresentation { .. } | Op::AddRoute { .. } => {}
                Op::ModifyCable { id, .. } => assert_eq!(*id, r.c1, "{row}"),
                Op::ModifyRepresentation { id, .. } => assert_eq!(*id, source, "{row}"),
                Op::ModifyRoute { before: b, after: a, .. } => {
                    assert_eq!(a.page, r.supply, "{row}");
                    assert_eq!(a.points, b.points, "{row}: a Supply line keeps its points");
                    assert!(a.ends.iter().any(|e| e.point().is_some_and(|t| after.named_points(t).is_some())), "{row}: {a:?}");
                }
                other => panic!("{row}: unexpected op {other:?}"),
            }
        }
        r.h.cmdline("u");
        assert_eq!(r.h.doc(), before, "{row}: undo returns the document before");
        r.h.cmdline("redo");
        assert_eq!(r.h.doc(), after, "{row}: redo returns the same continuation");
    }
}

/// AC1 after Connect: the cable drawn on two pages gives one net per wire, K1 to K2, and the
/// same net structure as the same circuit drawn on one page.
#[test]
fn ac1_two_page_cable_has_the_nets_of_the_one_page_circuit() {
    let mut r = connected(SEED + 2);
    let two = nets(&r.h.doc());
    let shape = |n: &[ecad_model::connectivity::derive::Net]| {
        let mut v: Vec<(usize, usize)> = n.iter().filter(|x| !x.wires.is_empty()).map(|x| (x.wires.len(), x.terminals.len())).collect();
        v.sort();
        v
    };
    // The same circuit on one page: K1 and K2 on Supply, wires terminal to terminal, one slash.
    let mut h = H::new(SEED + 3);
    let p = h.first_page();
    component(&mut h, p, "line -10,-5 0,-5 0,15 -10,15 c", [(0.0, 0.0), (0.0, 5.0), (0.0, 10.0)], "K1");
    component(&mut h, p, "line 100,-5 110,-5 110,15 100,15 c", [(100.0, 0.0), (100.0, 5.0), (100.0, 10.0)], "K2");
    for y in [0.0, 5.0, 10.0] {
        wire_pp(&mut h, json!([[0.0, y], [100.0, y]])).unwrap();
    }
    h.run("cableslash", json!({"from": [28, -3], "to": [32, 18], "tag": "C1"})).unwrap();
    let one = nets(&h.doc());
    assert_eq!(shape(&two), vec![(1, 2), (1, 2), (1, 2)]);
    assert_eq!(shape(&two), shape(&one));
    // The cable and its member list are the ones made by the slash on Supply.
    let doc = r.h.doc();
    assert_eq!(doc.cables.len(), 1);
    let members: Vec<WireId> = doc.cables[&r.c1].cores.iter().filter_map(|m| m.wire()).collect();
    assert_eq!(members, r.w.to_vec());
}

// ---------------------------------------------------------------- AC2

/// AC2: page moves and renames, by JSON and typed at the command line, are one transaction each;
/// both drawn texts, the derivation and `cablerefs` follow with no transaction of their own; undo
/// of a move puts the texts back.
#[test]
fn ac2_page_moves_and_renames_update_drawn_texts_with_no_transaction_of_their_own() {
    let mut r = paired(SEED + 4);
    let spare: PageId = id(&r.h.run("newpage", json!({"title": "Spare"})).unwrap(), "page");
    r.h.activate(r.supply);
    let (s, f, x) = (r.supply.hex(), r.field.hex(), spare.hex());
    // (step, how: JSON command | typed line, expected Supply text, expected Field text). Page
    // numbers are stored (brief 019), so moving or renaming pages changes no text.
    let rows: Vec<(&str, Result<(&str, Value), String>, &str, &str)> = vec![
        ("start", Ok(("cablerefs", json!({}))), "2/F1", "1/F2"),
        ("Spare first", Ok(("movepage", json!({"page": x, "index": 1}))), "2/F1", "1/F2"),
        ("Field first", Ok(("movepage", json!({"page": f, "index": 1}))), "2/F1", "1/F2"),
        ("rename Supply (unicode)", Ok(("renamepage", json!({"page": s, "title": "Fält ⚡ 電源"}))), "2/F1", "1/F2"),
        ("rename Field", Err(format!("renamepage {{\"page\":\"{f}\",\"title\":\"Field 2\"}}")), "2/F1", "1/F2"),
        ("Supply first, typed", Err(format!("movepage {{\"page\":\"{s}\",\"index\":1}}")), "2/F1", "1/F2"),
        ("undo the move", Err("u".to_string()), "2/F1", "1/F2"),
        ("redo the move", Err("redo".to_string()), "2/F1", "1/F2"),
        ("Field last", Ok(("movepage", json!({"page": f, "index": 3}))), "2/F1", "1/F2"),
    ];
    for (step, how, want_s, want_f) in rows {
        let head = r.h.head();
        let reads = match &how {
            Ok((cmd, params)) => {
                r.h.run(cmd, params.clone()).unwrap();
                *cmd == "cablerefs"
            }
            Err(line) => {
                r.h.cmdline(line);
                false
            }
        };
        assert!(r.h.s.running.is_none(), "{step}: never prompts");
        assert_eq!(r.h.head(), head + u64::from(!reads), "{step}: exactly one transaction (none for a read)");
        let head = r.h.head();
        for (page, want) in [(r.supply, want_s), (r.field, want_f)] {
            assert_eq!(listed_text(&mut r.h, page), want, "{step}: cablerefs");
            assert_eq!(drawn_text(&mut r.h, page).as_deref(), Some(want), "{step}: the drawing shows it");
            let doc = r.h.doc();
            assert_eq!(markers_on_page(&doc, page)[0].text, want, "{step}: derived");
        }
        assert_eq!(r.h.head(), head, "{step}: the texts recorded nothing");
    }
    // Renames show in the hover (the title), not in the marker text (design Q4).
    let rid = ref_on(&mut r.h, r.field).unwrap();
    let lines = ecad_engine::references::hover_lines(&r.h.doc(), rid).unwrap();
    assert!(lines.iter().any(|l| l.contains("\"Fält ⚡ 電源\"")), "{lines:?}");
    // Same title, same position: no transaction.
    let head = r.h.head();
    let v = r.h.run("renamepage", json!({"page": s, "title": "Fält ⚡ 電源"})).unwrap();
    assert_eq!(v["changed"], false);
    let idx = r.h.doc().page_order.iter().position(|p| *p == r.supply).unwrap() + 1;
    let v = r.h.run("movepage", json!({"page": s, "index": idx})).unwrap();
    assert_eq!(v["changed"], false);
    assert_eq!(r.h.head(), head);
}

/// AC2: moving one end's point (MOVE) changes the other end's column; the moved end's own text
/// keeps naming its partner.
#[test]
fn ac2_moving_a_reference_point_changes_the_other_ends_column() {
    let mut r = connected(SEED + 5);
    r.h.activate(r.supply);
    let rid = ref_on(&mut r.h, r.supply).unwrap();
    let point = r.h.doc().representations[&rid].items[0];
    let hp = r.h.handle_of(r.supply, point);
    // (delta, expected Field text after it)
    for (dx, want) in [(40.0, "1/F2"), (-200.0, "1/F1"), (1e6, "1/F8")] {
        let head = r.h.head();
        r.h.run("move", json!({"handles": [hp.hex()], "delta": [dx, 0]})).unwrap();
        assert_eq!(r.h.head(), head + 1);
        assert_eq!(drawn_text(&mut r.h, r.field).as_deref(), Some(want), "after moving by {dx}");
        assert_eq!(drawn_text(&mut r.h, r.supply).as_deref(), Some("2/F1"));
    }
}

// ---------------------------------------------------------------- AC3

/// AC3: erasing only the other end's reference point makes this end an orphan (red ring, red
/// `?`, diagnostic); undo restores the link with the same IDs; redo orphans it again.
#[test]
fn ac3_erasing_the_other_end_orphans_this_one_and_undo_redo_round_trip() {
    let mut r = paired(SEED + 6);
    let ring = |h: &mut H, page: PageId| {
        let u = uid(h, page);
        h.p(|p| overlay_marks(p, u)).into_iter().filter(|m| m.shape == MarkShape::Ring && m.stroke == 2.0).collect::<Vec<_>>()
    };
    let linked_colour = ring(&mut r.h, r.supply)[0].color;
    r.h.activate(r.field);
    let fid = ref_on(&mut r.h, r.field).unwrap();
    let point = r.h.doc().representations[&fid].items[0];
    let hp = r.h.handle_of(r.field, point);
    let linked = r.h.doc();
    r.h.run("erase", json!({"handles": [hp.hex()]})).unwrap();
    let orphaned = r.h.doc();
    assert!(ref_on(&mut r.h, r.field).is_none(), "Field's reference is gone");
    assert_eq!(drawn_text(&mut r.h, r.supply).as_deref(), Some("?"));
    // Brief 040: an orphan is the painted error disc, not a ring in another colour.
    assert!(ring(&mut r.h, r.supply).is_empty(), "no linked ring");
    let su = uid(&mut r.h, r.supply);
    let painted = r.h.p(|p| ecad_engine::cables::draw::painted_marks(p, su));
    assert!(painted.iter().any(|m| matches!(m, ecad_engine::cables::draw::Painted::Error(_))), "{painted:?}");
    let sid = ref_on(&mut r.h, r.supply).unwrap();
    let text = r.h.p(|p| p.pages[&r.supply].map.ref_text(sid)).unwrap();
    let e = r.h.pv(|p, pg| pg.entities_of(p, r.supply)).into_iter().find(|e| e.handle == text).unwrap();
    assert_eq!(format!("{:?}", e.common.color), format!("{:?}", cadcraft_engine::doc::color::Color::Index(1)), "red text");
    assert_eq!(diagnostics(&orphaned), vec!["Orphan: cable -C01 on page 1 (?)".to_string()]);
    // The wires and their attachments stay: only the reference went.
    assert_eq!(orphaned.wires, linked.wires);
    assert_eq!(nets(&orphaned), nets(&linked));
    r.h.cmdline("u");
    assert_eq!(r.h.doc(), linked, "undo restores the link, same IDs");
    assert_eq!(drawn_text(&mut r.h, r.supply).as_deref(), Some("2/F1"));
    assert_eq!(ring(&mut r.h, r.supply)[0].color, linked_colour);
    assert_eq!(diagnostics(&r.h.doc()), vec!["No orphan references.".to_string()]);
    r.h.cmdline("redo");
    assert_eq!(r.h.doc(), orphaned, "redo orphans it again");
}

/// AC3 / design §9 with named points (design 072 §4.4, F4): window-erasing the whole Field
/// drawing orphans Supply and keeps Supply's lines and the cable's cores, but the wires through
/// Field's points go with Field's stubs (a wire runs only from device point to device point);
/// `ecaddelete` of the cable removes both references and keeps every line on both pages;
/// `ecaddelete` of a continued conductor removes its route on every page.
#[test]
fn ac3_erase_and_delete_of_cable_drawings_follow_the_section_9_table() {
    let mut r = connected(SEED + 7);
    r.h.activate(r.field);
    let linked = r.h.doc();
    let supply_lines: Vec<ecad_model::RouteId> = r.w.iter().map(|w| r.h.route_of(r.supply, *w)).collect();
    let stubs: Vec<ecad_model::RouteId> = r.stubs.iter().map(|(s, _)| *s).collect();
    let handles = continuation_handles(&mut r.h, r.field);
    r.h.run("erase", json!({"handles": handles})).unwrap();
    let doc = r.h.doc();
    assert!(doc.wires.is_empty(), "no wire without Field's lines: {:?}", doc.wires.keys());
    assert!(stubs.iter().all(|s| !doc.routes.contains_key(s)), "Field's stubs erased");
    for l in &supply_lines {
        assert_eq!(doc.routes[l].points, linked.routes[l].points, "Supply's line stays");
    }
    assert_eq!(doc.cables[&r.c1].cores.len(), linked.cables[&r.c1].cores.len(), "the cable keeps its cores");
    assert_eq!(drawn_text(&mut r.h, r.supply).as_deref(), Some("?"));
    r.h.cmdline("u");
    assert_eq!(r.h.doc(), linked);
    // ecaddelete {cable}: both references and slashes go; the lines on both pages stay.
    r.h.run("ecaddelete", json!({"cable": r.c1.hex()})).unwrap();
    let doc = r.h.doc();
    assert!(cable_references(&doc).is_empty(), "both references removed");
    for l in supply_lines.iter().chain(&stubs) {
        assert_eq!(doc.routes.get(l).map(|x| &x.points), linked.routes.get(l).map(|x| &x.points), "lines stay on both pages");
    }
    r.h.cmdline("u");
    assert_eq!(r.h.doc(), linked);
    // A continued wire runs as a line on each page (design 072 §4.4, F4): `ecaddelete {conductor}`
    // refuses it as any wire drawn as more than one line; erasing its line on each page removes
    // it, and the references stay linked.
    let head = r.h.head();
    let e = r.h.run("ecaddelete", json!({"conductor": r.w[1].hex()}));
    assert_eq!(e, Err("This wire is not drawn as one line between its two points; erase the lines it runs along instead.".to_string()));
    assert_eq!(r.h.head(), head);
    for (page, line) in [(r.supply, supply_lines[1]), (r.field, stubs[1])] {
        r.h.activate(page);
        let h = r.h.line_handle(page, line);
        r.h.run("erase", json!({"handles": [h.hex()]})).unwrap();
    }
    let doc = r.h.doc();
    assert!(!doc.wires.contains_key(&r.w[1]), "the wire is gone");
    assert!(ecad_engine::references::route_pages(&doc, r.w[1]).is_empty());
    assert_eq!(drawn_text(&mut r.h, r.supply).as_deref(), Some("2/F1"));
    let v = r.h.run("cablerefs", json!({})).unwrap();
    assert!(v["references"].as_array().unwrap().iter().all(|x| x["wires_here"] == 2), "{v}");
}

// ---------------------------------------------------------------- AC4

/// AC4: save to disk (a folder with a space and non-ASCII letters), close, reopen: the same
/// document, the same `cablerefs`, the same drawn texts on both pages; replay equals live; the
/// snapshot at every earlier sequence number replays without error.
#[test]
fn ac4_save_close_reopen_and_replay_give_the_same_cable_drawings_and_references() {
    let dir = TempDir::new("reopen");
    let mut r = connected(SEED + 8);
    r.h.run("movepage", json!({"page": r.field.hex(), "index": 1})).unwrap();
    r.h.run("renamepage", json!({"page": r.supply.hex(), "title": "Fält ⚡ 電源"})).unwrap();
    r.h.cfg(|p| p.persistence = Persistence::File);
    let path = dir.path("two pages.ecad");
    r.h.run("saveas", json!({"path": path})).unwrap();
    let doc = r.h.doc();
    let refs = r.h.run("cablerefs", json!({})).unwrap();
    let texts = [drawn_text(&mut r.h, r.supply), drawn_text(&mut r.h, r.field)];
    assert_eq!(texts, [Some("2/F1".to_string()), Some("1/F2".to_string())]);
    let head = r.h.head();
    for seq in 0..=head {
        let snap = r.h.project.snapshot(&mut r.h.s, Some(seq));
        assert!(snap.is_ok(), "snapshot at {seq}: {snap:?}");
    }
    close_all(&mut r.h.s);
    drop(r);
    let mut s = Session::empty();
    let (project, summary) = ProjectHandle::open_file(&mut s, &path, SEED).unwrap();
    assert!(!summary.contains("read-only"), "{summary}");
    let mut h = H { s, project, trail: Vec::new() };
    assert_eq!(h.doc(), doc, "same document");
    assert!(h.p(|p| p.ledger.replay_matches().unwrap()), "replay equals live");
    assert_eq!(h.run("cablerefs", json!({})).unwrap(), refs, "same references");
    for page in doc.page_order.clone() {
        if h.p(|p| p.pages.get(&page).and_then(|v| v.uid)).is_none() {
            h.run("openpage", json!({"page": page.hex()})).unwrap();
        }
    }
    let supply = doc.page_order[1];
    let field = doc.page_order[0];
    assert_eq!([drawn_text(&mut h, supply), drawn_text(&mut h, field)], texts, "same drawn texts");
    h.check();
}

// ---------------------------------------------------------------- AC5

/// AC5: `cablegoto` never records, never prompts, opens a closed target page and selects the other
/// end; it works on a read-only project, where `cableref`/`cablecontinue` are refused (R8).
#[test]
fn ac5_cablegoto_opens_a_closed_page_and_works_read_only() {
    let dir = TempDir::new("goto");
    let mut r = connected(SEED + 9);
    r.h.cfg(|p| p.persistence = Persistence::File);
    let path = dir.path("shared.ecad");
    r.h.run("saveas", json!({"path": path})).unwrap();
    // Close Field's tab, then jump to it from Supply by the typed JSON form.
    let fu = uid(&mut r.h, r.field);
    let idx = r.h.s.docs.iter().position(|d| d.uid == fu).unwrap();
    r.h.s.execute("close", &json!({"index": idx})).unwrap();
    assert!(r.h.p(|p| p.pages.get(&r.field).and_then(|v| v.uid)).is_none(), "Field is closed");
    r.h.activate(r.supply);
    let sid = ref_on(&mut r.h, r.supply).unwrap();
    let head = r.h.head();
    r.h.cmdline(&format!("cablegoto {{\"reference\":\"{}\"}}", sid.hex()));
    assert!(r.h.s.running.is_none(), "never prompts");
    assert_eq!(r.h.head(), head, "records nothing");
    let fu = uid(&mut r.h, r.field);
    assert_eq!(r.h.s.state().unwrap().uid, fu, "Field opened and current");
    let fid = ref_on(&mut r.h, r.field).unwrap();
    let point = r.h.doc().representations[&fid].items[0];
    assert_eq!(r.h.s.selection(), vec![r.h.handle_of(r.field, point)], "the other end is selected");
    close_all(&mut r.h.s);
    drop(r);
    // A second session opens the same file read-only.
    let mut s1 = Session::empty();
    let (_p1, _) = ProjectHandle::open_file(&mut s1, &path, 1).unwrap();
    let mut s = Session::empty();
    let (project, summary) = ProjectHandle::open_file(&mut s, &path, 2).unwrap();
    assert!(summary.contains("read-only"), "{summary}");
    let mut h = H { s, project, trail: Vec::new() };
    let doc = h.doc();
    let refs = cable_references(&doc);
    let (s_ref, f_ref) = (refs[0].reference, refs[1].reference);
    let head = h.head();
    let v = h.run("cablegoto", json!({"reference": s_ref.hex()})).unwrap();
    assert_eq!(v["reference"], json!(f_ref.hex()));
    assert_eq!(h.head(), head);
    assert!(h.run("cablerefs", json!({})).is_ok());
    // Editing commands follow `wire` on a read-only project (as brief 004's slash): refused, or
    // edits in memory only; never a crash, and never the file.
    let file = std::fs::read(&path).unwrap();
    let w = h.run("wire", json!({"points": [[0, 40], [60, 40]]})).is_ok();
    let cr = h.run("cableref", json!({"cable": refs[0].cable.hex(), "at": [80, 5]}));
    if !w {
        assert!(cr.is_err(), "cableref refused like wire on a read-only project: {cr:?}");
    }
    assert!(h.run("qsave", Value::Null).is_err(), "Save refused read-only");
    assert_eq!(std::fs::read(&path).unwrap(), file, "the other session's file is untouched");
}

/// AC5: the hover lens on a reference and on member wires names every drawing of the cable.
#[test]
fn ac5_hover_on_member_wires_names_the_other_drawing() {
    let mut r = connected(SEED + 10);
    for (page, other) in [(r.supply, "Also drawn on page 2"), (r.field, "Also drawn on page 1")] {
        r.h.activate(page);
        let route = r.h.route_handle(page, r.w[0]);
        let u = uid(&mut r.h, page);
        let lines = r.h.p(|p| hover_lines(p, u, route)).unwrap();
        assert!(lines.iter().any(|l| l.contains(other)), "{lines:?}");
        assert!(lines.len() <= 12, "{lines:?}");
    }
    // The slash on Field lists both pages and both references.
    let doc = r.h.doc();
    let slash = doc.representations.iter().find(|(_, x)| x.page == r.field && x.role == ecad_model::Role::Slash).map(|(_, x)| x.items[0]).unwrap();
    let hs = r.h.handle_of(r.field, slash);
    let u = uid(&mut r.h, r.field);
    let lines = r.h.p(|p| hover_lines(p, u, hs)).unwrap();
    assert!(lines.iter().any(|l| l.contains("Drawn on: page 1, page 2")), "{lines:?}");
    assert!(lines.iter().any(|l| l.contains("1/F2") && l.contains("2/F1")), "{lines:?}");
}

// ---------------------------------------------------------------- one route per conductor per page

/// Design §0: code must work per page now a conductor has a route on two pages. Disconnect and
/// Connect on Field act on Field only; moving K2 drags only the Field stub ends (the harness
/// asserts no mismatch on any page after each step); a slash across the stubs is refused.
#[test]
fn per_page_edits_touch_only_the_page_they_are_made_on() {
    let mut r = connected(SEED + 11);
    r.h.activate(r.field);
    // Disconnecting the stub ends the wire through it and connecting makes it again (design 072
    // §4.3: a wire is its lines'), so Supply's line is followed by its route ID.
    let line = r.h.route_of(r.supply, r.w[0]);
    let supply_route = r.h.doc().routes[&line].clone();
    let (stub, end) = r.stubs[0].clone();
    r.h.run("disconnect", json!({"route": stub.hex(), "end": end})).unwrap();
    assert_eq!(r.h.doc().routes[&line], supply_route, "Supply's route untouched");
    r.h.run("connect", json!({"route": stub.hex(), "end": end, "terminal": r.k2[0].hex()})).unwrap();
    // Move K2 (its outline lines) down 20: Field's stub ends follow, Supply's routes do not.
    let doc = r.h.doc();
    let k2 = doc.terminals[&r.k2[0]].device().unwrap();
    let handles = r.h.rep_handles(r.field, k2);
    r.h.run("move", json!({"handles": handles, "delta": [0, -20]})).unwrap();
    assert_eq!(r.h.doc().routes[&line], supply_route, "Supply's route untouched by the drag");
    let b = r.h.doc().routes[&stub].end_points().map(|[_, b]| b);
    assert_eq!(b, Some(r.h.connection_point_pos(r.field, r.k2[0])), "Field's stub end followed K2");
    // A slash across the Field stubs: they are already members of C1.
    let head = r.h.head();
    let e = r.h.run("cableslash", json!({"from": [28, -30], "to": [32, 30]}));
    assert!(e.is_err(), "{e:?}");
    assert_eq!(r.h.head(), head);
    let n = nets(&r.h.doc());
    assert_eq!(n.iter().filter(|x| !x.wires.is_empty()).count(), 3);
}

// ---------------------------------------------------------------- AC6

/// AC6: Esc or Enter at every step of the interactive forms writes nothing; `U` steps back.
#[test]
fn ac6_escape_and_undo_steps_of_the_interactive_forms_write_nothing() {
    let mut r = referenced(SEED + 12);
    r.h.activate(r.field);
    let _ = r.h.s.zoom_extents();
    let before = r.h.doc();
    let head = r.h.head();
    // (keys after starting, then the ending key)
    for end in [Input::Cancel, Input::Enter] {
        r.h.s.start("cablecontinue").unwrap();
        r.h.s.input(Input::Enter).unwrap();
        assert_eq!(prompt(&r.h), "Specify reference point:");
        click(&mut r.h, 20.0, 5.0);
        assert!(prompt(&r.h).starts_with("Specify direction and length"), "{}", prompt(&r.h));
        r.h.s.cmdline("U").unwrap(); // typed, as a user does
        assert_eq!(prompt(&r.h), "Specify reference point:", "U drops the point");
        click(&mut r.h, 20.0, 5.0);
        let _ = r.h.s.input(end.clone());
        if r.h.s.running.is_some() {
            let _ = r.h.s.input(Input::Cancel);
        }
        assert!(r.h.s.running.is_none(), "{end:?} ends the command");
        assert_eq!(r.h.head(), head, "{end:?}: nothing written");
        assert_eq!(r.h.doc(), before);
        r.h.check();
    }
    r.h.activate(r.supply);
    r.h.s.start("cableref").unwrap();
    click(&mut r.h, 30.0, 7.5);
    let _ = r.h.s.input(Input::Cancel);
    assert!(r.h.s.running.is_none());
    assert_eq!(r.h.head(), head, "Esc in cableref writes nothing");
}

/// AC6: every JSON form, given hostile values in each parameter (and as a whole), either refuses
/// in words or records at most one transaction; it never prompts and never panics (a panic
/// caught by the engine's guard shows as an internal error and fails here). The harness checks
/// replay, projection and handle maps after every call.
#[test]
fn ac6_hostile_json_values_in_every_parameter_never_panic_or_prompt() {
    let mut r = continued(SEED + 13);
    r.h.activate(r.supply);
    let c1 = r.c1.hex();
    let sref = ref_on(&mut r.h, r.supply).unwrap().hex();
    let page = r.supply.hex();
    let long = "x".repeat(100_000);
    let garbage: Vec<Value> = vec![
        Value::Null,
        json!(true),
        json!(0),
        json!(-1),
        json!(1.5),
        json!(1e308),
        json!(-1e308),
        json!(u64::MAX),
        json!(i64::MIN),
        json!(""),
        json!("zz"),
        json!("0"),
        json!("ffffffffffffffffffffffffffffffffffffffff"),
        json!("-1"),
        json!("😀 ⚡ Fält"),
        json!("\u{0}\u{7}\r\n"),
        json!(long),
        json!([]),
        json!([1]),
        json!([1, 2, 3]),
        json!([1e308, 1e308]),
        json!([-1e12, 1e12]),
        json!([1e12, 1e12]),
        json!(["1", "2"]),
        json!([null, null]),
        json!([[1, 2], [3, 4]]),
        json!({}),
        json!({"x": 1, "y": 2}),
    ];
    let bases: Vec<(&str, Value)> = vec![
        ("cableref", json!({"cable": c1, "at": [80, 5]})),
        ("cablecontinue", json!({"cable": c1, "at": [20, 50], "toward": [40, 50], "members": [r.w[0].hex()]})),
        ("cablegoto", json!({"reference": sref})),
        ("cablerefs", json!({"cable": c1})),
        ("renamepage", json!({"page": page, "title": "T"})),
        ("movepage", json!({"page": page, "index": 2})),
    ];
    let mut calls = 0;
    for (cmd, base) in &bases {
        let keys: Vec<String> = base.as_object().unwrap().keys().cloned().collect();
        let mut cases: Vec<Value> = garbage.clone();
        for k in &keys {
            for g in &garbage {
                let mut p = base.clone();
                p[k.as_str()] = g.clone();
                cases.push(p);
            }
        }
        for params in cases {
            let head = r.h.head();
            let what = format!("{cmd} {}", params.to_string().chars().take(160).collect::<String>());
            let res = r.h.run(cmd, params);
            calls += 1;
            assert!(r.h.s.running.is_none(), "{what}: prompted");
            match res {
                Err(e) => {
                    no_internal_error(&e, &what);
                    assert!(!e.trim().is_empty(), "{what}: empty error");
                    assert_eq!(r.h.head(), head, "{what}: a refusal recorded something");
                }
                Ok(_) => {
                    // An accepted call (for example a valid title): undo it to keep later rows comparable.
                    if r.h.head() > head {
                        r.h.cmdline("u");
                    }
                }
            }
            r.h.activate(r.supply);
        }
    }
    assert!(calls > 300, "{calls}");
    assert!(!r.h.s.log.iter().any(|l| l.contains("Internal error")), "a guarded panic was logged");
}

fn ref_rep(subject: Subject, page: PageId, items: Vec<ItemId>) -> Representation {
    Representation { subject, page, form: Form::Drawn, role: ecad_model::Role::Reference, items, connection_points: Default::default() }
}

/// Add a reference representation holding one Point item to a hand-built document.
fn add_ref(doc: &mut Document, n: u128, at: Pt, page: PageId, subject: Subject) {
    let style = doc.items.values().next().unwrap().style.clone();
    let item = ItemId::from(0xdead_0000 + n);
    doc.items.insert(item, Item { page, style, kind: ItemKind::Primitive(Primitive::Point { at }) });
    doc.representations.insert(RepresentationId::from(0xbeef_0000 + n), ref_rep(subject, page, vec![item]));
}

/// AC6: hand-built (hostile) documents, as a file could hold, derive, draw, hover, diagnose and
/// inspect without panicking: references with no item, a Line item, a non-finite or huge point,
/// a missing cable, a missing member conductor, several on one page, a page in no page order,
/// one item shared by two references, and many references of one cable (bounded time).
#[test]
fn ac6_hostile_documents_never_panic() {
    let base = paired(SEED + 14).h.doc();
    let cable = *base.cables.keys().next().unwrap();
    let page = base.page_order[0];
    type Mutate = Box<dyn Fn(&mut Document)>;
    let rows: Vec<(&str, Mutate)> = vec![
        (
            "no item",
            Box::new(move |d| {
                d.representations.insert(RepresentationId::from(0xbeef_0001), ref_rep(Subject::Cable(cable), page, vec![]));
            }),
        ),
        (
            "a Line item",
            Box::new(move |d| {
                let item = ItemId::from(0xdead_0002);
                d.items.insert(
                    item,
                    Item {
                        page,
                        style: d.items.values().next().unwrap().style.clone(),
                        kind: ItemKind::Primitive(Primitive::Line { a: Pt::new(0.0, 0.0), b: Pt::new(1.0, 1.0) }),
                    },
                );
                d.representations.insert(RepresentationId::from(0xbeef_0002), ref_rep(Subject::Cable(cable), page, vec![item]));
            }),
        ),
        ("NaN point", Box::new(move |d| add_ref(d, 3, Pt::new(f64::NAN, 0.0), page, Subject::Cable(cable)))),
        ("infinite point", Box::new(move |d| add_ref(d, 4, Pt::new(f64::INFINITY, f64::NEG_INFINITY), page, Subject::Cable(cable)))),
        ("huge point", Box::new(move |d| add_ref(d, 5, Pt::new(-1e300, 1e300), page, Subject::Cable(cable)))),
        ("missing cable", Box::new(move |d| add_ref(d, 6, Pt::new(1.0, 1.0), page, Subject::Cable(CableId::from(77))))),
        (
            "missing member conductor",
            Box::new(move |d| {
                if let Some(c) = d.cables.get_mut(&cable) {
                    c.cores.push(Core { position: ecad_model::CorePosition::Wire(WireId::from(88)), ..c.cores[0].clone() });
                }
            }),
        ),
        (
            "three on one page",
            Box::new(move |d| {
                add_ref(d, 7, Pt::new(10.0, 0.0), page, Subject::Cable(cable));
                add_ref(d, 8, Pt::new(10.0, 0.0), page, Subject::Cable(cable));
            }),
        ),
        (
            "page in no order",
            Box::new(move |d| {
                let lost = PageId::from(99);
                d.pages.insert(lost, Page::titled("lost").unwrap());
                add_ref(d, 9, Pt::new(0.0, 0.0), lost, Subject::Cable(cable));
            }),
        ),
        ("page missing entirely", Box::new(move |d| add_ref(d, 10, Pt::new(0.0, 0.0), PageId::from(98), Subject::Cable(cable)))),
        (
            "one item in two references",
            Box::new(move |d| {
                add_ref(d, 11, Pt::new(3.0, 3.0), page, Subject::Cable(cable));
                let item = ItemId::from(0xdead_0000 + 11);
                d.representations.insert(RepresentationId::from(0xbeef_1011), ref_rep(Subject::Cable(cable), page, vec![item]));
            }),
        ),
        (
            "reference of a conductor, not a cable",
            Box::new(move |d| {
                // The paired fixture's lines are loose (no wire until continued, F4): a wire ID
                // that names nothing is as hostile.
                let w = d.wires.keys().next().copied().unwrap_or(WireId::from(0xbad_0012));
                add_ref(d, 12, Pt::new(3.0, 3.0), page, Subject::Wire(w));
            }),
        ),
        (
            "a cable with no members",
            Box::new(move |d| {
                if let Some(c) = d.cables.get_mut(&cable) {
                    c.cores.clear();
                }
            }),
        ),
        (
            "empty title, all pages unordered",
            Box::new(move |d| {
                for p in d.pages.values_mut() {
                    p.title = ecad_model::PageTitle::new("-").unwrap();
                }
                d.page_order.clear();
            }),
        ),
        (
            "1 000 references of one cable",
            Box::new(move |d| {
                for n in 0..1_000u128 {
                    add_ref(d, 100_000 + n, Pt::new(n as f64, 0.0), page, Subject::Cable(cable));
                }
            }),
        ),
    ];
    for (row, mutate) in rows {
        let mut doc = base.clone();
        mutate(&mut doc);
        let started = Instant::now();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let refs = cable_references(&doc);
            let pages: Vec<PageId> = doc.pages.keys().copied().collect();
            for p in &pages {
                let _ = markers_on_page(&doc, *p);
                // Combined trial of 009 and 010: 010's leaving ends are folded into 009's drawing.
                let _ = ecad_engine::cables::views::drawings_on_page(&doc, *p, &Default::default(), 1e-6);
                let _ = project_page(
                    &doc,
                    *p,
                    &Drawing::new_metric(),
                    &HandleMap::default(),
                    ecad_engine::project::engine_views().report_models(&doc).as_ref(),
                );
                let _ = plan_continue(&doc, *p, cable, None, Pt::new(0.0, 0.0), Pt::new(10.0, 0.0), 1e-6);
                let _ = ecad_engine::references::check_reference(&doc, *p, cable);
            }
            let _ = diagnostics(&doc);
            for r in refs.iter().take(50) {
                let _ = ecad_engine::references::hover_lines(&doc, r.reference);
            }
            let _ = ecad_electrical::references::cables::references_json(&doc, &refs);
            let _ = ecad_electrical::references::cables::over_referenced(&refs);
            refs.len()
        }));
        assert!(result.is_ok(), "{row}: panicked");
        assert!(started.elapsed() < Duration::from_secs(60), "{row}: took {:?}", started.elapsed());
    }
}

/// Design §4.4 R9 / check J: a large cable continues in bounded time, and the hover on its
/// reference stays capped at 12 lines.
#[test]
#[ignore = "slow: a 1,000-wire cable, about 16 s"]
fn ac6_a_large_cable_continues_in_bounded_time() {
    const N: usize = 1_000;
    let mut h = H::new(SEED + 15);
    let supply = h.first_page();
    let mut wires = Vec::with_capacity(N);
    for i in 0..N {
        let y = i as f64;
        // Straight through the session: the harness's per-step checks would make this quadratic.
        h.points_at(&[(0.0, y), (60.0, y)]);
        let v = h.s.execute("wire", &json!({"points": [[0.0, y], [60.0, y]]})).unwrap();
        wires.push(v["conductor"].as_str().unwrap().to_string());
    }
    let c: CableId = id(&h.run("cableadd", json!({"conductors": wires, "tag": "BIG"})).unwrap(), "cable");
    h.run("cableref", json!({"cable": c.hex(), "at": [65, 5]})).unwrap();
    let field: PageId = id(&h.run("newpage", json!({"title": "Field"})).unwrap(), "page");
    h.activate(field);
    let started = Instant::now();
    let v = h.run("cablecontinue", json!({"cable": c.hex(), "at": [20, 5], "toward": [40, 5]})).unwrap();
    let took = started.elapsed();
    assert_eq!(v["routes"].as_array().map(Vec::len), Some(N));
    assert!(took < Duration::from_secs(60), "continuing {N} wires took {took:?}");
    let rid = ref_on(&mut h, supply).unwrap();
    h.activate(supply);
    let u = uid(&mut h, supply);
    let text = h.p(|p| p.pages[&supply].map.ref_text(rid)).unwrap();
    let started = Instant::now();
    let lines = h.p(|p| hover_lines(p, u, text)).unwrap();
    assert!(lines.len() <= 12, "{}", lines.len());
    assert!(started.elapsed() < Duration::from_secs(5), "hover took {:?}", started.elapsed());
}

/// Design §9 and §0 (at most two references, made only by the commands): COPY of a reference
/// point, of its text, or of the whole Field drawing never gives a cable a third reference or a
/// second reference on one page; every such call is refused or leaves the references as they were.
#[test]
fn copying_reference_drawings_never_makes_a_third_reference() {
    // (row, page, what to select)
    let rows: Vec<(&str, bool)> = vec![("the reference point", false), ("the whole continuation", true)];
    for (row, whole) in rows {
        let mut r = connected(SEED + 16);
        r.h.activate(r.field);
        let fid = ref_on(&mut r.h, r.field).unwrap();
        let handles: Vec<String> = if whole {
            continuation_handles(&mut r.h, r.field)
        } else {
            let point = r.h.doc().representations[&fid].items[0];
            vec![r.h.handle_of(r.field, point).hex()]
        };
        let before = cable_references(&r.h.doc()).len();
        let res = r.h.run("copy", json!({"handles": handles, "delta": [0, 40]}));
        if let Err(e) = &res {
            no_internal_error(e, row);
        }
        let doc = r.h.doc();
        let refs = cable_references(&doc);
        assert_eq!(refs.len(), before, "{row}: COPY {res:?} changed the number of references: {refs:#?}");
        assert!(ecad_electrical::references::cables::over_referenced(&refs).is_empty(), "{row}: over-referenced after COPY");
    }
    // The text: refused as derived.
    let mut r = connected(SEED + 17);
    r.h.activate(r.field);
    let fid = ref_on(&mut r.h, r.field).unwrap();
    let text = r.h.p(|p| p.pages[&r.field].map.ref_text(fid)).unwrap();
    let head = r.h.head();
    for (cmd, params) in
        [("copy", json!({"handles": [text.hex()], "delta": [0, 40]})), ("rotate", json!({"handles": [text.hex()], "base": [0, 0], "angle": 90}))]
    {
        let res = r.h.run(cmd, params);
        if let Err(e) = &res {
            no_internal_error(e, cmd);
        }
        assert_eq!(r.h.head(), head, "{cmd} of the derived text recorded something: {res:?}");
    }
}

/// Design §4.5 step 1 / AC5: "Picking a reference's point or text jumps". On the continued end
/// the middle stub starts exactly at the reference point (design §5.2, odd k), so a click on the
/// point must still pick the reference, not the stub. Also a click on the text jumps.
#[test]
fn ac5_clicking_the_continued_ends_point_or_text_jumps() {
    // (row, page the click is on, where)
    let rows: Vec<(&str, bool, (f64, f64))> = vec![
        ("Field's point, where the middle stub starts", true, (20.0, 5.0)),
        // Combined trial with 009: the text sits in 009's flag, just right of and above the point.
        ("Field's text", true, (21.5, 6.0)),
        ("Supply's point", false, (65.0, 5.0)),
        ("Supply's text", false, (66.5, 6.0)),
    ];
    let mut failures = Vec::new();
    for (row, on_field, (x, y)) in rows {
        let mut r = connected(SEED + 18);
        let (from, to) = if on_field { (r.field, r.supply) } else { (r.supply, r.field) };
        r.h.s.viewport_px = (1280.0, 800.0);
        r.h.activate(from);
        let _ = r.h.s.zoom_extents();
        r.h.s.set_selection(Vec::new());
        r.h.s.start("cablegoto").unwrap();
        assert_eq!(prompt(&r.h), "Select a cable reference:");
        let n = r.h.s.log.len();
        click(&mut r.h, x, y);
        let want = uid(&mut r.h, to);
        let now = r.h.s.state().unwrap().uid;
        if now != want {
            failures.push(format!("{row}: no jump; said {:?}", r.h.s.log.get(n..).unwrap_or_default()));
        }
        r.h.s.cancel();
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Never-crash (input-sized work is bounded): a hostile file whose one cable has many references.
/// Round 1 measured the derivation growing with the square of the count (20 000 references: 3.9 s
/// and 400 million partner entries, about 6.4 GB, on every draw). Round 2: each reference keeps at
/// most 3 partner IDs and a count, so time and memory are linear; the debugger's diagnostics stay
/// quick too.
#[test]
fn many_references_of_one_cable_derive_in_bounded_time() {
    let base = paired(SEED + 19).h.doc();
    let cable = *base.cables.keys().next().unwrap();
    let page = base.page_order[0];
    // (references added, seconds allowed for one derivation)
    for (n, secs) in [(2_000u128, 2u64), (20_000, 5)] {
        let mut doc = base.clone();
        for i in 0..n {
            add_ref(&mut doc, 1_000_000 + i, Pt::new(i as f64, 0.0), page, Subject::Cable(cable));
        }
        let started = Instant::now();
        let refs = cable_references(&doc);
        let took = started.elapsed();
        let partners: usize = refs.iter().map(|r| r.partners.len()).sum();
        eprintln!("{n} references: {took:?}, {partners} partner entries");
        assert!(took < Duration::from_secs(secs), "{n} references: one derivation took {took:?} ({partners} partner entries)");
        assert!(partners <= 3 * refs.len(), "{partners} partner entries for {} references", refs.len());
        let started = Instant::now();
        let lines = diagnostics(&doc);
        assert!(started.elapsed() < Duration::from_secs(secs), "diagnostics took {:?}", started.elapsed());
        assert!(lines.iter().any(|l| l.contains("references; only two are supported.")), "{lines:?}");
    }
}

/// Round 2 nit, with named points (design 072 §4.4, F4): a stub's end lands on its core's point on
/// this page's reference, never on a terminal of another page. Erasing the reference takes the
/// points, so those ends are free here: grey "loose" squares, no green dot where nothing is
/// connected; the ends on K2 keep their dots.
#[test]
fn an_end_freed_by_erasing_its_reference_shows_as_loose_here() {
    let mut r = connected(SEED + 20);
    r.h.activate(r.field);
    let fid = ref_on(&mut r.h, r.field).unwrap();
    let point = r.h.doc().representations[&fid].items[0];
    let hp = r.h.handle_of(r.field, point);
    r.h.run("erase", json!({"handles": [hp.hex()]})).unwrap();
    let u = uid(&mut r.h, r.field);
    let marks = r.h.p(|p| overlay_marks(p, u));
    let at_20: Vec<_> = marks.iter().filter(|m| (m.at.x - 20.0).abs() < 1e-9 && m.shape != MarkShape::Ring).collect();
    assert_eq!(at_20.len(), 3, "{at_20:?}");
    assert!(at_20.iter().all(|m| m.shape == MarkShape::Square), "loose, not connected: {at_20:?}");
    let dots_at_k2 = marks.iter().filter(|m| m.shape == MarkShape::Dot && (m.at.x - 100.0).abs() < 1e-9).count();
    assert_eq!(dots_at_k2, 3, "the ends connected here keep their dots");
}
