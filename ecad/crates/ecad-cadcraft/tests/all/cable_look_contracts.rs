//! Brief 009 (cable look and cores), written from the brief and design 009: core labels
//! (`cablecore`, §5), the restyled fan (§4), core labels at the slash (§6), reference ends (§7,
//! built with brief 010's model: a `reference` representation of the cable holding one Point
//! item) and hover (§8). Every `cablecore` check drives the JSON form, so it can move to the MCP
//! harness once brief 011 is merged.
//!
//! Fixture H (design §10): two drawn components with connection points at x = 0 and x = 120,
//! y = 0, 5, 10, 15; four attached wires W1–W4 between them; the slash C1 from (58,-3) to (62,18),
//! with designation `MOTOR M1` and type `4/C #10`.
#![cfg(not(target_arch = "wasm32"))]

use crate::common;

use cadcraft_engine::Input;
use cadcraft_engine::doc::{EntityKind, Handle, Lineweight};
use cadcraft_engine::ext::MarkShape;
use cadcraft_engine::geom::Vec2;
use common::{H, id};
use ecad_cadcraft::interact::{hover_lines, overlay_marks};
use ecad_cadcraft::look::{self, MarkerEnd, Role, Shape};
use ecad_cadcraft::views::{self, FanOnPage};
use ecad_model::{
    CableId, Color, ConductorId, Form, Item, ItemId, ItemKind, Op, PageId, Primitive, Pt, Representation, RepresentationId, Style, Subject,
    Transaction,
};
use serde_json::{Value, json};

const SEED: u64 = 9_009;

struct Fx {
    h: H,
    page: PageId,
    c1: CableId,
    wires: [ConductorId; 4],
}

fn component(h: &mut H, page: PageId, line: &str, x: f64) {
    let before = h.lines_on(page);
    h.cmdline(line);
    let lines: Vec<Handle> = h.lines_on(page).into_iter().filter(|l| !before.contains(l)).collect();
    h.run("convertcomponent", json!({"handles": lines.iter().map(|l| l.hex()).collect::<Vec<_>>()})).unwrap();
    for y in [0.0, 5.0, 10.0, 15.0] {
        h.run("addterminal", json!({"handle": lines[0].hex(), "at": [x, y]})).unwrap();
    }
}

/// Fixture H, or H1 (`right` false: no right-hand component, the wires end loose at x = 120).
fn fixture(right: bool) -> Fx {
    let mut h = H::new(SEED);
    h.s.viewport_px = (1280.0, 800.0);
    let page = h.first_page();
    component(&mut h, page, "line -10,-5 0,-5 0,20 -10,20 c", 0.0);
    if right {
        component(&mut h, page, "line 120,-5 130,-5 130,20 120,20 c", 120.0);
    }
    let mut wires = Vec::new();
    for (i, y) in [0.0, 5.0, 10.0, 15.0].into_iter().enumerate() {
        let w: ConductorId = id(&h.run("wire", json!({"points": [[0.0, y], [120.0, y]]})).unwrap(), "conductor");
        h.run("setprop", json!({"conductor": w.hex(), "key": "tag", "value": format!("W{}", i + 1)})).unwrap();
        wires.push(w);
    }
    let c1: CableId = id(&h.run("cableslash", json!({"from": [58, -3], "to": [62, 18], "tag": "C1"})).unwrap(), "cable");
    h.run("setprop", json!({"cable": c1.hex(), "key": "designation", "value": "MOTOR M1"})).unwrap();
    h.run("setprop", json!({"cable": c1.hex(), "key": "type", "value": "4/C #10"})).unwrap();
    h.run("zoom", json!({"mode": "window", "p1": [-20, -30], "p2": [140, 50]})).unwrap();
    let wires = [wires[0], wires[1], wires[2], wires[3]];
    Fx { h, page, c1, wires }
}

fn core(h: &mut H, w: ConductorId, core: Value) -> Result<Value, String> {
    h.run("cablecore", json!({"conductor": w.hex(), "core": core}))
}

fn cores(h: &mut H, k: CableId) -> Vec<Option<String>> {
    h.doc().cables[&k].members.iter().map(|m| m.core.clone()).collect()
}

fn last_tx(h: &mut H) -> Transaction {
    h.p(|p| p.ledger.log().last().cloned().map(|t| Transaction { time_ms: None, ..t }).unwrap())
}

fn view(h: &mut H, k: CableId, v: &str) {
    h.run("cableview", json!({"cable": k.hex(), "view": v})).unwrap();
}

fn fan(h: &mut H, page: PageId, k: CableId) -> views::Fan {
    let doc = h.doc();
    let tol = h.p(|p| p.tolerance);
    match views::fan_on_page(&doc, page, k, tol) {
        FanOnPage::Drawn(f) => f,
        other => panic!("not drawn: {other:?}"),
    }
}

fn of_role(f: &views::Fan, role: Role) -> Vec<&Shape> {
    f.parts.iter().filter(|p| p.role == role).map(|p| &p.shape).collect()
}

fn path(s: &Shape) -> Vec<Pt> {
    match s {
        Shape::Path { points, .. } => points.clone(),
        other => panic!("not a path: {other:?}"),
    }
}

fn near(a: Pt, b: Pt) -> bool {
    a.near(b, 1e-9)
}

fn same_segment(p: &[Pt], a: Pt, b: Pt) -> bool {
    matches!(p, [x, y] if (near(*x, a) && near(*y, b)) || (near(*x, b) && near(*y, a)))
}

fn click(h: &mut H, x: f64, y: f64) -> cadcraft_engine::Result<()> {
    let at = Vec2::new(x, y);
    h.s.cursor = at;
    h.s.input(Input::Point(at))
}

fn prompt(h: &H) -> String {
    h.s.current_prompt().map(|p| p.display()).unwrap_or_default()
}

/// What a refused input said: its error, or the line the engine echoed for it.
fn said(h: &H, r: cadcraft_engine::Result<()>) -> String {
    match r {
        Err(e) => e.to_string(),
        Ok(()) => last_echo(h),
    }
}

fn last_echo(h: &H) -> String {
    h.s.log.last().cloned().unwrap_or_default()
}

/// Add a reference end of `cable` at `at` on `page` with brief 010's ops: a Point item and a
/// `reference` representation holding it (one transaction).
fn reference_end(h: &mut H, page: PageId, cable: CableId, at: Pt, n: u128) -> (ItemId, RepresentationId) {
    // Combined trial of 009 and 010: the first reference end of a cable on a page is placed with
    // 010's `cableref`; a second one there (010 refuses it; only a hand-edited file has one) is still
    // built with the ops.
    if let Ok(r) = h.run("cableref", json!({"cable": cable.hex(), "at": [at.x, at.y]})) {
        h.check();
        return (id(&r, "item"), id(&r, "reference"));
    }
    let layer = h.p(|p| p.default_layer);
    let item = ItemId::from(0x0900_0000_0000_0000_0000_0000_0000_0000 + n);
    let rep = RepresentationId::from(0x0901_0000_0000_0000_0000_0000_0000_0000 + n);
    let style = Style { layer, color: Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 };
    let ops = vec![
        Op::AddItem { id: item, item: Item { page, style, kind: ItemKind::Primitive(Primitive::Point { at }) } },
        Op::LinkRepresentation {
            id: rep,
            rep: Representation {
                subject: Subject::Cable(cable),
                page,
                form: Form::Drawn,
                role: look::REFERENCE_ROLE.into(),
                items: vec![item],
                connection_points: Default::default(),
            },
        },
    ];
    let before = h.head();
    h.project.apply_ops(&mut h.s, "Cable reference", ops).unwrap();
    assert_eq!(h.head(), before + 1);
    h.check();
    (item, rep)
}

// --------------------------------------------------------------------------- §5 cablecore

/// Check B (AC3): the JSON form records exactly the remove/add pair at the member's index, and
/// nothing else changes.
#[test]
fn b_json_core_label_is_one_transaction_of_two_existing_ops() {
    let mut f = fixture(true);
    let (before, head) = (f.h.doc(), f.h.head());
    let r = core(&mut f.h, f.wires[0], json!("1")).unwrap();
    assert_eq!(r, json!({"cable": f.c1.hex(), "conductor": f.wires[0].hex(), "index": 0, "core": "1", "changed": true}));
    assert_eq!(f.h.head(), head + 1);
    let t = last_tx(&mut f.h);
    assert_eq!(t.label, "Core label");
    let member = |core: Option<&str>| ecad_model::CableMember { conductor: f.wires[0], core: core.map(str::to_string) };
    assert_eq!(
        t.ops,
        vec![
            Op::RemoveCableMember { cable: f.c1, index: 0, member: member(None) },
            Op::AddCableMember { cable: f.c1, index: 0, member: member(Some("1")) },
        ]
    );
    let after = f.h.doc();
    let mut expect = before.clone();
    expect.cables.get_mut(&f.c1).unwrap().members[0].core = Some("1".into());
    assert_eq!(after, expect, "only the core changed");
    // The same value again: no transaction.
    let r = core(&mut f.h, f.wires[0], json!("1")).unwrap();
    assert_eq!(r["changed"], false);
    assert_eq!(f.h.head(), head + 1);
    f.h.check();
}

/// Check C (AC5): the interactive form records the same transaction as the JSON form.
#[test]
fn c_interactive_and_json_forms_record_identical_transactions() {
    let mut a = fixture(true);
    core(&mut a.h, a.wires[0], json!("1")).unwrap();
    let json_tx = last_tx(&mut a.h);
    let mut b = fixture(true);
    b.h.s.start("cablecore").unwrap();
    assert_eq!(prompt(&b.h), "Select a wire in a cable:");
    click(&mut b.h, 30.0, 0.0).unwrap();
    assert_eq!(prompt(&b.h), "Core number or colour for W1 <none>:");
    b.h.s.input(Input::Text("1".into())).unwrap();
    assert_eq!(last_echo(&b.h), "Wire W1 is now core 1 of cable -C01.");
    assert_eq!(prompt(&b.h), "Select a wire in a cable:", "back to step 1");
    b.h.s.input(Input::Enter).unwrap();
    assert!(b.h.s.running.is_none());
    assert_eq!(last_tx(&mut b.h).ops, json_tx.ops);
    assert_eq!(last_tx(&mut b.h).label, json_tx.label);
    b.h.check();
}

/// Check A and §5.1 step 2x: the prompts, the picked wire named, and picks that are not a member.
#[test]
fn a_interactive_prompts_and_refusals() {
    let mut f = fixture(true);
    f.h.s.start("cablecore").unwrap();
    click(&mut f.h, 30.0, 10.0).unwrap();
    assert_eq!(prompt(&f.h), "Core number or colour for W3 <none>:");
    f.h.s.input(Input::Text("3".into())).unwrap();
    assert_eq!(last_echo(&f.h), "Wire W3 is now core 3 of cable -C01.");
    // Empty space, the slash, a component.
    let r = click(&mut f.h, 60.0, 40.0);
    let e = said(&f.h, r);
    assert!(e.contains("Nothing there. Click a wire in a cable."), "{e}");
    let r = click(&mut f.h, 58.0 + 4.0 * 0.5 / 21.0, -2.5);
    let e = said(&f.h, r);
    assert!(e.contains("That is the cable; click one of its wires."), "{e}");
    let r = click(&mut f.h, -10.0, 2.0);
    let e = said(&f.h, r);
    assert!(e.contains("That is not a wire in a cable."), "{e}");
    // Enter with nothing typed changes nothing; `.` clears; `.` on an unset label writes nothing.
    click(&mut f.h, 30.0, 10.0).unwrap();
    assert_eq!(prompt(&f.h), "Core number or colour for W3 <3>:");
    let head = f.h.head();
    f.h.s.input(Input::Enter).unwrap();
    assert_eq!(last_echo(&f.h), "Core of W3 unchanged.");
    assert_eq!(f.h.head(), head);
    click(&mut f.h, 30.0, 10.0).unwrap();
    f.h.s.input(Input::Text(".".into())).unwrap();
    assert_eq!(last_echo(&f.h), "Core label of W3 cleared.");
    assert_eq!(f.h.head(), head + 1);
    click(&mut f.h, 30.0, 10.0).unwrap();
    f.h.s.input(Input::Text(".".into())).unwrap();
    assert_eq!(last_echo(&f.h), "W3 has no core label.");
    assert_eq!(f.h.head(), head + 1);
    // A refused label repeats the same wire's prompt.
    click(&mut f.h, 30.0, 10.0).unwrap();
    let r = f.h.s.input(Input::Text("x".repeat(33)));
    let e = said(&f.h, r);
    assert!(e.contains("A core label can be at most 32 characters."), "{e}");
    assert_eq!(prompt(&f.h), "Core number or colour for W3 <none>:");
    f.h.s.input(Input::Cancel).unwrap();
    assert!(f.h.s.running.is_none());
    f.h.check();
}

/// Check D (AC3): undo and redo restore the label in one step; replay equals the live state.
#[test]
fn d_undo_redo_and_replay() {
    let mut f = fixture(true);
    let before = f.h.doc();
    core(&mut f.h, f.wires[0], json!("1")).unwrap();
    let head = f.h.head();
    f.h.run("u", Value::Null).unwrap();
    assert_eq!(cores(&mut f.h, f.c1)[0], None);
    assert_eq!(f.h.doc(), before, "one step back");
    assert_eq!(f.h.head(), head + 1, "the undo is one transaction");
    f.h.run("redo", Value::Null).unwrap();
    assert_eq!(cores(&mut f.h, f.c1)[0].as_deref(), Some("1"));
    f.h.check();
}

/// Check E (AC3, AC6): `null`, `""` and spaces clear with the same transaction; clearing an unset
/// label writes nothing; labels are trimmed.
#[test]
fn e_every_clear_is_the_same_transaction() {
    let mut txs = Vec::new();
    for clear in [json!(null), json!(""), json!("   ")] {
        let mut f = fixture(true);
        core(&mut f.h, f.wires[1], json!("  2 ")).unwrap();
        assert_eq!(cores(&mut f.h, f.c1)[1].as_deref(), Some("2"), "trimmed");
        let head = f.h.head();
        let r = core(&mut f.h, f.wires[1], clear.clone()).unwrap();
        assert_eq!(r["core"], Value::Null);
        assert_eq!(f.h.head(), head + 1);
        txs.push(last_tx(&mut f.h).ops);
        let r = core(&mut f.h, f.wires[1], clear).unwrap();
        assert_eq!((r["changed"].as_bool(), f.h.head()), (Some(false), head + 1));
    }
    assert!(txs.windows(2).all(|w| w[0] == w[1]));
}

/// Check M (AC6) for `cablecore`: hostile input never panics and is refused with nothing written.
#[test]
fn m_hostile_core_input_is_refused() {
    let mut f = fixture(true);
    let head = f.h.head();
    let refused = |h: &mut H, params: Value, words: &str| {
        let e = h.run("cablecore", params.clone()).unwrap_err();
        assert!(e.contains(words), "{params}: {e}");
    };
    let w = f.wires[0].hex();
    refused(&mut f.h, json!({"conductor": w, "core": "x".repeat(33)}), "A core label can be at most 32 characters.");
    refused(&mut f.h, json!({"conductor": w, "core": "a\nb"}), "A core label cannot contain control characters.");
    refused(&mut f.h, json!({"conductor": w, "core": 7}), "`core` must be a string, or null to clear it");
    refused(&mut f.h, json!({"conductor": w}), "`core` must be a string, or null to clear it");
    refused(&mut f.h, json!({"core": "1"}), "`conductor` (hex id) is required");
    refused(&mut f.h, json!({"conductor": "zz", "core": "1"}), "`conductor` (hex id) is required");
    refused(&mut f.h, json!({"conductor": "ab", "core": "1"}), "no such conductor");
    refused(&mut f.h, json!({"cable": f.c1.hex(), "cores": ["1"]}), "`cores` must list one label (or null) per member: cable -C01 has 4 members");
    refused(&mut f.h, json!({"cable": f.c1.hex(), "cores": "1"}), "`cores` must list one label");
    refused(&mut f.h, json!({"cable": f.c1.hex(), "cores": ["1", "2", "a\tb", "4"]}), "Core 3: A core label cannot");
    assert_eq!(f.h.head(), head, "nothing written");
    // 32 characters (counted in characters, not bytes) are accepted.
    let long = "é".repeat(32);
    assert_eq!(core(&mut f.h, f.wires[0], json!(long)).unwrap()["core"], json!(long));
    // A wire in no cable.
    let loose: ConductorId = id(&f.h.run("wire", json!({"points": [[0, 60], [50, 60]]})).unwrap(), "conductor");
    f.h.run("setprop", json!({"conductor": loose.hex(), "key": "tag", "value": "W9"})).unwrap();
    refused(&mut f.h, json!({"conductor": loose.hex(), "core": "1"}), "Wire W9 is not in a cable.");
    f.h.check();
}

/// Check N: the batch form records one transaction with a pair per changed member, in member
/// order.
#[test]
fn n_batch_cores_are_one_transaction_in_member_order() {
    let mut f = fixture(true);
    core(&mut f.h, f.wires[2], json!("3")).unwrap();
    let head = f.h.head();
    let r = f.h.run("cablecore", json!({"cable": f.c1.hex(), "cores": ["1", "2", null, "4"]})).unwrap();
    assert_eq!(r, json!({"cable": f.c1.hex(), "cores": ["1", "2", null, "4"], "changed": true}));
    assert_eq!(f.h.head(), head + 1);
    let t = last_tx(&mut f.h);
    assert_eq!(t.label, "Core labels");
    let indexes: Vec<usize> = t
        .ops
        .iter()
        .map(|o| match o {
            Op::RemoveCableMember { index, .. } | Op::AddCableMember { index, .. } => *index,
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(indexes, vec![0, 0, 1, 1, 2, 2, 3, 3]);
    assert_eq!(cores(&mut f.h, f.c1), vec![Some("1".into()), Some("2".into()), None, Some("4".into())]);
    let r = f.h.run("cablecore", json!({"cable": f.c1.hex(), "cores": ["1", "2", null, "4"]})).unwrap();
    assert_eq!((r["changed"].as_bool(), f.h.head()), (Some(false), head + 1));
    f.h.check();
}

/// Check O: duplicate labels are accepted with a note.
#[test]
fn o_duplicates_are_allowed_with_a_note() {
    let mut f = fixture(true);
    core(&mut f.h, f.wires[0], json!("1")).unwrap();
    let r = core(&mut f.h, f.wires[1], json!("1")).unwrap();
    assert_eq!(r["duplicates"], json!([f.wires[0].hex()]));
    f.h.s.start("cablecore").unwrap();
    click(&mut f.h, 30.0, 10.0).unwrap();
    f.h.s.input(Input::Text("1".into())).unwrap();
    let log = f.h.s.log.clone();
    assert!(log.iter().any(|l| l == "Note: core 1 is also used by W1, W2 in this cable."), "{log:?}");
}

// --------------------------------------------------------------------------- §6, §4 drawing

/// Check F (AC4): core labels just past the slash crossings in traditional view, none for a
/// member without a label.
#[test]
fn f_traditional_core_labels_at_the_slash() {
    let mut f = fixture(true);
    f.h.run("cablecore", json!({"cable": f.c1.hex(), "cores": ["1", "2", null, "4"]})).unwrap();
    let texts: Vec<(Pt, String)> =
        f.h.pv(|p, pg| pg.entities_of(p, f.page))
            .iter()
            .filter(|e| e.common.layer == views::LAYER)
            .filter_map(|e| match &e.kind {
                EntityKind::Text(t) => Some((Pt::new(t.insert.x, t.insert.y), t.value.clone())),
                _ => None,
            })
            .collect();
    assert_eq!(texts.len(), 3, "{texts:?}");
    // The slash (58,-3)–(62,18) crosses y = 0, 5, 15 at these x.
    for (y, label) in [(0.0, "1"), (5.0, "2"), (15.0, "4")] {
        let x = 58.0 + 4.0 * (y + 3.0) / 21.0;
        let want = Pt::new(x + 1.25, y + 0.75);
        assert!(texts.iter().any(|(at, v)| v == label && at.near(want, 1e-9)), "{label} at {want:?} in {texts:?}");
    }
    f.h.check();
}

/// Check G (AC1, AC4): the fan of H is §4.6's worked example, with the trunk joining the middle of
/// each spine (owner request of 2026-11-07, replacing §4.2 step 6's end join).
#[test]
fn g_fan_is_the_worked_example() {
    let mut f = fixture(true);
    f.h.run("cablecore", json!({"cable": f.c1.hex(), "cores": ["1", "2", "3", "4"]})).unwrap();
    view(&mut f.h, f.c1, "fan");
    let fan = fan(&mut f.h, f.page, f.c1);
    let spines: Vec<Vec<Pt>> = of_role(&fan, Role::Spine).into_iter().map(path).collect();
    assert_eq!(spines.len(), 2);
    // The join at the middle (y = 7.5): each core bends away from it; the spine spans the bends.
    assert!(spines.iter().any(|s| same_segment(s, Pt::new(22.5, 2.5), Pt::new(22.5, 12.5))), "{spines:?}");
    assert!(spines.iter().any(|s| same_segment(s, Pt::new(97.5, 2.5), Pt::new(97.5, 12.5))), "{spines:?}");
    let bends: Vec<(Pt, f64)> = of_role(&fan, Role::Bend)
        .into_iter()
        .map(|s| match s {
            Shape::Arc { center, radius, .. } => (*center, *radius),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(bends.len(), 8);
    let tails: Vec<Vec<Pt>> = of_role(&fan, Role::Tail).into_iter().map(path).collect();
    assert_eq!(tails.len(), 8);
    for y in [0.0, 5.0, 10.0, 15.0] {
        let cy = if y < 7.5 { y + 2.5 } else { y - 2.5 };
        for cx in [20.0, 100.0] {
            assert!(bends.iter().any(|(c, r)| near(*c, Pt::new(cx, cy)) && *r == 2.5), "bend at {cx},{y}");
        }
        assert!(tails.iter().any(|t| same_segment(t, Pt::new(20.0, y), Pt::new(0.0, y))), "left tail {y}");
        assert!(tails.iter().any(|t| same_segment(t, Pt::new(100.0, y), Pt::new(120.0, y))), "right tail {y}");
    }
    // The trunk runs along y = 7.5 between the junctions, clipped by the box centred at (60,7.5).
    let trunk: Vec<Vec<Pt>> = of_role(&fan, Role::Trunk).into_iter().map(path).collect();
    assert_eq!(trunk.len(), 2, "{trunk:?}");
    assert!(trunk.iter().flatten().all(|p| (p.y - 7.5).abs() < 1e-9));
    let xs: Vec<f64> = trunk.iter().flatten().map(|p| p.x).collect();
    assert!(xs.iter().any(|x| (x - 22.5).abs() < 1e-9) && xs.iter().any(|x| (x - 97.5).abs() < 1e-9));
    let frame = path(of_role(&fan, Role::LabelBox)[0]);
    let centre = Pt::new(frame.iter().map(|p| p.x).sum::<f64>() / 4.0, frame.iter().map(|p| p.y).sum::<f64>() / 4.0);
    assert!(near(centre, Pt::new(60.0, 7.5)), "{centre:?}");
    let lines: Vec<String> = of_role(&fan, Role::BoxText)
        .into_iter()
        .map(|s| match s {
            Shape::Text { value, .. } => value.clone(),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(lines, ["-C01", "MOTOR M1", "4/C #10"]);
    // Labels at the tail start: on the left, `1` right-aligned at x = 19, `W1` further left.
    let text = |role: Role, value: &str| -> Pt {
        fan.parts
            .iter()
            .find_map(|p| match &p.shape {
                Shape::Text { at, value: v, .. } if p.role == role && v == value => Some(*at),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{value}"))
    };
    let one = fan
        .parts
        .iter()
        .filter(|p| p.role == Role::CoreLabel)
        .filter_map(|p| match &p.shape {
            Shape::Text { at, value, .. } if value == "1" && at.x < 60.0 => Some(*at),
            _ => None,
        })
        .next()
        .unwrap();
    assert!((one.x + look::font_width("1") - 19.0).abs() < 1e-9 && (one.y - 0.75).abs() < 1e-9, "{one:?}");
    let _ = text(Role::WireNumber, "W1");
    // Drawn on the reserved layer, trunk and spines thick, bends, tails and frame thin.
    let ents = f.h.pv(|p, pg| pg.entities_of(p, f.page));
    let on_layer: Vec<_> = ents.iter().filter(|e| e.common.layer == views::LAYER).collect();
    assert_eq!(on_layer.len(), fan.parts.len());
    let thick = on_layer.iter().filter(|e| e.common.lineweight == Lineweight::Mm100(70)).count();
    let thin = on_layer.iter().filter(|e| e.common.lineweight == Lineweight::Mm100(25)).count();
    assert_eq!(thick, 2 + 2, "two trunk pieces and two spines");
    assert_eq!(thin, 8 + 8 + 1, "bends, tails and the box frame");
    f.h.check();
}

/// Check H (AC2): one end stops at a reference end; erasing it fans that end again; undo brings
/// it back.
#[test]
fn h_one_sided_fan_stops_at_the_reference_end() {
    let mut f = fixture(false);
    view(&mut f.h, f.c1, "fan");
    let (point, rep) = reference_end(&mut f.h, f.page, f.c1, Pt::new(125.0, 21.25), 1);
    let fan = fan(&mut f.h, f.page, f.c1);
    assert_eq!(fan.markers.get(&rep), Some(&MarkerEnd::B), "the r₁ side (x = 120)");
    assert_eq!(of_role(&fan, Role::Spine).len(), 1, "only the left end fans");
    let tails: Vec<Vec<Pt>> = of_role(&fan, Role::Tail).into_iter().map(path).collect();
    assert_eq!(tails.len(), 4);
    assert!(tails.iter().all(|t| t.iter().all(|p| p.x <= 20.0 + 1e-9)), "{tails:?}");
    // The trunk runs from J₀ (the middle of the left spine) to the attach point M = (123.75, 22.5),
    // in axis-aligned steps.
    let trunk: Vec<Pt> = of_role(&fan, Role::Trunk).into_iter().flat_map(path).collect();
    assert!(trunk.iter().any(|p| near(*p, Pt::new(22.5, 7.5))), "{trunk:?}");
    assert!(trunk.iter().any(|p| near(*p, Pt::new(123.75, 22.5))), "{trunk:?}");
    for piece in of_role(&fan, Role::Trunk).into_iter().map(path) {
        assert!(piece.windows(2).all(|w| (w[0].x - w[1].x).abs() < 1e-9 || (w[0].y - w[1].y).abs() < 1e-9));
    }
    // The flag points right, away from the cable.
    let flag = path(of_role(&fan, Role::Flag)[0]);
    assert!(flag.iter().any(|p| p.x > 125.0 + look::font_width(look::REFERENCE_PLACEHOLDER) + 1.25 + 1e-9));
    // No loose-end squares at x = 120.
    let uid = f.h.p(|p| p.pages[&f.page].uid).unwrap();
    let squares: Vec<Vec2> = f.h.p(|p| overlay_marks(p, uid)).into_iter().filter(|m| m.shape == MarkShape::Square).map(|m| m.at).collect();
    assert!(squares.iter().all(|q| (q.x - 120.0).abs() > 1e-6), "{squares:?}");
    // Hover of the reference point.
    let h_point = f.h.handle_of(f.page, point);
    let lines = f.h.p(|p| hover_lines(p, uid, h_point)).unwrap();
    // Combined trial with 010: a reference end hovers with 010's lens (one source of hover text).
    assert_eq!(lines.first().map(String::as_str), Some("Cable reference  ?"), "{lines:?}");
    // ERASE the reference point: both ends fan again; `u` brings it back.
    f.h.run("erase", json!({"handles": [h_point.hex()]})).unwrap();
    assert!(!f.h.doc().representations.contains_key(&rep));
    assert_eq!(of_role(&fan_again(&mut f), Role::Spine).len(), 2);
    f.h.run("u", Value::Null).unwrap();
    assert!(f.h.doc().representations.contains_key(&rep));
    assert_eq!(of_role(&fan_again(&mut f), Role::Spine).len(), 1);
    f.h.check();
}

fn fan_again(f: &mut Fx) -> views::Fan {
    let (page, k) = (f.page, f.c1);
    fan(&mut f.h, page, k)
}

/// Check I (AC2): both ends at reference ends: a straight trunk between the attach points, no
/// spines, bends or tails.
#[test]
fn i_both_ends_at_reference_ends() {
    let mut f = fixture(false);
    view(&mut f.h, f.c1, "fan");
    reference_end(&mut f.h, f.page, f.c1, Pt::new(125.0, 21.25), 1);
    reference_end(&mut f.h, f.page, f.c1, Pt::new(-30.0, 21.25), 2);
    let fan = fan(&mut f.h, f.page, f.c1);
    assert!(of_role(&fan, Role::Spine).is_empty() && of_role(&fan, Role::Tail).is_empty());
    assert!(of_role(&fan, Role::Bend).is_empty());
    let trunk: Vec<Pt> = of_role(&fan, Role::Trunk).into_iter().flat_map(path).collect();
    // Each flag points away from the wires (fix round 1 of the stack): the right one reaches right,
    // the left one (wires to its right) reaches left, so both flat sides, where the trunk attaches,
    // face the cable, h/2 from their points.
    assert!(trunk.iter().any(|p| near(*p, Pt::new(125.0 - 1.25, 22.5))), "{trunk:?}");
    assert!(trunk.iter().any(|p| near(*p, Pt::new(-30.0 + 1.25, 22.5))), "{trunk:?}");
    assert_eq!(of_role(&fan, Role::Flag).len(), 2);
    f.h.check();
}

/// Check J (AC7): switching views with cores and a reference end present changes nothing in the
/// project.
#[test]
fn j_switching_stays_presentation_only() {
    let mut f = fixture(false);
    f.h.run("cablecore", json!({"cable": f.c1.hex(), "cores": ["1", "2", "3", "4"]})).unwrap();
    reference_end(&mut f.h, f.page, f.c1, Pt::new(125.0, 21.25), 1);
    let (doc, head, nets) = (f.h.doc(), f.h.head(), ecad_model::derive::nets(&f.h.doc()));
    let unsaved = f.h.p(|p| p.unsaved_transactions());
    for _ in 0..10 {
        f.h.run("cableview", json!({"cable": f.c1.hex()})).unwrap();
        f.h.check();
    }
    assert_eq!((f.h.doc(), f.h.head()), (doc, head));
    assert_eq!(ecad_model::derive::nets(&f.h.doc()), nets);
    assert_eq!(f.h.p(|p| p.unsaved_transactions()), unsaved);
}

/// Check K: hover names cores, designation, type and reference ends; the debugger trail labels a
/// member with its core.
#[test]
fn k_hover_and_trail_show_cores() {
    let mut f = fixture(true);
    core(&mut f.h, f.wires[2], json!("3")).unwrap();
    let uid = f.h.p(|p| p.pages[&f.page].uid).unwrap();
    let route = f.h.route_handle(f.page, f.wires[2]);
    let lines = f.h.p(|p| hover_lines(p, uid, route)).unwrap();
    assert!(lines.iter().any(|l| l == "In cable -C01, core 3"), "{lines:?}");
    let slash = f.h.doc().representations.values().find(|r| r.subject == Subject::Cable(f.c1)).unwrap().items[0];
    let h_slash = f.h.handle_of(f.page, slash);
    let lines = f.h.p(|p| hover_lines(p, uid, h_slash)).unwrap();
    for want in ["Designation: MOTOR M1", "Type: 4/C #10", "  W3  core 3", "  W1"] {
        assert!(lines.iter().any(|l| l == want), "{want} in {lines:?}");
    }
    // A tail in fan view.
    view(&mut f.h, f.c1, "fan");
    let tail = f.h.p(|p| {
        let map = &p.pages[&f.page].map;
        map.fan_handles().find(|h| map.core_of(*h) == Some(f.wires[2])).unwrap()
    });
    let lines = f.h.p(|p| hover_lines(p, uid, tail)).unwrap();
    assert_eq!(lines.get(1).map(String::as_str), Some("Core: 3 (W3)"), "{lines:?}");
    let doc = f.h.doc();
    assert_eq!(ecad_cadcraft::debug::object_label(&doc, ecad_cadcraft::debug::ObjectId::Conductor(f.wires[2])), "W3 · core 3");
    let snap = f.h.run("ecadinspect", json!({})).unwrap();
    assert!(snap.to_string().contains("\"core\":\"3\""), "ecadinspect shows the core");
}

/// Check L: computed drawing cannot be edited; the reference point can be moved.
#[test]
fn l_edits_of_computed_parts_are_refused() {
    let mut f = fixture(true);
    core(&mut f.h, f.wires[0], json!("1")).unwrap();
    // A traditional core label.
    let label = f.h.p(|p| p.pages[&f.page].map.fan_handles().next().unwrap());
    let head = f.h.head();
    let e = f.h.run("erase", json!({"handles": [label.hex()]})).unwrap_err();
    assert!(e.contains(views::DRAWN_REFUSED), "{e}");
    // A tail and the label box in fan view.
    view(&mut f.h, f.c1, "fan");
    let tail = f.h.p(|p| {
        let map = &p.pages[&f.page].map;
        map.fan_handles().find(|h| map.core_of(*h).is_some()).unwrap()
    });
    let e = f.h.run("erase", json!({"handles": [tail.hex()]})).unwrap_err();
    assert!(e.contains(views::FAN_REFUSED), "{e}");
    assert_eq!(f.h.head(), head);
    f.h.check();
}

// --------------------------------------------------------------------------- hostile geometry

/// Check M (AC6): a 20-core cable fans with 20 bends and tails per end; long tags are drawn
/// shortened; non-finite reference points and routes never panic.
#[test]
fn m_hostile_geometry_never_panics() {
    let mut h = H::new(SEED + 1);
    let page = h.first_page();
    let mut wires = Vec::new();
    for i in 0..20 {
        let y = f64::from(i) * 5.0;
        wires.push(id::<ConductorId>(&h.run("wire", json!({"points": [[0.0, y], [120.0, y]]})).unwrap(), "conductor"));
    }
    h.run("setprop", json!({"conductor": wires[0].hex(), "key": "tag", "value": "x".repeat(1000)})).unwrap();
    let k: CableId = id(&h.run("cableslash", json!({"from": [60, -3], "to": [60, 100]})).unwrap(), "cable");
    let cores: Vec<String> = (1..=20).map(|i| i.to_string()).collect();
    h.run("cablecore", json!({"cable": k.hex(), "cores": cores})).unwrap();
    view(&mut h, k, "fan");
    let f = fan(&mut h, page, k);
    assert_eq!((of_role(&f, Role::Bend).len(), of_role(&f, Role::Tail).len()), (40, 40));
    let long = format!("{}…", "x".repeat(23));
    assert!(f.parts.iter().any(|p| matches!(&p.shape, Shape::Text { value, .. } if *value == long)));
    // Spines lie beyond every tail.
    for s in of_role(&f, Role::Spine).into_iter().map(path) {
        let x = s[0].x;
        assert!(of_role(&f, Role::Tail).into_iter().map(path).all(|t| t.iter().all(|p| (p.x - x).abs() > 1e-9)));
    }
    h.check();
    // A reference point that is not finite, and a route near ±1e12: no panic.
    let mut doc = h.doc();
    let layer = h.p(|p| p.default_layer);
    let style = Style { layer, color: Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 };
    let item = ItemId::from(77);
    doc.items.insert(item, Item { page, style, kind: ItemKind::Primitive(Primitive::Point { at: Pt::new(f64::NAN, 0.0) }) });
    doc.representations.insert(
        RepresentationId::from(78),
        Representation {
            subject: Subject::Cable(k),
            page,
            form: Form::Drawn,
            role: look::REFERENCE_ROLE.into(),
            items: vec![item],
            connection_points: Default::default(),
        },
    );
    let d = look::cable_on_page(&doc, page, k, true, 1e-6, &look::font_width);
    assert!(d.is_fan());
    let (route, _) = ecad_model::derive::route_on_page(&doc, wires[3], page).unwrap();
    doc.items.get_mut(&route).unwrap().kind = ItemKind::Primitive(Primitive::Line { a: Pt::new(-1e12, 15.0), b: Pt::new(f64::INFINITY, 15.0) });
    let d = look::cable_on_page(&doc, page, k, true, 1e-6, &look::font_width);
    assert!(!d.is_fan(), "drawn traditional when a value is not finite");
    // Two wires ending at the same point (pitch 0).
    let mut doc = h.doc();
    let (r1, _) = ecad_model::derive::route_on_page(&doc, wires[1], page).unwrap();
    doc.items.get_mut(&r1).unwrap().kind = ItemKind::Primitive(Primitive::Line { a: Pt::new(0.0, 0.0), b: Pt::new(120.0, 0.0) });
    let _ = look::cable_on_page(&doc, page, k, true, 1e-6, &look::font_width);
}

// --------------------------------------------------------------------------- review S2

/// A labelled cable of three loose wires, its handles (wires, slash, core labels) and the labels'.
fn loose_labelled_cable(h: &mut H) -> (PageId, CableId, Vec<Handle>, Vec<Handle>) {
    let page = h.first_page();
    for y in [0.0, 5.0, 10.0] {
        h.run("wire", json!({"points": [[0.0, y], [60.0, y]]})).unwrap();
    }
    let k: CableId = id(&h.run("cableslash", json!({"from": [28, -3], "to": [32, 13]})).unwrap(), "cable");
    h.run("cablecore", json!({"cable": k.hex(), "cores": ["1", "2", "3"]})).unwrap();
    let (all, labels) = h.p(|p| {
        let map = &p.pages[&page].map;
        let labels: Vec<Handle> = map.fan_handles().collect();
        (map.handles().collect::<Vec<_>>(), labels)
    });
    assert_eq!(labels.len(), 3, "three core labels at the slash");
    (page, k, all, labels)
}

/// Review S2: a selection holding a labelled cable's core labels and its wires and slash moves,
/// copies or erases the wires and slash; the labels are left out and drawn again. Labels alone
/// are still refused.
#[test]
fn s2_selections_with_core_labels_edit_the_rest() {
    for (cmd, extra) in [("move", json!({"delta": [0, 40]})), ("copy", json!({"delta": [0, 40]})), ("erase", json!({}))] {
        let mut h = H::new(SEED + 2);
        let (page, k, all, _) = loose_labelled_cable(&mut h);
        let head = h.head();
        let mut params = extra.clone();
        params["handles"] = json!(all.iter().map(|x| x.hex()).collect::<Vec<_>>());
        h.run(cmd, params).unwrap_or_else(|e| panic!("{cmd}: {e}"));
        assert_eq!(h.head(), head + 1, "{cmd}: one transaction");
        h.check();
        let doc = h.doc();
        let texts =
            h.pv(|p, pg| pg.entities_of(p, page)).iter().filter(|e| e.common.layer == views::LAYER && matches!(e.kind, EntityKind::Text(_))).count();
        match cmd {
            "move" => {
                assert_eq!(texts, 3, "{cmd}: labels drawn again at the moved slash");
                assert!(doc.items.values().all(|i| !matches!(i.kind, ItemKind::Primitive(Primitive::Text { .. }))));
            }
            "copy" => assert_eq!(texts, 3, "{cmd}: the copy is no cable; its labels are not copied as text"),
            _ => {
                assert!(slash_items(&doc, page, k).is_empty(), "{cmd}: the slash is gone");
                assert_eq!(texts, 0, "{cmd}: no slash, no labels");
            }
        }
        // Labels alone: refused, nothing written.
        let mut h = H::new(SEED + 2);
        let (_, _, _, labels_only) = loose_labelled_cable(&mut h);
        let head = h.head();
        let mut params = extra;
        params["handles"] = json!(labels_only.iter().map(|x| x.hex()).collect::<Vec<_>>());
        let e = h.run(cmd, params).unwrap_err();
        assert!(e.contains(views::DRAWN_REFUSED), "{cmd}: {e}");
        assert_eq!(h.head(), head);
    }
}

fn slash_items(doc: &ecad_model::Document, page: PageId, k: CableId) -> Vec<ItemId> {
    ecad_cadcraft::slash::slashes_of(doc, page, k).into_iter().map(|(i, _, _)| i).collect()
}

// --------------------------------------------------------------------------- the fan's join

fn join_of(f: &views::Fan, left: bool) -> Pt {
    // The junction: the trunk point on that end's spine line (x = 22.5 left, 97.5 right).
    let x = if left { 22.5 } else { 97.5 };
    of_role(f, Role::Trunk).into_iter().flat_map(path).find(|p| (p.x - x).abs() < 1e-9).unwrap_or_else(|| panic!("no junction at x = {x}"))
}

/// Owner request (2026-11-07): the trunk joins the middle of each spine by default; `cablefan`
/// moves one end's join along its spine (a point snapped and clamped onto the spine, a fraction
/// from 0 to 1, or null for the middle), as one transaction of existing ops; undo, redo and replay
/// work; hostile values are refused. JSON-form contract, ready for the MCP harness (brief 011).
#[test]
fn cablefan_moves_the_join_along_the_spine() {
    let mut f = fixture(true);
    view(&mut f.h, f.c1, "fan");
    let k = f.c1.hex();
    assert!(near(join_of(&fan(&mut f.h, f.page, f.c1), true), Pt::new(22.5, 7.5)), "the middle by default");
    let (head, before) = (f.h.head(), f.h.doc());
    // A point snapped onto the spine: (30, 11) -> (22.5, 11) on the left end.
    let r = f.h.run("cablefan", json!({"cable": k, "at": [30, 11]})).unwrap();
    assert_eq!(r, json!({"cable": k, "end": "A", "at": [22.5, 11.0], "changed": true}));
    assert_eq!(f.h.head(), head + 1);
    let t = last_tx(&mut f.h);
    assert_eq!(t.label, "Cable join");
    assert!(matches!(t.ops.as_slice(), [Op::AddItem { .. }, Op::LinkRepresentation { .. }]), "{:?}", t.ops);
    let fan1 = fan(&mut f.h, f.page, f.c1);
    assert!(near(join_of(&fan1, true), Pt::new(22.5, 11.0)));
    assert!(near(join_of(&fan1, false), Pt::new(97.5, 7.5)), "the other end stays in the middle");
    f.h.check();
    // Clamped to the cores' range: far above -> the top core's level (y = 15).
    let r = f.h.run("cablefan", json!({"cable": k, "end": "A", "at": [22.5, 400]})).unwrap();
    assert_eq!(r["at"], json!([22.5, 15.0]));
    assert!(matches!(last_tx(&mut f.h).ops.as_slice(), [Op::ModifyItem { .. }]), "moving it is one ModifyItem");
    // A fraction, on the other end; the same value again changes nothing.
    let r = f.h.run("cablefan", json!({"cable": k, "end": "B", "at": 0.0})).unwrap();
    assert_eq!(r["changed"], true);
    let h1 = f.h.head();
    let r = f.h.run("cablefan", json!({"cable": k, "end": "B", "at": 0.0})).unwrap();
    assert_eq!((r["changed"].as_bool(), f.h.head()), (Some(false), h1));
    // The stored joins are not drawn; the page is consistent; undo and redo; null resets.
    let anchors = f.h.doc().representations.values().filter(|r| r.role == look::ANCHOR_ROLE).count();
    assert_eq!(anchors, 2);
    f.h.check();
    f.h.run("u", Value::Null).unwrap();
    f.h.run("redo", Value::Null).unwrap();
    f.h.check();
    f.h.run("cablefan", json!({"cable": k, "end": "A", "at": null})).unwrap();
    f.h.run("cablefan", json!({"cable": k, "end": "B", "at": null})).unwrap();
    assert!(near(join_of(&fan(&mut f.h, f.page, f.c1), true), Pt::new(22.5, 7.5)));
    assert_eq!(f.h.doc().representations.len(), before.representations.len(), "both joins removed");
    // Works whatever the view (programmatic calls ignore views).
    view(&mut f.h, f.c1, "traditional");
    f.h.run("cablefan", json!({"cable": k, "end": "A", "at": 1.0})).unwrap();
    f.h.check();
    // Hostile input: nothing written.
    let head = f.h.head();
    let cases = [
        (json!({"cable": k, "at": [1e300, 0]}), "Point out of range."),
        (json!({"cable": k, "at": 1.5, "end": "A"}), "`at` as a fraction must be from 0 to 1"),
        (json!({"cable": k, "at": 0.5}), "is required unless `at` is a point"),
        (json!({"cable": k, "at": "x"}), "`at` must be a point"),
        (json!({"cable": k, "end": "C", "at": 0.5}), "`end` must be"),
        (json!({"cable": "zz", "at": 0.5}), "`cable` (hex id) is required"),
        (json!({"cable": "ab", "end": "A", "at": 0.5}), "no such cable"),
    ];
    for (params, words) in cases {
        let e = f.h.run("cablefan", params.clone()).unwrap_err();
        assert!(e.contains(words), "{params}: {e}");
    }
    assert_eq!(f.h.head(), head);
}

/// The interactive form commits the same transaction as the JSON form; an end that stops at a
/// reference has no spine.
#[test]
fn cablefan_interactive_equals_json_and_reference_ends_have_no_spine() {
    let mut a = fixture(true);
    view(&mut a.h, a.c1, "fan");
    a.h.run("cablefan", json!({"cable": a.c1.hex(), "end": "A", "at": [22.5, 11.0]})).unwrap();
    let json_ops = last_tx(&mut a.h).ops;
    let mut b = fixture(true);
    view(&mut b.h, b.c1, "fan");
    b.h.s.start("cablefan").unwrap();
    assert_eq!(prompt(&b.h), "Select a cable shown as a fan, near the end to change:");
    click(&mut b.h, 22.5, 4.0).unwrap();
    assert_eq!(prompt(&b.h), "Specify where the cable joins or [Middle]:");
    click(&mut b.h, 30.0, 11.0).unwrap();
    assert_eq!(last_echo(&b.h), "Cable -C01 now joins end A at 22.5, 11.");
    assert!(b.h.s.running.is_none());
    let ops = last_tx(&mut b.h).ops;
    // Same ops apart from the fresh IDs of the new item and representation.
    let strip = |ops: &[Op]| -> Vec<Op> {
        ops.iter()
            .map(|o| match o.clone() {
                Op::AddItem { item, .. } => Op::AddItem { id: ItemId::from(1), item },
                Op::LinkRepresentation { mut rep, .. } => {
                    rep.items = vec![ItemId::from(1)];
                    Op::LinkRepresentation { id: RepresentationId::from(2), rep }
                }
                other => other,
            })
            .collect()
    };
    assert_eq!(strip(&ops), strip(&json_ops));
    // A one-sided fan: the end at the reference has no spine.
    let mut c = fixture(false);
    view(&mut c.h, c.c1, "fan");
    reference_end(&mut c.h, c.page, c.c1, Pt::new(125.0, 21.25), 1);
    let e = c.h.run("cablefan", json!({"cable": c.c1.hex(), "end": "B", "at": 0.5})).unwrap_err();
    assert!(e.contains("End B of cable -C01 stops at a reference; it has no spine."), "{e}");
    c.h.run("cablefan", json!({"cable": c.c1.hex(), "end": "A", "at": 1.0})).unwrap();
    c.h.check();
}
