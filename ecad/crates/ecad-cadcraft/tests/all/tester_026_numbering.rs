//! Tester 026 (numbering): probes that try to break tag uniqueness, fixed tags, the high-water
//! mark (FP4), provisional tags and Finalise, Issue Revision, and save / reopen of old projects.
//! A probe that found a bug is `#[ignore = "tester 026: <bug>"]` and fails until it is fixed.

use crate::common;

use std::collections::{BTreeMap, BTreeSet};

use cadcraft_engine::Session;
use common::{H, id};
use ecad_cadcraft::{Persistence, ProjectHandle};
use ecad_model::sheet::Grid;
use ecad_model::{Author, CableId, ComponentId, ObjectRef, Op, PageId, tag};
use serde_json::{Value, json};

fn cell(h: &mut H, page: PageId, col: u32, row: u32) -> [f64; 2] {
    h.p(|p| {
        let g = Grid::of(p.ledger.document(), page);
        [g.frame_left() + (f64::from(col) + 0.5) * g.column_width(), g.frame_top() - (f64::from(row) + 0.5) * g.row_height()]
    })
}

fn coil(h: &mut H, at: [f64; 2], tag: Option<&str>) -> Result<ComponentId, String> {
    let mut p = json!({"symbol": "coil", "at": at});
    if let Some(t) = tag {
        p["tag"] = json!(t);
    }
    h.run("place", p).map(|v| id(&v, "component"))
}

fn tag_of(h: &mut H, c: ComponentId) -> Option<String> {
    h.doc().components.get(&c).and_then(|x| x.properties.get("tag").cloned())
}

fn fixed(h: &mut H, c: ComponentId) -> bool {
    h.doc().components.get(&c).is_some_and(|x| x.properties.get("tag_fixed").is_some_and(|v| v == "yes"))
}

/// The tags of the fixed devices among `cs`.
fn fixed_tags(h: &mut H, cs: &[ComponentId]) -> BTreeMap<ComponentId, Option<String>> {
    let doc = h.doc();
    cs.iter()
        .filter_map(|c| {
            doc.components
                .get(c)
                .filter(|x| x.properties.get("tag_fixed").is_some_and(|v| v == "yes"))
                .map(|x| (*c, x.properties.get("tag").cloned()))
        })
        .collect()
}

fn erase_device(h: &mut H, page: PageId, c: ComponentId) {
    let item = h.symbol_item(c);
    let hd = h.handle_of(page, item);
    h.run("erase", json!({"handles": [hd.hex()]})).unwrap();
}

/// Every device and cable tag by its key; panics on a duplicate (the end-state rule, FP6).
fn assert_unique(h: &mut H, step: &str) -> BTreeMap<String, ObjectRef> {
    let doc = h.doc();
    let mut seen: BTreeMap<String, ObjectRef> = BTreeMap::new();
    let all = doc
        .components
        .iter()
        .filter_map(|(c, x)| x.properties.get("tag").map(|t| (t.clone(), ObjectRef::Component(*c))))
        .chain(doc.cables.iter().filter_map(|(k, x)| x.properties.get("tag").map(|t| (t.clone(), ObjectRef::Cable(*k)))));
    for (t, o) in all {
        if let Some(prev) = seen.insert(tag::key(&t), o) {
            panic!("{step}: duplicate tag {t} on {o:?} and {prev:?}");
        }
    }
    seen
}

fn k_numbers(h: &mut H) -> BTreeSet<u64> {
    let doc = h.doc();
    doc.components.values().filter_map(|c| c.properties.get("tag")).filter_map(|t| tag::parse(t)).filter(|(p, _)| p == "K").map(|(_, n)| n).collect()
}

/// A small, seeded generator (no dependency).
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

// ---------------------------------------------------------------- 1. oracle: random sequences

/// Random renumber, swap, merge, next, `tag_fixed`, typed and automatic place, delete, undo and
/// redo. After every step: no duplicate tag (devices and cables), replay equals live (the harness),
/// fixed tags unchanged by Renumber and Next, and an automatic tag never reuses a number that a
/// device held since the last high-water reset still in effect. The high-water check is left out
/// after an undone renumber here (the bug of [`undo_of_a_renumber_does_not_free_a_deleted_tag`]);
/// the strict run below keeps it.
#[test]
#[ignore = "slow: 24 seeds of 120 random numbering steps"]
fn oracle_random_numbering_sequences_keep_tags_unique_and_fixed_tags_still() {
    for seed in 1..=24u64 {
        oracle_run(seed, 120, false);
    }
}

/// [`oracle_random_numbering_sequences_keep_tags_unique_and_fixed_tags_still`] with the
/// high-water check after undone renumbers too (fails at seed 4, step 93: `-K16` given again).
#[test]
#[ignore = "slow: 24 seeds of 120 random numbering steps, strict high-water check"]
fn oracle_strict_high_water_after_undone_renumbers() {
    for seed in 1..=24u64 {
        oracle_run(seed, 120, true);
    }
}

#[derive(Clone)]
struct Model {
    /// K numbers held since the last reset in effect (never given again automatically).
    burnt: BTreeSet<u64>,
}

fn oracle_run(seed: u64, steps: usize, strict: bool) {
    let mut h = H::new(0x026_0000 + seed);
    let page = h.first_page();
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ seed.wrapping_mul(0x1234_5678_9ABC_DEF1));
    let mut slots: Vec<[f64; 2]> = Vec::new();
    for col in 0..8 {
        for row in 0..4 {
            slots.push(cell(&mut h, page, col, row));
        }
    }
    let mut used_slots: BTreeMap<usize, ComponentId> = BTreeMap::new();
    // One model per transaction on the undo stack, so undo / redo restore it.
    let mut undo: Vec<Model> = Vec::new();
    let mut redo: Vec<Model> = Vec::new();
    let mut m = Model { burnt: BTreeSet::new() };
    let mut trace: Vec<String> = Vec::new();
    // A renumber was undone since the last renumber in effect (non-strict runs skip the check).
    let mut undone_renumber = false;
    for step in 0..steps {
        let live: Vec<ComponentId> = {
            let doc = h.doc();
            used_slots.values().copied().filter(|c| doc.components.contains_key(c)).collect()
        };
        let pick = |rng: &mut Rng, v: &[ComponentId]| v.get(rng.below(v.len() as u64) as usize).copied();
        let head = h.head();
        let fixed_before = fixed_tags(&mut h, &live);
        let op = rng.below(11);
        let mut what = String::new();
        let mut is_renumber = false;
        let mut auto_placed: Option<ComponentId> = None;
        let mut fixed_must_hold = false;
        match op {
            0..=2 => {
                // Place, automatic or typed.
                let free: Vec<usize> = (0..slots.len()).filter(|i| !used_slots.contains_key(i) || !live.contains(&used_slots[i])).collect();
                if let Some(&i) = free.get(rng.below(free.len() as u64) as usize) {
                    let typed = (rng.below(3) == 0).then(|| format!("-K{}", 1 + rng.below(12)));
                    what = format!("place {typed:?}");
                    if let Ok(c) = coil(&mut h, slots[i], typed.as_deref()) {
                        used_slots.insert(i, c);
                        if typed.is_none() {
                            auto_placed = Some(c);
                        }
                    }
                }
            }
            3 => {
                if let Some(c) = pick(&mut rng, &live) {
                    what = format!("delete {:?}", tag_of(&mut h, c));
                    erase_device(&mut h, page, c);
                }
            }
            4 => {
                what = "renumber devices all".into();
                fixed_must_hold = true;
                if h.run("renumber", json!({"kind": "devices", "scope": "all"})).is_ok() {
                    is_renumber = true;
                }
            }
            5..=7 => {
                // A typed tag that may be taken, with a random conflict choice.
                if let Some(c) = pick(&mut rng, &live) {
                    let t = format!("-K{}", 1 + rng.below(12));
                    let conflict = ["swap", "next", "merge", "none"][rng.below(4) as usize];
                    what = format!("setprop {:?} -> {t} {conflict}", tag_of(&mut h, c));
                    fixed_must_hold = conflict == "next";
                    let mut p = json!({"component": c.hex(), "key": "tag", "value": t});
                    if conflict != "none" {
                        p["conflict"] = json!(conflict);
                    }
                    let _ = h.run("setprop", p);
                }
            }
            8 => {
                if let Some(c) = pick(&mut rng, &live) {
                    let on = !fixed(&mut h, c);
                    what = format!("tag_fixed {on} on {:?}", tag_of(&mut h, c));
                    let _ = h.run("setprop", json!({"component": c.hex(), "key": "tag_fixed", "value": if on { json!("yes") } else { Value::Null }}));
                }
            }
            9 => {
                what = "u".into();
                let _ = h.run("u", Value::Null);
            }
            _ => {
                what = "redo".into();
                let _ = h.run("redo", Value::Null);
            }
        }
        trace.push(format!("{step}: {what}"));
        let new_head = h.head();
        let ctx = || trace.join("\n");
        assert_unique(&mut h, &format!("seed {seed} step {step}\n{}", ctx()));
        // Fixed tags of the devices that are still there do not move under Renumber or Next.
        if fixed_must_hold {
            for (c, t) in &fixed_before {
                if h.doc().components.contains_key(c) {
                    // A Next on the fixed device itself rewrites its own tag (typed).
                    if what.starts_with("setprop") && what.contains(&format!("{t:?}")) {
                        continue;
                    }
                    assert_eq!(&tag_of(&mut h, *c), t, "seed {seed} step {step}: a fixed tag moved\n{}", ctx());
                }
            }
        }
        // The high-water oracle.
        if new_head > head {
            let last = h.p(|p| p.ledger.log().last().map(|t| t.kind));
            match last {
                Some(ecad_model::TxKind::Undo { .. }) => {
                    let label = h.p(|p| p.ledger.log().last().map(|t| t.label.clone())).unwrap_or_default();
                    if label.contains("Renumber") {
                        undone_renumber = true;
                    }
                    let prev = undo.pop().unwrap_or_else(|| m.clone());
                    redo.push(m.clone());
                    // Undoing never makes a held number free again.
                    let mut b = prev.burnt;
                    b.extend(m.burnt.iter().copied());
                    m = Model { burnt: b };
                }
                Some(ecad_model::TxKind::Redo { .. }) => {
                    let label = h.p(|p| p.ledger.log().last().map(|t| t.label.clone())).unwrap_or_default();
                    undo.push(m.clone());
                    let _ = redo.pop();
                    if label.contains("Renumber") {
                        m.burnt = k_numbers(&mut h);
                        undone_renumber = false;
                    }
                }
                _ => {
                    undo.push(m.clone());
                    redo.clear();
                    if is_renumber {
                        m.burnt = k_numbers(&mut h);
                        undone_renumber = false;
                    }
                }
            }
            if let Some(c) = auto_placed
                && (strict || !undone_renumber)
                && let Some((_, n)) = tag_of(&mut h, c).as_deref().and_then(tag::parse)
            {
                assert!(!m.burnt.contains(&n), "seed {seed} step {step}: automatic -K{n:02} was held since the last reset\n{}", ctx());
            }
            m.burnt.extend(k_numbers(&mut h));
        }
    }
}

// ---------------------------------------------------------------- 1. high-water mark, targeted

/// Three automatic coils `-K01`…`-K03` in columns A, C, E of the first page, and `-K04` placed
/// and deleted (the mark is 4).
fn three_and_a_deleted_fourth(seed: u64) -> (H, PageId, ComponentId, ComponentId, ComponentId) {
    let mut h = H::new(seed);
    let page = h.first_page();
    let at: Vec<[f64; 2]> = [0, 2, 4, 6].iter().map(|c| cell(&mut h, page, *c, 1)).collect();
    let a = coil(&mut h, at[0], None).unwrap();
    let b = coil(&mut h, at[1], None).unwrap();
    let c = coil(&mut h, at[2], None).unwrap();
    let d = coil(&mut h, at[3], None).unwrap();
    assert_eq!(tag_of(&mut h, d).as_deref(), Some("-K04"));
    erase_device(&mut h, page, d);
    (h, page, a, b, c)
}

/// Exchange two tags out of drawing order and release the typed one's fixed flag, so a renumber
/// has something to change.
fn scramble(h: &mut H, a: ComponentId, b: ComponentId) {
    let tb = tag_of(h, b).unwrap();
    h.run("setprop", json!({"component": a.hex(), "key": "tag", "value": tb, "conflict": "swap"})).unwrap();
    h.run("setprop", json!({"component": a.hex(), "key": "tag_fixed", "value": null})).unwrap();
}

/// FP4 and undo: the first renumber writes `numbering.high.K` (no value before). Undoing it gives
/// back the tags, but the undo's inverse op clears the property, which `note_ops` ignores, so the
/// mark stays at the reset value: the deleted `-K04` is given again although the reset is undone.
#[test]
fn undo_of_a_renumber_does_not_free_a_deleted_tag() {
    let (mut h, page, a, b, _c) = three_and_a_deleted_fourth(0x0260_0001);
    scramble(&mut h, a, b);
    h.run("renumber", json!({"kind": "devices", "scope": "all"})).unwrap();
    assert_eq!((tag_of(&mut h, a).as_deref(), tag_of(&mut h, b).as_deref()), (Some("-K01"), Some("-K02")));
    h.run("u", Value::Null).unwrap();
    let at = cell(&mut h, page, 7, 2);
    let e = coil(&mut h, at, None).unwrap();
    assert_eq!(tag_of(&mut h, e).as_deref(), Some("-K05"), "-K04 was held by a deleted device and the reset is undone");
}

/// The same with a reset already stored: the undo's inverse writes the older value back, which
/// `note_ops` takes as a new reset (overwrite), lowering the mark below the deleted `-K04`.
#[test]
fn undo_of_a_second_renumber_does_not_free_a_deleted_tag() {
    let mut h = H::new(0x0260_0002);
    let page = h.first_page();
    let at: Vec<[f64; 2]> = [0, 2, 4, 6, 7].iter().map(|c| cell(&mut h, page, *c, 1)).collect();
    let a = coil(&mut h, at[0], None).unwrap();
    let b = coil(&mut h, at[1], None).unwrap();
    scramble(&mut h, a, b);
    h.run("renumber", json!({"kind": "devices", "scope": "all"})).unwrap();
    let _c = coil(&mut h, at[2], None).unwrap();
    let d = coil(&mut h, at[3], None).unwrap();
    assert_eq!(tag_of(&mut h, d).as_deref(), Some("-K04"));
    erase_device(&mut h, page, d);
    scramble(&mut h, a, b);
    h.run("renumber", json!({"kind": "devices", "scope": "all"})).unwrap();
    h.run("u", Value::Null).unwrap();
    let e = coil(&mut h, at[4], None).unwrap();
    assert_eq!(tag_of(&mut h, e).as_deref(), Some("-K05"), "-K04 was held by a deleted device and the second reset is undone");
}

/// FP4: "the highest number now in use". A second renumber whose result equals the stored
/// `numbering.high.K` writes no reset op (before == after), so the mark stays at `-K04`, which a
/// device took and gave up after the first renumber: the next coil is `-K05`, not `-K04`.
#[test]
fn a_second_renumber_to_the_same_top_still_resets_the_mark() {
    let mut h = H::new(0x0260_0003);
    let page = h.first_page();
    let at: Vec<[f64; 2]> = [0, 2, 4, 6, 7].iter().map(|c| cell(&mut h, page, *c, 1)).collect();
    let a = coil(&mut h, at[0], None).unwrap();
    let b = coil(&mut h, at[1], None).unwrap();
    let _c = coil(&mut h, at[2], None).unwrap();
    scramble(&mut h, a, b);
    h.run("renumber", json!({"kind": "devices", "scope": "all"})).unwrap();
    let d = coil(&mut h, at[3], None).unwrap();
    assert_eq!(tag_of(&mut h, d).as_deref(), Some("-K04"));
    erase_device(&mut h, page, d);
    scramble(&mut h, a, b);
    let r = h.run("renumber", json!({"kind": "devices", "scope": "all"})).unwrap();
    assert_eq!(r["renumbered"], json!(2), "{r}");
    let e = coil(&mut h, at[4], None).unwrap();
    assert_eq!(tag_of(&mut h, e).as_deref(), Some("-K04"), "the renumber reset the mark to 3 (design §2.3)");
}

/// Without a renumber, a deleted tag is never given again, also after undo and redo of the
/// delete, and across save and reopen.
#[test]
fn a_deleted_tag_is_never_given_again_without_a_reset_also_after_reopen() {
    let dir = std::env::temp_dir().join(format!("ecad-t026-{}", ecad_store::disk::unique()));
    std::fs::create_dir_all(&dir).unwrap();
    let (mut h, page, ..) = three_and_a_deleted_fourth(0x0260_0004);
    h.run("u", Value::Null).unwrap();
    h.run("redo", Value::Null).unwrap();
    h.cfg(|p| p.persistence = Persistence::File);
    let path = dir.join("hw.ecad").display().to_string();
    h.run("saveas", json!({"path": path})).unwrap();
    while !h.s.docs.is_empty() {
        h.s.execute("close", &json!({"index": 0})).unwrap();
    }
    let mut s = Session::empty();
    let (project, _) = ProjectHandle::open_file(&mut s, &path, 79).unwrap();
    let mut h2 = H { s, project, trail: Vec::new() };
    let at = cell(&mut h2, page, 7, 3);
    let e = coil(&mut h2, at, None).unwrap();
    assert_eq!(tag_of(&mut h2, e).as_deref(), Some("-K05"));
    h2.check();
    drop(h2);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Renumber never produces a duplicate around fixed, out-of-form and typed tags, with starts that
/// collide with fixed numbers, and keeps every fixed tag.
#[test]
fn renumber_with_a_start_around_fixed_tags_makes_no_duplicate() {
    let mut h = H::new(0x0260_0005);
    let page = h.first_page();
    let mut cs = Vec::new();
    for col in 0..6 {
        let at = cell(&mut h, page, col, 1);
        cs.push(coil(&mut h, at, None).unwrap());
    }
    h.run("setprop", json!({"component": cs[5].hex(), "key": "tag_fixed", "value": "yes"})).unwrap();
    h.run("setprop", json!({"component": cs[0].hex(), "key": "tag", "value": "-K05", "conflict": "swap"})).unwrap();
    h.run("setprop", json!({"component": cs[2].hex(), "key": "tag", "value": "-K1A"})).unwrap();
    let fixed_tags = fixed_tags(&mut h, &cs);
    for start in [1u64, 4, 5, 6, 999_999_998] {
        let r = h.run("renumber", json!({"kind": "devices", "scope": "all", "start": start}));
        assert_unique(&mut h, &format!("start {start}: {r:?}"));
        for (c, t) in &fixed_tags {
            assert_eq!(&tag_of(&mut h, *c), t, "start {start}: a fixed tag moved");
        }
    }
    h.check();
}

// ---------------------------------------------------------------- 2. provisional tags

fn provisional_left(h: &mut H) -> ecad_cadcraft::numbering_cmds::Provisional {
    h.p(|p| ecad_cadcraft::numbering_cmds::provisional_items(p.ledger.document()))
}

/// Finalise follows the high-water mark, not only the highest tag in use: with `-K04` deleted,
/// provisional coils become `-K05` and up, and nothing provisional is left (tags, cables, nets).
#[test]
fn finalise_follows_the_mark_and_leaves_nothing_provisional() {
    let (mut h, page, ..) = three_and_a_deleted_fourth(0x0260_0006);
    h.run("numbering", json!({"provisional": true})).unwrap();
    let at = cell(&mut h, page, 7, 2);
    let k = coil(&mut h, at, None).unwrap();
    assert_eq!(tag_of(&mut h, k).as_deref(), Some("-K?1"));
    // A typed `-K?7` is kept; `-K?` takes the next one.
    let at = cell(&mut h, page, 7, 3);
    let k7 = coil(&mut h, at, Some("-K?7")).unwrap();
    assert_eq!(tag_of(&mut h, k7).as_deref(), Some("-K?7"));
    let at = cell(&mut h, page, 6, 3);
    let k8 = coil(&mut h, at, Some("-K?")).unwrap();
    assert_eq!(tag_of(&mut h, k8).as_deref(), Some("-K?8"));
    // A provisional wire net and a provisional cable.
    for y in [0.0, 10.0] {
        h.run("wire", json!({"points": [[3000, y], [3060, y]], "potential": "24V"})).unwrap();
    }
    let w: CableId = id(&h.run("cableslash", json!({"from": [3020, -3], "to": [3020, 13]})).unwrap(), "cable");
    let cable_tag = |h: &mut H| h.doc().cables[&w].properties.get("tag").cloned();
    assert_eq!(cable_tag(&mut h).as_deref(), Some("-W?1"));
    assert!(!provisional_left(&mut h).is_empty());
    let r = h.run("finalise", json!({})).unwrap();
    assert!(provisional_left(&mut h).is_empty(), "{:?} after {r}", provisional_left(&mut h));
    let tags: Vec<Option<String>> = [k, k7, k8].iter().map(|c| tag_of(&mut h, *c)).collect();
    for t in tags.iter().flatten() {
        let n = tag::parse(t).map(|x| x.1).unwrap_or(0);
        assert!(n >= 5, "{t}: -K04 was held by a deleted device; finalised tags follow the mark ({tags:?})");
    }
    assert!(cable_tag(&mut h).is_some_and(|t| !tag::is_provisional(&t)));
    assert_unique(&mut h, "after finalise");
    // A second Finalise changes nothing and records nothing.
    let head = h.head();
    h.run("finalise", json!({})).unwrap();
    assert_eq!(h.head(), head);
    // Undo brings every provisional tag back; redo finalises them again.
    h.run("u", Value::Null).unwrap();
    assert_eq!(tag_of(&mut h, k7).as_deref(), Some("-K?7"));
    h.run("redo", Value::Null).unwrap();
    assert!(provisional_left(&mut h).is_empty());
    h.check();
}

/// A taken provisional tag: refused without `conflict`; `swap` exchanges two provisional tags and
/// fixes neither; `next` gives the next provisional number and does not fix it.
#[test]
fn a_taken_provisional_tag_offers_the_three_ways_and_never_fixes() {
    let mut h = H::new(0x0260_0007);
    let page = h.first_page();
    h.run("numbering", json!({"provisional": true})).unwrap();
    let at = cell(&mut h, page, 0, 1);
    let a = coil(&mut h, at, None).unwrap();
    let at = cell(&mut h, page, 2, 1);
    let b = coil(&mut h, at, None).unwrap();
    let e = h.run("setprop", json!({"component": b.hex(), "key": "tag", "value": "-K?1"})).unwrap_err();
    assert!(e.contains("\"swap\""), "{e}");
    h.run("setprop", json!({"component": b.hex(), "key": "tag", "value": "-K?1", "conflict": "swap"})).unwrap();
    assert_eq!((tag_of(&mut h, a).as_deref(), tag_of(&mut h, b).as_deref()), (Some("-K?2"), Some("-K?1")));
    assert!(!fixed(&mut h, a) && !fixed(&mut h, b));
    h.run("setprop", json!({"component": b.hex(), "key": "tag", "value": "-K?2", "conflict": "next"})).unwrap();
    assert_eq!(tag_of(&mut h, b).as_deref(), Some("-K?3"));
    assert!(!fixed(&mut h, b));
    assert_unique(&mut h, "provisional conflicts");
    h.check();
}

/// Older projects never block: unnumbered nets with the setting off do not stop an issue.
#[test]
fn issue_with_unnumbered_nets_and_the_setting_off_does_not_stop() {
    let mut h = H::new(0x0260_0008);
    h.run("wire", json!({"points": [[0, 0], [60, 0]], "potential": "24V"})).unwrap();
    h.run("wire", json!({"points": [[0, 20], [60, 20]]})).unwrap();
    for (f, v) in [("rev", "A"), ("description", "First"), ("drawn", "CV")] {
        h.run("revision", json!({f: v})).unwrap();
    }
    assert!(provisional_left(&mut h).is_empty());
    let head = h.head();
    h.s.execute("issuerevision", &json!({})).unwrap();
    assert_eq!(h.head(), head + 1);
}

/// `issuerevision {provisional: "finalise"}` from the command layer: refused before anything is
/// written when the issue cannot go ahead (fields missing, a rev issued before); else two
/// transactions, the revision point holds the finalised tag, and two undos give the provisional
/// tag back.
#[test]
fn issue_with_finalise_writes_nothing_when_the_issue_is_refused_and_undoes_in_two_steps() {
    let mut h = H::new(0x0260_0009);
    let page = h.first_page();
    h.run("numbering", json!({"provisional": true})).unwrap();
    let at = cell(&mut h, page, 1, 1);
    let k = coil(&mut h, at, None).unwrap();
    let head = h.head();
    let e = h.s.execute("issuerevision", &json!({"provisional": "finalise"})).map_err(|e| e.to_string());
    assert!(e.is_err());
    assert_eq!(h.head(), head, "fields missing: refused before Finalise ({e:?})");
    assert_eq!(tag_of(&mut h, k).as_deref(), Some("-K?1"));
    for (f, v) in [("rev", "A"), ("description", "First"), ("drawn", "CV")] {
        h.run("revision", json!({f: v})).unwrap();
    }
    let head = h.head();
    h.s.execute("issuerevision", &json!({"provisional": "finalise"})).unwrap();
    assert_eq!(h.head(), head + 2, "Finalise, then the issue");
    assert_eq!(tag_of(&mut h, k).as_deref(), Some("-K01"));
    let rev_doc = h.p(|p| {
        let at = p.ledger.document().versions.values().copied().max().unwrap_or(0);
        p.ledger.state_at(at).unwrap()
    });
    assert_eq!(rev_doc.components[&k].properties.get("tag").map(String::as_str), Some("-K01"), "the revision point holds the finalised tag");
    // A rev issued before: refused before Finalise writes.
    let at = cell(&mut h, page, 3, 1);
    let k2 = coil(&mut h, at, None).unwrap();
    let head = h.head();
    let e = h.s.execute("issuerevision", &json!({"provisional": "finalise", "rev": "A"})).map_err(|e| e.to_string());
    assert!(e.is_err(), "rev A again");
    assert_eq!(h.head(), head, "a refused issue leaves no Finalise behind: {e:?}");
    assert_eq!(tag_of(&mut h, k2).as_deref(), Some("-K?1"));
    // Two undos from the issue: the issue, then Finalise.
    h.run("u", Value::Null).unwrap(); // the coil k2
    h.s.execute("undoissue", &json!({})).unwrap();
    assert_eq!(tag_of(&mut h, k).as_deref(), Some("-K01"), "undoing the issue keeps Finalise");
    h.s.execute("u", &Value::Null).unwrap();
    assert_eq!(tag_of(&mut h, k).as_deref(), Some("-K?1"));
    h.check();
}

// ---------------------------------------------------------------- 5, 6. save, reopen, old projects

/// A project written before 026 (the version 1 golden file) opens with no change, no transaction
/// and nothing provisional, and the numbering read records nothing.
#[test]
fn a_pre_026_golden_project_opens_unchanged() {
    let dir = std::env::temp_dir().join(format!("ecad-t026-gold-{}", ecad_store::disk::unique()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = concat!(env!("CARGO_MANIFEST_DIR"), "/../ecad-file/tests/golden/project-v1.ecad");
    let v1 = dir.join("old.ecad");
    std::fs::copy(src, &v1).unwrap();
    let (_, path) = ecad_cadcraft::file::convert(&v1.display().to_string(), None, "tester-026").unwrap();
    ecad_cadcraft::install();
    let mut s = Session::empty();
    let (project, _) = ProjectHandle::open_file(&mut s, &path, 80).unwrap();
    let mut h = H { s, project, trail: Vec::new() };
    let head = h.head();
    let before = h.doc();
    assert!(!h.p(|p| p.ledger.is_unsaved()), "opening an old file changes nothing");
    assert!(provisional_left(&mut h).is_empty());
    assert!(!ecad_model::numbering::provisional_on(&before));
    h.run("numbering", json!({})).unwrap();
    h.run("renumber", json!({"kind": "all", "preview": true})).unwrap();
    assert_eq!(h.head(), head);
    assert_eq!(h.doc(), before);
    drop(h);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Old cable tags (`C1`, `W0`, stored as typed before 026) are kept at reopen, share the
/// namespace with the new form (`-C01` is `C1`), and new cables continue after every W tag the
/// history held. Two old cables sharing a tag open with a Checks error, not a refusal.
#[test]
fn old_cable_tags_are_kept_at_reopen_and_still_count() {
    let dir = std::env::temp_dir().join(format!("ecad-t026-cab-{}", ecad_store::disk::unique()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut h = H::new(0x0260_000A);
    h.cfg(|p| p.persistence = Persistence::File);
    for y in [0.0, 10.0, 30.0, 40.0, 60.0, 70.0] {
        h.run("wire", json!({"points": [[0, y], [60, y]]})).unwrap();
    }
    let k1: CableId = id(&h.run("cableslash", json!({"from": [20, -3], "to": [20, 13]})).unwrap(), "cable");
    let k2: CableId = id(&h.run("cableslash", json!({"from": [20, 27], "to": [20, 43]})).unwrap(), "cable");
    let k3: CableId = id(&h.run("cableslash", json!({"from": [20, 57], "to": [20, 73]})).unwrap(), "cable");
    // What a pre-026 build stored: tags as typed, two of them the same.
    let old = [(k1, "C1"), (k2, "W0"), (k3, "W0")];
    h.cfg(|p| {
        let doc = p.ledger.document();
        let ops: Vec<Op> = old
            .iter()
            .map(|(k, t)| Op::SetProperty {
                object: ObjectRef::Cable(*k),
                key: "tag".into(),
                before: doc.cables[k].properties.get("tag").cloned(),
                after: Some((*t).into()),
            })
            .collect();
        p.ledger.commit(Author::User, "old tags", ops).unwrap();
    });
    let path = dir.join("cables.ecad").display().to_string();
    h.s.execute("saveas", &json!({"path": path})).unwrap();
    let live = h.doc();
    while !h.s.docs.is_empty() {
        h.s.execute("close", &json!({"index": 0})).unwrap();
    }
    let mut s = Session::empty();
    let (project, _) = ProjectHandle::open_file(&mut s, &path, 81).unwrap();
    let mut h2 = H { s, project, trail: Vec::new() };
    let head = h2.head();
    assert_eq!(h2.doc(), live, "no silent retagging at open");
    for (k, t) in old {
        assert_eq!(h2.doc().cables[&k].properties.get("tag").map(String::as_str), Some(t));
    }
    let kinds = h2.p(|p| ecad_cadcraft::checks::all(p.ledger.document(), p.tolerance).iter().map(|r| r.kind.name().to_string()).collect::<Vec<_>>());
    assert!(kinds.iter().any(|k| k == "duplicate-tag"), "{kinds:?}");
    assert_eq!(h2.head(), head);
    // `-C01` is `C1`'s tag, for a cable and for a device.
    let e = h2.run("setprop", json!({"cable": k2.hex(), "key": "tag", "value": "-C01"})).unwrap_err();
    assert!(e.contains("C1"), "{e}");
    let page = h2.first_page();
    let at = cell(&mut h2, page, 5, 3);
    let k = coil(&mut h2, at, None).unwrap();
    assert!(h2.run("setprop", json!({"component": k.hex(), "key": "tag", "value": "C1"})).is_err());
    // A new cable continues after every W tag the history held.
    h2.run("wire", json!({"points": [[0, 100], [60, 100]]})).unwrap();
    let k4: CableId = id(&h2.run("cableslash", json!({"from": [20, 97], "to": [20, 103]})).unwrap(), "cable");
    let t4 = h2.doc().cables[&k4].properties.get("tag").cloned().unwrap_or_default();
    assert_eq!(t4, "-W04", "-W01…-W03 were given before the old tags were typed");
    let doc = h2.doc();
    let mut seen = BTreeSet::new();
    for t in doc.components.values().filter_map(|c| c.properties.get("tag")).chain(doc.cables.values().filter_map(|c| c.properties.get("tag"))) {
        if t != "W0" {
            assert!(seen.insert(tag::key(t)), "duplicate {t}");
        }
    }
    drop(h2);
    let _ = std::fs::remove_dir_all(&dir);
}
