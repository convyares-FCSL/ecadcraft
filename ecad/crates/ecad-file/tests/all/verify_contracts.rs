//! Verify (brief 029, design §3, §8): the chain, then the replay with the commit-time check only,
//! then the full check at every snapshot and branch end. It catches everything the old Verify
//! caught (the full check after every transaction, kept here as [`reference`]), and a history
//! rewritten with valid ops between two snapshots is caught at the next snapshot.
#![cfg(not(target_arch = "wasm32"))]

use std::collections::BTreeMap;
use std::path::Path;

use crate::common::*;
use crate::container_v2::TempDir;
use ecad_file::chain::{self, Hash};
use ecad_file::v2::schema::{self, BranchId, VersionKind, VersionRecord};
use ecad_file::v2::verify::{VerifyReport, verify, verify_chain};
use ecad_file::v2::{ProjectV2, Store};
use ecad_model::*;
use redb::ReadableTable;

/// Snapshots every this many transactions in the fixture.
const EVERY: u64 = 16;

fn line_op(ids: &mut IdAllocator, page: PageId, layer: LayerId, x: f64) -> Op {
    let id: ItemId = ids.fresh();
    Op::AddItem {
        id,
        item: Item { page, style: style(layer), kind: ItemKind::Primitive(Primitive::Line { a: Pt::new(x, 0.0), b: Pt::new(x, 9.0) }) },
    }
}

/// The note property set once on main, at transaction [`Fixture2::note_at`].
const NOTE: &str = "verify.note";

struct Fixture2 {
    path: std::path::PathBuf,
    main: BranchId,
    branch: BranchId,
    /// Main's transaction that sets [`NOTE`].
    note_at: u64,
    page: PageId,
    _dir: TempDir,
}

/// Main with lines, undo and redo, a named version, the project property [`NOTE`] set once, and
/// automatic snapshots every [`EVERY`] transactions; a branch forked from main with edits of its
/// own and a snapshot.
fn build(name: &str) -> Fixture2 {
    let dir = TempDir::new(name);
    let mut f = fixture();
    let path = dir.path("p.ecad");
    let mut proj = {
        let store = Store::create(&path).unwrap();
        ProjectV2::create(store, PROJECT, &f.ledger, None, None, Some(1), WRITER, EVERY).unwrap().0
    };
    f.ledger.mark_persisted(f.ledger.head()).unwrap();
    for i in 0..20 {
        let op = line_op(&mut f.ids, f.page, f.layer, 100.0 + f64::from(i));
        f.ledger.commit(Author::User, "LINE", vec![op]).unwrap();
        if i == 4 {
            f.ledger.undo(Author::User).unwrap();
            f.ledger.redo(Author::User).unwrap();
        }
    }
    proj.save(&mut f.ledger).unwrap();
    let note = vec![Op::SetProperty { object: ObjectRef::Project, key: NOTE.into(), before: None, after: Some("first".into()) }];
    f.ledger.commit(Author::User, "Note", note).unwrap();
    let note_at = f.ledger.head();
    for i in 0..20 {
        let op = line_op(&mut f.ids, f.page, f.layer, 200.0 + f64::from(i));
        f.ledger.commit(Author::User, "LINE", vec![op]).unwrap();
    }
    let at = f.ledger.head();
    f.ledger.commit(Author::User, "Version", vec![Op::NameVersion { name: "Issue A".into(), at, description: None }]).unwrap();
    proj.save(&mut f.ledger).unwrap();
    let main = proj.branch();
    let (branch, _) = proj.create_branch("Option B", main, at, None, None).unwrap();
    let (mut bl, _) = proj.switch_to(&f.ledger, branch, SEED ^ 2).unwrap();
    for i in 0..20 {
        let op = line_op(&mut f.ids, f.page, f.layer, 300.0 + f64::from(i));
        bl.commit(Author::User, "LINE", vec![op]).unwrap();
    }
    proj.save(&mut bl).unwrap();
    let (_, _) = proj.switch_to(&bl, main, SEED ^ 3).unwrap();
    drop(proj);
    Fixture2 { path, main, branch, note_at, page: f.page, _dir: dir }
}

fn store(p: &Path) -> Store {
    Store::open(p, false).unwrap()
}

// ------------------------------------------------------------------ the old Verify, as reference

/// The old Verify's verdict (before brief 029): every branch's rows decoded against the chain,
/// replayed with **the full check after every transaction**
/// ([`Ledger::replay_verified_from`]), then every snapshot compared with a forward replay, and
/// every chunk read against its hash. Its failures, one line each, naming the branch and the
/// transaction or snapshot.
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
        let (start, seed) = match line[0].fork {
            Some((pi, fseq)) => match ecad_file::v2::project::line_state(&r, &line, pi, fseq) {
                Ok((d, _)) => (d, line[0].seed),
                Err(e) => {
                    out.push(format!("{name}: fork: {e}"));
                    continue;
                }
            },
            None => (Document::default(), line[0].seed),
        };
        let last = r.last_seq(id).unwrap().unwrap_or(0);
        let mut prev = seed;
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
        let mut versions: Vec<VersionRecord> = Vec::new();
        for kind in [VersionKind::User, VersionKind::Revision, VersionKind::Auto] {
            versions.extend(r.versions(id, kind).unwrap());
        }
        versions.sort_by_key(|v| v.point.seq);
        let mut doc = start;
        let mut done = 0u64;
        for v in &versions {
            for tx in ledger.log().iter().skip(done as usize).take((v.point.seq - done) as usize) {
                for op in &tx.ops {
                    op.apply(&mut doc).unwrap();
                }
            }
            done = v.point.seq;
            let mut bodies = Vec::new();
            let set = ecad_file::v2::chunks::ChunkSet::from_record(&v.chunks).unwrap();
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

// ------------------------------------------------------------------ tampering, straight into the tables

fn write(p: &Path, f: impl FnOnce(&redb::WriteTransaction)) {
    let db = redb::Database::open(p).unwrap();
    let w = db.begin_write().unwrap();
    f(&w);
    w.commit().unwrap();
}

fn branch_rec(p: &Path, b: BranchId) -> schema::BranchRecord {
    store(p).reader().unwrap().branch(b).unwrap().unwrap()
}

/// Rewrite `branch`'s rows from `from` on as `txs` (same seqs): each record re-encoded on the
/// recomputed chain, then the head and every version and merge row's chain value at or after
/// `from` recomputed, so the chain checks pass. Branches forked from `branch` at or after `from`
/// get the new fork chain value and are re-chained from their new seed in turn.
fn rewrite(p: &Path, branch: BranchId, from: u64, txs: &[Transaction]) {
    // The children's logs, read while the file still opens.
    let parent_hex = branch.hex();
    let children: Vec<(BranchId, u64, Vec<Transaction>)> = store(p)
        .reader()
        .unwrap()
        .branches()
        .unwrap()
        .into_iter()
        .filter(|(_, b)| b.parent.as_deref() == Some(parent_hex.as_str()))
        .filter_map(|(id, b)| b.fork_point.filter(|fp| fp.seq >= from).map(|fp| (id, fp.seq)))
        .map(|(id, seq)| (id, seq, log_of(p, id)))
        .collect();
    let chains = rewrite_rows(p, branch, from, txs, None, |_| {});
    for (child, fork, log) in children {
        let fork_chain = chains[&fork];
        let seed = chain::branch_seed(PROJECT.0, &child.0, &fork_chain);
        rewrite_rows(p, child, 1, &log, Some(seed), |rec| rec.fork_point = Some(schema::Point::new(fork, &fork_chain)));
    }
}

/// [`rewrite`] for one branch; its new chain values by seq.
fn rewrite_rows(
    p: &Path, branch: BranchId, from: u64, txs: &[Transaction], seed: Option<Hash>, edit: impl FnOnce(&mut schema::BranchRecord),
) -> BTreeMap<u64, Hash> {
    let s = store(p);
    let r = s.reader().unwrap();
    let mut prev = match seed {
        Some(seed) => seed,
        None if from == 1 => ecad_file::v2::project::lineage(&r, PROJECT, branch).unwrap()[0].seed,
        None => r.tx(branch, from - 1).unwrap().unwrap().1,
    };
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
    let mut rec = r.branch(branch).unwrap().unwrap();
    rec.head = schema::Point::new(rec.head.seq, &chains[&rec.head.seq]);
    edit(&mut rec);
    let mut versions: Vec<(Vec<u8>, VersionRecord)> = Vec::new();
    for kind in [VersionKind::User, VersionKind::Revision, VersionKind::Auto, VersionKind::Merge] {
        for mut v in r.versions(branch, kind).unwrap() {
            if let Some(h) = chains.get(&v.point.seq) {
                v.point = schema::Point::new(v.point.seq, h);
                versions.push((schema::version_key(branch, kind, &v.name), v));
            }
        }
    }
    drop(r);
    drop(s);
    write(p, |w| {
        let mut t = w.open_table(schema::TRANSACTIONS).unwrap();
        for (seq, row) in &rows {
            t.insert(schema::tx_key(branch, *seq), row.as_slice()).unwrap();
        }
        let mut b = w.open_table(schema::BRANCHES).unwrap();
        b.insert(branch.0, serde_json::to_vec(&rec).unwrap().as_slice()).unwrap();
        let mut vt = w.open_table(schema::VERSIONS).unwrap();
        for (k, v) in &versions {
            vt.insert(k.as_slice(), serde_json::to_vec(v).unwrap().as_slice()).unwrap();
        }
    });
    chains
}

/// `branch`'s whole log, decoded.
fn log_of(p: &Path, branch: BranchId) -> Vec<Transaction> {
    let o = ProjectV2::open(store(p), 1, WRITER).unwrap();
    let last = o.project.store().reader().unwrap().last_seq(branch).unwrap().unwrap();
    o.project.tx_display(branch, 1, last).unwrap()
}

/// The snapshots of `branch` (named versions and automatic ones), in point order.
fn snapshots_of(p: &Path, branch: BranchId) -> Vec<VersionRecord> {
    let r = store(p).reader().unwrap();
    let mut v = Vec::new();
    for kind in [VersionKind::User, VersionKind::Revision, VersionKind::Auto] {
        v.extend(r.versions(branch, kind).unwrap());
    }
    v.sort_by_key(|v| v.point.seq);
    v
}

// ------------------------------------------------------------------ the cases

/// One way to damage the fixture, and what both Verifies must name.
struct Case {
    name: &'static str,
    damage: fn(&Fixture2),
    /// A failure line of both the old and the new Verify contains this.
    names: fn(&Fixture2) -> String,
    /// The chain check alone finds it.
    chain: bool,
}

fn flip_row_byte(f: &Fixture2) {
    write(&f.path, |w| {
        let mut t = w.open_table(schema::TRANSACTIONS).unwrap();
        let k = schema::tx_key(f.main, 5);
        let mut v = t.get(k).unwrap().unwrap().value().to_vec();
        let last = v.len() - 1;
        v[last] ^= 0x01;
        t.insert(k, v.as_slice()).unwrap();
    });
}

fn remove_row(f: &Fixture2) {
    write(&f.path, |w| {
        w.open_table(schema::TRANSACTIONS).unwrap().remove(schema::tx_key(f.main, 7)).unwrap();
    });
}

fn wrong_head(f: &Fixture2) {
    let mut rec = branch_rec(&f.path, f.main);
    let r = store(&f.path);
    let c = r.reader().unwrap().tx(f.main, rec.head.seq - 1).unwrap().unwrap().1;
    drop(r);
    rec.head = schema::Point::new(rec.head.seq - 1, &c);
    write(&f.path, |w| {
        w.open_table(schema::BRANCHES).unwrap().insert(f.main.0, serde_json::to_vec(&rec).unwrap().as_slice()).unwrap();
    });
}

/// Transaction 8 of main rewritten to add a line on a layer that does not exist (I3), with a
/// correct chain from there on.
fn invariant_break(f: &Fixture2) {
    let mut log = log_of(&f.path, f.main);
    let mut ids = IdAllocator::seeded(0xBAD);
    let ghost: LayerId = ids.fresh();
    let tx = &mut log[7];
    assert_eq!(tx.seq, 8);
    tx.ops = vec![line_op(&mut ids, f.page, ghost, 999.0)];
    tx.kind = TxKind::Edit;
    rewrite(&f.path, f.main, 8, &log[7..]);
}

/// Main's third snapshot points at the second one's chunks.
fn snapshot_differs(f: &Fixture2) {
    let a = snapshots_of(&f.path, f.main);
    assert!(a.len() >= 3, "{} snapshots", a.len());
    let mut second = a[2].clone();
    second.chunks = a[1].chunks.clone();
    write(&f.path, |w| {
        let k = schema::version_key(f.main, second.kind, &second.name);
        w.open_table(schema::VERSIONS).unwrap().insert(k.as_slice(), serde_json::to_vec(&second).unwrap().as_slice()).unwrap();
    });
}

fn a_chunk(f: &Fixture2) -> [u8; 32] {
    let a = snapshots_of(&f.path, f.main);
    chain::parse_hex(a[1].chunks.values().next().unwrap()).unwrap()
}

fn missing_chunk(f: &Fixture2) {
    let h = a_chunk(f);
    write(&f.path, |w| {
        w.open_table(schema::CHUNKS).unwrap().remove(h).unwrap();
    });
}

fn chunk_not_its_hash(f: &Fixture2) {
    let h = a_chunk(f);
    write(&f.path, |w| {
        let body = br#"{"not":"the chunk"}"#;
        let mut v = vec![0u8];
        v.extend_from_slice(body);
        w.open_table(schema::CHUNKS).unwrap().insert(h, v.as_slice()).unwrap();
    });
}

/// A merge row at main's transaction 3 with a chain value the log does not have.
fn merge_row_chain(f: &Fixture2) {
    let rec = VersionRecord {
        kind: VersionKind::Merge,
        name: schema::auto_name(3),
        point: schema::Point::new(3, &[7; 32]),
        chunks: Default::default(),
        release: None,
        undo: Vec::new(),
        redo: Vec::new(),
        undo_hidden: 0,
        redo_hidden: 0,
        stack_cap: 0,
        description: None,
        merge: schema::MergeRow::of(&TxKind::Merge {
            source: f.branch.0,
            source_head: StepPoint { seq: 1, chain: [1; 32] },
            ancestor: BranchPoint { branch: f.main.0, seq: 0, chain: [2; 32] },
        }),
        tag_rule: None,
        extra: Default::default(),
    };
    write(&f.path, |w| {
        let k = schema::version_key(f.main, VersionKind::Merge, &rec.name);
        w.open_table(schema::VERSIONS).unwrap().insert(k.as_slice(), serde_json::to_vec(&rec).unwrap().as_slice()).unwrap();
    });
}

fn cases() -> Vec<Case> {
    vec![
        Case { name: "a flipped row byte", damage: flip_row_byte, names: |_| "main: transaction 5".into(), chain: true },
        Case { name: "a missing row", damage: remove_row, names: |_| "main: transaction 7 is missing".into(), chain: true },
        Case { name: "a wrong head", damage: wrong_head, names: |_| "main: the head".into(), chain: true },
        Case { name: "an invariant break mid-history", damage: invariant_break, names: |_| "main: replay failed".into(), chain: false },
        Case {
            name: "a snapshot that differs from the replay",
            damage: snapshot_differs,
            names: |f| {
                let a = snapshots_of(&f.path, f.main);
                format!("main: snapshot {:?} at {} DIFFERS from the replay", a[2].name, a[2].point.seq)
            },
            chain: false,
        },
        Case { name: "a missing chunk", damage: missing_chunk, names: |_| "is missing".into(), chain: true },
        Case { name: "a chunk not matching its hash", damage: chunk_not_its_hash, names: |_| "does not match its hash".into(), chain: true },
        Case {
            name: "a merge row whose chain value differs",
            damage: merge_row_chain,
            names: |_| "main: the merge row at 3: its chain value differs from the log's".into(),
            chain: true,
        },
    ]
}

fn has(lines: &[String], want: &str) -> bool {
    lines.iter().any(|l| l.contains(want))
}

#[test]
fn verify_passes_the_clean_fixture_with_one_full_check_per_snapshot_point_and_branch_end() {
    let f = build("verify-clean");
    assert!(reference(&f.path).is_empty(), "{:#?}", reference(&f.path));
    let c = verify_chain(&store(&f.path));
    assert!(c.ok, "{:#?}", c.lines);
    assert_eq!((c.snapshots, c.full_checks), (0, 0), "the chain check replays nothing");
    let v = verify(&store(&f.path));
    assert!(v.ok, "{:#?}", v.lines);
    assert!(v.failures.is_empty());
    // Verify mode stays out: this test build runs with `verify-invariants` on, yet Verify runs
    // the full check only at snapshot points and branch ends.
    assert!(cfg!(test) && ecad_model::ledger::verify_mode(), "every cargo test runs in verify mode");
    let r = store(&f.path).reader().unwrap();
    let mut points = std::collections::BTreeSet::new();
    let mut txs = 0;
    for b in [f.main, f.branch] {
        txs += r.last_seq(b).unwrap().unwrap();
        for kind in [VersionKind::User, VersionKind::Revision, VersionKind::Auto] {
            for v in r.versions(b, kind).unwrap() {
                points.insert((b, v.point.seq));
            }
        }
    }
    assert!(v.snapshots >= 4, "{} snapshots", v.snapshots);
    assert_eq!(v.full_checks, points.len() + 2, "one per snapshot point, plus each branch's end");
    assert!((v.full_checks as u64) < txs, "{} full checks, {txs} transactions", v.full_checks);
}

#[test]
fn verify_catches_what_the_old_verify_caught_naming_the_same_place() {
    for case in cases() {
        let f = build(&format!("verify-{}", case.name.replace(' ', "-")));
        (case.damage)(&f);
        let want = (case.names)(&f);
        let old = reference(&f.path);
        let new = verify(&store(&f.path));
        assert!(has(&old, &want), "{}: the old Verify names {want:?}: {old:#?}", case.name);
        assert!(!new.ok && has(&new.failures, &want), "{}: the new Verify names {want:?}: {:#?}", case.name, new.lines);
        let chain = verify_chain(&store(&f.path));
        if case.chain {
            assert!(!chain.ok && has(&chain.failures, &want), "{}: the chain check names {want:?}: {:#?}", case.name, chain.lines);
        } else {
            assert!(chain.ok, "{}: the chain check passes; the replay or a snapshot finds it: {:#?}", case.name, chain.lines);
        }
    }
}

#[test]
fn an_invariant_break_names_its_transaction_with_the_invariants_own_message() {
    let f = build("verify-invariant-message");
    invariant_break(&f);
    let v = verify(&store(&f.path));
    let line = v.failures.iter().find(|l| l.contains("replay failed")).cloned().unwrap_or_default();
    assert!(line.contains("transaction 8") && line.to_lowercase().contains("layer"), "{:#?}", v.lines);
}

#[test]
fn a_valid_rewrite_between_snapshots_passes_chain_and_replay_and_fails_at_the_next_snapshot() {
    let f = build("verify-rewrite");
    let a = snapshots_of(&f.path, f.main);
    let next = a.iter().find(|v| v.point.seq > f.note_at).cloned().unwrap();
    let before = a.iter().rev().find(|v| v.point.seq < f.note_at).cloned().unwrap();
    assert!(before.point.seq < f.note_at && f.note_at < next.point.seq);
    // The note set to another value: the ops apply cleanly, every chain value is recomputed.
    let mut log = log_of(&f.path, f.main);
    let i = (f.note_at - 1) as usize;
    log[i].ops = vec![Op::SetProperty { object: ObjectRef::Project, key: NOTE.into(), before: None, after: Some("second".into()) }];
    rewrite(&f.path, f.main, f.note_at, &log[i..]);
    let chain = verify_chain(&store(&f.path));
    assert!(chain.ok, "chain PASS: {:#?}", chain.lines);
    let v = verify(&store(&f.path));
    assert!(!v.ok);
    assert!(!has(&v.failures, "replay failed"), "replay PASS: {:#?}", v.lines);
    let first = &v.failures[0];
    let want = format!("main: snapshot {:?} at {} DIFFERS from the replay: project 0, property `{NOTE}`", next.name, next.point.seq);
    assert_eq!(first, &want, "{:#?}", v.lines);
    let old = reference(&f.path);
    assert!(has(&old, &format!("snapshot {:?} at {}", next.name, next.point.seq)), "the old Verify catches it at the same snapshot: {old:#?}");
}

#[test]
fn a_branch_starts_from_its_parents_replayed_state_at_the_fork() {
    let f = build("verify-fork");
    let v = verify(&store(&f.path));
    assert!(v.ok, "{:#?}", v.lines);
    assert!(has(&v.lines, "Option B: 20 transactions replay from the fork point with the commit check"), "{:#?}", v.lines);
    // Damage on main before the fork fails main, and the branch is still checked.
    invariant_break(&f);
    let v = verify(&store(&f.path));
    assert!(has(&v.failures, "main: replay failed"), "{:#?}", v.lines);
}

#[test]
fn verify_report_lists_its_failures_apart() {
    let rep = VerifyReport::default();
    assert!(rep.failures.is_empty() && !rep.ok);
}

/// Reviewer 029: the inspector stops the Verify of a file it replaced.
#[test]
fn verify_stops_when_asked() {
    let f = build("verify-stop");
    let s = store(&f.path);
    assert_eq!(ecad_file::v2::verify::verify_stoppable(&s, &|| true), None);
    let asked = std::cell::Cell::new(0u32);
    let r = ecad_file::v2::verify::verify_stoppable(&s, &|| {
        asked.set(asked.get() + 1);
        false
    });
    assert!(r.is_some_and(|r| r.ok) && asked.get() > 2, "asked {} times", asked.get());
}
