//! Brief 010 (cable references across pages), written from the brief and design 010.
//!
//! Fixture R (design §11): page 1 renamed `Supply` holds K1 with terminals at (0,0), (0,5),
//! (0,10), wires W0, W5, W10 from there to x = 60, and cable C1 slashed across them; page 2
//! `Field` holds K2 with terminals at (100,0), (100,5), (100,10).
//!
//! Every new or changed command (`cableref`, `cablecontinue`, `cablegoto`, `cablerefs`,
//! `renamepage`, `movepage`) is driven through its **JSON form** (`Session::execute`) in a table,
//! checking the result and the transaction, or the absence of one, so the rows can move to the MCP
//! harness of brief 011 unchanged.
#![cfg(not(target_arch = "wasm32"))]

mod common;

use cadcraft_engine::Input;
use cadcraft_engine::Session;
use cadcraft_engine::doc::EntityKind;
use cadcraft_engine::doc::color::{Color as CColor, Rgb};
use cadcraft_engine::ext::MarkShape;
use cadcraft_engine::geom::Vec2;
use common::{H, id};
use ecad_cadcraft::interact::{hover_lines, overlay_marks};
use ecad_cadcraft::references::{markers_on_page, plan_continue};
use ecad_cadcraft::{Mapped, ProjectHandle};
use ecad_model::derive::{cable_references, nets};
use ecad_model::{
    Cable, CableId, CableMember, Color, ConductorId, Document, Form, Item, ItemId, ItemKind, Layer, LayerId, Op, Page,
    PageId, Primitive, Pt, Representation, RepresentationId, Style, Subject, TerminalId,
};
use serde_json::{Value, json};

const SEED: u64 = 10_010;

struct R {
    h: H,
    supply: PageId,
    field: PageId,
    c1: CableId,
    w: [ConductorId; 3],
    k2: [TerminalId; 3],
}

fn tag(h: &mut H, key: &str, id: &str, value: &str) {
    h.run("setprop", json!({key: id, "key": "tag", "value": value})).unwrap();
}

/// A drawn component from a closed outline typed at the command line, with three terminals.
fn component(h: &mut H, page: PageId, outline: &str, at: [(f64, f64); 3], name: &str) -> [TerminalId; 3] {
    h.cmdline(outline);
    let lines = h.lines_on(page);
    let r = h.run("convertcomponent", json!({"handles": lines.iter().map(|x| x.hex()).collect::<Vec<_>>()})).unwrap();
    let c = r["component"].as_str().unwrap().to_string();
    tag(h, "component", &c, name);
    at.map(|(x, y)| id(&h.run("addterminal", json!({"handle": lines[0].hex(), "at": [x, y]})).unwrap(), "terminal"))
}

/// Fixture R up to (not including) `cableref`.
fn before_reference() -> R {
    let mut h = H::new(SEED);
    h.s.viewport_px = (1280.0, 800.0);
    let supply = h.first_page();
    h.run("renamepage", json!({"page": supply.hex(), "title": "Supply"})).unwrap();
    component(&mut h, supply, "line -10,-5 0,-5 0,15 -10,15 c", [(0.0, 0.0), (0.0, 5.0), (0.0, 10.0)], "K1");
    let mut w = Vec::new();
    for y in [0.0, 5.0, 10.0] {
        let c: ConductorId = id(&h.run("wire", json!({"points": [[0.0, y], [60.0, y]]})).unwrap(), "conductor");
        tag(&mut h, "conductor", &c.hex(), &format!("W{y}"));
        w.push(c);
    }
    let c1: CableId =
        id(&h.run("cableslash", json!({"from": [28, -3], "to": [32, 18], "tag": "C1"})).unwrap(), "cable");
    let field: PageId = id(&h.run("newpage", json!({"title": "Field"})).unwrap(), "page");
    let k2 = component(
        &mut h,
        field,
        "line 100,-5 110,-5 110,15 100,15 c",
        [(100.0, 0.0), (100.0, 5.0), (100.0, 10.0)],
        "K2",
    );
    h.activate(supply);
    let _ = h.s.zoom_extents();
    R { h, supply, field, c1, w: [w[0], w[1], w[2]], k2 }
}

/// Fixture R with the reference on Supply and the continuation on Field (not yet connected).
fn continued() -> (R, Value) {
    let mut r = before_reference();
    r.h.run("cableref", json!({"cable": r.c1.hex(), "at": [65, 5]})).unwrap();
    r.h.activate(r.field);
    let v = r.h.run("cablecontinue", json!({"cable": r.c1.hex(), "at": [20, 5], "toward": [40, 5]})).unwrap();
    (r, v)
}

/// Fixture R complete: the continued wires connected to K2.
fn connected() -> R {
    let (mut r, _) = continued();
    for (w, t) in r.w.iter().zip(r.k2) {
        r.h.run("connect", json!({"conductor": w.hex(), "end": "B", "terminal": t.hex()})).unwrap();
    }
    r
}

fn reference_on(h: &mut H, page: PageId) -> RepresentationId {
    let doc = h.doc();
    cable_references(&doc).into_iter().find(|x| x.page == page).expect("a reference on the page").reference
}

fn text_of(h: &mut H, page: PageId) -> String {
    let doc = h.doc();
    markers_on_page(&doc, page).first().map(|m| m.text.clone()).unwrap_or_default()
}

fn last_ops(h: &mut H) -> Vec<Op> {
    h.p(|p| p.ledger.log().last().map(|t| t.ops.clone()).unwrap_or_default())
}

/// The page window's document, with `page` made the active page (brief 016).
fn uid(h: &mut H, page: PageId) -> u64 {
    h.activate(page);
    h.p(|p| p.pages[&page].uid).expect("page open")
}

fn click(h: &mut H, x: f64, y: f64) {
    let at = Vec2::new(x, y);
    h.s.cursor = at;
    let _ = h.s.input(Input::Point(at));
}

fn prompt(h: &H) -> String {
    h.s.current_prompt().map(|p| p.display()).unwrap_or_default()
}

fn log_since(h: &H, n: usize) -> String {
    h.s.log.get(n..).unwrap_or_default().join("\n")
}

/// The ops of a transaction with fresh IDs blanked, for comparing JSON and interactive forms.
fn shape(ops: &[Op]) -> Vec<String> {
    ops.iter()
        .map(|op| match op {
            Op::AddItem { item, .. } => format!("AddItem {:?}", item.kind),
            Op::LinkRepresentation { rep, .. } => format!("Link {:?} {} {:?}", rep.form, rep.role, rep.subject),
            other => other.name().to_string(),
        })
        .collect()
}

// ---------------------------------------------------------------- AC1 (check A, B)

/// Check A: `cablecontinue` is one transaction with the §2.2 ops in order; the stubs and slash are
/// §5.3's; the cable's ID, members, conductors and nets are unchanged.
#[test]
fn continuing_a_cable_adds_routes_of_the_same_conductors_in_one_transaction() {
    let mut r = before_reference();
    r.h.run("cableref", json!({"cable": r.c1.hex(), "at": [65, 5]})).unwrap();
    r.h.activate(r.field);
    let before = r.h.doc();
    let head = r.h.head();
    let v = r.h.run("cablecontinue", json!({"cable": r.c1.hex(), "at": [20, 5], "toward": [40, 5]})).unwrap();
    assert_eq!(r.h.head(), head + 1, "one transaction");
    let ops = last_ops(&mut r.h);
    let names: Vec<&str> = ops.iter().map(Op::name).collect();
    assert_eq!(
        names,
        [
            "AddItem",
            "LinkRepresentation",
            "AddItem",
            "LinkRepresentation",
            "AddItem",
            "LinkRepresentation",
            "AddItem",
            "LinkRepresentation",
            "AddItem",
            "LinkRepresentation"
        ]
    );
    let lines: Vec<(Pt, Pt)> = ops
        .iter()
        .filter_map(|op| match op {
            Op::AddItem { item: Item { kind: ItemKind::Primitive(Primitive::Line { a, b }), .. }, .. } => {
                Some((*a, *b))
            }
            _ => None,
        })
        .collect();
    let p = Pt::new;
    assert_eq!(
        lines,
        vec![
            (p(20.0, 0.0), p(40.0, 0.0)),
            (p(20.0, 5.0), p(40.0, 5.0)),
            (p(20.0, 10.0), p(40.0, 10.0)),
            (p(27.5, -2.5), p(32.5, 12.5))
        ]
    );
    let roles: Vec<(String, Subject)> = ops
        .iter()
        .filter_map(|op| match op {
            Op::LinkRepresentation { rep, .. } => Some((rep.role.clone(), rep.subject)),
            _ => None,
        })
        .collect();
    let k = Subject::Cable(r.c1);
    assert_eq!(
        roles,
        vec![
            ("reference".into(), k),
            ("route".into(), Subject::Conductor(r.w[0])),
            ("route".into(), Subject::Conductor(r.w[1])),
            ("route".into(), Subject::Conductor(r.w[2])),
            ("slash".into(), k)
        ]
    );
    let after = r.h.doc();
    assert_eq!(after.cables, before.cables, "same cable, same members");
    assert_eq!(after.conductors, before.conductors, "no conductor or attachment changed");
    assert_eq!(nets(&after), nets(&before), "nets unchanged");
    assert_eq!(v["text"], "/1.2");
    assert_eq!(v["partner"], json!(reference_on(&mut r.h, r.supply).hex()));
    let free: Vec<&str> = v["routes"].as_array().unwrap().iter().map(|x| x["free_end"].as_str().unwrap()).collect();
    assert_eq!(free, ["B", "B", "B"]);
    assert_eq!(v["skipped"], json!([]));
}

/// Check B: after connecting, each net is {K1 terminal i, Wi, K2 terminal i}: the same structure
/// as the circuit drawn on one page; `cablerefs` lists two partners with three wires each.
#[test]
fn a_cable_on_two_pages_gives_the_nets_of_one_page() {
    let mut r = connected();
    let doc = r.h.doc();
    let n = nets(&doc);
    let wired: Vec<_> = n.iter().filter(|x| !x.conductors.is_empty()).collect();
    assert_eq!(wired.len(), 3);
    for net in wired {
        assert_eq!((net.conductors.len(), net.terminals.len()), (1, 2), "one wire between two terminals");
        assert!(net.terminals.iter().any(|t| r.k2.contains(t)), "each net reaches K2");
    }
    let v = r.h.run("cablerefs", json!({})).unwrap();
    let list = v["references"].as_array().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0]["partner"], list[1]["reference"]);
    assert_eq!(list[1]["partner"], list[0]["reference"]);
    assert!(list.iter().all(|x| x["wires_here"] == 3 && x["orphan"] == false));
}

// ---------------------------------------------------------------- AC2 (checks C, D)

/// Check C: each end shows the other's page and column, placed beyond it away from the wires; no
/// end mark where a continued wire leaves toward a reference.
#[test]
fn each_reference_shows_the_other_end_away_from_its_wires() {
    let mut r = connected();
    let doc = r.h.doc();
    let s = &markers_on_page(&doc, r.supply)[0];
    assert_eq!((s.text.as_str(), s.text_at, s.right_aligned, s.orphan), ("/2.1", Pt::new(70.0, 5.0), false, false));
    let f = &markers_on_page(&doc, r.field)[0];
    assert_eq!((f.text.as_str(), f.text_at, f.right_aligned), ("/1.2", Pt::new(15.0, 5.0), true));
    // The drawn text: one Text on ECAD-CABLES, mapped to its reference.
    for page in [r.supply, r.field] {
        // What the page shows once in front (only the page in front is redrawn on a change).
        r.h.activate(page);
        let rid = reference_on(&mut r.h, page);
        let h = r.h.p(|p| p.pages[&page].map.ref_text(rid)).expect("text drawn");
        assert_eq!(r.h.mapped(page, h), Some(Mapped::RefText(rid)));
        let e = r.h.pv(|p, pg| pg.entities_of(p, page)).into_iter().find(|e| e.handle == h).unwrap();
        assert_eq!(e.common.layer, "ECAD-CABLES");
        assert!(matches!(&e.kind, EntityKind::Text(t) if t.value == if page == r.supply { "/2.1" } else { "/1.2" }));
    }
    let ends = |h: &mut H, page: PageId, x: f64| {
        let u = uid(h, page);
        h.p(|p| overlay_marks(p, u))
            .into_iter()
            .filter(|m| m.shape != MarkShape::Ring && (m.at.x - x).abs() < 1e-9)
            .count()
    };
    assert_eq!(ends(&mut r.h, r.supply, 60.0), 0, "no end mark at the leaving ends on Supply");
    assert_eq!(ends(&mut r.h, r.field, 20.0), 0, "no end mark at the leaving ends on Field");
    assert_eq!(ends(&mut r.h, r.field, 100.0), 3, "dots at K2");
    let u = uid(&mut r.h, r.supply);
    let rings: Vec<_> =
        r.h.p(|p| overlay_marks(p, u)).into_iter().filter(|m| m.shape == MarkShape::Ring && m.stroke == 2.0).collect();
    assert_eq!(rings.len(), 1, "a bold ring on the reference point");
    assert_eq!(rings[0].at, Vec2::new(65.0, 5.0));
}

/// Check D: moving and renaming pages are one transaction each; the texts follow with none of
/// their own; moving the point changes the other end's column.
#[test]
fn texts_follow_page_moves_renames_and_point_moves_live() {
    let mut r = connected();
    let head = r.h.head();
    r.h.run("movepage", json!({"page": r.field.hex(), "index": 1})).unwrap();
    assert_eq!(r.h.head(), head + 1, "exactly the move");
    assert_eq!(text_of(&mut r.h, r.supply), "/1.1");
    assert_eq!(text_of(&mut r.h, r.field), "/2.2");
    assert_eq!(r.h.head(), head + 1, "no transaction of the texts' own");
    r.h.run("renamepage", json!({"page": r.field.hex(), "title": "Field 2"})).unwrap();
    assert_eq!(r.h.head(), head + 2);
    let rid = reference_on(&mut r.h, r.supply);
    let u = uid(&mut r.h, r.supply);
    let point = r.h.doc().representations[&rid].items[0];
    let hp = r.h.handle_of(r.supply, point);
    let lines = r.h.p(|p| hover_lines(p, u, hp)).unwrap();
    assert!(lines.iter().any(|l| l.contains("Other end: page 1 \"Field 2\"")), "{lines:?}");
    // Move Field's point to (90,5): Supply's text names column 3.
    r.h.activate(r.field);
    let fid = reference_on(&mut r.h, r.field);
    let fpoint = r.h.doc().representations[&fid].items[0];
    let fh = r.h.handle_of(r.field, fpoint);
    r.h.run("move", json!({"handles": [fh.hex()], "delta": [70, 0]})).unwrap();
    assert_eq!(text_of(&mut r.h, r.supply), "/1.3");
}

// ---------------------------------------------------------------- AC3 (check E)

/// Check E: erasing the whole Field drawing makes Supply's end an orphan (red, diagnostic, echo);
/// conductors and their Field attachments stay; undo restores the link with the same IDs.
#[test]
fn erasing_one_drawing_makes_the_other_end_an_orphan_and_undo_restores_it() {
    let mut r = connected();
    r.h.activate(r.field);
    let before = r.h.doc();
    let all: Vec<String> =
        r.h.pv(|p, pg| pg.entities_of(p, r.field))
            .iter()
            .map(|e| e.handle)
            .collect::<Vec<_>>()
            .into_iter()
            .filter(|h| !matches!(r.h.mapped(r.field, *h), Some(Mapped::Marker(..))))
            .map(|h| h.hex())
            .collect();
    // Keep K2's outline: erase only what the continuation drew (stubs, slash, point, text).
    let doc = r.h.doc();
    let keep: Vec<String> = doc
        .representations
        .values()
        .filter(|x| x.page == r.field && matches!(x.subject, Subject::Component(_)))
        .flat_map(|x| x.items.iter())
        .map(|i| r.h.handle_of(r.field, *i).hex())
        .collect();
    let erase: Vec<String> = all.into_iter().filter(|h| !keep.contains(h)).collect();
    let n = r.h.s.log.len();
    let head = r.h.head();
    r.h.run("erase", json!({ "handles": erase })).unwrap();
    assert_eq!(r.h.head(), head + 1, "one transaction");
    assert_eq!(text_of(&mut r.h, r.supply), "/?");
    let doc = r.h.doc();
    assert!(markers_on_page(&doc, r.supply)[0].orphan);
    // Red: the ring (`ecad.orphan`, #FF0000) and the text (ACI 1).
    // Only the active page is drawn (brief 016): Supply shows the new text once it is active.
    r.h.activate(r.supply);
    let su = uid(&mut r.h, r.supply);
    let ring = r.h.p(|p| overlay_marks(p, su)).into_iter().find(|m| m.at == Vec2::new(65.0, 5.0)).expect("ring");
    assert_eq!((ring.shape, ring.color), (MarkShape::Ring, Rgb(0xff, 0x00, 0x00)));
    let srid = reference_on(&mut r.h, r.supply);
    let th = r.h.p(|p| p.pages[&r.supply].map.ref_text(srid)).expect("text drawn");
    let te = r.h.pv(|p, pg| pg.entities_of(p, r.supply)).into_iter().find(|e| e.handle == th).unwrap();
    assert_eq!(te.common.color, CColor::Index(1));
    assert_eq!(doc.conductors, before.conductors, "conductors and attachments stay");
    assert_eq!(doc.cables, before.cables);
    assert!(
        log_since(&r.h, n).contains("Cable C1's reference on page 1 now has no other end."),
        "{}",
        log_since(&r.h, n)
    );
    assert_eq!(ecad_cadcraft::references::diagnostics(&doc), vec!["Orphan: cable C1 on page 1 (/?)".to_string()]);
    let snap = r.h.project.snapshot(&mut r.h.s, None).unwrap();
    assert_eq!(snap["checks"]["orphan_references"].as_array().map(Vec::len), Some(1));
    r.h.cmdline("u");
    assert_eq!(r.h.doc(), before, "undo restores the same IDs");
    assert_eq!(text_of(&mut r.h, r.supply), "/2.1");
}

/// Erasing only the reference text erases its reference (design §9); moving it is refused.
#[test]
fn the_text_is_derived_erasing_it_erases_the_reference_and_moving_it_is_refused() {
    let (mut r, _) = continued();
    r.h.activate(r.supply);
    let rid = reference_on(&mut r.h, r.supply);
    let text = r.h.p(|p| p.pages[&r.supply].map.ref_text(rid)).unwrap();
    let head = r.h.head();
    let e = r.h.run("move", json!({"handles": [text.hex()], "delta": [5, 0]}));
    assert_eq!(e, Err("The reference text is derived; move the reference point instead.".to_string()));
    assert_eq!(r.h.head(), head, "moving the text records nothing");
    r.h.run("erase", json!({"handles": [text.hex()]})).unwrap();
    assert_eq!(r.h.head(), head + 1);
    let doc = r.h.doc();
    assert!(!doc.representations.contains_key(&rid), "the reference is gone");
    assert_eq!(text_of(&mut r.h, r.field), "/?", "the other end is an orphan");
}

// ---------------------------------------------------------------- AC4 (check F)

/// Check F: save, reopen and replay give the same cable, drawings and references.
#[test]
fn save_reopen_and_replay_keep_cable_drawings_and_references() {
    let mut r = connected();
    r.h.run("qsave", Value::Null).unwrap();
    let doc = r.h.doc();
    let refs = r.h.run("cablerefs", json!({})).unwrap();
    let medium = r.h.p(|p| p.store.medium().clone());
    let mut s = Session::empty();
    let project = ProjectHandle::open_from(&mut s, "R", medium, SEED).unwrap();
    let mut h = H { s, project, trail: Vec::new() };
    assert_eq!(h.doc(), doc);
    assert!(h.p(|p| p.ledger.replay_matches().unwrap()));
    assert_eq!(h.run("cablerefs", json!({})).unwrap(), refs);
    h.check();
}

// ---------------------------------------------------------------- AC5 (check G)

/// Check G: the hover lens names the other end and its terminals; `cablegoto` opens it centred
/// with its point selected, and back; an orphan is refused.
#[test]
fn hover_previews_the_target_and_cablegoto_jumps_there_and_back() {
    let mut r = connected();
    r.h.activate(r.supply);
    let rid = reference_on(&mut r.h, r.supply);
    let u = uid(&mut r.h, r.supply);
    let text = r.h.p(|p| p.pages[&r.supply].map.ref_text(rid)).unwrap();
    let lines = r.h.p(|p| hover_lines(p, u, text)).unwrap();
    assert_eq!(lines[0], "Cable reference  /2.1");
    assert!(lines.contains(&"Other end: page 2 \"Field\", column 1, 3 wires".to_string()), "{lines:?}");
    assert_eq!(lines.iter().filter(|l| l.contains("-> K2:")).count(), 3, "{lines:?}");
    let v = r.h.run("cablegoto", json!({"reference": rid.hex()})).unwrap();
    let fid = reference_on(&mut r.h, r.field);
    assert_eq!(v["reference"], json!(fid.hex()));
    assert_eq!(v["page"], json!(r.field.hex()));
    assert_eq!(v["message"], "Page 2 \"Field\", at the other end of cable C1 (/2.1).");
    let want = uid(&mut r.h, r.field);
    assert_eq!(r.h.s.state().unwrap().uid, want, "Field is current");
    let point = r.h.doc().representations[&fid].items[0];
    assert_eq!(r.h.s.selection(), vec![r.h.handle_of(r.field, point)]);
    let c = r.h.s.state().unwrap().view().center;
    assert!((c.x - 20.0).abs() < 1e-9 && (c.y - 5.0).abs() < 1e-9, "centred on the other end");
    // And back, by the interactive form's step 0 (the selected point).
    r.h.s.start("cablegoto").unwrap();
    let want = uid(&mut r.h, r.supply);
    assert_eq!(r.h.s.state().unwrap().uid, want, "jumped back");
}

// ---------------------------------------------------------------- AC6 (checks H, I, J)

/// Check H: the interactive forms commit exactly the JSON forms' transactions (apart from IDs).
#[test]
fn interactive_forms_record_the_json_forms_transactions() {
    // JSON forms.
    let (mut j, _) = continued();
    let json_continue = shape(&last_ops(&mut j.h));
    let json_ref = {
        let mut r = before_reference();
        r.h.run("cableref", json!({"cable": r.c1.hex(), "at": [65, 5]})).unwrap();
        shape(&last_ops(&mut r.h))
    };
    // Interactive: Cable Reference, click the slash, click 65,5.
    let mut r = before_reference();
    r.h.s.start("cableref").unwrap();
    assert_eq!(prompt(&r.h), "Select a cable to reference:");
    click(&mut r.h, 30.0, 7.5);
    assert_eq!(prompt(&r.h), "Specify reference point or [Undo]:");
    click(&mut r.h, 65.0, 5.0);
    assert!(r.h.s.running.is_none(), "the command ends");
    assert_eq!(shape(&last_ops(&mut r.h)), json_ref);
    // Continue Cable on Field: Enter takes C1, click 20,5, click 40,5.
    r.h.activate(r.field);
    let _ = r.h.s.zoom_extents();
    r.h.s.start("cablecontinue").unwrap();
    assert_eq!(prompt(&r.h), "Enter cable to continue or [List] <C1>:");
    r.h.s.input(Input::Enter).unwrap();
    assert_eq!(prompt(&r.h), "Specify reference point:");
    click(&mut r.h, 20.0, 5.0);
    assert_eq!(prompt(&r.h), "Specify direction and length of the wires or [Undo]:");
    r.h.s.cursor = Vec2::new(40.0, 5.0);
    let shown = r.h.s.preview(Vec2::new(40.0, 5.0));
    assert_eq!(shown.iter().filter(|e| matches!(e.kind, EntityKind::Line(_))).count(), 4, "three stubs and the slash");
    assert!(shown.iter().any(|e| matches!(&e.kind, EntityKind::Text(t) if t.value == "-> 3 wires continue")));
    click(&mut r.h, 40.0, 5.0);
    assert!(r.h.s.running.is_none());
    assert_eq!(shape(&last_ops(&mut r.h)), json_continue);
}

/// Check I: refusals R1 to R6, each with its words and the head unchanged.
#[test]
fn refusals_say_why_and_change_nothing() {
    let (mut r, _) = continued();
    let c1 = r.c1.hex();
    let third: PageId = id(&r.h.run("newpage", json!({"title": "Third"})).unwrap(), "page");
    r.h.activate(r.supply);
    // A cable with no wires on Third, and one with no reference: W-new on Supply.
    let lone: ConductorId = id(&r.h.run("wire", json!({"points": [[0, 40], [60, 40]]})).unwrap(), "conductor");
    let c2: CableId = id(&r.h.run("cableadd", json!({"conductors": [lone.hex()], "tag": "C2"})).unwrap(), "cable");
    let cases: Vec<(PageId, &str, Value, &str)> = vec![
        (r.supply, "cableref", json!({"cable": c1, "at": [70, 0]}), "Cable C1 already has a reference on this page."),
        (third, "cableref", json!({"cable": c1, "at": [0, 0]}), "Cable C1 already has a reference on this page."),
        (
            third,
            "cablecontinue",
            json!({"cable": c1, "at": [0, 0], "toward": [10, 0]}),
            "Cable C1 already continues to another page; a cable can have only two references in this version.",
        ),
        (
            third,
            "cableref",
            json!({"cable": c2.hex(), "at": [0, 0]}),
            "Cable C2 has no wires drawn on this page; draw or continue it here first.",
        ),
        (
            third,
            "cablecontinue",
            json!({"cable": c2.hex(), "at": [0, 0], "toward": [10, 0]}),
            "Cable C2 has no reference to continue from; place one with Cable Reference on its page first.",
        ),
    ];
    for (page, cmd, params, want) in cases {
        r.h.activate(page);
        let head = r.h.head();
        let e = r.h.run(cmd, params.clone()).unwrap_err();
        // R1 on Third reads as R2 when the cable already has its two references elsewhere.
        assert!(
            e.contains(want) || (page == third && cmd == "cableref" && e.contains("already continues")),
            "{cmd} {params}: {e}"
        );
        assert_eq!(r.h.head(), head, "{cmd}: nothing recorded");
    }
    // R5: continuing on the reference's own page; R6: every wire connected at both ends.
    let mut f = before_reference();
    f.h.run("cableref", json!({"cable": f.c1.hex(), "at": [65, 5]})).unwrap();
    let head = f.h.head();
    let e = f.h.run("cablecontinue", json!({"cable": f.c1.hex(), "at": [80, 5], "toward": [90, 5]})).unwrap_err();
    assert!(e.contains("Cable C1 is already drawn on this page; continue it on another page."), "{e}");
    assert_eq!(f.h.head(), head);
    let doc = f.h.doc();
    let mut both = doc.clone();
    for w in f.w {
        both.conductors.get_mut(&w).unwrap().set_end(ecad_model::End::B, Some(f.k2[0]));
    }
    let e = plan_continue(&both, f.field, f.c1, None, Pt::new(0.0, 0.0), Pt::new(10.0, 0.0), 1e-6).unwrap_err();
    assert_eq!(
        e,
        "None of cable C1's wires can continue: each is connected at both ends or already drawn on this page."
    );
    // A mixed cable: the both-ends wire is skipped and reported.
    let mut mixed = doc.clone();
    mixed.conductors.get_mut(&f.w[2]).unwrap().set_end(ecad_model::End::B, Some(f.k2[0]));
    let plan = plan_continue(&mixed, f.field, f.c1, None, Pt::new(0.0, 0.0), Pt::new(10.0, 0.0), 1e-6).unwrap();
    assert_eq!(plan.stubs.len(), 2);
    assert_eq!(plan.skipped, vec![(f.w[2], ecad_cadcraft::references::Skip::BothConnected)]);
}

/// The JSON forms of every new or changed command, as an agent drives them (brief 011 rule):
/// result or refusal, and whether a transaction was recorded.
#[test]
fn json_forms_of_the_reference_and_page_commands() {
    let mut r = before_reference();
    let c1 = r.c1.hex();
    let (supply, field) = (r.supply.hex(), r.field.hex());
    // (page, command, params, Ok: substring of the result | Err: substring of the refusal, transactions added)
    type Row<'a> = (PageId, &'a str, Value, Result<&'a str, &'a str>, u64);
    let rows: Vec<Row<'_>> = vec![
        (r.supply, "cablerefs", json!({}), Ok(r#"{"references":[]}"#), 0),
        (r.supply, "cableref", json!({"cable": c1, "at": [65, 5]}), Ok(r#""orphan":true"#), 1),
        (r.supply, "cablerefs", json!({"cable": c1}), Ok(r#""text":"/?""#), 0),
        (
            r.supply,
            "cablegoto",
            json!({"reference": "<Supply's reference>"}),
            Err("this reference has no other end"),
            0,
        ),
        (r.field, "cablecontinue", json!({"cable": c1, "at": [20, 5], "toward": [40, 5]}), Ok(r#""text":"/1.2""#), 1),
        (r.field, "cablerefs", json!({}), Ok(r#""text":"/2.1""#), 0),
        (r.field, "cablegoto", json!({"reference": "0"}), Err("no such reference"), 0),
        (r.field, "renamepage", json!({"page": supply, "title": "  Supply  "}), Ok(r#""changed":false"#), 0),
        (r.field, "renamepage", json!({"page": supply, "title": "Source"}), Ok(r#""changed":true"#), 1),
        (r.field, "movepage", json!({"page": field, "index": 2}), Ok(r#""changed":false"#), 0),
        (r.field, "movepage", json!({"page": field, "index": 1}), Ok(r#""index":1"#), 1),
        (r.field, "cablerefs", json!({"cable": c1}), Ok(r#""text":"/2.2""#), 0),
    ];
    for (page, cmd, mut params, want, added) in rows {
        // IDs made by an earlier row are filled in when the row runs.
        if params["reference"] == "<Supply's reference>" {
            params["reference"] = Value::String(reference_hex(&mut r.h, r.supply));
        }
        r.h.activate(page);
        let head = r.h.head();
        let got = r.h.run(cmd, params.clone());
        match (&got, want) {
            (Ok(v), Ok(part)) => assert!(v.to_string().contains(part), "{cmd} {params}: {v}"),
            (Err(e), Err(part)) => assert!(e.contains(part), "{cmd} {params}: {e}"),
            _ => panic!("{cmd} {params}: got {got:?}, wanted {want:?}"),
        }
        assert_eq!(r.h.head(), head + added, "{cmd} {params}: transactions");
    }
    // cablegoto from Field reaches Supply (now page 2 "Source").
    let rid = reference_hex(&mut r.h, r.field);
    let v = r.h.run("cablegoto", json!({"reference": rid})).unwrap();
    assert_eq!(v["message"], "Page 2 \"Source\", at the other end of cable C1 (/2.2).");
}

fn reference_hex(h: &mut H, page: PageId) -> String {
    reference_on(h, page).hex()
}

/// Check J: hostile input never panics; each refusal is in words and records nothing.
#[test]
fn hostile_input_is_refused_in_words() {
    let (mut r, _) = continued();
    r.h.activate(r.supply);
    let c1 = r.c1.hex();
    let page = r.supply.hex();
    let w0 = r.w[0].hex();
    let many: Vec<String> = (0..10_001).map(|i| format!("{i:x}")).collect();
    let rows: Vec<(&str, Value, &str)> = vec![
        ("cableref", json!({"at": [0, 0]}), "`cable` (hex id) is required"),
        ("cableref", json!({"cable": "zz", "at": [0, 0]}), "`cable` (hex id) is required"),
        ("cableref", json!({"cable": "1", "at": [0, 0]}), "no such cable"),
        ("cableref", json!({"cable": c1}), "`at` [x,y] is required"),
        ("cableref", json!({"cable": c1, "at": [f64::NAN, 0]}), "is not a finite [x,y] point"),
        ("cableref", json!({"cable": c1, "at": [1e13, 0]}), "is not a finite [x,y] point"),
        ("cableref", json!({"cable": c1, "at": "x"}), "is not a finite [x,y] point"),
        ("cablecontinue", json!({"cable": c1, "at": [0, 0]}), "`toward` [x,y] is required"),
        ("cablecontinue", json!({"cable": c1, "at": [0, 0], "toward": [0, 0]}), "`toward` must differ from `at`"),
        (
            "cablecontinue",
            json!({"cable": c1, "at": [0, 0], "toward": [1, 0], "members": "x"}),
            "`members` must be a list of conductor ids",
        ),
        (
            "cablecontinue",
            json!({"cable": c1, "at": [0, 0], "toward": [1, 0], "members": [1]}),
            "`members` must be a list of conductor ids",
        ),
        (
            "cablecontinue",
            json!({"cable": c1, "at": [0, 0], "toward": [1, 0], "members": many}),
            "`members` lists more than 10000 wires",
        ),
        ("cablegoto", json!({}), "`reference` (hex id) is required"),
        ("cablegoto", json!({"reference": w0}), "no such reference"),
        ("cablerefs", json!({"cable": "nope"}), "`cable` (hex id) is required"),
        ("cablerefs", json!({"cable": "1"}), "no such cable"),
        (
            "renamepage",
            json!({"page": page, "title": ""}),
            "`title` must be a non-empty string of at most 200 characters",
        ),
        ("renamepage", json!({"page": page, "title": "   "}), "`title` must be a non-empty string"),
        ("renamepage", json!({"page": page, "title": "x".repeat(10_000)}), "`title` must be a non-empty string"),
        ("renamepage", json!({"page": page, "title": "a\u{7}b"}), "`title` must be a non-empty string"),
        ("renamepage", json!({"page": page, "title": 5}), "`title` must be a non-empty string"),
        ("renamepage", json!({"page": "1", "title": "x"}), "no such page"),
        ("renamepage", json!({"title": "x"}), "`page` (hex id) is required"),
        ("movepage", json!({"page": page, "index": 0}), "`index` must be a whole number from 1 to 2"),
        ("movepage", json!({"page": page, "index": -1}), "`index` must be a whole number from 1 to 2"),
        ("movepage", json!({"page": page, "index": 1e20}), "`index` must be a whole number from 1 to 2"),
        ("movepage", json!({"page": page, "index": "x"}), "`index` must be a whole number from 1 to 2"),
        ("movepage", json!({"page": page, "index": 1.5}), "`index` must be a whole number from 1 to 2"),
        ("movepage", json!({"page": page}), "`index` must be a whole number from 1 to 2"),
        ("movepage", json!({"page": "1", "index": 1}), "no such page"),
    ];
    for (cmd, params, want) in rows {
        let head = r.h.head();
        let e = r.h.run(cmd, params.clone()).unwrap_err();
        assert!(e.contains(want), "{cmd} {params}: {e}");
        assert_eq!(r.h.head(), head, "{cmd}: nothing recorded");
    }
    // `members` naming a non-member, or a member twice (on a fresh continuation page).
    let mut f = before_reference();
    f.h.run("cableref", json!({"cable": f.c1.hex(), "at": [65, 5]})).unwrap();
    f.h.activate(f.field);
    for (members, want) in
        [(json!([f.c1.hex()]), "is not a member of cable"), (json!([f.w[0].hex(), f.w[0].hex()]), "is listed twice")]
    {
        let e = f
            .h
            .run("cablecontinue", json!({"cable": f.c1.hex(), "at": [20, 5], "toward": [40, 5], "members": members}))
            .unwrap_err();
        assert!(e.contains(want), "{e}");
    }
    // `members` restricts and orders the continuation.
    let v = f
        .h
        .run("cablecontinue", json!({"cable": f.c1.hex(), "at": [20, 5], "toward": [40, 5], "members": [f.w[2].hex()]}))
        .unwrap();
    assert_eq!(v["routes"].as_array().map(Vec::len), Some(1));
}

/// Hand-built documents (as a file could hold): four references of one cable and a reference
/// whose point is not finite are derived without panicking (`+N`, `/?`).
#[test]
fn hostile_documents_derive_without_panicking() {
    let page: PageId = PageId::from(1);
    let layer: LayerId = LayerId::from(2);
    let cable: CableId = CableId::from(3);
    let mut doc = Document::default();
    doc.pages.insert(page, Page { title: "1".into() });
    doc.page_order.push(page);
    doc.layers.insert(layer, Layer { name: "0".into() });
    doc.cables.insert(cable, Cable { properties: Default::default(), members: Vec::<CableMember>::new() });
    let style = Style { layer, color: Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 };
    for (i, x) in [0.0, 50.0, 100.0, 150.0, f64::NAN].into_iter().enumerate() {
        let item = ItemId::from(100 + i as u128);
        doc.items.insert(
            item,
            Item { page, style: style.clone(), kind: ItemKind::Primitive(Primitive::Point { at: Pt::new(x, 0.0) }) },
        );
        doc.representations.insert(
            RepresentationId::from(200 + i as u128),
            Representation {
                subject: Subject::Cable(cable),
                page,
                form: Form::Drawn,
                role: "reference".into(),
                items: vec![item],
                connection_points: Default::default(),
            },
        );
    }
    let refs = cable_references(&doc);
    assert_eq!(refs.len(), 5);
    assert_eq!(refs[0].text, "/1.2 /1.3 /1.4 +1");
    let lost = refs.iter().find(|r| r.at.is_none()).unwrap();
    assert_eq!(lost.locator(), "/?");
    assert!(markers_on_page(&doc, page).len() == 4, "an end that cannot be located is not drawn");
    let lines = ecad_cadcraft::references::diagnostics(&doc);
    assert!(lines.iter().any(|l| l.contains("has 5 references; only two are supported.")), "{lines:?}");
    let snap_refs = ecad_inspect::over_referenced(&refs);
    assert_eq!(snap_refs, vec![cable]);

    // A reference on a page that is in no page order (only from a hostile file): it and its
    // partners read `/?` for it, and nothing panics.
    let lost_page = PageId::from(9);
    let mut two = Document::default();
    two.pages.insert(page, Page { title: "1".into() });
    two.pages.insert(lost_page, Page { title: "lost".into() });
    two.page_order.push(page);
    two.layers.insert(layer, Layer { name: "0".into() });
    two.cables.insert(cable, Cable { properties: Default::default(), members: Vec::new() });
    for (i, pg) in [page, lost_page].into_iter().enumerate() {
        let item = ItemId::from(300 + i as u128);
        two.items.insert(
            item,
            Item {
                page: pg,
                style: style.clone(),
                kind: ItemKind::Primitive(Primitive::Point { at: Pt::new(10.0, 0.0) }),
            },
        );
        two.representations.insert(
            RepresentationId::from(400 + i as u128),
            Representation {
                subject: Subject::Cable(cable),
                page: pg,
                form: Form::Drawn,
                role: "reference".into(),
                items: vec![item],
                connection_points: Default::default(),
            },
        );
    }
    let refs = cable_references(&two);
    let on_lost = refs.iter().find(|r| r.page == lost_page).unwrap();
    assert_eq!((on_lost.page_number, on_lost.locator()), (None, "/?".to_string()));
    let on_page = refs.iter().find(|r| r.page == page).unwrap();
    assert_eq!(on_page.text, "/?", "a partner that cannot be located reads /?");
    assert!(!on_page.orphan, "it still has another end");
    assert_eq!(markers_on_page(&two, lost_page).len(), 1);
    let _ = ecad_cadcraft::references::diagnostics(&two);
    let _ = ecad_cadcraft::references::hover_lines(&two, on_lost.reference);
}

/// B1 (never-crash): a hostile file with 20 000 references of one cable derives in bounded time
/// and memory: each reference keeps at most three partner IDs and a count, and the text ends `+N`.
#[test]
fn twenty_thousand_references_of_one_cable_stay_bounded_and_fast() {
    let page: PageId = PageId::from(1);
    let layer: LayerId = LayerId::from(2);
    let cable: CableId = CableId::from(3);
    let mut doc = Document::default();
    doc.pages.insert(page, Page { title: "1".into() });
    doc.page_order.push(page);
    doc.layers.insert(layer, Layer { name: "0".into() });
    doc.cables.insert(cable, Cable { properties: Default::default(), members: Vec::new() });
    let style = Style { layer, color: Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 };
    let n: usize = 20_000;
    for i in 0..n {
        let item = ItemId::from(1_000_000 + i as u128);
        doc.items.insert(
            item,
            Item {
                page,
                style: style.clone(),
                kind: ItemKind::Primitive(Primitive::Point { at: Pt::new(i as f64, 0.0) }),
            },
        );
        doc.representations.insert(
            RepresentationId::from(2_000_000 + i as u128),
            Representation {
                subject: Subject::Cable(cable),
                page,
                form: Form::Drawn,
                role: "reference".into(),
                items: vec![item],
                connection_points: Default::default(),
            },
        );
    }
    let started = std::time::Instant::now();
    let refs = cable_references(&doc);
    let took = started.elapsed();
    assert_eq!(refs.len(), n);
    let kept: usize = refs.iter().map(|r| r.partners.len()).sum();
    assert!(kept <= 3 * n, "{kept} partner IDs kept for {n} references");
    assert!(refs.iter().all(|r| r.partner_count == n - 1 && r.partners.len() == 3));
    assert!(refs[0].text.ends_with(&format!("+{}", n - 1 - 3)), "{}", refs[0].text);
    assert!(took < std::time::Duration::from_secs(10), "one derivation took {took:?}");
    let _ = ecad_cadcraft::references::diagnostics(&doc);
    // Combined trial of 009 and 010: 010's leaving ends are folded into 009's drawing (`quiet`).
    let _ = ecad_cadcraft::views::drawings_on_page(&doc, page, &Default::default(), 1e-6);
}

/// The per-page movement rule (design 010 §0, translate M3): on the continuation page, moving a
/// stub whose other end is connected on the first page never detaches that end, and dropping that
/// end on a terminal here connects nothing and says why.
#[test]
fn moving_a_stub_never_detaches_the_end_connected_on_the_other_page() {
    let (mut r, _) = continued();
    r.h.activate(r.field);
    let before = r.h.doc().conductors;
    let w5 = r.h.route_handle(r.field, r.w[1]);
    // Away from everything: the end at the reference (end A, attached to K1 on Supply) stays.
    r.h.run("move", json!({"handles": [w5.hex()], "delta": [0, 50]})).unwrap();
    assert_eq!(r.h.doc().conductors, before, "no attachment changed");
    assert_eq!(r.h.route_ends(r.field, r.w[1]).0, Pt::new(20.0, 55.0));
    // Back, then onto K2's middle terminal at (100,5): end A stays on K1 and the user is told.
    r.h.run("move", json!({"handles": [w5.hex()], "delta": [0, -50]})).unwrap();
    let n = r.h.s.log.len();
    r.h.run("move", json!({"handles": [w5.hex()], "delta": [80, 0]})).unwrap();
    assert_eq!(r.h.doc().conductors, before, "end A is still connected on Supply only");
    assert!(
        log_since(&r.h, n).contains("End A of W5 is connected on another page, so it was not connected here"),
        "{}",
        log_since(&r.h, n)
    );
}

// ---------------------------------------------------------------- scope 5 (check M), fan (check L)

/// Check M: the trail from a reference reaches its cable and partner; a cable lists its
/// representations by page; a conductor its route on each page.
#[test]
fn the_debugger_trail_follows_references_across_pages() {
    use ecad_cadcraft::debug::{ObjectId, used_by};
    let mut r = connected();
    let doc = r.h.doc();
    let s = reference_on(&mut r.h, r.supply);
    let f = reference_on(&mut r.h, r.field);
    assert_eq!(used_by(&doc, ObjectId::Representation(s)), vec![ObjectId::Cable(r.c1), ObjectId::Representation(f)]);
    let reps: Vec<PageId> = used_by(&doc, ObjectId::Cable(r.c1))
        .into_iter()
        .filter_map(|o| match o {
            ObjectId::Representation(x) => Some(doc.representations[&x].page),
            _ => None,
        })
        .collect();
    assert_eq!(reps, vec![r.supply, r.supply, r.field, r.field], "grouped by page in page order");
    let routes = used_by(&doc, ObjectId::Conductor(r.w[0]))
        .into_iter()
        .filter(|o| matches!(o, ObjectId::Representation(_)))
        .count();
    assert_eq!(routes, 2, "a route on each page");
    let snap = r.h.project.snapshot(&mut r.h.s, None).unwrap();
    assert_eq!(snap["checks"]["cable_references"].as_array().map(Vec::len), Some(2));
    assert_eq!(snap["checks"]["orphan_references"], json!([]));
}

/// Check L: fan view of C1 draws on both pages with the reference marker and text still shown.
#[test]
fn fan_view_keeps_the_reference_drawn() {
    let mut r = connected();
    r.h.run("cableview", json!({"cable": r.c1.hex(), "view": "fan"})).unwrap();
    for page in [r.supply, r.field] {
        r.h.activate(page);
        let rid = reference_on(&mut r.h, page);
        let point = r.h.doc().representations[&rid].items[0];
        assert!(r.h.p(|p| p.pages[&page].map.item(point)).is_some(), "the point stays drawn");
        assert!(r.h.p(|p| p.pages[&page].map.ref_text(rid)).is_some(), "the text stays drawn");
    }
    r.h.check();
}

/// Design check J: a cable of `MAX_CABLE_MEMBERS` (10 000) wires with a reference is continued on
/// another page in one transaction, in bounded time, with a stub per wire.
#[test]
fn a_ten_thousand_wire_cable_continues_in_bounded_time() {
    use ecad_model::{Author, Conductor};
    let n = ecad_cadcraft::slash::MAX_CABLE_MEMBERS;
    let mut h = H::new(SEED + 7);
    let supply = h.first_page();
    let ops = h.cfg(|p| {
        let style = Style { layer: p.default_layer, color: Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 };
        let cable: CableId = p.ids.fresh();
        let mut ops = vec![Op::CreateCable { id: cable, cable: Cable::default() }];
        for i in 0..n {
            let (c, it, r): (ConductorId, ItemId, RepresentationId) = (p.ids.fresh(), p.ids.fresh(), p.ids.fresh());
            let y = i as f64;
            ops.push(Op::CreateConductor { id: c, conductor: Conductor::default() });
            ops.push(Op::AddItem {
                id: it,
                item: Item {
                    page: supply,
                    style: style.clone(),
                    kind: ItemKind::Primitive(Primitive::Line { a: Pt::new(0.0, y), b: Pt::new(60.0, y) }),
                },
            });
            ops.push(Op::LinkRepresentation {
                id: r,
                rep: Representation {
                    subject: Subject::Conductor(c),
                    page: supply,
                    form: Form::Drawn,
                    role: "route".into(),
                    items: vec![it],
                    connection_points: Default::default(),
                },
            });
            ops.push(Op::AddCableMember { cable, index: i, member: CableMember { conductor: c, core: None } });
        }
        let (it, r): (ItemId, RepresentationId) = (p.ids.fresh(), p.ids.fresh());
        ops.push(Op::AddItem {
            id: it,
            item: Item { page: supply, style, kind: ItemKind::Primitive(Primitive::Point { at: Pt::new(65.0, 0.0) }) },
        });
        ops.push(Op::LinkRepresentation {
            id: r,
            rep: Representation {
                subject: Subject::Cable(cable),
                page: supply,
                form: Form::Drawn,
                role: "reference".into(),
                items: vec![it],
                connection_points: Default::default(),
            },
        });
        (cable, ops)
    });
    let (cable, ops) = ops;
    // The page is not reprojected after this direct commit: save and reopen, and use the reopened
    // project (as the 004 bulk tests do).
    h.cfg(|p| p.ledger.commit(Author::User, "bulk cable", ops)).unwrap();
    h.s.execute("qsave", &Value::Null).unwrap();
    let medium = h.p(|p| p.store.medium().clone());
    let mut s = Session::empty();
    let project = ProjectHandle::open_from(&mut s, "Big", medium, SEED + 7).unwrap();
    let mut h = H { s, project, trail: Vec::new() };
    h.run("newpage", json!({"title": "Field"})).unwrap();
    let head = h.head();
    let started = std::time::Instant::now();
    let v = h.run("cablecontinue", json!({"cable": cable.hex(), "at": [20, 0], "toward": [40, 0]})).unwrap();
    let took = started.elapsed();
    assert_eq!(h.head(), head + 1, "one transaction");
    assert_eq!(v["routes"].as_array().map(Vec::len), Some(n));
    assert_eq!(v["text"], "/1.2");
    assert!(took < std::time::Duration::from_secs(120), "continuing {n} wires took {took:?}");
}

// ---------------------------------------------------------------- the flag's side (fix round 1)

/// The drawn flag outline and text anchor of `page`'s reference end of `cable` (traditional view).
fn drawn_flag(h: &mut H, page: PageId, cable: CableId) -> (Vec<Pt>, Pt) {
    use ecad_cadcraft::look::{Role, Shape};
    let doc = h.doc();
    let drawings = ecad_cadcraft::views::drawings_on_page(&doc, page, &Default::default(), 1e-6);
    let d = drawings.get(&cable).expect("the cable is drawn");
    let flag = d.parts.iter().find_map(|p| match (&p.role, &p.shape) {
        (Role::Flag, Shape::Path { points, .. }) => Some(points.clone()),
        _ => None,
    });
    let text = d.parts.iter().find_map(|p| match (&p.role, &p.shape) {
        (Role::ReferenceText, Shape::Text { at, .. }) => Some(*at),
        _ => None,
    });
    (flag.expect("a flag"), text.expect("a reference text"))
}

/// Design 009 §7.2 with design 010 §6.2: the flag of `page`'s reference end points along `u`, away
/// from the wires; only its flat side's margin (h/2) is behind the point, the text is wholly on
/// the far side, and the pick box follows the flag (its mirror image, among the wires, picks
/// nothing).
fn assert_flag_away(h: &mut H, page: PageId, cable: CableId, u: Pt) {
    let doc = h.doc();
    let m = markers_on_page(&doc, page).into_iter().find(|m| m.cable == cable).expect("a marker");
    let along = |p: Pt| (p.x - m.at.x) * u.x + (p.y - m.at.y) * u.y;
    // The wires leave the point the other way.
    let ends = ecad_model::derive::route_ends_by_page(&doc);
    let routes: Vec<Pt> = doc.cables[&cable]
        .members
        .iter()
        .filter_map(|x| ends.get(&(x.conductor, page)).copied())
        .map(|(a, b)| if a.dist(m.at) <= b.dist(m.at) { b } else { a })
        .collect();
    assert!(!routes.is_empty() && routes.iter().all(|far| along(*far) < 0.0), "the wires leave against {u:?}");

    let (flag, text_at) = drawn_flag(h, page, cable);
    let hh = ecad_cadcraft::look::LABEL_HEIGHT;
    let w = ecad_cadcraft::look::font_width(&m.text);
    for p in &flag {
        assert!(along(*p) >= -0.5 * hh - 1e-9, "flag point {p:?} reaches back into the wires ({u:?})");
    }
    let reach = flag.iter().map(|p| along(*p)).fold(f64::MIN, f64::max);
    assert!(reach >= hh, "the flag points away from the wires ({u:?}): reach {reach}");
    let corners = [text_at, Pt::new(text_at.x + w, text_at.y), Pt::new(text_at.x, text_at.y + hh)];
    for c in corners.iter().chain([Pt::new(text_at.x + w, text_at.y + hh)].iter()) {
        assert!(along(*c) >= -1e-9, "the text {:?} sits away from the wires ({u:?}): corner {c:?}", m.text);
    }
    let centre = Pt::new(text_at.x + 0.5 * w, text_at.y + 0.5 * hh);
    // Among the wires, 1.5 h behind the point (beyond the flat side's h/2 margin).
    let mirror = Pt::new(m.at.x - 1.5 * hh * u.x, m.at.y - 1.5 * hh * u.y);
    assert_eq!(ecad_cadcraft::references::reference_at(&doc, page, centre, 0.1), Some(m.reference), "pick the text");
    assert_eq!(ecad_cadcraft::references::reference_at(&doc, page, mirror, 0.1), None, "the box follows the flag");
}

/// Fixture R's page 1: the wires come from the left, the flag points right.
#[test]
fn the_flag_points_right_when_the_wires_come_from_the_left() {
    let (mut r, _) = continued();
    assert_flag_away(&mut r.h, r.supply, r.c1, Pt::new(1.0, 0.0));
}

/// Fixture R's page 2 (the continued end): the wires leave to the right, the flag points left and
/// `/1.2` sits left of the point, clear of the wires.
#[test]
fn the_flag_points_left_when_the_wires_leave_to_the_right() {
    let (mut r, _) = continued();
    assert_flag_away(&mut r.h, r.field, r.c1, Pt::new(-1.0, 0.0));
}

/// Three vertical wires (x = 0, 5, 10, y = 0 to 60) slashed into cable C1, with a reference at `at`.
fn vertical_cable(at: [f64; 2]) -> (H, PageId, CableId) {
    let mut h = H::new(SEED);
    let page = h.first_page();
    for x in [0.0, 5.0, 10.0] {
        h.run("wire", json!({"points": [[x, 0.0], [x, 60.0]]})).unwrap();
    }
    let c1: CableId =
        id(&h.run("cableslash", json!({"from": [-3, 28], "to": [13, 32], "tag": "C1"})).unwrap(), "cable");
    h.run("cableref", json!({"cable": c1.hex(), "at": at})).unwrap();
    (h, page, c1)
}

/// Wires coming from below: the flag sits above the point and points up.
#[test]
fn the_flag_points_up_when_the_wires_come_from_below() {
    let (mut h, page, c1) = vertical_cable([5.0, 65.0]);
    assert_flag_away(&mut h, page, c1, Pt::new(0.0, 1.0));
}

/// Wires coming from above: the flag sits below the point and points down.
#[test]
fn the_flag_points_down_when_the_wires_come_from_above() {
    let (mut h, page, c1) = vertical_cable([5.0, -5.0]);
    assert_flag_away(&mut h, page, c1, Pt::new(0.0, -1.0));
}

// ---------------------------------------------------------------- stale pages (brief 014 fix round)

/// The reference texts drawn on `page` (the drawing as it is, not a fresh derivation).
fn drawn_texts(h: &mut H, page: PageId) -> Vec<String> {
    h.pv(|p, pg| {
        let Some(d) = pg.drawing_of(p, page) else { return Vec::new() };
        let view = &p.pages[&page];
        view.map
            .ref_text_handles()
            .filter_map(|hd| d.model.iter().find(|e| e.handle == hd))
            .map(|e| format!("{:?}", e.kind))
            .collect()
    })
}

/// Brief 016: only the active page is drawn. A commit on Field that changes Supply's reference
/// text draws nothing of Supply (it has no drawing); Supply shows the new text once it is made
/// active. A commit that changes nothing Supply shows logs a change that leaves Supply out.
#[test]
fn only_the_active_page_is_drawn_and_another_page_shows_the_new_text_when_made_active() {
    let mut r = connected();
    r.h.activate(r.supply);
    r.h.check();
    assert!(drawn_texts(&mut r.h, r.supply).iter().any(|t| t.contains("/2.1")));

    // A commit on Field that changes Supply's text (Field's point moves to column 3).
    r.h.activate(r.field);
    assert!(r.h.pv(|p, pg| pg.drawing_of(p, r.supply)).is_none(), "one page window: Supply is not drawn");
    let fid = reference_on(&mut r.h, r.field);
    let fpoint = r.h.doc().representations[&fid].items[0];
    let fh = r.h.handle_of(r.field, fpoint);
    let n = r.h.p(|p| p.change_count());
    r.h.run("move", json!({"handles": [fh.hex()], "delta": [70, 0]})).unwrap();
    assert_eq!(text_of(&mut r.h, r.supply), "/2.3", "the document says the new text");
    let changed = r.h.p(|p| p.changes_since(n)).expect("pages known");
    assert!(changed.contains(&r.supply) && changed.contains(&r.field), "{changed:?}");
    r.h.check();

    r.h.activate(r.supply);
    let texts = drawn_texts(&mut r.h, r.supply);
    assert!(texts.iter().any(|t| t.contains("/2.3")), "{texts:?}");
    assert!(!texts.iter().any(|t| t.contains("/2.1")), "{texts:?}");
    r.h.check();

    // A commit that moves the cable's wires on Field (K2 moves, its wires follow) but does not
    // change what Supply shows: the change log leaves Supply out.
    let k2 = r.h.p(|p| p.ledger.document().clone()).terminals[&r.k2[0]].component;
    r.h.activate(r.field);
    let handles = r.h.rep_handles(r.field, k2);
    let n = r.h.p(|p| p.change_count());
    let head = r.h.head();
    r.h.run("move", json!({"handles": handles, "delta": [0, 0.5]})).unwrap();
    assert_eq!(r.h.head(), head + 1, "K2 moved");
    let changed = r.h.p(|p| p.changes_since(n)).expect("pages known");
    assert!(!changed.contains(&r.supply), "Supply's texts did not change: {changed:?}");
    r.h.check();
}

/// Brief 016: undo on the active page of an edit made there stays on that page; another page
/// shows the reverted reference text when it is made active.
#[test]
fn undo_on_the_page_of_the_edit_stays_and_other_pages_show_the_reverted_text_when_active() {
    let mut r = connected();
    r.h.activate(r.field);
    let fid = reference_on(&mut r.h, r.field);
    let fpoint = r.h.doc().representations[&fid].items[0];
    let fh = r.h.handle_of(r.field, fpoint);
    r.h.run("move", json!({"handles": [fh.hex()], "delta": [70, 0]})).unwrap();
    r.h.activate(r.supply);
    assert!(drawn_texts(&mut r.h, r.supply).iter().any(|t| t.contains("/2.3")));

    r.h.activate(r.field);
    let head = r.h.head();
    r.h.run("u", Value::Null).unwrap();
    assert_eq!(r.h.head(), head + 1, "one undo");
    assert_eq!(r.h.p(|p| p.active_page()), Some(r.field), "the undo was on Field: Field stays");
    assert_eq!(text_of(&mut r.h, r.supply), "/2.1", "the document says the reverted text");
    r.h.check();

    r.h.activate(r.supply);
    let texts = drawn_texts(&mut r.h, r.supply);
    assert!(texts.iter().any(|t| t.contains("/2.1")), "{texts:?}");
    assert!(!texts.iter().any(|t| t.contains("/2.3")), "{texts:?}");
    r.h.check();
}

/// Owner's two-page trial (brief 014 fix round), with one page window (brief 016, AC8): page 2's
/// cable end is erased (page 1 shows `/?`), then page 1 is made active and the erase is undone
/// from there. The undone edit was on page 2, so page 2 comes to the front with its end back, and
/// page 1 shows `/2.1` when it is active again.
#[test]
fn undo_from_another_page_brings_the_page_of_the_edit_to_the_front() {
    let mut r = connected();
    r.h.activate(r.field);
    let doc = r.h.doc();
    let keep: Vec<cadcraft_engine::doc::Handle> = doc
        .representations
        .values()
        .filter(|x| x.page == r.field && matches!(x.subject, Subject::Component(_)))
        .flat_map(|x| x.items.iter())
        .map(|i| r.h.handle_of(r.field, *i))
        .collect();
    let erase: Vec<String> =
        r.h.pv(|p, pg| pg.entities_of(p, r.field))
            .iter()
            .map(|e| e.handle)
            .collect::<Vec<_>>()
            .into_iter()
            .filter(|h| !matches!(r.h.mapped(r.field, *h), Some(Mapped::Marker(..))) && !keep.contains(h))
            .map(|h| h.hex())
            .collect();
    let field_before = r.h.pv(|p, pg| pg.entities_of(p, r.field)).len();
    r.h.run("erase", json!({ "handles": erase })).unwrap();
    r.h.activate(r.supply);
    let texts = drawn_texts(&mut r.h, r.supply);
    assert!(texts.iter().any(|t| t.contains("/?")), "{texts:?}");

    let head = r.h.head();
    r.h.run("u", Value::Null).unwrap();
    assert_eq!(r.h.head(), head + 1, "one undo");
    assert_eq!(r.h.p(|p| p.active_page()), Some(r.field), "the page of the undone edit is in front");
    assert_eq!(r.h.pv(|p, pg| pg.entities_of(p, r.field)).len(), field_before, "Field's end is back");
    assert!(r.h.s.log.iter().any(|l| l == "Page 2 \"Field\" brought to the front."), "{:?}", r.h.s.log);
    assert_eq!(text_of(&mut r.h, r.supply), "/2.1");
    r.h.check();
    // Redo (typed, as the owner did) stays on Field, where the change is.
    r.h.cmdline("redo");
    assert_eq!(r.h.p(|p| p.active_page()), Some(r.field));
    r.h.activate(r.supply);
    assert!(drawn_texts(&mut r.h, r.supply).iter().any(|t| t.contains("/?")), "redo erased the end again");
    r.h.cmdline("u");
    assert_eq!(r.h.p(|p| p.active_page()), Some(r.field), "typed u brings Field to the front too");
    r.h.activate(r.supply);
    assert!(drawn_texts(&mut r.h, r.supply).iter().any(|t| t.contains("/2.1")));
    r.h.check();
}
