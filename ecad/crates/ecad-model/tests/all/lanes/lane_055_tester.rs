//! Tester 055 in the model (FP1): a `frame` style record's title block body through apply,
//! inverse, undo, redo, replay, the diff, merge (the whole record is one unit), the invariant (a
//! body only on `frame` records) and the bounds of design 055 §1.3. Written from brief 055 and
//! design 055 (§1.2, §1.3, §12.1 J and K, §14 FP1) before the implementation; the Rust names are
//! the design's (`StyleRecord.body`, `StyleBody::TitleBlock`, `TitleBlockLayout`, `style::Item`,
//! `Rect`, `TextLook`, `IfEmpty`, `Binding`, `Source`, `MAX_ITEMS`, `MAX_LOGO_PRIMS`,
//! `MAX_BODY_TEXT`). Field and variant spellings the design leaves open (`Align`, `Ink`,
//! `LineWeight`, `Newest`, the bound check's name) are the obvious ones and are fixed in phase 2.

use std::collections::BTreeMap;

use ecad_model::diff::ops_between;
use ecad_model::merge::{self, Choice, Inputs, Scope};
use ecad_model::style::{self, Binding, IfEmpty, Rect, Source, StyleBody, StyleLayer, StyleRecord, StyleRef, TextLook, TitleBlockLayout};
use ecad_model::tag::HighWater;
use ecad_model::{Author, Document, Ledger, Op};

fn look(size: f64) -> TextLook {
    TextLook { size, bold: false, align: style::Align::Left, colour: style::Ink::Ink }
}

fn r(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect { x, y, w, h }
}

fn field(source: Source, key: &str, at: Rect) -> style::Item {
    style::Item::Field { at, bind: Binding { source, key: key.into() }, label: Some("Drawing no.".into()), look: look(3.5), if_empty: IfEmpty::Blank }
}

/// A small block: a line, a static text, a field, a revision table and an empty logo.
fn layout() -> TitleBlockLayout {
    TitleBlockLayout {
        size: (180.0, 40.0),
        items: vec![
            style::Item::Line { a: (0.0, 20.0), b: (180.0, 20.0), weight: style::LineWeight::Thin },
            style::Item::Text { at: r(2.0, 30.0, 40.0, 8.0), text: "Drawing".into(), look: look(1.8) },
            field(Source::Project, "drawing_number", r(2.0, 22.0, 70.0, 8.0)),
            style::Item::RevTable { at: r(80.0, 0.0, 100.0, 20.0), rows: 4, header: true, newest: style::Newest::Top },
            style::Item::Logo { at: r(2.0, 2.0, 30.0, 15.0), name: "logo".into(), prims: Vec::new() },
        ],
    }
}

fn frame_rec(name: &str, body: Option<TitleBlockLayout>) -> StyleRecord {
    StyleRecord { name: name.into(), fields: BTreeMap::new(), body: body.map(StyleBody::TitleBlock) }
}

fn set_frame(id: StyleRef, before: Option<StyleRecord>, after: Option<StyleRecord>) -> Op {
    Op::SetStyle { layer: StyleLayer::Project, catalogue: "frame".into(), id, before, after }
}

fn own(n: u128) -> StyleRef {
    StyleRef::Own(n)
}

/// FP1: a record with a body applies, inverts exactly, undoes, redoes and replays; the model's
/// invariant holds.
#[test]
#[ignore = "tester 055: awaiting build"]
fn fp1_a_frame_body_applies_inverts_and_replays() {
    let mut l = Ledger::new(55);
    let empty = l.document().clone();
    let op = set_frame(own(1), None, Some(frame_rec("A3 Northfield", Some(layout()))));
    assert_eq!(op.inverse().inverse(), op, "inverse twice is the op");
    l.commit(Author::User, "new", vec![op]).unwrap();
    let doc = l.document().clone();
    let stored = doc.styles.get("frame").and_then(|m| m.get(&own(1))).expect("stored");
    assert_eq!(stored.body.as_ref(), Some(&StyleBody::TitleBlock(layout())));
    assert!(doc.check().is_ok());
    // Move the field: a whole-record SetStyle.
    let mut moved = layout();
    if let style::Item::Field { at, .. } = &mut moved.items[2] {
        at.x += 10.0;
    }
    l.commit(
        Author::User,
        "move",
        vec![set_frame(own(1), Some(frame_rec("A3 Northfield", Some(layout()))), Some(frame_rec("A3 Northfield", Some(moved))))],
    )
    .unwrap();
    let after = l.document().clone();
    l.undo(Author::User).unwrap();
    assert_eq!(*l.document(), doc, "undo restores the old body");
    l.redo(Author::User).unwrap();
    assert_eq!(*l.document(), after);
    let replayed = Ledger::replay(l.full_log().unwrap(), 55).unwrap();
    assert_eq!(*replayed.document(), after, "replay gives the same body");
    l.undo(Author::User).unwrap();
    l.undo(Author::User).unwrap();
    assert_eq!(*l.document(), empty);
}

/// FP1: `body: None` is omitted from the serialised record, so a frame record without a body
/// serialises exactly as 050a's did (older files unchanged).
#[test]
#[ignore = "tester 055: awaiting build"]
fn fp1_no_body_serialises_as_050a_did() {
    let rec = StyleRecord { name: "Heavy".into(), fields: [("border_weight".to_string(), "1".to_string())].into_iter().collect(), body: None };
    let v = serde_json::to_value(&rec).unwrap();
    assert_eq!(v, serde_json::json!({"name": "Heavy", "fields": {"border_weight": "1"}}), "no `body` key when None");
    let back: StyleRecord = serde_json::from_value(serde_json::json!({"name": "Heavy", "fields": {"border_weight": "1"}})).unwrap();
    assert_eq!(back, rec, "a 050a record reads with body None");
    let with = frame_rec("A3", Some(layout()));
    let back: StyleRecord = serde_json::from_value(serde_json::to_value(&with).unwrap()).unwrap();
    assert_eq!(back, with, "a body round-trips through serde");
}

/// FP1: an unknown body tag (a newer file) is kept raw and ignored: it reads, serialises back
/// unchanged, and is not a title block.
#[test]
#[ignore = "tester 055: awaiting build"]
fn fp1_an_unknown_body_tag_is_kept_raw() {
    let raw = serde_json::json!({"name": "Newer", "body": {"pattern_sheet": {"stripes": 3}}});
    let rec: StyleRecord = serde_json::from_value(raw.clone()).expect("an unknown body tag still reads");
    assert!(!matches!(rec.body, Some(StyleBody::TitleBlock(_))), "not a title block");
    assert_eq!(serde_json::to_value(&rec).unwrap(), raw, "kept raw: written back unchanged");
}

/// Design §0.3 invariant: a body only on `frame` records. A command never writes one elsewhere;
/// the model says so when one is applied to a `text` record.
#[test]
#[ignore = "tester 055: awaiting build"]
fn invariant_a_body_only_on_frame_records() {
    let mut l = Ledger::new(55);
    let op = Op::SetStyle {
        layer: StyleLayer::Project,
        catalogue: "text".into(),
        id: StyleRef::Builtin("tag".into()),
        before: None,
        after: Some(frame_rec("", Some(layout()))),
    };
    let e = l.commit(Author::User, "x", vec![op]).err();
    let doc = l.document().clone();
    // Either apply refuses it (preferred) or the invariant reports it; never silently accepted.
    assert!(e.is_some() || doc.check().is_err(), "a body on a text record is refused or reported");
}

/// §1.3: every bound with its value, and a breach just over each is reported by the layout's
/// check (commands refuse it); a layout at each bound passes.
#[test]
#[ignore = "tester 055: awaiting build"]
fn bounds_each_bound_passes_at_its_value_and_fails_just_over() {
    assert_eq!(style::MAX_ITEMS, 512);
    assert_eq!(style::MAX_BODY_TEXT, 256);
    assert_eq!(style::MAX_LOGO_PRIMS, 2_000);
    assert!(layout().check().is_ok(), "the small block is in bounds");
    let at_bound = |n: usize| TitleBlockLayout { size: (180.0, 40.0), items: vec![field(Source::Page, "title", r(1.0, 1.0, 10.0, 5.0)); n] };
    assert!(at_bound(512).check().is_ok(), "512 items");
    assert!(at_bound(513).check().is_err(), "513 items");
    let text = |n: usize| TitleBlockLayout {
        size: (180.0, 40.0),
        items: vec![style::Item::Text { at: r(1.0, 1.0, 10.0, 5.0), text: "x".repeat(n), look: look(2.5) }],
    };
    assert!(text(256).check().is_ok() && text(257).check().is_err(), "256 characters per text");
    let key = |n: usize| TitleBlockLayout { size: (180.0, 40.0), items: vec![field(Source::Project, &"k".repeat(n), r(1.0, 1.0, 10.0, 5.0))] };
    assert!(key(256).check().is_ok() && key(257).check().is_err(), "256 characters per binding key");
    for (w, h, ok) in [(1.0, 1.0, true), (2400.0, 400.0, true), (0.5, 40.0, false), (2400.1, 40.0, false), (180.0, 400.1, false), (180.0, 0.0, false)]
    {
        let b = TitleBlockLayout { size: (w, h), items: Vec::new() };
        assert_eq!(b.check().is_ok(), ok, "size {w} × {h}");
    }
    for (x, ok) in [(0.0, true), (2400.0, true), (-0.1, false), (2400.1, false), (f64::NAN, false), (f64::INFINITY, false)] {
        let b = TitleBlockLayout { size: (180.0, 40.0), items: vec![field(Source::Page, "title", r(x, 1.0, 10.0, 5.0))] };
        assert_eq!(b.check().is_ok(), ok, "x = {x}");
    }
    let tiny = TitleBlockLayout { size: (180.0, 40.0), items: vec![field(Source::Page, "title", r(1.0, 1.0, 0.4, 5.0))] };
    assert!(tiny.check().is_err(), "sizes ≥ 0.5 mm");
    for (rows, ok) in [(1u8, true), (20, true), (0, false), (21, false)] {
        let b = TitleBlockLayout {
            size: (180.0, 40.0),
            items: vec![style::Item::RevTable { at: r(1.0, 1.0, 100.0, 20.0), rows, header: true, newest: style::Newest::Top }],
        };
        assert_eq!(b.check().is_ok(), ok, "rows {rows}");
    }
}

/// §1.3: a logo of 2 001 primitives, and a body of 4 001 logo primitives, fail the check.
#[test]
#[ignore = "tester 055: awaiting build"]
fn bounds_logo_primitives_per_logo_and_per_body() {
    let prim = ecad_model::Primitive::Line { a: ecad_model::Pt::new(0.0, 0.0), b: ecad_model::Pt::new(1.0, 1.0) };
    let logo = |n: usize| style::Item::Logo { at: r(1.0, 1.0, 30.0, 15.0), name: "l".into(), prims: vec![prim.clone(); n] };
    let one = |n: usize| TitleBlockLayout { size: (180.0, 40.0), items: vec![logo(n)] };
    assert!(one(2_000).check().is_ok() && one(2_001).check().is_err(), "2 000 per logo");
    let two = TitleBlockLayout { size: (180.0, 40.0), items: vec![logo(2_000), logo(2_000)] };
    assert!(two.check().is_ok(), "4 000 in a body");
    let three = TitleBlockLayout { size: (180.0, 40.0), items: vec![logo(2_000), logo(2_000), logo(1)] };
    assert!(three.check().is_err(), "4 001 in a body");
}

/// §12.1 J (model side): applying a body over a bound never panics (the file layer opens it; the
/// drawing drops the excess). 10 000 items and NaN coordinates go through apply, the invariant,
/// clone, equality and serde without a panic.
#[test]
#[ignore = "tester 055: awaiting build"]
fn hostile_bodies_never_panic_in_the_model() {
    let mut big = layout();
    big.items = vec![field(Source::Page, "title", r(f64::NAN, f64::INFINITY, -1.0, 1e308)); 10_000];
    let mut l = Ledger::new(55);
    let _ = l.commit(Author::User, "hostile", vec![set_frame(own(9), None, Some(frame_rec("Hostile", Some(big.clone()))))]);
    let _ = l.document().check();
    let _ = serde_json::to_string(&frame_rec("Hostile", Some(big)));
}

// ------------------------------------------------------------------------- diff and merge

fn run_merge(a: &Document, t: &Document, s: &Document, scope: &Scope, choices: &BTreeMap<String, Choice>) -> merge::Plan {
    let high = HighWater::default();
    let inp = Inputs {
        ancestor: a,
        target: t,
        source: s,
        scope,
        target_name: "main",
        source_name: "branch",
        keep_project_key: &|_| true,
        high: &high,
        edit_rule: &|_, _, v| Ok(v.to_string()),
        join: &|_, _, _| Err("no".into()),
    };
    merge::plan(&inp, choices)
}

fn moved(dx: f64) -> TitleBlockLayout {
    let mut b = layout();
    if let style::Item::Field { at, .. } = &mut b.items[2] {
        at.x += dx;
    }
    b
}

/// §0.3, FP1: the diff carries a body change as one SetStyle that round-trips.
#[test]
#[ignore = "tester 055: awaiting build"]
fn k_the_diff_round_trips_a_body_change() {
    let mut l = Ledger::new(55);
    l.commit(Author::User, "new", vec![set_frame(own(1), None, Some(frame_rec("A3 Northfield", Some(layout()))))]).unwrap();
    let from = l.document().clone();
    l.commit(
        Author::User,
        "move",
        vec![set_frame(own(1), Some(frame_rec("A3 Northfield", Some(layout()))), Some(frame_rec("A3 Northfield", Some(moved(10.0)))))],
    )
    .unwrap();
    let to = l.document().clone();
    let ops = ops_between(&from, &to, &|_| true);
    assert_eq!(ops.iter().map(Op::name).collect::<Vec<_>>(), ["SetStyle"], "{ops:?}");
    let mut d = from.clone();
    for op in &ops {
        op.apply(&mut d).unwrap();
    }
    assert_eq!(d, to);
}

/// §12.1 K: the same frame edited on both branches (one moves a field, the other changes a text)
/// is one conflict worded with the frame's name; edits to different frames merge cleanly; every
/// choice gives a valid document.
#[test]
#[ignore = "tester 055: awaiting build"]
fn k_a_frames_record_is_one_merge_unit() {
    let mut base = Ledger::new(55);
    base.commit(
        Author::User,
        "base",
        vec![
            set_frame(own(1), None, Some(frame_rec("A3 Northfield", Some(layout())))),
            set_frame(own(2), None, Some(frame_rec("A2 Northfield", Some(layout())))),
        ],
    )
    .unwrap();
    let a = base.document().clone();
    let (mut t, mut s) = (base.clone(), base.clone());
    let mut texted = layout();
    if let style::Item::Text { text, .. } = &mut texted.items[1] {
        *text = "Drawing no.".into();
    }
    let t_ops = vec![set_frame(own(1), Some(frame_rec("A3 Northfield", Some(layout()))), Some(frame_rec("A3 Northfield", Some(moved(10.0)))))];
    let s_ops = vec![
        set_frame(own(1), Some(frame_rec("A3 Northfield", Some(layout()))), Some(frame_rec("A3 Northfield", Some(texted.clone())))),
        set_frame(own(2), Some(frame_rec("A2 Northfield", Some(layout()))), Some(frame_rec("A2 Northfield", Some(moved(5.0))))),
    ];
    t.commit(Author::User, "t", t_ops.clone()).unwrap();
    s.commit(Author::User, "s", s_ops.clone()).unwrap();
    let mut scope = Scope::default();
    scope.add_ops(&t_ops);
    scope.add_ops(&s_ops);
    let plan = run_merge(&a, t.document(), s.document(), &scope, &BTreeMap::new());
    assert_eq!(plan.conflicts.len(), 1, "different items of one frame are still one unit: {:?}", plan.conflicts);
    assert!(plan.conflicts[0].what.contains("A3 Northfield"), "worded with the frame's name: {}", plan.conflicts[0].what);
    let id = plan.conflicts[0].id.clone();
    for (choice, want) in [(Choice::Source, texted), (Choice::Target, moved(10.0))] {
        let plan = run_merge(&a, t.document(), s.document(), &scope, &BTreeMap::from([(id.clone(), choice)]));
        assert!(plan.unanswered.is_empty() && plan.problem.is_none(), "{:?} {:?}", plan.unanswered, plan.problem);
        let mut m = t.clone();
        m.commit(Author::User, "merge", plan.ops).unwrap();
        let d = m.document();
        let body = |n: u128| d.styles.get("frame").and_then(|x| x.get(&own(n))).and_then(|r| r.body.clone());
        assert_eq!(body(1), Some(StyleBody::TitleBlock(want)), "the chosen side's whole record");
        assert_eq!(body(2), Some(StyleBody::TitleBlock(moved(5.0))), "the other frame merged cleanly");
        assert!(d.check().is_ok());
    }
}
