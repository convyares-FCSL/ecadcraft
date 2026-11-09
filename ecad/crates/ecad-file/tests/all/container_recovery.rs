//! Tester, brief 021 (container v2 on redb): recovery and integrity gaps the crash suite
//! (`container_crash`) does not reach.
//!
//! - Every cut point of one save that writes rows, a named version's snapshot, an automatic
//!   snapshot and the head (the suite cuts at a random point below 24 operations).
//! - A second crash while the file is being recovered.
//! - A save that fails with an I/O error the process survives, then saves again.
//! - After such a failure, the work in memory can still be saved as a new file.
//! - A second opener while the first is in the middle of a save: read-only, at a committed state.
//! - Undo and redo across the 1 024-entry stack cap after a crash and reopen, both stacks over it.
//! - Damage at rest (bytes changed inside the stored file) in a transaction row, a chunk, the
//!   row a snapshot stands on, and a snapshot's recorded stacks: detected, never loaded silently;
//!   EEP-0005 §5.8 says the project then opens read-only with a report.
//! - Converting a version 1 file cut short at any point.
#![cfg(not(target_arch = "wasm32"))]

use std::io;
use std::ops::Bound;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::common::*;
use ecad_file::v2::schema::{self, VersionKind};
use ecad_file::v2::{ProjectV2, Store};
use ecad_model::*;
use ecad_store::power::{CrashMode, PowerLoss, Rng};

// --- Helpers ----------------------------------------------------------------------------------

/// A fresh folder under the system temp folder, removed when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!("ecad-tester-021-{name}-{}", ecad_store::disk::unique()));
        std::fs::create_dir_all(&p).unwrap();
        TempDir(p)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// One property edit (one transaction).
fn prop(ledger: &mut Ledger, i: usize) {
    ledger
        .commit(
            Author::User,
            "Property",
            vec![Op::SetProperty { object: ObjectRef::Project, key: format!("k{i}"), before: None, after: Some("v".into()) }],
        )
        .unwrap();
}

/// `n` line edits on the fixture's page, with an undo now and then.
fn lines(ledger: &mut Ledger, rng: &mut Rng, n: u64) {
    let doc = ledger.document();
    let page = doc.page_order[0];
    let layer = *doc.layers.keys().next().unwrap();
    for i in 0..n {
        let id = ItemId(u128::from(rng.next_u64()) << 64 | u128::from(rng.next_u64()));
        let x = (rng.below(1000) as f64) / 10.0;
        let item =
            Item { page, style: style(layer), kind: ItemKind::Primitive(Primitive::Line { a: Pt::new(x, 0.0), b: Pt::new(x, 5.0 + i as f64) }) };
        ledger.commit(Author::User, "LINE", vec![Op::AddItem { id, item }]).unwrap();
        if rng.below(8) == 0 {
            ledger.undo(Author::User).unwrap();
        }
    }
}

fn name_version(ledger: &mut Ledger, name: &str) {
    let at = ledger.head();
    ledger.commit(Author::User, "Name", vec![Op::NameVersion { name: name.into(), at }]).unwrap();
}

/// The durable bytes of a simulated disk (after a clean close nothing is pending).
fn image(disk: &PowerLoss) -> Vec<u8> {
    disk.crash(&mut Rng::new(7), CrashMode::TornLast)
}

fn reopen(image: Vec<u8>) -> std::result::Result<ecad_file::v2::OpenedV2, String> {
    let store = Store::with_backend(PowerLoss::from_image(image)).map_err(|e| format!("store: {e}"))?;
    ProjectV2::open(store, 1, WRITER).map_err(|e| format!("open: {e}"))
}

fn find_all(hay: &[u8], needle: &[u8]) -> Vec<usize> {
    if needle.is_empty() || needle.len() > hay.len() {
        return Vec::new();
    }
    hay.windows(needle.len()).enumerate().filter(|(_, w)| *w == needle).map(|(i, _)| i).collect()
}

/// A project on a simulated disk: the fixture, then `before` saves of line edits.
struct Sim {
    disk: PowerLoss,
    proj: ProjectV2,
    ledger: Ledger,
}

fn sim(every: u64, before: u64, rng: &mut Rng) -> Sim {
    let disk = PowerLoss::default();
    let mut f = fixture();
    let store = Store::with_backend(disk.clone()).unwrap();
    let (mut proj, _) = ProjectV2::create(store, PROJECT, &f.ledger, None, None, None, WRITER, every).unwrap();
    f.ledger.mark_persisted(f.ledger.head()).unwrap();
    let mut ledger = f.ledger;
    for _ in 0..before {
        lines(&mut ledger, rng, 5);
        proj.save(&mut ledger).unwrap();
    }
    Sim { disk, proj, ledger }
}

// --- 1. Every cut point of a save that writes chunks, versions and the head --------------------

/// Cut the power after each number of operations of one save that writes transaction rows, a
/// named version's snapshot and an automatic snapshot (its chunks), then the head; land the
/// pending writes reordered or torn. Reopening must show the last acknowledged save, or the cut
/// save complete when it had landed; never a partial save (rows without their head, a head
/// without its rows, a version or snapshot without its chunks), and Verify must pass.
fn cut_sweep(seeds: u64, modes: &[CrashMode]) -> (u64, usize) {
    let mut ops_in_save = 0;
    let mut rounds = 0;
    let mut failures = Vec::new();
    for seed in 0..seeds {
        for &mode in modes {
            let mut cut = 0u64;
            loop {
                let mut rng = Rng::new(0x7E57_0000 + seed);
                let Sim { disk, mut proj, mut ledger } = sim(16, 2, &mut rng);
                let acked = (ledger.head(), ledger.document().clone());
                lines(&mut ledger, &mut rng, 12);
                name_version(&mut ledger, "V");
                lines(&mut ledger, &mut rng, 10);
                let attempted = (ledger.head(), ledger.document().clone());
                let (w0, s0) = disk.counts();
                disk.cut_after(cut);
                let saved = proj.save(&mut ledger);
                if saved.is_ok() {
                    let (w1, s1) = disk.counts();
                    ops_in_save = ops_in_save.max(w1 - w0 + s1 - s0);
                }
                drop(proj);
                let img = disk.crash(&mut Rng::new(seed ^ cut), mode);
                rounds += 1;
                let tag = format!("seed {seed}, {mode:?}, cut after {cut}");
                match reopen(img) {
                    Err(e) => failures.push(format!("{tag}: {e}")),
                    Ok(o) => {
                        let head = o.ledger.head();
                        let expected = if head == acked.0 && saved.is_err() {
                            Some(&acked.1)
                        } else if head == attempted.0 {
                            Some(&attempted.1)
                        } else {
                            None
                        };
                        if o.report.damage.is_some() {
                            failures.push(format!("{tag}: damage {:?}", o.report.damage));
                        } else if expected.is_none() {
                            failures.push(format!(
                                "{tag}: reopened at {head}; acknowledged {}, attempted {} ({})",
                                acked.0,
                                attempted.0,
                                if saved.is_ok() { "saved" } else { "cut" }
                            ));
                        } else if Some(o.ledger.document()) != expected {
                            failures.push(format!("{tag}: the document at {head} is not the saved one"));
                        } else {
                            let r = o.project.store().reader().unwrap();
                            let v = r.version(o.project.branch(), VersionKind::User, "V").unwrap();
                            if (head == attempted.0) != v.is_some() {
                                failures.push(format!("{tag}: version row {:?} at head {head}", v.map(|v| v.point)));
                            }
                            drop(r);
                            let ver = ecad_file::v2::verify::verify(o.project.store());
                            if !ver.ok {
                                failures.push(format!("{tag}: verify {:?}", ver.lines));
                            }
                        }
                    }
                }
                // Once the save completes before the cut, later cuts only cut the close.
                if saved.is_ok() || cut > 400 {
                    break;
                }
                cut += 1;
            }
        }
    }
    assert!(failures.is_empty(), "{} of {rounds} rounds failed: {failures:#?}", failures.len());
    (ops_in_save, rounds)
}

#[test]
fn every_cut_point_of_a_snapshot_save_reopens_at_an_acknowledged_state_quick() {
    let (ops, rounds) = cut_sweep(1, &[CrashMode::Reordered]);
    eprintln!("cut sweep (quick): {rounds} rounds; one such save is {ops} writes and syncs");
    assert!(ops > 0);
}

#[test]
#[ignore = "slow: every cut point of a save, for many seeds"]
fn every_cut_point_of_a_snapshot_save_reopens_at_an_acknowledged_state() {
    let (ops, rounds) = cut_sweep(12, &[CrashMode::Reordered, CrashMode::TornLast]);
    eprintln!("cut sweep: {rounds} rounds; one such save is up to {ops} writes and syncs");
}

// --- 2. A crash while recovering --------------------------------------------------------------

/// Cut a save, then cut the power again while reopening (redb's repair writes the file), then
/// reopen: still the last acknowledged save or the cut save complete, never less.
#[test]
fn a_second_power_cut_during_recovery_loses_nothing_acknowledged() {
    let mut failures = Vec::new();
    let mut rounds = 0;
    for seed in 0..6u64 {
        let mut rng = Rng::new(0xD0B1_0000 + seed);
        let Sim { disk, mut proj, mut ledger } = sim(16, 3, &mut rng);
        let acked = (ledger.head(), ledger.document().clone());
        lines(&mut ledger, &mut rng, 30);
        let attempted = (ledger.head(), ledger.document().clone());
        disk.cut_after(3 + rng.below(10));
        let saved = proj.save(&mut ledger);
        drop(proj);
        let first = disk.crash(&mut rng, CrashMode::Reordered);
        let mut second_cut = 0u64;
        loop {
            let recovering = PowerLoss::from_image(first.clone());
            recovering.cut_after(second_cut);
            let opened = Store::with_backend(recovering.clone())
                .and_then(|s| ProjectV2::open(s, 1, WRITER).map_err(|e| ecad_file::v2::StoreError::Engine(e.to_string())));
            let survived = opened.is_ok();
            drop(opened);
            let second = recovering.crash(&mut rng, CrashMode::Reordered);
            rounds += 1;
            let tag = format!("seed {seed}, second cut after {second_cut}");
            match reopen(second) {
                Err(e) => failures.push(format!("{tag}: {e}")),
                Ok(o) => {
                    let head = o.ledger.head();
                    let ok = (head == acked.0 && o.ledger.document() == &acked.1 && saved.is_err())
                        || (head == attempted.0 && o.ledger.document() == &attempted.1);
                    if !ok || o.report.damage.is_some() {
                        failures.push(format!(
                            "{tag}: reopened at {head} (acknowledged {}, attempted {}), damage {:?}",
                            acked.0, attempted.0, o.report.damage
                        ));
                    }
                }
            }
            if survived || second_cut > 200 {
                break;
            }
            second_cut += 1;
        }
    }
    assert!(failures.is_empty(), "{} of {rounds} rounds: {failures:#?}", failures.len());
}

// --- 3. A save that fails, the process survives, and saves again ------------------------------

/// A simulated disk whose operation number `fail_at` fails once (an I/O error the system
/// recovers from: a full disk, a network share that drops); the others succeed.
#[derive(Debug, Clone, Default)]
struct Flaky(Arc<Mutex<FlakyInner>>);

#[derive(Debug, Default)]
struct FlakyInner {
    bytes: Vec<u8>,
    ops: u64,
    fail_at: Option<u64>,
}

impl Flaky {
    fn arm(&self, after: u64) {
        let mut i = self.0.lock().unwrap();
        i.fail_at = Some(i.ops + after);
    }
    fn tick(&self) -> io::Result<()> {
        let mut i = self.0.lock().unwrap();
        let n = i.ops;
        i.ops += 1;
        if i.fail_at == Some(n) {
            i.fail_at = None;
            return Err(io::Error::other("injected I/O error"));
        }
        Ok(())
    }
    fn bytes(&self) -> Vec<u8> {
        self.0.lock().unwrap().bytes.clone()
    }
}

impl redb::StorageBackend for Flaky {
    fn len(&self) -> io::Result<u64> {
        Ok(self.0.lock().unwrap().bytes.len() as u64)
    }
    fn read(&self, offset: u64, out: &mut [u8]) -> io::Result<()> {
        let i = self.0.lock().unwrap();
        let s = offset as usize;
        let src = i.bytes.get(s..s + out.len()).ok_or_else(|| io::Error::from(io::ErrorKind::UnexpectedEof))?;
        out.copy_from_slice(src);
        Ok(())
    }
    fn set_len(&self, len: u64) -> io::Result<()> {
        self.tick()?;
        self.0.lock().unwrap().bytes.resize(len as usize, 0);
        Ok(())
    }
    fn sync_data(&self) -> io::Result<()> {
        self.tick()
    }
    fn write(&self, offset: u64, data: &[u8]) -> io::Result<()> {
        self.tick()?;
        let mut i = self.0.lock().unwrap();
        let end = offset as usize + data.len();
        if i.bytes.len() < end {
            i.bytes.resize(end, 0);
        }
        i.bytes[offset as usize..end].copy_from_slice(data);
        Ok(())
    }
    fn try_lock_range(&self, _: Bound<u64>, _: Bound<u64>) -> std::result::Result<bool, redb::BackendError> {
        Ok(true)
    }
    fn try_lock_shared_range(&self, _: Bound<u64>, _: Bound<u64>) -> std::result::Result<bool, redb::BackendError> {
        Ok(true)
    }
    fn lock_range(&self, _: Bound<u64>, _: Bound<u64>) -> std::result::Result<(), redb::BackendError> {
        Ok(())
    }
    fn lock_shared_range(&self, _: Bound<u64>, _: Bound<u64>) -> std::result::Result<(), redb::BackendError> {
        Ok(())
    }
    fn unlock_range(&self, _: Bound<u64>, _: Bound<u64>) -> std::result::Result<(), redb::BackendError> {
        Ok(())
    }
    fn query_lock_range(&self, _: Bound<u64>, _: Bound<u64>) -> std::result::Result<bool, redb::BackendError> {
        Ok(false)
    }
}

/// A save that fails part-way leaves the project unsaved; saving again (or, if that is refused
/// after the error, reopening) never loses an acknowledged save and never shows part of a save.
/// A save that reported an I/O error may have landed complete (its outcome is unknown, as after
/// a power cut); a later save then sees the moved head and refuses rather than append twice.
#[test]
#[ignore = "slow: sixty injected I/O errors"]
fn a_failed_save_then_a_retry_never_loses_or_mixes_saves() {
    let mut failures = Vec::new();
    let mut outcomes = std::collections::BTreeMap::<&str, u64>::new();
    for fail_at in 0..60u64 {
        let mut rng = Rng::new(0xF1A4 + fail_at);
        let disk = Flaky::default();
        let mut f = fixture();
        let store = Store::with_backend(disk.clone()).unwrap();
        let (mut proj, _) = ProjectV2::create(store, PROJECT, &f.ledger, None, None, None, WRITER, 16).unwrap();
        f.ledger.mark_persisted(f.ledger.head()).unwrap();
        let mut ledger = f.ledger;
        lines(&mut ledger, &mut rng, 5);
        proj.save(&mut ledger).unwrap();
        let mut acked = ledger.head();
        // Heads of saves that reported failure: after an I/O error the outcome is unknown, so
        // such a save may have landed complete (never partly).
        let mut maybe = vec![];
        lines(&mut ledger, &mut rng, 20);
        name_version(&mut ledger, "V");
        disk.arm(fail_at);
        let first = proj.save(&mut ledger);
        if first.is_ok() {
            acked = ledger.head();
            *outcomes.entry("the injected error missed the save").or_default() += 1;
        } else if ledger.persisted() != acked {
            failures.push(format!("fail_at {fail_at}: a failed save marked {} persisted", ledger.persisted()));
        } else {
            maybe.push(ledger.head());
        }
        lines(&mut ledger, &mut rng, 3);
        match proj.save(&mut ledger) {
            Ok(_) => {
                acked = ledger.head();
                *outcomes.entry("the retry saved").or_default() += 1;
            }
            Err(e) => {
                maybe.push(ledger.head());
                let kind = match e {
                    ecad_file::v2::SaveErrorV2::ExternalChange(_) => "the retry was refused: the head moved",
                    _ => "the retry was refused: engine error",
                };
                *outcomes.entry(kind).or_default() += 1;
            }
        }
        drop(proj);
        match reopen(disk.bytes()) {
            Err(e) => failures.push(format!("fail_at {fail_at}: reopen {e}")),
            Ok(o) => {
                let head = o.ledger.head();
                if head != acked {
                    *outcomes.entry("reopened at a save that had reported failure").or_default() += 1;
                }
                if o.report.damage.is_some() || !(head == acked || (head > acked && maybe.contains(&head))) {
                    failures.push(format!(
                        "fail_at {fail_at}: reopened at {head} (acknowledged {acked}, unknown {maybe:?}), damage {:?}",
                        o.report.damage
                    ));
                } else if o.ledger.document() != &ledger.state_at(head).unwrap() {
                    failures.push(format!("fail_at {fail_at}: the document at {head} differs"));
                } else {
                    let v = ecad_file::v2::verify::verify(o.project.store());
                    if !v.ok {
                        failures.push(format!("fail_at {fail_at}: verify {:?}", v.lines));
                    }
                }
            }
        }
    }
    eprintln!("failed save then retry: {outcomes:?}");
    assert!(failures.is_empty(), "{failures:#?}");
}

// --- 4. A second opener while the first is saving ---------------------------------------------

/// A second session opens the file while the first holds a write transaction open (a save in
/// progress): it opens read-only, at the last committed head, with no damage, and refuses to
/// save; once the first commits, a new opener sees the new head.
#[test]
fn a_second_opener_mid_save_reads_the_last_commit_read_only() {
    use ecad_file::v2::disk::DiskProjectV2;
    let dir = TempDir::new("second-opener");
    let p = dir.path("p.ecad");
    let mut f = fixture();
    let (mut a, _) = DiskProjectV2::create(&p, PROJECT, &mut f.ledger, None, None, "first").unwrap();
    let mut ledger = f.ledger;
    let mut rng = Rng::new(42);
    lines(&mut ledger, &mut rng, 10);
    a.save(&mut ledger).unwrap();
    let committed = (ledger.head(), ledger.document().clone());
    // A save in progress: rows appended, uncommitted.
    let store = a.project().store().clone();
    let mut w = store.writer().unwrap();
    w.put_asset("application/octet-stream", &vec![7u8; 300_000]).unwrap();
    let b = DiskProjectV2::open(&p, 9, "second").expect("a second opener mid-save must open");
    assert!(b.project.read_only().is_some(), "the second opener must be read-only");
    assert!(b.project.lock_file().is_none());
    assert!(b.report.damage.is_none(), "{:?}", b.report);
    assert_eq!(b.ledger.head(), committed.0);
    assert_eq!(b.ledger.document(), &committed.1);
    let (mut bp, mut bl) = (b.project, b.ledger);
    prop(&mut bl, 1);
    assert!(bp.save(&mut bl).is_err(), "a read-only opener never writes");
    drop(w); // the in-progress save is abandoned
    drop((bp, bl));
    // The first session saves again; a new second opener sees it.
    lines(&mut ledger, &mut rng, 4);
    a.save(&mut ledger).unwrap();
    let c = DiskProjectV2::open(&p, 10, "third").unwrap();
    assert!(c.report.damage.is_none());
    assert_eq!(c.ledger.head(), ledger.head());
    assert_eq!(c.ledger.document(), ledger.document());
}

/// The same, with the first session saving in a loop on another thread while the second opens
/// again and again: every second open is read-only, undamaged, and at a head the first has
/// committed, with that head's document.
#[test]
#[ignore = "slow: forty opens while another thread saves 150 times"]
fn second_openers_during_a_stream_of_saves_always_see_a_committed_state() {
    use ecad_file::v2::disk::DiskProjectV2;
    use std::sync::mpsc;
    let dir = TempDir::new("second-stream");
    let p = dir.path("p.ecad");
    let (tx, rx) = mpsc::channel::<(u64, Document)>();
    let (ready_tx, ready_rx) = mpsc::channel::<()>();
    let (stop, stop_rx) = mpsc::channel::<()>();
    let writer = {
        let p = p.clone();
        std::thread::spawn(move || {
            let mut f = fixture();
            let (mut a, _) = DiskProjectV2::create(&p, PROJECT, &mut f.ledger, None, None, "first").unwrap();
            let mut ledger = f.ledger;
            tx.send((ledger.head(), ledger.document().clone())).unwrap();
            ready_tx.send(()).unwrap();
            let mut rng = Rng::new(9);
            let mut saves = 0;
            while stop_rx.try_recv().is_err() && saves < 150 {
                let n = 1 + rng.below(20);
                lines(&mut ledger, &mut rng, n);
                // Send before saving: the second opener may see it as soon as it commits.
                tx.send((ledger.head(), ledger.document().clone())).unwrap();
                a.save(&mut ledger).unwrap();
                saves += 1;
            }
            // Keep the file open (and locked) until every second open is done.
            let _ = stop_rx.recv();
            drop(a);
            saves
        })
    };
    ready_rx.recv().unwrap();
    let mut known = std::collections::BTreeMap::new();
    let mut failures = Vec::new();
    let mut heads = std::collections::BTreeSet::new();
    for round in 0..40 {
        let o = DiskProjectV2::open(&p, 100 + round, "second");
        while let Ok((h, d)) = rx.try_recv() {
            known.insert(h, d);
        }
        match o {
            Err(e) => failures.push(format!("open {round}: {e}")),
            Ok(o) => {
                let head = o.ledger.head();
                heads.insert(head);
                if o.project.read_only().is_none() {
                    failures.push(format!("open {round}: not read-only"));
                }
                if o.report.damage.is_some() {
                    failures.push(format!("open {round}: damage {:?}", o.report.damage));
                }
                match known.get(&head) {
                    Some(d) if d == o.ledger.document() => {}
                    Some(_) => failures.push(format!("open {round}: the document at {head} differs")),
                    None => failures.push(format!("open {round}: head {head} was never saved")),
                }
            }
        }
    }
    stop.send(()).unwrap();
    let saves = writer.join().unwrap();
    eprintln!("second openers: 40 opens during {saves} saves saw {} distinct heads", heads.len());
    assert!(failures.is_empty(), "{failures:#?}");
}

// --- 5. Undo and redo across the 1 024 cap after a crash --------------------------------------

/// Both stacks deeper than the 1 024 a snapshot records: 1 100 edits, 1 060 undone (the redo
/// stack is 1 060 deep, the undo stack 40), saved with an automatic snapshot, then a few redos
/// whose save is cut by a power loss. Reopened, redo all the way and undo all the way must match
/// the replayed project step for step.
#[test]
fn undo_and_redo_across_the_stack_cap_after_a_power_cut() {
    let disk = PowerLoss::default();
    let mut f = fixture();
    for i in 0..1_100 {
        prop(&mut f.ledger, i);
    }
    let store = Store::with_backend(disk.clone()).unwrap();
    let (mut proj, _) = ProjectV2::create(store, PROJECT, &f.ledger, None, None, None, WRITER, 500).unwrap();
    f.ledger.mark_persisted(f.ledger.head()).unwrap();
    let mut ledger = f.ledger;
    for _ in 0..1_060 {
        ledger.undo(Author::User).unwrap();
    }
    proj.save(&mut ledger).unwrap();
    {
        let r = proj.store().reader().unwrap();
        let s = r.auto_at_or_before(proj.branch(), ledger.head()).unwrap().unwrap();
        assert_eq!(s.point.seq, ledger.head(), "an automatic snapshot at the head");
        assert_eq!(s.redo.len() as u64, schema::STACK_CAP);
        assert_eq!(s.redo_hidden, 1_060 - schema::STACK_CAP);
    }
    let acked = ledger.clone();
    for _ in 0..5 {
        ledger.redo(Author::User).unwrap();
    }
    disk.cut_after(4);
    let cut = proj.save(&mut ledger);
    drop(proj);
    let o = reopen(disk.crash(&mut Rng::new(3), CrashMode::Reordered)).unwrap();
    assert!(o.report.damage.is_none(), "{:?}", o.report);
    let mut reference = if cut.is_err() && o.ledger.head() == acked.head() { acked } else { ledger };
    assert_eq!(o.ledger.head(), reference.head(), "save result {cut:?}");
    let mut opened = o.ledger;
    assert_eq!(opened.stack_depths(), (reference.stacks().0.len() as u64, reference.stacks().1.len() as u64), "stack depths after reopening");
    let mut n = 0;
    while reference.redo(Author::User).is_ok() {
        opened.redo(Author::User).unwrap_or_else(|e| panic!("redo {n} failed: {e}"));
        n += 1;
        if n % 50 == 0 {
            assert_eq!(opened.document(), reference.document(), "after {n} redos");
        }
    }
    assert!(opened.redo(Author::User).is_err(), "redo past the end");
    assert_eq!(opened.document(), reference.document(), "after all {n} redos");
    let mut m = 0;
    while reference.undo(Author::User).is_ok() {
        opened.undo(Author::User).unwrap_or_else(|e| panic!("undo {m} failed: {e}"));
        m += 1;
        if m % 50 == 0 {
            assert_eq!(opened.document(), reference.document(), "after {m} undos");
        }
    }
    assert!(opened.undo(Author::User).is_err(), "undo past the start");
    assert_eq!(opened.document(), reference.document(), "after all {m} undos");
    assert!(n >= 1_055 && m >= 1_100, "{n} redos, {m} undos");
}

// --- 6. Damage at rest ------------------------------------------------------------------------

/// A clean image of a project with a snapshot and a tail after it, and what it holds.
struct Rest {
    image: Vec<u8>,
    ledger: Ledger,
    snap: u64,
    branch: schema::BranchId,
}

fn at_rest() -> Rest {
    let mut rng = Rng::new(0xDA4A);
    let Sim { disk, mut proj, mut ledger } = sim(16, 0, &mut rng);
    lines(&mut ledger, &mut rng, 20);
    proj.save(&mut ledger).unwrap(); // an automatic snapshot here
    let snap = proj.snapshot_seq();
    lines(&mut ledger, &mut rng, 6);
    proj.save(&mut ledger).unwrap(); // the tail
    let branch = proj.branch();
    assert!(snap > 0 && ledger.head() > snap + 3);
    drop(proj);
    Rest { image: image(&disk), ledger, snap, branch }
}

/// What reopening a damaged image did: `Ok(None)` detected (damage reported or open refused),
/// `Ok(Some(..))` loaded as if sound and correct, `Err` loaded as if sound but wrong.
fn judge(img: Vec<u8>, truth: &Ledger) -> std::result::Result<&'static str, String> {
    match reopen(img) {
        Err(_) => Ok("open refused"),
        Ok(o) if o.report.damage.is_some() => {
            if o.project.read_only().is_none() {
                return Err("damage reported but the project is writable".into());
            }
            let at = o.ledger.head();
            if o.ledger.document() != &truth.state_at(at).unwrap() {
                return Err(format!("damage reported, but the document at {at} is not the true one"));
            }
            Ok("read-only with a report")
        }
        Ok(o) => {
            if o.ledger.head() == truth.head() && o.ledger.document() == truth.document() {
                Ok("unchanged content (a redundant byte)")
            } else {
                Err(format!("loaded silently at {} with a different document", o.ledger.head()))
            }
        }
    }
}

/// Bytes changed inside the stored value of every transaction row after the snapshot, and of
/// every chunk of the snapshot, directly in the file image: each is detected, or changes
/// nothing; never loaded as a different, unreported state.
#[test]
#[ignore = "slow: hundreds of damaged images reopened"]
fn bytes_changed_in_a_stored_row_or_chunk_are_never_loaded_silently() {
    let rest = at_rest();
    let (rows, chunks) = {
        let o = reopen(rest.image.clone()).unwrap();
        assert!(o.report.damage.is_none());
        let r = o.project.store().reader().unwrap();
        let rows: Vec<Vec<u8>> = (rest.snap + 1..=rest.ledger.head())
            .map(|s| {
                let (flag, chain, body) = r.tx(rest.branch, s).unwrap().unwrap();
                schema::tx_value(flag, &chain, &body)
            })
            .collect();
        let v = r.auto_at_or_before(rest.branch, rest.snap).unwrap().unwrap();
        let chunks: Vec<Vec<u8>> = v
            .chunks
            .values()
            .map(|h| {
                let hash = ecad_file::chain::parse_hex(h).unwrap();
                let (flag, body) = r.chunk(&hash).unwrap().unwrap();
                let mut val = vec![flag];
                val.extend(body);
                val
            })
            .collect();
        (rows, chunks)
    };
    let mut outcomes = std::collections::BTreeMap::<String, u64>::new();
    let mut failures = Vec::new();
    let mut located = 0;
    for (what, values) in [("row", &rows), ("chunk", &chunks)] {
        for (k, val) in values.iter().enumerate() {
            let at = find_all(&rest.image, val);
            let [start] = at.as_slice() else { continue };
            located += 1;
            // The flag, the chain value, and spread through the body.
            let mut offsets = vec![0, 1, 17, 32, val.len() - 1];
            offsets.extend((1..8).map(|i| i * val.len() / 8));
            for off in offsets {
                for bit in [0x01u8, 0x80] {
                    let mut img = rest.image.clone();
                    img[start + off] ^= bit;
                    match judge(img, &rest.ledger) {
                        Ok(o) => *outcomes.entry(format!("{what}: {o}")).or_default() += 1,
                        Err(e) => failures.push(format!("{what} {k}, byte {off} ^ {bit:#x}: {e}")),
                    }
                }
            }
        }
    }
    eprintln!("damage at rest: {located} stored values located; {outcomes:#?}");
    assert!(located >= rows.len(), "the rows were not found in the image");
    assert!(failures.is_empty(), "{failures:#?}");
}

/// EEP-0005 §5.8: damage found while loading opens the project read-only, at the last state that
/// verifies, with a report. The row a snapshot stands on is the top of its undo stack, read on
/// open; when its bytes are damaged the transactions before it and the snapshot itself are
/// sound, yet the project must still open (read-only), not be refused.
#[test]
fn damage_in_the_row_under_the_snapshot_opens_read_only_not_refused() {
    let rest = at_rest();
    let o = reopen(rest.image.clone()).unwrap();
    let (flag, chain, body) = o.project.store().reader().unwrap().tx(rest.branch, rest.snap).unwrap().unwrap();
    // The top of the undo stack recorded with the snapshot is at or before the snapshot.
    assert!(o.ledger.stacks().0.iter().any(|(s, _)| *s <= rest.snap));
    drop(o);
    let val = schema::tx_value(flag, &chain, &body);
    let at = find_all(&rest.image, &val);
    assert!(!at.is_empty(), "the row's stored value is in the image");
    // redb copies pages on write: an older copy may remain in a free page; damage every copy.
    let mut img = rest.image.clone();
    for a in &at {
        img[a + 33 + body.len() / 2] ^= 0x20;
    }
    let o = reopen(img).unwrap_or_else(|e| panic!("a damaged transaction {} (under the snapshot) refused the whole project: {e}", rest.snap));
    let d = o.report.damage.clone().expect("the damage is reported");
    assert!(o.project.read_only().is_some(), "{d:?}");
}

/// The same for a snapshot's recorded undo stack (a cache, like its chunks): a damaged chunk
/// opens read-only from the root, so a damaged stack record must too, not refuse the project.
#[test]
fn a_damaged_stack_record_in_a_snapshot_opens_read_only_not_refused() {
    let rest = at_rest();
    let img_disk = PowerLoss::from_image(rest.image.clone());
    {
        let store = Store::with_backend(img_disk.clone()).unwrap();
        let r = store.reader().unwrap();
        let mut v = r.auto_at_or_before(rest.branch, rest.snap).unwrap().unwrap();
        drop(r);
        // The top entry names another transaction ID (a changed digit).
        let top = v.undo.last_mut().expect("an undo stack");
        let mut id: Vec<u8> = top.1.clone().into_bytes();
        id[5] = if id[5] == b'0' { b'1' } else { b'0' };
        top.1 = String::from_utf8(id).unwrap();
        let mut w = store.writer().unwrap();
        w.put_version(rest.branch, &v).unwrap();
        w.commit().unwrap();
    }
    let img = image(&img_disk);
    let o = reopen(img).unwrap_or_else(|e| panic!("a damaged stack record refused the whole project: {e}"));
    let d = o.report.damage.clone().expect("the damage is reported");
    assert!(o.project.read_only().is_some(), "{d:?}");
    assert_eq!(o.ledger.document(), &rest.ledger.state_at(o.ledger.head()).unwrap());
}

// --- 7. Converting a version 1 file cut short -------------------------------------------------

/// A version 1 file cut at every line end and at points inside lines converts its sound prefix
/// (or is refused); the result holds exactly the complete transactions, with the version 1 chain
/// value of that prefix, the same document, and the source is never changed. A refused
/// conversion leaves no output.
#[test]
fn a_version_1_file_cut_short_converts_its_complete_prefix_or_is_refused_quick() {
    v1_cut(5);
}

#[test]
#[ignore = "slow: a conversion for every cut"]
fn a_version_1_file_cut_short_converts_its_complete_prefix_or_is_refused() {
    v1_cut(1);
}

/// Every `stride`-th line's cut points.
fn v1_cut(stride: usize) {
    use ecad_file::ProjectFile;
    use ecad_store::{Medium, MemoryMedium};
    let mut f = fixture();
    let mut rng = Rng::new(0x7121);
    lines(&mut f.ledger, &mut rng, 12);
    name_version(&mut f.ledger, "late");
    let file = ProjectFile::create(MemoryMedium::default(), PROJECT, &mut f.ledger, Some(1), WRITER).unwrap();
    let bytes = file.medium().read_all().unwrap();
    let ends: Vec<usize> = bytes.iter().enumerate().filter(|(_, b)| **b == b'\n').map(|(i, _)| i + 1).collect();
    let mut cuts: Vec<usize> = Vec::new();
    for w in ends.windows(2).step_by(stride) {
        let (a, b) = (w[0], w[1]);
        cuts.extend([a, a + 1, (a + b) / 2, b - 2, b - 1]);
    }
    cuts.extend([1, ends[0] / 2, ends[0] - 1]);
    cuts.sort_unstable();
    cuts.dedup();
    let dir = TempDir::new("v1-cut");
    let mut outcomes = std::collections::BTreeMap::<&str, u64>::new();
    let mut failures = Vec::new();
    for (k, cut) in cuts.iter().enumerate() {
        let src = dir.path(&format!("cut{k}.ecad"));
        let out = dir.path(&format!("cut{k}.v2.ecad"));
        let cut_bytes = bytes[..*cut].to_vec();
        std::fs::write(&src, &cut_bytes).unwrap();
        let v1 = ecad_file::open_project(&cut_bytes, 0);
        let result = ecad_file::v2::convert::convert_file(&src, Some(&out), WRITER);
        if std::fs::read(&src).unwrap() != cut_bytes {
            failures.push(format!("cut {cut}: the source changed"));
        }
        match result {
            Err(e) => {
                *outcomes.entry("refused").or_default() += 1;
                if out.exists() {
                    failures.push(format!("cut {cut}: refused ({e}) but an output was left"));
                }
                if let Ok(v1) = &v1
                    && !matches!(v1.report.outcome, ecad_file::Outcome::Corrupt(_))
                    && v1.ledger.head() > 0
                {
                    failures.push(format!("cut {cut}: version 1 reads {} transactions, yet refused: {e}", v1.ledger.head()));
                }
            }
            Ok(rep) => {
                *outcomes.entry(if rep.omitted.is_some() { "converted the prefix" } else { "converted whole" }).or_default() += 1;
                let complete = ends.iter().filter(|e| **e <= *cut).count().saturating_sub(1) as u64;
                let v1 = v1.unwrap();
                let o = ProjectV2::open(Store::open(&out, false).unwrap(), 1, WRITER).unwrap();
                if rep.transactions != complete && rep.transactions != v1.ledger.head() {
                    failures.push(format!("cut {cut}: {} transactions converted, {complete} complete", rep.transactions));
                }
                if o.ledger.head() != v1.ledger.head() || o.ledger.document() != v1.ledger.document() {
                    failures.push(format!("cut {cut}: the converted file differs from the version 1 prefix"));
                }
                if rep.source_hash != ecad_file::chain::hex(&v1.chain_head) || o.project.link_v1().map(|l| l.transactions) != Some(v1.ledger.head()) {
                    failures.push(format!("cut {cut}: link_v1 does not name the prefix"));
                }
                if o.report.damage.is_some() {
                    failures.push(format!("cut {cut}: damage {:?}", o.report.damage));
                }
            }
        }
    }
    eprintln!("version 1 cut short: {} cuts, {outcomes:?}", cuts.len());
    assert!(failures.is_empty(), "{failures:#?}");
}

/// After a save fails and the engine refuses further saves on that handle, the work is not lost:
/// Save As (copy the file's rows into a new file, open it, save the pending transactions, as
/// `DiskProjectV2::save_as` does) keeps everything in memory.
#[test]
fn after_a_failed_save_the_work_can_still_be_saved_as_a_new_file() {
    let mut refused = 0;
    for fail_at in 0..40u64 {
        let mut rng = Rng::new(0x5A5A + fail_at);
        let disk = Flaky::default();
        let mut f = fixture();
        let store = Store::with_backend(disk.clone()).unwrap();
        let (mut proj, _) = ProjectV2::create(store, PROJECT, &f.ledger, None, None, None, WRITER, 16).unwrap();
        f.ledger.mark_persisted(f.ledger.head()).unwrap();
        let mut ledger = f.ledger;
        lines(&mut ledger, &mut rng, 25);
        disk.arm(fail_at);
        if proj.save(&mut ledger).is_ok() {
            continue;
        }
        lines(&mut ledger, &mut rng, 2);
        if proj.save(&mut ledger).is_ok() {
            continue;
        }
        refused += 1;
        // Save As, as the disk binding does it (not a rescue: the project is not read-only).
        let target = PowerLoss::default();
        {
            let new = Store::with_backend(target.clone()).unwrap();
            let r = proj.store().reader().unwrap_or_else(|e| panic!("fail_at {fail_at}: read after the error: {e}"));
            let mut w = new.writer().unwrap();
            w.copy_all(&r, None).unwrap_or_else(|e| panic!("fail_at {fail_at}: copy after the error: {e}"));
            w.commit().unwrap();
            drop(r);
            let mut o = ProjectV2::open(new, 2, WRITER).unwrap();
            assert!(o.report.damage.is_none(), "fail_at {fail_at}: {:?}", o.report);
            o.project.save(&mut ledger).unwrap_or_else(|e| panic!("fail_at {fail_at}: saving the copy: {e}"));
        }
        drop(proj);
        let o = reopen(image(&target)).unwrap();
        assert_eq!(o.ledger.head(), ledger.head(), "fail_at {fail_at}");
        assert_eq!(o.ledger.document(), ledger.document(), "fail_at {fail_at}");
    }
    eprintln!("save as after a failed save: {refused} cases where the handle refused further saves");
    assert!(refused > 0);
}
