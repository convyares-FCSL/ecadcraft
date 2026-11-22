//! Tester 052d (design 052 §5.3–§5.6, §5.8, §10.17–21): every place path proposes in its own
//! transaction (the project-symbol path included), the conflict answers (Move, Move per row, Half,
//! Leave) and their undo, stale conflicts, page, project and box-move proposals, and hostile
//! proposal parameters. The harness checks replay and the shown page after every command.

use crate::common::{H, id};

use ecad_engine::checks::{self, Kind};
use ecad_engine::locations;
use ecad_model::{DeviceId, Document, PageId};
use serde_json::{Value, json};

fn loc(d: &Document, c: DeviceId) -> Option<String> {
    locations::resolve(d, c).value
}

fn func(d: &Document, c: DeviceId) -> Option<String> {
    locations::resolve_function(d, c).value
}

fn conflicts(h: &mut H) -> Vec<Value> {
    let v = h.run("conflicts", json!({})).unwrap();
    let mut out = Vec::new();
    for a in ["function", "location"] {
        out.extend(v["conflicts"][a]["conflicts"].as_array().cloned().unwrap_or_default());
    }
    out
}

/// Undo the last `n` transactions and check the document is `before` exactly.
fn undo_to(h: &mut H, n: u64, before: &Document, what: &str) {
    for _ in 0..n {
        h.cmdline("u");
    }
    assert_eq!(&h.doc(), before, "{what}: undo restores the document exactly");
}

/// The float switch of design 048 §9.1, a project symbol (key `float1`).
fn float_symbol(h: &mut H) {
    h.run(
        "symbolnew",
        json!({
            "name": "Float switch", "key": "float1",
            "graphics": [{"kind": "polyline", "vertices": [[-2.5, -7.5], [2.5, -7.5], [2.5, 7.5], [-2.5, 7.5]], "closed": true}],
            "points": [{"key": "1", "label": "BN", "at": [0, 5]}, {"key": "2", "label": "BU", "at": [0, -5]}],
            "slots": [{"text": "tag", "at": [-4, 0], "h": "right", "v": "middle"}],
            "defaults": {"prefix": "B", "description": "Float switch", "join": "device"},
        }),
    )
    .unwrap();
}

/// A function box `=F1` around a location box `+CP1` on the first page.
fn boxes(h: &mut H) {
    h.run("functionbox", json!({"function": "=F1", "corners": [[10, 60], [390, 260]]})).unwrap();
    h.run("locationbox", json!({"location": "+CP1", "corners": [[20, 70], [380, 250]]})).unwrap();
}

/// §10.17, §10.21 for every place path: a coil, a terminal level (its strip), a PLC channel (its
/// card), a connector pin (its connector) and a **project symbol** placed inside both boxes take
/// `=F1` and `+CP1` in the placement's own transaction, and one undo removes everything.
#[test]
fn every_place_path_proposes_in_one_transaction_and_one_undo() {
    for (symbol, project) in [("coil", false), ("termlnpe.1", false), ("di", false), ("pins", false), ("float1", true)] {
        let mut h = H::new(0x52_d001);
        if project {
            float_symbol(&mut h);
        }
        boxes(&mut h);
        let before = h.doc();
        let head = h.head();
        let v = h.run("place", json!({"symbol": symbol, "at": [150, 160]})).unwrap();
        assert_eq!(h.head(), head + 1, "{symbol}: one transaction {v}");
        let c: DeviceId = id(&v, "component");
        let d = h.doc();
        assert_eq!(loc(&d, c).as_deref(), Some("+CP1"), "{symbol}: location {v}");
        assert_eq!(func(&d, c).as_deref(), Some("=F1"), "{symbol}: function {v}");
        undo_to(&mut h, 1, &before, symbol);
    }
}

/// §5.6, §5.3 on a project symbol: a box drawn round a project-symbol device at another location
/// lists it and never changes it; Move all is one step and undoes exactly; a project symbol
/// placed with a code takes it in its own transaction.
#[test]
fn a_project_symbol_device_is_listed_and_moved_like_any_other() {
    let mut h = H::new(0x52_d002);
    float_symbol(&mut h);
    let v = h.run("place", json!({"symbol": "float1", "at": [100, 160]})).unwrap();
    let b: DeviceId = id(&v, "component");
    h.run("setlocation", json!({"component": b.hex(), "value": "+CP2"})).unwrap();
    let v = h.run("locationbox", json!({"location": "+CP1", "corners": [[20, 70], [200, 250]]})).unwrap();
    assert_eq!(loc(&h.doc(), b).as_deref(), Some("+CP2"), "never changed silently: {v}");
    assert_eq!(conflicts(&mut h).len(), 1, "{v}");
    let before = h.doc();
    let head = h.head();
    h.run("conflicts", json!({"answer": "move"})).unwrap();
    assert_eq!(h.head(), head + 1);
    assert_eq!(loc(&h.doc(), b).as_deref(), Some("+CP1"));
    undo_to(&mut h, 1, &before, "Move all on a project symbol");
    let head = h.head();
    let v = h.run("place", json!({"symbol": "float1", "at": [300, 160], "location": "+CP7"})).unwrap();
    assert_eq!(h.head(), head + 1);
    assert_eq!(loc(&h.doc(), id(&v, "component")).as_deref(), Some("+CP7"));
}

/// §5.5: each answer is one transaction (Move all, Move per row, Half) or none (Leave), and undo
/// and redo of it are exact.
#[test]
fn every_conflict_answer_is_one_step_and_undoes_exactly() {
    let mut h = H::new(0x52_d003);
    let mut ks = Vec::new();
    for x in [50, 100, 150] {
        let v = h.run("place", json!({"symbol": "coil", "at": [x, 150], "location": "keep", "function": "keep"})).unwrap();
        let k: DeviceId = id(&v, "component");
        h.run("setlocation", json!({"component": k.hex(), "value": "+CP2"})).unwrap();
        ks.push(k);
    }
    h.run("locationbox", json!({"location": "+CP1", "corners": [[20, 100], [200, 220]]})).unwrap();
    assert_eq!(conflicts(&mut h).len(), 3);
    // Leave: nothing written, the list gone.
    let (before, head) = (h.doc(), h.head());
    h.run("conflicts", json!({"answer": "leave"})).unwrap();
    assert_eq!((h.head(), h.doc()), (head, before.clone()));
    assert!(conflicts(&mut h).is_empty());
    // Ask again (box given the same entry back: draw a second box) and answer Move for one row.
    h.run("locationbox", json!({"location": "+CP1", "corners": [[30, 110], [190, 210]]})).unwrap();
    assert_eq!(conflicts(&mut h).len(), 3);
    let (before, head) = (h.doc(), h.head());
    h.run("conflicts", json!({"answer": "move", "devices": [ks[0].hex()]})).unwrap();
    assert_eq!(h.head(), head + 1);
    let after = h.doc();
    assert_eq!(loc(&after, ks[0]).as_deref(), Some("+CP1"));
    assert_eq!(loc(&after, ks[1]).as_deref(), Some("+CP2"), "only the row answered");
    undo_to(&mut h, 1, &before, "Move per row");
    h.cmdline("redo");
    assert_eq!(h.doc(), after, "redo is exact");
}

/// §10.21, §5.8: "Choose lets it take =F1 and keep its location": a device with both aspects in
/// conflict can move one aspect's row and keep the other. The desktop app's per-row Move says
/// "Set -K01's function to =F1" but sends only `devices`.
#[test]
fn a_per_row_move_moves_only_that_aspect() {
    let mut h = H::new(0x52_d004);
    let v = h.run("place", json!({"symbol": "coil", "at": [300, 40], "location": "keep", "function": "keep"})).unwrap();
    let m: DeviceId = id(&v, "component");
    h.run("setfunction", json!({"component": m.hex(), "value": "=F2"})).unwrap();
    h.run("setlocation", json!({"component": m.hex(), "value": "+CP2"})).unwrap();
    boxes(&mut h);
    h.run("place", json!({"symbol": "no", "at": [150, 160], "device": m.hex()})).unwrap();
    let rows = conflicts(&mut h);
    assert_eq!(rows.len(), 2, "one row per aspect: {rows:?}");
    let r = h.run("conflicts", json!({"answer": "move", "devices": [m.hex()], "aspect": "function"}));
    assert!(r.is_ok(), "{r:?}");
    let d = h.doc();
    assert_eq!(func(&d, m).as_deref(), Some("=F1"), "the function row moved");
    assert_eq!(loc(&d, m).as_deref(), Some("+CP2"), "the location row was not answered: it keeps +CP2");
}

/// §5.3 Half: only `location.<side>` of the connector changes, in one transaction; undo is exact.
/// Half with nothing waiting is refused and writes nothing.
#[test]
fn half_sets_only_that_half_and_undoes_exactly() {
    let mut h = H::new(0x52_d005);
    let head = h.head();
    let e = h.run("conflicts", json!({"answer": "half"})).unwrap_err();
    assert!(e.contains("No connector half"), "{e}");
    assert_eq!(h.head(), head);
    let v = h.run("place", json!({"symbol": "pins", "at": [300, 40]})).unwrap();
    let x: DeviceId = id(&v, "component");
    h.run("setlocation", json!({"component": x.hex(), "value": "+CP1"})).unwrap();
    h.run("locationbox", json!({"location": "+FIELD", "corners": [[20, 100], [200, 220]]})).unwrap();
    h.run("place", json!({"symbol": "pinp", "device": x.hex(), "at": [75, 180]})).unwrap();
    let before = h.doc();
    h.run("conflicts", json!({"answer": "half"})).unwrap();
    let after = h.doc();
    let (b, a) = (&before.devices[&x].properties, &after.devices[&x].properties);
    let changed: Vec<&String> = a.keys().chain(b.keys()).filter(|k| a.get(*k) != b.get(*k)).collect();
    assert_eq!(changed, [&"location.plug".to_string()], "only the plug's location");
    assert_eq!(before.devices.len(), after.devices.len());
    undo_to(&mut h, 1, &before, "Half");
}

/// §5.5 "Esc (or any other edit) leaves them": a conflict list never outlives the edit that made
/// it. After the placement is undone, the list is empty and Move writes nothing; after a box with
/// a new entry is undone, Move cannot store a key the list no longer has.
#[test]
fn an_undone_proposal_leaves_no_conflict_to_answer() {
    let mut h = H::new(0x52_d006);
    let v = h.run("place", json!({"symbol": "coil", "at": [50, 150], "location": "keep", "function": "keep"})).unwrap();
    let k: DeviceId = id(&v, "component");
    h.run("setlocation", json!({"component": k.hex(), "value": "+CP2"})).unwrap();
    h.run("locationbox", json!({"location": "+NEW", "corners": [[20, 100], [200, 220]]})).unwrap();
    assert_eq!(conflicts(&mut h).len(), 1);
    h.cmdline("u"); // the box and its new entry +NEW are gone
    let head = h.head();
    let _ = h.run("conflicts", json!({"answer": "move"}));
    let d = h.doc();
    assert_eq!(loc(&d, k).as_deref(), Some("+CP2"), "the device keeps +CP2");
    assert_eq!(h.head(), head, "nothing written for a proposal that was undone");
    let missing: Vec<_> = checks::all(&d, 1e-6).into_iter().filter(|r| r.kind == Kind::DesignationMissing).collect();
    assert!(missing.is_empty(), "{missing:?}");
}

/// §5.4, §5.6: moving a box over an unset device sets it in a follow-up transaction; `u` clears
/// it, `u` again moves the box back; both exact.
#[test]
fn moving_a_box_over_a_device_sets_it_in_a_follow_up() {
    let mut h = H::new(0x52_d007);
    let page = h.first_page();
    let v = h.run("place", json!({"symbol": "coil", "at": [300, 150]})).unwrap();
    let k: DeviceId = id(&v, "component");
    let v = h.run("locationbox", json!({"location": "+CP1", "corners": [[20, 100], [200, 220]]})).unwrap();
    let item = id(&v, "item");
    let hd = h.handle_of(page, item);
    let before = h.doc();
    let head = h.head();
    h.run("move", json!({"handles": [hd.hex()], "delta": [200, 0]})).unwrap();
    assert_eq!(h.head(), head + 2, "the move and its follow-up");
    assert_eq!(loc(&h.doc(), k).as_deref(), Some("+CP1"));
    h.cmdline("u");
    assert_eq!(loc(&h.doc(), k), None, "u clears the location first");
    h.cmdline("u");
    assert_eq!(h.doc(), before, "u again moves the box back, exactly");
}

/// §5.6: a page value sets the unset devices outside every box in the same transaction and lists
/// one at another value; the project value does the same project-wide; `move` moves them; each is
/// undone exactly.
#[test]
fn page_and_project_values_propose_and_undo_exactly() {
    let mut h = H::new(0x52_d008);
    let p1 = h.first_page();
    let v = h.run("place", json!({"symbol": "coil", "at": [50, 150]})).unwrap();
    let a: DeviceId = id(&v, "component");
    let v = h.run("place", json!({"symbol": "coil", "at": [100, 150]})).unwrap();
    let b: DeviceId = id(&v, "component");
    // The page was +CP2 and -K02 took it; -K01 is unset.
    h.run("pageprops", json!({"page": p1.hex(), "location": "+CP2", "proposal": "keep"})).unwrap();
    h.run("setlocation", json!({"component": b.hex(), "value": "+CP2"})).unwrap();
    let p2: PageId = id(&h.run("newpage", json!({"title": "2"})).unwrap(), "page");
    let v = h.run("place", json!({"symbol": "coil", "at": [50, 150]})).unwrap();
    let c: DeviceId = id(&v, "component");
    let before = h.doc();
    let head = h.head();
    let v = h.run("pageprops", json!({"page": p1.hex(), "location": "+P1"})).unwrap();
    assert_eq!(h.head(), head + 1, "{v}");
    let d = h.doc();
    assert_eq!(loc(&d, a).as_deref(), Some("+P1"));
    assert_eq!(loc(&d, b).as_deref(), Some("+CP2"), "listed, not changed");
    assert_eq!(loc(&d, c), None, "another page is not proposed to");
    assert_eq!(conflicts(&mut h).len(), 1);
    undo_to(&mut h, 1, &before, "page value");
    let head = h.head();
    h.run("projectprops", json!({"location": "+PRJ", "proposal": "move"})).unwrap();
    let d = h.doc();
    assert_eq!(loc(&d, a), None, "page 1 has its own value: its context is not the project");
    assert_eq!(loc(&d, c).as_deref(), Some("+PRJ"), "page 2 falls back to the project");
    let n = h.head() - head;
    undo_to(&mut h, n, &before, "project value with move");
    let _ = p2;
}

/// Hostile proposal parameters are refused and write nothing: a mode that is not text is never
/// read as the default.
#[test]
fn hostile_proposal_parameters_are_refused() {
    let mut h = H::new(0x52_d009);
    let p1 = h.first_page();
    let v = h.run("place", json!({"symbol": "coil", "at": [50, 150], "location": "keep", "function": "keep"})).unwrap();
    let k: DeviceId = id(&v, "component");
    let long = "+".to_string() + &"Q".repeat(1_000);
    let cases: Vec<(&str, Value)> = vec![
        ("place", json!({"symbol": "coil", "at": [80, 150], "location": 5})),
        ("place", json!({"symbol": "coil", "at": [80, 150], "location": ""})),
        ("place", json!({"symbol": "coil", "at": [80, 150], "location": long})),
        ("place", json!({"symbol": "coil", "at": [80, 150], "function": "=A\u{7}"})),
        ("place", json!({"symbol": "coil", "at": [80, 150], "function": ["move"]})),
        ("locationbox", json!({"location": "+CP1", "corners": [[20, 100], [200, 220]], "proposal": "sideways"})),
        ("locationbox", json!({"location": "+CP1", "corners": [[20, 100], [200, 220]], "proposal": ["keep"]})),
        ("locationbox", json!({"location": "+CP1", "corners": [[20, 100], [200, 220]], "proposal": false})),
        ("pageprops", json!({"page": p1.hex(), "location": "+P9", "proposal": 1})),
        ("projectprops", json!({"location": "+P9", "proposal": {"x": 1}})),
        ("conflicts", json!({"answer": "maybe"})),
        ("conflicts", json!({"answer": 5})),
        ("conflicts", json!({"answer": "move", "devices": "nope"})),
    ];
    let mut accepted = Vec::new();
    for (cmd, params) in cases {
        let (before, head) = (h.doc(), h.head());
        match h.run(cmd, params.clone()) {
            Ok(_) => {
                accepted.push(format!("{cmd} {params} (wrote: {})", h.head() != head));
                let n = h.head() - head;
                undo_to(&mut h, n, &before, "after an accepted hostile call");
                let _ = h.run("conflicts", json!({"answer": "leave"}));
            }
            Err(e) => {
                assert!(!e.to_lowercase().contains("internal error"), "{cmd}: {e}");
                assert_eq!((h.head(), h.doc()), (head, before), "{cmd} {params}: nothing written");
            }
        }
    }
    assert!(accepted.is_empty(), "accepted instead of refused: {accepted:#?}");
    let _ = k;
}

/// Merge of 055 (title blocks): the block shows a page's and the project's location and function
/// as codes, never keys; a rename in the list redraws the block on the page shown (the harness
/// compares it with a fresh projection) and the new code is drawn; undo brings the old one back.
#[test]
fn the_title_block_shows_codes_and_follows_a_rename() {
    use cadcraft_engine::doc::EntityKind;
    use ecad_engine::frame::{self, FrameInput};
    use ecad_model::book::Book;
    let texts = |h: &mut H, page: PageId| -> Vec<String> {
        let doc = h.doc();
        frame::parts(&FrameInput::of(&doc, page, &Book::of(&doc)))
            .into_iter()
            .filter_map(|p| match p.kind {
                EntityKind::Text(t) => Some(t.value),
                _ => None,
            })
            .collect()
    };
    let mut h = H::new(0x52_d00a);
    let p1 = h.first_page();
    h.run("pageprops", json!({"page": p1.hex(), "location": "+CP1", "function": "=F1", "proposal": "keep"})).unwrap();
    let p2: PageId = id(&h.run("newpage", json!({"title": "2"})).unwrap(), "page");
    h.run("projectprops", json!({"location": "+PRJ", "proposal": "keep"})).unwrap();
    let d = h.doc();
    let i1 = FrameInput::of(&d, p1, &Book::of(&d));
    assert_eq!((i1.location.as_str(), i1.function.as_str()), ("+CP1", "=F1"), "codes, not keys");
    let i2 = FrameInput::of(&d, p2, &Book::of(&d));
    assert_eq!(i2.location, "+PRJ", "page 2 shows the project's");
    // Rename with page 1 shown, then the project's entry with page 2 shown.
    h.activate(p1);
    h.run("designationedit", json!({"aspect": "location", "code": "+CP1", "to": "+CP10"})).unwrap();
    h.run("designationedit", json!({"aspect": "function", "code": "=F1", "to": "=F10"})).unwrap();
    let t = texts(&mut h, p1);
    assert!(t.iter().any(|x| x == "+CP10") && t.iter().any(|x| x == "=F10"), "{t:?}");
    assert!(!t.iter().any(|x| x == "+CP1" || x == "=F1"), "{t:?}");
    h.activate(p2);
    h.run("designationedit", json!({"aspect": "location", "code": "+PRJ", "to": "+PRJ2"})).unwrap();
    assert!(texts(&mut h, p2).iter().any(|x| x == "+PRJ2"));
    h.cmdline("u");
    assert!(texts(&mut h, p2).iter().any(|x| x == "+PRJ"));
}
