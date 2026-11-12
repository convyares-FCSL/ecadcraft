//! Tester contracts for brief 029 (Verify, design §3 and §8; FP1, design §10), written from the
//! brief and the design only. They add what `verify_contracts` and `tag_rule_contracts` do not
//! cover: every damage case on a **branch** as well as on main, the stored chain column changed on
//! its own, a rewrite at a snapshot's own transaction, a rewrite between two snapshots of a
//! branch, damage after the last snapshot, a rewrite before a fork, and hostile truncations.
//!
//! The reference (the Verify before 029: the full check after every transaction) is an
//! independent copy here. With `ECAD_TESTER029_EXPORT=<folder>` the damaged files are also kept
//! there, so a build of `dev` from before 029 can verify the same bytes (tester report).
#![cfg(not(target_arch = "wasm32"))]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::common::*;
use crate::container_v2::TempDir;
use ecad_file::chain::{self, Hash};
use ecad_file::v2::schema::{self, BranchId, VersionKind, VersionRecord};
use ecad_file::v2::verify::{verify, verify_chain};
use ecad_file::v2::{ProjectV2, Store};
use ecad_model::*;
use redb::ReadableTable;

const EVERY: u64 = 16;
const NOTE: &str = "tester.note";
const BRANCH: &str = "Option T";

struct Fx {
    path: PathBuf,
    main: BranchId,
    branch: BranchId,
    /// The branch's transaction that sets [`NOTE`] (branch-local seq).
    note_at: u64,
    page: PageId,
    _dir: TempDir,
}

fn line(ids: &mut IdAllocator, page: PageId, layer: LayerId, x: f64) -> Op {
    let id: ItemId = ids.fresh();
    Op::AddItem {
        id,
        item: Item { page, style: style(layer), kind: ItemKind::Primitive(Primitive::Line { a: Pt::new(x, 0.0), b: Pt::new(x, 9.0) }) },
    }
}

/// Main: 40 lines with saves (automatic snapshots every 16) and a named version; a branch
/// [`BRANCH`] forked at main's head with 20 lines, a save, the note, 20 more lines and a save, so
/// the branch has snapshots on both sides of its note; then three lines on main after the fork and
/// after main's last snapshot.
fn build(name: &str) -> Fx {
    let dir = TempDir::new(&format!("t029-{name}"));
    let mut f = fixture();
    let path = dir.path("p.ecad");
    let mut proj = ProjectV2::create(Store::create(&path).unwrap(), PROJECT, &f.ledger, None, None, Some(1), WRITER, EVERY).unwrap().0;
    f.ledger.mark_persisted(f.ledger.head()).unwrap();
    for i in 0..40 {
        let op = line(&mut f.ids, f.page, f.layer, 10.0 + f64::from(i));
        f.ledger.commit(Author::User, "LINE", vec![op]).unwrap();
        if i % 17 == 16 {
            proj.save(&mut f.ledger).unwrap();
        }
    }
    let at = f.ledger.head();
    f.ledger.commit(Author::User, "Version", vec![Op::NameVersion { name: "Issue T".into(), at, description: None }]).unwrap();
    proj.save(&mut f.ledger).unwrap();
    let main = proj.branch();
    let (branch, _) = proj.create_branch(BRANCH, main, at, None, None).unwrap();
    let (mut bl, _) = proj.switch_to(&f.ledger, branch, SEED ^ 7).unwrap();
    for i in 0..20 {
        let op = line(&mut f.ids, f.page, f.layer, 500.0 + f64::from(i));
        bl.commit(Author::User, "LINE", vec![op]).unwrap();
    }
    proj.save(&mut bl).unwrap();
    let note = vec![Op::SetProperty { object: ObjectRef::Project, key: NOTE.into(), before: None, after: Some("first".into()) }];
    bl.commit(Author::User, "Note", note).unwrap();
    let note_at = bl.head();
    for i in 0..20 {
        let op = line(&mut f.ids, f.page, f.layer, 600.0 + f64::from(i));
        bl.commit(Author::User, "LINE", vec![op]).unwrap();
    }
    proj.save(&mut bl).unwrap();
    let (mut ml, _) = proj.switch_to(&bl, main, SEED ^ 8).unwrap();
    for i in 0..3 {
        let op = line(&mut f.ids, f.page, f.layer, 900.0 + f64::from(i));
        ml.commit(Author::User, "LINE", vec![op]).unwrap();
    }
    proj.save(&mut ml).unwrap();
    drop(proj);
    Fx { path, main, branch, note_at, page: f.page, _dir: dir }
}

fn store(p: &Path) -> Store {
    Store::open(p, false).unwrap()
}

fn write(p: &Path, f: impl FnOnce(&redb::WriteTransaction)) {
    let db = redb::Database::open(p).unwrap();
    let w = db.begin_write().unwrap();
    f(&w);
    w.commit().unwrap();
}

fn log_of(p: &Path, b: BranchId) -> Vec<Transaction> {
    let o = ProjectV2::open(store(p), 1, WRITER).unwrap();
    let last = o.project.store().reader().unwrap().last_seq(b).unwrap().unwrap();
    o.project.tx_display(b, 1, last).unwrap()
}

fn snapshots_of(p: &Path, b: BranchId) -> Vec<VersionRecord> {
    let r = store(p).reader().unwrap();
    let mut v = Vec::new();
    for kind in [VersionKind::User, VersionKind::Revision, VersionKind::Auto] {
        v.extend(r.versions(b, kind).unwrap());
    }
    v.sort_by_key(|v| v.point.seq);
    v
}

/// Rewrite `b`'s rows from `from` on as `txs`, re-chained, with the head and every version row
/// at or after `from` re-pointed, so the chain check passes. (Branches forked from `b` keep their
/// old fork chain value.)
fn rewrite(p: &Path, b: BranchId, from: u64, txs: &[Transaction]) {
    let s = store(p);
    let r = s.reader().unwrap();
    let mut prev = if from == 1 { ecad_file::v2::project::lineage(&r, PROJECT, b).unwrap()[0].seed } else { r.tx(b, from - 1).unwrap().unwrap().1 };
    let mut chains: BTreeMap<u64, Hash> = BTreeMap::new();
    let mut rows = Vec::new();
    for tx in txs {
        let v = ecad_file::record::tx_value(tx, &prev).unwrap();
        let canonical = ecad_file::canonical::canonical(&v).unwrap();
        let h = chain::tx_hash(&canonical);
        let (flag, body) = ecad_file::v2::pack::pack(&canonical);
        rows.push((tx.seq, schema::tx_value(flag, &h, &body)));
        chains.insert(tx.seq, h);
        prev = h;
    }
    let mut rec = r.branch(b).unwrap().unwrap();
    rec.head = schema::Point::new(rec.head.seq, &chains[&rec.head.seq]);
    let mut versions = Vec::new();
    for kind in [VersionKind::User, VersionKind::Revision, VersionKind::Auto, VersionKind::Merge] {
        for mut v in r.versions(b, kind).unwrap() {
            if let Some(h) = chains.get(&v.point.seq) {
                v.point = schema::Point::new(v.point.seq, h);
                versions.push((schema::version_key(b, kind, &v.name), v));
            }
        }
    }
    drop(r);
    drop(s);
    write(p, |w| {
        let mut t = w.open_table(schema::TRANSACTIONS).unwrap();
        for (seq, row) in &rows {
            t.insert(schema::tx_key(b, *seq), row.as_slice()).unwrap();
        }
        w.open_table(schema::BRANCHES).unwrap().insert(b.0, serde_json::to_vec(&rec).unwrap().as_slice()).unwrap();
        let mut vt = w.open_table(schema::VERSIONS).unwrap();
        for (k, v) in &versions {
            vt.insert(k.as_slice(), serde_json::to_vec(v).unwrap().as_slice()).unwrap();
        }
    });
}

/// The Verify before 029, as a reference: rows against the chain, the replay with the full check
/// after every transaction, merge rows, then each snapshot (and its chunks) against a forward
/// replay. One failure line each, naming the branch and the transaction or snapshot.
fn reference(p: &Path) -> Vec<String> {
    let s = store(p);
    let r = s.reader().unwrap();
    let mut out = Vec::new();
    let mut branches = Vec::new();
    for (id, b) in r.branches().unwrap() {
        match ecad_file::v2::project::lineage(&r, PROJECT, id) {
            Ok(l) => branches.push((id, b, l)),
            Err(e) => out.push(format!("{}: {e}", b.name)),
        }
    }
    branches.sort_by_key(|(_, _, l)| l.len());
    for (id, b, line) in branches {
        let name = &b.name;
        let start = match line[0].fork {
            Some((pi, fseq)) => match ecad_file::v2::project::line_state(&r, &line, pi, fseq) {
                Ok((d, _)) => d,
                Err(e) => {
                    out.push(format!("{name}: fork: {e}"));
                    continue;
                }
            },
            None => Document::default(),
        };
        let last = r.last_seq(id).unwrap().unwrap_or(0);
        let mut prev = line[0].seed;
        let mut chains = vec![prev];
        let mut txs = Vec::new();
        let mut broken = false;
        for (i, (seq, flag, stored, body)) in r.tx_range(id, 1..=last).unwrap().into_iter().enumerate() {
            if seq != i as u64 + 1 {
                out.push(format!("{name}: transaction {} is missing", i + 1));
                broken = true;
                break;
            }
            let decoded = ecad_file::v2::pack::unpack(flag, &body).and_then(|rec| {
                if chain::tx_hash(&rec) != stored {
                    return Err("the record does not match its chain value".to_string());
                }
                let v: serde_json::Value = serde_json::from_slice(&rec).map_err(|e| e.to_string())?;
                if v["prev"].as_str().and_then(chain::parse_hex) != Some(prev) {
                    return Err("prev does not link".into());
                }
                ecad_file::record::decode_tx(&v).map_err(|e| format!("{e:?}"))
            });
            match decoded {
                Ok(tx) => txs.push(tx),
                Err(e) => {
                    out.push(format!("{name}: transaction {seq}: {e}"));
                    broken = true;
                    break;
                }
            }
            prev = stored;
            chains.push(prev);
        }
        if !broken && (b.head.seq != last || b.head.hash().ok() != Some(prev)) {
            out.push(format!("{name}: the head ({}) is not the last transaction ({last})", b.head.seq));
        }
        if broken {
            continue;
        }
        let ledger = match Ledger::replay_verified_from(start.clone(), txs, 0) {
            Ok(l) => l,
            Err(e) => {
                out.push(format!("{name}: replay failed: {e}"));
                continue;
            }
        };
        for v in r.versions(id, VersionKind::Merge).unwrap() {
            if chains.get(v.point.seq as usize).map(chain::hex) != Some(v.point.chain.clone()) {
                out.push(format!("{name}: the merge row at {}: its chain value differs from the log's", v.point.seq));
            }
        }
        let mut doc = start;
        let mut done = 0u64;
        for v in snapshots_of(p, id) {
            for tx in ledger.log().iter().skip(done as usize).take((v.point.seq - done) as usize) {
                for op in &tx.ops {
                    op.apply(&mut doc).unwrap();
                }
            }
            done = v.point.seq;
            let set = ecad_file::v2::chunks::ChunkSet::from_record(&v.chunks).unwrap();
            let mut bodies = Vec::new();
            let mut bad = None;
            for (k, h) in &set.chunks {
                match r.chunk_body(h) {
                    Ok(b) => bodies.push((k.clone(), b)),
                    Err(e) => bad = Some(e.to_string()),
                }
            }
            if let Some(e) = bad {
                out.push(format!("{name}: version {:?}: {e}", v.name));
                continue;
            }
            let (mut d, _) = ecad_file::v2::chunks::assemble(&set, bodies).unwrap();
            d.rebuild_index();
            if d != doc {
                out.push(format!("{name}: snapshot {:?} at {} DIFFERS from the replay", v.name, v.point.seq));
            }
        }
    }
    out
}

fn has(lines: &[String], want: &str) -> bool {
    lines.iter().any(|l| l.contains(want))
}

/// Keep the damaged file for the old-build check, when asked to.
fn export(f: &Fx, case: &str, want: &str) {
    if let Ok(dir) = std::env::var("ECAD_TESTER029_EXPORT") {
        let dir = PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(&f.path, dir.join(format!("{case}.ecad"))).unwrap();
        std::fs::write(dir.join(format!("{case}.want")), want).unwrap();
    }
}

// ------------------------------------------------------------------ damage

fn flip_body_byte(p: &Path, b: BranchId, seq: u64) {
    write(p, |w| {
        let mut t = w.open_table(schema::TRANSACTIONS).unwrap();
        let k = schema::tx_key(b, seq);
        let mut v = t.get(k).unwrap().unwrap().value().to_vec();
        let i = v.len() - 3;
        v[i] ^= 0x20;
        t.insert(k, v.as_slice()).unwrap();
    });
}

/// The stored chain value of a row replaced; the body is untouched.
fn replace_chain_column(p: &Path, b: BranchId, seq: u64) {
    let s = store(p);
    let (flag, body) = {
        let r = s.reader().unwrap();
        let (_, flag, _h, body) = r.tx_range(b, seq..=seq).unwrap().into_iter().next().unwrap();
        (flag, body)
    };
    drop(s);
    let row = schema::tx_value(flag, &[0x5A; 32], &body);
    write(p, |w| {
        w.open_table(schema::TRANSACTIONS).unwrap().insert(schema::tx_key(b, seq), row.as_slice()).unwrap();
    });
}

fn remove_row(p: &Path, b: BranchId, seq: u64) {
    write(p, |w| {
        w.open_table(schema::TRANSACTIONS).unwrap().remove(schema::tx_key(b, seq)).unwrap();
    });
}

fn head_one_back(p: &Path, b: BranchId) {
    let s = store(p);
    let mut rec = s.reader().unwrap().branch(b).unwrap().unwrap();
    let c = s.reader().unwrap().tx(b, rec.head.seq - 1).unwrap().unwrap().1;
    drop(s);
    rec.head = schema::Point::new(rec.head.seq - 1, &c);
    write(p, |w| {
        w.open_table(schema::BRANCHES).unwrap().insert(b.0, serde_json::to_vec(&rec).unwrap().as_slice()).unwrap();
    });
}

/// Transaction `seq` of `b` rewritten to add a line on a layer that does not exist, re-chained.
fn invariant_break(f: &Fx, b: BranchId, seq: u64) {
    let mut log = log_of(&f.path, b);
    let mut ids = IdAllocator::seeded(0x7E57);
    let ghost: LayerId = ids.fresh();
    let i = (seq - 1) as usize;
    log[i].ops = vec![line(&mut ids, f.page, ghost, 4242.0)];
    log[i].kind = TxKind::Edit;
    rewrite(&f.path, b, seq, &log[i..]);
}

/// `b`'s snapshot `i` given snapshot `j`'s chunks.
fn swap_snapshot_chunks(p: &Path, b: BranchId, i: usize, j: usize) -> VersionRecord {
    let a = snapshots_of(p, b);
    let mut v = a[i].clone();
    v.chunks = a[j].chunks.clone();
    write(p, |w| {
        let k = schema::version_key(b, v.kind, &v.name);
        w.open_table(schema::VERSIONS).unwrap().insert(k.as_slice(), serde_json::to_vec(&v).unwrap().as_slice()).unwrap();
    });
    a[i].clone()
}

/// A chunk only `b`'s snapshots name.
fn own_chunk(p: &Path, b: BranchId, other: BranchId) -> Hash {
    let theirs: BTreeSet<String> = snapshots_of(p, other).into_iter().flat_map(|v| v.chunks.into_values()).collect();
    let mine = snapshots_of(p, b).into_iter().flat_map(|v| v.chunks.into_values()).find(|h| !theirs.contains(h)).unwrap();
    chain::parse_hex(&mine).unwrap()
}

fn merge_row(p: &Path, b: BranchId, source: BranchId, seq: u64) {
    let rec = VersionRecord {
        kind: VersionKind::Merge,
        name: schema::auto_name(seq),
        point: schema::Point::new(seq, &[9; 32]),
        chunks: Default::default(),
        release: None,
        undo: Vec::new(),
        redo: Vec::new(),
        undo_hidden: 0,
        redo_hidden: 0,
        stack_cap: 0,
        description: None,
        merge: schema::MergeRow::of(&TxKind::Merge {
            source: source.0,
            source_head: StepPoint { seq: 1, chain: [1; 32] },
            ancestor: BranchPoint { branch: b.0, seq: 0, chain: [2; 32] },
        }),
        tag_rule: None,
        extra: Default::default(),
    };
    write(p, |w| {
        let k = schema::version_key(b, VersionKind::Merge, &rec.name);
        w.open_table(schema::VERSIONS).unwrap().insert(k.as_slice(), serde_json::to_vec(&rec).unwrap().as_slice()).unwrap();
    });
}

/// Both Verifies fail and name `want`; the chain check alone fails too when `chain`.
fn both_fail(case: &str, f: &Fx, want: &str, chain: bool) {
    export(f, case, want);
    let old = reference(&f.path);
    let new = verify(&store(&f.path));
    assert!(has(&old, want), "{case}: the old Verify names {want:?}: {old:#?}");
    assert!(!new.ok && has(&new.failures, want), "{case}: the new Verify names {want:?}: {:#?}", new.lines);
    let c = verify_chain(&store(&f.path));
    if chain {
        assert!(!c.ok && has(&c.failures, want), "{case}: the chain check names {want:?}: {:#?}", c.lines);
    } else {
        assert!(c.ok, "{case}: the chain check passes: {:#?}", c.lines);
    }
}

#[test]
fn the_tester_fixture_verifies_clean_with_snapshots_either_side_of_the_branch_note() {
    let f = build("clean");
    export(&f, "clean", "");
    assert!(reference(&f.path).is_empty(), "{:#?}", reference(&f.path));
    let v = verify(&store(&f.path));
    assert!(v.ok, "{:#?}", v.lines);
    let snaps = snapshots_of(&f.path, f.branch);
    assert!(snaps.iter().any(|s| s.point.seq < f.note_at) && snaps.iter().any(|s| s.point.seq > f.note_at), "{snaps:#?}");
    assert!(snapshots_of(&f.path, f.main).len() >= 3);
}

#[test]
fn chain_tamper_on_a_branch_is_found_by_both_and_by_the_chain_check() {
    let f = build("b-flip");
    flip_body_byte(&f.path, f.branch, 5);
    both_fail("b-flip", &f, &format!("{BRANCH}: transaction 5"), true);
    let f = build("b-missing");
    remove_row(&f.path, f.branch, 7);
    both_fail("b-missing", &f, &format!("{BRANCH}: transaction 7 is missing"), true);
    let f = build("b-head");
    head_one_back(&f.path, f.branch);
    both_fail("b-head", &f, &format!("{BRANCH}: the head"), true);
    let f = build("b-last-row");
    let last = store(&f.path).reader().unwrap().last_seq(f.branch).unwrap().unwrap();
    remove_row(&f.path, f.branch, last);
    let v = verify(&store(&f.path));
    assert!(!v.ok, "the branch's last row removed: {:#?}", v.lines);
    assert!(!reference(&f.path).is_empty());
}

#[test]
fn a_chain_value_changed_without_its_body_is_found() {
    for (case, on_main, name) in [("m-chaincol", true, "main"), ("b-chaincol", false, BRANCH)] {
        let f = build(case);
        let br = if on_main { f.main } else { f.branch };
        replace_chain_column(&f.path, br, 6);
        both_fail(case, &f, &format!("{name}: transaction 6"), true);
    }
}

#[test]
fn an_invariant_break_on_a_branch_names_its_transaction() {
    let f = build("b-invariant");
    invariant_break(&f, f.branch, 9);
    both_fail("b-invariant", &f, &format!("{BRANCH}: replay failed"), false);
    let v = verify(&store(&f.path));
    let l = v.failures.iter().find(|l| l.contains("replay failed")).cloned().unwrap_or_default();
    assert!(l.contains("transaction 9"), "names the branch's transaction 9: {:#?}", v.lines);
}

#[test]
fn an_invariant_break_after_the_last_snapshot_is_found() {
    let f = build("m-invariant-late");
    // Main's last three rows are lines after the fork and after main's last snapshot.
    let last = store(&f.path).reader().unwrap().last_seq(f.main).unwrap().unwrap();
    let snaps = snapshots_of(&f.path, f.main);
    let seq = last - 1;
    assert!(snaps.iter().all(|s| s.point.seq < seq - 1), "no snapshot at or after {seq}: {snaps:#?}");
    invariant_break(&f, f.main, seq);
    both_fail("m-invariant-late", &f, "main: replay failed", false);
    let v = verify(&store(&f.path));
    assert!(has(&v.failures, &format!("transaction {seq}")), "{:#?}", v.lines);
}

#[test]
fn a_branch_snapshot_that_differs_from_the_replay_is_found() {
    let f = build("b-snapshot");
    let snaps = snapshots_of(&f.path, f.branch);
    assert!(snaps.len() >= 2, "{snaps:#?}");
    let target = swap_snapshot_chunks(&f.path, f.branch, 1, 0);
    both_fail("b-snapshot", &f, &format!("{BRANCH}: snapshot {:?} at {} DIFFERS from the replay", target.name, target.point.seq), false);
}

#[test]
fn a_missing_or_wrong_chunk_of_a_branch_snapshot_is_found() {
    let f = build("b-chunk-missing");
    let h = own_chunk(&f.path, f.branch, f.main);
    write(&f.path, |w| {
        w.open_table(schema::CHUNKS).unwrap().remove(h).unwrap();
    });
    both_fail("b-chunk-missing", &f, "is missing", true);
    assert!(has(&verify(&store(&f.path)).failures, BRANCH), "names the branch");
    let f = build("b-chunk-bad");
    let h = own_chunk(&f.path, f.branch, f.main);
    write(&f.path, |w| {
        let mut v = vec![0u8];
        v.extend_from_slice(br#"{"a":"different"}"#);
        w.open_table(schema::CHUNKS).unwrap().insert(h, v.as_slice()).unwrap();
    });
    both_fail("b-chunk-bad", &f, "does not match its hash", true);
}

#[test]
fn a_bad_merge_row_on_a_branch_is_found() {
    let f = build("b-merge");
    merge_row(&f.path, f.branch, f.main, 4);
    both_fail("b-merge", &f, &format!("{BRANCH}: the merge row at 4"), true);
}

#[test]
fn a_valid_rewrite_between_two_branch_snapshots_is_caught_at_the_next_one() {
    let f = build("b-rewrite");
    let snaps = snapshots_of(&f.path, f.branch);
    let next = snaps.iter().find(|s| s.point.seq > f.note_at).cloned().unwrap();
    let mut log = log_of(&f.path, f.branch);
    let i = (f.note_at - 1) as usize;
    log[i].ops = vec![Op::SetProperty { object: ObjectRef::Project, key: NOTE.into(), before: None, after: Some("second".into()) }];
    rewrite(&f.path, f.branch, f.note_at, &log[i..]);
    export(&f, "b-rewrite", &format!("snapshot {:?} at {}", next.name, next.point.seq));
    let c = verify_chain(&store(&f.path));
    assert!(c.ok, "chain PASS: {:#?}", c.lines);
    let v = verify(&store(&f.path));
    assert!(!v.ok);
    assert!(!has(&v.failures, "replay failed"), "replay PASS: {:#?}", v.lines);
    let want = format!("{BRANCH}: snapshot {:?} at {} DIFFERS from the replay", next.name, next.point.seq);
    assert!(v.failures[0].starts_with(&want), "the first failure names the next snapshot: {:#?}", v.lines);
    assert!(v.failures[0].contains(NOTE), "and the first difference: {:#?}", v.lines);
    assert!(has(&reference(&f.path), &format!("snapshot {:?} at {}", next.name, next.point.seq)));
}

#[test]
fn a_valid_rewrite_of_a_snapshots_own_transaction_is_caught_at_that_snapshot() {
    // On the branch: a rewrite on main before the fork would also change the fork's chain value.
    let f = build("b-rewrite-at");
    let snaps = snapshots_of(&f.path, f.branch);
    let mut log = log_of(&f.path, f.branch);
    // An automatic snapshot whose own transaction is a line.
    let at = snaps
        .iter()
        .find(|s| s.kind == VersionKind::Auto && matches!(log[(s.point.seq - 1) as usize].ops.as_slice(), [Op::AddItem { .. }]))
        .cloned()
        .unwrap();
    let i = (at.point.seq - 1) as usize;
    if let [Op::AddItem { item, .. }] = log[i].ops.as_mut_slice() {
        item.kind = ItemKind::Primitive(Primitive::Line { a: Pt::new(-1.0, -1.0), b: Pt::new(-2.0, -2.0) });
    }
    rewrite(&f.path, f.branch, at.point.seq, &log[i..]);
    export(&f, "b-rewrite-at", &format!("snapshot {:?} at {}", at.name, at.point.seq));
    let c = verify_chain(&store(&f.path));
    assert!(c.ok, "{:#?}", c.lines);
    let v = verify(&store(&f.path));
    let want = format!("{BRANCH}: snapshot {:?} at {} DIFFERS from the replay", at.name, at.point.seq);
    assert!(!v.ok && v.failures[0].starts_with(&want), "{:#?}", v.lines);
    assert!(has(&reference(&f.path), &format!("snapshot {:?} at {}", at.name, at.point.seq)));
}

#[test]
fn a_rewrite_on_main_before_a_fork_is_found_by_the_chain_check_at_the_fork() {
    let f = build("m-before-fork");
    let mut log = log_of(&f.path, f.main);
    let i = 29;
    if let [Op::AddItem { item, .. }] = log[i].ops.as_mut_slice() {
        item.kind = ItemKind::Primitive(Primitive::Line { a: Pt::new(-1.0, -1.0), b: Pt::new(-2.0, -2.0) });
    }
    // Main alone re-chained; the branch keeps the old fork chain value.
    rewrite(&f.path, f.main, 30, &log[i..]);
    export(&f, "m-before-fork", "fork point");
    let c = verify_chain(&store(&f.path));
    assert!(!c.ok && has(&c.failures, "fork point"), "{:#?}", c.lines);
    assert!(!verify(&store(&f.path)).ok);
    assert!(!reference(&f.path).is_empty(), "the old Verify finds it too");
}

#[test]
fn verify_never_panics_on_truncated_or_byte_flipped_files() {
    let f = build("hostile");
    let bytes = std::fs::read(&f.path).unwrap();
    let dir = TempDir::new("t029-hostile-cuts");
    let p = dir.path("cut.ecad");
    let step = (bytes.len() / 24).max(1);
    let mut n = 0;
    for cut in (0..bytes.len()).step_by(step) {
        std::fs::write(&p, &bytes[..cut]).unwrap();
        if let Ok(s) = Store::open(&p, false) {
            let _ = verify(&s);
            let _ = verify_chain(&s);
        }
        n += 1;
    }
    for at in (64..bytes.len()).step_by(step) {
        let mut b = bytes.clone();
        b[at] ^= 0xFF;
        std::fs::write(&p, &b).unwrap();
        if let Ok(s) = Store::open(&p, false) {
            let _ = verify(&s);
        }
        n += 1;
    }
    assert!(n > 40);
}

// ------------------------------------------------------------------ FP1, the tag rule on a branch

mod fp1 {
    use super::*;
    use crate::tag_rule_contracts::rule_1;
    use ecad_file::v2::chunks::{META_KEY, ModelMeta};
    use ecad_model::tag::{HighWater, TAG_RULE};

    fn cable(tag: &str) -> Cable {
        let mut properties = ecad_model::doc::Properties::new();
        properties.insert("tag".into(), tag.into());
        Cable { properties, members: Vec::new() }
    }

    /// Main creates `-W01` and `-W02` and saves; a branch forked there deletes `-W02`, then 20
    /// lines and a save (a branch snapshot after the deletion). Every snapshot is then rewritten as
    /// a build before 026 wrote it (rule 1 marks, no rule in the row).
    fn rule_1_branch_file(name: &str) -> (TempDir, PathBuf, BranchId) {
        let dir = TempDir::new(&format!("t029-fp1-{name}"));
        let path = dir.path("p.ecad");
        let mut f = fixture();
        let (w1, w2): (CableId, CableId) = (f.ids.fresh(), f.ids.fresh());
        f.commit("Cables", vec![Op::CreateCable { id: w1, cable: cable("-W01") }, Op::CreateCable { id: w2, cable: cable("-W02") }]);
        let mut proj = ProjectV2::create(Store::create(&path).unwrap(), PROJECT, &f.ledger, None, None, Some(1), WRITER, EVERY).unwrap().0;
        f.ledger.mark_persisted(f.ledger.head()).unwrap();
        let main = proj.branch();
        let at = f.ledger.head();
        let (b, _) = proj.create_branch("Cables gone", main, at, None, None).unwrap();
        let (mut bl, _) = proj.switch_to(&f.ledger, b, SEED ^ 11).unwrap();
        bl.commit(Author::User, "Delete", vec![Op::DeleteCable { id: w2, cable: cable("-W02") }]).unwrap();
        for i in 0..20 {
            let op = line(&mut f.ids, f.page, f.layer, f64::from(i));
            bl.commit(Author::User, "LINE", vec![op]).unwrap();
        }
        proj.save(&mut bl).unwrap();
        let branch_log: Vec<Transaction> = bl.log().to_vec();
        let main_log: Vec<Transaction> = f.ledger.log().to_vec();
        let _ = proj.switch_to(&bl, main, SEED ^ 12).unwrap();
        drop(proj);
        // Rule 1: marks from main's log up to the fork, then the branch's own.
        let store = Store::open(&path, true).unwrap();
        let mut w = store.writer().unwrap();
        let r = store.reader().unwrap();
        for (br, prefix, own) in [(main, Vec::new(), main_log.clone()), (b, main_log[..at as usize].to_vec(), branch_log.clone())] {
            for kind in [VersionKind::User, VersionKind::Revision, VersionKind::Auto] {
                for mut rec in r.versions(br, kind).unwrap() {
                    let mut high = HighWater::new();
                    for tx in prefix.iter().chain(own.iter().take(rec.point.seq as usize)) {
                        rule_1(&tx.ops, &mut high);
                    }
                    let body = serde_json::to_vec(&ModelMeta { tag_high: high }).unwrap();
                    let h = schema::chunk_hash(&body);
                    w.put_chunk(&h, &body).unwrap();
                    rec.chunks.insert(META_KEY.into(), chain::hex(&h));
                    rec.tag_rule = None;
                    w.put_version(br, &rec).unwrap();
                }
            }
        }
        drop(r);
        w.commit().unwrap();
        drop(store);
        (dir, path, b)
    }

    #[test]
    fn a_rule_1_branch_snapshot_after_a_cable_deletion_opens_with_the_deleted_tag_still_counted() {
        let (_dir, path, b) = rule_1_branch_file("open");
        let snaps = snapshots_of(&path, b);
        assert!(!snaps.is_empty() && snaps.iter().all(|s| s.tag_rule.is_none()), "{snaps:#?}");
        let mut o = ProjectV2::open(Store::open(&path, true).unwrap(), 1, WRITER).unwrap();
        let main_high = o.project.base_tag_high().get("W").copied();
        let (_, rep) = o.project.switch_to(&o.ledger, b, 99).unwrap();
        assert!(rep.damage.is_none(), "{rep:?}");
        assert_eq!(o.project.base_tag_high().get("W"), Some(&2), "-W02, deleted on the branch before its rule 1 snapshot, stays used");
        assert_eq!(main_high, Some(2), "main's marks: -W01 and -W02");
        drop(o);
        let v = verify(&Store::open(&path, false).unwrap());
        assert!(v.ok, "{:#?}", v.lines);
        assert!(has(&v.lines, "recomputed on open (rule 1 → 2)"), "{:#?}", v.lines);
        export_file(&path, "fp1-rule1-branch");
    }

    #[test]
    fn a_resave_on_the_branch_writes_the_current_rule() {
        let (_dir, path, b) = rule_1_branch_file("resave");
        let mut o = ProjectV2::open(Store::open(&path, true).unwrap(), 1, WRITER).unwrap();
        let (mut bl, _) = o.project.switch_to(&o.ledger, b, 98).unwrap();
        let at = bl.head();
        bl.commit(Author::User, "Version", vec![Op::NameVersion { name: "Branch again".into(), at, description: None }]).unwrap();
        o.project.save(&mut bl).unwrap();
        drop(o);
        let snap = snapshots_of(&path, b).into_iter().find(|s| s.name == "Branch again").unwrap();
        assert_eq!(snap.tag_rule, Some(TAG_RULE));
        let s = Store::open(&path, false).unwrap();
        let meta: ModelMeta =
            serde_json::from_slice(&s.reader().unwrap().chunk_body(&chain::parse_hex(&snap.chunks[META_KEY]).unwrap()).unwrap()).unwrap();
        assert_eq!(meta.tag_high.get("W"), Some(&2));
        let v = verify(&s);
        assert!(v.ok, "{:#?}", v.lines);
        drop(s);
        export_file(&path, "fp1-resaved");
    }

    fn export_file(p: &Path, case: &str) {
        if let Ok(dir) = std::env::var("ECAD_TESTER029_EXPORT") {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::copy(p, PathBuf::from(dir).join(format!("{case}.ecad"))).unwrap();
        }
    }
}
