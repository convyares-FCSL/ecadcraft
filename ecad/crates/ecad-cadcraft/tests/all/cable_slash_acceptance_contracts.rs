//! Independent tester's acceptance checks for brief 004 (cable slash), written from the brief, the
//! owner's answers and design 004 only (fixture F of design §10, the crossing table of §4, the
//! messages of §5.3/§5.4/§6 and the existing-command table of §8).
//!
//! Every check runs through the harness in `common`, which after each step asserts replay equals
//! the live document, the drawing equals the projection and the handle map is one-to-one.

#![cfg(not(target_arch = "wasm32"))]

use crate::common;

use std::path::PathBuf;

use cadcraft_engine::geom::Vec2;
use cadcraft_engine::{Input, Session};
use common::{H, id};
use ecad_cadcraft::{Persistence, ProjectHandle};
use ecad_model::{
    CableId, ConductorId, Document, Form, ItemId, ItemKind, Op, PageId, Primitive, Pt, Subject, Transaction,
};
use serde_json::{Value, json};

const SEED: u64 = 404;

/// Design §10 fixture F: three parallel wires W0, W5, W10 (by their y) and a plain line at y = 15.
struct F {
    h: H,
    page: PageId,
    w0: ConductorId,
    w5: ConductorId,
    w10: ConductorId,
    plain: ItemId,
}

fn wire(h: &mut H, pts: &[(f64, f64)]) -> ConductorId {
    let p: Vec<Value> = pts.iter().map(|(x, y)| json!([x, y])).collect();
    id(&h.run("wire", json!({ "points": p })).unwrap(), "conductor")
}

fn fixture(seed: u64) -> F {
    let mut h = H::new(seed);
    h.s.viewport_px = (1280.0, 800.0);
    let w0 = wire(&mut h, &[(0.0, 0.0), (60.0, 0.0)]);
    let w5 = wire(&mut h, &[(0.0, 5.0), (60.0, 5.0)]);
    let w10 = wire(&mut h, &[(0.0, 10.0), (60.0, 10.0)]);
    h.run("line", json!({"points": [[0, 15], [60, 15]]})).unwrap();
    h.run("zoom", json!({"mode": "extents"})).unwrap();
    let page = h.first_page();
    let plain = plain_items(&h.doc());
    assert_eq!(plain.len(), 1, "one plain line");
    F { h, page, w0, w5, w10, plain: plain[0] }
}

/// Items that belong to no representation.
fn plain_items(d: &Document) -> Vec<ItemId> {
    d.items.keys().filter(|i| !d.representations.values().any(|r| r.items.contains(i))).copied().collect()
}

fn slash(h: &mut H, from: (f64, f64), to: (f64, f64)) -> Result<Value, String> {
    h.run("cableslash", json!({"from": [from.0, from.1], "to": [to.0, to.1]}))
}

fn hexes(v: &Value, key: &str) -> Vec<String> {
    v[key].as_array().map(|a| a.iter().map(|x| x.as_str().unwrap().to_string()).collect()).unwrap_or_default()
}

fn members(d: &Document, k: CableId) -> Vec<ConductorId> {
    d.cables[&k].members.iter().map(|m| m.conductor).collect()
}

/// Every conductor with its ends and the items of its representations, for "unchanged" checks.
fn conductors_snapshot(d: &Document) -> Vec<String> {
    d.conductors
        .iter()
        .map(|(c, x)| {
            let reps: Vec<String> = d
                .representations_of(Subject::Conductor(*c))
                .map(|(rid, r)| format!("{rid:?} {r:?} {:?}", r.items.iter().map(|i| &d.items[i]).collect::<Vec<_>>()))
                .collect();
            format!("{c:?} {x:?} {reps:?}")
        })
        .collect()
}

fn history(h: &mut H) -> Vec<Transaction> {
    h.p(|p| p.ledger.log().iter().map(|t| Transaction { time_ms: None, ..t.clone() }).collect())
}

fn last_tx(h: &mut H) -> Transaction {
    h.p(|p| p.ledger.log().last().cloned()).unwrap()
}

fn prompt(h: &H) -> String {
    h.s.current_prompt().map(|p| p.display()).unwrap_or_default()
}

fn click(h: &mut H, x: f64, y: f64) {
    let at = Vec2::new(x, y);
    h.s.cursor = at;
    h.s.input(Input::Point(at)).unwrap();
    h.check();
}

fn last_log(h: &H) -> String {
    h.s.log.last().cloned().unwrap_or_default()
}

/// Distinct centres of the circles in a preview (one per ring, however many strokes it has).
fn ring_centres(e: &[cadcraft_engine::doc::Entity]) -> Vec<(i64, i64)> {
    let mut out: Vec<(i64, i64)> = e
        .iter()
        .filter_map(|x| match &x.kind {
            cadcraft_engine::doc::EntityKind::Circle(c) => {
                Some(((c.center.x * 1e6).round() as i64, (c.center.y * 1e6).round() as i64))
            }
            _ => None,
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

fn short(hex: &str) -> &str {
    &hex[..8]
}

// ------------------------------------------------------------------ AC1, AC4

/// AC1, AC4 (design check B, C): one transaction, ops exactly as design §2, members in crossing
/// order with `core` None, the plain line ignored and reported, conductors untouched.
#[test]
fn ac1_one_transaction_with_members_in_crossing_order_and_conductors_unchanged() {
    for (from, to, reversed) in [((28.0, -3.0), (32.0, 18.0), false), ((32.0, 18.0), (28.0, -3.0), true)] {
        let mut f = fixture(SEED);
        let before = f.h.doc();
        let head = f.h.head();
        let r = slash(&mut f.h, from, to).unwrap();
        assert_eq!(f.h.head(), head + 1, "exactly one transaction");
        let d = f.h.doc();
        let k: CableId = id(&r, "cable");
        let want = if reversed { vec![f.w10, f.w5, f.w0] } else { vec![f.w0, f.w5, f.w10] };
        assert_eq!(members(&d, k), want, "crossing order from `from` to `to`");
        assert!(d.cables[&k].members.iter().all(|m| m.core.is_none()), "core stays None");
        assert!(!d.cables[&k].properties.contains_key("tag"), "no tag unless given");
        let want_hex: Vec<String> = want.iter().map(|c| c.hex()).collect();
        assert_eq!(hexes(&r, "conductors"), want_hex);
        // AC4: the plain line is reported, not taken.
        assert_eq!(hexes(&r, "ignored"), vec![f.plain.hex()]);
        assert!(hexes(&r, "touched").is_empty());
        assert_eq!(conductors_snapshot(&d), conductors_snapshot(&before), "DOC-04: conductors unchanged");
        // Design §2: the op list, in order.
        let tx = last_tx(&mut f.h);
        assert_eq!(tx.label, "Cable slash");
        assert_eq!(tx.ops.len(), 6, "{:?}", tx.ops);
        assert!(matches!(&tx.ops[0], Op::CreateCable { id, cable } if *id == k && cable.members.is_empty()));
        for (i, c) in want.iter().enumerate() {
            assert!(
                matches!(&tx.ops[1 + i], Op::AddCableMember { cable, index, member }
                    if *cable == k && *index == i && member.conductor == *c && member.core.is_none()),
                "member op {i}: {:?}",
                tx.ops[1 + i]
            );
        }
        let item: ItemId = id(&r, "item");
        match &tx.ops[4] {
            Op::AddItem { id, item: it } => {
                assert_eq!(*id, item);
                assert_eq!(it.page, f.page);
                match &it.kind {
                    ItemKind::Primitive(Primitive::Line { a, b }) => {
                        assert_eq!((a.x, a.y, b.x, b.y), (from.0, from.1, to.0, to.1), "slash geometry from -> to");
                    }
                    other => panic!("slash is a line: {other:?}"),
                }
                // O4: the same style as a new wire.
                let wire_item = d.representations_of(Subject::Conductor(f.w0)).next().unwrap().1.items[0];
                assert_eq!(it.style, d.items[&wire_item].style, "slash in the wire style (O4)");
            }
            other => panic!("op 4 is AddItem: {other:?}"),
        }
        match &tx.ops[5] {
            Op::LinkRepresentation { id: rid, rep } => {
                assert_eq!(rid.hex(), r["representation"].as_str().unwrap());
                assert_eq!(rep.subject, Subject::Cable(k));
                assert_eq!(rep.page, f.page);
                assert_eq!(rep.form, Form::Drawn);
                assert_eq!(rep.role, "slash");
                assert_eq!(rep.items, vec![item]);
                assert!(rep.connection_points.is_empty());
            }
            other => panic!("op 5 is LinkRepresentation: {other:?}"),
        }
    }
}

/// AC1 with N wires: 1, 2 and 7 parallel wires each give one cable of exactly those wires.
#[test]
fn ac1_n_parallel_wires_give_one_cable_of_exactly_n() {
    for n in [1usize, 2, 7] {
        let mut h = H::new(SEED + n as u64);
        let ws: Vec<ConductorId> =
            (0..n).map(|i| wire(&mut h, &[(0.0, i as f64 * 3.0), (50.0, i as f64 * 3.0)])).collect();
        // A wire elsewhere that the slash does not reach.
        let other = wire(&mut h, &[(100.0, 0.0), (150.0, 0.0)]);
        let r = slash(&mut h, (25.0, -1.0), (25.0, n as f64 * 3.0)).unwrap();
        let k: CableId = id(&r, "cable");
        let d = h.doc();
        assert_eq!(members(&d, k), ws, "{n} wires");
        assert!(!members(&d, k).contains(&other));
        assert_eq!(d.cables.len(), 1);
    }
}

/// AC4 and design §5.3: the interactive echo names the cable, the wires and the ignored line.
#[test]
fn ac4_the_echo_reports_ignored_lines() {
    let mut f = fixture(SEED);
    f.h.s.start("cableslash").unwrap();
    click(&mut f.h, 28.0, -3.0);
    click(&mut f.h, 32.0, 18.0);
    assert!(f.h.s.running.is_none(), "the command ends after a commit");
    let log = f.h.s.log.join("\n");
    let d = f.h.doc();
    let k = *d.cables.keys().next().unwrap();
    assert!(log.contains(&format!("Cable {} created with 3 wires: ", k.hex())), "{log}");
    for w in [f.w0, f.w5, f.w10] {
        assert!(log.contains(&format!("wire {}", short(&w.hex()))), "{log}");
    }
    assert!(log.contains("1 object that is not a wire was ignored."), "{log}");
    assert!(!log.contains("touched at its end"), "{log}");
}

/// Design §5.3: tags name wires; more than five are summarised as "and N more".
#[test]
fn ac4_wire_names_use_tags_and_are_capped_at_five() {
    let mut h = H::new(SEED);
    let ws: Vec<ConductorId> = (0..7).map(|i| wire(&mut h, &[(0.0, i as f64), (10.0, i as f64)])).collect();
    h.run("setprop", json!({"conductor": ws[0].hex(), "key": "tag", "value": "W1"})).unwrap();
    h.s.start("cableslash").unwrap();
    click(&mut h, 5.0, -1.0);
    click(&mut h, 5.0, 7.0);
    let log = h.s.log.join("\n");
    assert!(log.contains("created with 7 wires: W1, "), "{log}");
    assert!(log.contains("and 2 more"), "{log}");
}

// ------------------------------------------------------------------ AC2

/// AC2 (design check E): `u` removes cable, members, slash and representation in one step; `redo`
/// restores all of them with the same IDs.
#[test]
fn ac2_undo_removes_cable_and_slash_in_one_step_and_redo_restores_the_same_ids() {
    let mut f = fixture(SEED);
    let before = f.h.doc();
    let r = slash(&mut f.h, (28.0, -3.0), (32.0, 18.0)).unwrap();
    let after = f.h.doc();
    let head = f.h.head();
    f.h.run("u", Value::Null).unwrap();
    assert_eq!(f.h.head(), head + 1, "one undo transaction");
    let d = f.h.doc();
    assert_eq!(d, before, "cable, members, slash item and representation all gone in one step");
    assert!(d.cables.is_empty());
    let item: ItemId = id(&r, "item");
    assert!(!d.items.contains_key(&item));
    assert!(f.h.lines_on(f.page).len() == 4, "only the three wires and the plain line are drawn");
    f.h.run("redo", Value::Null).unwrap();
    assert_eq!(f.h.doc(), after, "redo gives back the same cable, members, slash and IDs");
    let k: CableId = id(&r, "cable");
    assert_eq!(members(&f.h.doc(), k), vec![f.w0, f.w5, f.w10]);
    assert!(f.h.doc().items.contains_key(&item));
    // Undo again after redo, then a typed `u` at the command line behaves the same.
    f.h.cmdline("u");
    assert_eq!(f.h.doc(), before);
    f.h.cmdline("redo");
    assert_eq!(f.h.doc(), after);
}

/// AC2 for the interactive form: the click commit is undone in one step too.
#[test]
fn ac2_undo_after_an_interactive_slash_is_one_step() {
    let mut f = fixture(SEED);
    let before = f.h.doc();
    f.h.s.start("cableslash").unwrap();
    click(&mut f.h, 28.0, -3.0);
    click(&mut f.h, 32.0, 18.0);
    let after = f.h.doc();
    assert_eq!(after.cables.len(), 1);
    f.h.cmdline("u");
    assert_eq!(f.h.doc(), before);
    f.h.cmdline("redo");
    assert_eq!(f.h.doc(), after);
}

// ------------------------------------------------------------------ AC3

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("ecad-tester-004-{name}-{}", ecad_store::disk::unique()));
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

fn close_all(s: &mut Session) {
    while !s.docs.is_empty() {
        s.execute("close", &json!({"index": 0})).unwrap();
    }
}

/// AC3 (design check F): save, close and reopen give the same cable, members, order and
/// representation, and replay equals the live state; the reopened cable still undoes and redoes.
#[test]
fn ac3_save_close_reopen_keeps_cable_members_order_and_slash() {
    let dir = TempDir::new("reopen");
    let mut f = fixture(SEED);
    f.h.cfg(|p| p.persistence = Persistence::File);
    // Reversed slash, so stored order differs from ID or creation order.
    let r = slash(&mut f.h, (32.0, 18.0), (28.0, -3.0)).unwrap();
    f.h.run("cableslash", json!({"from": [80, -1], "to": [80, 1], "tag": "C2"})).unwrap_err();
    let k: CableId = id(&r, "cable");
    let path = dir.path("slash.ecad");
    f.h.run("saveas", json!({"path": path})).unwrap();
    let live = f.h.doc();
    let log = f.h.p(|p| p.ledger.log().to_vec());
    close_all(&mut f.h.s);

    let mut s = Session::empty();
    let (project, _) = ProjectHandle::open_file(&mut s, &path, 99).unwrap();
    let mut h2 = H { s, project, trail: Vec::new() };
    let d = h2.doc();
    assert_eq!(d, live, "same cable, members, order, slash item and representation");
    assert_eq!(members(&d, k), vec![f.w10, f.w5, f.w0]);
    assert_eq!(h2.p(|p| p.ledger.log().to_vec()), log, "same history");
    assert!(h2.p(|p| p.ledger.replay_matches().unwrap()), "replay equals the live state");
    h2.check();
    let (rid, rep) = d.representations_of(Subject::Cable(k)).next().unwrap();
    assert_eq!(rid.hex(), r["representation"].as_str().unwrap());
    assert_eq!(rep.role, "slash");
    // The reopened slash still refuses a second cable over the same wire and undoes in one step.
    let head = h2.head();
    let e = h2.run("cableslash", json!({"from": [40, 3], "to": [40, 7]})).unwrap_err();
    assert!(e.contains("already in cable"), "{e}");
    assert_eq!(h2.head(), head);
    h2.run("u", Value::Null).unwrap();
    assert!(h2.doc().cables.is_empty());
    h2.run("redo", Value::Null).unwrap();
    assert_eq!(h2.doc(), live);
    // Save again and reopen once more: still equal.
    h2.run("qsave", Value::Null).unwrap();
    let live2 = h2.doc();
    close_all(&mut h2.s);
    let mut s = Session::empty();
    let (project, _) = ProjectHandle::open_file(&mut s, &path, 100).unwrap();
    let mut h3 = H { s, project, trail: Vec::new() };
    assert_eq!(h3.doc(), live2);
    h3.check();
}

/// AC3: replaying the history from scratch gives the same state after every slash step, including
/// refusals, undo, redo, erase and cable delete.
#[test]
fn ac3_replay_equals_live_after_every_step() {
    let mut f = fixture(SEED);
    let r = slash(&mut f.h, (28.0, -3.0), (32.0, 18.0)).unwrap();
    let _ = slash(&mut f.h, (40.0, 3.0), (40.0, 7.0));
    f.h.run("u", Value::Null).unwrap();
    f.h.run("redo", Value::Null).unwrap();
    let item: ItemId = id(&r, "item");
    let hd = f.h.handle_of(f.page, item);
    f.h.run("erase", json!({"handles": [hd.hex()]})).unwrap();
    f.h.run("ecaddelete", json!({"cable": r["cable"]})).unwrap();
    f.h.run("u", Value::Null).unwrap();
    assert!(f.h.p(|p| p.ledger.replay_matches().unwrap()));
    // `check` (run after every step above) already asserted replay and projection each time.
}

// ------------------------------------------------------------------ AC5

/// AC5 (design checks G, H) and §5.4: each refusal leaves head and document unchanged, with its
/// message, for the JSON form.
#[test]
fn ac5_refusals_leave_the_head_unchanged_with_a_clear_message() {
    let mut f = fixture(SEED);
    let first = slash(&mut f.h, (28.0, -3.0), (32.0, 18.0)).unwrap();
    let k: CableId = id(&first, "cable");
    let w5 = f.w5.hex();
    let cases: Vec<(&str, (f64, f64), (f64, f64), String)> = vec![
        ("empty space", (100.0, 100.0), (110.0, 110.0), "The slash crosses no wires; nothing was created.".into()),
        (
            "plain line only",
            (10.0, 13.0),
            (10.0, 17.0),
            "The slash crosses no wires; nothing was created. It crosses 1 object that is not a wire.".into(),
        ),
        // Design §5.4 names an untagged wire `wire <8 hex>…` after "Wire"; the implementation drops
        // the doubled word ("Wire c4516ba9… is already in cable 5d80fc29…"). Either reads clearly.
        ("W5 already in a cable", (40.0, 3.0), (40.0, 7.0), format!("{}… is already in cable ", short(&w5))),
    ];
    for (what, from, to, msg) in cases {
        let before = f.h.doc();
        let head = f.h.head();
        let e = slash(&mut f.h, from, to).unwrap_err();
        assert_eq!(f.h.head(), head, "{what}: head unchanged");
        assert_eq!(f.h.doc(), before, "{what}: nothing written");
        assert!(
            e.contains(&msg),
            "{what}: {e:?}
 want {msg:?}"
        );
        if what.starts_with("W5") {
            assert!(e.starts_with("Wire "), "{e}");
            assert!(e.contains(short(&k.hex())), "{what}: names the cable: {e}");
            assert!(e.ends_with("; a wire can be in only one cable. Nothing was created."), "{e}");
        }
    }
    // A slash over W5 and a free wire is refused as a whole (no partial cable).
    let free = wire(&mut f.h, &[(70.0, 0.0), (70.0, 30.0)]);
    let head = f.h.head();
    let e = slash(&mut f.h, (30.0, 4.0), (80.0, 6.0)).unwrap_err();
    assert!(e.contains("already in cable"), "{e}");
    assert_eq!(f.h.head(), head);
    assert!(f.h.doc().cables.values().all(|c| !c.members.iter().any(|m| m.conductor == free)));
    // With tags, the names are the tags (design §5.4).
    f.h.run("setprop", json!({"conductor": w5, "key": "tag", "value": "W5"})).unwrap();
    f.h.run("setprop", json!({"cable": k.hex(), "key": "tag", "value": "C1"})).unwrap();
    let e = slash(&mut f.h, (40.0, 3.0), (40.0, 7.0)).unwrap_err();
    assert!(e.contains("Wire W5 is already in cable C1; a wire can be in only one cable. Nothing was created."), "{e}");
}

/// AC5 interactive (design check G, §5.1 step 3): a refused second point writes nothing, echoes the
/// message and keeps the first point at the step-2 prompt; another second point then commits.
#[test]
fn ac5_interactive_refusal_keeps_the_first_point() {
    let mut f = fixture(SEED);
    slash(&mut f.h, (28.0, -3.0), (32.0, 18.0)).unwrap();
    let head = f.h.head();
    f.h.s.start("cableslash").unwrap();
    assert_eq!(prompt(&f.h), "Specify first point of cable slash:");
    click(&mut f.h, 50.0, 3.0);
    assert_eq!(prompt(&f.h), "Specify second point of cable slash or [Undo]:");
    click(&mut f.h, 50.0, 7.0);
    assert_eq!(f.h.head(), head, "refused: nothing written");
    assert!(last_log(&f.h).contains("already in cable"), "{}", last_log(&f.h));
    assert!(f.h.s.running.is_some(), "the command goes on");
    assert_eq!(prompt(&f.h), "Specify second point of cable slash or [Undo]:");
    // Same first point, a second point in empty space: crosses no wires, refused, prompt stays.
    click(&mut f.h, 50.0, 4.0);
    assert_eq!(f.h.head(), head);
    assert_eq!(last_log(&f.h), "The slash crosses no wires; nothing was created.");
    assert_eq!(prompt(&f.h), "Specify second point of cable slash or [Undo]:");
    // Esc: nothing written, command ends.
    f.h.s.input(Input::Cancel).unwrap();
    assert!(f.h.s.running.is_none());
    assert_eq!(f.h.head(), head);
}

/// O5: `cableadd` also refuses a conductor already in a cable, head unchanged.
#[test]
fn ac5_cableadd_refuses_a_wire_already_in_a_cable() {
    let mut f = fixture(SEED);
    slash(&mut f.h, (28.0, -3.0), (32.0, 18.0)).unwrap();
    let head = f.h.head();
    let before = f.h.doc();
    let e = f.h.run("cableadd", json!({"conductors": [f.w5.hex()], "tag": "X"})).unwrap_err();
    assert!(e.contains("already in cable") && e.contains("only one cable"), "{e}");
    assert_eq!(f.h.head(), head);
    assert_eq!(f.h.doc(), before);
}

/// AC5 read-only (owner answer O2): the slash behaves like the other ECAD edits on a read-only
/// project, and Save is still refused there, so the other session's file stays as it was.
#[test]
fn ac5_read_only_project_treats_the_slash_like_other_ecad_edits() {
    let dir = TempDir::new("readonly");
    let mut f = fixture(SEED);
    f.h.cfg(|p| p.persistence = Persistence::File);
    let path = dir.path("shared.ecad");
    f.h.run("saveas", json!({"path": path})).unwrap();
    let mut s = Session::empty();
    let (project, summary) = ProjectHandle::open_file(&mut s, &path, 7).unwrap();
    assert!(summary.contains("read-only"), "{summary}");
    let mut h2 = H { s, project, trail: Vec::new() };
    let file = std::fs::read(&path).unwrap();
    let w = h2.run("wire", json!({"points": [[0, 40], [60, 40]]})).is_ok();
    let sl = h2.run("cableslash", json!({"from": [28, -3], "to": [32, 18]}));
    assert_eq!(sl.is_ok(), w, "the slash behaves like `wire` on a read-only project: {sl:?}");
    assert!(h2.run("qsave", Value::Null).is_err(), "Save refused read-only");
    assert_eq!(std::fs::read(&path).unwrap(), file, "the other session's file is untouched");
    // Round 2, quick-start limitation: "edits work in memory; use Save As to keep them".
    let k: CableId = id(&sl.unwrap(), "cable");
    let live = h2.doc();
    let copy = dir.path("mine.ecad");
    h2.run("saveas", json!({"path": copy})).unwrap();
    assert!(!h2.p(|p| p.unsaved()));
    assert_eq!(std::fs::read(&path).unwrap(), file, "Save As does not touch the original");
    close_all(&mut h2.s);
    let mut s = Session::empty();
    let (project, summary) = ProjectHandle::open_file(&mut s, &copy, 8).unwrap();
    assert!(!summary.contains("read-only"), "{summary}");
    let mut h3 = H { s, project, trail: Vec::new() };
    assert_eq!(h3.doc(), live, "the copy keeps the wire and the cable made while read-only");
    assert_eq!(members(&h3.doc(), k), vec![f.w0, f.w5, f.w10]);
    h3.check();
}

// ------------------------------------------------------------------ AC6, scope 4

/// AC6 (design check K): erasing the slash keeps the cable and members (only the representation
/// goes); `ecaddelete {cable}` then deletes the cable and keeps every conductor.
#[test]
fn ac6_erase_keeps_the_cable_and_delete_keeps_the_conductors() {
    let mut f = fixture(SEED);
    let conductors = conductors_snapshot(&f.h.doc());
    let r = slash(&mut f.h, (28.0, -3.0), (32.0, 18.0)).unwrap();
    let k: CableId = id(&r, "cable");
    let item: ItemId = id(&r, "item");
    let hd = f.h.handle_of(f.page, item);
    f.h.run("erase", json!({"handles": [hd.hex()]})).unwrap();
    let d = f.h.doc();
    assert_eq!(members(&d, k), vec![f.w0, f.w5, f.w10], "the cable and its members stay");
    assert!(!d.items.contains_key(&item), "the slash item is gone");
    assert_eq!(d.representations_of(Subject::Cable(k)).count(), 0, "the representation is gone");
    let tx = last_tx(&mut f.h);
    assert!(
        tx.ops.iter().all(|o| matches!(o, Op::RemoveItem { .. } | Op::UnlinkRepresentation { .. })),
        "{:?}",
        tx.ops
    );
    // The cable still blocks: its wires are still members.
    assert!(slash(&mut f.h, (40.0, 3.0), (40.0, 7.0)).unwrap_err().contains("already in cable"));
    // Delete the cable.
    let head = f.h.head();
    f.h.run("ecaddelete", json!({"cable": k.hex()})).unwrap();
    assert_eq!(f.h.head(), head + 1, "one transaction");
    let d = f.h.doc();
    assert!(d.cables.is_empty());
    assert_eq!(conductors_snapshot(&d), conductors, "every conductor, its ends and route kept");
    // Now the wires are free again.
    slash(&mut f.h, (40.0, 3.0), (40.0, 7.0)).unwrap();
}

/// AC6 and design §8: `ecaddelete {cable}` with its slash still drawn removes the members, the
/// slash and the cable in one transaction, in that op order, and undo brings it all back.
#[test]
fn ac6_deleting_a_cable_with_its_slash_removes_both_and_undoes() {
    let mut f = fixture(SEED);
    let conductors = conductors_snapshot(&f.h.doc());
    let r = slash(&mut f.h, (28.0, -3.0), (32.0, 18.0)).unwrap();
    let with = f.h.doc();
    let k: CableId = id(&r, "cable");
    f.h.run("ecaddelete", json!({"cable": k.hex()})).unwrap();
    let echo = last_log(&f.h);
    let tx = last_tx(&mut f.h);
    let kinds: Vec<&str> = tx
        .ops
        .iter()
        .map(|o| match o {
            Op::RemoveCableMember { .. } => "member",
            Op::RemoveItem { .. } => "item",
            Op::UnlinkRepresentation { .. } => "unlink",
            Op::DeleteCable { .. } => "cable",
            _ => "other",
        })
        .collect();
    // Design §8 lists `RemoveItem` then `UnlinkRepresentation`; the implementation unlinks first.
    // Both replay to the same state (checked by the harness), so either order is accepted here.
    assert!(
        kinds == ["member", "member", "member", "item", "unlink", "cable"]
            || kinds == ["member", "member", "member", "unlink", "item", "cable"],
        "{:?}",
        tx.ops
    );
    let idx: Vec<usize> = tx
        .ops
        .iter()
        .filter_map(|o| if let Op::RemoveCableMember { index, .. } = o { Some(*index) } else { None })
        .collect();
    assert_eq!(idx, [2, 1, 0], "last index first");
    let d = f.h.doc();
    assert!(d.cables.is_empty() && !d.items.contains_key(&id::<ItemId>(&r, "item")));
    assert_eq!(conductors_snapshot(&d), conductors);
    let _ = echo; // the echo text is checked through the command line below
    f.h.run("u", Value::Null).unwrap();
    assert_eq!(f.h.doc(), with);
    f.h.cmdline(&format!("ecaddelete {}", json!({"cable": k.hex()})));
    assert!(f.h.s.log.join("\n").contains("its 3 wires are kept"), "{:?}", f.h.s.log.last());
}

/// Scope 4: moving member wires (and the slash) changes no membership; deleting a member wire
/// leaves the cable (P7, Q4), even empty.
#[test]
fn scope4_moving_wires_or_the_slash_keeps_membership_and_an_emptied_cable_stays() {
    let mut f = fixture(SEED);
    let r = slash(&mut f.h, (28.0, -3.0), (32.0, 18.0)).unwrap();
    let k: CableId = id(&r, "cable");
    let route = f.h.route_handle(f.page, f.w5);
    f.h.run("move", json!({"handles": [route.hex()], "delta": [0, 50]})).unwrap();
    assert_eq!(members(&f.h.doc(), k), vec![f.w0, f.w5, f.w10]);
    let slash_h = f.h.handle_of(f.page, id(&r, "item"));
    f.h.run("move", json!({"handles": [slash_h.hex()], "delta": [500, 0]})).unwrap();
    assert_eq!(members(&f.h.doc(), k), vec![f.w0, f.w5, f.w10]);
    for w in [f.w0, f.w5, f.w10] {
        f.h.run("ecaddelete", json!({"conductor": w.hex()})).unwrap();
    }
    let d = f.h.doc();
    assert!(d.cables.contains_key(&k), "an emptied cable stays");
    assert!(d.cables[&k].members.is_empty());
}

// ------------------------------------------------------------------ AC7

fn interactive_slash(h: &mut H, from: (f64, f64), to: (f64, f64)) {
    h.s.start("cableslash").unwrap();
    click(h, from.0, from.1);
    click(h, to.0, to.1);
}

/// AC7 (design check D): the interactive form and the JSON form give identical transactions for the
/// same points (the same seed, so even the IDs match; only wall-clock times may differ).
#[test]
fn ac7_interactive_and_json_give_identical_transactions() {
    for (from, to) in [((28.0, -3.0), (32.0, 18.0)), ((32.0, 18.0), (28.0, -3.0)), ((28.0, -3.0), (60.0, 10.0))] {
        let mut a = fixture(SEED);
        slash(&mut a.h, from, to).unwrap();
        let mut b = fixture(SEED);
        interactive_slash(&mut b.h, from, to);
        assert_eq!(history(&mut a.h), history(&mut b.h), "{from:?} -> {to:?}");
        assert_eq!(a.h.doc(), b.h.doc());
    }
    // Typed points at the prompts, including relative polar input.
    let mut a = fixture(SEED);
    slash(&mut a.h, (30.0, -3.0), (30.0, 17.0)).unwrap();
    let mut b = fixture(SEED);
    b.h.s.start("cableslash").unwrap();
    b.h.s.cmdline("30,-3").unwrap();
    b.h.s.cmdline("@20<90").unwrap();
    b.h.check();
    let (ha, hb) = (history(&mut a.h), history(&mut b.h));
    assert_eq!(ha.len(), hb.len());
    // `@20<90` resolves through cos/sin, so the x may differ by rounding: compare the members.
    let (da, db) = (a.h.doc(), b.h.doc());
    let ka = *da.cables.keys().next().unwrap();
    let kb = *db.cables.keys().next().unwrap();
    assert_eq!(members(&da, ka), members(&db, kb));
}

/// Design §5.1: Undo at step 2 drops the first point; Enter at either step ends with nothing
/// written; a second point equal to the first is ignored; Enter at `Command:` repeats the tool.
#[test]
fn ac7_prompt_machine_steps() {
    let mut f = fixture(SEED);
    let head = f.h.head();
    f.h.s.start("cableslash").unwrap();
    click(&mut f.h, 100.0, 100.0);
    f.h.s.cmdline("u").unwrap();
    assert_eq!(prompt(&f.h), "Specify first point of cable slash:", "Undo drops the first point");
    click(&mut f.h, 28.0, -3.0);
    let logs = f.h.s.log.len();
    click(&mut f.h, 28.0, -3.0);
    assert_eq!(prompt(&f.h), "Specify second point of cable slash or [Undo]:", "a repeated point is ignored");
    assert_eq!(f.h.s.log.len(), logs, "with no message");
    f.h.s.input(Input::Enter).unwrap();
    assert!(f.h.s.running.is_none(), "Enter at step 2 ends");
    assert_eq!(f.h.head(), head, "and writes nothing");
    f.h.s.start("cableslash").unwrap();
    f.h.s.input(Input::Enter).unwrap();
    assert!(f.h.s.running.is_none(), "Enter at step 1 ends");
    assert_eq!(f.h.head(), head);
    // Repeat by Enter at `Command:`.
    interactive_slash(&mut f.h, (28.0, -3.0), (32.0, 18.0));
    assert_eq!(f.h.head(), head + 1);
    f.h.s.input(Input::Enter).unwrap();
    assert_eq!(prompt(&f.h), "Specify first point of cable slash:", "Enter repeats cableslash");
    f.h.s.input(Input::Cancel).unwrap();
}

/// Design §5.2: the preview before the second click shows a green ring per joining wire and the
/// count text; a blocked wire shows red and the refusal text. The preview writes nothing.
#[test]
fn ac7_preview_shows_what_the_click_will_do() {
    use cadcraft_engine::doc::EntityKind;
    let texts = |e: &[cadcraft_engine::doc::Entity]| -> Vec<String> {
        e.iter()
            .filter_map(|x| match &x.kind {
                EntityKind::Text(t) => Some(t.value.clone()),
                _ => None,
            })
            .collect()
    };
    // A bold ring may be drawn as several concentric circles: count distinct centres.
    let circles = |e: &[cadcraft_engine::doc::Entity]| ring_centres(e).len();
    let mut f = fixture(SEED);
    let head = f.h.head();
    f.h.s.start("cableslash").unwrap();
    click(&mut f.h, 28.0, -3.0);
    let p = f.h.s.preview(Vec2::new(32.0, 18.0));
    assert_eq!(texts(&p), vec!["-> 3 wires, 1 other object ignored".to_string()], "{p:?}");
    assert_eq!(circles(&p), 3, "three rings");
    let p = f.h.s.preview(Vec2::new(28.5, 2.0));
    assert_eq!(texts(&p), vec!["-> 1 wire".to_string()]);
    let p = f.h.s.preview(Vec2::new(20.0, -10.0));
    assert_eq!(texts(&p), vec!["-> no wires".to_string()]);
    let p = f.h.s.preview(Vec2::new(f64::NAN, 0.0));
    assert!(texts(&p).iter().all(|t| !t.contains("wire")) || p.is_empty(), "no preview at NaN: {p:?}");
    assert_eq!(f.h.head(), head, "preview writes nothing");
    click(&mut f.h, 32.0, 18.0);
    f.h.s.start("cableslash").unwrap();
    click(&mut f.h, 40.0, 3.0);
    let p = f.h.s.preview(Vec2::new(40.0, 7.0));
    assert_eq!(texts(&p), vec!["-> refused: 1 wire already in a cable".to_string()], "{p:?}");
    let _ = f.h.s.input(Input::Cancel);
}

// ------------------------------------------------------------------ Crossing rules (design §4)

/// Design §4 / check J, with owner answer O1: C1 crossing counts, C2 bend counts, C3 slash end on
/// a wire counts, C4 touching a wire's end does not (reported), C5 running along does not, C6 a wire
/// crossed twice is one member, C9 other pages are not tested.
#[test]
fn crossing_rules_table() {
    // (case, extra wire, from, to, Ok(members by y-name) or Err(message part), touched by name)
    type Case = (
        &'static str,
        Option<Vec<(f64, f64)>>,
        (f64, f64),
        (f64, f64),
        Result<Vec<&'static str>, &'static str>,
        Vec<&'static str>,
    );
    let cases: Vec<Case> = vec![
        ("C1 plain crossing", None, (20.0, -1.0), (20.0, 1.0), Ok(vec!["W0"]), vec![]),
        (
            "C2 through a bend",
            Some(vec![(0.0, 20.0), (10.0, 20.0), (10.0, 30.0)]),
            (5.0, 15.0),
            (15.0, 25.0),
            Ok(vec!["X"]),
            vec![],
        ),
        ("C3 slash end on a wire", None, (20.0, 0.0), (20.0, -10.0), Ok(vec!["W0"]), vec![]),
        ("C3 other end on a wire", None, (20.0, -10.0), (20.0, 0.0), Ok(vec!["W0"]), vec![]),
        (
            "C4 only a wire end",
            None,
            (60.0, -2.0),
            (60.0, 2.0),
            Err("The slash crosses no wires; nothing was created."),
            vec![],
        ),
        ("C4 with others", None, (28.0, -3.0), (60.0, 10.0), Ok(vec!["W0", "W5"]), vec!["W10"]),
        (
            "C5 along a wire",
            None,
            (10.0, 0.0),
            (20.0, 0.0),
            Err("The slash crosses no wires; nothing was created."),
            vec![],
        ),
        (
            "C6 U-shaped wire crossed twice",
            Some(vec![(70.0, 0.0), (70.0, 10.0), (80.0, 10.0), (80.0, 0.0)]),
            (65.0, 5.0),
            (85.0, 5.0),
            Ok(vec!["X"]),
            vec![],
        ),
        ("C7 wire and plain line", None, (5.0, 8.0), (5.0, 17.0), Ok(vec!["W10"]), vec![]),
    ];
    for (case, extra, from, to, want, touched) in cases {
        let mut f = fixture(SEED);
        let x = extra.map(|pts| wire(&mut f.h, &pts));
        let name = |n: &str| match n {
            "W0" => f.w0,
            "W5" => f.w5,
            "W10" => f.w10,
            _ => x.unwrap(),
        };
        let head = f.h.head();
        match (slash(&mut f.h, from, to), want) {
            (Ok(r), Ok(names)) => {
                let want: Vec<String> = names.iter().map(|n| name(n).hex()).collect();
                assert_eq!(hexes(&r, "conductors"), want, "{case}");
                let t: Vec<String> = touched.iter().map(|n| name(n).hex()).collect();
                assert_eq!(hexes(&r, "touched"), t, "{case}: touched");
            }
            (Err(e), Err(msg)) => {
                assert!(e.contains(msg), "{case}: {e}");
                assert_eq!(f.h.head(), head, "{case}: head unchanged");
            }
            (got, want) => panic!("{case}: got {got:?}, want {want:?}"),
        }
    }
    // C4 echo line for the interactive form.
    let mut f = fixture(SEED);
    interactive_slash(&mut f.h, (28.0, -3.0), (60.0, 10.0));
    let log = f.h.s.log.join("\n");
    assert!(log.contains("1 wire touched at its end or along its length was not included."), "{log}");
}

/// C9: wires on another page are never members, and a slash on the second page does not reach the
/// first page's wires.
#[test]
fn crossing_ignores_other_pages() {
    let mut f = fixture(SEED);
    let r = f.h.run("newpage", json!({"title": "two"})).unwrap();
    let p2: PageId = id(&r, "page");
    f.h.activate(p2);
    let e = slash(&mut f.h, (28.0, -3.0), (32.0, 18.0)).unwrap_err();
    assert!(e.contains("crosses no wires"), "{e}");
    let w = wire(&mut f.h, &[(0.0, 5.0), (60.0, 5.0)]);
    let r = slash(&mut f.h, (28.0, -3.0), (32.0, 18.0)).unwrap();
    assert_eq!(hexes(&r, "conductors"), vec![w.hex()]);
    let d = f.h.doc();
    assert_eq!(d.items[&id::<ItemId>(&r, "item")].page, p2, "slash on the active page");
}

/// Design §4: equal crossing distances are ordered by conductor ID ascending as hex text.
#[test]
fn equal_distances_are_ordered_by_id() {
    let mut h = H::new(SEED);
    // Two wires crossing each other at (10,10); the slash passes through that point.
    let a = wire(&mut h, &[(0.0, 0.0), (20.0, 20.0)]);
    let b = wire(&mut h, &[(0.0, 20.0), (20.0, 0.0)]);
    let r = slash(&mut h, (10.0, 5.0), (10.0, 15.0)).unwrap();
    let mut want = vec![a.hex(), b.hex()];
    want.sort();
    assert_eq!(hexes(&r, "conductors"), want);
}

// ------------------------------------------------------------------ AC8 hostile input

/// AC8 and design §6, check L: malformed JSON params are refused with their messages, nothing is
/// written and nothing panics.
#[test]
fn ac8_hostile_json_params_are_refused_without_panic() {
    let mut f = fixture(SEED);
    let head = f.h.head();
    let before = f.h.doc();
    let cases: Vec<(Value, &str)> = vec![
        (json!({"to": [1, 1]}), "`from` [x,y] is required"),
        (json!({"from": [1, 1]}), "`to` [x,y] is required"),
        (json!({"from": [28, -3], "to": [28, -3]}), "`from` and `to` must differ"),
        (json!({"from": [28, -3], "to": [28.0 + 1e-12, -3]}), "`from` and `to` must differ"),
        (json!({"from": [null, 0], "to": [1, 1]}), "is not a finite [x,y] point"),
        (json!({"from": [1e13, 0], "to": [1, 1]}), "is not a finite [x,y] point"),
        (json!({"from": [0, 0], "to": [-1e13, 1]}), "is not a finite [x,y] point"),
        (json!({"from": "28,-3", "to": [1, 1]}), "is not a finite [x,y] point"),
        (json!({"from": [1], "to": [1, 1]}), "is not a finite [x,y] point"),
        (json!({"from": [28, -3], "to": [32, 18], "tag": 123}), "`tag` must be a string"),
        (json!({"from": [28, -3], "to": [32, 18], "tag": null}), ""),
        // The shared point parser reads the first two numbers (as `wire` does): no panic either way.
        (json!({"from": [1, 2, 3], "to": [1, 1]}), ""),
        (json!([1, 2]), ""),
        (Value::Null, ""),
    ];
    for (params, msg) in cases {
        let r = f.h.run("cableslash", params.clone());
        if msg.is_empty() {
            // Either refused or (tag null) accepted; never a panic. Restore when accepted.
            if r.is_ok() {
                f.h.run("u", Value::Null).unwrap();
                assert_eq!(f.h.doc(), before);
            }
            continue;
        }
        let e = r.expect_err(&params.to_string());
        assert!(e.contains(msg), "{params}: {e}");
        assert!(!e.to_lowercase().contains("panic"), "{params}: {e}");
    }
    // Text forms serde cannot carry as numbers (NaN, 1e309) through the command line.
    for text in [
        r#"cableslash {"from":[NaN,0],"to":[1,1]}"#,
        r#"cableslash {"from":[1e309,0],"to":[1,1]}"#,
        r#"cableslash {"from":[0,0],"to":[1,1],"tag":"#,
        "cableslash {}",
    ] {
        let _ = f.h.s.cmdline(text);
        if f.h.s.running.is_some() {
            let _ = f.h.s.input(Input::Cancel);
        }
        f.h.check();
    }
    let _ = head;
    assert_eq!(f.h.doc().cables.len(), 0, "no hostile input created a cable");
}

/// AC8: typed hostile points at both prompts are refused with `Point out of range.`; huge but
/// valid coordinates on F never panic; the tag is stored when given.
#[test]
fn ac8_hostile_typed_points_and_huge_valid_coordinates() {
    let mut f = fixture(SEED);
    let head = f.h.head();
    f.h.s.start("cableslash").unwrap();
    for text in ["nan,0", "1e309,0", "1e13,0", "0,-1e13"] {
        let _ = f.h.s.cmdline(text);
        assert!(f.h.s.running.is_some(), "{text}: the prompt repeats");
        assert_eq!(prompt(&f.h), "Specify first point of cable slash:", "{text}");
    }
    click(&mut f.h, 28.0, -3.0);
    for text in ["nan,0", "inf,0", "1e13,0", "@1e13,0"] {
        let _ = f.h.s.cmdline(text);
        assert_eq!(prompt(&f.h), "Specify second point of cable slash or [Undo]:", "{text}");
    }
    let _ = f.h.s.input(Input::Point(Vec2::new(f64::NAN, 1.0)));
    let _ = f.h.s.input(Input::Point(Vec2::new(f64::INFINITY, 1.0)));
    let _ = f.h.s.input(Input::Cancel);
    f.h.check();
    assert_eq!(f.h.head(), head);
    // Huge but valid: across the whole range.
    let r = slash(&mut f.h, (-1e12, -1e12), (1e12, 1e12));
    if let Ok(v) = &r {
        let got = hexes(v, "conductors");
        assert!(got.len() <= 3, "{v}");
    }
    f.h.check();
    // Wires out at ±1e12 and a slash there.
    let mut h = H::new(SEED);
    let a = wire(&mut h, &[(1e12 - 100.0, 1e12 - 1.0), (1e12, 1e12 - 1.0)]);
    let _ = wire(&mut h, &[(-1e12, -1e12), (1e12, 1e12)]);
    let r = slash(&mut h, (1e12 - 50.0, 1e12 - 2.0), (1e12 - 50.0, 1e12 - 0.5));
    if let Ok(v) = r {
        assert!(hexes(&v, "conductors").contains(&a.hex()), "{v}");
    }
    h.check();
    // Tag stored.
    let mut f = fixture(SEED);
    let r = f.h.run("cableslash", json!({"from": [28, -3], "to": [32, 18], "tag": "K1"})).unwrap();
    let k: CableId = id(&r, "cable");
    assert_eq!(f.h.doc().cables[&k].properties.get("tag").map(String::as_str), Some("K1"));
}

/// A reopened project with `n` + 1 parallel unit wires at y = 0 ..= n (x from 0 to 1), made in one
/// transaction to keep the run short: the ops of one real `wire` are copied with fresh IDs and
/// moved up by 1 each, committed to the ledger, saved, and the project reopened so its pages are
/// projected from the history like any opened project. Returns the wires bottom to top.
fn bulk_wires(n: usize) -> (H, Vec<ConductorId>) {
    use ecad_model::{Author, RepresentationId};
    let mut h = H::new(SEED);
    let first = wire(&mut h, &[(0.0, 0.0), (1.0, 0.0)]);
    let template = last_tx(&mut h).ops;
    let mut ops = Vec::new();
    let mut ws = vec![first];
    for i in 1..=n {
        let y = i as f64;
        let base = (0xABCD_u128 << 64) + 4 * i as u128;
        let (c, it, r) = (ConductorId::from(base), ItemId::from(base + 1), RepresentationId::from(base + 2));
        ws.push(c);
        for op in &template {
            ops.push(match op.clone() {
                Op::CreateConductor { conductor, .. } => Op::CreateConductor { id: c, conductor },
                Op::AddItem { mut item, .. } => {
                    item.kind = ItemKind::Primitive(Primitive::Line { a: Pt::new(0.0, y), b: Pt::new(1.0, y) });
                    Op::AddItem { id: it, item }
                }
                Op::LinkRepresentation { mut rep, .. } => {
                    rep.subject = Subject::Conductor(c);
                    rep.items = vec![it];
                    Op::LinkRepresentation { id: r, rep }
                }
                other => panic!("unexpected op in a wire transaction: {other:?}"),
            });
        }
    }
    // The old session's page is not reprojected after this direct commit, so it is saved without
    // the harness checks and only the reopened project is used.
    h.cfg(|p| p.ledger.commit(Author::User, "bulk wires", ops)).unwrap();
    h.s.execute("qsave", &Value::Null).unwrap();
    let medium = h.p(|p| p.store.medium().clone());
    let mut s = Session::empty();
    let project = ProjectHandle::open_from(&mut s, "Big", medium, 5).unwrap();
    let mut h = H { s, project, trail: Vec::new() };
    assert_eq!(h.doc().conductors.len(), n + 1);
    h.check();
    (h, ws)
}

/// AC8 and check L: `MAX_CABLE_MEMBERS` (10 000) crossed wires are accepted; 10 001 are refused with
/// the design's message; neither panics, and the preview caps its rings at 200.
#[test]
#[ignore = "slow: builds a cable at the member cap, about 7 s"]
fn ac8_max_cable_members_is_the_cap() {
    let max = ecad_cadcraft::slash::MAX_CABLE_MEMBERS;
    assert_eq!(max, 10_000);
    let (mut h, _) = bulk_wires(max);
    // Exactly 10 000 (y = 0 ..= 9 999): accepted.
    let head = h.head();
    let r = h.run("cableslash", json!({"from": [0.5, -0.5], "to": [0.5, max as f64 - 0.5]})).unwrap();
    assert_eq!(r["conductors"].as_array().unwrap().len(), max);
    assert_eq!(h.head(), head + 1);
    h.run("u", Value::Null).unwrap();
    // 10 001: refused, head unchanged.
    let head = h.head();
    let e = h.run("cableslash", json!({"from": [0.5, -0.5], "to": [0.5, max as f64 + 0.5]})).unwrap_err();
    assert!(e.contains("The slash crosses more than 10000 wires; nothing was created."), "{e}");
    assert_eq!(h.head(), head);
    // The preview says so too, never panics, and draws at most 200 rings.
    h.s.start("cableslash").unwrap();
    h.s.input(Input::Point(Vec2::new(0.5, -0.5))).unwrap();
    let p = h.s.preview(Vec2::new(0.5, max as f64 + 0.5));
    let text: Vec<String> = p
        .iter()
        .filter_map(|e| match &e.kind {
            cadcraft_engine::doc::EntityKind::Text(t) => Some(t.value.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(text, vec!["-> refused: more than 10000 wires".to_string()]);
    let rings = ring_centres(&p).len();
    assert!(rings <= 200, "rings capped at 200: {rings}");
    // The interactive commit is refused the same way and keeps the first point.
    click(&mut h, 0.5, max as f64 + 0.5);
    assert_eq!(h.head(), head);
    assert_eq!(last_log(&h), "The slash crosses more than 10000 wires; nothing was created.");
    h.s.input(Input::Cancel).unwrap();
    h.check();
}

// ------------------------------------------------------------------ Debugger trail (scope 5)

/// Scope 5 and design §9: the debugger trail from the slash item reaches the representation, the
/// cable, and then the members in member order before the representations.
#[test]
fn scope5_debugger_trail_slash_cable_conductors() {
    use ecad_cadcraft::debug::{ObjectId, used_by};
    let mut f = fixture(SEED);
    let r = slash(&mut f.h, (32.0, 18.0), (28.0, -3.0)).unwrap();
    let k: CableId = id(&r, "cable");
    let d = f.h.doc();
    let rep = d.representations_of(Subject::Cable(k)).next().unwrap().0;
    let related = used_by(&d, ObjectId::Cable(k));
    assert_eq!(
        related,
        vec![
            ObjectId::Conductor(f.w10),
            ObjectId::Conductor(f.w5),
            ObjectId::Conductor(f.w0),
            ObjectId::Representation(rep)
        ]
    );
    // Each conductor lists the cable.
    for w in [f.w0, f.w5, f.w10] {
        assert!(used_by(&d, ObjectId::Conductor(w)).contains(&ObjectId::Cable(k)));
    }
    let _ = Pt::new(0.0, 0.0);
}

// ------------------------------------------------------------------ Round 2 (fix round 1)

/// The members a JSON slash gives, or the error.
fn slash_members(h: &mut H, from: (f64, f64), to: (f64, f64)) -> Result<Vec<String>, String> {
    slash(h, from, to).map(|r| hexes(&r, "conductors"))
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

/// Round 2, item 1 (sort fix in `order`): crossings equal or within the tolerance (1e-6) of each
/// other are ordered by wire ID whichever way the slash is drawn; crossings further apart keep
/// distance order; the same input gives the same order on repeated runs; nothing panics.
#[test]
fn r2_near_tolerance_crossings_order_by_id_and_are_deterministic() {
    // (case, wire heights, expect ID order for the whole set)
    let cases: Vec<(&str, Vec<f64>, bool)> = vec![
        ("identical", vec![5.0, 5.0, 5.0], true),
        ("within tol", vec![5.0, 5.0 + 4e-7, 5.0 - 4e-7], true),
        ("chain each within tol", vec![5.0, 5.0 + 6e-7, 5.0 + 1.2e-6, 5.0 + 1.8e-6], true),
        ("apart", vec![5.0, 5.0 + 1e-3, 5.0 + 2e-3], false),
    ];
    for (case, ys, by_id) in cases {
        let mut runs = Vec::new();
        for _ in 0..3 {
            let mut h = H::new(SEED);
            let ws: Vec<ConductorId> = ys.iter().map(|y| wire(&mut h, &[(0.0, *y), (10.0, *y)])).collect();
            let up = slash_members(&mut h, (5.0, 0.0), (5.0, 10.0)).unwrap();
            h.run("u", Value::Null).unwrap();
            let down = slash_members(&mut h, (5.0, 10.0), (5.0, 0.0)).unwrap();
            let want_up: Vec<String> =
                if by_id { sorted(ws.iter().map(|w| w.hex()).collect()) } else { ws.iter().map(|w| w.hex()).collect() };
            assert_eq!(up, want_up, "{case}: upward");
            if by_id {
                assert_eq!(down, want_up, "{case}: ID order whichever way the slash runs");
            } else {
                assert_eq!(down, want_up.iter().rev().cloned().collect::<Vec<_>>(), "{case}: downward");
            }
            runs.push((up, down));
        }
        assert!(runs.windows(2).all(|w| w[0] == w[1]), "{case}: same order every run");
    }
}

/// Round 2, item 1: many wires through (nearly) one point of the slash, at huge coordinates and
/// next to non-finite input: no panic, ID order, deterministic.
#[test]
fn r2_many_wires_crossing_at_nearly_one_point() {
    for (case, cx, cy, jitter) in
        [("origin exact", 10.0, 10.0, 0.0), ("origin jitter", 10.0, 10.0, 1e-9), ("huge", 1e11, -1e11, 1e-3)]
    {
        let mut runs = Vec::new();
        for _ in 0..2 {
            let mut h = H::new(SEED);
            let mut ws = Vec::new();
            for i in 0..60 {
                // Angles from 5 to 64 degrees, never along the vertical slash.
                let a = (5.0 + i as f64).to_radians();
                let off = jitter * (i % 7) as f64;
                let (dx, dy) = (a.cos() * 5.0, a.sin() * 5.0);
                ws.push(wire(&mut h, &[(cx - dx, cy - dy + off), (cx + dx, cy + dy + off)]));
            }
            let r = slash_members(&mut h, (cx, cy - 20.0), (cx, cy + 20.0));
            // NaN-adjacent: the preview at a non-finite cursor after a valid first point.
            h.s.start("cableslash").unwrap();
            click(&mut h, cx, cy - 20.0);
            let _ = h.s.preview(Vec2::new(f64::NAN, cy));
            let _ = h.s.preview(Vec2::new(cx, f64::INFINITY));
            let _ = h.s.preview(Vec2::new(cx, cy + 1e300));
            h.s.input(Input::Cancel).unwrap();
            h.check();
            let got = r.unwrap_or_else(|e| panic!("{case}: {e}"));
            assert_eq!(sorted(got.clone()), sorted(ws.iter().map(|w| w.hex()).collect()), "{case}: all 60 wires");
            if jitter == 0.0 {
                assert_eq!(got, sorted(got.clone()), "{case}: all at one point, so ID order");
            }
            runs.push(got);
        }
        assert_eq!(runs[0], runs[1], "{case}: deterministic");
    }
}

/// Round 2, item 2 (design §5.2): the preview text is red whenever the slash would be refused,
/// including `-> no wires`, and green when it would be accepted.
#[test]
fn r2_preview_text_is_red_when_refused() {
    use cadcraft_engine::doc::EntityKind;
    use cadcraft_engine::doc::color::Color as CColor;
    let text = |e: &[cadcraft_engine::doc::Entity]| {
        e.iter().find_map(|x| match &x.kind {
            EntityKind::Text(t) => Some((t.value.clone(), x.common.color)),
            _ => None,
        })
    };
    let mut f = fixture(SEED);
    f.h.s.start("cableslash").unwrap();
    click(&mut f.h, 28.0, -3.0);
    let (green, red) = (CColor::Index(3), CColor::Index(1));
    for (to, want, colour) in [
        ((32.0, 18.0), "-> 3 wires, 1 other object ignored", green),
        ((28.5, 2.0), "-> 1 wire", green),
        ((20.0, -10.0), "-> no wires", red),
    ] {
        let p = f.h.s.preview(Vec2::new(to.0, to.1));
        assert_eq!(text(&p), Some((want.to_string(), colour)), "{want}");
    }
    f.h.s.input(Input::Cancel).unwrap();
    // Plain line only: still `no wires`, red.
    f.h.s.start("cableslash").unwrap();
    click(&mut f.h, 10.0, 13.0);
    let p = f.h.s.preview(Vec2::new(10.0, 17.0));
    assert_eq!(text(&p).map(|t| t.1), Some(red), "{:?}", text(&p));
    f.h.s.input(Input::Cancel).unwrap();
    // Blocked: red.
    slash(&mut f.h, (28.0, -3.0), (32.0, 18.0)).unwrap();
    f.h.s.start("cableslash").unwrap();
    click(&mut f.h, 40.0, 3.0);
    let p = f.h.s.preview(Vec2::new(40.0, 7.0));
    assert_eq!(text(&p), Some(("-> refused: 1 wire already in a cable".to_string(), red)));
    f.h.s.input(Input::Cancel).unwrap();
}

/// Round 2, item 3: `cableadd` refuses a wire listed twice, with a clear message, head and document
/// unchanged; also on an existing cable.
#[test]
fn r2_cableadd_refuses_a_wire_listed_twice() {
    let mut f = fixture(SEED);
    let (w0, w5) = (f.w0.hex(), f.w5.hex());
    for params in [
        json!({"conductors": [w0, w0]}),
        json!({"conductors": [w0, w5, w0], "tag": "X"}),
        json!({"conductors": [w5, w5.to_uppercase()]}),
    ] {
        let (head, before) = (f.h.head(), f.h.doc());
        let r = f.h.run("cableadd", params.clone());
        assert_eq!(f.h.head(), head, "{params}");
        assert_eq!(f.h.doc(), before, "{params}");
        let e = r.expect_err(&params.to_string());
        assert!(e.contains("twice") && e.contains("nothing was changed"), "{params}: {e}");
    }
    let r = f.h.run("cableadd", json!({"conductors": [w0], "tag": "K"})).unwrap();
    let k = r["cable"].as_str().unwrap().to_string();
    let (head, before) = (f.h.head(), f.h.doc());
    let e = f.h.run("cableadd", json!({"cable": k, "conductors": [w5, w5]})).unwrap_err();
    assert!(e.contains(&format!("`conductors` lists wire {w5} twice; nothing was changed")), "{e}");
    assert_eq!((f.h.head(), f.h.doc()), (head, before));
    // Adding a member again (already in this cable) is refused too.
    assert!(f.h.run("cableadd", json!({"cable": k, "conductors": [w0]})).is_err());
    assert_eq!(f.h.head(), head);
}

/// Round 2, item 3: the cap holds for the whole cable. A new cable of exactly 10 000 wires is
/// accepted and 10 001 refused; an existing cable of 9 999 takes one more (10 000) but not two, and
/// a full cable takes none. Every refusal leaves head and document unchanged.
#[test]
#[ignore = "slow: builds a cable at the member cap, about 9 s"]
fn r2_cableadd_cap_holds_for_the_whole_cable() {
    let max = ecad_cadcraft::slash::MAX_CABLE_MEMBERS;
    let (mut h, ws) = bulk_wires(max);
    let hx = |r: std::ops::Range<usize>| ws[r].iter().map(|w| w.hex()).collect::<Vec<_>>();
    let refused = |h: &mut H, params: Value, why: &str| {
        let (head, before) = (h.head(), h.doc());
        let r = h.run("cableadd", params);
        assert!(r.is_err(), "{why}: accepted");
        assert_eq!(h.head(), head, "{why}");
        assert_eq!(h.doc(), before, "{why}");
        r.unwrap_err()
    };
    // New cable: exactly max, then max + 1.
    let r = h.run("cableadd", json!({"conductors": hx(0..max)})).unwrap();
    let k: CableId = id(&r, "cable");
    assert_eq!(h.doc().cables[&k].members.len(), max);
    h.run("u", Value::Null).unwrap();
    let e = refused(&mut h, json!({"conductors": hx(0..max + 1)}), "new cable of max + 1");
    assert!(!e.to_lowercase().contains("panic"), "{e}");
    // Existing cable of max - 1.
    let r = h.run("cableadd", json!({"conductors": hx(0..max - 1)})).unwrap();
    let kid: CableId = id(&r, "cable");
    let k = kid.hex();
    let e = refused(&mut h, json!({"cable": k, "conductors": hx(max - 1..max + 1)}), "max - 1 + 2");
    assert_eq!(e, format!("a cable holds at most {max} wires; nothing was changed"));
    h.run("cableadd", json!({"cable": k, "conductors": hx(max - 1..max)})).unwrap();
    assert_eq!(h.doc().cables[&kid].members.len(), max, "exactly max via an existing cable");
    let e = refused(&mut h, json!({"cable": k, "conductors": hx(max..max + 1)}), "full + 1");
    assert_eq!(e, format!("a cable holds at most {max} wires; nothing was changed"));
}
