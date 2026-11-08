//! Independent tester's contract tests for brief 009 (cable look and cores), written from the
//! brief, design 009 and the overseer's decision that reference ends use brief 010's model (a
//! `reference` representation of the cable holding one Point item, built directly here; there is
//! no `cableref` command in 009).
//!
//! - AC2: one-sided fans at either end (fixture H1 and its mirror H1′), both ends at references, a
//!   third reference that stops nothing.
//! - AC3: `cablecore` undo, redo, save and reopen, replay equal to the live state, also in fan view.
//! - AC4: core labels at the slash for vertical and right-to-left wires; labels at the tail start
//!   on both sides of fixture H (design §4.5).
//! - AC5: the interactive and JSON forms record identical transactions for a table of inputs,
//!   including picks of a fan tail and of a traditional core label.
//! - AC6: a `cablecore` JSON contract table (valid, missing, invalid, wrong types, out of range);
//!   long and empty labels stored by another route; NaN, infinite and overflowing geometry; a
//!   20-core cable at a tight pitch, horizontal and vertical.
//! - AC7: brief 005 criteria 1, 2 and 4 with cores and a reference end present.
#![cfg(not(target_arch = "wasm32"))]

mod common;

use std::path::PathBuf;

use cadcraft_engine::doc::EntityKind;
use cadcraft_engine::geom::Vec2;
use cadcraft_engine::{Input, Session};
use common::{H, id};
use ecad_cadcraft::interact::{hover_lines, overlay_marks};
use ecad_cadcraft::look::{self, MarkerEnd, Role, Shape};
use ecad_cadcraft::views::{self, FanOnPage, Settings};
use ecad_cadcraft::{Persistence, Project, ProjectHandle};
use ecad_model::{
    CableId, CableMember, Color, ConductorId, Form, Item, ItemId, ItemKind, Op, PageId, Primitive, Pt, Representation,
    RepresentationId, Style, Subject, Transaction,
};
use serde_json::{Value, json};

const SEED: u64 = 0x0009_7e57;

// ------------------------------------------------------------------------------------- helpers

struct Tmp(PathBuf);

impl Tmp {
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("ecad-009-tester-{name}-{}", ecad_store::disk::unique()));
        std::fs::create_dir_all(&p).unwrap();
        Tmp(p)
    }
    fn path(&self, name: &str) -> String {
        self.0.join(name).display().to_string()
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Fx {
    h: H,
    page: PageId,
    c1: CableId,
    wires: Vec<ConductorId>,
}

/// A drawn component at `x` with terminals at y = 0, 5, 10, 15.
fn component(h: &mut H, page: PageId, line: &str, x: f64) {
    let before = h.lines_on(page);
    h.cmdline(line);
    let lines: Vec<_> = h.lines_on(page).into_iter().filter(|l| !before.contains(l)).collect();
    h.run("convertcomponent", json!({"handles": lines.iter().map(|l| l.hex()).collect::<Vec<_>>()})).unwrap();
    for y in [0.0, 5.0, 10.0, 15.0] {
        h.run("addterminal", json!({"handle": lines[0].hex(), "at": [x, y]})).unwrap();
    }
}

/// Fixture H of design §10 on `h`; `left` / `right`: draw the component on that side (H1 has no
/// right component, H1′ no left one).
fn fixture_on(mut h: H, left: bool, right: bool) -> Fx {
    h.s.viewport_px = (1280.0, 800.0);
    let page = h.first_page();
    if left {
        component(&mut h, page, "line -10,-5 0,-5 0,20 -10,20 c", 0.0);
    }
    if right {
        component(&mut h, page, "line 120,-5 130,-5 130,20 120,20 c", 120.0);
    }
    let mut wires = Vec::new();
    for (i, y) in [0.0, 5.0, 10.0, 15.0].into_iter().enumerate() {
        let w: ConductorId = id(&h.run("wire", json!({"points": [[0.0, y], [120.0, y]]})).unwrap(), "conductor");
        h.run("setprop", json!({"conductor": w.hex(), "key": "tag", "value": format!("W{}", i + 1)})).unwrap();
        wires.push(w);
    }
    let c1: CableId =
        id(&h.run("cableslash", json!({"from": [58, -3], "to": [62, 18], "tag": "C1"})).unwrap(), "cable");
    h.run("setprop", json!({"cable": c1.hex(), "key": "designation", "value": "MOTOR M1"})).unwrap();
    h.run("setprop", json!({"cable": c1.hex(), "key": "type", "value": "4/C #10"})).unwrap();
    h.run("zoom", json!({"mode": "window", "p1": [-30, -30], "p2": [140, 50]})).unwrap();
    Fx { h, page, c1, wires }
}

fn fixture(left: bool, right: bool) -> Fx {
    fixture_on(H::new(SEED), left, right)
}

fn last_tx(h: &mut H) -> Transaction {
    h.p(|p| p.ledger.log().last().cloned().map(|t| Transaction { time_ms: None, ..t }).unwrap())
}

fn cores(h: &mut H, k: CableId) -> Vec<Option<String>> {
    h.doc().cables[&k].members.iter().map(|m| m.core.clone()).collect()
}

fn view(h: &mut H, k: CableId, v: &str) {
    h.run("cableview", json!({"cable": k.hex(), "view": v})).unwrap();
}

fn drawing(h: &mut H, page: PageId, k: CableId, fan: bool) -> look::CableDrawing {
    let doc = h.doc();
    let tol = h.p(|p| p.tolerance);
    look::cable_on_page(&doc, page, k, fan, tol, &look::font_width)
}

fn fan(h: &mut H, page: PageId, k: CableId) -> views::Fan {
    let doc = h.doc();
    let tol = h.p(|p| p.tolerance);
    match views::fan_on_page(&doc, page, k, tol) {
        FanOnPage::Drawn(f) => f,
        other => panic!("not drawn: {other:?}"),
    }
}

fn of_role(f: &look::CableDrawing, role: Role) -> Vec<&look::Part> {
    f.parts.iter().filter(|p| p.role == role).collect()
}

fn path(s: &Shape) -> Vec<Pt> {
    match s {
        Shape::Path { points, .. } => points.clone(),
        other => panic!("not a path: {other:?}"),
    }
}

fn text_of(p: &look::Part) -> (Pt, f64, String) {
    match &p.shape {
        Shape::Text { at, rotation, value } => (*at, *rotation, value.clone()),
        other => panic!("not a text: {other:?}"),
    }
}

fn near(a: Pt, b: Pt) -> bool {
    a.near(b, 1e-9)
}

fn all_finite(d: &look::CableDrawing) -> bool {
    d.parts.iter().all(|p| match &p.shape {
        Shape::Path { points, .. } => points.iter().all(|q| q.is_finite()),
        Shape::Arc { center, radius, start, end } => {
            center.is_finite() && radius.is_finite() && start.is_finite() && end.is_finite()
        }
        Shape::Text { at, rotation, .. } => at.is_finite() && rotation.is_finite(),
    })
}

fn click(h: &mut H, x: f64, y: f64) -> cadcraft_engine::Result<()> {
    let at = Vec2::new(x, y);
    h.s.cursor = at;
    h.s.input(Input::Point(at))
}

fn prompt(h: &H) -> String {
    h.s.current_prompt().map(|p| p.display()).unwrap_or_default()
}

fn style(h: &mut H) -> Style {
    let layer = h.p(|p| p.default_layer);
    Style { layer, color: Color::ByLayer, linetype: "ByLayer".into(), lineweight: -1 }
}

/// A reference end of `cable` at `at` (brief 010's model: one Point item in a `reference`
/// representation), one transaction.
fn reference_end(h: &mut H, page: PageId, cable: CableId, at: Pt, n: u128) -> (ItemId, RepresentationId) {
    // Combined trial of 009 and 010: the first reference end of a cable on a page is placed with
    // 010's `cableref`; a second one there (010 refuses it; only a hand-edited file has one) is still
    // built with the ops.
    if let Ok(r) = h.run("cableref", json!({"cable": cable.hex(), "at": [at.x, at.y]})) {
        h.check();
        return (id(&r, "item"), id(&r, "reference"));
    }
    let item = ItemId::from(0x0977_0000_0000_0000_0000_0000_0000_0000 + n);
    let rep = RepresentationId::from(0x0978_0000_0000_0000_0000_0000_0000_0000 + n);
    let style = style(h);
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

/// Store `core` on member `index` of `cable` with the design §5.3 pair, bypassing `cablecore`'s
/// validation (a label stored "by another route", design §4.5).
fn store_core_raw(h: &mut H, cable: CableId, index: usize, core: Option<&str>) {
    let m = h.doc().cables[&cable].members[index].clone();
    let ops = vec![
        Op::RemoveCableMember { cable, index, member: m.clone() },
        Op::AddCableMember {
            cable,
            index,
            member: CableMember { conductor: m.conductor, core: core.map(str::to_string) },
        },
    ];
    h.project.apply_ops(&mut h.s, "Core label", ops).unwrap();
    h.check();
}

/// The squares (loose ends) of the overlay on `page`.
fn squares(h: &mut H, page: PageId) -> Vec<Vec2> {
    let uid = h.p(|p| p.pages[&page].uid).unwrap();
    h.p(|p| overlay_marks(p, uid))
        .into_iter()
        .filter(|m| m.shape == cadcraft_engine::ext::MarkShape::Square)
        .map(|m| m.at)
        .collect()
}

/// Texts on the reserved layer on `page`: (insert, rotation, value).
fn layer_texts(h: &mut H, page: PageId) -> Vec<(Pt, f64, String)> {
    h.pv(|p, pg| pg.entities_of(p, page))
        .iter()
        .filter(|e| e.common.layer == views::LAYER)
        .filter_map(|e| match &e.kind {
            EntityKind::Text(t) => Some((Pt::new(t.insert.x, t.insert.y), t.rotation, t.value.clone())),
            _ => None,
        })
        .collect()
}

// --------------------------------------------------------------------------- AC6 / JSON table

/// `cablecore` JSON form contract (design §5.2, §5.4): one row per input. `Ok(core)` rows write
/// one transaction storing `core` at member 0; `Err(words)` rows write nothing.
#[test]
fn cablecore_json_contract_table() {
    struct Row {
        name: &'static str,
        params: fn(&Fx) -> Value,
        want: Result<Option<&'static str>, &'static str>,
    }
    let rows = [
        Row { name: "string", params: |f| json!({"conductor": f.wires[0].hex(), "core": "1"}), want: Ok(Some("1")) },
        Row {
            name: "colour with spaces inside, trimmed outside",
            params: |f| json!({"conductor": f.wires[0].hex(), "core": "  gn/ye 1 "}),
            want: Ok(Some("gn/ye 1")),
        },
        Row {
            name: "32 scalar values",
            params: |f| json!({"conductor": f.wires[0].hex(), "core": "ü".repeat(32)}),
            want: Ok(Some("üüüüüüüüüüüüüüüüüüüüüüüüüüüüüüüü")),
        },
        Row {
            name: "upper-case hex id",
            params: |f| json!({"conductor": f.wires[0].hex().to_uppercase(), "core": "1"}),
            want: Ok(Some("1")),
        },
        Row {
            name: "33 characters",
            params: |f| json!({"conductor": f.wires[0].hex(), "core": "x".repeat(33)}),
            want: Err("A core label can be at most 32 characters."),
        },
        Row {
            name: "1000 characters",
            params: |f| json!({"conductor": f.wires[0].hex(), "core": "x".repeat(1000)}),
            want: Err("A core label can be at most 32 characters."),
        },
        Row {
            name: "carriage return",
            params: |f| json!({"conductor": f.wires[0].hex(), "core": "1\r"}),
            want: Ok(Some("1")),
        },
        Row {
            name: "tab inside",
            params: |f| json!({"conductor": f.wires[0].hex(), "core": "1\t2"}),
            want: Err("A core label cannot contain control characters."),
        },
        Row {
            name: "NUL inside",
            params: |f| json!({"conductor": f.wires[0].hex(), "core": "1\u{0}2"}),
            want: Err("A core label cannot contain control characters."),
        },
        Row {
            name: "core a number",
            params: |f| json!({"conductor": f.wires[0].hex(), "core": 7}),
            want: Err("`core` must be a string, or null to clear it"),
        },
        Row {
            name: "core a bool",
            params: |f| json!({"conductor": f.wires[0].hex(), "core": true}),
            want: Err("`core` must be a string, or null to clear it"),
        },
        Row {
            name: "core an array",
            params: |f| json!({"conductor": f.wires[0].hex(), "core": ["1"]}),
            want: Err("`core` must be a string, or null to clear it"),
        },
        Row {
            name: "core an object",
            params: |f| json!({"conductor": f.wires[0].hex(), "core": {"v": "1"}}),
            want: Err("`core` must be a string, or null to clear it"),
        },
        Row {
            name: "core missing",
            params: |f| json!({"conductor": f.wires[0].hex()}),
            want: Err("`core` must be a string, or null to clear it"),
        },
        Row { name: "empty object", params: |_| json!({}), want: Err("`conductor` (hex id) is required") },
        Row { name: "null params", params: |_| Value::Null, want: Err("`conductor` (hex id) is required") },
        Row {
            name: "conductor a number",
            params: |_| json!({"conductor": 12, "core": "1"}),
            want: Err("`conductor` (hex id) is required"),
        },
        Row {
            name: "conductor not hex",
            params: |_| json!({"conductor": "W1", "core": "1"}),
            want: Err("`conductor` (hex id) is required"),
        },
        Row {
            name: "conductor overflows 128 bits",
            params: |_| json!({"conductor": "f".repeat(40), "core": "1"}),
            want: Err("`conductor` (hex id) is required"),
        },
        Row {
            name: "a cable id as conductor",
            params: |f| json!({"conductor": f.c1.hex(), "core": "1"}),
            want: Err("no such conductor"),
        },
        Row {
            name: "batch, cores too short",
            params: |f| json!({"cable": f.c1.hex(), "cores": []}),
            want: Err("`cores` must list one label (or null) per member: cable C1 has 4 members"),
        },
        Row {
            name: "batch, cores too long (out of range core 5)",
            params: |f| json!({"cable": f.c1.hex(), "cores": ["1", "2", "3", "4", "5"]}),
            want: Err("`cores` must list one label (or null) per member: cable C1 has 4 members"),
        },
        Row {
            name: "batch, cores an object",
            params: |f| json!({"cable": f.c1.hex(), "cores": {"0": "1"}}),
            want: Err("`cores` must list one label (or null) per member"),
        },
        Row {
            name: "batch, cores null",
            params: |f| json!({"cable": f.c1.hex(), "cores": null}),
            want: Err("`cores` must list one label (or null) per member"),
        },
        Row {
            name: "batch, an entry a number",
            params: |f| json!({"cable": f.c1.hex(), "cores": ["1", 2, "3", "4"]}),
            want: Err("Core 2: `core` must be a string, or null to clear it"),
        },
        Row {
            name: "batch, an entry too long",
            params: |f| json!({"cable": f.c1.hex(), "cores": ["1", "2", "3", "y".repeat(33)]}),
            want: Err("Core 4: A core label can be at most 32 characters."),
        },
        Row {
            name: "batch, cable missing",
            params: |_| json!({"cores": ["1", "2", "3", "4"]}),
            want: Err("`cable` (hex id) is required"),
        },
        Row {
            name: "batch, no such cable",
            params: |f| json!({"cable": f.wires[0].hex(), "cores": ["1", "2", "3", "4"]}),
            want: Err("no such cable"),
        },
    ];
    let mut failures = Vec::new();
    for row in &rows {
        let mut f = fixture(true, true);
        let (head, doc) = (f.h.head(), f.h.doc());
        let params = (row.params)(&f);
        let r = f.h.run("cablecore", params.clone());
        match (&row.want, r) {
            (Ok(core), Ok(v)) => {
                if f.h.head() != head + 1 || cores(&mut f.h, f.c1)[0].as_deref() != *core {
                    failures.push(format!("{}: {params} gave {v}, cores {:?}", row.name, cores(&mut f.h, f.c1)));
                }
            }
            (Err(words), Err(e)) => {
                if !e.contains(words) || f.h.head() != head || f.h.doc() != doc {
                    failures
                        .push(format!("{}: {params} refused with {e:?}, want {words:?}, nothing written", row.name));
                }
            }
            (want, got) => failures.push(format!("{}: {params}: want {want:?}, got {got:?}", row.name)),
        }
        f.h.check();
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// AC3: a core label survives save and reopen, also with a reference end and in fan view after the
/// reopen; the reopened log and document equal the saved ones and replay equals the live state.
#[test]
fn ac3_save_and_reopen_keep_core_labels_and_replay_matches() {
    let work = Tmp::new("reopen");
    let settings = Tmp::new("reopen-settings");
    ecad_cadcraft::install();
    views::isolate_for_tests();
    let mut s = Session::empty();
    let handle = Project::create_with(&mut s, "Cores", SEED, Persistence::File).unwrap();
    let mut f = fixture_on(H { s, project: handle, trail: Vec::new() }, true, false);
    f.h.run("cablecore", json!({"cable": f.c1.hex(), "cores": ["1", "BK", "grün/gelb", null]})).unwrap();
    f.h.run("cablecore", json!({"conductor": f.wires[3].hex(), "core": "PE"})).unwrap();
    let (_, rep) = reference_end(&mut f.h, f.page, f.c1, Pt::new(125.0, 21.25), 1);
    let path = work.path("cores.ecad");
    f.h.run("saveas", json!({"path": path})).unwrap();
    assert!(!f.h.p(|p| p.unsaved()));
    let (doc, log) = (f.h.doc(), f.h.p(|p| p.ledger.log().to_vec()));
    while !f.h.s.docs.is_empty() {
        f.h.s.execute("close", &json!({"index": 0})).unwrap();
    }
    let mut s = Session::empty();
    let (handle, summary) = ProjectHandle::open_file_in(&mut s, &path, 3, Settings::at(&settings.0)).unwrap();
    assert!(!summary.contains("could not"), "{summary}");
    let mut h = H { s, project: handle, trail: Vec::new() };
    assert_eq!(h.doc(), doc, "document unchanged by save and reopen");
    assert_eq!(h.p(|p| p.ledger.log().to_vec()), log, "log unchanged by save and reopen");
    assert_eq!(
        cores(&mut h, f.c1),
        vec![Some("1".into()), Some("BK".into()), Some("grün/gelb".into()), Some("PE".into())]
    );
    assert!(h.p(|p| p.ledger.replay_matches().unwrap()));
    h.check();
    // The labels are drawn at once (traditional, at the slash) and in fan view after a switch.
    let texts: Vec<String> = layer_texts(&mut h, f.page).into_iter().map(|t| t.2).collect();
    for want in ["1", "BK", "grün/gelb", "PE"] {
        assert!(texts.iter().any(|t| t == want), "{want} in {texts:?}");
    }
    view(&mut h, f.c1, "fan");
    let d = fan(&mut h, f.page, f.c1);
    assert_eq!(d.markers.get(&rep), Some(&MarkerEnd::B));
    let labels: Vec<String> = of_role(&d, Role::CoreLabel).into_iter().map(|p| text_of(p).2).collect();
    assert_eq!(labels.len(), 4, "{labels:?}");
    // Undo after the reopen takes back the last real edit (the reference end), then the PE label.
    h.run("u", Value::Null).unwrap();
    assert!(!h.doc().representations.contains_key(&rep));
    h.run("u", Value::Null).unwrap();
    assert_eq!(cores(&mut h, f.c1)[3], None);
    h.run("redo", Value::Null).unwrap();
    assert_eq!(cores(&mut h, f.c1)[3].as_deref(), Some("PE"));
    h.check();
}

/// AC3: undo and redo of a core label in fan view change the drawn tail label in the same step,
/// and the projection equals a fresh one each time (`check`).
#[test]
fn ac3_undo_redo_in_fan_view_redraws_the_label() {
    let mut f = fixture(true, true);
    view(&mut f.h, f.c1, "fan");
    let labels = |h: &mut H| -> Vec<String> {
        let mut v: Vec<String> = layer_texts(h, f.page).into_iter().map(|t| t.2).filter(|t| t == "7").collect();
        v.sort();
        v
    };
    assert!(labels(&mut f.h).is_empty());
    f.h.run("cablecore", json!({"conductor": f.wires[2].hex(), "core": "7"})).unwrap();
    assert_eq!(labels(&mut f.h).len(), 2, "one label at each fanned end");
    let head = f.h.head();
    f.h.run("u", Value::Null).unwrap();
    assert_eq!(f.h.head(), head + 1, "undo is one transaction");
    assert!(labels(&mut f.h).is_empty());
    f.h.run("redo", Value::Null).unwrap();
    assert_eq!(labels(&mut f.h).len(), 2);
    assert!(f.h.p(|p| p.ledger.replay_matches().unwrap()));
    f.h.check();
}

// --------------------------------------------------------------------------- AC5

/// AC5: for each typed input, the interactive form records exactly the JSON form's transaction
/// (ops and label), whether the wire is picked by its route, by its fan tail or by its
/// traditional core label.
#[test]
fn ac5_interactive_equals_json_for_a_table_of_inputs() {
    // (typed text, the JSON `core`, the wire's label before)
    let cases: [(&str, Value, Option<&str>); 6] = [
        ("1", json!("1"), None),
        ("  BK  ", json!("  BK  "), None),
        ("grün/gelb", json!("grün/gelb"), None),
        (&"z".repeat(32), json!("z".repeat(32)), None),
        ("2", json!("2"), Some("1")),
        (".", Value::Null, Some("1")),
    ];
    // How the wire (W3, y = 10) is picked: (name, fan view, point on the canvas).
    let picks: [(&str, bool, (f64, f64)); 3] = [
        ("route", false, (30.0, 10.0)),
        ("fan tail", true, (10.0, 10.0)),
        ("traditional core label", false, (0.0, 0.0)),
    ];
    for (typed, value, before) in &cases {
        for (how, fan_view, at) in picks {
            if how == "traditional core label" && before.is_none() {
                continue; // no label to pick
            }
            let setup = |f: &mut Fx| {
                if let Some(b) = before {
                    f.h.run("cablecore", json!({"conductor": f.wires[2].hex(), "core": b})).unwrap();
                }
                if fan_view {
                    view(&mut f.h, f.c1, "fan");
                }
            };
            let mut a = fixture(true, true);
            setup(&mut a);
            a.h.run("cablecore", json!({"conductor": a.wires[2].hex(), "core": value})).unwrap();
            let json_tx = last_tx(&mut a.h);
            let mut b = fixture(true, true);
            setup(&mut b);
            let at = if how == "traditional core label" {
                // The label's own insertion point, just past the slash crossing on W3.
                layer_texts(&mut b.h, b.page).into_iter().find(|t| Some(t.2.as_str()) == *before).unwrap().0
            } else {
                Pt::new(at.0, at.1)
            };
            let head = b.h.head();
            b.h.s.start("cablecore").unwrap();
            click(&mut b.h, at.x + 0.2, at.y + 0.2).unwrap();
            assert_eq!(
                prompt(&b.h),
                format!("Core number or colour for W3 <{}>:", before.unwrap_or("none")),
                "{how}, {typed:?}"
            );
            b.h.s.input(Input::Text((*typed).to_string())).unwrap();
            b.h.s.input(Input::Enter).unwrap();
            assert!(b.h.s.running.is_none());
            assert_eq!(b.h.head(), head + 1, "{how}, {typed:?}: one transaction");
            let tx = last_tx(&mut b.h);
            assert_eq!((&tx.ops, &tx.label), (&json_tx.ops, &json_tx.label), "{how}, {typed:?}");
            assert_eq!(b.h.doc(), a.h.doc(), "{how}, {typed:?}");
            b.h.check();
        }
    }
}

/// Design §5.1: typed through the command line exactly like a user: the bare command starts the
/// interactive form; Esc at the text prompt writes nothing.
#[test]
fn ac5_cablecore_typed_at_the_command_line() {
    let mut f = fixture(true, true);
    let head = f.h.head();
    f.h.s.cmdline("cablecore").unwrap();
    assert_eq!(prompt(&f.h), "Select a wire in a cable:");
    f.h.s.cmdline("30,5").unwrap();
    assert_eq!(prompt(&f.h), "Core number or colour for W2 <none>:");
    f.h.s.input(Input::Cancel).unwrap();
    assert!(f.h.s.running.is_none());
    assert_eq!(f.h.head(), head, "Esc writes nothing");
    // A typed point that is not finite or far out of range at the pick prompt: refused, no panic.
    f.h.s.cmdline("cablecore").unwrap();
    for typed in ["1e400,0", "-1e400,5", "nan,nan", "1e300,1e300"] {
        let _ = f.h.s.cmdline(typed);
        assert_eq!(f.h.head(), head, "{typed}");
    }
    let _ = f.h.s.input(Input::Cancel);
    assert!(f.h.s.running.is_none());
    f.h.check();
}

// --------------------------------------------------------------------------- AC4

/// AC4 / design §4.5, check G: both ends of H, every core: the core label and the wire number at
/// the tail start, reading left to right; right side (r = +x) from S + (g, 0.3h), left side
/// (r = −x) right-aligned at S − g.
#[test]
fn ac4_fan_labels_at_the_tail_start_on_both_sides() {
    let mut f = fixture(true, true);
    f.h.run("cablecore", json!({"cable": f.c1.hex(), "cores": ["1", "2", "3", "4"]})).unwrap();
    view(&mut f.h, f.c1, "fan");
    let d = fan(&mut f.h, f.page, f.c1);
    let (g, lift) = (look::LABEL_GAP, 0.3 * look::LABEL_HEIGHT);
    for (i, y) in [0.0, 5.0, 10.0, 15.0].into_iter().enumerate() {
        let (core, wire) = (format!("{}", i + 1), format!("W{}", i + 1));
        let (wc, ww) = (look::font_width(&core), look::font_width(&wire));
        let find = |role: Role, value: &str, left: bool| -> Pt {
            of_role(&d, role)
                .into_iter()
                .map(text_of)
                .find(|(at, rot, v)| v == value && (at.x < 60.0) == left && *rot == 0.0)
                .unwrap_or_else(|| panic!("{value} on the {} side", if left { "left" } else { "right" }))
                .0
        };
        // Left: S = (20, y).
        assert!(near(find(Role::CoreLabel, &core, true), Pt::new(20.0 - g - wc, y + lift)), "left core {core}");
        assert!(
            near(find(Role::WireNumber, &wire, true), Pt::new(20.0 - g - wc - look::LABEL_SPACING - ww, y + lift)),
            "left {wire}"
        );
        // Right: S = (100, y).
        assert!(near(find(Role::CoreLabel, &core, false), Pt::new(100.0 + g, y + lift)), "right core {core}");
        assert!(
            near(find(Role::WireNumber, &wire, false), Pt::new(100.0 + g + wc + look::LABEL_SPACING, y + lift)),
            "right {wire}"
        );
        // Labels sit on their own wire's tail, never on the spine side of the tail start.
        let tail_end_x = |left: bool| if left { 0.0 } else { 120.0 };
        for left in [true, false] {
            let at = find(Role::WireNumber, &wire, left);
            assert!((at.x - tail_end_x(left)).abs() <= 20.0, "{wire} within its tail");
        }
    }
    f.h.check();
}

/// AC4 / design §6: a vertical wire's label reads bottom to top (w = +y), a right-to-left wire's
/// label reads left to right (w turned to +x); both just past the crossing.
#[test]
fn ac4_traditional_labels_on_vertical_and_reversed_wires() {
    let mut h = H::new(SEED + 1);
    let page = h.first_page();
    let mut wires = Vec::new();
    // Two vertical wires drawn top to bottom, two horizontal wires drawn right to left.
    for pts in [[[0.0, 40.0], [0.0, 0.0]], [[5.0, 40.0], [5.0, 0.0]]] {
        wires.push(id::<ConductorId>(&h.run("wire", json!({"points": pts})).unwrap(), "conductor"));
    }
    let v: CableId = id(&h.run("cableslash", json!({"from": [-3, 20], "to": [8, 20]})).unwrap(), "cable");
    h.run("cablecore", json!({"cable": v.hex(), "cores": ["A", "B"]})).unwrap();
    for pts in [[[100.0, 0.0], [50.0, 0.0]], [[100.0, 5.0], [50.0, 5.0]]] {
        wires.push(id::<ConductorId>(&h.run("wire", json!({"points": pts})).unwrap(), "conductor"));
    }
    let r: CableId = id(&h.run("cableslash", json!({"from": [75, -3], "to": [75, 8]})).unwrap(), "cable");
    h.run("cablecore", json!({"cable": r.hex(), "cores": ["C", "D"]})).unwrap();
    let hh = look::LABEL_HEIGHT;
    let texts = layer_texts(&mut h, page);
    let want = [
        ("A", Pt::new(0.0 - 0.3 * hh, 20.0 + 0.5 * hh), std::f64::consts::FRAC_PI_2),
        ("B", Pt::new(5.0 - 0.3 * hh, 20.0 + 0.5 * hh), std::f64::consts::FRAC_PI_2),
        ("C", Pt::new(75.0 + 0.5 * hh, 0.0 + 0.3 * hh), 0.0),
        ("D", Pt::new(75.0 + 0.5 * hh, 5.0 + 0.3 * hh), 0.0),
    ];
    for (value, at, rot) in want {
        assert!(
            texts.iter().any(|(p, r, v)| v == value && p.near(at, 1e-6) && (r - rot).abs() < 1e-9),
            "{value} at {at:?} rotated {rot} in {texts:?}"
        );
    }
    h.check();
}

// --------------------------------------------------------------------------- AC2

/// AC2 / check H, mirror fixture H1′: the component on the right, the wires loose at x = 0, the
/// reference end on the left: the left end stops at the reference (flag pointing left), the right
/// end fans; no loose-end squares at x = 0; the attached ends keep their marks.
#[test]
fn ac2_mirror_one_sided_fan_stops_on_the_left() {
    let mut f = fixture(false, true);
    view(&mut f.h, f.c1, "fan");
    let (point, rep) = reference_end(&mut f.h, f.page, f.c1, Pt::new(-20.0, 21.25), 1);
    let d = fan(&mut f.h, f.page, f.c1);
    assert_eq!(d.markers.get(&rep), Some(&MarkerEnd::A), "the r₀ side (x = 0)");
    let spines: Vec<Vec<Pt>> = of_role(&d, Role::Spine).into_iter().map(|p| path(&p.shape)).collect();
    assert_eq!(spines.len(), 1, "{spines:?}");
    assert!(spines[0].iter().all(|p| (p.x - 97.5).abs() < 1e-9), "right spine at x = 97.5: {spines:?}");
    let tails: Vec<Vec<Pt>> = of_role(&d, Role::Tail).into_iter().map(|p| path(&p.shape)).collect();
    assert_eq!(tails.len(), 4);
    assert!(tails.iter().all(|t| t.iter().all(|p| p.x >= 100.0 - 1e-9)), "{tails:?}");
    assert!(of_role(&d, Role::CoreLabel).is_empty(), "no cores set");
    assert_eq!(of_role(&d, Role::WireNumber).len(), 4, "wire numbers on the right only");
    // The flag's flat side faces the cable (right); the point is on the left.
    let w = look::font_width(look::REFERENCE_PLACEHOLDER);
    let m = Pt::new(-20.0 + w + 0.5 * look::LABEL_HEIGHT, 22.5);
    let flag = path(&of_role(&d, Role::Flag)[0].shape);
    let min_x = flag.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
    assert!(min_x < -20.0 - 0.5 * look::LABEL_HEIGHT - 1e-9, "the point is on the left: {flag:?}");
    // The trunk runs from the junction, now the middle of the spine (97.5, 7.5; owner request of
    // 2026-11-07), to M on the flag's flat (right) side, in axis-aligned steps.
    let trunk: Vec<Pt> = of_role(&d, Role::Trunk).into_iter().flat_map(|p| path(&p.shape)).collect();
    assert!(trunk.iter().any(|p| near(*p, Pt::new(97.5, 7.5))), "{trunk:?}");
    assert!(trunk.iter().any(|p| near(*p, m)), "{m:?} in {trunk:?}");
    // No loose-end squares at x = 0 (the reference end); none expected anywhere here.
    let sq = squares(&mut f.h, f.page);
    assert!(sq.iter().all(|q| q.x.abs() > 1e-6), "{sq:?}");
    // The hover of the reference names the A side.
    let uid = f.h.p(|p| p.pages[&f.page].uid).unwrap();
    let hp = f.h.handle_of(f.page, point);
    let lines = f.h.p(|p| hover_lines(p, uid, hp)).unwrap();
    assert_eq!(lines.last().map(String::as_str), Some("At end: A side"), "{lines:?}");
    // Traditional view: the reference end is drawn (flag) and nothing else changes; the loose ends
    // at x = 0 show their squares again.
    view(&mut f.h, f.c1, "traditional");
    let t = drawing(&mut f.h, f.page, f.c1, false);
    assert!(!t.is_fan() && t.hidden.is_empty());
    assert_eq!(of_role(&t, Role::Flag).len(), 1);
    let sq = squares(&mut f.h, f.page);
    assert_eq!(sq.iter().filter(|q| q.x.abs() < 1e-6).count(), 4, "{sq:?}");
    f.h.check();
}

/// AC2: the H1 marker moved (one `ModifyItem` of the stored point) from the right to the left
/// changes which end stops; a third reference stops no end and says so in its hover; erasing the
/// stopping reference and undoing it toggles the fan of that end. Everything stays consistent.
#[test]
fn ac2_moving_and_extra_references() {
    let mut f = fixture(false, false);
    view(&mut f.h, f.c1, "fan");
    let (item, rep) = reference_end(&mut f.h, f.page, f.c1, Pt::new(125.0, 21.25), 1);
    assert_eq!(fan(&mut f.h, f.page, f.c1).markers.get(&rep), Some(&MarkerEnd::B));
    // Move it to the left of the wires.
    let before = f.h.doc().items[&item].clone();
    let after = Item { kind: ItemKind::Primitive(Primitive::Point { at: Pt::new(-20.0, 21.25) }), ..before.clone() };
    f.h.project.apply_ops(&mut f.h.s, "Move", vec![Op::ModifyItem { id: item, before, after }]).unwrap();
    f.h.check();
    let d = fan(&mut f.h, f.page, f.c1);
    assert_eq!(d.markers.get(&rep), Some(&MarkerEnd::A), "the end is worked out again");
    let tails: Vec<Vec<Pt>> = of_role(&d, Role::Tail).into_iter().map(|p| path(&p.shape)).collect();
    assert!(tails.iter().all(|t| t.iter().all(|p| p.x >= 100.0 - 1e-9)), "the right end fans: {tails:?}");
    // A second at the right and a third: the third is extra.
    let (_, r2) = reference_end(&mut f.h, f.page, f.c1, Pt::new(125.0, 21.25), 2);
    let (p3, r3) = reference_end(&mut f.h, f.page, f.c1, Pt::new(60.0, 60.0), 3);
    let d = fan(&mut f.h, f.page, f.c1);
    assert_eq!(d.markers.get(&r2), Some(&MarkerEnd::B));
    assert_eq!(d.markers.get(&r3), Some(&MarkerEnd::Extra));
    assert!(of_role(&d, Role::Tail).is_empty(), "both ends stop");
    assert_eq!(of_role(&d, Role::Flag).len(), 3, "every reference keeps its flag");
    let uid = f.h.p(|p| p.pages[&f.page].uid).unwrap();
    let h3 = f.h.handle_of(f.page, p3);
    let lines = f.h.p(|p| hover_lines(p, uid, h3)).unwrap();
    assert!(lines.iter().any(|l| l == "Extra reference: not at an end of the cable on this page."), "{lines:?}");
    // Erase the third: nothing changes at the ends. Then erase the right one: the right end fans
    // again; undo: it stops again (design §7.1 "Editing and removing").
    let h3 = f.h.handle_of(f.page, p3);
    f.h.run("erase", json!({"handles": [h3.hex()]})).unwrap();
    let d = fan(&mut f.h, f.page, f.c1);
    assert_eq!((of_role(&d, Role::Spine).len(), of_role(&d, Role::Flag).len()), (0, 2));
    let r2_item = f.h.doc().representations[&r2].items[0];
    let h2 = f.h.handle_of(f.page, r2_item);
    f.h.run("erase", json!({"handles": [h2.hex()]})).unwrap();
    let d = fan(&mut f.h, f.page, f.c1);
    assert_eq!(of_role(&d, Role::Spine).len(), 1);
    f.h.run("u", Value::Null).unwrap();
    let d = fan(&mut f.h, f.page, f.c1);
    assert_eq!(of_role(&d, Role::Spine).len(), 0);
    f.h.check();
}

// --------------------------------------------------------------------------- AC6 hostile

/// AC6: labels stored by another route (bypassing `cablecore`): 1000 characters, control
/// characters, only spaces, empty cable tag and wire tag, a 1000-character designation. Both
/// views draw them shortened or not at all, never panic, and the projection stays consistent.
#[test]
fn ac6_long_and_empty_labels_draw_safely_in_both_views() {
    let mut f = fixture(true, true);
    let long = "L".repeat(1000);
    store_core_raw(&mut f.h, f.c1, 0, Some(&long));
    store_core_raw(&mut f.h, f.c1, 1, Some("a\nb\u{7}"));
    store_core_raw(&mut f.h, f.c1, 2, Some("   "));
    store_core_raw(&mut f.h, f.c1, 3, Some(""));
    f.h.run("setprop", json!({"cable": f.c1.hex(), "key": "tag", "value": ""})).ok();
    f.h.run("setprop", json!({"cable": f.c1.hex(), "key": "designation", "value": "D".repeat(1000)})).unwrap();
    f.h.run("setprop", json!({"conductor": f.wires[1].hex(), "key": "tag", "value": ""})).ok();
    f.h.run("setprop", json!({"conductor": f.wires[2].hex(), "key": "tag", "value": "T".repeat(1000)})).unwrap();
    f.h.check();
    let shown32 = format!("{}…", "L".repeat(31));
    let shown24 = |c: &str| format!("{}…", c.repeat(23));
    for fan_view in [false, true] {
        view(&mut f.h, f.c1, if fan_view { "fan" } else { "traditional" });
        f.h.check();
        let texts: Vec<String> = layer_texts(&mut f.h, f.page).into_iter().map(|t| t.2).collect();
        assert!(texts.contains(&shown32), "1000-char core shortened to 32: {texts:?}");
        assert!(texts.iter().any(|t| t == "a?b?"), "control characters drawn as ?: {texts:?}");
        assert!(texts.iter().all(|t| !t.trim().is_empty()), "blank labels draw nothing: {texts:?}");
        assert!(texts.iter().all(|t| t.chars().count() <= 32), "{texts:?}");
        assert!(texts.iter().all(|t| !t.chars().any(char::is_control)), "{texts:?}");
        if fan_view {
            assert!(texts.contains(&shown24("T")), "wire tag shortened to 24: {texts:?}");
            assert!(texts.contains(&shown24("D")), "designation shortened to 24: {texts:?}");
            let d = fan(&mut f.h, f.page, f.c1);
            assert!(all_finite(&d));
            // The tails are long enough for the longest labels on their side (design §4.2 step 4).
            let wc = look::font_width(&shown32);
            let ww = look::font_width(&shown24("T"));
            // The gap between core label and wire number is one text height (overseer decision on
            // the brief 009 review); the gaps at either end stay `ecad.label-gap`.
            let need = 2.0 * look::LABEL_GAP + look::LABEL_SPACING + wc + ww;
            for t in of_role(&d, Role::Tail) {
                let p = path(&t.shape);
                assert!((p[0].dist(p[1]) - need).abs() < 1e-6, "tail {} long, want {need}", p[0].dist(p[1]));
            }
        }
    }
    // Hover lines with the long values are capped and never contain control characters.
    let uid = f.h.p(|p| p.pages[&f.page].uid).unwrap();
    view(&mut f.h, f.c1, "traditional");
    let slash = f.h.doc().representations.values().find(|r| r.subject == Subject::Cable(f.c1)).unwrap().items[0];
    let hs = f.h.handle_of(f.page, slash);
    let lines = f.h.p(|p| hover_lines(p, uid, hs)).unwrap();
    // Design §8: the core part of each member line is shortened like the drawing; no control
    // characters reach the hover. (Long wire tags in member lines predate brief 009.)
    assert!(lines.iter().any(|l| l.ends_with(&format!("core {shown32}"))), "{lines:?}");
    assert!(lines.iter().all(|l| !l.chars().any(char::is_control)), "{lines:?}");
    f.h.check();
}

/// AC6: an empty cable tag shows `?` in the label box; a cable with no designation or type has a
/// one-line box.
#[test]
fn ac6_unnamed_cable_box_shows_a_question_mark() {
    let mut h = H::new(SEED + 2);
    let page = h.first_page();
    for y in [0.0, 5.0] {
        h.run("wire", json!({"points": [[0.0, y], [60.0, y]]})).unwrap();
    }
    let k: CableId = id(&h.run("cableslash", json!({"from": [30, -3], "to": [30, 8]})).unwrap(), "cable");
    view(&mut h, k, "fan");
    let d = fan(&mut h, page, k);
    let lines: Vec<String> = of_role(&d, Role::BoxText).into_iter().map(|p| text_of(p).2).collect();
    assert_eq!(lines, ["?"]);
    h.check();
}

/// AC6: NaN, infinite and overflowing geometry. Committed edits refuse non-finite values (nothing
/// written); documents holding them (as a hand-edited file could) draw without panic: the fan
/// falls back to traditional, every computed shape is finite, a non-finite reference is ignored.
#[test]
fn ac6_nan_infinite_and_overflowing_geometry_never_panics() {
    let mut f = fixture(true, false);
    f.h.run("cablecore", json!({"cable": f.c1.hex(), "cores": ["1", "2", "3", "4"]})).unwrap();
    view(&mut f.h, f.c1, "fan");
    reference_end(&mut f.h, f.page, f.c1, Pt::new(125.0, 21.25), 1);
    let base = f.h.doc();
    let (route, _) = ecad_model::derive::route_on_page(&base, f.wires[1], f.page).unwrap();
    let bad = [f64::NAN, f64::INFINITY, f64::NEG_INFINITY];
    // Committing a non-finite route or reference point is refused; nothing written.
    for x in bad {
        let head = f.h.head();
        let before = base.items[&route].clone();
        let after = Item {
            kind: ItemKind::Primitive(Primitive::Line { a: Pt::new(0.0, 5.0), b: Pt::new(x, 5.0) }),
            ..before.clone()
        };
        assert!(f.h.project.apply_ops(&mut f.h.s, "Bad", vec![Op::ModifyItem { id: route, before, after }]).is_err());
        assert_eq!(f.h.head(), head);
        let rep_item = base.representations.values().find(|r| r.role == look::REFERENCE_ROLE).unwrap().items[0];
        let before = base.items[&rep_item].clone();
        let after = Item { kind: ItemKind::Primitive(Primitive::Point { at: Pt::new(x, 0.0) }), ..before.clone() };
        let r = f.h.project.apply_ops(&mut f.h.s, "Bad", vec![Op::ModifyItem { id: rep_item, before, after }]);
        assert!(r.is_err(), "a non-finite reference point is refused");
        assert_eq!(f.h.head(), head);
        f.h.check();
    }
    // Documents that hold them anyway, drawn directly, in both views.
    let route_cases: Vec<(&str, Primitive)> = vec![
        ("NaN end", Primitive::Line { a: Pt::new(0.0, 5.0), b: Pt::new(f64::NAN, 5.0) }),
        ("infinite end", Primitive::Line { a: Pt::new(f64::NEG_INFINITY, 5.0), b: Pt::new(120.0, 5.0) }),
        ("NaN start", Primitive::Line { a: Pt::new(f64::NAN, f64::NAN), b: Pt::new(120.0, 5.0) }),
        ("huge finite", Primitive::Line { a: Pt::new(-1e308, 5.0), b: Pt::new(1e308, 5.0) }),
        ("near ±1e12", Primitive::Line { a: Pt::new(-1e12, 5.0), b: Pt::new(1e12, 5.0) }),
        (
            "overflowing polyline",
            Primitive::Polyline {
                vertices: vec![Pt::new(0.0, 5.0), Pt::new(1.7e308, 5.0), Pt::new(-1.7e308, 5.0), Pt::new(120.0, 5.0)],
                closed: false,
            },
        ),
        ("zero length", Primitive::Line { a: Pt::new(0.0, 5.0), b: Pt::new(0.0, 5.0) }),
    ];
    for (name, prim) in route_cases {
        let mut doc = base.clone();
        doc.items.get_mut(&route).unwrap().kind = ItemKind::Primitive(prim);
        for fan_view in [false, true] {
            let d = look::cable_on_page(&doc, f.page, f.c1, fan_view, 1e-6, &look::font_width);
            assert!(all_finite(&d), "{name}, fan {fan_view}: every shape finite");
        }
    }
    let point_cases = [Pt::new(f64::NAN, 0.0), Pt::new(0.0, f64::INFINITY), Pt::new(-1e12, 1e12)];
    let rep_item = base.representations.values().find(|r| r.role == look::REFERENCE_ROLE).unwrap().items[0];
    for at in point_cases {
        let mut doc = base.clone();
        doc.items.get_mut(&rep_item).unwrap().kind = ItemKind::Primitive(Primitive::Point { at });
        for fan_view in [false, true] {
            let d = look::cable_on_page(&doc, f.page, f.c1, fan_view, 1e-6, &look::font_width);
            assert!(all_finite(&d), "reference at {at:?}, fan {fan_view}: every shape finite");
        }
    }
    // Hostile tolerances and measuring functions.
    for tol in [f64::NAN, f64::INFINITY, -1.0, 0.0, 1e300] {
        let d = look::cable_on_page(&base, f.page, f.c1, true, tol, &look::font_width);
        assert!(all_finite(&d), "tol {tol}");
    }
    for m in [f64::NAN, f64::INFINITY, -5.0, 1e308] {
        let d = look::cable_on_page(&base, f.page, f.c1, true, 1e-6, &move |_| m);
        assert!(all_finite(&d), "measure {m}");
    }
    // Through the JSON form: wires at ±1e12 and at 1e300 (finite but overflowing in products),
    // slashed, labelled and fanned.
    for big in [1e12, 1e300] {
        let mut h = H::new(SEED + 3);
        let page = h.first_page();
        let mut ws = Vec::new();
        for y in [0.0, 5.0, 10.0] {
            match h.run("wire", json!({"points": [[-big, y], [big, y]]})) {
                Ok(v) => ws.push(id::<ConductorId>(&v, "conductor")),
                Err(e) => eprintln!("wire at {big} refused: {e}"),
            }
        }
        if ws.len() < 2 {
            continue;
        }
        if let Ok(v) = h.run("cableslash", json!({"from": [0, -3], "to": [0, 13]})) {
            let k: CableId = id(&v, "cable");
            let cores: Vec<String> = (1..=ws.len()).map(|i| i.to_string()).collect();
            h.run("cablecore", json!({"cable": k.hex(), "cores": cores})).unwrap();
            view(&mut h, k, "fan");
            let d = drawing(&mut h, page, k, true);
            assert!(all_finite(&d), "{big}");
            h.run("zoom", json!({"mode": "extents"})).ok();
            h.check();
        }
    }
}

/// AC6 / check M: a 20-core cable at a 1-unit pitch between two components with terminals, cores
/// `1`–`20` in one batch: 40 bends of radius 0.5 (half the pitch), 40 tails, no tail crossing the
/// spine or another tail, labels on their own tails; undo and redo of the batch; save-free replay
/// equals the live state. Then the same cable turned vertical (labels rotated 90°).
#[test]
fn ac6_twenty_core_cable_at_a_tight_pitch() {
    for vertical in [false, true] {
        let mut h = H::new(SEED + 4);
        let page = h.first_page();
        let p = |a: f64, b: f64| if vertical { [b, a] } else { [a, b] };
        let mut ws = Vec::new();
        for i in 0..20 {
            let t = f64::from(i);
            let w: ConductorId = id(&h.run("wire", json!({"points": [p(0.0, t), p(80.0, t)]})).unwrap(), "conductor");
            h.run("setprop", json!({"conductor": w.hex(), "key": "tag", "value": format!("X{i:02}")})).unwrap();
            ws.push(w);
        }
        let k: CableId =
            id(&h.run("cableslash", json!({"from": p(40.0, -2.0), "to": p(40.0, 21.0)})).unwrap(), "cable");
        assert_eq!(h.doc().cables[&k].members.len(), 20);
        let head = h.head();
        let labels: Vec<String> = (1..=20).map(|i| i.to_string()).collect();
        h.run("cablecore", json!({"cable": k.hex(), "cores": labels})).unwrap();
        assert_eq!(h.head(), head + 1, "one transaction for 20 labels");
        assert_eq!(last_tx(&mut h).ops.len(), 40);
        view(&mut h, k, "fan");
        h.check();
        let d = fan(&mut h, page, k);
        assert!(all_finite(&d));
        let bends: Vec<f64> = of_role(&d, Role::Bend)
            .into_iter()
            .map(|b| match b.shape {
                Shape::Arc { radius, .. } => radius,
                _ => f64::NAN,
            })
            .collect();
        assert_eq!(bends.len(), 40);
        assert!(bends.iter().all(|r| (r - 0.5).abs() < 1e-9), "radius = pitch / 2: {bends:?}");
        let tails: Vec<Vec<Pt>> = of_role(&d, Role::Tail).into_iter().map(|t| path(&t.shape)).collect();
        assert_eq!(tails.len(), 40);
        // Each tail lies on its own wire's line (no two tails share a line on the same side).
        let across = |q: Pt| if vertical { q.x } else { q.y };
        let along = |q: Pt| if vertical { q.y } else { q.x };
        let mut lines: Vec<(bool, i64)> =
            tails.iter().map(|t| (along(t[0]) < 40.0, (across(t[0]) * 1000.0).round() as i64)).collect();
        lines.sort();
        lines.dedup();
        assert_eq!(lines.len(), 40, "tails on distinct lines");
        // Spines lie beyond the tail starts (towards the middle) and do not touch any tail.
        for s in of_role(&d, Role::Spine).into_iter().map(|s| path(&s.shape)) {
            let a = along(s[0]);
            for t in &tails {
                let (lo, hi) = (along(t[0]).min(along(t[1])), along(t[0]).max(along(t[1])));
                assert!(a < lo - 1e-9 || a > hi + 1e-9, "spine at {a} touches tail {t:?}");
            }
        }
        // Core labels: 40, rotated 90° for the vertical cable, horizontal otherwise.
        let core_labels = of_role(&d, Role::CoreLabel);
        assert_eq!(core_labels.len(), 40);
        let want_rot = if vertical { std::f64::consts::FRAC_PI_2 } else { 0.0 };
        assert!(core_labels.iter().all(|l| (text_of(l).1 - want_rot).abs() < 1e-9));
        assert_eq!(of_role(&d, Role::WireNumber).len(), 40);
        // Undo the batch: no labels; redo: all back.
        h.run("u", Value::Null).unwrap();
        assert!(of_role(&fan(&mut h, page, k), Role::CoreLabel).is_empty());
        h.run("redo", Value::Null).unwrap();
        assert_eq!(of_role(&fan(&mut h, page, k), Role::CoreLabel).len(), 40);
        assert!(h.p(|p| p.ledger.replay_matches().unwrap()));
        h.check();
    }
}

// --------------------------------------------------------------------------- AC7 (005 AC1/2/4)

/// AC7 = brief 005 criteria 1, 2 and 4 with brief 009's state present (cores, a reference end):
/// ten switches, by JSON and interactively, leave head, log, IDs and nets unchanged and the
/// project saved; undo after the switches takes back the last real edit (a core label), never a
/// switch; redo restores it, still in the view last chosen.
#[test]
fn ac7_switching_stays_presentation_only_with_cores_and_references() {
    let work = Tmp::new("ac7");
    ecad_cadcraft::install();
    views::isolate_for_tests();
    let mut s = Session::empty();
    let handle = Project::create_with(&mut s, "Views", SEED, Persistence::File).unwrap();
    let mut f = fixture_on(H { s, project: handle, trail: Vec::new() }, true, false);
    f.h.run("cablecore", json!({"cable": f.c1.hex(), "cores": ["1", "2", "3", "4"]})).unwrap();
    reference_end(&mut f.h, f.page, f.c1, Pt::new(125.0, 21.25), 1);
    f.h.run("cablecore", json!({"conductor": f.wires[0].hex(), "core": "PE"})).unwrap();
    f.h.run("saveas", json!({"path": work.path("v.ecad")})).unwrap();
    let doc = f.h.doc();
    let log = f.h.p(|p| p.ledger.log().to_vec());
    let head = f.h.head();
    let nets = ecad_model::derive::nets(&doc);
    for i in 0..10 {
        if i % 2 == 0 {
            f.h.run("cableview", json!({"cable": f.c1.hex()})).unwrap();
        } else {
            let was = f.h.run("cableview", json!({})).unwrap()["views"][f.c1.hex()].clone();
            f.h.s.start("cableview").unwrap();
            // The slash in traditional view, a tail in fan view.
            if was == "fan" {
                click(&mut f.h, 10.0, 5.0).unwrap();
            } else {
                click(&mut f.h, 60.0, 7.5).unwrap();
            }
            f.h.s.input(Input::Enter).unwrap();
            assert!(f.h.s.running.is_none());
            let now = f.h.run("cableview", json!({})).unwrap()["views"][f.c1.hex()].clone();
            assert_ne!(now, was, "switch {i} by clicking");
        }
        f.h.check();
        assert!(!f.h.p(|p| p.unsaved()), "switch {i} is not an unsaved change");
    }
    assert_eq!(f.h.doc(), doc);
    assert_eq!(f.h.p(|p| p.ledger.log().to_vec()), log);
    assert_eq!(f.h.head(), head);
    assert_eq!(ecad_model::derive::nets(&f.h.doc()), nets);
    view(&mut f.h, f.c1, "fan");
    f.h.run("u", Value::Null).unwrap();
    assert_eq!(cores(&mut f.h, f.c1)[0].as_deref(), Some("1"), "undo took back the PE label, not a switch");
    let v = f.h.run("cableview", json!({})).unwrap()["views"][f.c1.hex()].clone();
    assert_eq!(v, "fan", "the view is untouched by undo");
    f.h.run("redo", Value::Null).unwrap();
    assert_eq!(cores(&mut f.h, f.c1)[0].as_deref(), Some("PE"));
    f.h.check();
}

/// AC6: a reference point that is finite but huge (1e308, as a hand-edited file or a future
/// command without a range check could store; the model accepts it) must not put non-finite
/// geometry into the drawing: every computed entity on the reserved layer stays finite, in both
/// views, and the page stays consistent.
#[test]
fn ac6_huge_finite_reference_point_draws_only_finite_geometry() {
    for at in [Pt::new(1e308, 1e308), Pt::new(-1.7e308, 21.25), Pt::new(125.0, 1.7e308)] {
        let mut f = fixture(true, false);
        reference_end(&mut f.h, f.page, f.c1, at, 1);
        for v in ["traditional", "fan"] {
            view(&mut f.h, f.c1, v);
            f.h.check();
            let d = drawing(&mut f.h, f.page, f.c1, v == "fan");
            let bad: Vec<_> = d
                .parts
                .iter()
                .filter(|p| !all_finite(&look::CableDrawing { parts: vec![(*p).clone()], ..Default::default() }))
                .collect();
            assert!(bad.is_empty(), "reference at {at:?}, {v}: non-finite parts {bad:?}");
        }
    }
}

/// Design §5.1 step 3b with §5.4 (only spaces clear, like `null`): typing only spaces at the
/// prompt for a wire with no label writes nothing and says `W3 has no core label.`, as `.` does;
/// for a wire with a label it records the same clear transaction as `.`.
#[test]
fn ac6_interactive_spaces_clear_like_a_dot() {
    let mut f = fixture(true, true);
    let head = f.h.head();
    f.h.s.start("cablecore").unwrap();
    click(&mut f.h, 30.0, 10.0).unwrap();
    f.h.s.input(Input::Text("   ".into())).unwrap();
    assert_eq!(f.h.head(), head, "nothing written");
    let echo = f.h.s.log.last().cloned().unwrap_or_default();
    f.h.s.input(Input::Cancel).unwrap();
    // With a label: spaces and `.` give the same transaction.
    let mut txs = Vec::new();
    for typed in ["   ", "."] {
        let mut g = fixture(true, true);
        g.h.run("cablecore", json!({"conductor": g.wires[2].hex(), "core": "3"})).unwrap();
        g.h.s.start("cablecore").unwrap();
        click(&mut g.h, 30.0, 10.0).unwrap();
        g.h.s.input(Input::Text(typed.into())).unwrap();
        g.h.s.input(Input::Cancel).unwrap();
        assert_eq!(cores(&mut g.h, g.c1)[2], None, "{typed:?} clears");
        txs.push(last_tx(&mut g.h).ops);
        g.h.check();
    }
    assert_eq!(txs[0], txs[1]);
    assert_eq!(echo, "W3 has no core label.", "the echo for clearing an unset label");
}

// --------------------------------------------------------------------------- round 2: cablefan

/// The junction of the fan's trunk with the spine at x = `x` (22.5 left, 97.5 right in H).
fn junction(d: &look::CableDrawing, x: f64) -> Pt {
    of_role(d, Role::Trunk)
        .into_iter()
        .flat_map(|p| path(&p.shape))
        .find(|p| (p.x - x).abs() < 1e-9)
        .unwrap_or_else(|| panic!("no junction at x = {x}"))
}

/// `cablefan` JSON contract (owner request on brief 009): one row per input; `Ok(y)` rows write one
/// transaction and put the left (A) join at `y`; `Err(words)` rows write nothing.
#[test]
fn cablefan_json_contract_table() {
    struct Row {
        name: &'static str,
        params: fn(&str) -> Value,
        want: Result<f64, &'static str>,
    }
    let rows = [
        Row { name: "point on the spine", params: |k| json!({"cable": k, "at": [22.5, 11]}), want: Ok(11.0) },
        Row { name: "point off the spine snaps", params: |k| json!({"cable": k, "at": [5, 2]}), want: Ok(2.0) },
        Row {
            name: "point above clamps",
            params: |k| json!({"cable": k, "end": "A", "at": [0, 1e12]}),
            want: Ok(15.0),
        },
        Row {
            name: "point below clamps",
            params: |k| json!({"cable": k, "end": "A", "at": [0, -1e12]}),
            want: Ok(0.0),
        },
        Row { name: "fraction 0", params: |k| json!({"cable": k, "end": "A", "at": 0}), want: Ok(0.0) },
        Row { name: "fraction 1", params: |k| json!({"cable": k, "end": "A", "at": 1}), want: Ok(15.0) },
        Row { name: "fraction 0.2", params: |k| json!({"cable": k, "end": "A", "at": 0.2}), want: Ok(3.0) },
        Row {
            name: "fraction just over 1",
            params: |k| json!({"cable": k, "end": "A", "at": 1.000001}),
            want: Err("`at` as a fraction must be from 0 to 1"),
        },
        Row {
            name: "negative fraction",
            params: |k| json!({"cable": k, "end": "A", "at": -0.1}),
            want: Err("`at` as a fraction must be from 0 to 1"),
        },
        Row {
            name: "huge fraction",
            params: |k| json!({"cable": k, "end": "A", "at": 1e308}),
            want: Err("`at` as a fraction must be from 0 to 1"),
        },
        Row {
            name: "point beyond 1e12",
            params: |k| json!({"cable": k, "end": "A", "at": [0, 1.0000001e12]}),
            want: Err("Point out of range."),
        },
        Row {
            name: "point 1e308",
            params: |k| json!({"cable": k, "at": [1e308, 1e308]}),
            want: Err("Point out of range."),
        },
        Row { name: "at missing", params: |k| json!({"cable": k, "end": "A"}), want: Err("`at` must be a point") },
        Row {
            name: "at a bool",
            params: |k| json!({"cable": k, "end": "A", "at": true}),
            want: Err("`at` must be a point"),
        },
        Row {
            name: "at a string",
            params: |k| json!({"cable": k, "end": "A", "at": "0.5"}),
            want: Err("`at` must be a point"),
        },
        Row {
            name: "at an object",
            params: |k| json!({"cable": k, "end": "A", "at": {"x": 0, "y": 1}}),
            want: Err("`at` must be a point"),
        },
        Row {
            name: "at one number",
            params: |k| json!({"cable": k, "end": "A", "at": [1]}),
            want: Err("`at` must be a point"),
        },
        Row {
            name: "at strings",
            params: |k| json!({"cable": k, "end": "A", "at": ["1", "2"]}),
            want: Err("`at` must be a point"),
        },
        Row {
            name: "end lower case",
            params: |k| json!({"cable": k, "end": "a", "at": 0.5}),
            want: Err("`end` must be"),
        },
        Row { name: "end a number", params: |k| json!({"cable": k, "end": 0, "at": 0.5}), want: Err("`end` must be") },
        Row {
            name: "cable missing",
            params: |_| json!({"end": "A", "at": 0.5}),
            want: Err("`cable` (hex id) is required"),
        },
        Row { name: "null params", params: |_| Value::Null, want: Err("`cable` (hex id) is required") },
    ];
    let mut failures = Vec::new();
    for row in &rows {
        let mut f = fixture(true, true);
        view(&mut f.h, f.c1, "fan");
        let (head, doc) = (f.h.head(), f.h.doc());
        let params = (row.params)(&f.c1.hex());
        let r = f.h.run("cablefan", params.clone());
        match (&row.want, r) {
            (Ok(y), Ok(v)) => {
                let j = junction(&fan(&mut f.h, f.page, f.c1), 22.5);
                if f.h.head() != head + 1 || (j.y - y).abs() > 1e-9 || v["end"] != "A" {
                    failures.push(format!("{}: {params} gave {v}, junction {j:?}", row.name));
                }
            }
            (Err(words), Err(e)) => {
                if !e.contains(words) || f.h.head() != head || f.h.doc() != doc {
                    failures
                        .push(format!("{}: {params} refused with {e:?}, want {words:?}, nothing written", row.name));
                }
            }
            (want, got) => failures.push(format!("{}: {params}: want {want:?}, got {got:?}", row.name)),
        }
        f.h.check();
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Save and reopen keep the moved joins (a real `.ecad` file); undo after the reopen takes the last
/// join back; the reopened log equals the saved one and replay matches.
#[test]
fn cablefan_join_survives_save_and_reopen() {
    let work = Tmp::new("fan-reopen");
    let settings = Tmp::new("fan-reopen-settings");
    ecad_cadcraft::install();
    views::isolate_for_tests();
    let mut s = Session::empty();
    let handle = Project::create_with(&mut s, "Join", SEED, Persistence::File).unwrap();
    let mut f = fixture_on(H { s, project: handle, trail: Vec::new() }, true, true);
    view(&mut f.h, f.c1, "fan");
    f.h.run("cablefan", json!({"cable": f.c1.hex(), "end": "A", "at": [22.5, 15]})).unwrap();
    f.h.run("cablefan", json!({"cable": f.c1.hex(), "end": "B", "at": 0.0})).unwrap();
    let saved = fan(&mut f.h, f.page, f.c1);
    let (left, right) = (junction(&saved, 22.5), junction(&saved, 97.5));
    assert!(near(left, Pt::new(22.5, 15.0)), "{left:?}");
    // Note: fraction 0 on end B is the top core (y = 15), on end A the bottom one (see the report).
    let path = work.path("join.ecad");
    f.h.run("saveas", json!({"path": path})).unwrap();
    let (doc, log) = (f.h.doc(), f.h.p(|p| p.ledger.log().to_vec()));
    while !f.h.s.docs.is_empty() {
        f.h.s.execute("close", &json!({"index": 0})).unwrap();
    }
    let mut s = Session::empty();
    let (handle, _) = ProjectHandle::open_file_in(&mut s, &path, 3, Settings::at(&settings.0)).unwrap();
    let mut h = H { s, project: handle, trail: Vec::new() };
    assert_eq!(h.doc(), doc);
    assert_eq!(h.p(|p| p.ledger.log().to_vec()), log);
    assert!(h.p(|p| p.ledger.replay_matches().unwrap()));
    h.check();
    // The view is per user (a fresh settings folder: traditional); switch to fan to see the joins.
    view(&mut h, f.c1, "fan");
    let d = fan(&mut h, f.page, f.c1);
    assert!(near(junction(&d, 22.5), left), "left join kept");
    assert!(near(junction(&d, 97.5), right), "right join kept: {:?}", junction(&d, 97.5));
    h.run("u", Value::Null).unwrap();
    let d = fan(&mut h, f.page, f.c1);
    assert!(near(junction(&d, 97.5), Pt::new(97.5, 7.5)), "undo after reopen: right join back to the middle");
    assert!(near(junction(&d, 22.5), left));
    h.check();
}

/// The stored join is clamped between the outermost wires when wires move or are removed, and
/// the cable's other lifecycle edits (delete the cable, undo, erase the slash) stay consistent.
#[test]
fn cablefan_join_follows_wires_and_cable_lifecycle() {
    let mut f = fixture(false, false);
    view(&mut f.h, f.c1, "fan");
    f.h.run("cablefan", json!({"cable": f.c1.hex(), "end": "A", "at": [22.5, 15]})).unwrap();
    assert!(near(junction(&fan(&mut f.h, f.page, f.c1), 22.5), Pt::new(22.5, 15.0)));
    // Delete the top wire (W4, y = 15): the join is clamped to the new top core (y = 10).
    f.h.run("ecaddelete", json!({"conductor": f.wires[3].hex()})).unwrap();
    f.h.check();
    let d = fan(&mut f.h, f.page, f.c1);
    let j = junction(&d, 22.5);
    assert!(j.y <= 10.0 + 1e-9 && j.y >= -1e-9, "clamped between the outermost wires: {j:?}");
    assert!(all_finite(&d));
    // Move the remaining wires down by 50 (loose wires, in traditional view): still clamped.
    view(&mut f.h, f.c1, "traditional");
    let routes: Vec<String> = f.wires[..3].iter().map(|w| f.h.route_handle(f.page, *w).hex()).collect();
    f.h.run("move", json!({"handles": routes, "delta": [0, -50]})).unwrap();
    f.h.check();
    view(&mut f.h, f.c1, "fan");
    let d = fan(&mut f.h, f.page, f.c1);
    let spine: Vec<Pt> = of_role(&d, Role::Spine).into_iter().flat_map(|p| path(&p.shape)).collect();
    let (lo, hi) = spine.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), p| (a.min(p.y), b.max(p.y)));
    assert!(hi < -30.0, "the spines moved with the wires: {lo}..{hi}");
    let j = junction(&d, 22.5);
    assert!((-50.0 - 1e-9..=-40.0 + 1e-9).contains(&j.y), "join within the moved wires: {j:?}");
    // Undo both, redo both.
    f.h.run("u", Value::Null).unwrap();
    f.h.run("u", Value::Null).unwrap();
    assert!(near(junction(&fan(&mut f.h, f.page, f.c1), 22.5), Pt::new(22.5, 15.0)));
    f.h.run("redo", Value::Null).unwrap();
    f.h.run("redo", Value::Null).unwrap();
    f.h.check();
    // Delete the whole cable with a stored join: the document stays valid; undo restores both.
    let anchors = |h: &mut H| h.doc().representations.values().filter(|r| r.role == look::ANCHOR_ROLE).count();
    assert_eq!(anchors(&mut f.h), 1);
    f.h.run("ecaddelete", json!({"cable": f.c1.hex()})).unwrap();
    f.h.check();
    assert!(!f.h.doc().cables.contains_key(&f.c1));
    assert_eq!(anchors(&mut f.h), 0, "the join goes with the cable");
    f.h.run("u", Value::Null).unwrap();
    f.h.check();
    assert_eq!(anchors(&mut f.h), 1);
    // ERASE of the slash in traditional view with a stored join present: never an invalid document.
    view(&mut f.h, f.c1, "traditional");
    let slash = ecad_cadcraft::slash::slashes_of(&f.h.doc(), f.page, f.c1)[0].0;
    let hs = f.h.handle_of(f.page, slash);
    let r = f.h.run("erase", json!({"handles": [hs.hex()]}));
    f.h.check();
    eprintln!("erase slash with a stored join: {r:?}");
}

/// A hand-edited stored join far off the spine (finite or not) never puts non-finite geometry in
/// the drawing and keeps the join on the spine.
#[test]
fn cablefan_hostile_stored_join_draws_finite() {
    let mut f = fixture(true, true);
    view(&mut f.h, f.c1, "fan");
    f.h.run("cablefan", json!({"cable": f.c1.hex(), "end": "A", "at": 0.5})).unwrap();
    let base = f.h.doc();
    let item = base.representations.values().find(|r| r.role == look::ANCHOR_ROLE).unwrap().items[0];
    for at in [Pt::new(1e308, 1e308), Pt::new(-1e308, 1e308), Pt::new(f64::NAN, 0.0), Pt::new(0.0, f64::INFINITY)] {
        let mut doc = base.clone();
        doc.items.get_mut(&item).unwrap().kind = ItemKind::Primitive(Primitive::Point { at });
        let d = look::cable_on_page(&doc, f.page, f.c1, true, 1e-6, &look::font_width);
        assert!(d.is_fan(), "{at:?}");
        assert!(all_finite(&d), "{at:?}");
        let j = junction(&d, 22.5);
        assert!((-1e-9..=15.0 + 1e-9).contains(&j.y), "{at:?}: join on the spine {j:?}");
    }
    // Committed through the model, a finite far point is accepted and still drawn on the spine.
    let before = base.items[&item].clone();
    let after = Item { kind: ItemKind::Primitive(Primitive::Point { at: Pt::new(1e308, -1e308) }), ..before.clone() };
    f.h.project.apply_ops(&mut f.h.s, "Bad", vec![Op::ModifyItem { id: item, before, after }]).unwrap();
    f.h.check();
    let d = fan(&mut f.h, f.page, f.c1);
    assert!(all_finite(&d));
}

/// The interactive form: `Middle` resets like `at: null`; points out of range and typed text are
/// refused without writing; Esc writes nothing.
#[test]
fn cablefan_interactive_middle_and_refusals() {
    let mut a = fixture(true, true);
    view(&mut a.h, a.c1, "fan");
    a.h.run("cablefan", json!({"cable": a.c1.hex(), "end": "A", "at": 1.0})).unwrap();
    let mut b = fixture(true, true);
    view(&mut b.h, b.c1, "fan");
    b.h.run("cablefan", json!({"cable": b.c1.hex(), "end": "A", "at": 1.0})).unwrap();
    a.h.run("cablefan", json!({"cable": a.c1.hex(), "end": "A", "at": null})).unwrap();
    let json_ops = last_tx(&mut a.h).ops;
    let head = b.h.head();
    b.h.s.start("cablefan").unwrap();
    click(&mut b.h, 22.5, 4.0).unwrap();
    for typed in ["1e400,0", "nan,nan", "hello"] {
        let _ = b.h.s.cmdline(typed);
        assert_eq!(b.h.head(), head, "{typed}");
    }
    let _ = b.h.s.input(Input::Point(Vec2::new(1e300, 0.0)));
    assert_eq!(b.h.head(), head);
    b.h.s.input(Input::Keyword("Middle".into())).unwrap();
    assert_eq!(b.h.head(), head + 1);
    assert_eq!(last_tx(&mut b.h).ops, json_ops, "Middle equals at: null");
    assert_eq!(b.h.doc(), a.h.doc());
    // Esc after picking: nothing written.
    b.h.s.start("cablefan").unwrap();
    click(&mut b.h, 22.5, 4.0).unwrap();
    b.h.s.input(Input::Cancel).unwrap();
    assert_eq!(b.h.head(), head + 1);
    b.h.check();
}

/// Review S2 through a crossing-window selection made the way a user makes it (every entity in a
/// window round a labelled cable of attached wires, components included): MOVE, COPY and ERASE
/// each record one transaction and leave the page consistent.
#[test]
fn s2_window_round_a_labelled_cable_with_components() {
    for (cmd, extra) in [("move", json!({"delta": [0, 60]})), ("copy", json!({"delta": [0, 60]})), ("erase", json!({}))]
    {
        let mut f = fixture(true, true);
        f.h.run("cablecore", json!({"cable": f.c1.hex(), "cores": ["1", "2", "3", "4"]})).unwrap();
        let all: Vec<String> = f.h.p(|p| p.pages[&f.page].map.handles().map(|h| h.hex()).collect());
        let head = f.h.head();
        let mut params = extra.clone();
        params["handles"] = json!(all);
        let r = f.h.run(cmd, params);
        assert!(r.is_ok(), "{cmd}: {r:?}");
        assert_eq!(f.h.head(), head + 1, "{cmd}: one transaction");
        f.h.check();
        if cmd == "move" {
            let texts = layer_texts(&mut f.h, f.page);
            assert_eq!(texts.len(), 4, "{cmd}: labels drawn again");
            assert!(texts.iter().all(|t| t.0.y > 55.0), "{cmd}: labels follow the moved slash: {texts:?}");
        }
    }
}

/// Round 2 (round-1 finding 3): a blank stored core (empty or spaces, by another route) is shown
/// in the hover as no core at all: `  W4`, and `In cable C1` on the wire.
#[test]
fn hover_shows_blank_cores_as_unset() {
    let mut f = fixture(true, true);
    store_core_raw(&mut f.h, f.c1, 3, Some(""));
    store_core_raw(&mut f.h, f.c1, 2, Some("   "));
    let uid = f.h.p(|p| p.pages[&f.page].uid).unwrap();
    let slash = ecad_cadcraft::slash::slashes_of(&f.h.doc(), f.page, f.c1)[0].0;
    let hs = f.h.handle_of(f.page, slash);
    let lines = f.h.p(|p| hover_lines(p, uid, hs)).unwrap();
    assert!(lines.iter().any(|l| l == "  W4"), "{lines:?}");
    assert!(lines.iter().any(|l| l == "  W3"), "{lines:?}");
    assert!(lines.iter().all(|l| !l.ends_with("core ") && !l.contains("core    ")), "{lines:?}");
    let route = f.h.route_handle(f.page, f.wires[3]);
    let lines = f.h.p(|p| hover_lines(p, uid, route)).unwrap();
    assert!(lines.iter().any(|l| l == "In cable C1"), "{lines:?}");
}
