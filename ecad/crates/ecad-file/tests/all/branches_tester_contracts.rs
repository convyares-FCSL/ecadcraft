//! Tester contracts for brief 020a's store work (EEP-0005 rev 8, design 020 §3, §7, §17):
//!
//! - **Power loss across branch actions:** a sequence of saves, branch creates, switches
//!   (`meta.active`), renames, a merge saved with Close branch (brief 020b: the merge index row
//!   and the source's `merged` status in the same write) and archives is cut at any disk
//!   operation and the disk image
//!   crashed; the file reopens undamaged on a consistent branch, with every acknowledged step and
//!   at most the step in flight, and verifies.
//! - **An older build that only knows `main`** saves main (leaving `meta.active` and the branch
//!   rows alone, and possibly dropping the branch's `cache.latest` entry): this build reopens on
//!   the branch, both lines are intact, and the file verifies.
//! - **A second reader** opened before a branch existed shows it afterwards.
#![cfg(not(target_arch = "wasm32"))]

use std::collections::BTreeMap;
use std::time::Instant;

use crate::common::*;
use crate::container_v2::TempDir;
use ecad_file::v2::schema::{BranchId, BranchStatus};
use ecad_file::v2::{ProjectV2, Store};
use ecad_model::*;
use ecad_store::power::{CrashMode, PowerLoss, Rng};
use redb::ReadableDatabase;

type Result<T, E = String> = std::result::Result<T, E>;

fn line(l: &mut Ledger, rng: &mut Rng) {
    let doc = l.document();
    let page = doc.page_order[0];
    let layer = *doc.layers.keys().next().unwrap();
    let id = ItemId(u128::from(rng.next_u64()) << 64 | u128::from(rng.next_u64()));
    let x = (rng.below(1000) as f64) / 10.0;
    let item = Item { page, style: style(layer), kind: ItemKind::Primitive(Primitive::Line { a: Pt::new(x, 0.0), b: Pt::new(x, 7.0) }) };
    l.commit(Author::User, "LINE", vec![Op::AddItem { id, item }]).unwrap();
}

/// What the file must show after a step is acknowledged: the active branch's name, and every
/// branch's (name → (status, document at its head)).
#[derive(Clone, Debug)]
struct Expect {
    active: String,
    branches: BTreeMap<String, (BranchStatus, Document)>,
}

/// One round: the scripted branch session on a simulated disk, power cut after `cut` operations
/// (or never). Returns the number of disk operations the session used, or the failure.
fn branch_round(seed: u64, mode: CrashMode, cut: Option<u64>) -> Result<(u64, &'static str), String> {
    let mut rng = Rng::new(seed);
    let disk = PowerLoss::default();
    let mut f = fixture();
    let store = Store::with_backend(disk.clone()).map_err(|e| e.to_string())?;
    let (mut proj, _) = ProjectV2::create(store, PROJECT, &f.ledger, None, None, None, "crash", 16).map_err(|e| e.to_string())?;
    f.ledger.mark_persisted(f.ledger.head()).unwrap();
    if let Some(c) = cut {
        disk.cut_after(c);
    }
    let start = disk.ops();
    let main = proj.branch();
    let mut ledger = f.ledger;
    let mut exp = Expect { active: "main".into(), branches: BTreeMap::new() };
    exp.branches.insert("main".into(), (BranchStatus::Open, ledger.document().clone()));
    let mut acked: Vec<Expect> = vec![exp.clone()];
    let mut ids: BTreeMap<String, BranchId> = BTreeMap::from([("main".to_string(), main)]);
    let mut current = "main".to_string();
    let mut in_flight: Option<Expect>;

    // Each step: work out the expected state if it completes, run it, acknowledge it on success.
    macro_rules! step {
        ($next:expr, $run:expr) => {{
            in_flight = Some($next.clone());
            match $run {
                Ok(_) => {
                    exp = $next;
                    acked.push(exp.clone());
                }
                Err(_) => break,
            }
        }};
    }

    #[allow(clippy::never_loop)]
    loop {
        // 1. Edits on main, saved.
        for _ in 0..1 + rng.below(20) {
            line(&mut ledger, &mut rng);
        }
        let mut next = exp.clone();
        next.branches.insert("main".into(), (BranchStatus::Open, ledger.document().clone()));
        step!(next, proj.save(&mut ledger).map_err(|e| e.to_string()));
        // 2. A branch from main's head (or an older point).
        let back = rng.below(3).min(ledger.head().saturating_sub(1));
        let at = ledger.head() - back;
        let at_doc = ledger.state_at(at).unwrap();
        let mut next = exp.clone();
        next.branches.insert("B".into(), (BranchStatus::Open, at_doc.clone()));
        let mut made = None;
        step!(
            next,
            proj.create_branch("B", main, at, Some(1), None).map(|(id, _)| {
                made = Some(id);
            })
        );
        ids.insert("B".into(), made.unwrap());
        // 3. Switch to B (meta.active).
        let mut next = exp.clone();
        next.active = "B".into();
        let mut bl = None;
        step!(next, proj.switch_to(&ledger, ids["B"], seed).map(|(l, _)| bl = Some(l)));
        ledger = bl.take().unwrap();
        current = "B".into();
        // 4. Edits on B and a version, saved (snapshots every 16 steps).
        for _ in 0..1 + rng.below(30) {
            line(&mut ledger, &mut rng);
        }
        if rng.below(2) == 0 {
            let at = ledger.head();
            ledger.commit(Author::User, "Version", vec![Op::NameVersion { name: format!("B{at}"), at, description: Some("d".into()) }]).unwrap();
        }
        let mut next = exp.clone();
        next.branches.insert("B".into(), (BranchStatus::Open, ledger.document().clone()));
        step!(next, proj.save(&mut ledger).map_err(|e| e.to_string()));
        // 5. Rename B to C.
        let mut next = exp.clone();
        let b = next.branches.remove("B").unwrap();
        next.branches.insert("C".into(), b);
        next.active = "C".into();
        step!(next, proj.rename_branch(ids["B"], "C"));
        ids.insert("C".into(), ids["B"]);
        current = "C".into();
        // 6. Switch back to main.
        let mut next = exp.clone();
        next.active = "main".into();
        let mut ml = None;
        step!(next, proj.switch_to(&ledger, main, seed ^ 1).map(|(l, _)| ml = Some(l)));
        ledger = ml.take().unwrap();
        current = "main".into();
        // 6b. Merge C into main (brief 020b): one merge transaction that closes C, saved.
        let c_rec = proj.store().reader().map_err(|e| e.to_string())?.branch(ids["C"]).map_err(|e| e.to_string())?.ok_or("no C")?;
        let kind = TxKind::Merge {
            source: ids["C"].0,
            source_head: StepPoint { seq: c_rec.head.seq, chain: c_rec.head.hash()? },
            ancestor: BranchPoint { branch: main.0, seq: 0, chain: proj.chain_of(main, 0)? },
        };
        let page = ledger.document().page_order[0];
        let layer = *ledger.document().layers.keys().next().unwrap();
        let id = ItemId(u128::from(rng.next_u64()) << 64 | 7);
        let item = Item { page, style: style(layer), kind: ItemKind::Primitive(Primitive::Line { a: Pt::new(1.0, 1.0), b: Pt::new(2.0, 8.0) }) };
        let tx = ledger.commit_merge(Author::User, "Merge \"C\"", kind, vec![Op::AddItem { id, item }], None).unwrap();
        proj.close_on_save(tx, ids["C"]);
        let mut next = exp.clone();
        next.branches.insert("main".into(), (BranchStatus::Open, ledger.document().clone()));
        next.branches.get_mut("C").unwrap().0 = BranchStatus::Merged;
        step!(next, proj.save(&mut ledger).map_err(|e| e.to_string()));
        // 7. Archive C.
        let mut next = exp.clone();
        next.branches.get_mut("C").unwrap().0 = BranchStatus::Archived;
        step!(next, proj.set_branch_open(ids["C"], false));
        // 8. More on main, saved.
        for _ in 0..1 + rng.below(5) {
            line(&mut ledger, &mut rng);
        }
        let mut next = exp.clone();
        next.branches.insert("main".into(), (BranchStatus::Open, ledger.document().clone()));
        step!(next, proj.save(&mut ledger).map_err(|e| e.to_string()));
        in_flight = None;
        break;
    }
    let _ = current;
    let ops = disk.ops() - start;
    drop(proj);
    if cut.is_none() && in_flight.is_some() {
        return Err("a step failed without a power cut".into());
    }
    let image = disk.crash(&mut rng, mode);
    let store = Store::with_backend(PowerLoss::from_image(image)).map_err(|e| format!("reopen: {e}"))?;
    let o = ProjectV2::open(store, 1, "crash").map_err(|e| format!("open: {e}"))?;
    if let Some(d) = &o.report.damage {
        return Err(format!("damage after the cut: {d:?}"));
    }
    let found = observe(&o).map_err(|e| format!("observe: {e}"))?;
    let v = ecad_file::v2::verify::verify(o.project.store());
    if !v.ok {
        return Err(format!("verify: {:?}", v.lines));
    }
    if same(&found, &exp) {
        return Ok((ops, if cut.is_none() || in_flight.is_none() { "all steps done" } else { "cut: last acknowledged step" }));
    }
    if let Some(next) = &in_flight
        && same(&found, next)
    {
        return Ok((ops, "cut: the step in flight landed"));
    }
    Err(format!(
        "reopened as {:?} on {}; acknowledged {} steps, last {:?}; in flight {:?}",
        found.branches.iter().map(|(k, (s, d))| (k, s, d.items.len())).collect::<Vec<_>>(),
        found.active,
        acked.len(),
        exp.branches.iter().map(|(k, (s, d))| (k, s, d.items.len())).collect::<Vec<_>>(),
        in_flight
            .as_ref()
            .map(|n| (n.active.clone(), n.branches.iter().map(|(k, (s, d))| (k.clone(), s.clone(), d.items.len())).collect::<Vec<_>>())),
    ))
}

fn observe(o: &ecad_file::v2::OpenedV2) -> Result<Expect, String> {
    let mut branches = BTreeMap::new();
    for (id, rec) in o.project.branches().map_err(|e| e.to_string())? {
        let doc = o.project.point_state(id, rec.head.seq)?;
        branches.insert(rec.name.clone(), (rec.status, doc));
    }
    let active = o.project.branch_record().name.clone();
    if branches.get(&active).map(|(_, d)| d) != Some(o.ledger.document()) {
        return Err(format!("the shown branch {active}'s document is not its head state"));
    }
    Ok(Expect { active, branches })
}

fn same(a: &Expect, b: &Expect) -> bool {
    a.active == b.active && a.branches == b.branches
}

fn run_branch_rounds(name: &str, n: u64, mode: CrashMode) {
    let started = Instant::now();
    let mut failures = Vec::new();
    let mut outcomes: BTreeMap<&str, u64> = BTreeMap::new();
    for round in 0..n {
        let seed = 0x0B2A_0000 + round;
        let r = branch_round(seed, mode, None).and_then(|(ops, _)| {
            let pick = Rng::new(seed ^ 0xC0FFEE).next_u64() % (ops + 1);
            branch_round(seed, mode, Some(pick))
        });
        match r {
            Ok((_, o)) => *outcomes.entry(o).or_default() += 1,
            Err(e) => failures.push(format!("round {round} (seed {seed:#x}): {e}")),
        }
    }
    eprintln!("{name}: {n} rounds, {} failures, {:.1} s; {outcomes:?}", failures.len(), started.elapsed().as_secs_f64());
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn power_loss_during_branch_actions_reopens_consistently_quick() {
    run_branch_rounds("branch power loss (quick)", 8, CrashMode::Reordered);
}

#[test]
#[ignore = "slow: hundreds of simulated power cuts across branch actions"]
fn power_loss_during_branch_actions_reopens_consistently() {
    let n = std::env::var("ECAD_CRASH_ROUNDS").ok().and_then(|v| v.parse().ok()).unwrap_or(200);
    run_branch_rounds("branch power loss", n, CrashMode::Reordered);
    run_branch_rounds("branch torn write", n, CrashMode::TornLast);
}

// ------------------------------------------------------------------ an older build saves main

/// Raw `meta` access, as an older build sees the table.
fn meta_get(path: &std::path::Path, name: &str) -> Option<Vec<u8>> {
    let db = redb::Database::open(path).unwrap();
    let r = db.begin_read().unwrap();
    let t = r.open_table(ecad_file::v2::schema::META).unwrap();
    t.get(name).unwrap().map(|v| v.value().to_vec())
}

fn meta_put(path: &std::path::Path, name: &str, value: Option<&[u8]>) {
    let db = redb::Database::open(path).unwrap();
    let w = db.begin_write().unwrap();
    {
        let mut t = w.open_table(ecad_file::v2::schema::META).unwrap();
        match value {
            Some(v) => {
                t.insert(name, v).unwrap();
            }
            None => {
                t.remove(name).unwrap();
            }
        }
    }
    w.commit().unwrap();
}

#[test]
fn a_save_on_main_by_a_build_that_only_knows_main_leaves_the_branch_consistent() {
    for drop_branch_cache in [false, true] {
        let dir = TempDir::new("oldbuild");
        let mut f = fixture();
        let p = dir.path("p.ecad");
        let store = Store::create(&p).unwrap();
        let (mut proj, _) = ProjectV2::create(store, PROJECT, &f.ledger, None, None, Some(1), WRITER, 16).unwrap();
        f.ledger.mark_persisted(f.ledger.head()).unwrap();
        let mut rng = Rng::new(5);
        let main = proj.branch();
        for _ in 0..5 {
            line(&mut f.ledger, &mut rng);
        }
        proj.save(&mut f.ledger).unwrap();
        let fork = f.ledger.head() - 2;
        let fork_doc = f.ledger.state_at(fork).unwrap();
        let (b, _) = proj.create_branch("Option B", main, fork, None, None).unwrap();
        let (mut bl, _) = proj.switch_to(&f.ledger, b, 3).unwrap();
        for _ in 0..25 {
            line(&mut bl, &mut rng);
        }
        proj.save(&mut bl).unwrap();
        let b_doc = bl.document().clone();
        let b_states: Vec<Document> = (0..=bl.head()).map(|s| bl.state_at(s).unwrap()).collect();
        drop(proj);
        // The older build: it never reads `meta.active` (so it opens main), saves main, and
        // writes back the cache as it knows it.
        let active = meta_get(&p, "active").expect("switching wrote meta.active");
        meta_put(&p, "active", None);
        let o = ProjectV2::open(Store::open(&p, true).unwrap(), 9, "older build").unwrap();
        assert_eq!(o.project.branch(), main);
        let (mut old, mut ml) = (o.project, o.ledger);
        for _ in 0..20 {
            line(&mut ml, &mut rng);
        }
        old.save(&mut ml).unwrap();
        let main_doc = ml.document().clone();
        drop(old);
        meta_put(&p, "active", Some(&active));
        if drop_branch_cache {
            let raw = meta_get(&p, "cache").unwrap();
            let mut v: serde_json::Value = serde_json::from_slice(&raw).unwrap();
            v["latest"].as_object_mut().unwrap().retain(|k, _| *k == main.hex());
            meta_put(&p, "cache", Some(&serde_json::to_vec(&v).unwrap()));
        }
        // This build: reopens on the branch, both lines intact.
        let o = ProjectV2::open(Store::open(&p, true).unwrap(), 11, WRITER).unwrap();
        assert!(o.report.damage.is_none(), "drop cache {drop_branch_cache}: {:?}", o.report);
        assert_eq!(o.project.branch(), b, "reopens on the last-used branch");
        assert_eq!(o.ledger.document(), &b_doc);
        for (s, d) in b_states.iter().enumerate() {
            assert_eq!(&o.ledger.state_at(s as u64).unwrap(), d, "state_at {s}");
        }
        assert_eq!(o.project.point_state(main, fork).unwrap(), fork_doc);
        let v = ecad_file::v2::verify::verify(o.project.store());
        assert!(v.ok, "drop cache {drop_branch_cache}: {:#?}", v.lines);
        let mut proj = o.project;
        let (ml2, _) = proj.switch_to(&o.ledger, main, 4).unwrap();
        assert_eq!(ml2.document(), &main_doc, "the older build's save on main is there");
        // And the branch still saves on top.
        let (mut bl2, _) = proj.switch_to(&ml2, b, 5).unwrap();
        line(&mut bl2, &mut rng);
        proj.save(&mut bl2).unwrap();
        drop(proj);
        let v = ecad_file::v2::verify::verify(&Store::open(&p, false).unwrap());
        assert!(v.ok, "{:#?}", v.lines);
    }
}

/// `meta.active` naming a branch that is gone, archived, or not a branch ID at all: the file
/// opens on main, undamaged (an older build or a hand edit may leave any of these).
#[test]
fn a_stale_or_odd_meta_active_opens_on_main() {
    let dir = TempDir::new("active");
    let f = fixture();
    let p = dir.path("p.ecad");
    let store = Store::create(&p).unwrap();
    let (mut proj, _) = ProjectV2::create(store, PROJECT, &f.ledger, None, None, Some(1), WRITER, 2_000).unwrap();
    let main = proj.branch();
    let (b, _) = proj.create_branch("Gone", main, f.ledger.head(), None, None).unwrap();
    proj.set_branch_open(b, false).unwrap();
    drop(proj);
    for value in [
        serde_json::json!(b.hex()),
        serde_json::json!("00112233445566778899aabbccddeeff"),
        serde_json::json!("not hex"),
        serde_json::json!(42),
        serde_json::json!({"branch": b.hex()}),
    ] {
        meta_put(&p, "active", Some(&serde_json::to_vec(&value).unwrap()));
        let o = ProjectV2::open(Store::open(&p, true).unwrap(), 1, WRITER).unwrap_or_else(|e| panic!("{value}: {e}"));
        assert_eq!(o.project.branch(), main, "{value}");
        assert!(o.report.damage.is_none(), "{value}: {:?}", o.report);
        assert_eq!(o.ledger.document(), f.ledger.document());
    }
}

/// A reader opened before the branch existed lists and reads it afterwards (the writer's rows are
/// committed at once); its own stale cache does not change what it shows.
#[test]
fn a_reader_opened_earlier_reads_a_branch_written_later() {
    let dir = TempDir::new("reader");
    let mut f = fixture();
    let p = dir.path("p.ecad");
    let store = Store::create(&p).unwrap();
    let (mut proj, _) = ProjectV2::create(store, PROJECT, &f.ledger, None, None, Some(1), WRITER, 16).unwrap();
    f.ledger.mark_persisted(f.ledger.head()).unwrap();
    let main = proj.branch();
    let reader = ProjectV2::open(Store::open(&p, false).unwrap(), 2, "reader").unwrap();
    let mut rng = Rng::new(8);
    let (b, _) = proj.create_branch("Late", main, f.ledger.head(), None, None).unwrap();
    let (mut bl, _) = proj.switch_to(&f.ledger, b, 3).unwrap();
    for _ in 0..20 {
        line(&mut bl, &mut rng);
    }
    proj.save(&mut bl).unwrap();
    let mut r = reader.project;
    assert_eq!(r.branches().unwrap().len(), 2);
    let (rl, _) = r.switch_to(&reader.ledger, b, 4).unwrap();
    assert_eq!(rl.document(), bl.document());
    assert!(r.create_branch("Mine", main, 0, None, None).is_err(), "the reader writes nothing");
}
