#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;

use crate::document::op::Op;
use crate::naming::id::{ConnectionPointId, DeviceId, PageId, WireId};
use crate::naming::tag::{self, HighWater};
use crate::{ConnectionPoint, Device, Document, End, Page, Wire};

use super::apply::*;
use super::state::*;
use crate::naming::id::{ConductorId, NetId};

fn doc_with(tags: &[(u128, &str)]) -> Document {
    let mut d = Document::default();
    for (id, t) in tags {
        // `-K?1` is a provisional tag, stored bare.
        let device = match t.split_once('?') {
            Some((a, b)) => Device::default().with_tag(Some(&format!("{a}{b}")), tag::TagState::Provisional),
            None => Device::default().with_tag(Some(t), tag::TagState::Fixed),
        };
        d.devices.insert(DeviceId(*id), device);
    }
    d.rebuild_index();
    d
}

fn run(a: &Document, t: &Document, s: &Document, scope: &Scope, choices: &BTreeMap<String, Choice>) -> Plan {
    let all = |_: &str| true;
    let high = HighWater::new();
    let rule = |_: Obj, _: &str, v: &str| if v.trim().is_empty() { Err("empty".to_string()) } else { Ok(v.trim().to_string()) };
    // Two devices described "coil" cannot be one (a stand-in for the engine's rules).
    let join = |d: &Document, from: DeviceId, into: DeviceId| -> Result<Vec<Op>, String> {
        let coil = |c: DeviceId| d.devices.get(&c).and_then(|x| x.description.as_ref()).is_some_and(|v| v.as_str() == "coil");
        if coil(from) && coil(into) {
            return Err("already has a coil.".into());
        }
        let comp = d.devices.get(&from).cloned().ok_or("no such device")?;
        Ok(vec![Op::DeleteDevice { id: from, device: comp }])
    };
    let inp = Inputs {
        ancestor: a,
        target: t,
        source: s,
        scope,
        target_name: "main",
        source_name: "\"B\"",
        keep_project_key: &all,
        high: &high,
        edit_rule: &rule,
        join: &join,
    };
    plan(&inp, choices)
}

fn scope_of(ids: &[u128]) -> Scope {
    let mut s = Scope::default();
    for i in ids {
        s.touched.devices.insert(DeviceId(*i));
    }
    s
}

#[test]
fn one_sided_changes_merge_and_two_sided_ones_conflict() {
    let a = doc_with(&[(1, "-K01"), (2, "-K02")]);
    let mut t = a.clone();
    t.devices.get_mut(&DeviceId(1)).unwrap().properties.insert("remark".into(), "16 A".into());
    let mut s = a.clone();
    s.devices.get_mut(&DeviceId(1)).unwrap().properties.insert("remark".into(), "20 A".into());
    s.devices.get_mut(&DeviceId(2)).unwrap().description = crate::objects::devices::Description::new("Pump").ok();
    let scope = scope_of(&[1, 2]);
    let p = run(&a, &t, &s, &scope, &BTreeMap::new());
    assert_eq!(p.conflicts.len(), 1);
    let id = p.conflicts[0].id.clone();
    assert!(id.ends_with("/prop.remark"), "{id}");
    assert!(p.ops.is_empty());
    assert_eq!(p.unanswered, vec![id.clone()]);
    let mut ch = BTreeMap::new();
    ch.insert(id.clone(), Choice::Source);
    let p = run(&a, &t, &s, &scope, &ch);
    assert!(p.unanswered.is_empty());
    let mut doc = t.clone();
    for op in &p.ops {
        op.apply(&mut doc).unwrap();
    }
    assert_eq!(doc.devices[&DeviceId(1)].properties["remark"], "20 A");
    assert_eq!(doc.devices[&DeviceId(2)].text(crate::TextField::Description), Some("Pump"));
    ch.insert(id, Choice::Value("25 A".into()));
    let p = run(&a, &t, &s, &scope, &ch);
    let mut doc = t.clone();
    for op in &p.ops {
        op.apply(&mut doc).unwrap();
    }
    assert_eq!(doc.devices[&DeviceId(1)].properties["remark"], "25 A");
}

#[test]
fn a_tag_added_on_both_sides_is_renumbered() {
    let a = doc_with(&[]);
    let t = doc_with(&[(1, "-K05")]);
    let s = doc_with(&[(2, "-K05")]);
    let scope = scope_of(&[1, 2]);
    let p = run(&a, &t, &s, &scope, &BTreeMap::new());
    let c = p.conflicts.iter().find(|c| c.kind == ConflictKind::Tag).expect("a tag conflict");
    assert_eq!(c.id, "rule/tag/K5");
    let mut ch = BTreeMap::new();
    ch.insert(c.id.clone(), Choice::RenumberSource);
    let p = run(&a, &t, &s, &scope, &ch);
    assert!(p.unanswered.is_empty(), "{:?}", p.unanswered);
    let mut doc = t.clone();
    for op in &p.ops {
        op.apply(&mut doc).unwrap();
    }
    assert_eq!(doc.devices[&DeviceId(1)].tag().unwrap(), "-K05");
    assert_eq!(doc.devices[&DeviceId(2)].tag().unwrap(), "-K06");
}

#[test]
fn a_tag_added_on_both_sides_can_be_made_one() {
    let a = doc_with(&[]);
    let t = doc_with(&[(1, "-K05")]);
    let s = doc_with(&[(2, "-K05")]);
    let scope = scope_of(&[1, 2]);
    let p = run(&a, &t, &s, &scope, &BTreeMap::new());
    let c = p.conflicts.iter().find(|c| c.kind == ConflictKind::Tag).expect("a tag conflict");
    assert!(c.choices.contains(&Choice::MakeOne));
    let p = run(&a, &t, &s, &scope, &BTreeMap::from([(c.id.clone(), Choice::MakeOne)]));
    assert!(p.problem.is_none() && p.unanswered.is_empty(), "{:?}", p.problem);
    let mut doc = t.clone();
    for op in &p.ops {
        op.apply(&mut doc).unwrap();
    }
    assert!(doc.devices.contains_key(&DeviceId(1)) && !doc.devices.contains_key(&DeviceId(2)));
    // Two coils cannot be one: the choice is left out, with the reason; renumbering still works.
    let mut t2 = doc_with(&[(1, "-K05")]);
    let mut s2 = doc_with(&[(2, "-K05")]);
    for (d, id) in [(&mut t2, 1), (&mut s2, 2)] {
        d.devices.get_mut(&DeviceId(id)).unwrap().description = crate::objects::devices::Description::new("coil").ok();
    }
    let p = run(&a, &t2, &s2, &scope, &BTreeMap::new());
    let c = p.conflicts.iter().find(|c| c.kind == ConflictKind::Tag).unwrap();
    assert!(!c.choices.contains(&Choice::MakeOne));
    assert_eq!(p.unavailable.get(&c.id).map(String::as_str), Some("already has a coil."));
    let p = run(&a, &t2, &s2, &scope, &BTreeMap::from([(c.id.clone(), Choice::MakeOne)]));
    assert!(p.ops.is_empty() && p.problem.is_some());
    let p = run(&a, &t2, &s2, &scope, &BTreeMap::from([(c.id.clone(), Choice::RenumberSource)]));
    let mut doc = t2.clone();
    for op in &p.ops {
        op.apply(&mut doc).unwrap();
    }
    assert_eq!(doc.devices[&DeviceId(2)].tag().unwrap(), "-K06");
}

#[test]
fn provisional_duplicates_are_not_conflicts() {
    let a = doc_with(&[]);
    let t = doc_with(&[(1, "-K?1")]);
    let s = doc_with(&[(2, "-K?1")]);
    let p = run(&a, &t, &s, &scope_of(&[1, 2]), &BTreeMap::new());
    assert!(p.conflicts.is_empty(), "{:?}", p.conflicts);
    assert_eq!(p.provisional_renumbered, 1);
    let mut doc = t.clone();
    for op in &p.ops {
        op.apply(&mut doc).unwrap();
    }
    assert_eq!(doc.devices[&DeviceId(2)].tag_shown().unwrap(), "-K?2");
}

#[test]
fn removed_on_one_side_and_changed_on_the_other_offers_both() {
    let a = doc_with(&[(1, "-K01")]);
    let t = doc_with(&[]);
    let mut s = a.clone();
    s.devices.get_mut(&DeviceId(1)).unwrap().description = crate::objects::devices::Description::new("x").ok();
    let scope = scope_of(&[1]);
    let p = run(&a, &t, &s, &scope, &BTreeMap::new());
    assert_eq!(p.conflicts[0].kind, ConflictKind::Removed);
    let mut ch = BTreeMap::new();
    ch.insert(p.conflicts[0].id.clone(), Choice::Source);
    let p = run(&a, &t, &s, &scope, &ch);
    let mut doc = t.clone();
    for op in &p.ops {
        op.apply(&mut doc).unwrap();
    }
    assert!(doc.devices.contains_key(&DeviceId(1)));
}

// ---- each conflict kind with each of its answers

fn applied(t: &Document, p: &Plan) -> Document {
    assert!(p.problem.is_none() && p.unanswered.is_empty(), "{:?} {:?}", p.problem, p.unanswered);
    let mut doc = t.clone();
    for op in &p.ops {
        op.apply(&mut doc).unwrap();
    }
    doc
}

fn one(id: &str, c: Choice) -> BTreeMap<String, Choice> {
    BTreeMap::from([(id.to_string(), c)])
}

fn device(d: &mut Document, c: u128, tag_text: &str, connection_points: &[u128]) {
    d.devices.insert(DeviceId(c), Device::default().with_tag(Some(tag_text), tag::TagState::Fixed));
    for t in connection_points {
        d.connection_points.insert(
            ConnectionPointId(*t),
            ConnectionPoint { owner: DeviceId(c).into(), label: Some(format!("A{t}")), key: None, accepts: Default::default() },
        );
    }
}

fn wire(d: &mut Document, w: u128, a: u128, b: u128) {
    d.wires.insert(WireId(w), Wire::new(NetId(90), ConductorId(91), ConnectionPointId(a), ConnectionPointId(b)));
}

fn scope_all(ids: &[Obj]) -> Scope {
    let mut s = Scope::default();
    for o in ids {
        match *o {
            Obj::Device(c) => s.touched.devices.insert(c),
            Obj::ConnectionPoint(t) => s.touched.connection_points.insert(t),
            Obj::Wire(c) => s.touched.wires.insert(c),
            Obj::Page(p) => s.touched.pages.insert(p),
            _ => false,
        };
    }
    s
}

#[test]
fn a_value_conflict_keeps_main_and_an_empty_or_refused_edit_is_a_problem() {
    let a = doc_with(&[(1, "-K01")]);
    let mut t = a.clone();
    t.devices.get_mut(&DeviceId(1)).unwrap().properties.insert("remark".into(), "16 A".into());
    let mut s = a.clone();
    s.devices.get_mut(&DeviceId(1)).unwrap().properties.insert("remark".into(), "20 A".into());
    let scope = scope_of(&[1]);
    let id = format!("{}/prop.remark", DeviceId(1).hex());
    let p = run(&a, &t, &s, &scope, &one(&id, Choice::Target));
    assert_eq!(applied(&t, &p).devices[&DeviceId(1)].properties["remark"], "16 A");
    let p = run(&a, &t, &s, &scope, &one(&id, Choice::Value("  ".into())));
    assert!(p.problem.as_deref().is_some_and(|m| m.to_lowercase().contains("remark")), "{:?}", p.problem);
    assert!(p.ops.is_empty());
}

#[test]
fn removed_on_main_and_kept_from_the_branch_brings_back_its_connection_points() {
    let mut a = Document::default();
    device(&mut a, 1, "-K01", &[11, 12]);
    let t = Document::default();
    let mut s = a.clone();
    s.devices.get_mut(&DeviceId(1)).unwrap().description = crate::objects::devices::Description::new("Pump").ok();
    let scope = scope_all(&[Obj::Device(DeviceId(1)), Obj::ConnectionPoint(ConnectionPointId(11)), Obj::ConnectionPoint(ConnectionPointId(12))]);
    let p = run(&a, &t, &s, &scope, &BTreeMap::new());
    let k = p.conflicts.iter().find(|c| c.kind == ConflictKind::Removed).unwrap();
    assert!(k.source.contains("connection points"), "{}", k.source);
    let id = k.id.clone();
    // Keep removed: nothing comes back.
    let doc = applied(&t, &run(&a, &t, &s, &scope, &one(&id, Choice::Target)));
    assert!(doc.devices.is_empty() && doc.connection_points.is_empty());
    // Keep the device: its connection points come back with it.
    let doc = applied(&t, &run(&a, &t, &s, &scope, &one(&id, Choice::Source)));
    assert!(doc.devices.contains_key(&DeviceId(1)));
    assert_eq!(doc.connection_points.len(), 2);
}

#[test]
fn a_wire_ends_conflict_offers_main_and_branch() {
    let mut a = Document::default();
    device(&mut a, 1, "-K01", &[11, 12, 13, 14]);
    wire(&mut a, 5, 11, 12);
    let mut t = a.clone();
    wire(&mut t, 5, 11, 13);
    let mut s = a.clone();
    wire(&mut s, 5, 11, 14);
    let scope = scope_all(&[Obj::Wire(WireId(5))]);
    let id = format!("{}/ends", WireId(5).hex());
    let p = run(&a, &t, &s, &scope, &BTreeMap::new());
    assert_eq!(p.conflicts[0].id, id);
    assert_eq!(p.conflicts[0].choices, vec![Choice::Target, Choice::Source]);
    for (c, want) in [(Choice::Target, 13), (Choice::Source, 14)] {
        let doc = applied(&t, &run(&a, &t, &s, &scope, &one(&id, c.clone())));
        assert_eq!(doc.wires[&WireId(5)].end(End::B), ConnectionPointId(want), "{c:?}");
    }
}

#[test]
fn a_point_the_branch_uses_and_main_removed_is_brought_back_or_its_wire_removed() {
    let mut a = Document::default();
    device(&mut a, 1, "-K01", &[11]);
    device(&mut a, 2, "-K02", &[21]);
    let mut s = a.clone();
    wire(&mut s, 5, 21, 11);
    let mut t = a.clone();
    t.connection_points.remove(&ConnectionPointId(11));
    t.devices.remove(&DeviceId(1));
    let scope = scope_all(&[Obj::Wire(WireId(5)), Obj::ConnectionPoint(ConnectionPointId(11)), Obj::Device(DeviceId(1))]);
    let tid = format!("{}/needed", ConnectionPointId(11).hex());
    let cid = format!("{}/needed", DeviceId(1).hex());
    // Not brought back: a wire runs between two points, so without one it goes.
    let doc = applied(&t, &run(&a, &t, &s, &scope, &one(&tid, Choice::Drop)));
    assert!(!doc.wires.contains_key(&WireId(5)));
    assert!(!doc.connection_points.contains_key(&ConnectionPointId(11)));
    // Bring back the point and its device.
    let both = BTreeMap::from([(tid.clone(), Choice::BringBack), (cid.clone(), Choice::BringBack)]);
    let doc = applied(&t, &run(&a, &t, &s, &scope, &both));
    assert!(doc.connection_points.contains_key(&ConnectionPointId(11)) && doc.devices.contains_key(&DeviceId(1)));
    // Bring back the point but not what it needs: a problem with the way through, no ops.
    let contradiction = BTreeMap::from([(tid, Choice::BringBack), (cid, Choice::Drop)]);
    let p = run(&a, &t, &s, &scope, &contradiction);
    assert!(p.problem.as_deref().is_some_and(|m| m.contains("first")), "{:?}", p.problem);
    assert!(p.ops.is_empty());
}

#[test]
fn a_duplicate_tag_renumbers_main_s_device_when_asked() {
    let a = doc_with(&[]);
    let t = doc_with(&[(1, "-K05")]);
    let s = doc_with(&[(2, "-K05")]);
    let p = run(&a, &t, &s, &scope_of(&[1, 2]), &one("rule/tag/K5", Choice::RenumberTarget));
    let doc = applied(&t, &p);
    assert_eq!(doc.devices[&DeviceId(1)].tag().unwrap(), "-K06");
    assert_eq!(doc.devices[&DeviceId(2)].tag().unwrap(), "-K05");
}

#[test]
fn a_page_number_added_on_both_is_renumbered_either_way() {
    let page = |n: &str| {
        let mut p = Page::titled("x").unwrap();
        p.number = crate::document::book::PageNo::parse(n);
        p
    };
    let a = Document::default();
    let mut t = a.clone();
    t.pages.insert(PageId(1), page("5"));
    t.page_order.push(PageId(1));
    let mut s = a.clone();
    s.pages.insert(PageId(2), page("5"));
    s.page_order.push(PageId(2));
    let mut scope = scope_all(&[Obj::Page(PageId(1)), Obj::Page(PageId(2))]);
    scope.touched.page_order = true;
    let p = run(&a, &t, &s, &scope, &BTreeMap::new());
    let id = p.conflicts.iter().find(|c| c.kind == ConflictKind::PageNumber).map(|c| c.id.clone()).unwrap();
    for (c, renumbered) in [(Choice::RenumberSource, PageId(2)), (Choice::RenumberTarget, PageId(1))] {
        let doc = applied(&t, &run(&a, &t, &s, &scope, &one(&id, c)));
        assert_eq!(doc.pages[&renumbered].number.as_ref().map(crate::document::book::PageNo::as_str), Some("6"));
        assert_eq!(doc.page_order.len(), 2);
    }
}

#[test]
fn page_order_changed_differently_on_both_sides_is_chosen() {
    let mut a = Document::default();
    for i in 1..=3u128 {
        a.pages.insert(PageId(i), Page::titled(format!("{i}")).unwrap());
        a.page_order.push(PageId(i));
    }
    let mut t = a.clone();
    t.page_order = vec![PageId(3), PageId(1), PageId(2)];
    let mut s = a.clone();
    s.page_order = vec![PageId(2), PageId(1), PageId(3)];
    let mut scope = Scope::default();
    scope.touched.page_order = true;
    for (c, want) in [(Choice::Target, t.page_order.clone()), (Choice::Source, s.page_order.clone())] {
        let doc = applied(&t, &run(&a, &t, &s, &scope, &one("rule/pageorder", c)));
        assert_eq!(doc.page_order, want);
    }
}

#[test]
fn an_answer_for_a_conflict_not_raised_is_reported_as_ignored() {
    let a = doc_with(&[(1, "-K01")]);
    let mut s = a.clone();
    s.devices.get_mut(&DeviceId(1)).unwrap().description = crate::objects::devices::Description::new("Pump").ok();
    let p = run(&a, &a, &s, &scope_of(&[1]), &one(&format!("{}/prop.rating", DeviceId(1).hex()), Choice::Source));
    assert_eq!(p.ignored.len(), 1, "{:?}", p.ignored);
    assert!(p.ignored[0].starts_with("-K01"), "{:?}", p.ignored);
    assert!(!p.ops.is_empty(), "the merge itself goes ahead");
}
